//! Small always-on-top message window shown when the teacher sends a message.
//!
//! With `--reply` it has a reply box (Telegram-style): each reply is printed to stdout as one
//! JSON string per line, which the agent forwards to the teacher's chat.

use eframe::egui;
use std::io::Write;
use vimora_ui::{icons, ThemeMode};

pub fn run(title: &str, text: &str, reply: bool) -> anyhow::Result<()> {
    let (title, text) = (title.to_string(), text.to_string());
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora — xabar")
            .with_inner_size([460.0, if reply { 300.0 } else { 240.0 }])
            .with_resizable(false)
            .with_always_on_top(),
        centered: true,
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-popup",
        opts,
        Box::new(move |cc| {
            vimora_ui::install_fonts(&cc.egui_ctx);
            let mode = if cc.egui_ctx.style().visuals.dark_mode {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            vimora_ui::apply_theme(&cc.egui_ctx, mode);
            Ok(Box::new(Popup {
                title,
                text,
                reply,
                draft: String::new(),
                sent: 0,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("popup: {e}"))
}

struct Popup {
    title: String,
    text: String,
    reply: bool,
    draft: String,
    sent: usize,
}

impl Popup {
    fn send(&mut self) {
        let t = self.draft.trim();
        if t.is_empty() {
            return;
        }
        let mut out = std::io::stdout().lock();
        if writeln!(out, "{}", serde_json::to_string(t).unwrap_or_default())
            .and_then(|_| out.flush())
            .is_ok()
        {
            self.sent += 1;
            self.draft.clear();
        }
    }
}

impl eframe::App for Popup {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let p = vimora_ui::palette(ctx);
        egui::TopBottomPanel::bottom("actions")
            .frame(egui::Frame::none().fill(p.panel).inner_margin(14.0))
            .show(ctx, |ui| {
                if self.reply {
                    ui.horizontal(|ui| {
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.draft)
                                .hint_text("Javob yozing…")
                                .desired_width(ui.available_width() - 132.0),
                        );
                        let enter =
                            edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if vimora_ui::primary_button(ui, icons::SEND, "Yuborish").clicked() || enter
                        {
                            self.send();
                            edit.request_focus();
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    if self.sent > 0 {
                        ui.label(vimora_ui::muted(
                            ui,
                            format!("{} Javob yuborildi", icons::CHECK_CHECK),
                        ));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if vimora_ui::button(
                            ui,
                            Some(icons::CHECK),
                            "Tushundim",
                            vimora_ui::Kind::Outline,
                        )
                        .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.panel).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(vimora_ui::icon(icons::MESSAGE_SQUARE_TEXT, 26.0, p.accent));
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&self.title)
                                .size(17.0)
                                .family(vimora_ui::bold()),
                        );
                        ui.label(vimora_ui::muted(ui, "O'qituvchidan"));
                    });
                });
                ui.add_space(10.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(egui::RichText::new(&self.text).size(15.0));
                });
            });
    }
}
