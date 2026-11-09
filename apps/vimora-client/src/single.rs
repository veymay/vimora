//! One agent per user session: starting a second copy replaces the first.
//!
//! Without this, launching the agent from the folder a few times left several agents running,
//! and the console showed one computer as two or three. The new copy finds the previous one
//! through a small PID file, ends it (and its overlay / chat children) and takes over.

use crate::sh;
use std::path::PathBuf;

fn pid_file() -> PathBuf {
    #[cfg(windows)]
    let dir = std::env::temp_dir();
    #[cfg(not(windows))]
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| {
            // Per user, so two users on one PC don't end each other's agent.
            let uid = std::env::var("UID")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default();
            std::env::temp_dir().join(format!("vimora-{uid}"))
        });
    let _ = std::fs::create_dir_all(&dir);
    dir.join("vimora-agent.pid")
}

/// Is `pid` a running Vimora agent (and not something that reused the number)?
fn is_agent(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        sh::run("tasklist", &["/FI", &filter, "/FO", "CSV", "/NH"])
            .map(|(_, out)| out.to_lowercase().contains("vimora-client"))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|c| c.trim().starts_with("vimora-client"))
            .unwrap_or(false)
    }
}

fn kill(pid: u32) {
    #[cfg(windows)]
    {
        // /T takes the overlay / chat children down with it.
        let _ = sh::run("taskkill", &["/F", "/T", "/PID", &pid.to_string()]);
    }
    #[cfg(not(windows))]
    {
        let _ = sh::run("kill", &["-TERM", &pid.to_string()]);
        std::thread::sleep(std::time::Duration::from_millis(600));
        if is_agent(pid) {
            let _ = sh::run("kill", &["-KILL", &pid.to_string()]);
        }
    }
}

/// Ends the previous agent of this user (if any) and records this process as the current one.
pub fn take_over() {
    let path = pid_file();
    let me = std::process::id();
    if let Some(old) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|p| *p != me)
    {
        if is_agent(old) {
            tracing::info!("replacing the running agent (pid {old})");
            kill(old);
            // Let the old copy release its sockets before this one binds.
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
    }
    let _ = std::fs::write(&path, me.to_string());
    // Two copies started in the same instant (logon task + Run key) both get here: the one
    // whose number is not in the file after a moment steps aside.
    std::thread::sleep(std::time::Duration::from_millis(700));
    let current = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    if current.is_some_and(|c| c != me) {
        tracing::info!("another agent started at the same time — this copy exits");
        std::process::exit(0);
    }
}
