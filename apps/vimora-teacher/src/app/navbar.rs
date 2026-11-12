//! Window chrome: two rows on top, a status bar at the bottom.
//!
//! 1. **Navbar** – brand · class actions · chat / journal.
//! 2. **Tabs row** – room tabs · search.
//! 3. **Status bar** – online count · view mode · zoom · theme · settings.
//!
//! Everything here is drawn with one control height ([`H`]).

use super::*;
use vimora_ui::{tf, tr};

/// Navbar row height.
pub(super) const NAVBAR_H: f32 = 52.0;
/// Toolbar row height.
pub(super) const TOOLBAR_H: f32 = 48.0;
/// Status bar height.
pub(super) const STATUS_H: f32 = 36.0;

/// Height of every control in the chrome.
const H: f32 = 34.0;
/// Horizontal page margin.
const MARGIN: f32 = 20.0;

/// Height of a navbar button (the row keeps ~9 px above and below it).
const NAV_BTN_H: f32 = 34.0;
/// Height of a tab.
const TAB_BTN_H: f32 = 34.0;
/// Height of a status bar button.
const STATUS_BTN_H: f32 = 26.0;
/// Gap between neighbouring buttons.
const BTN_GAP: f32 = 4.0;
/// Space between icon and label, and the side padding of a labelled button.
const ICON_GAP: f32 = 6.0;
const SIDE_PAD: f32 = 14.0;
const ICON_W: f32 = 16.0;

/// UPPERCASE button / tab label (tight tracking, same font in every state).
fn label_galley(ui: &egui::Ui, text: &str, color: Color32) -> std::sync::Arc<egui::Galley> {
    let format = egui::TextFormat {
        font_id: FontId::new(11.0, ui_kit::medium()),
        color,
        extra_letter_spacing: 0.7,
        ..Default::default()
    };
    ui.painter()
        .layout_job(egui::text::LayoutJob::single_section(
            text.to_uppercase(),
            format,
        ))
}

/// The one navbar/tab button: flat, icon + UPPERCASE label, tinted + underlined when active.
/// The font never changes with state.
struct Btn<'a> {
    icon: &'a str,
    label: &'a str,
    /// Icon + text colour when set (state colour such as warning / danger).
    tint: Option<Color32>,
    /// Drawn as "on" (tinted background and underline).
    active: bool,
    badge: Option<(String, Color32)>,
    /// Icon-only unless `labelled`.
    labelled: bool,
    /// Button height.
    h: f32,
    /// Disabled buttons are dimmed and ignore clicks.
    enabled: bool,
    /// Side padding of a labelled button (the navbar narrows it when space is short).
    pad: f32,
}

impl Btn<'_> {
    fn width(&self, ui: &egui::Ui, p: &Palette) -> f32 {
        if self.labelled && !self.label.is_empty() {
            let g = label_galley(ui, self.label, p.text);
            let icon = if self.icon.is_empty() {
                0.0
            } else {
                ICON_W + ICON_GAP
            };
            self.pad + icon + g.size().x + self.pad
        } else {
            self.h
        }
    }

    fn show(self, ui: &mut egui::Ui, p: &Palette) -> egui::Response {
        let w = self.width(ui, p);
        let (r, resp) = ui.allocate_exact_size(
            vec2(w, self.h),
            if self.enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let hov = resp.hovered() && self.enabled;
        let ink = match (self.tint, hov, self.active) {
            _ if !self.enabled => p.faint.gamma_multiply(0.6),
            (Some(c), _, _) => c,
            (None, true, false) => p.accent_ink,
            (None, _, true) => p.strong,
            _ => p.text,
        };
        // Hover: a light wash of the primary colour with a 1 px primary border.
        // Active: a stronger wash and a 2 px primary underline.
        if self.active {
            let c = self.tint.unwrap_or(p.accent);
            ui.painter()
                .rect_filled(r, Rounding::ZERO, c.gamma_multiply(0.14));
            ui.painter()
                .hline(r.x_range(), r.bottom() - 1.0, Stroke::new(2.0_f32, c));
        } else if hov {
            ui.painter()
                .rect_filled(r, Rounding::ZERO, p.accent.gamma_multiply(0.08));
            ui.painter().rect_stroke(
                r.shrink(0.5),
                Rounding::ZERO,
                Stroke::new(1.0_f32, p.accent.gamma_multiply(0.7)),
            );
        }
        let labelled = self.labelled && !self.label.is_empty();
        let icon_x = if labelled {
            r.left() + self.pad + ICON_W / 2.0
        } else {
            r.center().x
        };
        let has_icon = !self.icon.is_empty();
        if has_icon {
            ui.painter().text(
                pos2(icon_x, r.center().y),
                Align2::CENTER_CENTER,
                self.icon,
                FontId::proportional(16.0),
                ink,
            );
        }
        if labelled {
            let g = label_galley(ui, self.label, ink);
            ui.painter().galley(
                pos2(
                    r.left() + self.pad + if has_icon { ICON_W + ICON_GAP } else { 0.0 },
                    r.center().y - g.size().y / 2.0,
                ),
                g,
                ink,
            );
        }
        if let Some((text, col)) = &self.badge {
            let ink = if *col == p.accent {
                p.on_accent
            } else {
                Color32::WHITE
            };
            let g = ui.painter().layout_no_wrap(
                text.clone(),
                FontId::new(9.0, ui_kit::mono_bold()),
                ink,
            );
            let bw = (g.size().x + 8.0).max(15.0);
            let at = if labelled {
                pos2(r.left() + self.pad + ICON_W - 2.0, r.top() + 6.0)
            } else {
                pos2(r.center().x + 9.0, r.top() + 6.0)
            };
            let br = Rect::from_min_size(pos2(at.x - bw / 2.0, at.y), vec2(bw, 15.0));
            ui.painter().rect_filled(br, Rounding::ZERO, *col);
            ui.painter().galley(br.center() - g.size() / 2.0, g, ink);
        }
        if hov {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp
    }
}

fn divider(ui: &mut egui::Ui, p: &Palette, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, h), Sense::hover());
    ui.painter().vline(
        r.center().x,
        (r.center().y - h * 0.3)..=(r.center().y + h * 0.3),
        Stroke::new(1.0_f32, p.line_strong),
    );
}

/// One entry of the toolbar's action strip.
struct Act {
    icon: &'static str,
    label: &'static str,
    hint: &'static str,
    menu: Menu,
    active: bool,
    tint: Option<Color32>,
}

impl TeacherApp {
    /// Draws navbar + toolbar. Call before the central panel.
    pub(super) fn navbar(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        self.statusbar(ctx, p, all);
        self.top_row(ctx, p, all);
        self.toolbar(ctx, p, all);
    }

    // ── Row 1 ────────────────────────────────────────────────────────────────
    fn top_row(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let unread = self.hub.lock().unwrap().unread;
        let attention = all
            .iter()
            .filter(|v| v.help.is_some() || v.alert.is_some())
            .count();

        egui::TopBottomPanel::top("navbar")
            .exact_height(NAVBAR_H)
            .frame(
                Frame::none()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().hline(
                    full.x_range().expand(MARGIN),
                    full.bottom(),
                    Stroke::new(1.0_f32, p.line),
                );
                ui.spacing_mut().item_spacing.x = BTN_GAP;
                ui.horizontal_centered(|ui| {
                    // Brand
                    let (lr, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                    ui_kit::logo(ui.painter(), lr);
                    ui.add_space(6.0);
                    let g = ui.painter().layout_no_wrap(
                        "VIMORA".into(),
                        FontId::new(16.0, ui_kit::bold()),
                        p.strong,
                    );
                    let (br, _) = ui.allocate_exact_size(vec2(g.size().x, H), Sense::hover());
                    ui.painter().galley(
                        pos2(br.left(), br.center().y - g.size().y / 2.0),
                        g,
                        p.strong,
                    );
                    ui.add_space(10.0);

                    // Right cluster is laid out first (right-to-left); actions fill what remains.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Navbar buttons sit edge to edge.
                        ui.spacing_mut().item_spacing.x = 0.0;

                        // Fullscreen (rightmost): same as F11; the icon follows the real state,
                        // whichever way it was switched.
                        let full = self.fullscreen;
                        let r = Btn {
                            icon: if full {
                                icons::MINIMIZE_2
                            } else {
                                icons::MAXIMIZE_2
                            },
                            label: "",
                            tint: None,
                            active: false,
                            badge: None,
                            labelled: false,
                            h: NAV_BTN_H,
                            enabled: true,
                            pad: SIDE_PAD,
                        }
                        .show(ui, p)
                        .tip(if full {
                            tr("To'liq ekrandan chiqish (F11)")
                        } else {
                            tr("To'liq ekran (F11)")
                        });
                        if r.clicked() {
                            self.toggle_fullscreen(ctx);
                        }

                        // Journal / alerts
                        let r = Btn {
                            icon: if attention > 0 {
                                icons::BELL_DOT
                            } else {
                                icons::BELL
                            },
                            label: "",
                            tint: (attention > 0).then_some(p.warning),
                            active: matches!(self.dialog, Some(Dialog::Activity)),
                            badge: (attention > 0)
                                .then(|| (attention.min(9).to_string(), p.danger)),
                            labelled: false,
                            h: NAV_BTN_H,
                            enabled: true,
                            pad: SIDE_PAD,
                        }
                        .show(ui, p)
                        .tip(tr("Jurnal va ogohlantirishlar"));
                        if r.clicked() {
                            self.menu(Menu::Activity, all);
                        }

                        // Chat
                        let r = Btn {
                            icon: icons::MESSAGE_SQUARE_TEXT,
                            label: "",
                            tint: None,
                            active: matches!(self.dialog, Some(Dialog::Message { .. })),
                            badge: (unread > 0).then(|| (unread.min(99).to_string(), p.accent)),
                            labelled: false,
                            h: NAV_BTN_H,
                            enabled: true,
                            pad: SIDE_PAD,
                        }
                        .show(ui, p)
                        .tip(tr("Chat"));
                        if r.clicked() {
                            self.menu(Menu::Message, all);
                        }
                        // Class actions: whatever width is left.
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            self.action_strip(ui, p, all);
                        });
                    });
                });
            });
    }

    /// Search box + helpers.
    /// Search field (filters computers by name).
    fn search_box(&mut self, ui: &mut egui::Ui, p: &Palette) {
        Frame::none()
            .fill(p.sunken)
            .stroke(Stroke::new(1.0_f32, p.line))
            .rounding(Rounding::ZERO)
            .inner_margin(Margin {
                left: 10.0,
                right: 10.0,
                top: 0.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.set_height(H - 2.0);
                ui.set_width(190.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(RichText::new(icons::SEARCH).size(14.0).color(p.faint));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text(tr("Qidirish…"))
                            .frame(false)
                            .vertical_align(Align::Center)
                            .desired_width(150.0)
                            .clip_text(true),
                    );
                });
            });
    }

    // ── Row 2: room tabs + search ────────────────────────────────────────────
    fn toolbar(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let rooms = self.cfg.read().unwrap().rooms.clone();
        egui::TopBottomPanel::top("toolbar")
            .exact_height(TOOLBAR_H)
            .frame(
                Frame::none()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().hline(
                    full.x_range().expand(MARGIN),
                    full.bottom(),
                    Stroke::new(1.0_f32, p.line),
                );
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = BTN_GAP;
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.search_box(ui, p);
                        // New computers: scan the network, then add them one by one.
                        if ui_kit::button(ui, Some(icons::SEARCH), tr("Skanerlash"), Kind::Outline)
                            .tip(tr("Tarmoqdan yangi kompyuterlarni qidirish"))
                            .clicked()
                        {
                            self.open_dialog(Dialog::scan());
                        }
                        ui.add_space(10.0);

                        // Rooms: whatever width is left, scrolls sideways if there are many.
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            egui::ScrollArea::horizontal()
                                .id_salt("room-tabs")
                                .auto_shrink([false, true])
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = BTN_GAP;
                                    ui.horizontal_centered(|ui| self.room_tabs(ui, p, all, &rooms));
                                });
                        });
                    });
                });
            });
    }

    // ── Status bar: online count · zoom · view mode · theme · settings ────────
    pub(super) fn statusbar(&mut self, ctx: &egui::Context, p: &Palette, all: &[View]) {
        let known = self.cfg.read().unwrap().known.len();
        let update = self.updater.available();
        let online = all.iter().filter(|v| v.online).count();
        let total = all.len().max(known).max(online);
        let h = STATUS_BTN_H;
        let btn = |icon: &'static str, label: &'static str, active: bool| Btn {
            icon,
            label,
            tint: None,
            active,
            badge: None,
            labelled: !label.is_empty(),
            h,
            enabled: true,
            pad: SIDE_PAD,
        };

        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(STATUS_H)
            .frame(
                Frame::none()
                    .fill(p.panel)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().hline(
                    full.x_range().expand(MARGIN),
                    full.top(),
                    Stroke::new(1.0_f32, p.line),
                );
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = BTN_GAP;

                    // Left: online count
                    let g = ui.painter().layout_no_wrap(
                        tf!("{on} / {all}", on = online, all = total),
                        FontId::new(11.5, ui_kit::mono_bold()),
                        p.text,
                    );
                    let (sr, sresp) =
                        ui.allocate_exact_size(vec2(g.size().x + 18.0, STATUS_H), Sense::hover());
                    ui.painter().rect_filled(
                        Rect::from_center_size(
                            pos2(sr.left() + 3.0, sr.center().y),
                            vec2(6.0, 6.0),
                        ),
                        Rounding::ZERO,
                        if online > 0 { p.success } else { p.faint },
                    );
                    ui.painter().galley(
                        pos2(sr.left() + 14.0, sr.center().y - g.size().y / 2.0),
                        g,
                        p.text,
                    );
                    sresp.tip(tf!(
                        "{on} ta kompyuter onlayn, jami {all} ta",
                        on = online,
                        all = total
                    ));
                    // New computers (in no teacher yet) waiting to be added.
                    let free = self.hub.lock().unwrap().free_now().len();
                    if free > 0 {
                        ui.add_space(12.0);
                        let r = ui
                            .add(
                                egui::Button::new(
                                    RichText::new(with_icon(
                                        icons::PLUS,
                                        &tf!("{n} ta yangi kompyuter", n = free),
                                    ))
                                    .size(12.5)
                                    .color(p.accent_ink),
                                )
                                .frame(false),
                            )
                            .tip(tr("Hali qo'shilmagan kompyuterlar: qo'shish uchun bosing"));
                        if r.clicked() {
                            self.open_dialog(Dialog::scan());
                        }
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(5));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = BTN_GAP;

                        // Rightmost first: settings, theme, update
                        let r = btn(
                            icons::SETTINGS,
                            "",
                            matches!(self.dialog, Some(Dialog::Settings(_))),
                        )
                        .show(ui, p)
                        .tip(tr("Sozlamalar"));
                        if r.clicked() {
                            if self.admin {
                                self.open_dialog(Dialog::Settings(SettingsTab::General));
                            } else {
                                self.ask_admin(Some(Menu::Settings));
                            }
                        }
                        // With an administrator password: who is at the teacher.
                        if !self.cfg.read().unwrap().admin_hash.is_empty() {
                            let (ic, tip) = if self.admin {
                                (
                                    icons::LOCK_OPEN,
                                    tr("Administrator rejimi — bosib o'qituvchi rejimiga qaytish"),
                                )
                            } else {
                                (
                                    icons::LOCK,
                                    tr("O'qituvchi rejimi — bosib administrator bo'lib kirish"),
                                )
                            };
                            let r = btn(ic, "", self.admin).show(ui, p).tip(tip);
                            if r.clicked() {
                                if self.admin {
                                    self.admin = false;
                                } else {
                                    self.ask_admin(None);
                                }
                            }
                        }

                        let (ic, tip) = match self.theme {
                            ThemeMode::Dark => (icons::SUN, tr("Yorug' rejim")),
                            ThemeMode::Light => (icons::MOON, tr("Qorong'i rejim")),
                        };
                        let r = btn(ic, "", false).show(ui, p).tip(tip);
                        if r.clicked() {
                            let t = self.theme.toggled();
                            {
                                let mut c = self.cfg.write().unwrap();
                                c.theme_system = false;
                                c.dark_mode = t == ThemeMode::Dark;
                                config::save(&c);
                            }
                            self.set_theme(ctx, t);
                        }

                        // Teacher update in progress: a small bar + percent, visible anywhere.
                        if self.updater.working() {
                            use crate::update::Phase;
                            let (frac, text) = match self.updater.phase() {
                                Phase::Downloading { done, total, .. } if total > 0 => {
                                    let f = done as f32 / total as f32;
                                    (
                                        f * 0.8,
                                        format!("{} {}%", tr("Yangilanmoqda"), (f * 100.0) as u32),
                                    )
                                }
                                Phase::Verifying(_) => (0.85, tr("Tekshirilmoqda…").to_string()),
                                Phase::Installing(_) => (0.92, tr("O'rnatilmoqda…").to_string()),
                                _ => (0.0, tr("Yangilanmoqda").to_string()),
                            };
                            let g = ui.painter().layout_no_wrap(
                                text,
                                FontId::new(12.0, ui_kit::medium()),
                                p.accent_ink,
                            );
                            let (r, resp) = ui.allocate_exact_size(
                                vec2(g.size().x + 110.0, h - 8.0),
                                Sense::click(),
                            );
                            let bar = Rect::from_min_size(
                                pos2(r.right() - 90.0, r.center().y - 3.0),
                                vec2(84.0, 6.0),
                            );
                            ui.painter().rect_filled(bar, Rounding::ZERO, p.sunken);
                            ui.painter().rect_filled(
                                Rect::from_min_size(bar.min, vec2(bar.width() * frac, 6.0)),
                                Rounding::ZERO,
                                p.accent,
                            );
                            ui.painter().galley(
                                pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0),
                                g,
                                p.accent_ink,
                            );
                            if resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.open_dialog(Dialog::Settings(SettingsTab::Updates));
                            }
                            ui.ctx().request_repaint_after(Duration::from_millis(200));
                        }
                        if let Some(v) = &update {
                            let mut b = btn(icons::CIRCLE_ARROW_UP, "", true);
                            b.tint = Some(p.accent_ink);
                            let r = b.show(ui, p).tip(tf!("Yangilash mavjud: {v}", v = v));
                            if r.clicked() {
                                self.open_dialog(Dialog::Settings(SettingsTab::Updates));
                            }
                        }
                        divider(ui, p, h);

                        // View mode (right to left: list, then cards)
                        let view = self.cfg.read().unwrap().view;
                        if btn(icons::LIST, "", view == ViewMode::List)
                            .show(ui, p)
                            .tip(tr("Ro'yxat"))
                            .clicked()
                        {
                            self.cfg.write().unwrap().view = ViewMode::List;
                        }
                        if btn(icons::LAYOUT_GRID, "", view == ViewMode::Grid)
                            .show(ui, p)
                            .tip(tr("Katakchalar"))
                            .clicked()
                        {
                            self.cfg.write().unwrap().view = ViewMode::Grid;
                        }

                        // Zoom (cards only; right to left: + , 100 % , -)
                        if view == ViewMode::Grid {
                            divider(ui, p, h);
                            let cols = self.columns();
                            let mut plus = btn(icons::PLUS, "", false);
                            plus.enabled = cols > MIN_COLUMNS;
                            if plus.show(ui, p).tip(tr("Kattalashtirish")).clicked() {
                                self.set_columns(cols - 1);
                            }
                            let pct = format!("{}%", zoom_percent(cols));
                            if (Btn {
                                icon: "",
                                label: &pct,
                                tint: None,
                                active: false,
                                badge: None,
                                labelled: true,
                                h,
                                enabled: true,
                                pad: SIDE_PAD,
                            })
                            .show(ui, p)
                            .tip(tr("Bosing: 100%"))
                            .clicked()
                            {
                                self.set_columns(DEFAULT_COLUMNS);
                            }
                            let mut minus = btn(icons::MINUS, "", false);
                            minus.enabled = cols < MAX_COLUMNS;
                            if minus.show(ui, p).tip(tr("Kichiklashtirish")).clicked() {
                                self.set_columns(cols + 1);
                            }
                        }
                    });
                });
            });
    }

    /// The class actions as big square buttons.
    fn action_strip(&mut self, ui: &mut egui::Ui, p: &Palette, all: &[View]) {
        ui.spacing_mut().item_spacing.x = BTN_GAP;

        let a = |icon, label, hint, menu, active, tint| Act {
            icon,
            label,
            hint,
            menu,
            active,
            tint,
        };
        // Grouped by use: [lock, unlock] · [present, lesson, site] · [block, freeze] ·
        // [updates, power]. Power, the rare and drastic one, sits last.
        // Lock and unlock are two fixed buttons: they never change with what is selected.
        let groups: Vec<Vec<Act>> = vec![
            vec![
                a(
                    icons::LOCK,
                    tr("Qulflash"),
                    tr("Ekranlarni qulflash (Ctrl+L)"),
                    Menu::Lock,
                    false,
                    None,
                ),
                a(
                    icons::LOCK_OPEN,
                    tr("unlock|Ochish"),
                    tr("Qulfni ochish (Ctrl+U)"),
                    Menu::Unlock,
                    false,
                    None,
                ),
            ],
            vec![
                a(
                    icons::CAST,
                    if self.showing {
                        tr("To'xtatish")
                    } else {
                        tr("Namoyish")
                    },
                    tr("Ekraningizni o'quvchilarga ko'rsatish"),
                    Menu::Show,
                    self.showing,
                    self.showing.then_some(p.accent_ink),
                ),
                a(
                    icons::GRADUATION_CAP,
                    tr("Dars"),
                    tr("Fayllar, davomat, so'rov, test, taymer"),
                    Menu::Lesson,
                    false,
                    None,
                ),
                a(
                    icons::ROCKET,
                    tr("Buyruq"),
                    tr("Kompyuterlarda sayt yoki dastur ochish"),
                    Menu::Web,
                    false,
                    None,
                ),
            ],
            vec![
                a(
                    icons::SHIELD_BAN,
                    tr("Bloklash"),
                    tr("Saytlar, kalit so'zlar, dasturlar"),
                    Menu::Net,
                    false,
                    None,
                ),
                a(
                    icons::SNOWFLAKE,
                    tr("Muzlatish"),
                    tr("Qayta yuklanganda kompyuter toza holatga qaytadi (Shadow Defender, UWF)"),
                    Menu::Freeze,
                    false,
                    None,
                ),
            ],
            vec![
                a(
                    icons::DOWNLOAD,
                    tr("Yangilanish"),
                    tr("Dasturlar, Windows va Vimora yangilanishlari"),
                    Menu::Updates,
                    false,
                    None,
                ),
                a(
                    icons::POWER,
                    tr("Quvvat"),
                    tr("Yoqish, o'chirish, qayta yuklash"),
                    Menu::Power,
                    false,
                    None,
                ),
            ],
        ];

        // Every action keeps its text. When the row is short of space (narrow window, a longer
        // language) the buttons' side padding shrinks instead; they sit edge to edge.
        ui.spacing_mut().item_spacing.x = 0.0;
        let btn_of = |act: &Act, pad: f32| Btn {
            icon: act.icon,
            label: act.label,
            tint: act.tint,
            active: act.active,
            badge: None,
            labelled: true,
            h: NAV_BTN_H,
            enabled: true,
            pad,
        };
        let flat: Vec<&Act> = groups.iter().flatten().collect();
        let room = ui.available_width() - 8.0;
        let mut pad = SIDE_PAD;
        while pad > 6.0
            && flat
                .iter()
                .map(|a| btn_of(a, pad).width(ui, p))
                .sum::<f32>()
                > room
        {
            pad -= 1.0;
        }

        let mut clicked = None;
        for act in flat {
            let r = btn_of(act, pad).show(ui, p).tip(act.hint);
            if r.clicked() {
                clicked = Some(act.menu);
            }
        }
        if let Some(m) = clicked {
            self.menu(m, all);
        }
    }

    /// Room tabs: pills with a count; the active one is raised.
    fn room_tabs(&mut self, ui: &mut egui::Ui, p: &Palette, all: &[View], rooms: &[String]) {
        self.tab_btn(
            ui,
            p,
            icons::LAYOUT_GRID,
            tr("Hammasi"),
            all.len(),
            self.room_tab == RoomTab::All,
            None,
            |app| app.room_tab = RoomTab::All,
        );
        for r in rooms {
            let n = all
                .iter()
                .filter(|v| v.room.as_deref() == Some(r.as_str()))
                .count();
            let sel = self.room_tab == RoomTab::Room(r.clone());
            let room = r.clone();
            self.tab_btn(
                ui,
                p,
                icons::USERS,
                r,
                n,
                sel,
                Some(r.clone()),
                move |app| app.room_tab = RoomTab::Room(room),
            );
        }
        if !rooms.is_empty() {
            let unassigned = all.iter().filter(|v| v.room.is_none()).count();
            self.tab_btn(
                ui,
                p,
                icons::LAPTOP,
                tr("Xonasiz"),
                unassigned,
                self.room_tab == RoomTab::Unassigned,
                None,
                |app| app.room_tab = RoomTab::Unassigned,
            );
        }
        let resp = Btn {
            icon: icons::PLUS,
            label: "",
            tint: None,
            active: false,
            badge: None,
            labelled: false,
            h: TAB_BTN_H,
            enabled: true,
            pad: SIDE_PAD,
        }
        .show(ui, p)
        .tip(tr("Yangi xona"));
        if resp.clicked() {
            self.open_dialog(Dialog::Room {
                name: String::new(),
                rename: None,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_btn(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        icon: &'static str,
        label: &str,
        count: usize,
        selected: bool,
        room_name: Option<String>,
        on_click: impl FnOnce(&mut TeacherApp),
    ) {
        let g_label = label_galley(ui, label, p.text);
        let g_count = ui.painter().layout_no_wrap(
            count.to_string(),
            FontId::new(10.5, ui_kit::mono_bold()),
            p.muted,
        );
        let count_w = (g_count.size().x + 10.0).max(20.0);
        let w = SIDE_PAD + ICON_W + ICON_GAP + g_label.size().x + 8.0 + count_w + SIDE_PAD - 4.0;
        let (r, resp) = ui.allocate_exact_size(vec2(w, TAB_BTN_H), Sense::click());
        let hov = resp.hovered();

        if selected {
            ui.painter()
                .rect_filled(r, Rounding::ZERO, p.accent.gamma_multiply(0.12));
            ui.painter().hline(
                r.x_range(),
                r.bottom() - 1.0,
                Stroke::new(2.0_f32, p.accent),
            );
        } else if hov {
            ui.painter().rect_filled(r, Rounding::ZERO, p.hover);
        }
        let ink = if selected || hov { p.strong } else { p.text };
        ui.painter().text(
            pos2(r.left() + SIDE_PAD + ICON_W / 2.0, r.center().y),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            ink,
        );
        let g_label = label_galley(ui, label, ink);
        ui.painter().galley(
            pos2(
                r.left() + SIDE_PAD + ICON_W + ICON_GAP,
                r.center().y - g_label.size().y / 2.0,
            ),
            g_label,
            ink,
        );
        let cr = Rect::from_min_size(
            pos2(r.right() - (SIDE_PAD - 4.0) - count_w, r.center().y - 9.0),
            vec2(count_w, 18.0),
        );
        ui.painter().rect_filled(
            cr,
            Rounding::ZERO,
            if selected { p.accent } else { p.sunken },
        );
        ui.painter().galley(
            cr.center() - g_count.size() / 2.0,
            g_count,
            if selected { p.on_accent } else { p.muted },
        );
        if hov {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let resp = if room_name.is_some() {
            resp.tip(tr("O'ng tugma — nomini o'zgartirish yoki o'chirish"))
        } else {
            resp
        };
        if resp.clicked() {
            on_click(self);
        }
        if let Some(rname) = room_name {
            if resp.secondary_clicked() {
                let x = ui
                    .input(|i| i.pointer.interact_pos())
                    .map(|p| p.x)
                    .unwrap_or(r.center().x);
                self.ctx_menu = Some(CtxMenu {
                    target: CtxTarget::Room(rname),
                    anchor: r,
                    x,
                    frame: self.now_frame,
                    rooms_open: false,
                });
            }
        }
    }
}
