use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Shared secret; must match the teacher console.
    pub key: String,
    /// Room to join. Empty = join any console that knows the key.
    pub room: String,
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
            key: "vimora".into(),
            room: String::new(),
            server: None,
            name: None,
            unlock_after_disconnect_secs: 120,
        }
    }
}

impl Config {
    pub fn merge_cli(
        mut self,
        key: Option<String>,
        room: Option<String>,
        server: Option<String>,
        name: Option<String>,
    ) -> Self {
        if let Some(k) = key {
            self.key = k;
        }
        if let Some(r) = room {
            self.room = r;
        }
        if server.is_some() {
            self.server = server;
        }
        if name.is_some() {
            self.name = name;
        }
        self
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
