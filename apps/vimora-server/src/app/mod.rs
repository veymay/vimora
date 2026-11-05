//! Teacher console UI.
//!
//! Layout: one header line (logo · icon menu · count/theme/settings), a room bar
//! (room tabs · search · zoom), then a hairline grid of student cells. Activity, NetControl and the
//! other tools open as windows from the menu.

mod forms;
mod grid;
mod header;
mod model;
mod overlay;
mod rooms;
mod selection;
mod viewer;

use crate::config::{self, split_list, ServerConfig, ViewMode};

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
use vimora_ui::{self as ui_kit, icons, with_icon, Kind, Palette, ThemeMode};

const HEADER_H: f32 = 64.0;

const DEFAULT_COLUMNS: u32 = 6;

const MIN_COLUMNS: u32 = 2;

const MAX_COLUMNS: u32 = 12;

const STRIP_H: f32 = 36.0;

const GAP: f32 = 14.0;

pub struct ConsoleApp {
    hub: SharedHub,
    cfg: SharedConfig,
    show: Arc<Show>,
    theme: ThemeMode,
    selected: BTreeSet<u64>,
    search: String,
    room_tab: RoomTab,
    /// Rubber-band drag in progress.
    marquee: Option<Marquee>,
    /// Last clicked computer (Shift+click range start).
    anchor: Option<u64>,
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
    ) -> Self {
        let theme = if cfg.read().unwrap().dark_mode {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        ui_kit::install_fonts(&cc.egui_ctx);
        ui_kit::apply_theme(&cc.egui_ctx, theme);
        // We implement Ctrl +/-/0 and Ctrl+wheel ourselves (grid zoom, not UI scale).
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = Self {
            hub,
            cfg,
            show,
            theme,
            selected: BTreeSet::new(),
            search: String::new(),
            room_tab: RoomTab::All,
            marquee: None,
            anchor: None,
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
        // Dev aid: VIMORA_OPEN=net|activity|launch opens a window at start (for screenshots).
        match std::env::var("VIMORA_OPEN").as_deref() {
            Ok("net") => app.dev_open = Some(app.net_dialog()),
            Ok("activity") => app.dev_open = Some(Dialog::Activity),
            Ok("settings") => {
                let c = app.cfg.read().unwrap().clone();
                app.dev_open = Some(Dialog::Settings(Box::new(c)));
            }
            Ok("room") => {
                app.dev_open = Some(Dialog::Room {
                    name: String::new(),
                    rename: None,
                })
            }
            Ok("freeze") => app.dev_open = Some(app.shadow_dialog()),
            Ok("shadow-apps") => {
                let mut d = app.shadow_dialog();
                if let Dialog::Freeze { tab, request, .. } = &mut d {
                    *tab = ShadowTab::Apps;
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
        let (assignments, aliases) = {
            let c = self.cfg.read().unwrap();
            (c.assignments.clone(), c.aliases.clone())
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
        list.sort_by_key(|(_, s)| natural_key(&label(s)));
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
            h.log(Level::Info, format!("{what} → {n} ta kompyuter"));
        }
    }

    /// Signed command executed by the privileged Vimora service on each PC.
    fn send_priv(&self, ids: &[u64], cmd: PrivCommand, what: &str) {
        let key = self.cfg.read().unwrap().key.clone();
        let mut h = self.hub.lock().unwrap();
        let n = h.send_priv(ids, &cmd, &key);
        if !what.is_empty() {
            h.log(Level::Info, format!("{what} → {n} ta kompyuter"));
        }
    }

    fn lock(&self, ids: &[u64]) {
        let (title, message) = {
            let c = self.cfg.read().unwrap();
            (c.lock_title.clone(), c.lock_message.clone())
        };
        self.send(ids, Command::Lock { title, message }, "Qulflash");
    }

    fn set_show(&mut self, on: bool, all: &[View]) {
        if on {
            let t = self.targets(all);
            *self.show.targets.lock().unwrap() = t.clone();
            self.send(&t, Command::ShowStart, "Namoyish boshlandi");
            self.show.active.store(true, Ordering::Relaxed);
        } else {
            self.show.active.store(false, Ordering::Relaxed);
            let t = std::mem::take(&mut *self.show.targets.lock().unwrap());
            self.send(&t, Command::ShowStop, "Namoyish to'xtatildi");
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
        self.theme = t;
        ui_kit::apply_theme(ctx, t);
        let mut c = self.cfg.write().unwrap();
        c.dark_mode = t == ThemeMode::Dark;
        config::save(&c);
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
            action: FreezeAction::Freeze,
            restart: true,
            folders: c.persistent_folders.clone(),
            wu: c.update_policy.os,
            app_changes: BTreeMap::new(),
            app_filter: String::new(),
            request: None,
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
            Menu::Lock => self.lock(&t),
            Menu::Unlock => self.send(&t, Command::Unlock, "Qulfdan chiqarish"),
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
            Menu::Launch => self.open_dialog(Dialog::Launch {
                web: true,
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
            self.send(&t, Command::Unlock, "Qulfdan chiqarish");
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
        self.snapshots(ctx);
        self.sync_textures(ctx);
        let all = self.snapshot();
        if self.showing && !self.show.active.load(Ordering::Relaxed) {
            self.set_show(false, &all); // capture failed: tell students to close the presentation
        }
        self.selected.retain(|id| all.iter().any(|v| v.id == *id));
        let p = self.theme.palette();

        self.shortcuts(ctx, &all);
        self.header(ctx, &p, &all);
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
        self.dialogs(ctx, &p, &all);
        self.dev_screenshot(ctx);
        // Dev aid: VIMORA_DEV_PRIV=check sends a signed CheckUpdates to everyone once, 4 s after start.
        // Dev aid: VIMORA_DEV_PRIV=chat sends one chat message 4 s after start.
        if std::env::var("VIMORA_DEV_PRIV").as_deref() == Ok("chat")
            && !self.dev_priv_sent
            && ctx.input(|i| i.time) > 4.0
        {
            if let Some(Dialog::Message { text, send }) = &mut self.dialog {
                self.dev_priv_sent = true;
                *text = "Salom! Bugun 3-mavzuni boshlaymiz, kitobni oching.".into();
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
                "DEV: yangilanishlarni tekshirish",
            );
        }
        if std::mem::take(&mut self.want_snapshot) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }

        // Keep "online for" counters fresh even without network activity.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        config::save(&self.cfg.read().unwrap());
    }
}
