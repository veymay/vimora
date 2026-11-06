//! The single header line: logo, equal-width menu cells, status and quick settings.

use super::*;

impl ConsoleApp {
    pub(super) fn header(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let mut clicked: Option<Menu> = None;
        let unread = self.hub.lock().unwrap().unread;
        egui::TopBottomPanel::top("header")
            .exact_height(HEADER_H)
            .frame(
                Frame::none()
                    .fill(p.panel)
                    .stroke(Stroke::new(1.0_f32, p.line)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                ui.horizontal_centered(|ui| {
                    // Logo cell
                    ui.add_space(18.0);
                    let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                    ui.painter().rect_filled(r, Rounding::ZERO, p.accent);
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        icons::GRADUATION_CAP,
                        FontId::proportional(18.0),
                        Color32::WHITE,
                    );
                    ui.add_space(10.0);
                    let g = ui_kit::caps(ui, "Vimora", 17.0, ui_kit::bold(), p.strong);
                    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, p.strong);
                    ui.add_space(18.0);
                    ui_kit::rule(ui, HEADER_H);

                    let items: [(Menu, &str, &str, Option<Color32>); 12] = [
                        (Menu::Lock, icons::LOCK, "Qulflash", Some(p.warning)),
                        (Menu::Unlock, icons::LOCK_OPEN, "Ochish", None),
                        (
                            Menu::Show,
                            icons::CAST,
                            if self.showing {
                                "To'xtatish"
                            } else {
                                "Namoyish"
                            },
                            self.showing.then_some(p.danger),
                        ),
                        (Menu::Message, icons::MESSAGE_SQUARE_TEXT, "Xabar", None),
                        (Menu::Launch, icons::ROCKET, "Ilova", None),
                        (Menu::File, icons::FILE_UP, "Fayl", None),
                        (Menu::Net, icons::GLOBE_LOCK, "Internet", Some(p.accent_ink)),
                        (Menu::Apps, icons::BAN, "Bloklash", None),
                        (Menu::Freeze, icons::SNOWFLAKE, "Shadow", Some(p.accent_ink)),
                        (Menu::Updates, icons::DOWNLOAD, "Yangilash", None),
                        (Menu::Power, icons::POWER, "Quvvat", Some(p.danger)),
                        (Menu::Activity, icons::ACTIVITY, "Jurnal", None),
                    ];
                    // Every item gets the same width; on narrow windows they shrink to icons.
                    let right_w = 240.0;
                    let item_w = ((ui.available_width() - right_w) / items.len() as f32)
                        .clamp(52.0, 94.0)
                        .floor();
                    for (m, ic, label, tint) in items {
                        let active = match m {
                            Menu::Show => self.showing,
                            Menu::Activity => matches!(self.dialog, Some(Dialog::Activity)),
                            Menu::Message => matches!(self.dialog, Some(Dialog::Message { .. })),
                            _ => false,
                        };
                        let resp = ui_kit::nav_item(ui, ic, label, item_w, HEADER_H, active, tint);
                        if resp.clicked() {
                            clicked = Some(m);
                        }
                        // Unread student replies on the chat item.
                        if m == Menu::Message && unread > 0 {
                            let g = ui_kit::caps(
                                ui,
                                &unread.min(99).to_string(),
                                9.0,
                                ui_kit::mono_bold(),
                                Color32::WHITE,
                            );
                            let r = Rect::from_min_size(
                                pos2(resp.rect.center().x + 8.0, resp.rect.top() + 9.0),
                                vec2((g.size().x + 8.0).max(16.0), 15.0),
                            );
                            ui.painter().rect_filled(r, Rounding::ZERO, p.accent);
                            ui.painter()
                                .galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui_kit::icon_cell(ui, icons::SETTINGS, HEADER_H, false)
                            .on_hover_text("Sozlamalar")
                            .clicked()
                        {
                            let c = self.cfg.read().unwrap().clone();
                            self.open_dialog(Dialog::Settings(Box::new(c)));
                        }
                        ui_kit::rule(ui, HEADER_H);
                        let (ic, tip) = match self.theme {
                            ThemeMode::Dark => (icons::SUN, "Yorug' rejim"),
                            ThemeMode::Light => (icons::MOON, "Qorong'i rejim"),
                        };
                        if ui_kit::icon_cell(ui, ic, HEADER_H, false)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.set_theme(ctx, self.theme.toggled());
                        }
                        ui_kit::rule(ui, HEADER_H);
                        ui.add_space(16.0);
                        // "12 kompyuter" with "● 9 onlayn" underneath.
                        let online = all.len();
                        let total = self.cfg.read().unwrap().known.len().max(online);
                        let font = |t: &str, c: Color32| {
                            ui_kit::caps(ui, t, ui_kit::HUD, ui_kit::mono_bold(), c)
                        };
                        let top = font(&format!("{total} kompyuter"), p.text);
                        let on_col = if online > 0 { p.success } else { p.faint };
                        let bottom = font(&format!("{online} onlayn"), on_col);
                        let gap = 5.0;
                        let w = top.size().x.max(bottom.size().x);
                        let (r, _) = ui.allocate_exact_size(vec2(w, HEADER_H), Sense::hover());
                        let block = top.size().y + gap + bottom.size().y;
                        let y0 = r.center().y - block / 2.0;
                        // Both lines right-aligned to the same edge.
                        let by = y0 + top.size().y + gap;
                        ui.painter()
                            .galley(pos2(r.right() - top.size().x, y0), top, p.text);
                        ui.painter()
                            .galley(pos2(r.right() - bottom.size().x, by), bottom, on_col);
                    });
                });
            });
        if let Some(m) = clicked {
            self.menu(m, all);
        }
    }
}
