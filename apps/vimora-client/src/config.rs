use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Optional fixed console address (ip:port). Skips LAN discovery.
    pub server: Option<String>,
    /// Optional display name.
    pub name: Option<String>,
    /// Auto-unlock if the console is gone for this many seconds (0 = never).
    pub unlock_after_disconnect_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: None,
            name: None,
            unlock_after_disconnect_secs: 120,
        }
    }
}

impl Config {
    pub fn merge_cli(mut self, server: Option<String>, name: Option<String>) -> Self {
        if server.is_some() {
            self.server = server;
        }
        if name.is_some() {
            self.name = name;
        }
        self
    }
}

// ---------------------------------------------------------------------------------------------
// Pairing (public key of the console this PC follows). The authoritative copy lives with the
// privileged service; this per-user copy lets the agent pick the right console on the network
// and verify it even without the service (dev mode). It only holds a *public* key.

fn pin_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("paired-console"))
}

pub fn pin() -> Option<vimora_core::auth::ConsoleKey> {
    pin_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| vimora_core::auth::key_from_hex(&s))
}

pub fn save_pin(key: &vimora_core::auth::ConsoleKey) {
    if let Some(p) = pin_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, vimora_core::auth::to_hex(key));
    }
}

pub fn forget_pin() -> std::io::Result<()> {
    match pin_path() {
        Some(p) if p.exists() => std::fs::remove_file(p),
        _ => Ok(()),
    }
}

/// The console address that worked last time: tried first on the next start, so the agent
/// reconnects even if LAN broadcasts are blocked or slow.
pub fn last_console() -> Option<String> {
    dirs::config_dir()
        .map(|d| d.join("vimora").join("last-console"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_last_console(addr: &str) {
    if let Some(p) = dirs::config_dir().map(|d| d.join("vimora").join("last-console")) {
        if last_console().as_deref() == Some(addr) {
            return;
        }
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, addr);
    }
}

/// System-wide location first (set by the administrator), then the per-user one.
fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    if let Some(pd) = std::env::var_os("ProgramData") {
        v.push(PathBuf::from(pd).join("Vimora").join("client.json"));
    }
    #[cfg(unix)]
    v.push(PathBuf::from("/etc/vimora/client.json"));
    if let Some(d) = dirs::config_dir() {
        v.push(d.join("vimora").join("client.json"));
    }
    v
}

pub fn load() -> Config {
    for p in candidates() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            match serde_json::from_str(&s) {
                Ok(c) => {
                    tracing::info!("config: {}", p.display());
                    return c;
                }
                Err(e) => tracing::warn!("bad config {}: {e}", p.display()),
            }
        }
    }
    Config::default()
}

pub fn save(cfg: &Config) -> anyhow::Result<PathBuf> {
    let mut last_err = None;
    for p in candidates() {
        if let Some(dir) = p.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                continue;
            }
        }
        match std::fs::write(&p, serde_json::to_string_pretty(cfg)?) {
            Ok(()) => return Ok(p),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err
        .map(Into::into)
        .unwrap_or_else(|| anyhow::anyhow!("no writable config location")))
}
