//! Teacher console UI.
//!
//! Layout: top navbar (logo · action strip · tools · status), room tabs row,
//! then a full-width grid of student cards. No sidebar.

mod forms;
mod grid;
mod model;
mod navbar;
mod overlay;
mod rooms;
mod selection;
mod settings;
mod viewer;

use crate::config::{self, split_list, ServerConfig, ViewMode};
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

use crate::net::{thumb_stream, Level, SharedConfig, SharedHub, Show};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Event, FontId, Frame, Key, Layout, Margin, Pos2,
    Rect, RichText, Rounding, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use std::sync::atomic::Ordering;

use std::sync::Arc;

use std::time::{Duration, Instant};

use vimora_core::{
    ClientStatus, Command, FreezeAction, FreezeState, InputEvent, MouseButton, NetPolicy,
    OsUpdateMode, PowerAction, PrivCommand, ServerMsg, SpecialKey, StreamConfig, UpdatePolicy,
};

use forms::*;
use model::*;
use selection::*;
use settings::*;
use vimora_ui::{self as ui_kit, icons, with_icon, Kind, Palette, ResponseExt, ThemeMode};

const DEFAULT_COLUMNS: u32 = 6;

/// 200%: three computers per row at most.
const MIN_COLUMNS: u32 = 3;

/// 100% (the default) is the smallest card size.
const MAX_COLUMNS: u32 = 6;

/// Card inner padding and the gap between cards.
const CARD_PAD: f32 = 6.0;

const GAP: f32 = 8.0;

pub struct ConsoleApp {
    hub: SharedHub,
    cfg: SharedConfig,
    show: Arc<Show>,
    updater: crate::update::Updater,
    theme: ThemeMode,
    /// Settings changed and not written to disk yet (saved shortly after the last edit).
    settings_dirty: Option<Instant>,
    /// Last automatic save (for the "saved" hint in the settings window).
    saved_at: Option<Instant>,
    selected: BTreeSet<u64>,
    search: String,
    /// Column filters of the list view.
    list_filter: ListFilter,
    room_tab: RoomTab,
    /// Rubber-band drag in progress.
    marquee: Option<Marquee>,
    /// Last clicked computer (Shift+click range start).
    anchor: Option<u64>,
    /// Previous click (for our own double-click detection).
    last_click: Option<(u64, Instant)>,
    /// Open right-click menu.
    ctx_menu: Option<CtxMenu>,
    /// egui frame number of the current frame.
    now_frame: u64,
    /// Computers in on-screen order this frame (for Shift ranges).
    visible_order: Vec<u64>,
    zoom_acc: f32,
    /// Computers to put into the room the "new room" dialog creates.
    pending_assign: Vec<String>,
    textures: HashMap<u64, (u64, TextureHandle)>,
    dialog: Option<Dialog>,
    viewer: Option<Viewer>,
    showing: bool,
    /// Dialog animating out after it was closed (drawn, not interactive).
    closing: Option<Dialog>,
    /// Dialog waiting for its blurred backdrop snapshot.
    pending: Option<(Dialog, Instant)>,
    want_snapshot: bool,
    backdrop: Option<TextureHandle>,
    dev_shot: bool,
    dev_open: Option<Dialog>,
    dev_priv_sent: bool,
    /// Dev aid: `VIMORA_SCREENSHOT=out.jpg` saves a screenshot of the console and exits.
    screenshot: Option<(String, Instant)>,
}

impl ConsoleApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        hub: SharedHub,
        cfg: SharedConfig,
        show: Arc<Show>,
        updater: crate::update::Updater,
    ) -> Self {
        updater.set_ctx(cc.egui_ctx.clone());
        let theme = {
            let c = cfg.read().unwrap();
            let system_light = cc.egui_ctx.system_theme() == Some(egui::Theme::Light);
            if (c.theme_system && !system_light) || (!c.theme_system && c.dark_mode) {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            }
        };
        ui_kit::install_fonts(&cc.egui_ctx);
        ui_kit::apply_theme(&cc.egui_ctx, theme);
        // We implement Ctrl +/-/0 and Ctrl+wheel ourselves (grid zoom, not UI scale).
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = Self {
            hub,
            cfg,
            show,
            updater,
            theme,
            settings_dirty: None,
            saved_at: None,
            selected: BTreeSet::new(),
            search: String::new(),
            list_filter: ListFilter::default(),
            room_tab: RoomTab::All,
            marquee: None,
            anchor: None,
            last_click: None,
            ctx_menu: None,
            now_frame: 0,
            visible_order: vec![],
            zoom_acc: 1.0,
            pending_assign: vec![],
            textures: HashMap::new(),
            dialog: None,
            viewer: None,
            showing: false,
            pending: None,
            closing: None,
            want_snapshot: false,
            backdrop: None,
            dev_shot: false,
            dev_open: None,
            dev_priv_sent: false,
            screenshot: std::env::var("VIMORA_SCREENSHOT")
                .ok()
                .map(|p| (p, Instant::now())),
        };
        if let Some(n) = std::env::var("VIMORA_DEMO")
            .ok()
            .and_then(|n| n.parse().ok())
        {
            app.demo_students(n);
        }
        // Dev aid: VIMORA_OPEN=net|activity|launch opens a window at start (for screenshots).
        match std::env::var("VIMORA_OPEN").as_deref() {
            Ok("net") => app.dev_open = Some(app.net_dialog()),
            Ok("activity") => app.dev_open = Some(Dialog::Activity),
            Ok("settings") => app.dev_open = Some(Dialog::Settings(SettingsTab::General)),
            Ok(s) if s.starts_with("settings:") => {
                let tab = match &s[9..] {
                    "appearance" => SettingsTab::Appearance,
                    "lock" => SettingsTab::LockScreen,
                    "students" => SettingsTab::Students,
                    "updates" => SettingsTab::Updates,
                    "about" => SettingsTab::About,
                    _ => SettingsTab::General,
                };
                app.dev_open = Some(Dialog::Settings(tab));
            }
            Ok("power") => app.dev_open = Some(Dialog::Power(PowerAction::Shutdown)),
            Ok("room") => {
                app.dev_open = Some(Dialog::Room {
                    name: String::new(),
                    rename: None,
                })
            }
            Ok("freeze") => app.dev_open = Some(app.shadow_dialog()),
            Ok("shadow-changes") => {
                let mut d = app.shadow_dialog();
                if let Dialog::Freeze { tab, .. } = &mut d {
                    *tab = ShadowTab::Changes;
                }
                app.dev_open = Some(d);
            }
            Ok("shadow-apps") => {
                let mut d = app.shadow_dialog();
                if let Dialog::Freeze { tab, request, .. } = &mut d {
                    *tab = ShadowTab::Protection;
                    *request = Some(ShadowRequest::ListApps);
                }
                app.dev_open = Some(d);
            }
            Ok("updates") => {
                let policy = app.cfg.read().unwrap().update_policy;
                app.dev_open = Some(Dialog::Updates {
                    policy,
                    os: true,
                    apps: true,
                    restart: false,
                    request: None,
                });
            }
            Ok("message") => {
                app.dev_open = Some(Dialog::Message {
                    text: String::new(),
                    send: false,
                })
            }
            Ok("launch") => {
                app.dev_open = Some(Dialog::Launch {
                    web: true,
                    url: "https://".into(),
                    program: String::new(),
                    args: String::new(),
                })
            }
            _ => {}
        }
        app
    }

    /// Dev aid: `VIMORA_DEMO=<n>` fills the console with fake computers (never saved to disk).
    fn demo_students(&mut self, n: usize) {
        let mut h = self.hub.lock().unwrap();
        let rooms = self.cfg.read().unwrap().rooms.clone();
        for i in 0..n {
            let (tx, _rx) = tokio::sync::mpsc::channel(1);
            let (w, hgt) = (160usize, 90usize);
            let mut px = Vec::with_capacity(w * hgt);
            for y in 0..hgt {
                for x in 0..w {
                    let t = (x + y) as f32 / (w + hgt) as f32;
                    let c = |a: f32, b: f32| (a + (b - a) * t) as u8;
                    px.push(Color32::from_rgb(
                        c(30.0 + 12.0 * (i % 5) as f32, 200.0),
                        c(60.0 + 10.0 * (i % 3) as f32, 220.0),
                        c(120.0, 245.0),
                    ));
                }
            }
            let mut status = ClientStatus {
                service: i % 7 != 3,
                locked: i % 6 == 2,
                ..Default::default()
            };
            if i % 5 == 1 {
                status.freeze = FreezeState::Frozen;
                status.shield_usage = Some(vimora_core::ShieldUsage {
                    used_mb: 300 + 40 * i as u32,
                    max_mb: 4096,
                });
            } else if i % 5 == 2 {
                status.freeze = FreezeState::FreezePending;
            } else if i % 5 == 3 {
                status.freeze = FreezeState::Unsupported;
                status.shield_note =
                    Some("UWF yo'q: Windows 10/11 Enterprise yoki Education kerak".into());
            } else {
                status.freeze = FreezeState::Thawed;
            }
            let demo_changes = (status.freeze == FreezeState::Frozen).then(|| {
                use vimora_core::{ChangeInfo, ChangeKind::*};
                [
                    ("/home/student/Desktop/dars-3.odt", Added, 48_200),
                    ("/home/student/Documents/hisobot.docx", Modified, 1_204_000),
                    ("/etc/hosts", Modified, 312),
                    ("/home/student/Pictures/eski-rasm.png", Deleted, 0),
                    ("/opt/geogebra/settings.xml", Modified, 2_900),
                ]
                .into_iter()
                .map(|(p, kind, size)| ChangeInfo {
                    path: p.into(),
                    kind,
                    size,
                })
                .collect::<Vec<_>>()
            });
            if i % 8 == 4 {
                status.web_blocklist = 3;
            }
            let id = 1000 + i as u64;
            h.students.insert(
                id,
                crate::net::Student {
                    addr: format!("192.168.1.{}:5000", 10 + i).parse().unwrap(),
                    info: vimora_core::ClientInfo {
                        hostname: format!("PC-{}", i + 1),
                        username: ["Ali", "Vali", "Madina", "Sardor"][i % 4].into(),
                        os: "Windows 11".into(),
                        display_name: None,
                        machine_id: format!("demo-{i}"),
                    },
                    status,
                    frame: Some(egui::ColorImage {
                        size: [w, hgt],
                        pixels: px,
                    }),
                    frame_seq: 1,
                    frame_size: (1920, 1080),
                    tx,
                    connected_at: Instant::now() - Duration::from_secs(600 + 97 * i as u64),
                    help: (i % 9 == 5).then(|| "Yordam".to_string()),
                    alert: (i % 11 == 7).then(|| ("Taqiqlangan so'z".to_string(), Instant::now())),
                    client_nonce: vec![],
                    priv_counter: 0,
                    apps: None,
                    changes: demo_changes,
                },
            );
            if let (Some(r), true) = (rooms.first(), i % 2 == 0) {
                self.cfg
                    .write()
                    .unwrap()
                    .assignments
                    .insert(format!("demo-{i}"), r.clone());
            }
        }
    }

    fn sync_textures(&mut self, ctx: &egui::Context) {
        let mut h = self.hub.lock().unwrap();
        self.textures.retain(|id, _| h.students.contains_key(id));
        for (id, s) in h.students.iter_mut() {
            let stale = self
                .textures
                .get(id)
                .is_none_or(|(seq, _)| *seq != s.frame_seq);
            if !stale {
                continue;
            }
            if let Some(img) = s.frame.take() {
                match self.textures.get_mut(id) {
                    Some((seq, tex)) => {
                        tex.set(img, TextureOptions::LINEAR);
                        *seq = s.frame_seq;
                    }
                    None => {
                        let tex =
                            ctx.load_texture(format!("student-{id}"), img, TextureOptions::LINEAR);
                        self.textures.insert(*id, (s.frame_seq, tex));
                    }
                }
            }
        }
    }

    fn snapshot(&self) -> Vec<View> {
        let (assignments, aliases, sort) = {
            let c = self.cfg.read().unwrap();
            (c.assignments.clone(), c.aliases.clone(), c.sort)
        };
        let h = self.hub.lock().unwrap();
        let mut list: Vec<_> = h.students.iter().collect();
        // Natural order by name: PC-2 before PC-10.
        let label = |s: &crate::net::Student| {
            aliases
                .get(&s.info.key())
                .cloned()
                .unwrap_or_else(|| s.info.label().to_string())
        };
        match sort {
            config::SortBy::Name => list.sort_by_key(|(_, s)| natural_key(&label(s))),
            config::SortBy::Ip => list.sort_by_key(|(_, s)| (s.addr.ip(), natural_key(&label(s)))),
        }
        list.into_iter()
            .enumerate()
            .map(|(i, (id, s))| View {
                id: *id,
                number: i + 1,
                key: s.info.key(),
                hostname: s.info.hostname.clone(),
                label: label(s),
                user: s.info.username.clone(),
                os: s.info.os.clone(),
                ip: s.addr.ip().to_string(),
                status: s.status.clone(),
                help: s.help.clone(),
                alert: s
                    .alert
                    .as_ref()
                    .filter(|(_, t)| t.elapsed() < Duration::from_secs(90))
                    .map(|(a, _)| a.clone()),
                room: assignments.get(&s.info.key()).cloned(),
                frame_size: s.frame_size,
                online_secs: s.connected_at.elapsed().as_secs(),
            })
            .collect()
    }

    /// Selected computers, or everyone in the current room tab when nothing is selected.
    fn targets(&self, all: &[View]) -> Vec<u64> {
        let sel: Vec<u64> = all
            .iter()
            .map(|v| v.id)
            .filter(|id| self.selected.contains(id))
            .collect();
        if sel.is_empty() {
            all.iter()
                .filter(|v| self.in_tab(v))
                .map(|v| v.id)
                .collect()
        } else {
            sel
        }
    }

    fn send(&self, ids: &[u64], cmd: Command, what: &str) {
        let mut h = self.hub.lock().unwrap();
        let n = h.send(ids, &ServerMsg::Command(cmd));
        if !what.is_empty() {
            h.log(
                Level::Info,
                tf!("{what} → {n} ta kompyuter", what = what, n = n),
            );
        }
    }

    /// Signed command executed by the privileged Vimora service on each PC.
    fn send_priv(&self, ids: &[u64], cmd: PrivCommand, what: &str) {
        let mut h = self.hub.lock().unwrap();
        let n = h.send_priv(ids, &cmd);
        if !what.is_empty() {
            h.log(
                Level::Info,
                tf!("{what} → {n} ta kompyuter", what = what, n = n),
            );
        }
    }

    fn lock(&self, ids: &[u64]) {
        let (title, message) = {
            let c = self.cfg.read().unwrap();
            (c.lock_title.clone(), c.lock_message.clone())
        };
        self.send(ids, Command::Lock { title, message }, tr("Qulflash"));
    }

    fn set_show(&mut self, on: bool, all: &[View]) {
        if on {
            let t = self.targets(all);
            *self.show.targets.lock().unwrap() = t.clone();
            self.send(&t, Command::ShowStart, tr("Namoyish boshlandi"));
            self.show.active.store(true, Ordering::Relaxed);
        } else {
            self.show.active.store(false, Ordering::Relaxed);
            let t = std::mem::take(&mut *self.show.targets.lock().unwrap());
            self.send(&t, Command::ShowStop, tr("Namoyish to'xtatildi"));
        }
        self.showing = on;
    }

    fn open_viewer(&mut self, id: u64, control: bool) {
        self.close_viewer();
        self.send(&[id], Command::SetStream(StreamConfig::LIVE), "");
        self.viewer = Some(Viewer {
            id,
            control,
            last_move: None,
        });
    }

    fn close_viewer(&mut self) {
        if let Some(v) = self.viewer.take() {
            let s = thumb_stream(&self.cfg.read().unwrap());
            self.send(&[v.id], Command::SetStream(s), "");
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, t: ThemeMode) {
        if self.theme != t {
            self.theme = t;
            ui_kit::apply_theme(ctx, t);
        }
    }

    /// The theme the settings ask for right now ("system" follows the OS).
    fn wanted_theme(&self, ctx: &egui::Context) -> ThemeMode {
        let c = self.cfg.read().unwrap();
        let dark = if c.theme_system {
            ctx.system_theme() != Some(egui::Theme::Light)
        } else {
            c.dark_mode
        };
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }

    /// Writes settings to disk shortly after the last change (they apply immediately anyway).
    fn autosave(&mut self) {
        if self
            .settings_dirty
            .is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        {
            self.settings_dirty = None;
            config::save(&self.cfg.read().unwrap());
            self.saved_at = Some(Instant::now());
        }
    }

    fn columns(&self) -> u32 {
        self.cfg
            .read()
            .unwrap()
            .grid_columns
            .clamp(MIN_COLUMNS, MAX_COLUMNS)
    }

    fn set_columns(&self, c: u32) {
        self.cfg.write().unwrap().grid_columns = c.clamp(MIN_COLUMNS, MAX_COLUMNS);
    }

    fn shadow_dialog(&self) -> Dialog {
        let c = self.cfg.read().unwrap();
        Dialog::Freeze {
            tab: ShadowTab::Protection,
            restart: false,
            folders: c.persistent_folders.clone(),
            wu: c.update_policy.os,
            app_changes: BTreeMap::new(),
            app_filter: String::new(),
            request: None,
            picked: BTreeSet::new(),
            change_filter: String::new(),
            manual: String::new(),
            password: c.shadow_password.clone(),
        }
    }

    fn net_dialog(&self) -> Dialog {
        let c = self.cfg.read().unwrap();
        Dialog::Net {
            tab: NetTab::Domains,
            domains: c.web_blocklist.clone(),
            keywords: c.keyword_blocklist.clone(),
            safe: c.safe_search,
        }
    }

    fn menu(&mut self, m: Menu, all: &[View]) {
        let t = self.targets(all);
        match m {
            Menu::Lock => {
                let all_locked = !t.is_empty()
                    && t.iter()
                        .all(|id| all.iter().any(|v| v.id == *id && v.status.locked));
                if all_locked {
                    self.send(&t, Command::Unlock, tr("Qulfdan chiqarish"));
                } else {
                    self.lock(&t);
                }
            }
            Menu::Show => {
                let on = !self.showing;
                self.set_show(on, all);
            }
            Menu::Message => {
                if matches!(self.dialog, Some(Dialog::Message { .. })) {
                    self.close_dialog();
                } else {
                    self.open_dialog(Dialog::Message {
                        text: String::new(),
                        send: false,
                    });
                }
            }
            Menu::Web | Menu::Program => self.open_dialog(Dialog::Launch {
                web: m == Menu::Web,
                url: "https://".into(),
                program: String::new(),
                args: String::new(),
            }),
            Menu::File => self.open_dialog(Dialog::File(String::new())),
            Menu::Net => self.open_dialog(self.net_dialog()),
            Menu::Apps => {
                let list = self.cfg.read().unwrap().app_blocklist.clone();
                self.open_dialog(Dialog::Apps(list));
            }
            Menu::Power => self.open_dialog(Dialog::Power(PowerAction::Shutdown)),
            Menu::Freeze => self.open_dialog(self.shadow_dialog()),
            Menu::Updates => {
                let policy = self.cfg.read().unwrap().update_policy;
                self.open_dialog(Dialog::Updates {
                    policy,
                    os: true,
                    apps: true,
                    restart: false,
                    request: None,
                });
            }
            Menu::Activity => {
                if matches!(self.dialog, Some(Dialog::Activity)) {
                    self.close_dialog();
                } else {
                    self.open_dialog(Dialog::Activity);
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, all: &[View]) {
        if self.dialog.is_some()
            || self.pending.is_some()
            || self.viewer.as_ref().is_some_and(|v| v.control)
        {
            return;
        }
        use egui::Modifiers as M;
        let (a, l, u, zin, zout, z0) = ctx.input_mut(|i| {
            (
                i.consume_key(M::COMMAND, Key::A),
                i.consume_key(M::COMMAND, Key::L),
                i.consume_key(M::COMMAND, Key::U),
                i.consume_key(M::COMMAND, Key::Plus) || i.consume_key(M::COMMAND, Key::Equals),
                i.consume_key(M::COMMAND, Key::Minus),
                i.consume_key(M::COMMAND, Key::Num0),
            )
        });
        if a {
            self.selected = all.iter().map(|v| v.id).collect();
        }
        if l {
            let t = self.targets(all);
            self.lock(&t);
        }
        if u {
            let t = self.targets(all);
            self.send(&t, Command::Unlock, tr("Qulfdan chiqarish"));
        }
        if zin {
            self.set_columns(self.columns() - 1);
        }
        if zout {
            self.set_columns(self.columns() + 1);
        }
        if z0 {
            self.set_columns(DEFAULT_COLUMNS);
        }
    }

    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        if self
            .screenshot
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_millis(2500))
        {
            if let Some(d) = self.dev_open.take() {
                self.open_dialog(d);
            }
        }
        let Some((_, started)) = &mut self.screenshot else {
            return;
        };
        let delay = std::env::var("VIMORA_SCREENSHOT_DELAY")
            .ok()
            .and_then(|d| d.parse().ok())
            .unwrap_or(5);
        if !self.dev_shot && started.elapsed() > Duration::from_secs(delay) {
            self.dev_shot = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.now_frame += 1;
        self.snapshots(ctx);
        let want = self.wanted_theme(ctx);
        self.set_theme(ctx, want);
        if self.updater.take_exit() {
            // Windows: the installer is replacing the console.
            config::save(&self.cfg.read().unwrap());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.sync_textures(ctx);
        let all = self.snapshot();
        if self.showing && !self.show.active.load(Ordering::Relaxed) {
            self.set_show(false, &all); // capture failed: tell students to close the presentation
        }
        self.selected.retain(|id| all.iter().any(|v| v.id == *id));
        let p = self.theme.palette();

        self.shortcuts(ctx, &all);
        self.navbar(ctx, &p, &all);
        // Dev aid: VIMORA_OPEN=viewer watches the first computer 3 s after start.
        if std::env::var("VIMORA_OPEN").as_deref() == Ok("viewer")
            && !self.dev_priv_sent
            && ctx.input(|i| i.time) > 3.0
        {
            if let Some(first) = all.first() {
                self.dev_priv_sent = true;
                self.visible_order = all.iter().map(|v| v.id).collect();
                self.open_viewer(first.id, true);
            }
        }
        if self.viewer.is_some() {
            self.viewer_page(ctx, &p, &all);
        } else {
            self.grid(ctx, &p, &all);
        }
        // Dev aid: VIMORA_MENU=1 opens the context menu of the second computer 3 s after start.
        if std::env::var("VIMORA_MENU").is_ok()
            && !self.dev_priv_sent
            && ctx.input(|i| i.time) > 3.0
        {
            if let Some(v) = all.get(1) {
                self.dev_priv_sent = true;
                self.ctx_menu = Some(CtxMenu {
                    target: CtxTarget::Computer(v.id),
                    anchor: Rect::from_min_size(pos2(194.0, 110.0), vec2(172.0, 150.0)),
                    x: 280.0,
                    frame: self.now_frame,
                    rooms_open: true,
                });
            }
        }
        self.context_menu(ctx, &p, &all);
        self.dialogs(ctx, &p, &all);
        self.toasts(ctx, &p);
        self.autosave();
        self.dev_screenshot(ctx);
        // Dev aid: VIMORA_DEV_PRIV=check sends a signed CheckUpdates to everyone once, 4 s after start.
        // Dev aid: VIMORA_DEV_PRIV=chat sends one chat message 4 s after start.
        if std::env::var("VIMORA_DEV_PRIV").as_deref() == Ok("chat")
            && !self.dev_priv_sent
            && ctx.input(|i| i.time) > 4.0
        {
            if let Some(Dialog::Message { text, send }) = &mut self.dialog {
                self.dev_priv_sent = true;
                *text = tr("Salom! Bugun 3-mavzuni boshlaymiz, kitobni oching.").into();
                *send = true;
            }
        }
        if std::env::var("VIMORA_DEV_PRIV").as_deref() == Ok("check")
            && !self.dev_priv_sent
            && ctx.input(|i| i.time) > 4.0
        {
            self.dev_priv_sent = true;
            let ids: Vec<u64> = all.iter().map(|v| v.id).collect();
            self.send_priv(
                &ids,
                PrivCommand::CheckUpdates,
                tr("DEV: yangilanishlarni tekshirish"),
            );
        }
        if std::mem::take(&mut self.want_snapshot) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }

        // Keep "online for" counters fresh even without network activity.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if std::env::var("VIMORA_DEMO").is_ok() {
            return;
        }
        config::save(&self.cfg.read().unwrap());
    }
}
