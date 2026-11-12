//! `vimora-student about`: the small window of the tray icon (left or right click).
//!
//! In the middle of the screen, wide: logo and version on the left; the teacher computer this one
//! is connected to and "check for updates" (the service installs a newer signed release at once)
//! on the right. Only an X — no minimize.
//!
//! The agent passes what it knows in the environment: `VIMORA_TEACHER` (the teacher computer's
//! name) and `VIMORA_CONNECTED` (1 / 0).

use crate::service::{IpcReq, IpcResp};
use crate::smallwin;
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Sense, Stroke};
use std::sync::{Arc, Mutex};
use vimora_ui::{icons, Kind};

const SIZE: [f32; 2] = [520.0, 236.0];

fn t(lang: &str, key: &str) -> &'static str {
    match (key, lang) {
        ("version", "ru") => "Версия",
        ("version", "en") => "Version",
        ("version", _) => "Versiya",
        ("teacher", "ru") => "Компьютер учителя",
        ("teacher", "en") => "Teacher's computer",
        ("teacher", _) => "O'qituvchi kompyuteri",
        ("connected", "ru") => "подключён",
        ("connected", "en") => "connected",
        ("connected", _) => "ulangan",
        ("waiting", "ru") => "ожидание подключения",
        ("waiting", "en") => "waiting for the teacher",
        ("waiting", _) => "ulanish kutilmoqda",
        ("none", "ru") => "Ещё не подключён",
        ("none", "en") => "Not connected yet",
        ("none", _) => "Hali ulanmagan",
        ("check", "ru") => "Проверить обновления",
        ("check", "en") => "Check for updates",
        ("check", _) => "Yangilanishlarni tekshirish",
        ("checking", "ru") => "Проверка…",
        ("checking", "en") => "Checking…",
        ("checking", _) => "Tekshirilmoqda…",
        ("latest", "ru") => "Установлена последняя версия",
        ("latest", "en") => "This is the latest version",
        ("latest", _) => "Eng so'nggi versiya o'rnatilgan",
        ("installing", "ru") => "Новая версия устанавливается",
        ("installing", "en") => "A new version is being installed",
        ("installing", _) => "Yangi versiya o'rnatilmoqda",
        ("noservice", "ru") => "Служба Vimora не отвечает",
        ("noservice", "en") => "The Vimora service does not answer",
        ("noservice", _) => "Vimora xizmati javob bermayapti",
        _ => "",
    }
}

#[derive(Clone)]
enum Check {
    Idle,
    Running,
    Done(bool, String),
}

struct About {
    lang: &'static str,
    teacher: Option<String>,
    connected: bool,
    check: Arc<Mutex<Check>>,
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
            Ok(Box::new(About {
                lang,
                teacher,
                connected,
                check: Arc::new(Mutex::new(Check::Idle)),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("about: {e}"))
}

impl About {
    fn start_check(&self, ctx: &egui::Context) {
        *self.check.lock().unwrap() = Check::Running;
        let (check, ctx, lang) = (self.check.clone(), ctx.clone(), self.lang);
        std::thread::spawn(move || {
            let res = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|rt| rt.block_on(crate::service::call(IpcReq::UpdateNow)));
            let out = match res {
                Ok(IpcResp::Done { ok: true, message }) => {
                    if message.contains("eng so'nggi") {
                        Check::Done(true, t(lang, "latest").into())
                    } else if message.contains("o'rnatilmoqda") || message.contains("yuklanmoqda") {
                        Check::Done(true, t(lang, "installing").into())
                    } else {
                        Check::Done(true, message)
                    }
                }
                Ok(IpcResp::Done { ok: false, message }) => Check::Done(false, message),
                _ => Check::Done(false, t(lang, "noservice").into()),
            };
            *check.lock().unwrap() = out;
            ctx.request_repaint();
        });
    }
}

impl eframe::App for About {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        smallwin::screenshot(ctx);
        if smallwin::title_bar(ctx, "Vimora") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let p = vimora_ui::palette(ctx);
        let l = self.lang;
        // The footer has its own strip: nothing can run over it.
        egui::TopBottomPanel::bottom("about-foot")
            .exact_height(30.0)
            .frame(egui::Frame::none().fill(p.panel))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                ui.painter().text(
                    pos2(r.left() + 18.0, r.center().y),
                    Align2::LEFT_CENTER,
                    "VEYMAY · github.com/veymay/vimora",
                    FontId::proportional(11.0),
                    p.faint,
                );
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(18.0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                // Left: logo, name, version.
                let left = Rect::from_min_size(full.min, vec2(150.0, full.height()));
                let logo =
                    Rect::from_min_size(pos2(left.left(), left.top() + 4.0), vec2(56.0, 56.0));
                vimora_ui::logo(ui.painter(), logo);
                ui.painter().text(
                    pos2(left.left(), logo.bottom() + 18.0),
                    Align2::LEFT_CENTER,
                    "Vimora",
                    FontId::new(20.0, vimora_ui::bold()),
                    p.strong,
                );
                ui.painter().text(
                    pos2(left.left(), logo.bottom() + 40.0),
                    Align2::LEFT_CENTER,
                    format!("{} {}", t(l, "version"), env!("CARGO_PKG_VERSION")),
                    FontId::new(12.5, vimora_ui::mono()),
                    p.muted,
                );
                ui.painter().vline(
                    left.right() + 4.0,
                    full.y_range(),
                    Stroke::new(1.0_f32, p.line),
                );
                // Right: the teacher computer and updates.
                let right = Rect::from_min_max(pos2(left.right() + 22.0, full.top()), full.max);
                let mut ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                let ui = &mut ui;
                let (r, _) = ui.allocate_exact_size(vec2(right.width(), 58.0), Sense::hover());
                ui.painter()
                    .rect(r, 0.0, p.card, Stroke::new(1.0_f32, p.line));
                ui.painter().text(
                    pos2(r.left() + 24.0, r.center().y),
                    Align2::CENTER_CENTER,
                    icons::GRADUATION_CAP,
                    FontId::proportional(20.0),
                    if self.connected { p.accent } else { p.faint },
                );
                ui.painter().text(
                    pos2(r.left() + 46.0, r.center().y - 10.0),
                    Align2::LEFT_CENTER,
                    t(l, "teacher"),
                    FontId::proportional(11.5),
                    p.muted,
                );
                let name = self
                    .teacher
                    .clone()
                    .unwrap_or_else(|| t(l, "none").to_string());
                let g = vimora_ui::truncated(
                    ui,
                    &name,
                    FontId::new(14.0, vimora_ui::bold()),
                    p.strong,
                    r.width() - 150.0,
                );
                ui.painter().galley(
                    pos2(r.left() + 46.0, r.center().y + 9.0 - g.size().y / 2.0),
                    g,
                    p.strong,
                );
                if self.teacher.is_some() {
                    let (dot, word) = if self.connected {
                        (p.success, t(l, "connected"))
                    } else {
                        (p.faint, t(l, "waiting"))
                    };
                    ui.painter()
                        .circle_filled(pos2(r.right() - 14.0, r.center().y), 4.0, dot);
                    ui.painter().text(
                        pos2(r.right() - 24.0, r.center().y),
                        Align2::RIGHT_CENTER,
                        word,
                        FontId::proportional(11.5),
                        p.muted,
                    );
                }
                ui.add_space(14.0);
                // Updates: the button, then the state in the same shared style as everywhere.
                let state = self.check.lock().unwrap().clone();
                if !matches!(state, Check::Running)
                    && vimora_ui::button(ui, Some(icons::REFRESH_CW), t(l, "check"), Kind::Outline)
                        .clicked()
                {
                    self.start_check(ctx);
                }
                ui.add_space(10.0);
                let (row, _) = ui.allocate_exact_size(vec2(right.width(), 22.0), Sense::hover());
                match &state {
                    Check::Idle => {}
                    Check::Running => {
                        vimora_ui::progress(ui, row, vimora_ui::Progress::Waiting(t(l, "checking")))
                    }
                    Check::Done(true, text) => {
                        vimora_ui::progress(ui, row, vimora_ui::Progress::Done(text))
                    }
                    Check::Done(false, text) => {
                        vimora_ui::progress(ui, row, vimora_ui::Progress::Failed(text))
                    }
                }
            });
    }
}
