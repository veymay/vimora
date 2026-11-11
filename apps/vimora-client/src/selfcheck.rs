//! "Am I the installed version?" — for the agent and the service after an update.
//!
//! Comparing file times is not enough: the installer moves running files aside, and the running
//! process may then be looking at the moved copy. Instead the installed program is asked for its
//! version (`vimora-client --version`); when it differs from ours, we restart on it.

use std::path::PathBuf;
use std::time::Duration;

/// The installed program: next to us under its usual name (Windows), or our own path without a
/// " (deleted)" mark (Linux, after dpkg replaced the file).
pub fn installed_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(windows) {
        return exe.parent().map(|d| d.join("vimora-client.exe"));
    }
    let s = exe.to_string_lossy().into_owned();
    Some(PathBuf::from(s.trim_end_matches(" (deleted)")))
}

/// Version the installed program reports ("0.3.21"), if it can be asked.
pub fn installed_version() -> Option<String> {
    let exe = installed_exe()?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--version");
    let out = crate::sh::quiet(&mut cmd).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v = text.split_whitespace().nth(1)?.trim().to_string();
    (!v.is_empty() && v.chars().next()?.is_ascii_digit()).then_some(v)
}

/// A different version is installed than the one running.
pub fn replaced() -> Option<String> {
    let v = installed_version()?;
    (v != vimora_update::current()).then_some(v)
}

/// Checks every minute; `restart` is called (once) when the installed version changed.
pub fn watch(name: &'static str, restart: impl Fn(PathBuf) + Send + 'static) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        if let Some(v) = replaced() {
            // Give the installer a moment to finish (it runs "setup" last).
            std::thread::sleep(Duration::from_secs(15));
            tracing::info!(
                "{name}: version {v} is installed (running {}) — restarting",
                vimora_update::current()
            );
            if let Some(exe) = installed_exe() {
                restart(exe);
            }
            return;
        }
    });
}
