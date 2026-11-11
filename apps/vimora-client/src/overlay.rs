//! Fullscreen overlay process: lock screen and teacher-screen presentation.
//!
//! Driven by the agent over stdin using length-prefixed bincode [`OverlayCmd`]s. When stdin
//! closes (agent stopped or wants the overlay gone) the process exits.

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vimora_ui::{icons, ThemeMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayCmd {
    Lock {
        title: String,
        message: String,
    },
    Show,
    Frame {
        jpeg: Vec<u8>,
    },
    /// Offline unlock code (salted hash + length); an empty hash means "no code".
    Code {
        salt: Vec<u8>,
        hash: Vec<u8>,
        digits: u8,
    },
    /// Fade out and exit (the agent kills it anyway shortly after).
    Bye,
    /// The icon in the middle of the lock screen (one glyph of the icon font; empty = lock).
    Style {
        icon: String,
    },
}

#[derive(Clone)]
enum Mode {
    Lock { title: String, message: String },
    Show,
}

struct Shared {
    /// Icon the teacher chose for the lock screen.
    icon: Option<String>,
    code: Option<(Vec<u8>, Vec<u8>, u8)>,
    mode: Mode,
    frame: Option<egui::ColorImage>,
    frame_dirty: bool,
    /// Asked to leave: fading out since then.
    closing: Option<std::time::Instant>,
}

pub fn run() -> anyhow::Result<()> {
    let shared = Arc::new(Mutex::new(Shared {
        icon: None,
        code: None,
        mode: Mode::Lock {
            title: String::new(),
            message: String::new(),
        },
        frame: None,
        frame_dirty: false,
        closing: None,
    }));

    // Windows: a desktop of its own — nothing to Alt+Tab or Start-menu to (see `desktop`).
    // Must come before any window or hook is created on this thread.
    // The agent turns it off (VIMORA_LOCK_DESKTOP=0) when no graphics backend can draw there.
    let own_desktop =
        std::env::var("VIMORA_LOCK_DESKTOP").as_deref() != Ok("0") && crate::desktop::enter();
    tracing::info!("overlay on its own desktop: {own_desktop}");
    #[cfg(windows)]
    input_block::install();

    // Windows: not "fullscreen" but a borderless window one pixel taller than the screen.
    // A window that covers the screen exactly is put into exclusive full-screen mode by many
    // drivers: the picture goes black and Windows minimizes it as soon as anything (the Start
    // menu) takes focus. The guard thread keeps it at that size, on top and restored.
    #[cfg(windows)]
    let viewport = {
        let (w, h) = input_block::screen_size_logical();
        egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_position([0.0, 0.0])
            .with_inner_size([w, h + 1.0])
            .with_resizable(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_taskbar(false)
    };
    #[cfg(not(windows))]
    let viewport = egui::ViewportBuilder::default()
        .with_title("Vimora")
        .with_fullscreen(true)
        .with_always_on_top()
        .with_decorations(false)
        .with_taskbar(false);
    let opts = eframe::NativeOptions {
        viewport,
        renderer: crate::ui_renderer(),
        ..Default::default()
    };

    let s2 = shared.clone();
    eframe::run_native(
        "vimora-overlay",
        opts,
        Box::new(move |cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            vimora_ui::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            // Dev aid: VIMORA_OVERLAY_DEMO=<code> shows a sample lock screen without the agent;
            // VIMORA_OVERLAY_TYPE=<digits> pretends they were typed; VIMORA_OVERLAY_SCREENSHOT=<file>.
            let demo = std::env::var("VIMORA_OVERLAY_DEMO").ok();
            let mut entered = String::new();
            let mut typed_at = None;
            if let Some(code) = &demo {
                let salt = vec![1u8, 2, 3];
                let mut sh = s2.lock().unwrap();
                sh.code = Some((
                    salt.clone(),
                    vimora_core::pin::hash(&salt, code),
                    code.len() as u8,
                ));
                sh.mode = Mode::Lock {
                    title: "Diqqat! Ekran qulflangan".into(),
                    message: "Iltimos, o'qituvchini tinglang.".into(),
                };
                if let Ok(t) = std::env::var("VIMORA_OVERLAY_TYPE") {
                    entered = t;
                    typed_at = Some(Instant::now());
                }
            } else {
                spawn_stdin_reader(s2.clone(), cc.egui_ctx.clone());
            }
            Ok(Box::new(OverlayApp {
                shared: s2,
                texture: None,
                last_focus: Instant::now(),
                started: Instant::now(),
                entered,
                wrong_at: None,
                failures: 0,
                blocked_until: None,
                unlocked_at: None,
                typed_at,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("overlay: {e}"))
}

fn spawn_stdin_reader(shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut len = [0u8; 4];
            if stdin.read_exact(&mut len).is_err() {
                break;
            }
            let len = u32::from_be_bytes(len) as usize;
            if len > vimora_core::net::MAX_FRAME {
                break;
            }
            let mut buf = vec![0u8; len];
            if stdin.read_exact(&mut buf).is_err() {
                break;
            }
            let Ok(cmd) = bincode::deserialize::<OverlayCmd>(&buf) else {
                continue;
            };
            let mut s = shared.lock().unwrap();
            match cmd {
                OverlayCmd::Lock { title, message } => s.mode = Mode::Lock { title, message },
                OverlayCmd::Show => s.mode = Mode::Show,
                OverlayCmd::Bye => s.closing = Some(std::time::Instant::now()),
                OverlayCmd::Style { icon } => {
                    // Only a single glyph of the icon font is accepted.
                    let mut ch = icon.chars();
                    s.icon = match (ch.next(), ch.next()) {
                        (Some(c), None) if ('\u{e000}'..='\u{f8ff}').contains(&c) => Some(icon),
                        _ => None,
                    };
                }
                OverlayCmd::Code { salt, hash, digits } => {
                    s.code = (!hash.is_empty()).then_some((salt, hash, digits.clamp(4, 8)))
                }
                OverlayCmd::Frame { jpeg } => {
                    drop(s);
                    if let Ok(f) = vimora_core::capture::decode_jpeg(&jpeg) {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [f.width as usize, f.height as usize],
                            &f.rgba,
                        );
                        let mut s = shared.lock().unwrap();
                        s.frame = Some(img);
                        s.frame_dirty = true;
                    }
                }
            }
            ctx.request_repaint();
        }
        // Agent went away: leave.
        std::process::exit(0);
    });
}

struct OverlayApp {
    shared: Arc<Mutex<Shared>>,
    texture: Option<egui::TextureHandle>,
    last_focus: Instant,
    started: Instant,
    /// Digits typed so far.
    entered: String,
    /// When the last wrong code was entered (drives the shake).
    wrong_at: Option<Instant>,
    failures: u32,
    /// Too many wrong codes: no input until then.
    blocked_until: Option<Instant>,
    /// The right code was entered.
    unlocked_at: Option<Instant>,
    /// Last key press (the cells light up while someone is typing).
    typed_at: Option<Instant>,
}

impl OverlayApp {
    /// Digits the student typed since the last frame: from the hook on Windows (the hook eats
    /// every key before any window sees it), from the window's own events elsewhere.
    fn typed_chars(&self, ctx: &egui::Context) -> Vec<char> {
        #[cfg(windows)]
        {
            let _ = ctx;
            input_block::take_keys()
        }
        #[cfg(not(windows))]
        {
            let mut out = vec![];
            ctx.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::Text(t) => out.extend(t.chars().filter(char::is_ascii_digit)),
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            ..
                        } => out.push('\u{8}'),
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed: true,
                            ..
                        } => out.push('\u{1b}'),
                        _ => {}
                    }
                }
            });
            out
        }
    }

    /// Feeds typed keys into the code box and checks the code once it is complete.
    fn handle_code_input(&mut self, ctx: &egui::Context) {
        let code = self.shared.lock().unwrap().code.clone();
        let chars = self.typed_chars(ctx);
        let Some((salt, hash, digits)) = code else {
            return;
        };
        if self.unlocked_at.is_some() {
            return;
        }
        if self.blocked_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        self.blocked_until = None;
        for c in chars {
            self.typed_at = Some(Instant::now());
            match c {
                '\u{8}' => {
                    self.entered.pop();
                }
                '\u{1b}' => self.entered.clear(),
                d if d.is_ascii_digit() && self.entered.len() < digits as usize => {
                    self.entered.push(d)
                }
                _ => {}
            }
            if self.entered.len() == digits as usize {
                if vimora_core::pin::verify(&salt, &hash, &self.entered) {
                    self.unlocked_at = Some(Instant::now());
                    // The agent reads this line and lifts the lock.
                    use std::io::Write;
                    let mut out = std::io::stdout().lock();
                    let _ = writeln!(out, "UNLOCK");
                    let _ = out.flush();
                } else {
                    self.failures += 1;
                    self.wrong_at = Some(Instant::now());
                    if self.failures.is_multiple_of(5) {
                        let secs = (30 * (self.failures as u64 / 5)).min(300);
                        self.blocked_until = Some(Instant::now() + Duration::from_secs(secs));
                    }
                }
                self.entered.clear();
                break;
            }
        }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refuse to close and keep ourselves on top.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        // The pointer may move (so it ends up over us) but is never shown.
        ctx.set_cursor_icon(egui::CursorIcon::None);
        // Only repair what was actually lost: re-sending fullscreen/focus on a timer made the
        // screen flash (the window was re-laid out every time). On Windows the hooks swallow
        // all input, so egui never even sees the window as focused: the native guard thread in
        // `input_block` keeps it in front instead, and this block must stay out of the way.
        if cfg!(not(windows)) && self.last_focus.elapsed() > Duration::from_secs(1) {
            self.last_focus = Instant::now();
            let (full, focused) = ctx.input(|i| {
                let v = i.viewport();
                (v.fullscreen.unwrap_or(true), v.focused.unwrap_or(true))
            });
            if !full {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            }
            if !focused {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));

        overlay_dev_screenshot(ctx);
        // Enter: fade in from black; leave: fade to black, then exit. Every frame is drawn while
        // that happens (the screen otherwise repaints only twice a second, which made the
        // background and the glow jump in instead of fading).
        const LEAVE_SECS: f32 = 0.35;
        let leave = self
            .shared
            .lock()
            .unwrap()
            .closing
            .map(|t| (t.elapsed().as_secs_f32() / LEAVE_SECS).min(1.0))
            .unwrap_or(0.0);
        if leave >= 1.0 {
            std::process::exit(0);
        }
        let entering = self.started.elapsed() < Duration::from_millis(900);
        if entering || leave > 0.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.handle_code_input(ctx);
        let mode = {
            let mut s = self.shared.lock().unwrap();
            if s.frame_dirty {
                s.frame_dirty = false;
                if let Some(img) = s.frame.take() {
                    match &mut self.texture {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => {
                            self.texture =
                                Some(ctx.load_texture("show", img, egui::TextureOptions::LINEAR))
                        }
                    }
                }
            }
            s.mode.clone()
        };

        let p = vimora_ui::DARK;
        match mode {
            Mode::Lock { title, message } => {
                let now = Instant::now();
                let view = LockView {
                    icon: self.shared.lock().unwrap().icon.clone(),
                    digits: self.shared.lock().unwrap().code.as_ref().map(|c| c.2),
                    filled: self.entered.len(),
                    wrong_age: self
                        .wrong_at
                        .map(|t| now.duration_since(t).as_secs_f32())
                        .filter(|a| *a < 1.6),
                    blocked_secs: self
                        .blocked_until
                        .and_then(|t| t.checked_duration_since(now))
                        .map(|d| d.as_secs() + 1),
                    unlocked: self.unlocked_at.is_some(),
                    typing: self
                        .typed_at
                        .is_some_and(|t| now.duration_since(t) < Duration::from_secs(6)),
                    time: self.started.elapsed().as_secs_f32(),
                    leave,
                };
                let animated = view.digits.is_some();
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(p.bg))
                    .show(ctx, |ui| {
                        lock_screen(ui, &title, &message, &view);
                    });
                if animated {
                    ctx.request_repaint_after(Duration::from_millis(33));
                }
            }
            Mode::Show => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(egui::Color32::BLACK))
                    .show(ctx, |ui| {
                        let avail = ui.available_rect_before_wrap();
                        if let Some(t) = &self.texture {
                            let s = t.size_vec2();
                            let scale = (avail.width() / s.x).min(avail.height() / s.y);
                            let rect = egui::Rect::from_center_size(avail.center(), s * scale);
                            ui.painter().image(
                                t.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}  O'qituvchi ekrani yuklanmoqda…",
                                        icons::CAST
                                    ))
                                    .size(22.0)
                                    .color(p.muted),
                                );
                            });
                        }
                    });
            }
        }
    }
}

/// Everything the lock screen draws besides the texts.
struct LockView {
    /// The teacher's icon (None = the lock).
    icon: Option<String>,
    /// Length of the unlock code (None = no code set, nothing to type).
    digits: Option<u8>,
    filled: usize,
    /// Seconds since a wrong code (shake / red flash).
    wrong_age: Option<f32>,
    /// Too many wrong codes: seconds left.
    blocked_secs: Option<u64>,
    unlocked: bool,
    /// Someone is typing right now.
    typing: bool,
    /// Seconds since the overlay started (animations).
    time: f32,
    /// 0 → 1 while fading out before closing.
    leave: f32,
}

fn lock_screen(ui: &mut egui::Ui, title: &str, message: &str, v: &LockView) {
    use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Rounding, Stroke};
    let p = vimora_ui::DARK;
    let rect = ui.max_rect();
    let painter = ui.painter();
    // Ease-out fade in over 0.6 s, ease-in fade out while leaving.
    let t_in = (v.time / 0.6).clamp(0.0, 1.0);
    let fade = (1.0 - (1.0 - t_in).powi(3)) * (1.0 - v.leave * v.leave);

    // Background: a calm vertical gradient with a soft green glow behind the lock tile.
    let c = pos2(rect.center().x, rect.center().y - 70.0);
    let top = Color32::from_rgb(0x1d, 0x1f, 0x1b);
    let bottom = Color32::from_rgb(0x0e, 0x0f, 0x0d);
    {
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
    // A steady glow (it used to "breathe", which only made it jump between repaints).
    glow(painter, c, 575.0, p.accent, 0.13 * fade);
    glow(painter, c, 307.0, p.accent, 0.20 * fade);
    glow(painter, c, 150.0, p.accent, 0.20 * fade);
    // Fine corner brackets: a quiet "locked" frame around the screen.
    let m = 28.0;
    let len = 46.0;
    let line = Stroke::new(2.0_f32, p.accent.gamma_multiply(0.55 * fade));
    for (corner, dx, dy) in [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.left_bottom(), 1.0, -1.0),
        (rect.right_bottom(), -1.0, -1.0),
    ] {
        let o = corner + vec2(dx * m, dy * m);
        painter.line_segment([o, o + vec2(dx * len, 0.0)], line);
        painter.line_segment([o, o + vec2(0.0, dy * len)], line);
    }

    // Brand (top left) and clock (top right).
    let logo = Rect::from_min_size(rect.left_top() + vec2(48.0, 44.0), vec2(30.0, 30.0));
    vimora_ui::logo(painter, logo);
    painter.text(
        pos2(logo.right() + 12.0, logo.center().y),
        Align2::LEFT_CENTER,
        "VIMORA",
        FontId::new(15.0, vimora_ui::mono_bold()),
        p.muted.gamma_multiply(fade),
    );
    let now = chrono::Local::now();
    painter.text(
        pos2(rect.right() - 48.0, logo.center().y - 6.0),
        Align2::RIGHT_CENTER,
        now.format("%H:%M").to_string(),
        FontId::new(30.0, vimora_ui::mono_bold()),
        p.text.gamma_multiply(fade),
    );
    painter.text(
        pos2(rect.right() - 48.0, logo.center().y + 18.0),
        Align2::RIGHT_CENTER,
        now.format("%d.%m.%Y").to_string(),
        FontId::new(12.0, vimora_ui::mono()),
        p.faint.gamma_multiply(fade),
    );

    // The lock itself: a large icon on the soft glow, no box around it.
    painter.text(
        c,
        Align2::CENTER_CENTER,
        if v.unlocked {
            icons::LOCK_OPEN
        } else {
            v.icon.as_deref().unwrap_or(icons::LOCK)
        },
        // Grows into place as it fades in.
        FontId::proportional(84.0 * (0.86 + 0.14 * fade)),
        if v.unlocked { p.success } else { p.accent }.gamma_multiply(fade),
    );

    // Texts: shown only when the teacher wrote them.
    let mut y = c.y + 92.0;
    if !title.trim().is_empty() {
        let g = painter.layout(
            title.to_string(),
            FontId::proportional(38.0),
            p.text.gamma_multiply(fade),
            rect.width() * 0.8,
        );
        painter.galley(
            pos2(rect.center().x - g.size().x / 2.0, y),
            g.clone(),
            p.text,
        );
        y += g.size().y + 14.0;
    }
    if !message.trim().is_empty() {
        let g = painter.layout(
            message.to_string(),
            FontId::proportional(21.0),
            p.muted.gamma_multiply(fade),
            rect.width() * 0.65,
        );
        painter.galley(
            pos2(rect.center().x - g.size().x / 2.0, y),
            g.clone(),
            p.muted,
        );
        y += g.size().y + 14.0;
    }

    // The code boxes: like a phone's PIN screen.
    if let Some(n) = v.digits {
        y += 26.0;
        let cell = vec2(56.0, 56.0);
        let gap = 12.0;
        let total = n as f32 * cell.x + (n as f32 - 1.0) * gap;
        let shake = v
            .wrong_age
            .filter(|a| *a < 0.5)
            .map(|a| (a * 46.0).sin() * 14.0 * (1.0 - a / 0.5))
            .unwrap_or(0.0);
        let x0 = rect.center().x - total / 2.0 + shake;
        let wrong = v.wrong_age.is_some_and(|a| a < 1.2);
        let edge = if v.unlocked {
            p.success
        } else if wrong || v.blocked_secs.is_some() {
            p.danger
        } else if v.typing {
            p.accent
        } else {
            p.line_strong
        };
        let idle = if v.typing || v.unlocked || wrong {
            1.0
        } else {
            0.55
        };
        for i in 0..n as usize {
            let r = Rect::from_min_size(pos2(x0 + i as f32 * (cell.x + gap), y), cell);
            let filled = i < v.filled || v.unlocked;
            painter.rect_filled(
                r,
                Rounding::ZERO,
                if filled {
                    p.sunken
                } else {
                    p.card.gamma_multiply(0.7)
                }
                .gamma_multiply(idle),
            );
            painter.rect_stroke(
                r,
                Rounding::ZERO,
                Stroke::new(
                    if i == v.filled && v.typing && !v.unlocked {
                        2.0_f32
                    } else {
                        1.0_f32
                    },
                    edge.gamma_multiply(idle),
                ),
            );
            if filled {
                // A typed digit shows as a dot, like a password field.
                painter.circle_filled(
                    r.center(),
                    8.5,
                    if v.unlocked {
                        p.success
                    } else if wrong {
                        p.danger
                    } else {
                        p.accent
                    },
                );
            } else if i == v.filled && v.typing && !v.unlocked && (v.time * 2.2).sin() > 0.0 {
                // Blinking caret in the active cell.
                painter.line_segment(
                    [r.center() - vec2(9.0, -14.0), r.center() + vec2(9.0, 14.0)],
                    Stroke::new(2.0_f32, p.accent),
                );
            }
        }
        let status = if v.unlocked {
            Some(("Ochildi", p.success))
        } else if let Some(s) = v.blocked_secs {
            return_blocked(painter, rect.center().x, y + cell.y + 26.0, s, p.danger);
            None
        } else if wrong {
            Some(("Kod noto'g'ri", p.danger))
        } else {
            Some(("Ochish kodini kiriting", p.faint))
        };
        if let Some((t, col)) = status {
            painter.text(
                pos2(rect.center().x, y + cell.y + 26.0),
                Align2::CENTER_CENTER,
                t,
                FontId::new(13.0, vimora_ui::mono()),
                col.gamma_multiply(if col == p.faint { idle } else { 1.0 }),
            );
        }
    }

    painter.text(
        pos2(rect.center().x, rect.bottom() - 40.0),
        Align2::CENTER_CENTER,
        format!("{}  Vimora", icons::GRADUATION_CAP),
        FontId::proportional(15.0),
        p.faint.gamma_multiply(fade),
    );
    let _ = Color32::WHITE;
    // Enter from black / leave to black: one veil over everything.
    if fade < 1.0 {
        painter.rect_filled(
            rect,
            Rounding::ZERO,
            Color32::from_black_alpha(((1.0 - fade) * 255.0) as u8),
        );
    }
}

/// A soft round glow: a triangle fan fading from `color` at the centre to nothing at `radius`.
fn glow(
    painter: &egui::Painter,
    centre: egui::Pos2,
    radius: f32,
    color: egui::Color32,
    alpha: f32,
) {
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(centre, color.gamma_multiply(alpha));
    const N: usize = 72;
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            centre + egui::vec2(a.cos(), a.sin()) * radius,
            egui::Color32::TRANSPARENT,
        );
    }
    for i in 0..N {
        mesh.add_triangle(0, 1 + i as u32, 1 + ((i + 1) % N) as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn return_blocked(painter: &egui::Painter, x: f32, y: f32, secs: u64, col: egui::Color32) {
    painter.text(
        egui::pos2(x, y),
        egui::Align2::CENTER_CENTER,
        format!("Juda ko'p urinish. {secs} soniyadan keyin qayta urining"),
        egui::FontId::new(13.0, vimora_ui::mono()),
        col,
    );
}

/// Windows: swallow all physical keyboard and mouse input while the overlay is alive, and keep
/// the overlay above everything (the taskbar / Start menu must not climb over the lock screen).
/// (Ctrl+Alt+Del is handled by the OS and can never be blocked.)
///
/// The hooks live on their own thread with its own message loop: a low-level hook whose thread
/// does not answer within the system timeout is silently dropped by Windows, which is exactly
/// how keys such as Ctrl+Shift+Esc used to slip through while the overlay was busy drawing.
#[cfg(windows)]
mod input_block {
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_ESCAPE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// Marks input we inject ourselves (Esc to close the Start menu, Alt to take focus):
    /// the hooks let exactly these through.
    const MAGIC: usize = 0x5649_4D52;
    /// A virtual key that is assigned to nothing.
    const NOOP_KEY: u16 = 0xE8;

    /// Keys the lock screen's code box may use, collected by the keyboard hook.
    static KEYS: std::sync::Mutex<Vec<char>> = std::sync::Mutex::new(Vec::new());
    /// Our full-screen window (found once it exists).
    static OURS: AtomicIsize = AtomicIsize::new(0);

    /// Drains the typed digits / Backspace (`\u{8}`) / Escape (`\u{1b}`).
    pub fn take_keys() -> Vec<char> {
        std::mem::take(&mut *KEYS.lock().unwrap())
    }

    /// Mouse: everything is eaten — clicks, wheel and movement (the pointer stays still and
    /// hidden over us).
    unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let m = &*(l as *const MSLLHOOKSTRUCT);
            if m.dwExtraInfo != MAGIC {
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }

    /// The primary screen in the units egui uses before the window exists (the process is not
    /// DPI-aware yet, so Windows reports scaled, logical pixels).
    pub fn screen_size_logical() -> (f32, f32) {
        unsafe {
            (
                GetSystemMetrics(SM_CXSCREEN).max(800) as f32,
                GetSystemMetrics(SM_CYSCREEN).max(600) as f32,
            )
        }
    }

    /// Keeps our window restored and covering the primary screen (plus one pixel, see `run`).
    unsafe fn keep_covering(ours: HWND) {
        if IsIconic(ours) != 0 || IsWindowVisible(ours) == 0 {
            ShowWindow(ours, SW_RESTORE);
            ShowWindow(ours, SW_SHOW);
        }
        let (cx, cy) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(ours, &mut r);
        if r.left != 0 || r.top != 0 || r.right - r.left != cx || r.bottom - r.top != cy + 1 {
            SetWindowPos(
                ours,
                HWND_TOPMOST,
                0,
                0,
                cx,
                cy + 1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOOWNERZORDER,
            );
        }
    }

    /// Keyboard: eat everything, but remember digits for the unlock code.
    unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let k = &*(l as *const KBDLLHOOKSTRUCT);
            if k.dwExtraInfo == MAGIC {
                return CallNextHookEx(null_mut(), code, w, l);
            }
            if w as u32 == WM_KEYDOWN || w as u32 == WM_SYSKEYDOWN {
                let vk = k.vkCode;
                let c = match vk {
                    0x30..=0x39 => Some((b'0' + (vk - 0x30) as u8) as char),
                    0x60..=0x69 => Some((b'0' + (vk - 0x60) as u8) as char), // numpad
                    0x08 => Some('\u{8}'),
                    0x1B => Some('\u{1b}'),
                    _ => None,
                };
                if let Some(c) = c {
                    if let Ok(mut keys) = KEYS.lock() {
                        if keys.len() < 64 {
                            keys.push(c);
                        }
                    }
                }
            }
            return 1;
        }
        CallNextHookEx(null_mut(), code, w, l)
    }

    unsafe fn hook_all() -> (HHOOK, HHOOK) {
        let module = GetModuleHandleW(null());
        (
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0),
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0),
        )
    }

    unsafe extern "system" fn find_ours(hwnd: HWND, _: LPARAM) -> BOOL {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        // Visible or minimized (a minimized window is what must be brought back).
        if pid == GetCurrentProcessId() && (IsWindowVisible(hwnd) != 0 || IsIconic(hwnd) != 0) {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            if r.right - r.left > 400 || IsIconic(hwnd) != 0 {
                OURS.store(hwnd as isize, Ordering::Relaxed);
                return 0;
            }
        }
        1
    }

    unsafe fn key(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: MAGIC,
                },
            },
        }
    }

    unsafe fn press(vk: u16) {
        let i = [key(vk, false), key(vk, true)];
        SendInput(2, i.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }

    /// Keeps the overlay in front without touching it while all is well (that touching was the
    /// once-a-second flashing): if something else got to the front — the Start menu, a window —
    /// close the Start menu and take the foreground back.
    unsafe fn guard() {
        let mut ours = OURS.load(Ordering::Relaxed) as HWND;
        if ours.is_null() || IsWindow(ours) == 0 {
            OURS.store(0, Ordering::Relaxed);
            EnumWindows(Some(find_ours), 0);
            ours = OURS.load(Ordering::Relaxed) as HWND;
            if ours.is_null() {
                return;
            }
        }
        keep_covering(ours);
        let fg = GetForegroundWindow();
        if fg == ours {
            return;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, &mut pid);
        if !fg.is_null() && pid != GetCurrentProcessId() {
            // Start / search / any shell popup closes on Esc.
            press(VK_ESCAPE);
        }
        // Windows only hands the foreground to a process that just got input. The injected key
        // is one no keyboard has (0xE8): it satisfies that rule and does nothing else. (An Alt
        // tap was used before — but Alt puts the front window into menu mode, which made the
        // title bar / system menu show up when this ran many times in a row.)
        press(NOOP_KEY);
        SetWindowPos(
            ours,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER,
        );
        SetForegroundWindow(ours);
    }

    pub fn install() {
        std::thread::spawn(|| unsafe {
            crate::desktop::join_thread();
            let mut hooks = hook_all();
            if hooks.0.is_null() || hooks.1.is_null() {
                tracing::warn!("input hooks: keyboard {:?}, mouse {:?}", hooks.0, hooks.1);
            }
            let mut renewed = std::time::Instant::now();
            let mut msg: MSG = std::mem::zeroed();
            loop {
                MsgWaitForMultipleObjects(0, null(), 0, 200, QS_ALLINPUT);
                while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                // Windows silently drops a low-level hook that once answered too slowly, and a
                // hook installed later by another program runs before ours. Re-hook every few
                // seconds: new hooks first, then the old ones go, so there is never a gap.
                if renewed.elapsed() > std::time::Duration::from_secs(3) {
                    renewed = std::time::Instant::now();
                    let fresh = hook_all();
                    // Only swap a hook for one that was actually installed: a failed install
                    // must never leave the keyboard or mouse unguarded.
                    if !fresh.0.is_null() {
                        if !hooks.0.is_null() {
                            UnhookWindowsHookEx(hooks.0);
                        }
                        hooks.0 = fresh.0;
                    } else {
                        tracing::warn!("keyboard hook could not be renewed");
                    }
                    if !fresh.1.is_null() {
                        if !hooks.1.is_null() {
                            UnhookWindowsHookEx(hooks.1);
                        }
                        hooks.1 = fresh.1;
                    } else {
                        tracing::warn!("mouse hook could not be renewed");
                    }
                }
            }
        });
        std::thread::spawn(|| {
            crate::desktop::join_thread();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                unsafe { guard() };
            }
        });
    }
}

fn overlay_dev_screenshot(ctx: &egui::Context) {
    let Ok(path) = std::env::var("VIMORA_OVERLAY_SCREENSHOT") else {
        return;
    };
    if (2.0..2.05).contains(&ctx.input(|i| i.time)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
    let shot = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = shot {
        let rgb: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| [c.r(), c.g(), c.b()])
            .collect();
        if let Ok(f) = std::fs::File::create(path) {
            let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(f, 90).encode(
                &rgb,
                img.size[0] as u32,
                img.size[1] as u32,
                image::ExtendedColorType::Rgb8,
            );
        }
        std::process::exit(0);
    }
    ctx.request_repaint();
}
