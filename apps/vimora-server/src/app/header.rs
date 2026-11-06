//! The single header line: logo, six menu cells (three of them open a short dropdown), and on the
//! right the computer count, journal, update badge, theme and settings.

use super::*;
use vimora_ui::{tf, tr};

/// Menu cells that open a dropdown instead of acting at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum NavGroup {
    /// Website, program, file.
    Send,
    /// Internet, applications.
    Restrict,
    /// Shadow, updates, power.
    System,
}

enum Cell {
    Act(Menu),
    Group(NavGroup),
}

impl ConsoleApp {
    pub(super) fn header(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let mut clicked: Option<Menu> = None;
        let mut open_group: Option<(NavGroup, Rect)> = None;
        let unread = self.hub.lock().unwrap().unread;
        let attention = all.iter().any(|v| v.help.is_some() || v.alert.is_some());
        let targets = self.targets(all);
        let all_locked = !targets.is_empty()
            && targets
                .iter()
                .all(|id| all.iter().any(|v| v.id == *id && v.status.locked));
        let update = self.updater.available();
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
                    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                    ui_kit::logo(ui.painter(), r);
                    ui.add_space(11.0);
                    let g = ui_kit::caps(ui, "Vimora", 17.0, ui_kit::bold(), p.strong);
                    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, p.strong);
                    ui.add_space(20.0);
                    ui_kit::rule(ui, HEADER_H);

                    let lock = if all_locked {
                        (icons::LOCK_OPEN, tr("unlock|Ochish"), None)
                    } else {
                        (icons::LOCK, tr("Qulflash"), Some(p.warning))
                    };
                    let show = if self.showing {
                        (icons::CAST, tr("To'xtatish"), Some(p.danger))
                    } else {
                        (icons::CAST, tr("Namoyish"), None)
                    };
                    let items: [(Cell, &str, &str, Option<Color32>); 6] = [
                        (Cell::Act(Menu::Lock), lock.0, lock.1, lock.2),
                        (Cell::Act(Menu::Show), show.0, show.1, show.2),
                        (
                            Cell::Act(Menu::Message),
                            icons::MESSAGE_SQUARE_TEXT,
                            tr("Chat"),
                            None,
                        ),
                        (
                            Cell::Group(NavGroup::Send),
                            icons::SEND,
                            tr("Yuborish"),
                            None,
                        ),
                        (
                            Cell::Group(NavGroup::Restrict),
                            icons::SHIELD_BAN,
                            tr("Cheklash"),
                            Some(p.accent_ink),
                        ),
                        (
                            Cell::Group(NavGroup::System),
                            icons::MONITOR_COG,
                            tr("Tizim"),
                            None,
                        ),
                    ];
                    let right_w = 300.0;
                    let item_w = ((ui.available_width() - right_w) / items.len() as f32)
                        .clamp(72.0, 118.0)
                        .floor();
                    for (cell, ic, label, tint) in items {
                        let active = match &cell {
                            Cell::Act(Menu::Show) => self.showing,
                            Cell::Act(Menu::Lock) => all_locked,
                            Cell::Act(Menu::Message) => {
                                matches!(self.dialog, Some(Dialog::Message { .. }))
                            }
                            Cell::Group(g) => self.nav_open == Some(*g),
                            _ => false,
                        };
                        let resp = ui_kit::nav_item(ui, ic, label, item_w, HEADER_H, active, tint);
                        if let Cell::Group(g) = &cell {
                            if self.nav_open == Some(*g) {
                                self.nav_anchor = resp.rect; // follows window resizes
                            }
                            // Small chevron: this cell opens a menu.
                            ui.painter().text(
                                pos2(resp.rect.right() - 13.0, resp.rect.top() + HEADER_H * 0.36),
                                Align2::CENTER_CENTER,
                                icons::CHEVRON_DOWN,
                                FontId::proportional(11.0),
                                p.faint,
                            );
                            if resp.clicked() {
                                open_group = Some((*g, resp.rect));
                            }
                        } else if let Cell::Act(m) = cell {
                            if resp.clicked() {
                                clicked = Some(m);
                            }
                            if m == Menu::Message && unread > 0 {
                                counter(ui, p, resp.rect, unread);
                            }
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui_kit::icon_cell(
                            ui,
                            icons::SETTINGS,
                            HEADER_H,
                            matches!(self.dialog, Some(Dialog::Settings(_))),
                        )
                        .on_hover_text(tr("Sozlamalar"))
                        .clicked()
                        {
                            self.open_dialog(Dialog::Settings(SettingsTab::General));
                        }
                        ui_kit::rule(ui, HEADER_H);
                        let (ic, tip) = match self.theme {
                            ThemeMode::Dark => (icons::SUN, tr("Yorug' rejim")),
                            ThemeMode::Light => (icons::MOON, tr("Qorong'i rejim")),
                        };
                        if ui_kit::icon_cell(ui, ic, HEADER_H, false)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            let t = self.theme.toggled();
                            {
                                let mut c = self.cfg.write().unwrap();
                                c.theme_system = false;
                                c.dark_mode = t == ThemeMode::Dark;
                                config::save(&c);
                            }
                            self.set_theme(ctx, t);
                        }
                        ui_kit::rule(ui, HEADER_H);
                        let journal_open = matches!(self.dialog, Some(Dialog::Activity));
                        let r = ui_kit::icon_cell(
                            ui,
                            if attention {
                                icons::BELL_DOT
                            } else {
                                icons::BELL
                            },
                            HEADER_H,
                            journal_open,
                        )
                        .on_hover_text(tr("Jurnal"));
                        if attention {
                            ui.painter().circle_filled(
                                pos2(r.rect.center().x + 6.0, r.rect.center().y - 7.0),
                                3.5,
                                p.warning,
                            );
                        }
                        if r.clicked() {
                            clicked = Some(Menu::Activity);
                        }
                        ui_kit::rule(ui, HEADER_H);
                        if let Some(v) = &update {
                            let r = ui_kit::icon_cell(ui, icons::CIRCLE_ARROW_UP, HEADER_H, true)
                                .on_hover_text(tf!("Yangi versiya: {v}", v = v));
                            if r.clicked() {
                                self.open_dialog(Dialog::Settings(SettingsTab::Updates));
                            }
                            ui_kit::rule(ui, HEADER_H);
                        }
                        ui.add_space(16.0);
                        // "12 kompyuter" with "● 9 onlayn" underneath.
                        let online = all.len();
                        let total = self.cfg.read().unwrap().known.len().max(online);
                        let font = |t: &str, c: Color32| {
                            ui_kit::caps(ui, t, ui_kit::HUD, ui_kit::mono_bold(), c)
                        };
                        let top = font(&tf!("{n} kompyuter", n = total), p.text);
                        let on_col = if online > 0 { p.success } else { p.faint };
                        let bottom = font(&tf!("{n} onlayn", n = online), on_col);
                        let gap = 5.0;
                        let w = top.size().x.max(bottom.size().x);
                        let (r, _) = ui.allocate_exact_size(vec2(w, HEADER_H), Sense::hover());
                        let block = top.size().y + gap + bottom.size().y;
                        let y0 = r.center().y - block / 2.0;
                        let by = y0 + top.size().y + gap;
                        ui.painter()
                            .galley(pos2(r.right() - top.size().x, y0), top, p.text);
                        ui.painter()
                            .galley(pos2(r.right() - bottom.size().x, by), bottom, on_col);
                    });
                });
            });
        if let Some((g, rect)) = open_group {
            if self.nav_open == Some(g) {
                self.nav_open = None;
            } else {
                self.nav_open = Some(g);
                self.nav_anchor = rect;
            }
        }
        if let Some(m) = self.nav_menu(ctx, p) {
            clicked = Some(m);
        }
        if let Some(m) = clicked {
            self.nav_open = None;
            self.menu(m, all);
        }
    }

    /// The open dropdown under its menu cell. Closes on a pick, Esc or a click elsewhere.
    fn nav_menu(&mut self, ctx: &egui::Context, p: &Palette) -> Option<Menu> {
        let g = self.nav_open?;
        let rows: Vec<(Menu, &str, &str, &str, Option<Color32>)> = match g {
            NavGroup::Send => vec![
                (
                    Menu::Web,
                    icons::GLOBE,
                    tr("Sayt ochish"),
                    tr("Barcha ekranlarda bir sahifa"),
                    None,
                ),
                (
                    Menu::Program,
                    icons::ROCKET,
                    tr("Dastur ishga tushirish"),
                    tr("Masalan, Word yoki Scratch"),
                    None,
                ),
                (
                    Menu::File,
                    icons::FILE_UP,
                    tr("Fayl yuborish"),
                    tr("Dars materiali, topshiriq"),
                    None,
                ),
            ],
            NavGroup::Restrict => vec![
                (
                    Menu::Net,
                    icons::GLOBE_LOCK,
                    tr("Internet"),
                    tr("Saytlar, kalit so'zlar, SafeSearch"),
                    None,
                ),
                (
                    Menu::Apps,
                    icons::BAN,
                    tr("Dasturlarni bloklash"),
                    tr("O'yinlar, messenjerlar"),
                    None,
                ),
            ],
            NavGroup::System => vec![
                (
                    Menu::Freeze,
                    icons::SNOWFLAKE,
                    tr("Shadow rejim"),
                    tr("Qayta yuklashda toza holat"),
                    Some(p.accent_ink),
                ),
                (
                    Menu::Updates,
                    icons::DOWNLOAD,
                    tr("Yangilanishlar"),
                    tr("Windows, dasturlar, Vimora"),
                    None,
                ),
                (
                    Menu::Power,
                    icons::POWER,
                    tr("Quvvat"),
                    tr("O'chirish, qayta yuklash"),
                    Some(p.danger),
                ),
            ],
        };
        let anchor = self.nav_anchor;
        let width = anchor.width().max(280.0);
        let mut picked = None;
        let area = egui::Area::new(egui::Id::new("nav-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(anchor.left(), anchor.bottom()))
            .show(ctx, |ui| {
                Frame::none()
                    .fill(p.card)
                    .stroke(Stroke::new(1.0_f32, p.line))
                    .shadow(egui::epaint::Shadow {
                        offset: vec2(0.0, 10.0),
                        blur: 28.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(if p.dark { 110 } else { 30 }),
                    })
                    .inner_margin(Margin::symmetric(0.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (m, ic, title, hint, tint) in &rows {
                            if ui_kit::menu_row(ui, ic, title, hint, *tint).clicked() {
                                picked = Some(*m);
                            }
                        }
                    });
            });
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| !area.response.rect.contains(pos) && !anchor.contains(pos))
        });
        if picked.is_none() && (outside || ctx.input(|i| i.key_pressed(Key::Escape))) {
            self.nav_open = None;
        }
        picked
    }
}

/// Unread-replies counter on a menu cell.
fn counter(ui: &egui::Ui, p: &Palette, rect: Rect, n: usize) {
    let g = ui_kit::caps(
        ui,
        &n.min(99).to_string(),
        9.0,
        ui_kit::mono_bold(),
        Color32::WHITE,
    );
    let r = Rect::from_min_size(
        pos2(rect.center().x + 8.0, rect.top() + 9.0),
        vec2((g.size().x + 8.0).max(16.0), 15.0),
    );
    ui.painter().rect_filled(r, Rounding::ZERO, p.accent);
    ui.painter()
        .galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
}
