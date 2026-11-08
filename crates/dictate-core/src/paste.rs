//! A persistent portal connection, owned by one worker, independent of the UI toolkit.
use anyhow::{Context, Result};
use eitype::{EiType, EiTypeConfig};
use std::sync::mpsc::{self, Sender};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub enum PasteEvent {
    Connected,
    ConnectionFailed(String),
    Done,
    Error(String),
}
enum Command {
    Connect,
    Paste(bool),
    Shutdown,
}
#[derive(Clone)]
pub struct PasteService(Sender<Command>);
impl PasteService {
    pub fn start(emit: impl Fn(PasteEvent) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut connection: Option<EiType> = None;
            for command in rx {
                match command {
                    Command::Shutdown => break,
                    Command::Connect => {
                        if connection.is_some() {
                            emit(PasteEvent::Connected);
                            continue;
                        }
                        match connect_saved() {
                            Ok(c) => {
                                connection = Some(c);
                                emit(PasteEvent::Connected);
                            }
                            Err(e) => emit(PasteEvent::ConnectionFailed(format!("{e}"))),
                        }
                    }
                    Command::Paste(shift) => {
                        let result = (|| -> anyhow::Result<()> {
                            if connection.is_none() && Self::has_saved_permission() {
                                connection = Some(connect_saved()?);
                                emit(PasteEvent::Connected);
                            }
                            let c = connection.as_mut().ok_or_else(|| {
                                anyhow::anyhow!("Connect automatic paste in settings first")
                            })?;
                            c.hold_modifier("ctrl")?;
                            let result = (|| {
                                if shift {
                                    c.hold_modifier("shift")?;
                                }
                                c.press_key("v")
                            })();
                            let release = c.release_modifiers();
                            result?;
                            release?;
                            Ok(())
                        })();
                        match result {
                            Ok(()) => emit(PasteEvent::Done),
                            Err(e) => {
                                connection = None;
                                emit(PasteEvent::Error(format!(
                                    "Text copied, but paste failed: {e}"
                                )));
                            }
                        }
                    }
                }
            }
        });
        Self(tx)
    }
    pub fn has_saved_permission() -> bool {
        token_path()
            .and_then(|p| read_token(&p))
            .ok()
            .flatten()
            .is_some()
    }
    pub fn connect(&self) {
        let _ = self.0.send(Command::Connect);
    }
    pub fn paste(&self, shift: bool) {
        let _ = self.0.send(Command::Paste(shift));
    }
    pub fn shutdown(&self) {
        let _ = self.0.send(Command::Shutdown);
    }
}

// Separate from API settings: rotating portal credentials must not be overwritten
// when an already-open settings window saves its older configuration snapshot.
fn token_path() -> Result<PathBuf> {
    Ok(crate::Config::path()?.with_file_name("paste-restore-token"))
}
fn read_token(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => Ok(Some(s)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("Failed to read automatic paste permission"),
    }
}
fn persist_token(path: &Path, token: Option<&str>) -> Result<()> {
    if let Some(token) = token {
        let dir = path.parent().context("Permission directory not found")?;
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(token.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path)
            .context("Failed to save automatic paste permission")?;
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn restore<C>(
    path: &Path,
    connect: impl FnOnce(Option<&str>) -> Result<(C, Option<String>)>,
) -> Result<C> {
    let token = read_token(path)?;
    let (connection, next) = connect(token.as_deref())?;
    // Tokens are single-use. Always replace with the newly issued token;
    // never retain an old one when the portal declines persistence.
    persist_token(path, next.as_deref())?;
    Ok(connection)
}
fn connect_saved() -> Result<EiType> {
    // Serialize reading and rotating the shared portal permission so two
    // processes cannot present the same single-use token concurrently.
    use nix::fcntl::{Flock, FlockArg};
    use std::os::unix::fs::OpenOptionsExt;
    let path = token_path()?;
    let dir = path.parent().context("Permission directory not found")?;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(dir.join("paste-restore.lock"))?;
    let _lock = Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, e)| e)?;
    restore(&path, |token| {
        EiType::connect_portal_with_token(
            EiTypeConfig {
                delay_ms: 10,
                ..Default::default()
            },
            token,
        )
        .map_err(|e| {
            eprintln!("Automatic paste portal: {e}");
            anyhow::anyhow!(
                "Could not connect through the desktop portal. Copy text and paste manually."
            )
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_survives_restart_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permission");
        restore(&path, |token| {
            assert_eq!(token, None);
            Ok(((), Some("first".into())))
        })
        .unwrap();
        restore(&path, |token| {
            assert_eq!(token, Some("first"));
            Ok(((), Some("second".into())))
        })
        .unwrap();
        assert_eq!(read_token(&path).unwrap().as_deref(), Some("second"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        restore(&path, |token| {
            assert_eq!(token, Some("second"));
            Ok(((), None))
        })
        .unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn transient_failure_does_not_erase_permission() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permission");
        persist_token(&path, Some("saved")).unwrap();
        assert!(restore::<()>(&path, |_| anyhow::bail!("offline")).is_err());
        assert_eq!(read_token(&path).unwrap().as_deref(), Some("saved"));
    }
}
