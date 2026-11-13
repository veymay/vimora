use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use vimora_core::DEFAULT_TCP_PORT;

/// What the teacher remembers about a computer that has connected before.
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
    /// The version it had before the last change (shown as "0.3.28 → 0.3.30").
    pub prev_version: String,
    /// Hardware address (12 hex digits) for Wake-on-LAN.
    pub mac: String,
    /// Unix seconds of the last time it was connected.
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TeacherConfig {
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
    /// Look for a new teacher version on GitHub at start-up (and every few hours).
    pub check_updates: bool,
    pub thumb_interval_ms: u32,
    /// Computers per row in the grid (zoom).
    pub grid_columns: u32,
    pub view: ViewMode,
    /// Rooms the teacher created, in tab order.
    pub rooms: Vec<String>,
    /// Computer (system hostname) -> room.
    pub assignments: BTreeMap<String, String>,
    /// Computer (system hostname) -> name shown in the teacher.
    pub aliases: BTreeMap<String, String>,
    /// Every computer that has ever connected (key -> last name), for the "N kompyuter" count.
    pub known: BTreeMap<String, String>,
    /// Details of every computer seen before, so it stays in the list (offline) after it
    /// disconnects until the teacher removes it.
    pub known_pcs: BTreeMap<String, KnownPc>,
    /// Sites opened recently (newest first), offered again in the "Sayt" window.
    pub recent_urls: Vec<String>,
    /// Computers the teacher removed from this teacher: they are turned away if they come
    /// back, so another teacher can take them.
    pub removed: std::collections::BTreeSet<String>,
    /// Icon shown on the lock screen (a glyph of the icon font; empty = lock).
    pub lock_icon: String,
    /// Sites the teacher saved as favourites in the "Sayt" window.
    pub bookmarks: Vec<String>,
    /// Shadow mode: folders kept across restarts (one per line).
    pub persistent_folders: String,
    /// Shadow Defender password on the student PCs (empty = its default, no password).
    pub shadow_password: String,
    pub lock_title: String,
    pub lock_message: String,
    /// 4–8 digit code that opens the lock screen on a student PC without the teacher.
    pub unlock_pin: String,
    pub app_blocklist: String,
    pub web_blocklist: String,
    pub keyword_blocklist: String,
    /// Site categories ("O'yinlar", imported lists, …) and what is blocked in them.
    pub site_categories: Vec<crate::blockcat::BlockCategory>,
    /// Program categories.
    pub app_categories: Vec<crate::blockcat::BlockCategory>,
    /// Word categories (searches) — like `site_categories`.
    pub word_categories: Vec<crate::blockcat::BlockCategory>,
    pub safe_search: bool,
    /// What students may use: USB, CD/DVD, printer, camera, microphone, Bluetooth, sound.
    pub device_policy: vimora_core::DevicePolicy,
    /// Sites: 0 = listed blocked, 1 = only listed allowed, 2 = internet off.
    pub web_mode: u8,
    pub allow_sites: String,
    /// Programs: 0 = listed blocked, 1 = only listed allowed.
    pub app_mode: u8,
    pub allow_apps: String,
    /// Blocked outgoing ports ("25, 6881").
    pub blocked_ports: String,
    /// "Ekran tarixi": save every computer's picture every N minutes (0 = off).
    pub history_minutes: u32,
    /// "Jadval": things the teacher does by itself at set times.
    pub schedules: Vec<Schedule>,
    /// Administrator password (salted hash, hex). Empty = no password: everyone is admin.
    pub admin_salt: String,
    pub admin_hash: String,
    pub update_policy: vimora_core::UpdatePolicy,
    /// Per computer (key): automatic updates set in the Updates drawer. Computers without an
    /// entry follow `update_policy`.
    pub update_policies: BTreeMap<String, vimora_core::UpdatePolicy>,
}

/// The language chosen in Vimora's setup window (Windows: HKLM\SOFTWARE\Vimora "Language").
fn installed_language() -> Option<vimora_ui::Lang> {
    #[cfg(windows)]
    {
        let out = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Vimora", "/v", "Language"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        let code = text.split_whitespace().last()?.to_string();
        match code.as_str() {
            "uz" => Some(vimora_ui::Lang::Uz),
            "ru" => Some(vimora_ui::Lang::Ru),
            "en" => Some(vimora_ui::Lang::En),
            _ => None,
        }
    }
    #[cfg(not(windows))]
    None
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

impl Default for TeacherConfig {
    fn default() -> Self {
        let language = installed_language().unwrap_or_else(vimora_ui::Lang::detect);
        let (lock_title, lock_message) = default_lock(language);
        Self {
            server_name: hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "Teacher".into()),
            port: DEFAULT_TCP_PORT,
            language,
            dark_mode: true,
            theme_system: true,
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
            removed: Default::default(),
            lock_icon: String::new(),
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
            word_categories: vec![],
            safe_search: true,
            device_policy: Default::default(),
            web_mode: 0,
            allow_sites: String::new(),
            app_mode: 0,
            allow_apps: String::new(),
            blocked_ports: String::new(),
            history_minutes: 0,
            schedules: vec![],
            admin_salt: String::new(),
            admin_hash: String::new(),
            update_policies: BTreeMap::new(),
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

/// The teacher's secret identity. Kept in its own file next to the config: if it were lost,
/// already paired student PCs would refuse the new teacher.
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

pub fn load() -> TeacherConfig {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &TeacherConfig) {
    if let Some(p) = path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&p, serde_json::to_string_pretty(cfg).unwrap_or_default()) {
            tracing::warn!("cannot save config {}: {e}", p.display());
        }
    }
}

impl TeacherConfig {
    /// The update policy of one computer.
    pub fn policy_of(&self, key: &str) -> vimora_core::UpdatePolicy {
        self.update_policies
            .get(key)
            .copied()
            .unwrap_or(self.update_policy)
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

/// Folder with `server.json` and the teacher identity.
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

/// One scheduled action ("Jadval"): at `time` on the ticked weekdays (Monday first).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Schedule {
    pub on: bool,
    /// "08:30"
    pub time: String,
    pub days: [bool; 7],
    /// See `app::admin::ACTIONS`.
    pub action: u8,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            on: true,
            time: "08:00".into(),
            days: [true, true, true, true, true, true, false],
            action: 0,
        }
    }
}
