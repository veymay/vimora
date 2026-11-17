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
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Stroke};
use std::sync::{Arc, Mutex};
use vimora_ui::{icons, Kind, Progress};

const SIZE: [f32; 2] = [400.0, 410.0];

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
}

struct About {
    /// When the update state last changed (a result stays on the screen for a few seconds).
    changed: Option<std::time::Instant>,
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
            if let Ok(IpcResp::UpdateState(busy, last)) =
                rt.block_on(crate::service::call(IpcReq::UpdateState))
            {
                let mut s = shared.lock().unwrap();
                if s.busy != busy || s.last != last {
                    s.busy = busy;
                    s.last = last;
                    ctx.request_repaint();
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

        // The bar at the bottom, like a status bar: a line above it and "Chiqish" at the right.
        egui::TopBottomPanel::bottom("bar")
            .exact_height(64.0)
            .frame(egui::Frame::none().fill(p.panel))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                let w = r.width() - 40.0;
                let mut inner = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(r.shrink2(vec2(20.0, 0.0)))
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                inner.add_space((r.height() - vimora_ui::CONTROL_H) / 2.0);
                if vimora_ui::button_w(&mut inner, Some(icons::X), t(l, "exit"), Kind::Cancel, w)
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

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
        let mut y = r.top() + 26.0;
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
        // The teacher this computer is connected to.
        let (dot, word) = if self.connected {
            (p.success, t(l, "connected"))
        } else {
            (p.faint, t(l, "waiting"))
        };
        let (name, ink) = match &self.teacher {
            Some(n) => (n.clone(), p.text),
            None => (t(l, "none").to_string(), p.muted),
        };
        let line = if self.teacher.is_some() {
            format!("{name} · {word}")
        } else {
            name
        };
        let g = vimora_ui::truncated(
            ui,
            &line,
            FontId::new(13.5, vimora_ui::medium()),
            ink,
            r.width() - 90.0,
        );
        let total = 22.0 + g.size().x;
        let x0 = cx - total / 2.0;
        ui.painter().text(
            pos2(x0 + 7.0, y + 10.0),
            Align2::CENTER_CENTER,
            icons::GRADUATION_CAP,
            FontId::proportional(15.0),
            if self.connected { p.accent } else { p.faint },
        );
        ui.painter()
            .galley(pos2(x0 + 22.0, y + 10.0 - g.size().y / 2.0), g, ink);
        ui.painter()
            .circle_filled(pos2(x0 + total + 10.0, y + 10.0), 3.5, dot);
        y += 34.0;
        // The version.
        ui.painter().text(
            pos2(cx, y + 8.0),
            Align2::CENTER_CENTER,
            format!("v{}", env!("CARGO_PKG_VERSION")),
            FontId::new(13.0, vimora_ui::mono()),
            p.accent_ink,
        );
        y += 34.0;
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
        y += vimora_ui::CONTROL_H + 14.0;
        let progress = self.progress_of(state, installed);
        if show_state && !matches!(progress, Progress::Idle(..)) {
            let row = Rect::from_min_size(pos2(r.left() + 40.0, y), vec2(r.width() - 80.0, 24.0));
            vimora_ui::progress(ui, row, progress);
        }
    }
}

/// "…45%" at the end of a status line → 45.
fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%')
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.parse::<f32>().ok())
}
