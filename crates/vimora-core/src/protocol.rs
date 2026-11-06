//! Wire protocol shared by the teacher console (server) and the student agent (client).
//!
//! Every message is a bincode-encoded enum, framed with a big-endian `u32` length prefix
//! (see [`crate::net`]).

use serde::{Deserialize, Serialize};

/// Bumped whenever the wire format changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 10;

/// UDP port the teacher console broadcasts discovery beacons on.
pub const DISCOVERY_PORT: u16 = 47800;
/// Default TCP port the teacher console listens on.
pub const DEFAULT_TCP_PORT: u16 = 47801;
/// Magic prefix of discovery beacons.
pub const BEACON_MAGIC: &[u8; 8] = b"VIMORA01";

/// Broadcast by the teacher console so students on the LAN can find it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beacon {
    pub version: u32,
    pub server_name: String,
    pub tcp_port: u16,
    /// The console's public key: paired PCs connect only to the console they pinned.
    pub console_key: crate::auth::ConsoleKey,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClientInfo {
    pub hostname: String,
    pub username: String,
    pub os: String,
    /// Optional display name configured on the student PC (e.g. "PC-12").
    pub display_name: Option<String>,
    /// Hardware identity (primary network card MAC); stays the same when the PC is renamed
    /// and differs between PCs cloned from one image.
    pub machine_id: String,
}

impl ClientInfo {
    /// Stable key the console uses for rooms and names.
    pub fn key(&self) -> String {
        let base = if self.machine_id.is_empty() {
            self.hostname.as_str()
        } else {
            self.machine_id.as_str()
        };
        match &self.display_name {
            Some(n) => format!("{base}/{n}"),
            None => base.to_string(),
        }
    }
}

impl ClientInfo {
    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.hostname)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClientStatus {
    pub locked: bool,
    pub showing_teacher: bool,
    pub app_blocklist: usize,
    /// Blocked domains currently enforced.
    pub web_blocklist: usize,
    /// Blocked search keywords currently enforced.
    pub keywords: usize,
    pub safe_search: bool,
    pub help_requested: bool,
    /// Privileged Vimora service is running on this PC (freeze, updates, NetControl).
    pub service: bool,
    pub freeze: FreezeState,
    /// Shadow-mode overlay fill level while protected.
    pub shield_usage: Option<ShieldUsage>,
    /// Detail when Shadow mode is unsupported / not installed.
    pub shield_note: Option<String>,
    /// Pending OS updates from the last check (None = not checked yet).
    pub os_updates: Option<u32>,
    /// Pending application updates from the last check.
    pub app_updates: Option<u32>,
    /// An update/maintenance job is running right now.
    pub busy: Option<String>,
    /// Teacher changes waiting for the next commit cycle (installs, renames on a protected PC).
    pub queued: u32,
    /// Folders that survive restarts while Shadow mode is on.
    pub persistent_folders: Vec<String>,
}

/// An installed application and whether Vimora lets it update.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppInfo {
    /// Stable id: winget package id, apt package or snap name.
    pub id: String,
    pub name: String,
    pub version: String,
    /// Newer version available (from the last check).
    pub available: Option<String>,
    /// "winget", "apt" or "snap".
    pub source: String,
    pub updates: bool,
}

/// How full the Shadow-mode overlay is (changes kept until the next restart).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShieldUsage {
    pub used_mb: u32,
    pub max_mb: u32,
}

impl ShieldUsage {
    pub fn percent(&self) -> u32 {
        self.used_mb
            .saturating_mul(100)
            .checked_div(self.max_mb)
            .unwrap_or(0)
            .min(100)
    }
}

/// Disk-freeze ("Deep Freeze"/Shadow Defender style) state.
/// Windows: Unified Write Filter (UWF). Ubuntu: overlayroot.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum FreezeState {
    #[default]
    Unknown,
    /// This edition/OS has no supported write filter (e.g. Windows Home/Pro without UWF).
    Unsupported,
    Thawed,
    Frozen,
    /// Will be frozen after the next restart.
    FreezePending,
    /// Will be thawed after the next restart.
    ThawPending,
    /// Thawed temporarily to install updates; refreezes automatically.
    Maintenance,
}

impl FreezeState {
    pub fn is_frozen(self) -> bool {
        matches!(self, FreezeState::Frozen | FreezeState::ThawPending)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FreezeAction {
    Freeze,
    Thaw,
    /// Thaw → install all updates → freeze again (Windows: UWF servicing mode).
    Maintenance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OsUpdateMode {
    /// Windows installs updates by itself.
    Auto,
    /// Only download/notify; the teacher installs from the console.
    #[default]
    Manual,
    /// Windows Update / unattended-upgrades off.
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdatePolicy {
    pub os: OsUpdateMode,
    /// Let applications (Store/winget/snap, Chrome, Edge) update themselves.
    pub apps_auto: bool,
    /// Vimora updates itself from the project's GitHub releases.
    #[serde(default = "yes")]
    pub vimora: bool,
}

fn yes() -> bool {
    true
}

/// Commands that need administrator/root rights. They are signed by the console and executed
/// by the privileged Vimora service, so a student can't fake them locally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrivCommand {
    Freeze {
        action: FreezeAction,
        restart: bool,
    },
    SetUpdatePolicy(UpdatePolicy),
    CheckUpdates,
    /// Update Vimora itself to the latest GitHub release now.
    UpdateVimora,
    InstallUpdates {
        os: bool,
        apps: bool,
        restart: bool,
    },
    Power(PowerAction),
    SetNetPolicy(NetPolicy),
    /// Collect the installed applications (reported with `ClientMsg::Apps`).
    ListApps,
    /// Allow / forbid updates per application id.
    SetAppUpdates(Vec<(String, bool)>),
    /// Folders kept across restarts in Shadow mode (replaces the list).
    SetPersistentFolders(Vec<String>),
    /// Change the system computer name (takes effect after a restart on Windows).
    RenameComputer {
        name: String,
        restart: bool,
    },
}

/// Valid computer name: 1–15 letters, digits or '-', not starting/ending with '-'.
pub fn valid_computer_name(name: &str) -> bool {
    (1..=15).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.chars().all(|c| c.is_ascii_digit())
}

/// Internet control ("NetControl") policy for a student PC.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NetPolicy {
    /// Domains that must not open (subdomains included).
    pub domains: Vec<String>,
    /// Words/phrases that must not be searched for (Google, Yandex, Bing, YouTube, …).
    /// A browser tab whose title contains one of them is closed and the teacher is alerted.
    pub keywords: Vec<String>,
    /// Force SafeSearch on Google/Bing/DuckDuckGo/Yandex and restricted mode on YouTube.
    pub safe_search: bool,
}

impl NetPolicy {
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty() && self.keywords.is_empty() && !self.safe_search
    }
}

/// How often and how large the student should send screen frames.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct StreamConfig {
    /// 0 = do not send frames at all.
    pub interval_ms: u32,
    pub max_width: u32,
    /// JPEG quality 1..=100
    pub quality: u8,
}

impl StreamConfig {
    pub const THUMBNAIL: StreamConfig = StreamConfig {
        interval_ms: 2000,
        max_width: 480,
        quality: 55,
    };
    pub const LIVE: StreamConfig = StreamConfig {
        interval_ms: 120,
        max_width: 1600,
        quality: 70,
    };
    pub const OFF: StreamConfig = StreamConfig {
        interval_ms: 0,
        max_width: 0,
        quality: 0,
    };
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PowerAction {
    Shutdown,
    Restart,
    Logoff,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpecialKey {
    Enter,
    Backspace,
    Tab,
    Escape,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
}

/// Remote-control input. Coordinates are normalised to 0.0..=1.0 of the student's screen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InputEvent {
    MouseMove { x: f32, y: f32 },
    MouseButton { button: MouseButton, down: bool },
    Scroll { dx: f32, dy: f32 },
    Text(String),
    Key { key: SpecialKey, down: bool },
}

/// Commands the teacher sends to students.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Command {
    /// Fullscreen lock screen; keyboard and mouse are blocked.
    Lock {
        title: String,
        message: String,
    },
    Unlock,
    /// Show a popup message.
    Message {
        title: String,
        text: String,
    },
    OpenUrl(String),
    Run {
        program: String,
        args: Vec<String>,
    },
    Power(PowerAction),
    /// Process names (e.g. "chrome", "steam.exe") that get terminated while blocked.
    SetAppBlocklist(Vec<String>),
    /// Internet control: blocked domains, blocked search keywords, SafeSearch.
    SetNetPolicy(NetPolicy),
    SendFile {
        name: String,
        data: Vec<u8>,
    },
    /// Start fullscreen presentation of the teacher's screen.
    ShowStart,
    ShowStop,
    SetStream(StreamConfig),
    /// Acknowledge / clear a student's help request.
    ClearHelp,
    /// The teacher closed the chat: the student's chat panel closes and stays closed until
    /// the next message.
    ChatEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Hello {
        version: u32,
        nonce: Vec<u8>,
        console_key: crate::auth::ConsoleKey,
    },
    /// `sig` = console's signature over (client nonce, server nonce).
    Welcome {
        sig: Vec<u8>,
        stream: StreamConfig,
    },
    Command(Command),
    Input(InputEvent),
    /// JPEG frame of the teacher's screen while presenting.
    ShowFrame {
        width: u32,
        height: u32,
        jpeg: Vec<u8>,
    },
    /// Signed [`PrivCommand`] (bincode in `payload`), see [`crate::auth::Identity::sign_priv`].
    Privileged {
        counter: u64,
        payload: Vec<u8>,
        sig: Vec<u8>,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello {
        version: u32,
        nonce: Vec<u8>,
        info: ClientInfo,
    },
    Frame {
        width: u32,
        height: u32,
        jpeg: Vec<u8>,
    },
    Status(ClientStatus),
    CommandResult {
        ok: bool,
        message: String,
    },
    HelpRequest {
        text: String,
    },
    /// Something the teacher should know about (e.g. a blocked keyword was searched).
    Alert {
        text: String,
    },
    /// Installed applications (answer to `PrivCommand::ListApps`).
    Apps(Vec<AppInfo>),
    /// A student's reply in the message chat.
    Chat {
        text: String,
    },
    Pong,
}
