//! Settings: a two-pane modal (sections on the left, rows on the right). Every change is applied
//! at once and saved automatically — there is no "Save" button.

use super::*;
use crate::config::SortBy;
use crate::update::Phase;
use vimora_ui::{tf, tr, Lang};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SettingsTab {
    General,
    LockScreen,
    /// "Dastur haqida": version, teacher updates and where things are.
    Updates,
    Schedule,
    Reports,
    Access,
}

impl SettingsTab {
    const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::LockScreen,
        SettingsTab::Schedule,
        SettingsTab::Reports,
        SettingsTab::Access,
        SettingsTab::Updates,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            SettingsTab::General => tr("Asosiy"),
            SettingsTab::LockScreen => tr("Qulf ekrani"),
            SettingsTab::Updates => tr("Dastur haqida"),
            SettingsTab::Schedule => tr("Jadval"),
            SettingsTab::Reports => tr("Hisobotlar"),
            SettingsTab::Access => tr("Kirish"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SettingsTab::General => icons::SLIDERS_HORIZONTAL,
            SettingsTab::LockScreen => icons::LOCK,
            SettingsTab::Updates => icons::BADGE_INFO,
            SettingsTab::Schedule => icons::TIMER,
            SettingsTab::Reports => icons::FILE_TEXT,
            SettingsTab::Access => icons::SHIELD,
        }
    }
}

/// Things the settings page asks the app to do (it only edits the config itself).
#[derive(Default)]
pub(super) struct SettingsOut {
    pub(super) check_update: bool,
    pub(super) install_update: bool,
    pub(super) restart: bool,
    /// Export a report: 0 computers, 1 activity log, 2 attendance.
    pub(super) report: Option<u8>,
    /// The administrator password was just set.
    pub(super) password_set: bool,
    pub(super) backup_save: bool,
    pub(super) backup_load: bool,
}

/// Settings live in a drawer like everything else: same width as the others.
pub(super) const SETTINGS_W: f32 = super::overlay::DRAWER_W;
const NAV_W: f32 = 176.0;
const ROW_H: f32 = 56.0;
const NAV_ITEM_H: f32 = 46.0;

pub(super) struct SettingsCtx<'a> {
    pub(super) fingerprint: &'a str,
    pub(super) phase: Phase,
    /// Last automatic save, for the "saved" hint.
    pub(super) saved: bool,
    /// Newest version known (an available release, else this teacher's own).
    pub(super) latest: String,
}

pub(super) fn settings_page(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut SettingsTab,
    c: &mut TeacherConfig,
    cx: &SettingsCtx<'_>,
    out: &mut SettingsOut,
    body_h: f32,
) {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    ui.horizontal_top(|ui| {
        // --- section list ------------------------------------------------------------------
        let (nav, _) = ui.allocate_exact_size(vec2(NAV_W, body_h), Sense::hover());
        ui.painter().rect_filled(nav, Rounding::ZERO, p.bg);
        ui.painter().vline(
            nav.right() - 0.5,
            nav.y_range(),
            Stroke::new(1.0_f32, p.line),
        );
        let mut nav_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(nav)
                .layout(Layout::top_down(Align::Min)),
        );
        for t in SettingsTab::ALL {
            let on = *tab == t;
            let (r, resp) =
                nav_ui.allocate_exact_size(vec2(NAV_W - 1.0, NAV_ITEM_H), Sense::click());
            if on || resp.hovered() {
                nav_ui
                    .painter()
                    .rect_filled(r, Rounding::ZERO, if on { p.card } else { p.hover });
            }
            if on {
                nav_ui.painter().rect_filled(
                    Rect::from_min_size(r.min, vec2(3.0, r.height())),
                    Rounding::ZERO,
                    p.accent,
                );
            }
            let col = if on { p.strong } else { p.text };
            nav_ui.painter().text(
                pos2(r.left() + 28.0, r.center().y),
                Align2::CENTER_CENTER,
                t.icon(),
                FontId::proportional(17.0),
                if on { p.accent_ink } else { p.muted },
            );
            let g = ui_kit::truncated(
                &nav_ui,
                t.label(),
                FontId::new(13.0, ui_kit::medium()),
                col,
                NAV_W - 54.0 - 34.0,
            );
            nav_ui.painter().galley(
                pos2(r.left() + 54.0, r.center().y - g.size().y / 2.0),
                g,
                col,
            );
            // Arrow at the right edge: shows where the item leads, brighter when selected.
            nav_ui.painter().text(
                pos2(r.right() - 22.0, r.center().y),
                Align2::CENTER_CENTER,
                icons::CHEVRON_RIGHT,
                FontId::proportional(15.0),
                if on {
                    p.accent_ink
                } else if resp.hovered() {
                    p.muted
                } else {
                    p.faint
                },
            );
            if resp.hovered() {
                nav_ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                *tab = t;
            }
        }
        // "Saved automatically" hint at the bottom of the list.
        let hint = if cx.saved {
            with_icon(icons::CHECK, tr("Saqlandi"))
        } else {
            tr("O'zgarishlar darhol saqlanadi").to_string()
        };
        let g = ui_kit::truncated(
            ui,
            &hint,
            FontId::proportional(11.5),
            if cx.saved { p.success } else { p.faint },
            NAV_W - 32.0,
        );
        ui.painter().galley(
            pos2(nav.left() + 24.0, nav.bottom() - 18.0 - g.size().y),
            g,
            p.faint,
        );

        // --- rows ----------------------------------------------------------------------------
        ui.allocate_ui_with_layout(
            vec2(SETTINGS_W - NAV_W, body_h),
            Layout::top_down(Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("settings", *tab as u8))
                    .max_height(body_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::none()
                            .inner_margin(Margin::symmetric(26.0, 14.0))
                            .show(ui, |ui| {
                                ui.set_width(SETTINGS_W - NAV_W - 52.0);
                                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                                match tab {
                                    SettingsTab::General => general(ui, p, c, out),
                                    SettingsTab::LockScreen => lock_screen(ui, p, c),
                                    SettingsTab::Schedule => super::admin::schedule_page(ui, p, c),
                                    SettingsTab::Reports => super::admin::reports_page(ui, p, out),
                                    SettingsTab::Access => super::admin::access_page(ui, p, c, out),
                                    SettingsTab::Updates => {
                                        about(ui, p);
                                        updates(ui, p, c, cx, out);
                                        about_rows(ui, p, cx);
                                    }
                                }
                            });
                    });
            },
        );
    });
}

// ---------------------------------------------------------------------------------------------
// Building blocks

fn heading(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(10.0);
    ui_kit::hud(ui, text, p.muted);
    ui.add_space(6.0);
}

/// Title (+ optional ⓘ tooltip) on the left, the control on the right, hairline below.
pub(super) fn row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    info: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, p.line),
    );
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(rect.min, vec2(w * 0.5, ROW_H)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    left.spacing_mut().item_spacing.x = 6.0;
    left.label(RichText::new(title).size(13.5).color(p.text));
    if let Some(i) = info {
        ui_kit::info(&mut left, i);
    }
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            // One control height, centred: segmented frames and fields don't stretch to the row.
            .max_rect(Rect::from_min_max(
                pos2(
                    rect.left() + w * 0.42,
                    rect.center().y - ui_kit::CONTROL_H / 2.0 - 1.0,
                ),
                pos2(
                    rect.right(),
                    rect.center().y + ui_kit::CONTROL_H / 2.0 + 1.0,
                ),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    right.spacing_mut().item_spacing.x = 0.0;
    control(&mut right);
}

/// Segmented choice; returns true when the value changed.
fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    p: &Palette,
    value: &mut T,
    items: &[(T, &str)],
) -> bool {
    let _ = p;
    let cells: Vec<(Option<&str>, &str)> = items.iter().map(|(_, l)| (None, *l)).collect();
    let current = items.iter().position(|(v, _)| v == value).unwrap_or(0);
    match ui_kit::choices(ui, &cells, current) {
        Some(i) if items[i].0 != *value => {
            *value = items[i].0;
            true
        }
        _ => false,
    }
}

/// Dropdown list: the current value with a chevron; the popup lists every option with a check mark.
pub(super) fn dropdown<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    p: &Palette,
    id: &str,
    value: &mut T,
    items: &[(T, &str)],
) -> bool {
    const W: f32 = 220.0;
    const ITEM_H: f32 = 38.0;
    let popup_id = egui::Id::new(("dropdown", id));
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    let (r, resp) = ui.allocate_exact_size(vec2(W, ui_kit::CONTROL_H), Sense::click());
    let hov = resp.hovered();
    ui.painter().rect(
        r,
        Rounding::ZERO,
        if hov || open { p.hover } else { p.sunken },
        Stroke::new(1.0_f32, if open { p.accent } else { p.line_strong }),
    );
    let label = items
        .iter()
        .find(|(v, _)| *v == *value)
        .map(|(_, l)| *l)
        .unwrap_or("");
    ui.painter().text(
        pos2(r.left() + 14.0, r.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(13.5, ui_kit::medium()),
        p.strong,
    );
    ui.painter().text(
        pos2(r.right() - 18.0, r.center().y),
        Align2::CENTER_CENTER,
        if open {
            icons::CHEVRON_UP
        } else {
            icons::CHEVRON_DOWN
        },
        FontId::proportional(15.0),
        if open { p.accent_ink } else { p.muted },
    );
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let mut changed = false;
    egui::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(W);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for (v, label) in items {
                let (ir, iresp) = ui.allocate_exact_size(vec2(W, ITEM_H), Sense::click());
                let on = *value == *v;
                if iresp.hovered() {
                    ui.painter().rect_filled(ir, Rounding::ZERO, p.hover);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.painter().text(
                    pos2(ir.left() + 14.0, ir.center().y),
                    Align2::LEFT_CENTER,
                    *label,
                    FontId::new(13.5, ui_kit::medium()),
                    if on { p.accent_ink } else { p.text },
                );
                if on {
                    ui.painter().text(
                        pos2(ir.right() - 18.0, ir.center().y),
                        Align2::CENTER_CENTER,
                        icons::CHECK,
                        FontId::proportional(15.0),
                        p.accent_ink,
                    );
                }
                if iresp.clicked() && !on {
                    *value = *v;
                    changed = true;
                }
            }
        },
    );
    changed
}

// ---------------------------------------------------------------------------------------------
// Sections

fn general(ui: &mut egui::Ui, p: &Palette, c: &mut TeacherConfig, out: &mut SettingsOut) {
    ui.add_space(4.0);
    row(ui, p, tr("Til"), None, |ui| {
        let items: Vec<(Lang, &str)> = Lang::ALL.iter().map(|l| (*l, l.native_name())).collect();
        dropdown(ui, p, "language", &mut c.language, &items);
    });
    row(
        ui,
        p,
        tr("O'qituvchi nomi"),
        Some(tr("O'qituvchi tarmoqda shu nom bilan e'lon qilinadi. O'quvchi kompyuterlari jurnalida ham shu nom ko'rinadi.")),
        |ui| {
            ui.add(ui_kit::text_field(&mut c.server_name).desired_width(240.0));
        },
    );
    row(
        ui,
        p,
        tr("Ovozli signallar"),
        Some(tr(
            "Natija, xato va o'quvchi ogohlantirishlarida qisqa ovoz chiqadi.",
        )),
        |ui| {
            if ui_kit::toggle(ui, &mut c.sounds).changed() && c.sounds {
                crate::sound::play(crate::sound::Sound::Ok);
            }
        },
    );
    appearance(ui, p, c);
    let mut every = c.history_minutes;
    row(
        ui,
        p,
        tr("Ekran tarixi"),
        Some(tr("Har bir kompyuter ekrani shu oraliqda rasm qilib saqlanadi (Rasmlar/Vimora/Tarix). Bir haftadan eskilari o'chadi.")),
        |ui| {
            if ui_kit::icon_cell(ui, icons::FOLDER, ui_kit::CONTROL_H, false)
                .tip(tr("Papkani ochish"))
                .clicked()
            {
                let dir = super::media_dir(dirs::picture_dir()).join("Tarix");
                let _ = std::fs::create_dir_all(&dir);
                config::open(&dir.to_string_lossy());
            }
            ui.add_space(6.0);
            if choice(
                ui,
                p,
                &mut every,
                &[(0, tr("O'chiq")), (5, "5 min"), (15, "15 min"), (30, "30 min")],
            ) {
                c.history_minutes = every;
            }
        },
    );
    heading(ui, p, tr("Zaxira nusxa"));
    row(
        ui,
        p,
        tr("Sozlamalar fayli"),
        Some(tr(
            "Barcha sozlamalar, xonalar va ro'yxatlar bitta faylga saqlanadi yoki undan tiklanadi.",
        )),
        |ui| {
            if ui_kit::button(ui, Some(icons::FILE_UP), tr("Tiklash"), Kind::Outline).clicked() {
                out.backup_load = true;
            }
            ui.add_space(6.0);
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("Saqlash"), Kind::Outline).clicked() {
                out.backup_save = true;
            }
        },
    );
}

fn appearance(ui: &mut egui::Ui, p: &Palette, c: &mut TeacherConfig) {
    heading(ui, p, tr("Ko'rinish"));
    #[derive(Clone, Copy, PartialEq)]
    enum Theme {
        Dark,
        Light,
        System,
    }
    let mut t = if c.theme_system {
        Theme::System
    } else if c.dark_mode {
        Theme::Dark
    } else {
        Theme::Light
    };
    row(
        ui,
        p,
        tr("Mavzu"),
        Some(tr(
            "Tizim — kompyuteringizdagi qorong'i yoki yorug' sozlamaga ergashadi.",
        )),
        |ui| {
            if choice(
                ui,
                p,
                &mut t,
                &[
                    (Theme::System, tr("Tizim")),
                    (Theme::Light, tr("Yorug'")),
                    (Theme::Dark, tr("Qorong'i")),
                ],
            ) {
                c.theme_system = t == Theme::System;
                if t != Theme::System {
                    c.dark_mode = t == Theme::Dark;
                }
            }
        },
    );
    heading(ui, p, tr("Kompyuterlar"));
    row(ui, p, tr("Ko'rinishi"), None, |ui| {
        choice(
            ui,
            p,
            &mut c.view,
            &[
                (ViewMode::Grid, tr("Katakchalar")),
                (ViewMode::List, tr("Ro'yxat")),
            ],
        );
    });
    row(
        ui,
        p,
        tr("Tartiblash"),
        Some(tr("IP bo'yicha tartib ko'p xonalarda kompyuterlarning stol ustidagi joylashuviga mos keladi.")),
        |ui| {
            choice(ui, p, &mut c.sort, &[(SortBy::Name, tr("Nomi bo'yicha")), (SortBy::Ip, tr("IP bo'yicha"))]);
        },
    );
    let mut ms = c.thumb_interval_ms;
    let sec: Vec<String> = [1, 2, 5, 10].iter().map(|n| tf!("{n} s", n = n)).collect();
    row(
        ui,
        p,
        tr("Ekran rasmlarini yangilash"),
        Some(tr("Kompyuterlar sahifasidagi kichik ekran rasmlari qanchalik tez-tez yangilanadi. Tezroq — tarmoqqa ko'proq yuk.")),
        |ui| {
            if choice(ui, p, &mut ms, &[(1000, sec[0].as_str()), (2000, sec[1].as_str()), (5000, sec[2].as_str()), (10000, sec[3].as_str())]) {
                c.thumb_interval_ms = ms;
            }
        },
    );
}

/// Icons the teacher can put in the middle of the lock screen (glyph, name).
const LOCK_ICONS: &[(&str, &str)] = &[
    ("\u{e10f}", "Qulf"),
    ("\u{e15c}", "Qalqon"),
    ("\u{e234}", "Bitiruv qalpoqchasi"),
    ("\u{e063}", "Kitob"),
    ("\u{e4b3}", "Taqdimot"),
    ("\u{e235}", "Karnay"),
    ("\u{e1d7}", "Qo'l"),
    ("\u{e132}", "Pauza"),
    ("\u{e09a}", "Tanaffus"),
    ("\u{e3ca}", "Miya"),
];

/// Accent colours offered for the lock screen (all zero = the default green).
const LOCK_COLORS: [[u8; 3]; 7] = [
    [0, 0, 0],
    [0x3b, 0x82, 0xf6],
    [0xf5, 0x9e, 0x0b],
    [0xef, 0x44, 0x44],
    [0xa8, 0x55, 0xf7],
    [0x14, 0xb8, 0xa6],
    [0xe5, 0xe5, 0xe5],
];

fn lock_screen(ui: &mut egui::Ui, p: &Palette, c: &mut TeacherConfig) {
    ui.add_space(4.0);
    let current = if c.lock_icon.is_empty() {
        LOCK_ICONS[0].0
    } else {
        c.lock_icon.as_str()
    }
    .to_string();
    // What the student will see, first: every change below shows here at once.
    // Live preview: what the student sees with these texts and this code.
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 230.0), Sense::hover());
    let d = ui_kit::DARK;
    let painter = ui.painter().with_clip_rect(r);
    {
        let (top, bottom) = (
            Color32::from_rgb(0x1d, 0x1f, 0x1b),
            Color32::from_rgb(0x0e, 0x0f, 0x0d),
        );
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(r.left_top(), top);
        mesh.colored_vertex(r.right_top(), top);
        mesh.colored_vertex(r.right_bottom(), bottom);
        mesh.colored_vertex(r.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
    let acc = if c.lock_accent == [0, 0, 0] {
        d.accent
    } else {
        Color32::from_rgb(c.lock_accent[0], c.lock_accent[1], c.lock_accent[2])
    };
    let font = |size: f32| {
        if c.lock_font == 1 {
            FontId::new(size, ui_kit::mono())
        } else {
            FontId::proportional(size)
        }
    };
    if !c.lock_no_logo {
        let lr = Rect::from_min_size(r.left_top() + vec2(14.0, 12.0), vec2(14.0, 14.0));
        ui_kit::logo(&painter, lr);
        painter.text(
            pos2(lr.right() + 6.0, lr.center().y),
            Align2::LEFT_CENTER,
            if c.lock_brand.trim().is_empty() {
                "VIMORA".to_string()
            } else {
                c.lock_brand.trim().to_uppercase()
            },
            FontId::new(10.0, ui_kit::brand()),
            d.muted,
        );
    }
    if c.lock_clock != 3 {
        let k = match c.lock_clock {
            1 => 0.7,
            2 => 1.5,
            _ => 1.0,
        };
        painter.text(
            pos2(r.right() - 14.0, r.top() + 12.0 + 4.0 * k),
            Align2::RIGHT_CENTER,
            "09:41",
            FontId::new(12.0 * k, ui_kit::mono_bold()),
            Color32::WHITE,
        );
    }
    let has_code = vimora_core::pin::valid(&c.unlock_pin);
    let has_text = !c.lock_title.trim().is_empty() || !c.lock_message.trim().is_empty();
    let mut y = r.top() + if has_code || has_text { 30.0 } else { 80.0 };
    // Soft glow + tile.
    let centre = pos2(r.center().x, y + 24.0);
    {
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(centre, acc.gamma_multiply(0.28));
        const N: u32 = 40;
        for i in 0..N {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                centre + vec2(a.cos(), a.sin()) * 120.0,
                Color32::TRANSPARENT,
            );
        }
        for i in 0..N {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % N);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    // The icon on its own (no tile), as on the real lock screen.
    painter.text(
        centre,
        Align2::CENTER_CENTER,
        current.as_str(),
        FontId::proportional(40.0),
        acc,
    );
    y += 62.0;
    if !c.lock_title.trim().is_empty() {
        let t = ui_kit::truncated(
            ui,
            &c.lock_title,
            FontId::new(18.0, ui_kit::bold()),
            Color32::WHITE,
            w - 40.0,
        );
        painter.galley(pos2(r.center().x - t.size().x / 2.0, y), t, Color32::WHITE);
        y += 28.0;
    }
    if !c.lock_message.trim().is_empty() {
        let m = ui_kit::truncated(ui, &c.lock_message, font(13.0), d.muted, w - 40.0);
        painter.galley(pos2(r.center().x - m.size().x / 2.0, y), m, d.muted);
        y += 24.0;
    }
    if has_code {
        y += 10.0;
        let n = c.unlock_pin.len() as f32;
        let (cell, gap) = (26.0, 6.0);
        let x0 = r.center().x - (n * cell + (n - 1.0) * gap) / 2.0;
        for i in 0..c.unlock_pin.len() {
            let cr = Rect::from_min_size(pos2(x0 + i as f32 * (cell + gap), y), vec2(cell, cell));
            painter.rect_filled(cr, Rounding::ZERO, d.sunken);
            painter.rect_stroke(cr, Rounding::ZERO, Stroke::new(1.0_f32, d.line_strong));
            if i < 2 {
                painter.circle_filled(cr.center(), 4.0, acc);
            }
        }
    }
    heading(ui, p, tr("Matn"));
    row(
        ui,
        p,
        tr("Sarlavha"),
        Some(tr(
            "Bo'sh qoldirsangiz, qulf ekranida sarlavha ko'rinmaydi.",
        )),
        |ui| {
            ui.add(ui_kit::text_field(&mut c.lock_title).desired_width(300.0));
        },
    );
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Matn")).size(13.5).color(p.text));
        ui_kit::info(ui, tr("Qulflanganda o'quvchi ekranining o'rtasida chiqadi. Bo'sh qoldirsangiz ko'rinmaydi. O'zgarish keyingi qulflashda ko'rinadi."));
    });
    ui.add_space(6.0);
    ui.add(
        ui_kit::text_area(&mut c.lock_message)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(12.0);
    row(
        ui,
        p,
        tr("Ochish kodi"),
        Some(tr("4–8 raqam. Internet uzilib qolsa, o'quvchi ekrandagi qulfni shu kod bilan ochadi. Bo'sh qoldirsangiz kod so'ralmaydi. Keyingi qulflashdan boshlab kuchga kiradi.")),
        |ui| {
            let r = ui.add(
                ui_kit::text_field(&mut c.unlock_pin)
                    .password(true)
                    .hint_text(tr("masalan 4821"))
                    .desired_width(160.0),
            );
            if r.changed() {
                c.unlock_pin.retain(|ch| ch.is_ascii_digit());
                c.unlock_pin.truncate(8);
            }
        },
    );
    if !c.unlock_pin.is_empty() && !vimora_core::pin::valid(&c.unlock_pin) {
        ui.label(
            RichText::new(tr("Kod kamida 4 raqamdan iborat bo'lishi kerak."))
                .size(12.0)
                .color(p.warning),
        );
    }
    heading(ui, p, tr("Ko'rinish"));
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Ikonka")).size(13.5).color(p.text));
        ui_kit::info(ui, tr("Qulflanganda o'quvchi ekranining o'rtasida chiqadigan belgi. Keyingi qulflashda ko'rinadi."));
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for (glyph, name) in LOCK_ICONS {
            let on = current == *glyph;
            let (r, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
            let hov = resp.hovered();
            ui.painter().rect(
                r,
                Rounding::ZERO,
                if on {
                    p.card
                } else if hov {
                    p.hover
                } else {
                    p.sunken
                },
                Stroke::new(
                    1.0_f32,
                    if on {
                        p.accent
                    } else if hov {
                        p.line_strong
                    } else {
                        p.line
                    },
                ),
            );
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                *glyph,
                FontId::proportional(19.0),
                if on { p.accent_ink } else { p.muted },
            );
            if hov {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.tip(tr(name)).clicked() {
                c.lock_icon = if *glyph == LOCK_ICONS[0].0 {
                    String::new()
                } else {
                    glyph.to_string()
                };
            }
        }
    });
    ui.add_space(8.0);
    row(ui, p, tr("Soat"), None, |ui| {
        choice(
            ui,
            p,
            &mut c.lock_clock,
            &[
                (1, tr("Kichik")),
                (0, tr("O'rta")),
                (2, tr("Katta")),
                (3, tr("Yo'q")),
            ],
        );
    });
    row(ui, p, tr("Logo"), None, |ui| {
        let mut on = !c.lock_no_logo;
        ui_kit::toggle(ui, &mut on);
        c.lock_no_logo = !on;
    });
    if !c.lock_no_logo {
        row(
            ui,
            p,
            tr("Logo yonidagi nom"),
            Some(tr("Bo'sh qoldirsangiz VIMORA yoziladi.")),
            |ui| {
                ui.add(
                    ui_kit::text_field(&mut c.lock_brand)
                        .hint_text("VIMORA")
                        .desired_width(200.0),
                );
            },
        );
    }
    row(ui, p, tr("Shrift"), None, |ui| {
        choice(
            ui,
            p,
            &mut c.lock_font,
            &[(0, tr("Oddiy")), (1, tr("Mono"))],
        );
    });
    row(ui, p, tr("Rang"), None, |ui| {
        // Right to left: the last swatch is added first.
        for rgb in LOCK_COLORS.iter().rev() {
            let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            let shown = if *rgb == [0, 0, 0] {
                [0x7d, 0xba, 0x28]
            } else {
                *rgb
            };
            ui.painter().rect_filled(
                r,
                Rounding::ZERO,
                Color32::from_rgb(shown[0], shown[1], shown[2]),
            );
            let on = c.lock_accent == *rgb;
            ui.painter().rect_stroke(
                r.expand(if on { 3.0 } else { 0.0 }),
                Rounding::ZERO,
                Stroke::new(if on { 1.5_f32 } else { 0.0 }, p.strong),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                c.lock_accent = *rgb;
            }
            ui.add_space(6.0);
        }
    });
}

fn updates(
    ui: &mut egui::Ui,
    p: &Palette,
    c: &mut TeacherConfig,
    cx: &SettingsCtx<'_>,
    out: &mut SettingsOut,
) {
    heading(ui, p, tr("Yangilanish"));
    // Status card: what's installed, what's available, one action.
    let w = ui.available_width();
    ui_kit::card(p, false, false).fill(p.bg).show(ui, |ui| {
        ui.set_width(w - 26.0);
        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
        ui.horizontal(|ui| {
            let (ic, col, line) = match &cx.phase {
                Phase::Idle => (
                    icons::CLOUD_DOWNLOAD,
                    p.muted,
                    tr("Hali tekshirilmagan").to_string(),
                ),
                Phase::Checking => (
                    icons::REFRESH_CW,
                    p.muted,
                    tr("Tekshirilmoqda…").to_string(),
                ),
                Phase::UpToDate => (
                    icons::BADGE_CHECK,
                    p.success,
                    tr("Eng so'nggi versiya o'rnatilgan").to_string(),
                ),
                Phase::Available(r) => (
                    icons::CIRCLE_ARROW_UP,
                    p.accent_ink,
                    tf!("Yangi versiya mavjud: {v}", v = r.version),
                ),
                Phase::Downloading {
                    version,
                    done,
                    total,
                    speed,
                } => (
                    icons::CLOUD_DOWNLOAD,
                    p.accent_ink,
                    download_line(version, *done, *total, *speed),
                ),
                Phase::Verifying(v) => (
                    icons::SHIELD_CHECK,
                    p.accent_ink,
                    tf!("{v} yuklab olindi, fayl tekshirilmoqda (SHA-256)…", v = v),
                ),
                Phase::Installing(v) => (
                    icons::PACKAGE_CHECK,
                    p.accent_ink,
                    if cfg!(windows) {
                        tf!("{v} o'rnatilmoqda: dastur yopiladi va o'zi qayta ochiladi", v = v)
                    } else {
                        tf!("{v} o'rnatilmoqda: parol oynasi chiqsa, parolingizni kiriting", v = v)
                    },
                ),
                Phase::Installed(v) => (
                    icons::BADGE_CHECK,
                    p.success,
                    tf!("{v} o'rnatildi — qayta ishga tushiring", v = v),
                ),
                Phase::Failed(e) => (
                    icons::TRIANGLE_ALERT,
                    p.warning,
                    {
                        // Plain words, not the raw network error.
                        let why = if e.contains("403") || e.contains("429") {
                            tr("GitHub hozir so'rovni qabul qilmadi. Birozdan keyin qayta urinib ko'ring.")
                        } else {
                            tr("Internet bilan aloqa yo'q yoki GitHub javob bermadi.")
                        };
                        format!("{}. {why}", tr("Tekshirib bo'lmadi"))
                    },
                ),
            };
            ui.add_space(2.0);
            ui.label(RichText::new(ic).size(34.0).color(col));
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new(tf!("Vimora {v}", v = vimora_update::current()))
                        .family(ui_kit::bold())
                        .size(15.0)
                        .color(p.strong),
                );
                ui.label(RichText::new(line).size(12.5).color(col));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &cx.phase {
                Phase::Available(r) => {
                    if ui_kit::primary_button(
                        ui,
                        icons::DOWNLOAD,
                        &tf!("{v} ni o'rnatish", v = r.version),
                    )
                    .clicked()
                    {
                        out.install_update = true;
                    }
                }
                Phase::Installed(_) => {
                    if ui_kit::primary_button(ui, icons::ROTATE_CCW, tr("Qayta ishga tushirish"))
                        .clicked()
                    {
                        out.restart = true;
                    }
                }
                Phase::Checking
                | Phase::Downloading { .. }
                | Phase::Verifying(_)
                | Phase::Installing(_) => {
                    ui.spinner();
                }
                _ => {
                    if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline)
                        .clicked()
                    {
                        out.check_update = true;
                    }
                }
            });
        });
        // Three steps, so it is always clear what happens now and what comes next.
        let step = match &cx.phase {
            Phase::Downloading { .. } => Some(0),
            Phase::Verifying(_) => Some(1),
            Phase::Installing(_) => Some(2),
            Phase::Installed(_) => Some(3),
            _ => None,
        };
        if let Some(step) = step {
            ui.add_space(4.0);
            let frac = match &cx.phase {
                Phase::Downloading { done, total, .. } if *total > 0 => {
                    *done as f32 / *total as f32 * 0.8
                }
                Phase::Downloading { .. } => 0.0,
                Phase::Verifying(_) => 0.85,
                Phase::Installing(_) => 0.92,
                _ => 1.0,
            };
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_height(6.0)
                    .fill(p.accent)
                    .animate(step < 3),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for (i, name) in [tr("step|Yuklab olish"), tr("step|Tekshirish"), tr("step|O'rnatish")]
                    .into_iter()
                    .enumerate()
                {
                    let (ic, col) = if i < step {
                        (icons::CIRCLE_CHECK, p.success)
                    } else if i == step {
                        (icons::REFRESH_CW, p.accent_ink)
                    } else {
                        (icons::CIRCLE_DOT, p.faint)
                    };
                    ui.label(RichText::new(with_icon(ic, name)).size(12.0).color(col));
                }
            });
        }
    });
    ui.add_space(8.0);
    row(
        ui,
        p,
        tr("Avtomatik tekshirish"),
        Some(tr("O'qituvchi ishga tushganda va har 6 soatda GitHub'dan yangi versiyani tekshiradi. Yangilanish faqat siz \"O'rnatish\" tugmasini bosganingizda o'rnatiladi.")),
        |ui| {
            ui_kit::toggle(ui, &mut c.check_updates);
        },
    );
    row(ui, p, tr("Relizlar sahifasi"), None, |ui| {
        if ui_kit::button(ui, Some(icons::EXTERNAL_LINK), "GitHub", Kind::Outline).clicked() {
            config::open(vimora_update::RELEASES_PAGE);
        }
    });
}

fn about(ui: &mut egui::Ui, p: &Palette) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        ui_kit::logo(ui.painter(), r);
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Vimora")
                    .family(ui_kit::bold())
                    .size(22.0)
                    .color(p.strong),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tf!(
                    "Versiya {v} · MIT litsenziyasi",
                    v = vimora_update::current()
                ))
                .size(12.5)
                .color(p.muted),
            );
        });
    });
}

fn about_rows(ui: &mut egui::Ui, p: &Palette, cx: &SettingsCtx<'_>) {
    heading(ui, p, tr("Ma'lumot"));
    row(ui, p, tr("O'qituvchi identifikatori"), Some(tr("Vimora o'rnatilgan kompyuterlar tarmoqda o'zi paydo bo'ladi. Har bir kompyuter birinchi ulangan o'qituvchini eslab qoladi va faqat unga bo'ysunadi. Kompyuterni o'qituvchidan olib tashlasangiz, uni boshqa o'qituvchi ulab olishi mumkin.")), |ui| {
        ui.label(
            RichText::new(cx.fingerprint)
                .family(ui_kit::mono())
                .size(14.0)
                .color(p.strong),
        );
    });
    row(
        ui,
        p,
        tr("Sozlamalar papkasi"),
        Some(tr("server.json (sozlamalar) va identity.key (o'qituvchi kaliti) shu yerda. identity.key yo'qolsa, kompyuterlarni qaytadan bog'lash kerak bo'ladi.")),
        |ui| {
            if ui_kit::button(ui, Some(icons::FOLDER), tr("Ochish"), Kind::Outline).clicked() {
                if let Some(d) = config::dir() {
                    config::open(&d.to_string_lossy());
                }
            }
        },
    );
    row(ui, p, tr("Manba kodi"), None, |ui| {
        if ui_kit::button(ui, Some(icons::GITHUB), "GitHub", Kind::Outline).clicked() {
            config::open("https://github.com/veymay/vimora");
        }
    });
}

/// "3.2 / 9.1 MB · 1.4 MB/s · 5 s left".
pub(super) fn download_line(version: &str, done: u64, total: u64, speed: u64) -> String {
    let mb = |b: u64| b as f64 / 1_048_576.0;
    let mut s = tf!(
        "{v} yuklab olinmoqda: {d} / {t} MB",
        v = version,
        d = format!("{:.1}", mb(done)),
        t = format!("{:.1}", mb(total))
    );
    if speed > 0 {
        s += &format!(" · {:.1} MB/s", mb(speed));
        if total > done {
            let left = (total - done) / speed.max(1);
            s += &format!(" · {}", tf!("{n} s qoldi", n = left + 1));
        }
    }
    s
}
