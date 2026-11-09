//! Settings: a two-pane modal (sections on the left, rows on the right). Every change is applied
//! at once and saved automatically — there is no "Save" button.

use super::*;
use crate::config::SortBy;
use crate::update::Phase;
use vimora_ui::{tf, tr, Lang};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SettingsTab {
    General,
    Appearance,
    LockScreen,
    Students,
    Updates,
    About,
}

impl SettingsTab {
    const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::Appearance,
        SettingsTab::LockScreen,
        SettingsTab::Students,
        SettingsTab::Updates,
        SettingsTab::About,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsTab::General => tr("Umumiy"),
            SettingsTab::Appearance => tr("Ko'rinish"),
            SettingsTab::LockScreen => tr("Qulf ekrani"),
            SettingsTab::Students => tr("O'quvchi kompyuterlari"),
            SettingsTab::Updates => tr("Yangilanish"),
            SettingsTab::About => tr("Dastur haqida"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SettingsTab::General => icons::SLIDERS_HORIZONTAL,
            SettingsTab::Appearance => icons::PALETTE,
            SettingsTab::LockScreen => icons::LOCK,
            SettingsTab::Students => icons::MONITOR_COG,
            SettingsTab::Updates => icons::CLOUD_DOWNLOAD,
            SettingsTab::About => icons::BADGE_INFO,
        }
    }
}

/// Things the settings page asks the app to do (it only edits the config itself).
#[derive(Default)]
pub(super) struct SettingsOut {
    pub(super) check_update: bool,
    pub(super) install_update: bool,
    pub(super) restart: bool,
    pub(super) update_students: bool,
    pub(super) push_policy: bool,
    pub(super) backup_save: bool,
    pub(super) backup_load: bool,
}

pub(super) const SETTINGS_W: f32 = 880.0;
const NAV_W: f32 = 240.0;
const BODY_H: f32 = 520.0;
const ROW_H: f32 = 56.0;
const NAV_ITEM_H: f32 = 46.0;

pub(super) struct SettingsCtx<'a> {
    pub(super) fingerprint: &'a str,
    pub(super) phase: Phase,
    pub(super) online: usize,
    /// Last automatic save, for the "saved" hint.
    pub(super) saved: bool,
}

pub(super) fn settings_page(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut SettingsTab,
    c: &mut ServerConfig,
    cx: &SettingsCtx<'_>,
    out: &mut SettingsOut,
) {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    ui.horizontal_top(|ui| {
        // --- section list ------------------------------------------------------------------
        let (nav, _) = ui.allocate_exact_size(vec2(NAV_W, BODY_H), Sense::hover());
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
            vec2(SETTINGS_W - NAV_W, BODY_H),
            Layout::top_down(Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("settings", *tab as u8))
                    .max_height(BODY_H)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::none()
                            .inner_margin(Margin::symmetric(26.0, 14.0))
                            .show(ui, |ui| {
                                ui.set_width(SETTINGS_W - NAV_W - 52.0);
                                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                                match tab {
                                    SettingsTab::General => general(ui, p, c, out),
                                    SettingsTab::Appearance => appearance(ui, p, c),
                                    SettingsTab::LockScreen => lock_screen(ui, p, c),
                                    SettingsTab::Students => students(ui, p, c, cx, out),
                                    SettingsTab::Updates => updates(ui, p, c, cx, out),
                                    SettingsTab::About => about(ui, p, cx),
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
fn row(
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
    let mut changed = false;
    ui_kit::segmented(p).show(ui, |ui| {
        // The row's control side is right-to-left: add the tabs last-first so they read in order.
        ui.spacing_mut().item_spacing.x = 0.0;
        for (v, label) in items.iter().rev() {
            if ui_kit::tab(ui, label, None, *value == *v).clicked() && *value != *v {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

/// Dropdown list: the current value with a chevron; the popup lists every option with a check mark.
fn dropdown<T: Copy + PartialEq>(
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
        Stroke::new(
            1.0_f32,
            if open {
                p.accent
            } else if hov {
                p.line_strong
            } else {
                p.line
            },
        ),
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

fn note(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(12.0).color(p.faint));
}

// ---------------------------------------------------------------------------------------------
// Sections

fn general(ui: &mut egui::Ui, p: &Palette, c: &mut ServerConfig, out: &mut SettingsOut) {
    heading(ui, p, tr("Umumiy"));
    row(ui, p, tr("Til"), None, |ui| {
        let items: Vec<(Lang, &str)> = Lang::ALL.iter().map(|l| (*l, l.native_name())).collect();
        dropdown(ui, p, "language", &mut c.language, &items);
    });
    row(
        ui,
        p,
        tr("Konsol nomi"),
        Some(tr("Konsol tarmoqda shu nom bilan e'lon qilinadi. O'quvchi kompyuterlari jurnalida ham shu nom ko'rinadi.")),
        |ui| {
            ui.add(ui_kit::text_field(&mut c.server_name).desired_width(240.0));
        },
    );
    row(
        ui,
        p,
        tr("Oynani to'liq ochish"),
        Some(tr("Yoqilsa, konsol to'liq ekranda ochiladi. Aks holda oyna o'lchami doim bir xil (1280 × 760).")),
        |ui| {
            ui_kit::toggle(ui, &mut c.start_maximized);
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
    row(
        ui,
        p,
        tr("Zaxira nusxa"),
        Some(tr(
            "Barcha sozlamalar, xonalar va ro'yxatlar bitta faylga saqlanadi yoki undan tiklanadi.",
        )),
        |ui| {
            if ui_kit::button(ui, Some(icons::FILE_UP), tr("Tiklash"), Kind::Outline).clicked() {
                out.backup_load = true;
            }
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("Saqlash"), Kind::Outline).clicked() {
                out.backup_save = true;
            }
        },
    );
}

fn appearance(ui: &mut egui::Ui, p: &Palette, c: &mut ServerConfig) {
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
            "\"Tizim\" — kompyuteringizdagi qorong'i/yorug' sozlamaga ergashadi.",
        )),
        |ui| {
            if choice(
                ui,
                p,
                &mut t,
                &[
                    (Theme::Dark, tr("Qorong'i")),
                    (Theme::Light, tr("Yorug'")),
                    (Theme::System, tr("Tizim")),
                ],
            ) {
                c.theme_system = t == Theme::System;
                if t != Theme::System {
                    c.dark_mode = t == Theme::Dark;
                }
            }
        },
    );
    row(
        ui,
        p,
        tr("Ixcham tugmalar"),
        Some(tr("Yoqilsa, yuqoridagi amallar tugmalarida faqat ikonka ko'rinadi (nomi ustiga olib borganda chiqadi). O'chiq bo'lsa — ikonka bilan birga yozuv.")),
        |ui| {
            ui_kit::toggle(ui, &mut c.compact_nav);
        },
    );
    row(ui, p, tr("Kompyuterlar"), None, |ui| {
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
    let mut cols = c.grid_columns.clamp(MIN_COLUMNS, MAX_COLUMNS);
    row(
        ui,
        p,
        tr("Kartochka o'lchami"),
        Some(tr("100% — bir qatorda 6 ta kompyuter, 200% — 3 ta. Kompyuterlar sahifasida Ctrl + g'ildirak yoki − / + tugmalari bilan ham o'zgartiriladi.")),
        |ui| {
            if choice(ui, p, &mut cols, &[(6, "100%"), (5, "120%"), (4, "150%"), (3, "200%")]) {
                c.grid_columns = cols;
            }
        },
    );
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

fn lock_screen(ui: &mut egui::Ui, p: &Palette, c: &mut ServerConfig) {
    heading(ui, p, tr("Qulf ekrani"));
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
    // Live preview: what the student sees with these texts and this code.
    heading(ui, p, tr("Ko'rinishi"));
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
    let has_code = vimora_core::pin::valid(&c.unlock_pin);
    let has_text = !c.lock_title.trim().is_empty() || !c.lock_message.trim().is_empty();
    let mut y = r.top() + if has_code || has_text { 30.0 } else { 80.0 };
    // Soft glow + tile.
    let centre = pos2(r.center().x, y + 24.0);
    {
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(centre, d.accent.gamma_multiply(0.28));
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
    painter.rect_filled(
        Rect::from_center_size(centre, vec2(48.0, 48.0)),
        Rounding::ZERO,
        d.accent,
    );
    painter.text(
        centre,
        Align2::CENTER_CENTER,
        icons::LOCK,
        FontId::proportional(22.0),
        d.on_accent,
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
        let m = ui_kit::truncated(
            ui,
            &c.lock_message,
            FontId::proportional(13.0),
            d.muted,
            w - 40.0,
        );
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
                painter.circle_filled(cr.center(), 4.0, d.accent);
            }
        }
    }
}

fn students(
    ui: &mut egui::Ui,
    p: &Palette,
    c: &mut ServerConfig,
    cx: &SettingsCtx<'_>,
    out: &mut SettingsOut,
) {
    heading(ui, p, tr("Vimora yangilanishi"));
    row(
        ui,
        p,
        tr("Avtomatik yangilansin"),
        Some(tr("O'quvchi kompyuterlari har 6 soatda GitHub'dagi yangi versiyani tekshiradi va o'zi, so'rovsiz o'rnatadi. Shadow yoqilgan kompyuterlar o'tkazib yuboriladi. O'zgarish hozir onlayn bo'lgan kompyuterlarga yuboriladi.")),
        |ui| {
            if ui_kit::toggle(ui, &mut c.update_policy.vimora).changed() {
                out.push_policy = true;
            }
        },
    );
    row(
        ui,
        p,
        tr("Hozir yangilash"),
        Some(tr("Onlayn kompyuterlar so'nggi versiyani hozir yuklab oladi va o'rnatadi. Agent o'zi qayta ishga tushadi.")),
        |ui| {
            let r = ui.add_enabled_ui(cx.online > 0, |ui| {
                ui_kit::button(ui, Some(icons::CLOUD_DOWNLOAD), &tf!("{n} ta kompyuter", n = cx.online), Kind::Outline)
            });
            if r.inner.clicked() {
                out.update_students = true;
            }
        },
    );
    heading(ui, p, tr("Ulanish"));
    row(
        ui,
        p,
        tr("Konsol identifikatori"),
        Some(tr("Vimora o'rnatilgan kompyuterlar tarmoqda o'zi paydo bo'ladi. Har bir kompyuter birinchi ulangan konsolni eslab qoladi va faqat unga bo'ysunadi — boshqa birov o'z konsolini ochib boshqara olmaydi. Konsolni almashtirish uchun o'quvchi kompyuterida: vimora-client unpair")),
        |ui| {
            ui.label(RichText::new(cx.fingerprint).family(ui_kit::mono()).size(14.0).color(p.strong));
        },
    );
    note(
        ui,
        p,
        tr("Kompyuterlarni xonalarga kompyuterlar sahifasida o'ng tugma orqali taqsimlaysiz."),
    );
}

fn updates(
    ui: &mut egui::Ui,
    p: &Palette,
    c: &mut ServerConfig,
    cx: &SettingsCtx<'_>,
    out: &mut SettingsOut,
) {
    heading(ui, p, tr("Konsol versiyasi"));
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
                } => (
                    icons::CLOUD_DOWNLOAD,
                    p.accent_ink,
                    tf!(
                        "{v} yuklab olinmoqda — {pct}%",
                        v = version,
                        pct = if *total > 0 { done * 100 / total } else { 0 }
                    ),
                ),
                Phase::Installing(v) => (
                    icons::PACKAGE_CHECK,
                    p.accent_ink,
                    tf!("{v} o'rnatilmoqda…", v = v),
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
                Phase::Checking | Phase::Downloading { .. } | Phase::Installing(_) => {
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
        if let Phase::Downloading { done, total, .. } = cx.phase {
            if total > 0 {
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_height(4.0));
            }
        }
    });
    ui.add_space(8.0);
    row(
        ui,
        p,
        tr("Avtomatik tekshirish"),
        Some(tr("Konsol ishga tushganda va har 6 soatda GitHub'dan yangi versiyani tekshiradi. Yangilanish faqat siz \"O'rnatish\" tugmasini bosganingizda o'rnatiladi.")),
        |ui| {
            ui_kit::toggle(ui, &mut c.check_updates);
        },
    );
    row(ui, p, tr("Relizlar sahifasi"), None, |ui| {
        if ui_kit::button(ui, Some(icons::EXTERNAL_LINK), "GitHub", Kind::Outline).clicked() {
            config::open(vimora_update::RELEASES_PAGE);
        }
    });
    note(
        ui,
        p,
        tr("Har bir yuklab olingan fayl SHA-256 nazorat yig'indisi bilan tekshiriladi."),
    );
}

fn about(ui: &mut egui::Ui, p: &Palette, cx: &SettingsCtx<'_>) {
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
    ui.add_space(12.0);
    ui.label(RichText::new(tr("Kompyuter xonalarini boshqarish uchun ochiq kodli dastur: ekranlarni kuzatish, qulflash, namoyish, chat, internet nazorati va Shadow rejim.")).size(13.0).color(p.text));
    ui.add_space(8.0);
    row(ui, p, tr("Konsol identifikatori"), None, |ui| {
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
        Some(tr("server.json (sozlamalar) va identity.key (konsol kaliti) shu yerda. identity.key yo'qolsa, kompyuterlarni qaytadan bog'lash kerak bo'ladi.")),
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
