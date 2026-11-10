//! Modal shown when a student searches for a forbidden keyword: the whole screen is dimmed, the
//! browser can't be used until the student acknowledges it ("Tushundim", Enter or Esc).

use eframe::egui;
use std::time::{Duration, Instant};
use vimora_ui::{icons, ThemeMode};

pub fn run(word: &str) -> anyhow::Result<()> {
    let word = word.to_string();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_fullscreen(true)
            .with_always_on_top()
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-blocked",
        opts,
        Box::new(move |cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            vimora_ui::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            Ok(Box::new(Blocked {
                word,
                last_focus: Instant::now(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("blocked: {e}"))
}

struct Blocked {
    word: String,
    last_focus: Instant,
}

impl eframe::App for Blocked {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Stay in front until acknowledged (only repair focus when it was actually lost).
        if self.last_focus.elapsed() > Duration::from_millis(600) {
            self.last_focus = Instant::now();
            if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(600));
        let p = vimora_ui::DARK;
        let mut close =
            ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_black_alpha(215)))
            .show(ctx, |ui| {
                let screen = ui.max_rect();
                let card = egui::Rect::from_center_size(screen.center(), egui::vec2(460.0, 300.0));
                ui.painter().rect(
                    card,
                    egui::Rounding::ZERO,
                    p.card,
                    egui::Stroke::new(1.0_f32, p.line_strong),
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(card.min, egui::vec2(card.width(), 4.0)),
                    egui::Rounding::ZERO,
                    p.danger,
                );
                let mut c = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(card.shrink2(egui::vec2(32.0, 28.0)))
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                c.add_space(6.0);
                c.label(egui::RichText::new(icons::SHIELD_BAN).size(46.0).color(p.danger));
                c.add_space(12.0);
                c.label(
                    egui::RichText::new("Bu qidiruv taqiqlangan")
                        .family(vimora_ui::bold())
                        .size(21.0)
                        .color(p.strong),
                );
                c.add_space(10.0);
                c.label(
                    egui::RichText::new(format!(
                        "\u{201c}{}\u{201d} so'zi bilan qidirish mumkin emas.\nSahifa yopildi va o'qituvchiga xabar berildi.",
                        self.word
                    ))
                    .size(14.0)
                    .color(p.text),
                );
                c.add_space(22.0);
                if vimora_ui::primary_button(&mut c, icons::CHECK, "Tushundim").clicked() {
                    close = true;
                }
            });
        if close {
            std::process::exit(0);
        }
    }
}
