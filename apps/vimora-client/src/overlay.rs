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
    Lock { title: String, message: String },
    Show,
    Frame { jpeg: Vec<u8> },
}

#[derive(Clone)]
enum Mode {
    Lock { title: String, message: String },
    Show,
}

struct Shared {
    mode: Mode,
    frame: Option<egui::ColorImage>,
    frame_dirty: bool,
}

pub fn run() -> anyhow::Result<()> {
    let shared = Arc::new(Mutex::new(Shared {
        mode: Mode::Lock {
            title: "Ekran qulflangan".into(),
            message: String::new(),
        },
        frame: None,
        frame_dirty: false,
    }));

    #[cfg(windows)]
    input_block::install();

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_fullscreen(true)
            .with_always_on_top()
            .with_decorations(false)
            .with_taskbar(false),
        ..Default::default()
    };

    let s2 = shared.clone();
    eframe::run_native(
        "vimora-overlay",
        opts,
        Box::new(move |cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            vimora_ui::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            spawn_stdin_reader(s2.clone(), cc.egui_ctx.clone());
            Ok(Box::new(OverlayApp {
                shared: s2,
                texture: None,
                last_focus: Instant::now(),
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
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refuse to close and keep ourselves on top.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.last_focus.elapsed() > Duration::from_secs(2) {
            self.last_focus = Instant::now();
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        ctx.request_repaint_after(Duration::from_millis(500));

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
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(p.bg))
                    .show(ctx, |ui| {
                        lock_screen(ui, &title, &message);
                    });
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

fn lock_screen(ui: &mut egui::Ui, title: &str, message: &str) {
    let p = vimora_ui::DARK;
    let rect = ui.max_rect();
    // Square accent tile with soft square halos (the design has no rounded shapes).
    let c = rect.center() - egui::vec2(0.0, 90.0);
    for (r, a) in [(220.0, 10u8), (150.0, 16), (100.0, 26)] {
        ui.painter().rect_filled(
            egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
            egui::Rounding::ZERO,
            p.accent.gamma_multiply(a as f32 / 255.0),
        );
    }
    ui.painter().rect_filled(
        egui::Rect::from_center_size(c, egui::vec2(128.0, 128.0)),
        egui::Rounding::ZERO,
        p.accent,
    );
    ui.painter().text(
        c,
        egui::Align2::CENTER_CENTER,
        icons::LOCK,
        egui::FontId::proportional(56.0),
        egui::Color32::WHITE,
    );

    let mut y = c.y + 110.0;
    let title_galley = ui.painter().layout(
        if title.is_empty() {
            "Ekran qulflangan".into()
        } else {
            title.to_string()
        },
        egui::FontId::proportional(40.0),
        p.text,
        rect.width() * 0.8,
    );
    ui.painter().galley(
        egui::pos2(rect.center().x - title_galley.size().x / 2.0, y),
        title_galley.clone(),
        p.text,
    );
    y += title_galley.size().y + 16.0;

    let msg = if message.is_empty() {
        "Iltimos, o'qituvchini tinglang."
    } else {
        message
    };
    let msg_galley = ui.painter().layout(
        msg.to_string(),
        egui::FontId::proportional(22.0),
        p.muted,
        rect.width() * 0.7,
    );
    ui.painter().galley(
        egui::pos2(rect.center().x - msg_galley.size().x / 2.0, y),
        msg_galley,
        p.muted,
    );

    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 40.0),
        egui::Align2::CENTER_CENTER,
        format!("{}  Vimora", icons::GRADUATION_CAP),
        egui::FontId::proportional(16.0),
        p.muted,
    );
}

/// Windows: swallow all physical keyboard and mouse input while the overlay is alive.
/// (Ctrl+Alt+Del is handled by the OS and can never be blocked.)
#[cfg(windows)]
mod input_block {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    unsafe extern "system" fn swallow(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            return 1;
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    pub fn install() {
        unsafe {
            let module = GetModuleHandleW(null());
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(swallow), module, 0);
            SetWindowsHookExW(WH_MOUSE_LL, Some(swallow), module, 0);
        }
    }
}
