use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use vimora_core::DEFAULT_TCP_PORT;

/// What the console remembers about a computer that has connected before.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct KnownPc {
    pub label: String,
    pub hostname: String,
    pub user: String,
    pub os: String,
    pub ip: String,
    /// Agent version it reported last ("0.3.8"); empty for old agents.
    pub version: String,
    /// Hardware address (12 hex digits) for Wake-on-LAN.
    pub mac: String,
    /// Unix seconds of the last time it was connected.
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerConfig {
    /// Name shown to students / in logs.
    pub server_name: String,
    pub port: u16,
    /// Interface language.
    pub language: vimora_ui::Lang,
    pub dark_mode: bool,
    /// Follow the operating system's light/dark setting (overrides `dark_mode`).
    pub theme_system: bool,
    /// Play short sounds for results, warnings and student alerts.
    pub sounds: bool,
    /// Order of the computers in the grid and list.
    pub sort: SortBy,
    /// Look for a new console version on GitHub at start-up (and every few hours).
    pub check_updates: bool,
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
    /// Details of every computer seen before, so it stays in the list (offline) after it
    /// disconnects until the teacher removes it.
    pub known_pcs: BTreeMap<String, KnownPc>,
    /// Sites opened recently (newest first), offered again in the "Sayt" window.
    pub recent_urls: Vec<String>,
    /// Sites the teacher saved as favourites in the "Sayt" window.
    pub bookmarks: Vec<String>,
    /// Shadow mode: folders kept across restarts (one per line).
    pub persistent_folders: String,
    /// Shadow Defender password on the student PCs (empty = its default, no password).
    pub shadow_password: String,
    pub lock_title: String,
    pub lock_message: String,
    /// 4–8 digit code that opens the lock screen on a student PC without the console.
    pub unlock_pin: String,
    pub app_blocklist: String,
    pub web_blocklist: String,
    pub keyword_blocklist: String,
    /// Site categories ("O'yinlar", imported lists, …) and what is blocked in them.
    pub site_categories: Vec<crate::blockcat::BlockCategory>,
    /// Program categories.
    pub app_categories: Vec<crate::blockcat::BlockCategory>,
    pub safe_search: bool,
    pub update_policy: vimora_core::UpdatePolicy,
}

/// Default lock-screen title and text in each language (the teacher can edit them).
pub fn default_lock(l: vimora_ui::Lang) -> (&'static str, &'static str) {
    use vimora_ui::Lang;
    match l {
        Lang::Uz => (
            "Diqqat! Ekran qulflangan",
            "Iltimos, o'qituvchini tinglang.",
        ),
        Lang::En => (
            "Attention! The screen is locked",
            "Please listen to your teacher.",
        ),
        Lang::Ru => (
            "Внимание! Экран заблокирован",
            "Пожалуйста, слушайте учителя.",
        ),
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        let language = vimora_ui::Lang::detect();
        let (lock_title, lock_message) = default_lock(language);
        Self {
            server_name: hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "Teacher".into()),
            port: DEFAULT_TCP_PORT,
            language,
            dark_mode: true,
            theme_system: false,
            sounds: true,
            sort: SortBy::Name,
            check_updates: true,
            thumb_interval_ms: 2000,
            grid_columns: 6,
            view: ViewMode::Grid,
            rooms: vec![],
            assignments: BTreeMap::new(),
            aliases: BTreeMap::new(),
            known: BTreeMap::new(),
            known_pcs: BTreeMap::new(),
            recent_urls: vec![],
            bookmarks: vec![],
            persistent_folders: String::new(),
            shadow_password: String::new(),
            lock_title: lock_title.into(),
            lock_message: lock_message.into(),
            unlock_pin: String::new(),
            app_blocklist: "steam, discord, telegram".into(),
            web_blocklist: "tiktok.com\ninstagram.com".into(),
            keyword_blocklist: "kazino\ncasino\n1xbet".into(),
            site_categories: vec![],
            app_categories: vec![],
            safe_search: true,
            update_policy: vimora_core::UpdatePolicy {
                os: vimora_core::OsUpdateMode::Manual,
                apps_auto: false,
                vimora: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortBy {
    /// Natural order of the shown names (PC-2 before PC-10).
    #[default]
    Name,
    /// By IP address (follows the physical network layout in many rooms).
    Ip,
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

/// The console's secret identity. Kept in its own file next to the config: if it were lost,
/// already paired student PCs would refuse the new console.
pub fn identity() -> vimora_core::auth::Identity {
    use vimora_core::auth::Identity;
    let path = path().map(|p| p.with_file_name("identity.key"));
    if let Some(id) = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| Identity::from_hex(&s))
    {
        return id;
    }
    let id = Identity::generate();
    if let Some(p) = &path {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if std::fs::write(p, id.to_hex()).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    id
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

/// Items of a block list that are switched on (a leading `#` switches an item off but keeps it).
pub fn active_list(s: &str) -> Vec<String> {
    split_list(s)
        .into_iter()
        .filter(|i| !i.starts_with('#'))
        .collect()
}

/// Folder with `server.json` and the console identity.
pub fn dir() -> Option<PathBuf> {
    path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Opens a web page or folder with the system's default handler.
pub fn open(target: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    if let Err(e) = r {
        tracing::warn!("cannot open {target}: {e}");
    }
}
