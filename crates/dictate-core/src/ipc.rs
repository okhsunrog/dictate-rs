//! Single-instance command dispatch over a private Unix socket.
//!
//! Exactly one resident instance is elected with an advisory lock held for the
//! whole process lifetime — this is race-free even when two launches start at
//! the same moment (`flock` hands the lock to exactly one of them). The winner
//! binds a Unix socket and runs the GUI; everyone else forwards their command
//! over that socket and exits. A separate recorder lock prevents concurrent
//! microphone capture within the same runtime directory.

use nix::fcntl::{Flock, FlockArg};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// Keeps the resident election lock alive for the process lifetime (the kernel
/// also releases it on exit, so a missed `Drop` is harmless).
static RESIDENT_LOCK: Mutex<Option<Flock<File>>> = Mutex::new(None);

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}
fn socket_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("{name}.sock"))
}
fn lock_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("{name}.lock"))
}

/// Outcome of [`start`].
pub enum Startup {
    /// We are the resident instance: run the GUI with this listener.
    Resident(UnixListener),
    /// The command was delivered to the running instance; the caller exits.
    Forwarded,
    /// Nothing to do (no instance to command, or forwarding failed); exit.
    Nothing,
}

/// Elect the resident and route `command`. `starts_instance` says whether this
/// command should launch a new resident when none is running (true for
/// show/toggle, false for quit/cancel which are meaningless with nothing up).
pub fn start(name: &str, command: &str, starts_instance: bool) -> Startup {
    match take_lock(name) {
        // We won the election, so no resident is running.
        Ok(lock) => {
            if !starts_instance {
                return Startup::Nothing; // e.g. `quit` with nothing to quit.
            }
            // Safe to clear a stale socket: we hold the exclusive lock.
            let _ = std::fs::remove_file(socket_path(name));
            match UnixListener::bind(socket_path(name)) {
                Ok(listener) => {
                    *RESIDENT_LOCK.lock().unwrap() = Some(lock);
                    Startup::Resident(listener)
                }
                Err(e) => {
                    eprintln!("single-instance: failed to bind socket {name}: {e}");
                    Startup::Nothing
                }
            }
        }
        // The lock is held by the resident. Forward, retrying briefly in case
        // it has taken the lock but not yet bound the socket.
        Err(()) => {
            for _ in 0..100 {
                if forward(name, command) {
                    return Startup::Forwarded;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Startup::Nothing
        }
    }
}

fn take_lock(name: &str) -> Result<Flock<File>, ()> {
    let file = File::create(lock_path(name)).map_err(|_| ())?;
    Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|_| ())
}

fn forward(name: &str, command: &str) -> bool {
    match UnixStream::connect(socket_path(name)) {
        Ok(mut stream) => writeln!(stream, "{command}").is_ok(),
        Err(_) => false,
    }
}

/// Spawn the accept loop, invoking `on_command` for each forwarded command.
pub fn spawn_accept(listener: UnixListener, on_command: impl Fn(String) + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                let cmd = line.trim().to_string();
                if !cmd.is_empty() {
                    on_command(cmd);
                }
            }
        }
    });
}

/// Remove the socket file on shutdown so a stale path doesn't linger. The lock
/// file is intentionally left in place (its inode must stay stable).
pub fn cleanup(name: &str) {
    let _ = std::fs::remove_file(socket_path(name));
}
