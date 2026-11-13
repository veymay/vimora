//! Wire protocol shared by the teacher (server) and the student agent (student).
//!
//! Every message is a bincode-encoded enum, framed with a big-endian `u32` length prefix
//! (see [`crate::net`]).

use serde::{Deserialize, Serialize};

/// Wire family. Never bump it for additions: new enum variants are appended at the end and
/// unknown messages are skipped by the receiver (see [`crate::net::read_msg`]), so builds of
/// different ages keep working together. Peers refuse each other only below
/// [`MIN_PROTOCOL_VERSION`].
pub const PROTOCOL_VERSION: u32 = 10;
/// Oldest wire family this build still talks to.
pub const MIN_PROTOCOL_VERSION: u32 = 10;

/// UDP port the teacher broadcasts discovery beacons on.
pub const DISCOVERY_PORT: u16 = 47800;
/// Default TCP port the teacher listens on.
pub const DEFAULT_TCP_PORT: u16 = 47801;
/// Magic prefix of discovery beacons.
pub const BEACON_MAGIC: &[u8; 8] = b"VIMORA01";

/// Broadcast by the teacher so students on the LAN can find it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beacon {
    pub version: u32,
    pub server_name: String,
    pub tcp_port: u16,
    /// The teacher's public key: paired PCs connect only to the teacher they pinned.
    pub teacher_key: crate::auth::TeacherKey,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StudentInfo {
    pub hostname: String,
    pub username: String,
    pub os: String,
    /// Optional display name configured on the student PC (e.g. "PC-12").
    pub display_name: Option<String>,
    /// Hardware identity (primary network card MAC); stays the same when the PC is renamed
    /// and differs between PCs cloned from one image.
    pub machine_id: String,
}

impl StudentInfo {
    /// Stable key the teacher uses for rooms and names.
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

impl StudentInfo {
    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.hostname)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StudentStatus {
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

/// What happened to a path since the PC booted under Shadow protection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// One change held in the Shadow overlay (lost on restart unless the teacher commits it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeInfo {
    pub path: String,
    pub kind: ChangeKind,
    pub size: u64,
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
    /// Only download/notify; the teacher installs from the teacher.
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

/// Commands that need administrator/root rights. They are signed by the teacher and executed
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
    /// Collect the installed applications (reported with `StudentMsg::Apps`).
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
    // New variants go at the END only: the wire format is positional, so older and newer
    // builds keep understanding each other.
    /// Password of the Shadow Defender installed on the student PC (Windows).
    SetShadowPassword(String),
    /// List what the Shadow overlay holds (reported with `StudentMsg::Changes`).
    ListChanges,
    /// Make these paths survive restarts (a "commit" in the Shadow overlay).
    CommitPaths(Vec<String>),
    /// Forget the paired teacher: the next teacher found may pair with this PC.
    Unpair,
    /// Which write filter "freezing" uses on Windows: 0 = automatic (Shadow Defender if it is
    /// installed, else UWF), 1 = Shadow Defender, 2 = Unified Write Filter.
    SetShieldEngine(u8),
    /// What the student may use: USB drives, CD/DVD, printer, camera, microphone, Bluetooth,
    /// sound. Kept until changed (also with no teacher connected).
    SetDevicePolicy(DevicePolicy),
    /// Stricter internet rules: internet off (the local network keeps working), only listed
    /// sites allowed, blocked network ports.
    SetAccessPolicy(AccessPolicy),
    /// Start (true) or stop a system service.
    ServiceControl {
        name: String,
        start: bool,
    },
    /// Run a command as administrator (PowerShell on Windows, sh on Ubuntu); the output comes
    /// back as the command result. Recorded in the teacher's activity log.
    RunScript(String),
    /// Uninstall an application (id as in [`AppInfo::id`]), silently.
    UninstallApp(String),
    /// Remove Vimora from this computer (only the teacher can; nothing is left behind).
    UninstallVimora,
    /// Install these pending system updates (ids from [`UpdatesInfo`]); on a frozen computer
    /// through a commit cycle so they stay.
    InstallUpdateIds(Vec<String>),
    /// Update these applications now (ids as in [`AppInfo::id`]); kept on a frozen computer.
    UpgradeApps(Vec<String>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessPolicy {
    /// No internet at all; the classroom network (teacher) keeps working.
    pub internet_off: bool,
    /// Only these sites open in browsers (empty = no such limit).
    pub allow_sites: Vec<String>,
    /// Outgoing TCP/UDP ports that are blocked.
    pub ports: Vec<u16>,
}

/// USB storage: what a plugged-in flash drive may do.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum UsbMode {
    #[default]
    Allow,
    /// Files can be opened but not written.
    ReadOnly,
    Block,
}

/// Device restrictions (`true` = blocked / off).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevicePolicy {
    pub usb: UsbMode,
    pub cd: bool,
    pub printer: bool,
    pub camera: bool,
    pub microphone: bool,
    pub bluetooth: bool,
    /// Sound muted (kept muted while set).
    pub sound_off: bool,
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
    // Append-only (see PrivCommand).
    /// Code that opens the lock screen on the student PC without the teacher (internet lost).
    /// Only a salted hash travels; `digits` = code length (the lock screen draws that many
    /// cells). An empty `hash` removes the code.
    UnlockCode {
        salt: Vec<u8>,
        hash: Vec<u8>,
        digits: u8,
    },
    /// How the lock screen looks: the icon in its tile (one glyph of the icon font; empty =
    /// the default lock). Sent before `Lock`.
    LockStyle {
        icon: String,
    },
    /// Report the PC's technical state once ([`StudentMsg::SystemInfo`]); the teacher asks
    /// every couple of seconds while its "Kompyuter holati" drawer is open.
    GetSystemInfo,
    /// End a process (Task Manager's "End task").
    KillProcess(u32),
    /// Only these programs may run (process names); empty = no such limit. Windows.
    SetAppAllowlist(Vec<String>),
    /// Block the keyboard and/or the mouse without the lock screen (teacher's remote control
    /// still works).
    InputLock {
        keyboard: bool,
        mouse: bool,
    },
    /// Open a file the teacher sent (name as in `SendFile`).
    OpenFile(String),
    /// Send back what is in the student's "Topshiriq" folder ([`StudentMsg::Collected`]).
    CollectFiles,
    /// A quick question: answer with one of `options`, or with text when there are none
    /// ([`StudentMsg::PollAnswer`]).
    Poll {
        id: u32,
        question: String,
        options: Vec<String>,
    },
    /// Close the poll window.
    PollEnd,
    /// Lesson goal and countdown at the top of the screen (empty goal and `end_unix` 0 = off).
    LessonBanner {
        goal: String,
        end_unix: u64,
    },
    /// Start a test in a fullscreen window ([`StudentMsg::TestResult`] when handed in).
    StartTest(TestDef),
    /// Time is up: hand in what is answered.
    EndTest,
    /// Report antivirus, firewall, hardware, sessions and services ([`StudentMsg::SecurityInfo`]).
    GetSecurityInfo,
    /// A newer Vimora installer from the teacher (for computers without internet), with the
    /// signed `SHA256SUMS` and its signature: the computer's service checks the signature against
    /// the release key built into it before installing.
    InstallUpdate {
        name: String,
        data: Vec<u8>,
        sums: String,
        sig: String,
    },
    /// Sent by the teacher program right after connecting: its computer's name (shown in the
    /// student's tray window).
    TeacherInfo {
        name: String,
    },
    /// How the lock screen looks besides its icon (sent before `Lock`).
    LockLook(LockLook),
}

/// The lock screen's look. All zero / empty = the default look.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LockLook {
    /// Clock size: 0 normal, 1 small, 2 large, 3 off.
    pub clock: u8,
    /// Hide the logo and the brand name.
    pub no_logo: bool,
    /// The name next to the logo (empty = "VIMORA").
    pub brand: String,
    /// Typeface of the texts: 0 Geist, 1 Geist Mono.
    pub font: u8,
    /// Accent colour (RGB); all zero = the default green.
    pub accent: [u8; 3],
}

/// Security and inventory facts about a student PC.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SecurityInfo {
    /// (name, on, up to date)
    pub antivirus: Vec<(String, bool, bool)>,
    /// (profile, on)
    pub firewall: Vec<(String, bool)>,
    /// User Account Control on (None = not applicable).
    pub uac: Option<bool>,
    /// (what, value): maker, model, serial, graphics, monitors …
    pub hardware: Vec<(String, String)>,
    /// Signed-in users.
    pub sessions: Vec<String>,
    pub services: Vec<ServiceInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ServiceInfo {
    pub name: String,
    pub display: String,
    pub running: bool,
    /// "Automatic", "Manual", "Disabled", "enabled" …
    pub start: String,
}

/// A test the teacher made.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TestDef {
    pub id: u32,
    pub title: String,
    /// Time limit (0 = none).
    pub minutes: u32,
    /// Questions and their answer options in a different order for each student.
    pub shuffle: bool,
    pub questions: Vec<TestQuestion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TestQuestion {
    pub text: String,
    /// Choices; empty = a written answer.
    pub options: Vec<String>,
}

/// One answer: the question's index in the teacher's order, the chosen option (also in the
/// teacher's order) or the written text.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TestAnswer {
    pub question: u32,
    pub choice: Option<u32>,
    pub text: String,
}

/// Technical state of a student PC, Task Manager style.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SystemInfo {
    pub os: String,
    pub cpu_name: String,
    pub cores: u32,
    /// Whole-CPU load, 0–100.
    pub cpu: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disks: Vec<DiskInfo>,
    pub battery: Option<BatteryInfo>,
    /// Network throughput since the previous report, bytes per second.
    pub net_rx: u64,
    pub net_tx: u64,
    pub uptime_secs: u64,
    pub processes: Vec<ProcInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DiskInfo {
    /// "C:\\" or "/".
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct BatteryInfo {
    pub percent: u8,
    pub charging: bool,
    /// On mains power (charging or full).
    pub plugged: bool,
    /// Estimated time left on battery.
    pub secs_left: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    /// Share of the whole CPU, 0–100 (Task Manager style).
    pub cpu: f32,
    /// Memory in use, bytes.
    pub mem: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TeacherMsg {
    Hello {
        version: u32,
        nonce: Vec<u8>,
        teacher_key: crate::auth::TeacherKey,
    },
    /// `sig` = teacher's signature over (student nonce, server nonce).
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
pub enum StudentMsg {
    Hello {
        version: u32,
        nonce: Vec<u8>,
        info: StudentInfo,
    },
    Frame {
        width: u32,
        height: u32,
        jpeg: Vec<u8>,
    },
    Status(StudentStatus),
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
    // Append-only (see PrivCommand).
    /// Changes held in the Shadow overlay (answer to `PrivCommand::ListChanges`).
    Changes(Vec<ChangeInfo>),
    /// The agent's own version ("0.3.8"), sent once after the handshake.
    AgentVersion(String),
    /// Shape of the student's mouse pointer while the teacher watches live
    /// (see [`cursor_kind`]).
    Cursor(u8),
    /// The student has seen the teacher's chat messages (Telegram's second tick).
    ChatRead,
    /// Answer to [`Command::GetSystemInfo`].
    SystemInfo(SystemInfo),
    /// The student's assignment files: (path inside the folder, content).
    Collected(Vec<(String, Vec<u8>)>),
    PollAnswer {
        id: u32,
        choice: Option<u32>,
        text: String,
    },
    TestResult {
        id: u32,
        answers: Vec<TestAnswer>,
    },
    SecurityInfo(SecurityInfo),
    /// Sent instead of `Hello` by a computer removed from its teacher: "I am free, add me".
    /// The teacher lists it (in every teacher on the network) until a teacher adds it.
    Free(StudentInfo),
    /// The computer must restart to finish installed updates (sent when it changes and to a new
    /// teacher session).
    RebootNeeded(bool),
    /// The system and its pending updates (Windows Update with drivers / apt).
    Updates(UpdatesInfo),
    /// Something the student copied (only while the teacher watches this computer live).
    Clipboard(ClipData),
}

/// What is on a clipboard: text, a picture (PNG) or files (name, content).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipData {
    Text(String),
    Image(Vec<u8>),
    Files(Vec<(String, Vec<u8>)>),
}

/// What a student computer's system is and which updates wait for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdatesInfo {
    /// "Windows 11 Education 24H2", "Ubuntu 24.04.1 LTS".
    pub os_name: String,
    /// The installed build ("26100.2033").
    pub os_build: String,
    pub items: Vec<UpdateItem>,
}

/// One pending update.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdateItem {
    pub id: String,
    pub title: String,
    /// A driver (else the system itself).
    pub driver: bool,
    /// Installed version / build.
    pub current: String,
    /// The version this update brings.
    pub new: String,
    pub size_mb: u32,
}

/// Pointer shapes carried by [`StudentMsg::Cursor`].
pub mod cursor_kind {
    pub const ARROW: u8 = 0;
    pub const TEXT: u8 = 1;
    pub const HAND: u8 = 2;
    pub const WAIT: u8 = 3;
    pub const SIZE_WE: u8 = 4;
    pub const SIZE_NS: u8 = 5;
    pub const SIZE_NWSE: u8 = 6;
    pub const SIZE_NESW: u8 = 7;
    pub const MOVE: u8 = 8;
    pub const CROSS: u8 = 9;
    pub const NO: u8 = 10;
    pub const PROGRESS: u8 = 11;
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    fn tag<T: Serialize>(v: &T) -> u32 {
        let b = bincode::serialize(v).unwrap();
        u32::from_le_bytes(b[..4].try_into().unwrap())
    }

    /// The wire format is positional: these indices are what every released build uses.
    /// New variants may only be appended.
    #[test]
    fn variant_indices_never_move() {
        assert_eq!(tag(&PrivCommand::ListApps), 7);
        assert_eq!(tag(&PrivCommand::SetAppUpdates(vec![])), 8);
        assert_eq!(tag(&PrivCommand::SetPersistentFolders(vec![])), 9);
        assert_eq!(
            tag(&PrivCommand::RenameComputer {
                name: String::new(),
                restart: false
            }),
            10
        );
        assert_eq!(tag(&PrivCommand::SetShadowPassword(String::new())), 11);
        assert_eq!(tag(&PrivCommand::CommitPaths(vec![])), 13);
        assert_eq!(tag(&PrivCommand::Unpair), 14);
        assert_eq!(tag(&PrivCommand::SetShieldEngine(0)), 15);
        assert_eq!(tag(&PrivCommand::SetDevicePolicy(Default::default())), 16);
        assert_eq!(tag(&PrivCommand::SetAccessPolicy(Default::default())), 17);
        assert_eq!(tag(&PrivCommand::RunScript(String::new())), 19);
        assert_eq!(tag(&PrivCommand::UninstallApp(String::new())), 20);
        assert_eq!(tag(&PrivCommand::UninstallVimora), 21);
        assert_eq!(tag(&PrivCommand::InstallUpdateIds(vec![])), 22);
        assert_eq!(tag(&PrivCommand::UpgradeApps(vec![])), 23);
        assert_eq!(tag(&StudentMsg::Updates(Default::default())), 20);
        assert_eq!(
            tag(&StudentMsg::Clipboard(ClipData::Text(String::new()))),
            21
        );
        assert_eq!(tag(&StudentMsg::SecurityInfo(Default::default())), 17);
        assert_eq!(
            tag(&StudentMsg::Free(StudentInfo {
                hostname: String::new(),
                username: String::new(),
                os: String::new(),
                display_name: None,
                machine_id: String::new(),
            })),
            18
        );
        assert_eq!(tag(&StudentMsg::RebootNeeded(false)), 19);
        let c = crate::net::encode_claim("pc-1", 47801);
        assert_eq!(
            crate::net::decode_claim(&c),
            Some(("pc-1".to_string(), 47801))
        );
        assert_eq!(tag(&Command::GetSecurityInfo), tag(&Command::ChatEnd) + 14);
        assert_eq!(
            tag(&Command::InstallUpdate {
                name: String::new(),
                data: vec![],
                sums: String::new(),
                sig: String::new()
            }),
            tag(&Command::ChatEnd) + 15
        );
        assert_eq!(
            tag(&Command::TeacherInfo {
                name: String::new()
            }),
            tag(&Command::ChatEnd) + 16
        );
        assert_eq!(
            tag(&Command::LockLook(LockLook::default())),
            tag(&Command::ChatEnd) + 17
        );
        assert_eq!(
            tag(&Command::SetAppAllowlist(vec![])),
            tag(&Command::ChatEnd) + 5
        );
        assert_eq!(
            tag(&Command::InputLock {
                keyboard: false,
                mouse: false
            }),
            tag(&Command::ChatEnd) + 6
        );
        assert_eq!(
            tag(&Command::OpenFile(String::new())),
            tag(&Command::ChatEnd) + 7
        );
        assert_eq!(tag(&Command::CollectFiles), tag(&Command::ChatEnd) + 8);
        assert_eq!(tag(&StudentMsg::Collected(vec![])), 14);
        assert_eq!(tag(&Command::EndTest), tag(&Command::ChatEnd) + 13);
        assert_eq!(
            tag(&StudentMsg::TestResult {
                id: 0,
                answers: vec![]
            }),
            16
        );
        assert_eq!(
            tag(&Command::LockStyle {
                icon: String::new()
            }),
            tag(&Command::ChatEnd) + 2
        );
        assert_eq!(
            tag(&StudentMsg::Chat {
                text: String::new()
            }),
            7
        );
        assert_eq!(tag(&StudentMsg::Pong), 8);
        assert_eq!(tag(&StudentMsg::Changes(vec![])), 9);
        assert_eq!(tag(&StudentMsg::AgentVersion(String::new())), 10);
        assert_eq!(tag(&StudentMsg::Cursor(0)), 11);
        assert_eq!(tag(&StudentMsg::ChatRead), 12);
        assert_eq!(tag(&StudentMsg::SystemInfo(Default::default())), 13);
        assert_eq!(tag(&Command::GetSystemInfo), tag(&Command::ChatEnd) + 3);
        assert_eq!(tag(&Command::KillProcess(0)), tag(&Command::ChatEnd) + 4);
        // Command: ChatEnd was the last one before the unlock code was added.
        assert_eq!(
            tag(&Command::UnlockCode {
                salt: vec![],
                hash: vec![],
                digits: 0
            }),
            tag(&Command::ChatEnd) + 1
        );
        assert_eq!(tag(&Command::ChatEnd), 13);
    }
}
