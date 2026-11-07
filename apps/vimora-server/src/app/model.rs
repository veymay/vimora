//! UI-side data: per-frame student snapshots, dialogs, actions and small formatting helpers.

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

/// Snapshot of a student for one UI frame (so the hub lock isn't held while drawing).
pub(super) struct View {
    pub(super) id: u64,
    pub(super) number: usize,
    /// Stable identity (hardware id; rooms and aliases are keyed by it).
    pub(super) key: String,
    /// System computer name.
    pub(super) hostname: String,
    /// Shown name: the teacher's alias, else the agent's display name / hostname.
    pub(super) label: String,
    pub(super) user: String,
    pub(super) os: String,
    pub(super) ip: String,
    pub(super) status: ClientStatus,
    pub(super) help: Option<String>,
    pub(super) alert: Option<String>,
    pub(super) room: Option<String>,
    pub(super) frame_size: (u32, u32),
    pub(super) online_secs: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum RoomTab {
    All,
    Unassigned,
    Room(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NetTab {
    Domains,
    Keywords,
    Search,
}

#[derive(Clone)]
pub(super) enum Dialog {
    /// Telegram-style chat drawer.
    Message {
        text: String,
        send: bool,
    },
    Launch {
        web: bool,
        url: String,
        program: String,
        args: String,
    },
    File(String),
    Apps(String),
    Net {
        tab: NetTab,
        domains: String,
        keywords: String,
        safe: bool,
    },
    Power(PowerAction),
    Settings(SettingsTab),
    /// Create a room (`rename: None`) or rename one.
    Room {
        name: String,
        rename: Option<String>,
    },
    /// Event log (drawer, no footer).
    Activity,
    Rename {
        id: u64,
        key: String,
        alias: String,
        system: bool,
        system_name: String,
        restart: bool,
    },
    Freeze {
        tab: ShadowTab,
        /// Computers the drawer acts on (ticked in its list).
        chosen: std::collections::BTreeSet<u64>,
        restart: bool,
        /// Persistent folders, one per line.
        folders: String,
        request: Option<ShadowRequest>,
        /// Paths ticked for commit.
        picked: std::collections::BTreeSet<String>,
        /// A path typed by hand (platforms that cannot list the overlay).
        manual: String,
        /// Shadow Defender password for the Windows student PCs.
        password: String,
    },
    Updates {
        policy: UpdatePolicy,
        os: bool,
        apps: bool,
        restart: bool,
        request: Option<UpdateRequest>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UpdateRequest {
    Check,
    Install,
}

pub(super) struct Viewer {
    pub(super) id: u64,
    pub(super) control: bool,
    pub(super) last_move: Option<(f32, f32)>,
}

#[derive(Clone)]
pub(super) enum Action {
    Assign(u64, Option<String>),
    NewRoomFor(u64),
    /// Click on an item (Explorer semantics: plain / Ctrl / Shift).
    Click(u64, egui::Modifiers),
    View(u64, bool),
    Lock(u64),
    Unlock(u64),
    Message(u64),
    ClearHelp(u64),
    Rename(u64),
    /// Right click: open the context menu for this computer (anchor rect, pointer x).
    OpenMenu(u64, Rect, f32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShadowTab {
    Protection,
    Commit,
    Settings,
}

/// Inline actions inside the Shadow drawer (they don't close it).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShadowRequest {
    /// Turn Shadow on / off / run the update cycle on the targets.
    Apply(FreezeAction),
    ListChanges,
    /// Commit the ticked paths.
    Commit,
    SaveFolders,
}

/// Entries kept for actions that have no navbar button right now (program launch, file send).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Menu {
    /// Locks the targets, or unlocks them when they are all locked already.
    Lock,
    Show,
    Message,
    Web,
    Program,
    File,
    Net,
    Apps,
    Freeze,
    Updates,
    Power,
    Activity,
}

pub(super) fn natural_key(s: &str) -> Vec<(String, u64)> {
    let mut out = vec![];
    let mut text = String::new();
    let mut num = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            if !num.is_empty() {
                out.push((std::mem::take(&mut text), num.parse().unwrap_or(0)));
                num.clear();
            }
            text.push(c);
        }
    }
    out.push((text, num.parse().unwrap_or(0)));
    out
}

pub(super) fn card_height(w: f32) -> f32 {
    // thumbnail + name line + badge line; every card gets exactly this height
    let tw = w - CARD_PAD * 2.0;
    CARD_PAD + (tw * 9.0 / 16.0).round() + 6.0 + 18.0 + 4.0 + 18.0 + CARD_PAD
}

/// 6 per row = 100%, 3 per row = 200%.
pub(super) fn zoom_percent(cols: u32) -> u32 {
    (DEFAULT_COLUMNS as f32 / cols as f32 * 100.0).round() as u32
}

pub(super) fn freeze_label(f: FreezeState) -> &'static str {
    match f {
        FreezeState::Unknown => tr("noma'lum"),
        FreezeState::Unsupported => tr("qo'llanmaydi"),
        FreezeState::Thawed => tr("o'chiq"),
        FreezeState::Frozen => tr("yoqilgan"),
        FreezeState::FreezePending => tr("qayta yuklanganda yoqiladi"),
        FreezeState::ThawPending => tr("qayta yuklanganda o'chadi"),
        FreezeState::Maintenance => tr("xizmat rejimi"),
    }
}

pub(super) fn freeze_color(p: &Palette, f: FreezeState) -> Color32 {
    match f {
        FreezeState::Frozen => p.accent_ink,
        FreezeState::Thawed => p.muted,
        FreezeState::Unsupported | FreezeState::Unknown => p.faint,
        _ => p.warning,
    }
}

pub(super) fn updates_label(s: &ClientStatus) -> String {
    if let Some(b) = &s.busy {
        return b.clone();
    }
    match (s.os_updates, s.app_updates) {
        (None, None) => "tekshirilmagan".into(),
        (o, a) => tf!(
            "tizim {v0}, dasturlar {v1}",
            v0 = o.map_or("?".into(), |n| n.to_string()),
            v1 = a.map_or("?".into(), |n| n.to_string())
        ),
    }
}

pub(super) fn fmt_dur(s: u64) -> String {
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        tf!("{v0} daq", v0 = s / 60)
    } else {
        tf!("{v0} soat {v1} daq", v0 = s / 3600, v1 = (s % 3600) / 60)
    }
}

/// Columns of the list view.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Col {
    Num,
    Thumb,
    Name,
    Status,
    Shadow,
    Flags,
    Room,
    User,
    Ip,
    Online,
}

impl Col {
    pub(super) fn title(self) -> &'static str {
        match self {
            Col::Num => "#",
            Col::Thumb => "",
            Col::Name => tr("Kompyuter"),
            Col::Status => tr("Holat"),
            Col::Shadow => tr("Shadow"),
            Col::Flags => tr("Belgilar"),
            Col::Room => tr("Xona"),
            Col::User => tr("Foydalanuvchi"),
            Col::Ip => tr("IP"),
            Col::Online => tr("Ulangan"),
        }
    }

    /// Columns whose header opens a filter.
    pub(super) fn filterable(self) -> bool {
        matches!(
            self,
            Col::Name | Col::Status | Col::Shadow | Col::Room | Col::User | Col::Ip
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum StatusFilter {
    #[default]
    Any,
    Locked,
    Attention,
}

#[derive(Clone, PartialEq, Eq, Default)]
pub(super) enum RoomFilter {
    #[default]
    Any,
    Unassigned,
    Is(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ShadowFilter {
    #[default]
    Any,
    On,
    Off,
}

/// Column filters of the list view (all conditions must match).
#[derive(Clone, PartialEq, Eq, Default)]
pub(super) struct ListFilter {
    pub(super) name: String,
    pub(super) user: String,
    pub(super) ip: String,
    pub(super) room: RoomFilter,
    pub(super) status: StatusFilter,
    pub(super) shadow: ShadowFilter,
}

impl ListFilter {
    pub(super) fn active(&self, col: Col) -> bool {
        match col {
            Col::Name => !self.name.is_empty(),
            Col::User => !self.user.is_empty(),
            Col::Ip => !self.ip.is_empty(),
            Col::Room => self.room != RoomFilter::Any,
            Col::Status => self.status != StatusFilter::Any,
            Col::Shadow => self.shadow != ShadowFilter::Any,
            _ => false,
        }
    }

    pub(super) fn any(&self) -> bool {
        [
            Col::Name,
            Col::User,
            Col::Ip,
            Col::Room,
            Col::Status,
            Col::Shadow,
        ]
        .into_iter()
        .any(|c| self.active(c))
    }

    pub(super) fn matches(&self, v: &View) -> bool {
        let has = |hay: &str, needle: &str| {
            needle.is_empty() || hay.to_lowercase().contains(&needle.to_lowercase())
        };
        has(&v.label, &self.name)
            && has(&v.user, &self.user)
            && has(&v.ip, &self.ip)
            && match &self.room {
                RoomFilter::Any => true,
                RoomFilter::Unassigned => v.room.is_none(),
                RoomFilter::Is(r) => v.room.as_deref() == Some(r.as_str()),
            }
            && match self.status {
                StatusFilter::Any => true,
                StatusFilter::Locked => v.status.locked,
                StatusFilter::Attention => v.help.is_some() || v.alert.is_some(),
            }
            && {
                let on = !matches!(
                    v.status.freeze,
                    FreezeState::Unknown | FreezeState::Unsupported | FreezeState::Thawed
                );
                match self.shadow {
                    ShadowFilter::Any => true,
                    ShadowFilter::On => on,
                    ShadowFilter::Off => !on,
                }
            }
    }
}

/// What a context menu belongs to.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum CtxTarget {
    Computer(u64),
    Room(String),
}

/// The open context menu (drawn as an arrow popup next to its anchor).
#[derive(Clone)]
pub(super) struct CtxMenu {
    pub(super) target: CtxTarget,
    pub(super) anchor: Rect,
    pub(super) x: f32,
    /// Frame it opened in (the click that opened it must not close it).
    pub(super) frame: u64,
    /// "Xonaga biriktirish" list expanded.
    pub(super) rooms_open: bool,
}
