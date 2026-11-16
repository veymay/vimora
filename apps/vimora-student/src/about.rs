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

const SIZE: [f32; 2] = [780.0, 520.0];
const NAV_W: f32 = 190.0;

/// The pages of the window (like the teacher's settings): the menu on the left.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    About,
    Author,
}

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
        ("home", "ru") => "Главное",
        ("home", "en") => "Home",
        ("home", _) => "Asosiy",
        ("about", "ru") => "О программе",
        ("about", "en") => "About",
        ("about", _) => "Dastur haqida",
        ("author", "ru") => "Автор",
        ("author", "en") => "Author",
        ("author", _) => "Muallif",
        ("tagline", "ru") => "Открытое управление классом · MIT",
        ("tagline", "en") => "Open-source classroom management · MIT",
        ("tagline", _) => "Ochiq manbali sinf boshqaruvi · MIT",
        ("source", "ru") => "Исходный код",
        ("source", "en") => "Source code",
        ("source", _) => "Manba kodi",
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
    page: Page,
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
            // Dev aid: VIMORA_ABOUT_PAGE=about|author (for screenshots).
            let page = match std::env::var("VIMORA_ABOUT_PAGE").as_deref() {
                Ok("about") => Page::About,
                Ok("author") => Page::Author,
                _ => Page::Home,
            };
            Ok(Box::new(About {
                page,
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

        // Left: the menu.
        egui::SidePanel::left("nav")
            .exact_width(NAV_W)
            .resizable(false)
            .frame(egui::Frame::none().fill(p.panel))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter()
                    .vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0_f32, p.line));
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (page, icon, key) in [
                    (Page::Home, icons::SLIDERS_HORIZONTAL, "home"),
                    (Page::About, icons::BADGE_INFO, "about"),
                    (Page::Author, icons::USER, "author"),
                ] {
                    let on = self.page == page;
                    let (rr, resp) =
                        ui.allocate_exact_size(vec2(NAV_W - 1.0, 46.0), Sense::click());
                    if on || resp.hovered() {
                        ui.painter()
                            .rect_filled(rr, 0.0, if on { p.card } else { p.hover });
                    }
                    if on {
                        ui.painter().rect_filled(
                            Rect::from_min_size(rr.min, vec2(3.0, rr.height())),
                            0.0,
                            p.accent,
                        );
                    }
                    ui.painter().text(
                        pos2(rr.left() + 28.0, rr.center().y),
                        Align2::CENTER_CENTER,
                        icon,
                        FontId::proportional(17.0),
                        if on { p.accent_ink } else { p.muted },
                    );
                    ui.painter().text(
                        pos2(rr.left() + 54.0, rr.center().y),
                        Align2::LEFT_CENTER,
                        t(l, key),
                        FontId::new(13.5, vimora_ui::medium()),
                        if on { p.strong } else { p.text },
                    );
                    ui.painter().text(
                        pos2(rr.right() - 20.0, rr.center().y),
                        Align2::CENTER_CENTER,
                        icons::CHEVRON_RIGHT,
                        FontId::proportional(15.0),
                        if on { p.accent_ink } else { p.faint },
                    );
                    if resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        self.page = page;
                    }
                }
            });

        // Right: the page.
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(26.0, 20.0)),
            )
            .show(ctx, |ui| match self.page {
                Page::Home => self.home(ui, ctx),
                Page::About => self.about(ui),
                Page::Author => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| vimora_ui::author::page(ui, self.lang));
                }
            });
    }
}

impl About {
    /// "Asosiy": the teacher this computer is connected to and the Vimora update.
    fn home(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = vimora_ui::palette(ui.ctx());
        let l = self.lang;
        let state = self.shared.lock().unwrap().clone();
        let width = ui.available_width();
        // The teacher.
        let (r, _) = ui.allocate_exact_size(vec2(width, 52.0), Sense::hover());
        ui.painter()
            .rect(r, 0.0, p.sunken, Stroke::new(1.0_f32, p.line));
        ui.painter().text(
            pos2(r.left() + 26.0, r.center().y),
            Align2::CENTER_CENTER,
            icons::GRADUATION_CAP,
            FontId::proportional(20.0),
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
            FontId::new(15.0, vimora_ui::bold()),
            ink,
            r.width() - 170.0,
        );
        ui.painter().galley(
            pos2(r.left() + 52.0, r.center().y - g.size().y / 2.0),
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
                .circle_filled(pos2(r.right() - 16.0, r.center().y), 4.0, dot);
            ui.painter().text(
                pos2(r.right() - 28.0, r.center().y),
                Align2::RIGHT_CENTER,
                word,
                FontId::proportional(12.5),
                p.muted,
            );
        }
        ui.add_space(14.0);

        // The update, as the whole system sees it.
        let (row, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        let installed;
        let version_line = format!("{} {}", t(l, "version"), env!("CARGO_PKG_VERSION"));
        let busy = state.busy.as_deref();
        let progress = if let Some(v) = &state.installed {
            installed = format!("{}: {v}", t(l, "installed"));
            Progress::Done(&installed)
        } else if let Some(b) = busy {
            match percent_of(b) {
                Some(pct) => Progress::Percent(pct),
                None if b.contains("o'rnatilmoqda") => Progress::Waiting(t(l, "installing")),
                None => Progress::Waiting(t(l, "checking")),
            }
        } else if state.no_service {
            Progress::Failed(t(l, "noservice"))
        } else {
            match &state.last {
                // "Installing" with nothing running any more: the outcome is not known yet
                // (the new version reports itself above when it arrives).
                Some((true, m)) if m.contains("o'rnatilmoqda") => {
                    Progress::Idle(&version_line, p.muted)
                }
                Some((true, _)) => Progress::Done(t(l, "latest")),
                Some((false, m)) => Progress::Failed(m),
                None => Progress::Idle(&version_line, p.muted),
            }
        };
        let running = matches!(progress, Progress::Waiting(_) | Progress::Percent(_));
        vimora_ui::progress(ui, row, progress);
        ui.add_space(12.0);
        ui.add_enabled_ui(!running, |ui| {
            if vimora_ui::button(ui, Some(icons::REFRESH_CW), t(l, "check"), Kind::Outline)
                .clicked()
            {
                self.start_check(ctx);
            }
        });
    }

    /// "Dastur haqida": the mark, the name, the version and where the source is.
    fn about(&mut self, ui: &mut egui::Ui) {
        let p = vimora_ui::palette(ui.ctx());
        let l = self.lang;
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
        let logo = Rect::from_min_size(r.min, vec2(96.0, 96.0));
        vimora_ui::logo(ui.painter(), logo);
        ui.painter().text(
            pos2(logo.right() + vimora_ui::logo_gap(96.0), r.top() + 26.0),
            Align2::LEFT_CENTER,
            "VIMORA",
            FontId::new(30.0, vimora_ui::brand()),
            p.strong,
        );
        ui.painter().text(
            pos2(logo.right() + vimora_ui::logo_gap(96.0), r.top() + 60.0),
            Align2::LEFT_CENTER,
            format!("{} {}", t(l, "version"), env!("CARGO_PKG_VERSION")),
            FontId::new(13.0, vimora_ui::mono()),
            p.accent_ink,
        );
        ui.painter().text(
            pos2(logo.right() + 20.0, r.top() + 82.0),
            Align2::LEFT_CENTER,
            t(l, "tagline"),
            FontId::proportional(12.5),
            p.muted,
        );
        ui.add_space(20.0);
        // The source: one row, a click opens it.
        let (row, resp) = ui.allocate_exact_size(vec2(w, 42.0), Sense::click());
        ui.painter()
            .rect(row, 0.0, p.sunken, Stroke::new(1.0_f32, p.line));
        if resp.hovered() {
            ui.painter().rect_filled(row, 0.0, p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        ui.painter().text(
            pos2(row.left() + 22.0, row.center().y),
            Align2::CENTER_CENTER,
            icons::GITHUB,
            FontId::proportional(16.0),
            p.accent_ink,
        );
        ui.painter().text(
            pos2(row.left() + 46.0, row.center().y),
            Align2::LEFT_CENTER,
            t(l, "source"),
            FontId::new(13.5, vimora_ui::medium()),
            p.text,
        );
        ui.painter().text(
            pos2(row.right() - 16.0, row.center().y),
            Align2::RIGHT_CENTER,
            "github.com/veymay/vimora",
            FontId::new(12.5, vimora_ui::mono()),
            p.muted,
        );
        if resp.clicked() {
            ui.ctx()
                .open_url(egui::OpenUrl::new_tab("https://github.com/veymay/vimora"));
        }
    }
}

/// "…45%" at the end of a status line → 45.
fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%')
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.parse::<f32>().ok())
}
