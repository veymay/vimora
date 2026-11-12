//! Student chat (`vimora-student chat`): a panel docked to the right screen edge.
//!
//! * A new teacher message opens the panel (**open**). If the student doesn't touch it (pointer
//!   or typing) within a few seconds, it folds into a small square in the bottom-right corner
//!   (**folded**) with the number of unread messages; a click opens it again. The header's
//!   button folds it too.
//! * The window is resized only when it opens or folds — never animated frame by frame (that
//!   flickered, and a see-through window turned black on some graphics drivers).
//! * Once the panel has been on screen for a moment, the agent is told it was read ("READ" on
//!   stdout), which the teacher sees as the second tick.
//! * When the teacher ends the chat the agent closes stdin / sends `End` and the process exits,
//!   so it can't be reopened until the teacher writes again.
//!
//! Driven by the agent over stdin (length-prefixed bincode [`ChatIn`]); the student's replies
//! go to stdout as one JSON string per line.

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vimora_ui::{icons, Palette, ThemeMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatIn {
    /// A message from the teacher.
    Teacher { text: String, time: String },
    /// The teacher ended the chat.
    End,
}

const OPEN_W: f32 = 380.0;
/// The folded chat: a square in the bottom-right corner.
const FOLD: f32 = 56.0;
const FOLD_MARGIN: f32 = 16.0;
/// Folds this long after the student stopped using it.
const LINGER: Duration = Duration::from_millis(1500);
/// A message that pops the panel out hides it again after this, unless the student uses it.
const AUTO_HIDE: Duration = Duration::from_secs(6);
/// On screen this long = read.
const READ_AFTER: Duration = Duration::from_millis(1200);
/// Composer row: the input and the send button share this height.
const CONTROL_H: f32 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Open,
    Folded,
}

struct Msg {
    mine: bool,
    text: String,
    time: String,
}

#[derive(Default)]
struct Inbox {
    messages: Vec<Msg>,
    /// Arrived while collapsed.
    unseen: usize,
    new_message: bool,
}

pub fn run() -> anyhow::Result<()> {
    let inbox: Arc<Mutex<Inbox>> = Arc::default();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora — xabarlar")
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_min_inner_size([FOLD, FOLD])
            .with_inner_size([OPEN_W, 640.0])
            // Hidden until it has been moved to the right screen edge (no jump on start).
            .with_visible(false),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    let i2 = inbox.clone();
    eframe::run_native(
        "vimora-chat",
        opts,
        Box::new(move |cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            let mode = if cc.egui_ctx.style().visuals.dark_mode {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            vimora_ui::apply_theme(&cc.egui_ctx, mode);
            spawn_stdin_reader(i2.clone(), cc.egui_ctx.clone());
            Ok(Box::new(ChatApp {
                inbox: i2,
                mode: Mode::Open,
                last_geometry: None,
                draft: String::new(),
                shown_at: Instant::now(),
                touched: false,
                read: 0,
                scroll_down: true,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("chat: {e}"))
}

fn spawn_stdin_reader(inbox: Arc<Mutex<Inbox>>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut len = [0u8; 4];
            if stdin.read_exact(&mut len).is_err() {
                break;
            }
            let mut buf = vec![0u8; u32::from_be_bytes(len) as usize];
            if stdin.read_exact(&mut buf).is_err() {
                break;
            }
            match bincode::deserialize::<ChatIn>(&buf) {
                Ok(ChatIn::Teacher { text, time }) => {
                    let mut i = inbox.lock().unwrap();
                    i.messages.push(Msg {
                        mine: false,
                        text,
                        time,
                    });
                    i.new_message = true;
                    i.unseen += 1;
                }
                Ok(ChatIn::End) => break,
                Err(_) => continue,
            }
            ctx.request_repaint();
        }
        // Teacher ended the chat or the agent went away.
        std::process::exit(0);
    });
}

struct ChatApp {
    inbox: Arc<Mutex<Inbox>>,
    mode: Mode,
    /// Last position and size given to the window: (x, y, w, h).
    last_geometry: Option<(f32, f32, f32, f32)>,
    draft: String,
    /// When it last slid out on its own (new message).
    shown_at: Instant,
    /// The student used it since then (pointer or typing): it stays.
    touched: bool,
    /// Teacher messages already reported as read.
    read: usize,
    /// Jump to the newest message on the next frame.
    scroll_down: bool,
}

impl ChatApp {
    fn send(&mut self) {
        let t = self.draft.trim().to_string();
        if t.is_empty() {
            return;
        }
        let mut out = std::io::stdout().lock();
        if writeln!(out, "{}", serde_json::to_string(&t).unwrap_or_default())
            .and_then(|_| out.flush())
            .is_ok()
        {
            let time = now_hhmm();
            self.inbox.lock().unwrap().messages.push(Msg {
                mine: true,
                text: t,
                time,
            });
            self.draft.clear();
        }
    }

    /// Open: the right screen edge, top to bottom (without covering the taskbar). Folded: a
    /// square in the bottom-right corner. The window is moved only when this changes.
    fn place(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));
        let ppp = ctx.pixels_per_point().max(0.5);
        let (_, top, right, bottom) = work_area()
            .map(|(l, t, r, b)| (l / ppp, t / ppp, r / ppp, b / ppp))
            .unwrap_or((0.0, 0.0, monitor.x, monitor.y));
        let geo = match self.mode {
            Mode::Open => (
                (right - OPEN_W).round(),
                top.round(),
                OPEN_W,
                (bottom - top).round(),
            ),
            Mode::Folded => (
                (right - FOLD - FOLD_MARGIN).round(),
                (bottom - FOLD - FOLD_MARGIN).round(),
                FOLD,
                FOLD,
            ),
        };
        if self.last_geometry != Some(geo) {
            let first = self.last_geometry.is_none();
            self.last_geometry = Some(geo);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(geo.2, geo.3)));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                geo.0, geo.1,
            )));
            if first {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
    }
}

/// The part of the primary screen not taken by the taskbar, in physical pixels
/// (left, top, right, bottom). Windows only.
#[cfg(windows)]
fn work_area() -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) };
    (ok != 0 && r.right > r.left && r.bottom > r.top).then_some((
        r.left as f32,
        r.top as f32,
        r.right as f32,
        r.bottom as f32,
    ))
}

#[cfg(not(windows))]
fn work_area() -> Option<(f32, f32, f32, f32)> {
    None
}

/// HH:MM local time for chat stamps.
pub fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl eframe::App for ChatApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        dev_screenshot(ctx);
        let p = vimora_ui::palette(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            // Alt+F4 & co. only fold it; the teacher decides when the chat ends.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.mode = Mode::Folded;
        }

        let hovered = ctx.input(|i| i.pointer.hover_pos().is_some());
        let typing = ctx.memory(|m| m.focused().is_some()) && !self.draft.is_empty();
        {
            let mut inbox = self.inbox.lock().unwrap();
            if std::mem::take(&mut inbox.new_message) {
                // A new message opens it — not over a full-screen program (a video, a
                // presentation): then the folded square just counts it.
                if self.mode == Mode::Folded && !fullscreen_app_active() {
                    self.mode = Mode::Open;
                    self.touched = false;
                }
                self.shown_at = Instant::now();
                self.scroll_down = true;
            }
            if self.mode == Mode::Open {
                inbox.unseen = 0;
            }
        }
        if self.mode == Mode::Open {
            if hovered || typing {
                self.touched = true;
                self.shown_at = Instant::now();
            } else if self.shown_at.elapsed() > if self.touched { LINGER } else { AUTO_HIDE } {
                self.mode = Mode::Folded;
            }
            ctx.request_repaint_after(Duration::from_millis(200));
            // Read receipt: open and on screen for a moment.
            let teacher = self
                .inbox
                .lock()
                .unwrap()
                .messages
                .iter()
                .filter(|m| !m.mine)
                .count();
            if teacher > self.read && (hovered || self.shown_at.elapsed() > READ_AFTER) {
                self.read = teacher;
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "READ").and_then(|_| out.flush());
            }
        }
        self.place(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(if self.mode == Mode::Open {
                p.panel
            } else {
                p.accent
            }))
            .show(ctx, |ui| match self.mode {
                Mode::Open => self.panel(ui, &p),
                Mode::Folded => self.folded(ui, &p),
            });
    }
}

impl ChatApp {
    /// The folded chat: a message icon and the unread count; a click opens it.
    fn folded(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let r = ui.max_rect();
        let resp = ui.interact(r, egui::Id::new("fold"), egui::Sense::click());
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            icons::MESSAGE_SQUARE_TEXT,
            egui::FontId::proportional(24.0),
            p.invert_fg,
        );
        let unseen = self.inbox.lock().unwrap().unseen;
        if unseen > 0 {
            let b = egui::Rect::from_min_size(
                egui::pos2(r.right() - 20.0, r.top()),
                egui::vec2(20.0, 18.0),
            );
            ui.painter().rect_filled(b, egui::Rounding::ZERO, p.danger);
            ui.painter().text(
                b.center(),
                egui::Align2::CENTER_CENTER,
                unseen.min(99).to_string(),
                egui::FontId::new(11.0, vimora_ui::mono_bold()),
                egui::Color32::WHITE,
            );
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.mode = Mode::Open;
            self.touched = true;
            self.shown_at = Instant::now();
            self.scroll_down = true;
        }
    }
}

/// Dev aid: `VIMORA_CHAT_SCREENSHOT=out.jpg` saves the panel after 3 s and exits.
fn dev_screenshot(ctx: &egui::Context) {
    let Ok(path) = std::env::var("VIMORA_CHAT_SCREENSHOT") else {
        return;
    };
    let t = ctx.input(|i| i.time);
    if (3.0..3.05).contains(&t) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }
    let shot = ctx.input(|i| {
        i.events.iter().find_map(|e| {
            if let egui::Event::Screenshot { image, .. } = e {
                Some(image.clone())
            } else {
                None
            }
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

/// Is a full-screen program in front (a video, a game, a presentation)? Then hovering the
/// screen edge must not pull the chat over it.
#[cfg(windows)]
fn fullscreen_app_active() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect,
    };
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return false;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, cls.as_mut_ptr(), cls.len() as i32).max(0) as usize;
        let class = String::from_utf16_lossy(&cls[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false; // the desktop itself
        }
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(fg, &mut r);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        // A maximised window leaves the taskbar free; full screen covers the whole monitor.
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn fullscreen_app_active() -> bool {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    let Ok((c, screen)) = x11rb::connect(None) else {
        return false;
    };
    let root = c.setup().roots[screen].root;
    let atom = |n: &str| {
        c.intern_atom(false, n.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    };
    (|| -> Option<bool> {
        let (active, state, full) = (
            atom("_NET_ACTIVE_WINDOW")?,
            atom("_NET_WM_STATE")?,
            atom("_NET_WM_STATE_FULLSCREEN")?,
        );
        let r = c
            .get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let win = r.value32()?.next()?;
        if win == 0 {
            return Some(false);
        }
        let s = c
            .get_property(false, win, state, AtomEnum::ATOM, 0, 32)
            .ok()?
            .reply()
            .ok()?;
        let fullscreen = s.value32()?.any(|a| a == full);
        Some(fullscreen)
    })()
    .unwrap_or(false)
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn fullscreen_app_active() -> bool {
    false
}

impl ChatApp {
    fn panel(&mut self, ui: &mut egui::Ui, p: &Palette) {
        const HEADER_H: f32 = 48.0;
        const COMPOSER_H: f32 = 72.0;
        let full = ui.max_rect();
        ui.painter().rect_stroke(
            full.shrink(0.5),
            egui::Rounding::ZERO,
            egui::Stroke::new(1.0_f32, p.line),
        );

        // Header
        let head = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), HEADER_H));
        ui.painter().hline(
            head.x_range(),
            head.bottom() - 0.5,
            egui::Stroke::new(1.0_f32, p.line),
        );
        ui.painter().text(
            egui::pos2(head.left() + 18.0, head.center().y),
            egui::Align2::LEFT_CENTER,
            icons::GRADUATION_CAP,
            egui::FontId::proportional(16.0),
            p.accent,
        );
        ui.painter().text(
            egui::pos2(head.left() + 42.0, head.center().y),
            egui::Align2::LEFT_CENTER,
            "O'qituvchi",
            egui::FontId::new(15.0, vimora_ui::bold()),
            p.strong,
        );
        // Fold into the corner (the chat itself stays until the teacher ends it).
        let fold = egui::Rect::from_min_max(
            egui::pos2(head.right() - HEADER_H, head.top()),
            egui::pos2(head.right(), head.bottom() - 1.0),
        );
        let fr = ui.interact(fold, egui::Id::new("fold-btn"), egui::Sense::click());
        if fr.hovered() {
            ui.painter()
                .rect_filled(fold, egui::Rounding::ZERO, p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        ui.painter().text(
            fold.center(),
            egui::Align2::CENTER_CENTER,
            icons::CHEVRON_RIGHT,
            egui::FontId::proportional(18.0),
            p.muted,
        );
        if fr.clicked() {
            self.mode = Mode::Folded;
        }
        // Messages
        let list = egui::Rect::from_min_max(
            egui::pos2(full.left(), head.bottom()),
            egui::pos2(full.right(), full.bottom() - COMPOSER_H),
        );
        ui.painter()
            .rect_filled(list.shrink(1.0), egui::Rounding::ZERO, p.bg);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(list), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                        .show(ui, |ui| {
                            ui.set_width(list.width() - 28.0);
                            let max_w = (list.width() - 28.0) * 0.8;
                            for m in &self.inbox.lock().unwrap().messages {
                                let side = if m.mine {
                                    egui::Layout::top_down(egui::Align::Max)
                                } else {
                                    egui::Layout::top_down(egui::Align::Min)
                                };
                                ui.with_layout(side, |ui| {
                                    let (fill, ink, meta) = if m.mine {
                                        (
                                            p.accent,
                                            egui::Color32::WHITE,
                                            egui::Color32::from_white_alpha(200),
                                        )
                                    } else {
                                        (p.card_hover, p.text, p.faint)
                                    };
                                    egui::Frame::none()
                                        .fill(fill)
                                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                        .show(ui, |ui| {
                                            ui.set_max_width(max_w);
                                            ui.with_layout(
                                                egui::Layout::top_down(egui::Align::Min),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(&m.text)
                                                            .size(14.0)
                                                            .color(ink),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&m.time)
                                                            .size(10.5)
                                                            .color(meta),
                                                    );
                                                },
                                            );
                                        });
                                });
                                ui.add_space(8.0);
                            }
                            // Opening or a new message: show the newest one.
                            if std::mem::take(&mut self.scroll_down) {
                                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                            }
                        });
                });
        });

        // Composer
        let comp = egui::Rect::from_min_max(
            egui::pos2(full.left(), full.bottom() - COMPOSER_H),
            full.max,
        );
        ui.painter().hline(
            comp.x_range(),
            comp.top() + 0.5,
            egui::Stroke::new(1.0_f32, p.line),
        );
        // Input and send button: one height, centred in the composer.
        let inner =
            egui::Rect::from_center_size(comp.center(), egui::vec2(comp.width() - 28.0, CONTROL_H));
        let send_rect = egui::Rect::from_min_size(
            egui::pos2(inner.right() - CONTROL_H, inner.top()),
            egui::vec2(CONTROL_H, CONTROL_H),
        );
        let edit_rect = egui::Rect::from_min_max(
            inner.min,
            egui::pos2(send_rect.left() - 8.0, inner.bottom()),
        );
        let focused = ui.memory(|m| m.has_focus(egui::Id::new("chat-input")));
        ui.painter().rect(
            edit_rect,
            egui::Rounding::ZERO,
            p.sunken,
            egui::Stroke::new(1.0_f32, if focused { p.accent } else { p.line_strong }),
        );
        let field = edit_rect.shrink2(egui::vec2(10.0, 1.0));
        let edit = ui.put(
            field,
            egui::TextEdit::multiline(&mut self.draft)
                .id(egui::Id::new("chat-input"))
                .frame(false)
                .margin(egui::vec2(0.0, 0.0))
                .hint_text("Javob yozing…")
                .desired_rows(1)
                .vertical_align(egui::Align::Center)
                .desired_width(field.width())
                .min_size(field.size())
                .return_key(Some(egui::KeyboardShortcut::new(
                    egui::Modifiers::SHIFT,
                    egui::Key::Enter,
                ))),
        );
        let enter =
            edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        let send = ui.interact(send_rect, egui::Id::new("chat-send"), egui::Sense::click());
        let ready = !self.draft.trim().is_empty();
        let fill = if !ready {
            p.hover
        } else if send.hovered() {
            p.accent_ink
        } else {
            p.accent
        };
        ui.painter()
            .rect_filled(send_rect, egui::Rounding::ZERO, fill);
        ui.painter().text(
            send_rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::SEND,
            egui::FontId::proportional(17.0),
            if ready { egui::Color32::WHITE } else { p.faint },
        );
        if (send.clicked() || enter) && ready {
            self.send();
            edit.request_focus();
        }
    }
}
