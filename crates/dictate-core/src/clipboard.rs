//! Wayland clipboard writes via `wl-copy`.
//!
//! On Wayland the clipboard contents are served by the *source* application for
//! as long as it stays alive, so a short-lived process that sets the clipboard
//! with a toolkit API and then exits loses the selection. `wl-copy` forks a
//! tiny daemon that keeps serving the offer, which lets the application deliver
//! text and quit safely. Target is KDE Plasma Wayland.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

/// Put `text` on the clipboard. Returns once `wl-copy` has taken ownership
/// (it backgrounds itself to keep serving the selection afterwards).
pub fn set_text(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Не удалось запустить wl-copy; установи wl-clipboard")?;
    child
        .stdin
        .take()
        .context("wl-copy не принял ввод")?
        .write_all(text.as_bytes())?;
    // wl-copy reads stdin, then daemonizes; the foreground process returns
    // promptly once it owns the selection.
    let status = child.wait()?;
    if !status.success() {
        bail!("wl-copy завершился с ошибкой");
    }
    Ok(())
}
