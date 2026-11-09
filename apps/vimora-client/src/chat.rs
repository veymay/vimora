//! Student chat panel (`vimora-client chat`), docked to the right screen edge like AIMP's
//! docked mode.
//!
//! * A new teacher message slides the panel in and keeps it open (**pinned**).
//! * ✕ hides it into a thin strip on the screen edge (**collapsed**).
//! * Hovering the strip slides it out (**peek**); moving the mouse away hides it again.
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
const STRIP_W: f32 = 20.0;
const PEEK_LINGER: Duration = Duration::from_millis(700);
const SLIDE_SECS: f32 = 0.18;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pinned,
    Peek,
    Collapsed,
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
            .with_min_inner_size([STRIP_W, 200.0])
            .with_inner_size([STRIP_W, 640.0])
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
                mode: Mode::Pinned,
                width: STRIP_W,
                last_geometry: None,
                left_at: None,
                draft: String::new(),
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
    /// Current (animated) width.
    width: f32,
    last_geometry: Option<(f32, f32, f32)>,
    /// When the pointer left the panel while peeking.
    left_at: Option<Instant>,
    draft: String,
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

    /// Keeps the window glued to the right screen edge, `width` wide.
    fn place(&mut self, ctx: &egui::Context) {
        // Full height of the usable screen: the right edge, top to bottom, without covering the
        // taskbar (Windows reports the work area; elsewhere the whole monitor is used).
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));
        let ppp = ctx.pixels_per_point().max(0.5);
        let (left, top, right, bottom) = work_area()
            .map(|(l, t, r, b)| (l / ppp, t / ppp, r / ppp, b / ppp))
            .unwrap_or((0.0, 0.0, monitor.x, monitor.y));
        let h = (bottom - top).round();
        let x = (right - self.width).round();
        let y = top.round();
        let _ = left;
        let geo = (x, y, self.width.round());
        if self.last_geometry != Some(geo) {
            let first = self.last_geometry.is_none();
            self.last_geometry = Some(geo);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(geo.2, h)));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            if first {
                // Placed at the edge (still a thin strip): show it, then it slides out.
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        dev_screenshot(ctx);
        let p = vimora_ui::palette(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            // Alt+F4 & co. only hide it; the teacher decides when the chat ends.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.mode = Mode::Collapsed;
        }

        // State machine.
        {
            let mut inbox = self.inbox.lock().unwrap();
            if std::mem::take(&mut inbox.new_message) {
                self.mode = Mode::Pinned;
            }
            if self.mode != Mode::Collapsed {
                inbox.unseen = 0;
            }
        }
        let hovered = ctx.input(|i| i.pointer.hover_pos().is_some());
        match self.mode {
            Mode::Collapsed if hovered => {
                self.mode = Mode::Peek;
                self.left_at = None;
            }
            Mode::Peek if !hovered => {
                let t = *self.left_at.get_or_insert_with(Instant::now);
                if t.elapsed() > PEEK_LINGER {
                    self.mode = Mode::Collapsed;
                    self.left_at = None;
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Mode::Peek => self.left_at = None,
            _ => {}
        }

        // Slide.
        let target = if self.mode == Mode::Collapsed {
            STRIP_W
        } else {
            OPEN_W
        };
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let step = (OPEN_W - STRIP_W) * dt / SLIDE_SECS;
        if (self.width - target).abs() > 0.5 {
            self.width = if self.width < target {
                (self.width + step).min(target)
            } else {
                (self.width - step).max(target)
            };
            ctx.request_repaint();
        }
        self.place(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.panel))
            .show(ctx, |ui| {
                if self.width < OPEN_W * 0.5 {
                    strip(ui, &p, self.inbox.lock().unwrap().unseen);
                } else {
                    self.panel(ui, &p);
                }
            });
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

/// The collapsed tab on the screen edge.
fn strip(ui: &mut egui::Ui, p: &Palette, unseen: usize) {
    let r = ui.max_rect();
    ui.painter().rect_filled(r, egui::Rounding::ZERO, p.accent);
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        icons::MESSAGE_CIRCLE,
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );
    if unseen > 0 {
        ui.painter().text(
            egui::pos2(r.center().x, r.center().y + 22.0),
            egui::Align2::CENTER_CENTER,
            unseen.min(99).to_string(),
            egui::FontId::new(11.0, vimora_ui::mono_bold()),
            egui::Color32::WHITE,
        );
    }
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
        let close = egui::Rect::from_min_size(
            egui::pos2(head.right() - HEADER_H, head.top()),
            egui::vec2(HEADER_H, HEADER_H),
        );
        let resp = ui.interact(close, egui::Id::new("chat-close"), egui::Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(close, egui::Rounding::ZERO, p.hover);
        }
        ui.painter().vline(
            close.left(),
            close.y_range(),
            egui::Stroke::new(1.0_f32, p.line),
        );
        ui.painter().text(
            close.center(),
            egui::Align2::CENTER_CENTER,
            icons::X,
            egui::FontId::proportional(16.0),
            p.muted,
        );
        if resp
            .on_hover_text("Yashirish — chetga sichqonchani olib borsangiz yana ochiladi")
            .clicked()
        {
            self.mode = Mode::Collapsed;
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
        let inner = comp.shrink2(egui::vec2(14.0, 13.0));
        let send_rect = egui::Rect::from_min_size(
            egui::pos2(inner.right() - inner.height(), inner.top()),
            egui::vec2(inner.height(), inner.height()),
        );
        let edit_rect = egui::Rect::from_min_max(
            inner.min,
            egui::pos2(send_rect.left() - 8.0, inner.bottom()),
        );
        let edit = ui.put(
            edit_rect,
            vimora_ui::text_area(&mut self.draft)
                .hint_text("Javob yozing…")
                .desired_rows(1)
                .vertical_align(egui::Align::Center)
                .desired_width(edit_rect.width())
                .min_size(edit_rect.size())
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
