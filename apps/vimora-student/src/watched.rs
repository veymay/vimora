//! A small pill at the top of the student's screen while the teacher watches it live
//! ("O'qituvchi ekraningizni ko'rmoqda"). It never takes focus and lets clicks through.

use eframe::egui;
use vimora_ui::ThemeMode;

const W: f32 = 300.0;
const H: f32 = 30.0;

pub fn run() -> anyhow::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_resizable(false)
            .with_inner_size([W, H])
            .with_position([0.0, 0.0]),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-watched",
        opts,
        Box::new(|cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            vimora_ui::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            Ok(Box::new(Pill { placed: false }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("watched: {e}"))
}

struct Pill {
    placed: bool,
}

impl eframe::App for Pill {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        if !self.placed {
            if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    (m.x - W) / 2.0,
                    6.0,
                )));
                self.placed = true;
            }
        }
        let p = vimora_ui::DARK;
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().rect(
                    r,
                    egui::Rounding::ZERO,
                    egui::Color32::from_black_alpha(215),
                    egui::Stroke::new(1.0_f32, p.accent),
                );
                // A slowly pulsing dot.
                let t = ui.input(|i| i.time) as f32;
                let a = 0.55 + 0.45 * (t * 2.5).sin().abs();
                ui.painter().circle_filled(
                    egui::pos2(r.left() + 16.0, r.center().y),
                    4.5,
                    p.danger.gamma_multiply(a),
                );
                ui.painter().text(
                    egui::pos2(r.left() + 30.0, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    "O'qituvchi ekraningizni ko'rmoqda",
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
    }
}

/// Agent side: one indicator process at a time.
static CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// Shows or hides the indicator.
pub fn set(on: bool) {
    let mut c = CHILD.lock().unwrap();
    // Gone by itself (closed, crashed): forget it.
    if c.as_mut()
        .is_some_and(|ch| !matches!(ch.try_wait(), Ok(None)))
    {
        *c = None;
    }
    match (on, c.is_some()) {
        (true, false) => {
            *c = std::env::current_exe()
                .ok()
                .and_then(|exe| std::process::Command::new(exe).arg("watched").spawn().ok());
        }
        (false, true) => {
            if let Some(mut ch) = c.take() {
                let _ = ch.kill();
                let _ = ch.wait();
            }
        }
        _ => {}
    }
}
