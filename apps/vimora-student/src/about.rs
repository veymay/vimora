//! `vimora-student about`: the small window of the tray icon (left or right click).
//!
//! In the middle of the screen, wide: logo and version on the left; on the right the teacher
//! computer this one is connected to and the Vimora update — one state for the whole system:
//! whoever started an update (the teacher, the service by itself, this window's button), the
//! same progress shows here (downloading with a percent, installing, done). Only an X.
//!
//! The agent passes what it knows in the environment: `VIMORA_TEACHER` (the teacher computer's
//! name) and `VIMORA_CONNECTED` (1 / 0).

use crate::service::{IpcReq, IpcResp};
use crate::smallwin;
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, Sense, Stroke};
use std::sync::{Arc, Mutex};
use vimora_ui::{icons, Kind, Progress};

const SIZE: [f32; 2] = [520.0, 212.0];

fn t(lang: &str, key: &str) -> &'static str {
    match (key, lang) {
        ("version", "ru") => "Версия",
        ("version", "en") => "Version",
        ("version", _) => "Versiya",
        ("connected", "ru") => "подключён",
        ("connected", "en") => "connected",
        ("connected", _) => "ulangan",
        ("waiting", "ru") => "ожидание",
        ("waiting", "en") => "waiting",
        ("waiting", _) => "kutilmoqda",
        ("none", "ru") => "Учитель ещё не подключён",
        ("none", "en") => "No teacher connected yet",
        ("none", _) => "O'qituvchi hali ulanmagan",
        ("check", "ru") => "Проверить обновления",
        ("check", "en") => "Check for updates",
        ("check", _) => "Yangilanishlarni tekshirish",
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
        ("noservice", "ru") => "Служба Vimora не отвечает",
        ("noservice", "en") => "The Vimora service does not answer",
        ("noservice", _) => "Vimora xizmati javob bermayapti",
        _ => "",
    }
}

/// What the service says about the Vimora update, refreshed every second.
#[derive(Clone, Default)]
struct Shared {
    /// Running now ("Vimora 0.3.32 yuklanmoqda 45%", "… o'rnatilmoqda").
    busy: Option<String>,
    /// How the last attempt ended.
    last: Option<(bool, String)>,
    /// The service did not answer the button.
    no_service: bool,
    /// Another version is installed than the one this window runs from.
    installed: Option<String>,
}

struct About {
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
}

impl eframe::App for About {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        smallwin::screenshot(ctx);
        if smallwin::title_bar(ctx, "Vimora") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let p = vimora_ui::palette(ctx);
        let l = self.lang;
        let state = self.shared.lock().unwrap().clone();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(20.0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                // Left: logo, name, version.
                let logo =
                    Rect::from_min_size(pos2(full.left(), full.top() + 2.0), vec2(52.0, 52.0));
                vimora_ui::logo(ui.painter(), logo);
                ui.painter().text(
                    pos2(full.left(), logo.bottom() + 18.0),
                    Align2::LEFT_CENTER,
                    "VIMORA",
                    FontId::new(21.0, vimora_ui::brand()),
                    p.strong,
                );
                ui.painter().text(
                    pos2(full.left(), logo.bottom() + 40.0),
                    Align2::LEFT_CENTER,
                    format!("{} {}", t(l, "version"), env!("CARGO_PKG_VERSION")),
                    FontId::new(12.5, vimora_ui::mono()),
                    p.muted,
                );
                let split = full.left() + 150.0;
                ui.painter()
                    .vline(split, full.y_range(), Stroke::new(1.0_f32, p.line));

                // Right, three plain rows: the teacher, the update state, the button.
                let right = Rect::from_min_max(pos2(split + 22.0, full.top()), full.max);
                let mut ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                let ui = &mut ui;
                let (r, _) = ui.allocate_exact_size(vec2(right.width(), 40.0), Sense::hover());
                ui.painter().text(
                    pos2(r.left() + 10.0, r.center().y),
                    Align2::CENTER_CENTER,
                    icons::GRADUATION_CAP,
                    FontId::proportional(18.0),
                    if self.connected { p.accent } else { p.faint },
                );
                let name = self
                    .teacher
                    .clone()
                    .unwrap_or_else(|| t(l, "none").to_string());
                let ink = if self.teacher.is_some() {
                    p.strong
                } else {
                    p.muted
                };
                let g = vimora_ui::truncated(
                    ui,
                    &name,
                    FontId::new(14.0, vimora_ui::bold()),
                    ink,
                    r.width() - 130.0,
                );
                ui.painter().galley(
                    pos2(r.left() + 30.0, r.center().y - g.size().y / 2.0),
                    g,
                    ink,
                );
                if self.teacher.is_some() {
                    let (dot, word) = if self.connected {
                        (p.success, t(l, "connected"))
                    } else {
                        (p.faint, t(l, "waiting"))
                    };
                    ui.painter()
                        .circle_filled(pos2(r.right() - 5.0, r.center().y), 4.0, dot);
                    ui.painter().text(
                        pos2(r.right() - 16.0, r.center().y),
                        Align2::RIGHT_CENTER,
                        word,
                        FontId::proportional(11.5),
                        p.muted,
                    );
                }
                ui.painter()
                    .hline(r.x_range(), r.bottom() + 4.0, Stroke::new(1.0_f32, p.line));
                ui.add_space(14.0);

                // The update, as the whole system sees it.
                let (row, _) = ui.allocate_exact_size(vec2(right.width(), 24.0), Sense::hover());
                let installed;
                let busy = state.busy.as_deref();
                let progress = if let Some(v) = &state.installed {
                    installed = format!("{}: {v}", t(l, "installed"));
                    Progress::Done(&installed)
                } else if let Some(b) = busy {
                    match percent_of(b) {
                        Some(pct) => Progress::Percent(pct),
                        None if b.contains("o'rnatilmoqda") => {
                            Progress::Waiting(t(l, "installing"))
                        }
                        None => Progress::Waiting(t(l, "checking")),
                    }
                } else if state.no_service {
                    Progress::Failed(t(l, "noservice"))
                } else {
                    match &state.last {
                        // "Installing" with nothing running any more: the outcome is not
                        // known yet (the new version reports itself above when it arrives).
                        Some((true, m)) if m.contains("o'rnatilmoqda") => {
                            Progress::Idle("", p.faint)
                        }
                        Some((true, _)) => Progress::Done(t(l, "latest")),
                        Some((false, m)) => Progress::Failed(m),
                        None => Progress::Idle("", p.faint),
                    }
                };
                let running = matches!(progress, Progress::Waiting(_) | Progress::Percent(_));
                vimora_ui::progress(ui, row, progress);
                ui.add_space(10.0);
                ui.add_enabled_ui(!running, |ui| {
                    if vimora_ui::button(ui, Some(icons::REFRESH_CW), t(l, "check"), Kind::Outline)
                        .clicked()
                    {
                        self.start_check(ctx);
                    }
                });
            });
    }
}

/// "…45%" at the end of a status line → 45.
fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%')
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.parse::<f32>().ok())
}
