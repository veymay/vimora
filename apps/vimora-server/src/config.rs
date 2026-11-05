use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use vimora_core::DEFAULT_TCP_PORT;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerConfig {
    /// Shared connection key; student agents must use the same key.
    pub key: String,
    /// Network group announced on the LAN (advanced; empty = any agent with the key joins).
    pub room: String,
    /// Name shown to students / in logs.
    pub server_name: String,
    pub port: u16,
    pub dark_mode: bool,
    pub thumb_interval_ms: u32,
    /// Computers per row in the grid (zoom).
    pub grid_columns: u32,
    pub view: ViewMode,
    /// Rooms the teacher created, in tab order.
    pub rooms: Vec<String>,
    /// Computer (system hostname) -> room.
    pub assignments: BTreeMap<String, String>,
    /// Computer (system hostname) -> name shown in the console.
    pub aliases: BTreeMap<String, String>,
    /// Every computer that has ever connected (key -> last name), for the "N kompyuter" count.
    pub known: BTreeMap<String, String>,
    /// Shadow mode: folders kept across restarts (one per line).
    pub persistent_folders: String,
    pub lock_title: String,
    pub lock_message: String,
    pub app_blocklist: String,
    pub web_blocklist: String,
    pub keyword_blocklist: String,
    pub safe_search: bool,
    pub update_policy: vimora_core::UpdatePolicy,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            key: "vimora".into(),
            room: String::new(),
            server_name: hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "Teacher".into()),
            port: DEFAULT_TCP_PORT,
            dark_mode: true,
            thumb_interval_ms: 2000,
            grid_columns: 6,
            view: ViewMode::Grid,
            rooms: vec![],
            assignments: BTreeMap::new(),
            aliases: BTreeMap::new(),
            known: BTreeMap::new(),
            persistent_folders: String::new(),
            lock_title: "Diqqat! Ekran qulflangan".into(),
            lock_message: "Iltimos, o'qituvchini tinglang.".into(),
            app_blocklist: "steam, discord, telegram".into(),
            web_blocklist: "tiktok.com\ninstagram.com".into(),
            keyword_blocklist: "kazino\ncasino\n1xbet".into(),
            safe_search: true,
            update_policy: vimora_core::UpdatePolicy {
                os: vimora_core::OsUpdateMode::Manual,
                apps_auto: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViewMode {
    /// Cards with screen thumbnails.
    #[default]
    Grid,
    /// Compact table, one computer per row.
    List,
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vimora").join("server.json"))
}

pub fn load() -> ServerConfig {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &ServerConfig) {
    if let Some(p) = path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&p, serde_json::to_string_pretty(cfg).unwrap_or_default()) {
            tracing::warn!("cannot save config {}: {e}", p.display());
        }
    }
}

/// Splits on commas, semicolons and new lines (spaces are kept: keywords may be phrases).
pub fn split_list(s: &str) -> Vec<String> {
    s.split([',', '\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
