//! `vimora-student about`: the small window of the tray icon (a click or a double click).
//!
//! One window: the logo, the name, the teacher computer this one is connected to, the version
//! and the update state (one state for the whole system: whoever started an update — the
//! teacher, the service by itself, this window's button — the same progress shows here), and a
//! bar at the bottom with "Chiqish" and "Yangilanishni tekshirish".
//!
//! The agent passes what it knows in the environment: `VIMORA_TEACHER` (the teacher computer's
//! name) and `VIMORA_CONNECTED` (1 / 0).

use crate::service::{IpcReq, IpcResp};
use crate::smallwin;
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect};
use std::sync::{Arc, Mutex};
use vimora_ui::{icons, Kind, Progress};

const SIZE: [f32; 2] = [400.0, 440.0];

fn t(lang: &str, key: &str) -> &'static str {
    match (key, lang) {
        ("connected", "ru") => "подключён",
        ("connected", "en") => "connected",
        ("connected", _) => "ulangan",
        ("waiting", "ru") => "ожидание",
        ("waiting", "en") => "waiting",
        ("waiting", _) => "kutilmoqda",
        ("none", "ru") => "Учитель ещё не подключён",
        ("none", "en") => "No teacher connected yet",
        ("none", _) => "O'qituvchi hali ulanmagan",
        ("check", "ru") => "Обновления",
        ("check", "en") => "Updates",
        ("check", _) => "Yangilanishni tekshirish",
        ("checking", "ru") => "Проверка…",
        ("checking", "en") => "Checking…",
        ("checking", _) => "Tekshirilmoqda…",
        ("installing", "ru") => "Устанавливается…",
        ("installing", "en") => "Installing…",
        ("installing", _) => "O'rnatilmoqda…",
        ("latest", "ru") => "Последняя версия",
        ("latest", "en") => "Up to date",
        ("latest", _) => "Eng so'nggi versiya",
        ("installed", "ru") => "Обновлено",
        ("installed", "en") => "Updated",
        ("installed", _) => "Yangilandi",
        ("noservice", "ru") => "Служба VIMORA не отвечает",
        ("noservice", "en") => "The VIMORA service does not answer",
        ("noservice", _) => "VIMORA xizmati javob bermayapti",
        ("exit", "ru") => "Выход",
        ("exit", "en") => "Exit",
        ("exit", _) => "Chiqish",
        _ => "",
    }
}

/// What the service says about the Vimora update, refreshed every second.
#[derive(Clone, Default)]
struct Shared {
    /// Running now ("VIMORA 0.3.32 yuklanmoqda 45%", "… o'rnatilmoqda").
    busy: Option<String>,
    /// How the last attempt ended.
    last: Option<(bool, String)>,
    /// The service did not answer the button.
    no_service: bool,
    /// Another version is installed than the one this window runs from.
    installed: Option<String>,
    /// Why the service cannot be reached right now (shown in red; nothing when it answers).
    service_err: Option<String>,
}

struct About {
    /// When the update state last changed (a result stays on the screen for a few seconds).
    changed: Option<std::time::Instant>,
    /// When installing began (the window closes itself shortly after).
    closing_at: Option<std::time::Instant>,
    seen: (bool, Option<(bool, String)>, bool),
    lang: &'static str,
    teacher: Option<String>,
    connected: bool,
    shared: Arc<Mutex<Shared>>,
}

pub fn run() -> anyhow::Result<()> {
    let lang = crate::layout::language();
    let teacher = std::env::var("VIMORA_TEACHER")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let connected = std::env::var("VIMORA_CONNECTED").as_deref() == Ok("1");
    eframe::run_native(
        "vimora-about",
        smallwin::options("vimora-about", SIZE, None),
        Box::new(move |cc| {
            smallwin::style(&cc.egui_ctx);
            let shared: Arc<Mutex<Shared>> = Arc::default();
            watch(shared.clone(), cc.egui_ctx.clone());
            Ok(Box::new(About {
                changed: None,
                closing_at: None,
                seen: (false, None, false),
                lang,
                teacher,
                connected,
                shared,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("about: {e}"))
}

/// Follows the service's update state (and, now and then, which version is installed).
fn watch(shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        let mut round = 0u32;
        loop {
            match rt.block_on(crate::service::call(IpcReq::UpdateState)) {
                Ok(IpcResp::UpdateState(busy, last)) => {
                    let mut s = shared.lock().unwrap();
                    if s.busy != busy || s.last != last || s.service_err.is_some() {
                        s.busy = busy;
                        s.last = last;
                        s.service_err = None;
                        ctx.request_repaint();
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let mut s = shared.lock().unwrap();
                    let text = format!("{e:#}");
                    if s.service_err.as_deref() != Some(text.as_str()) {
                        s.service_err = Some(text);
                        ctx.request_repaint();
                    }
                }
            }
            if round % 5 == 4 {
                let v = crate::selfcheck::replaced();
                let mut s = shared.lock().unwrap();
                if s.installed != v {
                    s.installed = v;
                    ctx.request_repaint();
                }
            }
            round = round.wrapping_add(1);
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    });
}

impl eframe::App for About {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        smallwin::screenshot(ctx);
        if smallwin::title_bar(ctx, "VIMORA") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let p = vimora_ui::palette(ctx);
        let l = self.lang;
        let state = self.shared.lock().unwrap().clone();
        let installed = state
            .installed
            .as_ref()
            .map(|v| format!("{}: {v}", t(l, "installed")))
            .unwrap_or_default();
        let running = matches!(
            self.progress_of(&state, &installed),
            Progress::Waiting(_) | Progress::Percent(_)
        );

        // While the new version goes in (and once it is in) this window is not kept open: the
        // old program must be free to be replaced, the new one starts clean.
        let installing = state
            .busy
            .as_deref()
            .is_some_and(|b| b.contains("o'rnatilmoqda"));
        if installing || state.installed.is_some() {
            let since = *self.closing_at.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() > std::time::Duration::from_millis(1500) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                std::process::exit(0);
            }
        } else {
            self.closing_at = None;
        }

        // A result (done / failed) is shown for a few seconds, then the bar goes away.
        let key = (
            state.busy.is_some(),
            state.last.clone(),
            state.installed.is_some(),
        );
        if key != self.seen {
            // Only a run that happened while this window was open (or is running) is news;
            // an old result left by the service is not shown again.
            if key.0 || self.seen.0 || key.2 {
                self.changed = Some(std::time::Instant::now());
            }
            self.seen = key;
        }
        let show_state = running
            || self
                .changed
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(8));
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg))
            .show(ctx, |ui| {
                self.home(ui, ctx, &state, &installed, running, show_state)
            });
    }
}

impl About {
    fn start_check(&self, ctx: &egui::Context) {
        let (shared, ctx) = (self.shared.clone(), ctx.clone());
        shared.lock().unwrap().busy = Some("…".into());
        std::thread::spawn(move || {
            let ok = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|rt| rt.block_on(crate::service::call(IpcReq::UpdateNow)))
                .is_ok();
            let mut s = shared.lock().unwrap();
            s.no_service = !ok;
            if !ok {
                s.busy = None;
            }
            ctx.request_repaint();
        });
    }

    /// The update state as the whole system sees it (one state, whoever started it).
    fn progress_of<'a>(&self, state: &'a Shared, installed: &'a str) -> Progress<'a> {
        let l = self.lang;
        let quiet = vimora_ui::DARK.muted;
        if state.installed.is_some() {
            return Progress::Done(installed);
        }
        if let Some(b) = state.busy.as_deref() {
            return match percent_of(b) {
                Some(pct) => Progress::Percent(pct),
                None if b.contains("o'rnatilmoqda") => Progress::Waiting(t(l, "installing")),
                None => Progress::Waiting(t(l, "checking")),
            };
        }
        if state.no_service {
            return Progress::Failed(t(l, "noservice"));
        }
        match &state.last {
            Some((true, m)) if m.contains("o'rnatilmoqda") => Progress::Idle("", quiet),
            Some((true, _)) => Progress::Done(t(l, "latest")),
            Some((false, m)) => Progress::Failed(m),
            None => Progress::Idle("", quiet),
        }
    }

    /// The window: the mark, the name, the teacher, the version, the update state.
    fn home(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        state: &Shared,
        installed: &str,
        running: bool,
        show_state: bool,
    ) {
        let p = vimora_ui::palette(ui.ctx());
        let l = self.lang;
        let r = ui.max_rect();
        let cx = r.center().x;
        // The whole block sits in the middle of the area (the same room above and below).
        const BLOCK_H: f32 = 340.0;
        let mut y = r.top() + ((r.height() - BLOCK_H) / 2.0).max(8.0);
        // Logo, then the name under it (the brand face, capitals).
        let logo = Rect::from_center_size(pos2(cx, y + 44.0), vec2(88.0, 88.0));
        vimora_ui::logo(ui.painter(), logo);
        y = logo.bottom() + 12.0;
        ui.painter().text(
            pos2(cx, y + 14.0),
            Align2::CENTER_CENTER,
            "VIMORA",
            FontId::new(28.0, vimora_ui::brand()),
            p.strong,
        );
        y += 40.0;
        // The teacher this computer is connected to: the name, and under it the state.
        let (dot, word) = if self.connected {
            (p.success, t(l, "connected"))
        } else {
            (p.faint, t(l, "waiting"))
        };
        let (name, ink) = match &self.teacher {
            Some(n) => (n.clone(), p.strong),
            None => (t(l, "none").to_string(), p.muted),
        };
        let g = vimora_ui::truncated(
            ui,
            &name,
            FontId::new(15.0, vimora_ui::bold()),
            ink,
            r.width() - 100.0,
        );
        let total = 26.0 + g.size().x;
        let x0 = cx - total / 2.0;
        ui.painter().text(
            pos2(x0 + 9.0, y + 11.0),
            Align2::CENTER_CENTER,
            icons::GRADUATION_CAP,
            FontId::proportional(18.0),
            if self.connected { p.accent } else { p.faint },
        );
        ui.painter()
            .galley(pos2(x0 + 26.0, y + 11.0 - g.size().y / 2.0), g, ink);
        y += 28.0;
        if self.teacher.is_some() {
            let wg =
                ui.painter()
                    .layout_no_wrap(word.to_string(), FontId::proportional(12.5), p.muted);
            let wx = cx - (14.0 + wg.size().x) / 2.0;
            ui.painter()
                .circle_filled(pos2(wx + 3.5, y + 8.0), 3.5, dot);
            ui.painter()
                .galley(pos2(wx + 14.0, y + 8.0 - wg.size().y / 2.0), wg, p.muted);
        }
        y += 34.0;
        // The version.
        ui.painter().text(
            pos2(cx, y + 8.0),
            Align2::CENTER_CENTER,
            format!("v{}", env!("CARGO_PKG_VERSION")),
            FontId::new(13.0, vimora_ui::mono()),
            p.accent_ink,
        );
        y += 36.0;
        // Under the version: the update check, and what it is doing (checking, downloading with
        // a percent, installing, done).
        let label = t(l, "check");
        let bw = 250.0;
        let btn = Rect::from_min_size(pos2(cx - bw / 2.0, y), vec2(bw, vimora_ui::CONTROL_H));
        let mut b = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(btn)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        b.add_enabled_ui(!running, |ui| {
            if vimora_ui::button(ui, Some(icons::REFRESH_CW), label, Kind::Primary).clicked() {
                self.start_check(ctx);
            }
        });
        y += vimora_ui::CONTROL_H + 18.0;
        if let (Some(err), None) = (&state.service_err, std::env::var_os("VIMORA_ABOUT_STATE")) {
            // The service is not reachable: the reason, in red, centred under the button.
            let mut job = egui::text::LayoutJob::simple(
                format!("{}: {err}", t(l, "noservice")),
                FontId::proportional(12.0),
                p.danger,
                r.width() - 60.0,
            );
            job.halign = egui::Align::Center;
            let g = ui.painter().layout_job(job);
            ui.painter().galley(pos2(cx, y), g, p.danger);
            return;
        }
        let mut progress = self.progress_of(state, installed);
        let mut show_state = show_state;
        // Dev aid: VIMORA_ABOUT_STATE=percent|wait|done|fail (for screenshots).
        match std::env::var("VIMORA_ABOUT_STATE").as_deref() {
            Ok("percent") => progress = Progress::Percent(45.0),
            Ok("wait") => progress = Progress::Waiting(t(l, "installing")),
            Ok("done") => progress = Progress::Done(t(l, "latest")),
            Ok("fail") => progress = Progress::Failed("o'rnatuvchi xatosi (1603)"),
            _ => {}
        }
        if std::env::var_os("VIMORA_ABOUT_STATE").is_some() {
            show_state = true;
        }
        if show_state {
            status_block(ui, cx, y, r.width() - 80.0, progress);
        }
    }
}

/// The update state in the middle, under the button: a spinner and its words, a bar with the
/// percent, or a result with its icon. Nothing when there is nothing to say.
fn status_block(ui: &mut egui::Ui, cx: f32, y: f32, width: f32, state: Progress<'_>) {
    let p = vimora_ui::palette(ui.ctx());
    let line_y = y + 11.0;
    // An icon (or spinner) and a text as one group, centred.
    let group = |ui: &egui::Ui, icon: Option<&str>, text: &str, col: egui::Color32, spin: bool| {
        let g = vimora_ui::truncated(ui, text, FontId::proportional(13.0), col, width - 30.0);
        let total = 22.0 + g.size().x;
        let x0 = cx - total / 2.0;
        if spin {
            let s = Rect::from_center_size(pos2(x0 + 7.0, line_y), vec2(14.0, 14.0));
            egui::Spinner::new()
                .size(14.0)
                .color(p.accent)
                .paint_at(ui, s);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        } else if let Some(ic) = icon {
            ui.painter().text(
                pos2(x0 + 7.0, line_y),
                Align2::CENTER_CENTER,
                ic,
                FontId::proportional(15.0),
                col,
            );
        }
        ui.painter()
            .galley(pos2(x0 + 22.0, line_y - g.size().y / 2.0), g, col);
    };
    match state {
        Progress::Idle(..) => {}
        Progress::Waiting(text) => group(ui, None, text, p.text, true),
        Progress::Done(text) => group(ui, Some(icons::CIRCLE_CHECK), text, p.success, false),
        Progress::Failed(text) => group(ui, Some(icons::TRIANGLE_ALERT), text, p.danger, false),
        Progress::Percent(pct) => {
            let bw = width.min(260.0);
            let bar = Rect::from_center_size(pos2(cx, y + 4.0), vec2(bw, 6.0));
            ui.painter().rect_filled(bar, 0.0, p.sunken);
            ui.painter().rect_filled(
                Rect::from_min_size(bar.min, vec2(bw * pct.clamp(0.0, 100.0) / 100.0, 6.0)),
                0.0,
                p.accent,
            );
            ui.painter().text(
                pos2(cx, y + 22.0),
                Align2::CENTER_CENTER,
                format!("{pct:.0}%"),
                FontId::new(12.5, vimora_ui::mono()),
                p.muted,
            );
        }
    }
}

/// "…45%" at the end of a status line → 45.
fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%')
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.parse::<f32>().ok())
}
