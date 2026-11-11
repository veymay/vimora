//! Bodies of the modals and drawers (one function per dialog).

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

pub(super) fn activity_body(
    ui: &mut egui::Ui,
    p: &Palette,
    hub: &SharedHub,
    all: &[View],
    clear_help: &mut Option<u64>,
) {
    let attention: Vec<&View> = all
        .iter()
        .filter(|v| v.alert.is_some() || v.help.is_some())
        .collect();
    if !attention.is_empty() {
        ui_kit::hud(ui, tr("Diqqat talab qiladi"), p.warning);
        for v in attention {
            ui_kit::card(p, false, false).fill(p.bg).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&v.label).family(ui_kit::bold()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if v.help.is_some()
                            && ui_kit::button(ui, Some(icons::CHECK), "", Kind::Outline)
                                .on_hover_text(tr("Yordam berildi"))
                                .clicked()
                        {
                            *clear_help = Some(v.id);
                        }
                    });
                });
                if let Some(a) = v.alert.as_ref().or(v.help.as_ref()) {
                    ui.label(RichText::new(a).color(p.muted).size(12.5));
                }
            });
        }
        ui.add_space(10.0);
    }
    ui.horizontal(|ui| {
        ui_kit::hud(ui, tr("Hodisalar"), p.muted);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui_kit::button(ui, Some(icons::TRASH_2), tr("Tozalash"), Kind::Outline).clicked() {
                hub.lock().unwrap().log.clear();
            }
        });
    });
    let h = hub.lock().unwrap();
    if h.log.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui_kit::hud(ui, tr("Hozircha hech narsa yo'q"), p.faint));
    }
    ui.add_space(4.0);
    // One clean line per event: coloured edge, icon, time, then the text (two lines at most;
    // the whole message is in the tooltip).
    let w = ui.available_width();
    for e in h.log.iter() {
        let (ic, col) = match e.level {
            Level::Info => (icons::INFO, p.muted),
            Level::Ok => (icons::CIRCLE_CHECK, p.success),
            Level::Warn => (icons::TRIANGLE_ALERT, p.warning),
        };
        let short: String = if e.text.chars().count() > 150 {
            e.text.chars().take(147).collect::<String>() + "…"
        } else {
            e.text.clone()
        };
        let text_w = w - 100.0;
        let g = ui
            .painter()
            .layout(short, FontId::proportional(13.0), p.text, text_w);
        let hgt = (g.size().y + 18.0).max(38.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, hgt), Sense::hover());
        ui.painter().rect_filled(rect, Rounding::ZERO, p.bg);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
            Rounding::ZERO,
            col,
        );
        ui.painter().text(
            pos2(rect.left() + 20.0, rect.top() + 19.0),
            Align2::CENTER_CENTER,
            ic,
            FontId::proportional(14.0),
            col,
        );
        ui.painter().text(
            pos2(rect.left() + 36.0, rect.top() + 19.0),
            Align2::LEFT_CENTER,
            &e.time[..e.time.len().min(5)],
            FontId::new(11.0, ui_kit::mono()),
            p.faint,
        );
        ui.painter()
            .galley(pos2(rect.left() + 86.0, rect.top() + 9.0), g, p.text);
        if e.text.chars().count() > 150 {
            resp.tip(&e.text);
        }
        ui.add_space(2.0);
    }
}

/// Body of every modal / drawer.
/// Read-only context the forms need besides the dialog itself.
pub(super) struct FormCtx<'a> {
    /// Newest Vimora version known.
    pub(super) latest: &'a str,
    /// Offline computers the power window would wake, and how many have a known address.
    pub(super) offline: usize,
    pub(super) offline_with_mac: usize,
    pub(super) hub: &'a SharedHub,
    pub(super) all: &'a [View],
    pub(super) targets: &'a [u64],
    /// Administrator mode (the IT tools need it when a password is set).
    pub(super) admin: bool,
}

pub(super) fn dialog_body(
    ui: &mut egui::Ui,
    p: &Palette,
    dialog: &mut Dialog,
    confirm: &mut bool,
    clear_help: &mut Option<u64>,
    cx: &FormCtx<'_>,
) {
    let (hub, all) = (cx.hub, cx.all);
    let wide = |ui: &mut egui::Ui, s: &mut String, hint: &str| {
        ui.add(
            ui_kit::text_field(s)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        )
    };
    match dialog {
        Dialog::Activity => activity_body(ui, p, hub, all, clear_help),
        Dialog::Freeze {
            tab,
            chosen,
            restart,
            folders,
            request,
            manual,
            password,
            ..
        } => shadow_form(
            ui,
            p,
            ShadowForm {
                chosen,
                tab,
                restart,
                folders,
                request,
                manual,
                password,
            },
            cx,
        ),
        Dialog::Rename {
            key,
            alias,
            system,
            system_name,
            restart,
            ..
        } => rename_form(ui, p, alias, system, system_name, restart, key),
        Dialog::Updates {
            tab,
            policy,
            restart,
            request,
        } => updates_form(ui, p, tab, policy, restart, request, cx),
        Dialog::Lesson(l) => lesson_form(ui, p, l, cx),
        Dialog::FreePcs {
            add,
            started,
            rescan,
            ..
        } => free_pcs_form(ui, p, add, started.elapsed().as_secs_f32(), rescan, cx),
        Dialog::AdminLogin {
            password, error, ..
        } => {
            ui.label(
                RichText::new(tr("Bu bo'lim administrator uchun. Parolni kiriting."))
                    .size(13.0)
                    .color(p.text),
            );
            ui.add_space(8.0);
            let r = ui.add(
                ui_kit::text_field(password)
                    .password(true)
                    .hint_text(tr("Parol"))
                    .desired_width(f32::INFINITY),
            );
            if !r.has_focus() && password.is_empty() {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                *confirm = true;
            }
            if *error {
                ui.label(
                    RichText::new(tr("Parol noto'g'ri"))
                        .size(12.5)
                        .color(p.danger),
                );
            }
        }
        Dialog::Machine {
            id,
            tab,
            filter,
            sort,
            kill,
            script,
            request,
        } => machine_form(
            ui,
            p,
            MachineForm {
                id: *id,
                tab,
                filter,
                sort,
                kill,
                script,
                request,
            },
            cx,
        ),
        Dialog::Message { .. } => {} // laid out by chat_form
        Dialog::Launch {
            web,
            url,
            program,
            args,
            bookmarks,
            recent,
        } => {
            ui_kit::segmented(p).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if ui_kit::tab_icon(ui, Some(icons::GLOBE), tr("Sayt"), None, *web).clicked() {
                        *web = true;
                    }
                    if ui_kit::tab_icon(ui, Some(icons::PLAY), tr("Dastur"), None, !*web).clicked()
                    {
                        *web = false;
                    }
                });
            });
            ui.add_space(10.0);
            if *web {
                launch_site(ui, p, url, bookmarks, recent, confirm);
            } else {
                wide(
                    ui,
                    program,
                    tr("notepad.exe · C:\\Program Files\\…\\app.exe · firefox"),
                );
                wide(ui, args, tr("Argumentlar (ixtiyoriy)"));
            }
        }
        Dialog::File(path) => {
            wide(
                ui,
                path,
                tr("Fayl yo'li: C:\\Users\\…\\dars.pdf yoki /home/…/dars.pdf"),
            );
            ui.label(
                RichText::new(tr("O'quvchining Yuklanmalar/Vimora papkasiga saqlanadi."))
                    .size(12.5)
                    .color(p.faint),
            );
        }
        Dialog::Net {
            tab,
            domains,
            keywords,
            apps,
            site_cats,
            app_cats,
            open_cat,
            devices,
            extra,
            safe,
            entry,
            import,
        } => net_form(
            ui, p, tab, domains, keywords, apps, site_cats, app_cats, open_cat, devices, extra,
            safe, entry, import,
        ),
        Dialog::Power(a) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (act, ic, label, col) in [
                    (PowerChoice::Wake, icons::POWER, tr("Yoqish"), p.accent),
                    (
                        PowerChoice::Shutdown,
                        icons::POWER_OFF,
                        tr("O'chirish"),
                        p.danger,
                    ),
                    (
                        PowerChoice::Restart,
                        icons::ROTATE_CCW,
                        tr("Qayta yuklash"),
                        p.danger,
                    ),
                    (
                        PowerChoice::Logoff,
                        icons::LOG_OUT,
                        tr("Seansdan chiqish"),
                        p.danger,
                    ),
                ] {
                    if power_tile(ui, p, ic, label, *a == act, 4.0, col).clicked() {
                        *a = act;
                    }
                }
            });
            ui.add_space(10.0);
            if *a == PowerChoice::Wake {
                let (off, with_mac) = (cx.offline, cx.offline_with_mac);
                ui.label(
                    RichText::new(if off == 0 {
                        tr("Oflayn kompyuter yo'q.").to_string()
                    } else {
                        tf!(
                            "{n} ta oflayn kompyuterga yoqish signali yuboriladi ({m} tasining manzili ma'lum).",
                            n = off,
                            m = with_mac
                        )
                    })
                    .color(p.text),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr("Kompyuterda BIOS'da Wake-on-LAN yoqilgan va tarmoq simi ulangan bo'lishi kerak."))
                        .size(12.0)
                        .color(p.faint),
                );
            } else {
                ui.label(
                    RichText::new(with_icon(
                        icons::TRIANGLE_ALERT,
                        tr("Saqlanmagan ishlar yo'qolishi mumkin!"),
                    ))
                    .color(p.warning),
                );
            }
        }
        Dialog::Settings(_) => {}
        Dialog::Room { name, .. } => {
            let r = ui.add(
                ui_kit::text_field(name)
                    .hint_text(tr("Masalan: 204-XONA, INFORMATIKA"))
                    .desired_width(f32::INFINITY),
            );
            // Room names are always in capitals, as they are typed.
            if r.changed() && name.chars().any(|c| c.is_lowercase()) {
                *name = name.to_uppercase();
            }
            if ui.memory(|m| m.focused().is_none()) {
                r.request_focus();
            }
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                *confirm = true;
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Kompyuterlarni o'ng tugma → \"Xonaga biriktirish\" orqali qo'shasiz.",
                ))
                .size(12.5)
                .color(p.faint),
            );
        }
    }
}

/// "https://www.youtube.com/" → "youtube.com": what the teacher sees; the scheme stays hidden.
pub(super) fn site_label(u: &str) -> String {
    u.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_string()
}

/// One row of a site list: globe, name, optional action icons on the right.
/// Returns (row clicked, index of the action icon clicked).
fn site_row(
    ui: &mut egui::Ui,
    p: &Palette,
    first: bool,
    label: &str,
    actions: &[(&str, Color32, &str)],
) -> (bool, Option<usize>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if !first {
        ui.painter()
            .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.line));
    }
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::ZERO, p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().text(
        pos2(rect.left() + 18.0, rect.center().y),
        Align2::CENTER_CENTER,
        icons::GLOBE,
        FontId::proportional(14.0),
        p.accent_ink,
    );
    let t = ui_kit::truncated(
        ui,
        label,
        FontId::new(13.5, ui_kit::medium()),
        p.text,
        rect.width() - 60.0 - 30.0 * actions.len() as f32,
    );
    ui.painter().galley(
        pos2(rect.left() + 38.0, rect.center().y - t.size().y / 2.0),
        t,
        p.text,
    );
    let mut hit = None;
    for (i, (icon, col, tip)) in actions.iter().enumerate() {
        let r = Rect::from_center_size(
            pos2(
                rect.right() - 20.0 - 30.0 * (actions.len() - 1 - i) as f32,
                rect.center().y,
            ),
            vec2(28.0, 28.0),
        );
        let ar = ui.interact(r, resp.id.with(("act", i)), Sense::click());
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            *icon,
            FontId::proportional(14.0),
            if ar.hovered() { *col } else { p.faint },
        );
        if ar.clone().tip(*tip).clicked() {
            hit = Some(i);
        }
    }
    (hit.is_none() && resp.clicked(), hit)
}

/// The "Sayt" tab: one box (no `https://` to type or see), favourites, recent sites.
fn launch_site(
    ui: &mut egui::Ui,
    p: &Palette,
    url: &mut String,
    bookmarks: &mut Vec<String>,
    recent: &mut Vec<String>,
    confirm: &mut bool,
) {
    let norm = crate::app::overlay::normalize_url(url);
    let saved = norm.as_ref().and_then(|u| {
        let l = site_label(u);
        bookmarks.iter().position(|b| site_label(b) == l)
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let r = ui.add(
            ui_kit::text_field(url)
                .hint_text(tr("youtube.com yoki qidiruv so'zi"))
                .desired_width(ui.available_width() - 150.0),
        );
        if ui.memory(|m| m.focused().is_none()) {
            r.request_focus();
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            *confirm = true;
        }
        ui.add_enabled_ui(norm.is_some(), |ui| {
            let (ic, label) = if saved.is_some() {
                (icons::BOOKMARK_CHECK, tr("Saqlangan"))
            } else {
                (icons::BOOKMARK_PLUS, tr("Saqlash"))
            };
            if ui_kit::button(ui, Some(ic), label, Kind::Outline).clicked() {
                match (saved, &norm) {
                    (Some(i), _) => {
                        bookmarks.remove(i);
                    }
                    (None, Some(u)) => bookmarks.push(u.clone()),
                    _ => {}
                }
            }
        });
    });

    // Favourites: one click opens the site on the students' screens.
    let head = |ui: &mut egui::Ui, icon: &str, title: &str| {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(icon).size(13.0).color(p.muted));
            ui_kit::hud(ui, title, p.muted);
        });
        ui.add_space(4.0);
    };
    head(ui, icons::BOOKMARK, tr("Saralangan"));
    if bookmarks.is_empty() {
        ui.label(
            RichText::new(tr("Bo'sh. Manzil yozib \"Saqlash\" ni bosing."))
                .size(12.5)
                .color(p.faint),
        );
    } else {
        let mut remove: Option<usize> = None;
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, b) in bookmarks.iter().enumerate() {
                    let (open, act) = site_row(
                        ui,
                        p,
                        i == 0,
                        &site_label(b),
                        &[(icons::X, p.danger, tr("Olib tashlash"))],
                    );
                    if act.is_some() {
                        remove = Some(i);
                    } else if open {
                        *url = b.clone();
                        *confirm = true;
                    }
                }
            });
        if let Some(i) = remove {
            bookmarks.remove(i);
        }
    }

    if !recent.is_empty() {
        head(ui, icons::HISTORY, tr("Oxirgi"));
        let mut remove: Option<usize> = None;
        let mut star: Option<usize> = None;
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, r) in recent.iter().enumerate() {
                    let l = site_label(r);
                    let is_saved = bookmarks.iter().any(|b| site_label(b) == l);
                    let (open, act) = site_row(
                        ui,
                        p,
                        i == 0,
                        &l,
                        &[
                            (
                                if is_saved {
                                    icons::BOOKMARK_CHECK
                                } else {
                                    icons::BOOKMARK_PLUS
                                },
                                p.accent_ink,
                                tr("Saralanganlarga"),
                            ),
                            (icons::X, p.danger, tr("Olib tashlash")),
                        ],
                    );
                    match act {
                        Some(0) => star = Some(i),
                        Some(_) => remove = Some(i),
                        None if open => {
                            *url = r.clone();
                            *confirm = true;
                        }
                        None => {}
                    }
                }
            });
        if let Some(i) = star {
            let l = site_label(&recent[i]);
            match bookmarks.iter().position(|b| site_label(b) == l) {
                Some(j) => {
                    bookmarks.remove(j);
                }
                None => bookmarks.push(recent[i].clone()),
            }
        }
        if let Some(i) = remove {
            recent.remove(i);
        }
    }
}

pub(super) fn power_tile(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    selected: bool,
    cols: f32,
    accent: Color32,
) -> egui::Response {
    tile(ui, p, icon, label, selected, accent, cols)
}

pub(super) fn tile(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    selected: bool,
    accent: Color32,
    cols: f32,
) -> egui::Response {
    // Width from the whole row (not what is left), so all tiles match.
    let w = (ui.max_rect().width() - 8.0 * (cols - 1.0)) / cols;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(100.0), 84.0), Sense::click());
    let fill = if selected {
        p.sunken
    } else if resp.hovered() {
        p.hover
    } else {
        p.bg
    };
    ui.painter()
        .rect(rect, Rounding::ZERO, fill, Stroke::new(1.0_f32, p.line));
    if selected {
        // Drawn inside the tile so every tile keeps exactly the same size.
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            Rounding::ZERO,
            Stroke::new(2.0_f32, accent),
        );
    }
    let col = if selected { accent } else { p.text };
    ui.painter().text(
        rect.center() - vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(22.0),
        col,
    );
    let g = ui_kit::caps(ui, label, 10.0, ui_kit::mono_bold(), col);
    ui.painter().galley(
        pos2(rect.center().x - g.size().x / 2.0, rect.center().y + 14.0),
        g,
        col,
    );
    resp
}

/// Section title inside drawers.
pub(super) fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.add_space(6.0);
    ui_kit::hud(ui, title, p.muted);
    ui.add_space(2.0);
}

/// Ticked list of every computer (name, state chip); "all" ticks or clears them all.
fn pick_rows(
    ui: &mut egui::Ui,
    p: &Palette,
    all: &[View],
    chosen: &mut std::collections::BTreeSet<u64>,
    chip: impl Fn(&View) -> (String, Color32),
) {
    let all_on = !all.is_empty() && all.iter().all(|v| chosen.contains(&v.id));
    egui::ScrollArea::vertical()
        .id_salt("shadow-pcs")
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            Frame::none()
                .stroke(Stroke::new(1.0_f32, p.line))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let row = |ui: &mut egui::Ui,
                               first: bool,
                               on: bool,
                               name: &str,
                               bold: bool,
                               chip: Option<(String, Color32)>,
                               tip: Option<&str>|
                     -> bool {
                        let (rect, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        if !first {
                            ui.painter().hline(
                                rect.x_range(),
                                rect.top(),
                                Stroke::new(1.0_f32, p.line),
                            );
                        }
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, Rounding::ZERO, p.hover);
                        }
                        let bx = Rect::from_center_size(
                            pos2(rect.left() + 20.0, rect.center().y),
                            vec2(16.0, 16.0),
                        );
                        ui.painter().rect(
                            bx,
                            Rounding::ZERO,
                            if on { p.accent } else { p.sunken },
                            Stroke::new(1.0_f32, if on { p.accent } else { p.line_strong }),
                        );
                        if on {
                            ui.painter().text(
                                bx.center(),
                                Align2::CENTER_CENTER,
                                icons::CHECK,
                                FontId::proportional(12.0),
                                p.on_accent,
                            );
                        }
                        let name = ui_kit::truncated(
                            ui,
                            name,
                            FontId::new(
                                13.5,
                                if bold {
                                    ui_kit::bold()
                                } else {
                                    ui_kit::medium()
                                },
                            ),
                            p.text,
                            rect.width() * 0.45,
                        );
                        ui.painter().galley(
                            pos2(rect.left() + 40.0, rect.center().y - name.size().y / 2.0),
                            name,
                            p.text,
                        );
                        if let Some((text, col)) = chip {
                            let g = ui_kit::caps(ui, &text, 9.5, ui_kit::mono(), col);
                            let r = Rect::from_min_size(
                                pos2(
                                    rect.right() - 12.0 - g.size().x - 12.0,
                                    rect.center().y - 10.0,
                                ),
                                vec2(g.size().x + 12.0, 20.0),
                            );
                            ui_kit::paint_chip(ui.painter(), r, g, col);
                        }
                        if let Some(t) = tip {
                            resp.clone().tip(t);
                        }
                        resp.clicked()
                    };
                    if row(ui, true, all_on, tr("Hammasi"), true, None, None) {
                        if all_on {
                            chosen.clear();
                        } else {
                            chosen.extend(all.iter().map(|v| v.id));
                        }
                    }
                    for v in all {
                        let on = chosen.contains(&v.id);
                        let note = v.status.shield_note.as_deref().filter(|_| v.status.service);
                        if row(ui, false, on, &v.label, false, Some(chip(v)), note) {
                            if on {
                                chosen.remove(&v.id);
                            } else {
                                chosen.insert(v.id);
                            }
                        }
                    }
                });
        });
}

/// One row per target PC: name on the left, a status chip on the right.
#[allow(dead_code)]
pub(super) fn target_rows(
    ui: &mut egui::Ui,
    p: &Palette,
    all: &[View],
    targets: &[u64],
    chip: impl Fn(&View) -> (String, Color32),
) {
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, v) in all.iter().filter(|v| targets.contains(&v.id)).enumerate() {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                if i > 0 {
                    ui.painter()
                        .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.line));
                }
                let name = ui_kit::truncated(
                    ui,
                    &v.label,
                    FontId::new(13.5, ui_kit::bold()),
                    p.text,
                    rect.width() * 0.45,
                );
                ui.painter().galley(
                    pos2(rect.left() + 12.0, rect.center().y - name.size().y / 2.0),
                    name,
                    p.text,
                );
                let (text, col) = chip(v);
                let g = ui_kit::caps(ui, &text, 9.5, ui_kit::mono(), col);
                let r = Rect::from_min_size(
                    pos2(
                        rect.right() - 12.0 - g.size().x - 12.0,
                        rect.center().y - 10.0,
                    ),
                    vec2(g.size().x + 12.0, 20.0),
                );
                ui_kit::paint_chip(ui.painter(), r, g, col);
            }
        });
}

fn note(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(p.muted));
}

/// The Shadow drawer: one page — the switch, the computers, and what to commit.
pub(super) struct ShadowForm<'a> {
    pub(super) chosen: &'a mut std::collections::BTreeSet<u64>,
    pub(super) tab: &'a mut ShadowTab,
    pub(super) restart: &'a mut bool,
    pub(super) folders: &'a mut String,
    pub(super) request: &'a mut Option<ShadowRequest>,
    pub(super) manual: &'a mut String,
    pub(super) password: &'a mut String,
}

/// Small caption with a "?" that explains it on hover.
fn titled(ui: &mut egui::Ui, p: &Palette, title: &str, help: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui_kit::hud(ui, title, p.muted);
        ui_kit::info(ui, help);
    });
    ui.add_space(2.0);
}

pub(super) fn shadow_form(ui: &mut egui::Ui, p: &Palette, mut f: ShadowForm<'_>, cx: &FormCtx<'_>) {
    // Two tabs only: each filter has its protection, "saqlab qolish" and settings in one page.
    if matches!(*f.tab, ShadowTab::Commit | ShadowTab::Settings) {
        *f.tab = ShadowTab::Protection;
    }
    // The protection tabs: Shadow Defender (1) or UWF (2). A computer counts here when it uses
    // this filter, or is not tied to one (Ubuntu, older agents).
    let engine: u8 = if *f.tab == ShadowTab::Uwf { 2 } else { 1 };
    let mine = |v: &View| {
        let e = engine_of(v);
        e == 0 || e == engine
    };
    let views: Vec<&View> = cx.all.iter().filter(|v| f.chosen.contains(&v.id)).collect();
    let usable = views
        .iter()
        .filter(|v| {
            v.status.service
                && (engine == 2
                    || !matches!(
                        v.status.freeze,
                        FreezeState::Unsupported | FreezeState::Unknown
                    ))
        })
        .count();
    let want_on = |v: &View| {
        mine(v)
            && matches!(
                v.status.freeze,
                FreezeState::Frozen | FreezeState::FreezePending | FreezeState::Maintenance
            )
    };
    let mut on = usable > 0
        && views.iter().all(|v| {
            !v.status.service
                || (mine(v)
                    && matches!(
                        v.status.freeze,
                        FreezeState::Unsupported | FreezeState::Unknown
                    ))
                || want_on(v)
        });
    let pending = views.iter().any(|v| {
        v.status.service
            && mine(v)
            && matches!(
                v.status.freeze,
                FreezeState::FreezePending | FreezeState::ThawPending | FreezeState::Maintenance
            )
    });
    let no_service = views.iter().filter(|v| !v.status.service).count();
    let unsupported = views
        .iter()
        .filter(|v| {
            v.status.service
                && mine(v)
                && matches!(
                    v.status.freeze,
                    FreezeState::Unsupported | FreezeState::Unknown
                )
        })
        .count();

    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, ic, label) in [
                (ShadowTab::Uwf, icons::SHIELD, "UWF"),
                (ShadowTab::Protection, icons::SNOWFLAKE, "Shadow Defender"),
            ] {
                if ui_kit::tab_icon(ui, Some(ic), label, None, *f.tab == t).clicked() {
                    *f.tab = t;
                }
            }
        });
    });
    ui.add_space(12.0);

    if matches!(*f.tab, ShadowTab::Protection | ShadowTab::Uwf) {
        // A switch flipped a moment ago: show where it is going until the computers report
        // back (Shadow changes take a few seconds and often only finish at the next restart).
        let wait_id = egui::Id::new("shadow-switch-wait");
        let now = ui.input(|i| i.time);
        let mut waiting: Option<(bool, f64)> = ui.data(|d| d.get_temp(wait_id));
        if let Some((target, since)) = waiting {
            if (usable > 0 && on == target) || now - since > 20.0 {
                waiting = None;
                ui.data_mut(|d| d.remove::<(bool, f64)>(wait_id));
            }
        }
        if let Some((target, _)) = waiting {
            on = target;
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        // The switch card: icon, title and state on a fixed grid, the switch centred.
        let w = ui.available_width();
        let (card, _) = ui.allocate_exact_size(vec2(w, 68.0), Sense::hover());
        let edge = if waiting.is_some() {
            p.warning
        } else if on {
            p.accent
        } else {
            p.line
        };
        ui.painter()
            .rect(card, Rounding::ZERO, p.bg, Stroke::new(1.0_f32, edge));
        let cy = card.center().y;
        ui.painter().text(
            pos2(card.left() + 30.0, cy),
            Align2::CENTER_CENTER,
            if engine == 2 {
                icons::SHIELD
            } else {
                icons::SNOWFLAKE
            },
            FontId::proportional(26.0),
            if on { p.accent_ink } else { p.faint },
        );
        let text_x = card.left() + 58.0;
        let title = ui.painter().layout_no_wrap(
            if engine == 2 {
                "UWF"
            } else {
                "Shadow Defender"
            }
            .into(),
            FontId::new(15.0, ui_kit::bold()),
            p.strong,
        );
        let title_w = title.size().x;
        ui.painter()
            .galley(pos2(text_x, cy - 2.0 - title.size().y), title, p.strong);
        let mut help_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(text_x + title_w + 6.0, cy - 21.0),
                    vec2(20.0, 20.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        ui_kit::info(
            &mut help_ui,
            if engine == 2 {
                tr("Windows 10/11 Education va Enterprise'dagi o'rnatilgan himoya (Unified Write Filter). Yoniqda qayta yuklanganda barcha o'zgarishlar o'chadi. Birinchi yoqishda komponent o'rnatiladi va kompyuter bir marta qayta yuklanadi.")
            } else {
                tr("Yoniqda qayta yuklanganda barcha o'zgarishlar o'chadi, kompyuter toza qaytadi. Konsoldan qilganingiz saqlanadi. Windows'da Shadow Defender o'rnatilgan bo'lishi kerak; Ubuntu'da o'zi ishlaydi.")
            },
        );
        let (txt, col) = if let Some((target, _)) = waiting {
            (
                if target {
                    tr("Yoqilmoqda…")
                } else {
                    tr("O'chirilmoqda…")
                },
                p.warning,
            )
        } else if views.is_empty() {
            (tr("Pastdan kompyuter belgilang"), p.faint)
        } else if usable == 0 {
            (tr("Mavjud emas"), p.faint)
        } else if pending {
            (tr("Qayta yuklangach kuchga kiradi"), p.warning)
        } else if on {
            (tr("Yoniq"), p.accent_ink)
        } else {
            (tr("O'chiq"), p.muted)
        };
        ui.painter().text(
            pos2(text_x, cy + 3.0),
            Align2::LEFT_TOP,
            txt,
            FontId::proportional(12.5),
            col,
        );
        let mut sw = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_center_size(
                    pos2(card.right() - 36.0, cy),
                    vec2(40.0, 22.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        sw.add_enabled_ui(usable > 0 && waiting.is_none(), |ui| {
            let was = on;
            if ui_kit::toggle(ui, &mut on).clicked() && on != was {
                *f.request = Some(ShadowRequest::Apply(if on {
                    FreezeAction::Freeze
                } else {
                    FreezeAction::Thaw
                }));
                ui.data_mut(|d| d.insert_temp(wait_id, (on, now)));
            }
        });

        // Problems, one short line each.
        for (n, text) in [
            (no_service, tr("{n} ta kompyuterda Vimora xizmati yo'q")),
            (
                unsupported,
                if engine == 2 {
                    tr("{n} ta kompyuterda UWF yo'q (Windows Education yoki Enterprise kerak)")
                } else {
                    tr("{n} ta kompyuterda Shadow yo'q (Windows: Shadow Defender kerak)")
                },
            ),
        ] {
            if n > 0 {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(with_icon(
                        icons::TRIANGLE_ALERT,
                        &text.replace("{n}", &n.to_string()),
                    ))
                    .size(12.0)
                    .color(p.warning),
                );
            }
        }

        titled(
            ui,
            p,
            tr("Kompyuterlar"),
            tr("Har bir kompyuterda Shadow holati. Foiz — xotira to'lishi."),
        );
        pick_rows(ui, p, cx.all, f.chosen, |v| {
            if !v.status.service {
                return (tr("Xizmat yo'q").into(), p.danger);
            }
            if !mine(v) {
                // Protected by the other filter: say which.
                let other = if engine == 2 {
                    "Shadow Defender"
                } else {
                    "UWF"
                };
                return (other.into(), p.faint);
            }
            if v.status.freeze == FreezeState::Unsupported {
                return (tr("Mavjud emas").into(), p.faint);
            }
            let mut t = freeze_label(v.status.freeze).to_string();
            if let Some(u) = v.status.shield_usage {
                t.push_str(&format!(" · {}%", u.percent()));
            }
            (t, freeze_color(p, v.status.freeze))
        });
    }

    // Update cycle and restart: part of switching the protection on and off.
    ui.add_space(12.0);
    {
        let usable = views.iter().any(|v| v.status.service);
        sgroup(ui, p, |ui| {
            srow(
                ui,
                p,
                true,
                tr("Yangilab yoqish"),
                Some(tr(
                    "Shadow vaqtincha o'chadi, yangilanishlar o'rnatiladi, keyin Shadow o'zi yoqiladi.",
                )),
                |ui| {
                    ui.add_enabled_ui(usable, |ui| {
                        if ui_kit::button(ui, Some(icons::WRENCH), tr("Boshlash"), Kind::Outline)
                            .clicked()
                        {
                            *f.request = Some(ShadowRequest::Apply(FreezeAction::Maintenance));
                        }
                    });
                },
            );
            srow(
                ui,
                p,
                false,
                tr("Darhol qayta yuklash"),
                Some(tr("Shadow yoqilganda yoki o'chirilganda kompyuterlar darhol qayta yuklanadi. Saqlanmagan ish yo'qolishi mumkin.")),
                |ui| {
                    ui_kit::toggle(ui, f.restart);
                },
            );
        });
    }

    // Saqlab qolish: what to keep from the frozen session.
    freeze_section(
        ui,
        p,
        tr("Saqlab qolish"),
        tr("Muzlatilgan kompyuterdagi kerakli o'zgarishlarni qayta yuklashdan keyin ham saqlab qolish."),
    );
    commit_section(ui, p, &mut f, cx);

    // Settings of this filter (Shadow Defender only: UWF has none).
    if engine == 1 {
        freeze_section(
            ui,
            p,
            tr("Sozlamalar"),
            tr("Shu himoya usulining sozlamalari."),
        );
        caption(
            ui,
            p,
            tr("Shadow Defender paroli"),
            tr("Windows kompyuterlardagi Shadow Defender paroli. Parol yo'q bo'lsa, bo'sh qoldiring."),
        );
        ui_kit::input_frame(p).show(ui, |ui| {
            ui.add(
                ui_kit::text_field(f.password)
                    .password(true)
                    .frame(false)
                    .desired_width(ui.available_width()),
            );
        });
    }
}

/// A section heading inside the Muzlatish page: a rule, the title and a "?".
fn freeze_section(ui: &mut egui::Ui, p: &Palette, title: &str, help: &str) {
    ui.add_space(22.0);
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, p.line));
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(title)
                .family(ui_kit::bold())
                .size(15.0)
                .color(p.strong),
        );
        ui_kit::info(ui, help);
    });
    ui.add_space(8.0);
}

/// A small caption with a "?" above a field.
fn caption(ui: &mut egui::Ui, p: &Palette, text: &str, help: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui_kit::hud(ui, text, p.muted);
        ui_kit::info(ui, help);
    });
    ui.add_space(4.0);
}

/// Files and folders that always survive restarts (a list like the block lists: add, remove;
/// every change goes to the computers at once) and one button that commits them now.
fn commit_section(ui: &mut egui::Ui, p: &Palette, f: &mut ShadowForm<'_>, cx: &FormCtx<'_>) {
    let usable = cx
        .all
        .iter()
        .any(|v| f.chosen.contains(&v.id) && v.status.service);
    titled(
        ui,
        p,
        tr("Doim saqlanadigan fayl va papkalar"),
        tr("Ro'yxatdagi fayl va papkalar (ichidagi hamma fayllar bilan) qayta yuklashdan keyin ham saqlanib qoladi. Qo'shish yoki olib tashlash darhol kompyuterlarga yuboriladi."),
    );
    // Dev aid: VIMORA_OPEN=shadow-commit shows this part (for screenshots).
    if std::env::var("VIMORA_OPEN").as_deref() == Ok("shadow-commit") {
        ui.scroll_to_cursor(Some(Align::TOP));
    }
    // The list (one path per line in `folders`).
    let mut list: Vec<String> = f
        .folders
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let before = list.clone();
    let w = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(w, ui_kit::CONTROL_H),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let plus = ui_kit::button(ui, Some(icons::PLUS), tr("Qo'shish"), Kind::Outline);
            let r = ui.add(
                ui_kit::text_field(f.manual)
                    .hint_text(tr(
                        "Masalan: D:\\Darslar yoki C:\\Users\\Public\\Desktop\\dars.pptx",
                    ))
                    .desired_width(ui.available_width()),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if plus.clicked() || enter {
                let path = f
                    .manual
                    .trim()
                    .trim_matches('"')
                    .trim_end_matches(['\\', '/'])
                    .to_string();
                if !path.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&path)) {
                    list.push(path);
                }
                f.manual.clear();
            }
        },
    );
    ui.add_space(8.0);
    if list.is_empty() {
        ui.label(
            RichText::new(tr("Ro'yxat bo'sh."))
                .size(12.5)
                .color(p.faint),
        );
    } else {
        let mut remove = None;
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, path) in list.iter().enumerate() {
                    let rw = ui.available_width();
                    let (rect, _) = ui.allocate_exact_size(vec2(rw, 38.0), Sense::hover());
                    if i > 0 {
                        ui.painter().hline(
                            rect.x_range(),
                            rect.top(),
                            Stroke::new(1.0_f32, p.line),
                        );
                    }
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    child.spacing_mut().item_spacing.x = 10.0;
                    let folder = !path.rsplit(['\\', '/']).next().unwrap_or("").contains('.');
                    child.label(ui_kit::icon(
                        if folder {
                            icons::FOLDER
                        } else {
                            icons::FILE_TEXT
                        },
                        16.0,
                        p.accent,
                    ));
                    let g = ui_kit::truncated(
                        &child,
                        path,
                        FontId::new(13.0, ui_kit::mono()),
                        p.text,
                        rw - 90.0,
                    );
                    child.label(egui::WidgetText::from(g));
                    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui_kit::icon_cell(ui, icons::X, 30.0, false)
                            .tip(tr("Olib tashlash"))
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            list.remove(i);
        }
    }
    if list != before {
        *f.folders = list.join("\n");
        // No separate "save": the computers get the new list right away.
        if usable {
            *f.request = Some(ShadowRequest::SaveFolders);
        }
    }
    ui.add_space(12.0);
    ui.add_enabled_ui(usable && !list.is_empty(), |ui| {
        if ui_kit::button(
            ui,
            Some(icons::SHIELD_CHECK),
            tr("Hozirgi o'zgarishlarni commit qilish"),
            Kind::Primary,
        )
        .tip(tr(
            "Ro'yxatdagi fayl va papkalarning hozirgi holati darhol saqlanadi (commit)",
        ))
        .clicked()
        {
            *f.request = Some(ShadowRequest::Commit);
        }
    });
}

pub(super) fn rename_form(
    ui: &mut egui::Ui,
    p: &Palette,
    alias: &mut String,
    system: &mut bool,
    system_name: &mut String,
    restart: &mut bool,
    key: &str,
) {
    section(ui, p, tr("Konsoldagi nomi"));
    ui.add(
        ui_kit::text_field(alias)
            .hint_text(key)
            .desired_width(f32::INFINITY),
    );
    note(ui, p, tr("Faqat shu konsolda ko'rinadi, darhol o'zgaradi. Bo'sh qoldirsangiz — tizim nomi ko'rsatiladi."));
    ui.add_space(8.0);
    ui.checkbox(system, tr("Kompyuterning tizim nomini ham o'zgartirish"));
    if *system {
        ui.add(
            ui_kit::text_field(system_name)
                .hint_text("PC-12")
                .desired_width(f32::INFINITY),
        );
        let ok = vimora_core::valid_computer_name(system_name.trim());
        note(
            ui,
            p,
            if ok {
                tr("Windows'da qayta yuklangandan keyin kuchga kiradi. Shadow rejimda ham saqlanadi (commit).")
            } else {
                tr("1–15 ta lotin harfi, raqam yoki '-' (faqat raqamdan iborat bo'lmasin).")
            },
        );
        ui.checkbox(restart, tr("Darhol qayta yuklash"));
    }
}

/// One settings line inside a drawer: title (+ "?" help) on the left, a control on the right.
fn srow(
    ui: &mut egui::Ui,
    p: &Palette,
    first: bool,
    title: &str,
    help: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    if !first {
        ui.painter()
            .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.line));
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(12.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 4.0;
    child.label(RichText::new(title).size(13.5).color(p.text));
    if let Some(h) = help {
        ui_kit::info(&mut child, h);
    }
    child.with_layout(Layout::right_to_left(Align::Center), control);
}

/// A bordered group of [`srow`]s.
fn sgroup(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
}

/// "…45%" at the end of a status line → 45.
fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%')
        .and_then(|s| s.rsplit(' ').next())
        .and_then(|n| n.parse::<f32>().ok())
}

/// The computers of the Updates drawer as a table: name, a value (version / pending count) and
/// the state, with a progress bar while something is running.
fn update_table(
    ui: &mut egui::Ui,
    p: &Palette,
    views: &[&View],
    head: [&str; 3],
    cell: impl Fn(&View) -> (String, String, Color32),
    // Is this computer's running job part of this tab?
    mine: impl Fn(&str) -> bool,
) {
    let w = ui.available_width();
    let (c1, c2) = (w * 0.34, w * 0.2);
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            ui.painter().rect_filled(hr, Rounding::ZERO, p.bg);
            for (x, t) in [(0.0, head[0]), (c1, head[1]), (c1 + c2, head[2])] {
                let g = ui_kit::caps(ui, t, 9.5, ui_kit::mono(), p.faint);
                ui.painter().galley(
                    pos2(hr.left() + 12.0 + x, hr.center().y - g.size().y / 2.0),
                    g,
                    p.faint,
                );
            }
            if views.is_empty() {
                let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    tr("Onlayn kompyuter yo'q"),
                    FontId::proportional(12.5),
                    p.faint,
                );
            }
            for v in views {
                let (r, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                let y = r.center().y;
                let name = ui_kit::truncated(
                    ui,
                    &v.label,
                    FontId::new(13.0, ui_kit::bold()),
                    p.text,
                    c1 - 20.0,
                );
                ui.painter()
                    .galley(pos2(r.left() + 12.0, y - name.size().y / 2.0), name, p.text);
                let (value, state, col) = cell(v);
                ui.painter().text(
                    pos2(r.left() + 12.0 + c1, y),
                    Align2::LEFT_CENTER,
                    value,
                    FontId::new(12.0, ui_kit::mono()),
                    p.muted,
                );
                let right = Rect::from_min_max(pos2(r.left() + 12.0 + c1 + c2, r.top()), r.max);
                match v.status.busy.as_deref().filter(|b| mine(b)) {
                    Some(b) => {
                        // Running: a bar (exact when the agent reports a percent).
                        let bar = Rect::from_min_size(
                            pos2(right.left(), y - 3.0),
                            vec2(right.width() - 58.0, 6.0),
                        );
                        ui.painter().rect_filled(bar, Rounding::ZERO, p.sunken);
                        match percent_of(b) {
                            Some(pct) => {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        bar.min,
                                        vec2(bar.width() * pct / 100.0, 6.0),
                                    ),
                                    Rounding::ZERO,
                                    p.accent,
                                );
                                ui.painter().text(
                                    pos2(right.right() - 12.0, y),
                                    Align2::RIGHT_CENTER,
                                    format!("{pct:.0}%"),
                                    FontId::new(12.0, ui_kit::mono()),
                                    p.accent_ink,
                                );
                            }
                            None => {
                                // No percent known: a block sliding along the bar.
                                let t = (ui.input(|i| i.time) as f32 * 0.7).fract();
                                let bw = bar.width() * 0.3;
                                let x = bar.left() + (bar.width() - bw) * t;
                                ui.painter().rect_filled(
                                    Rect::from_min_size(pos2(x, bar.top()), vec2(bw, 6.0)),
                                    Rounding::ZERO,
                                    p.accent,
                                );
                            }
                        }
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(60));
                    }
                    None => {
                        let g = ui_kit::truncated(
                            ui,
                            &state,
                            FontId::proportional(12.5),
                            col,
                            right.width() - 12.0,
                        );
                        ui.painter()
                            .galley(pos2(right.left(), y - g.size().y / 2.0), g, col);
                    }
                }
            }
        });
}

/// Every kind of update in one place, one tab each: the Vimora agent, the system, applications.
pub(super) fn updates_form(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut UpdateTab,
    policy: &mut UpdatePolicy,
    restart: &mut bool,
    request: &mut Option<UpdateRequest>,
    cx: &FormCtx<'_>,
) {
    let views: Vec<&View> = cx
        .all
        .iter()
        .filter(|v| v.online && cx.targets.contains(&v.id))
        .collect();
    let before = *policy;
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, ic, label) in [
                (UpdateTab::Apps, icons::LAYOUT_GRID, tr("Dasturlar")),
                (UpdateTab::System, icons::MONITOR, "Windows"),
                (UpdateTab::Vimora, icons::CLOUD_DOWNLOAD, "Vimora"),
            ] {
                if ui_kit::tab_icon(ui, Some(ic), label, None, *tab == t).clicked() {
                    *tab = t;
                }
            }
        });
    });
    ui.add_space(12.0);
    match tab {
        UpdateTab::Vimora => {
            sgroup(ui, p, |ui| {
                srow(
                    ui,
                    p,
                    true,
                    tr("Avtomatik yangilanish"),
                    Some(tr(
                        "O'quvchi kompyuterlari yangi Vimora versiyasini o'zi topib o'rnatadi.",
                    )),
                    |ui| {
                        ui_kit::toggle(ui, &mut policy.vimora);
                    },
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!views.is_empty(), |ui| {
                    if ui_kit::button(
                        ui,
                        Some(icons::CLOUD_DOWNLOAD),
                        tr("Hozir yangilash"),
                        Kind::Primary,
                    )
                    .clicked()
                    {
                        *request = Some(UpdateRequest::Vimora);
                    }
                });
                ui.label(
                    RichText::new(tf!("Eng so'nggi: {v}", v = cx.latest))
                        .size(12.5)
                        .color(p.muted),
                );
            });
            ui.add_space(10.0);
            update_table(
                ui,
                p,
                &views,
                [tr("Kompyuter"), tr("Versiya"), tr("Holat")],
                |v| {
                    let ver = if v.version.is_empty() {
                        "—".to_string()
                    } else {
                        v.version.clone()
                    };
                    let current =
                        !v.version.is_empty() && !vimora_update::is_newer(cx.latest, &v.version);
                    if current {
                        (ver, tr("eng so'nggi").to_string(), p.success)
                    } else {
                        (ver, tr("yangilash kerak").to_string(), p.warning)
                    }
                },
                |b| b.contains("Vimora"),
            );
        }
        UpdateTab::System => {
            sgroup(ui, p, |ui| {
                srow(
                    ui,
                    p,
                    true,
                    tr("Rejim"),
                    Some(tr("Avto: tizim o'zi o'rnatadi. Qo'lda: faqat siz shu yerdan. O'chiq: yangilanmaydi.")),
                    |ui| {
                        ui_kit::segmented(p).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            // Right-to-left row: add last-first so they read in order.
                            for (m, label) in [
                                (OsUpdateMode::Disabled, tr("O'chiq")),
                                (OsUpdateMode::Manual, tr("Qo'lda")),
                                (OsUpdateMode::Auto, tr("Avto")),
                            ] {
                                if ui_kit::tab(ui, label, None, policy.os == m).clicked() {
                                    policy.os = m;
                                }
                            }
                        });
                    },
                );
                srow(
                    ui,
                    p,
                    false,
                    tr("O'rnatgach qayta yuklash"),
                    Some(tr("Kerak bo'lsa, kompyuter o'zi qayta yuklanadi.")),
                    |ui| {
                        ui_kit::toggle(ui, restart);
                    },
                );
            });
            ui.add_space(8.0);
            ui.add_enabled_ui(!views.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline)
                        .clicked()
                    {
                        *request = Some(UpdateRequest::Check);
                    }
                    if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("O'rnatish"), Kind::Primary)
                        .clicked()
                    {
                        *request = Some(UpdateRequest::InstallSystem);
                    }
                });
            });
            ui.add_space(10.0);
            update_table(
                ui,
                p,
                &views,
                [tr("Kompyuter"), tr("Kutmoqda"), tr("Holat")],
                |v| match v.status.os_updates {
                    None => ("—".into(), tr("tekshirilmagan").to_string(), p.faint),
                    Some(0) => ("0".into(), tr("yangi").to_string(), p.success),
                    Some(n) => (n.to_string(), tr("yangilash kerak").to_string(), p.warning),
                },
                |b| !b.contains("Vimora"),
            );
        }
        UpdateTab::Apps => {
            sgroup(ui, p, |ui| {
                srow(
                    ui,
                    p,
                    true,
                    tr("Avtomatik yangilanish"),
                    Some(tr("Store, Chrome, Edge, snap dasturlari o'zi yangilanadi.")),
                    |ui| {
                        ui_kit::toggle(ui, &mut policy.apps_auto);
                    },
                );
            });
            ui.add_space(8.0);
            ui.add_enabled_ui(!views.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline)
                        .clicked()
                    {
                        *request = Some(UpdateRequest::Check);
                    }
                    if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("O'rnatish"), Kind::Primary)
                        .clicked()
                    {
                        *request = Some(UpdateRequest::InstallApps);
                    }
                });
            });
            ui.add_space(10.0);
            update_table(
                ui,
                p,
                &views,
                [tr("Kompyuter"), tr("Kutmoqda"), tr("Holat")],
                |v| match v.status.app_updates {
                    None => ("—".into(), tr("tekshirilmagan").to_string(), p.faint),
                    Some(0) => ("0".into(), tr("yangi").to_string(), p.success),
                    Some(n) => (n.to_string(), tr("yangilash kerak").to_string(), p.warning),
                },
                |b| !b.contains("Vimora"),
            );
        }
    }
    // Switches apply at once: no "Save" button in this drawer.
    if *policy != before {
        *request = Some(UpdateRequest::Policy);
    }
}

/// Block-list text ⇄ items. A leading `#` keeps an item in the list but switched off.
fn list_items(s: &str) -> Vec<(bool, String)> {
    split_list(s)
        .into_iter()
        .map(|i| match i.strip_prefix('#') {
            Some(rest) => (false, rest.trim().to_string()),
            None => (true, i),
        })
        .filter(|(_, t)| !t.is_empty())
        .collect()
}

fn items_text(items: &[(bool, String)]) -> String {
    items
        .iter()
        .map(|(on, t)| if *on { t.clone() } else { format!("#{t}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Adds lines to a block list, skipping what is already there. Returns how many were new.
pub(super) fn add_lines(list: &mut String, text: &str) -> usize {
    let mut items = list_items(list);
    let mut added = 0;
    for raw in split_list(text) {
        let t = raw.trim_start_matches('#').trim().to_string();
        if t.is_empty() || t.len() > 200 {
            continue;
        }
        if !items.iter().any(|(_, x)| x.eq_ignore_ascii_case(&t)) {
            items.push((true, t));
            added += 1;
        }
    }
    *list = items_text(&items);
    added
}

/// The editable list of one tab: add box, import, ready-made sets, rows with a switch and a
/// delete button.
#[allow(clippy::too_many_arguments)]
fn block_list(
    ui: &mut egui::Ui,
    p: &Palette,
    list: &mut String,
    entry: &mut String,
    import: &mut bool,
    hint: &str,
    help: &str,
    sets: &[(&str, &[&str])],
    extra: impl FnOnce(&mut egui::Ui),
) {
    // Right to left: the buttons keep their size, the text box takes what is left (nothing
    // sticks out of the drawer, whatever the language).
    let w = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(w, ui_kit::CONTROL_H),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui_kit::info(ui, help);
            if ui_kit::icon_cell(ui, icons::FILE_UP, ui_kit::CONTROL_H, false)
                .tip(tr("Matn faylidan yuklash (har qatorda bittadan)"))
                .clicked()
            {
                *import = true;
            }
            let add = ui_kit::button(ui, Some(icons::PLUS), tr("Qo'shish"), Kind::Outline);
            let r = ui.add(
                ui_kit::text_field(entry)
                    .hint_text(hint)
                    .desired_width(ui.available_width()),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (add.clicked() || enter) && !entry.trim().is_empty() {
                add_lines(list, entry);
                entry.clear();
                r.request_focus();
            }
        },
    );
    ui.add_space(8.0);
    if !sets.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for (name, items) in sets {
                if ui_kit::button(ui, Some(icons::PLUS), name, Kind::Ghost).clicked() {
                    add_lines(list, &items.join("\n"));
                }
            }
        });
        ui.add_space(8.0);
    }
    extra(ui);

    let mut items = list_items(list);
    if items.is_empty() {
        if !sets.is_empty() {
            ui.label(
                RichText::new(tr("Ro'yxat bo'sh."))
                    .size(12.5)
                    .color(p.faint),
            );
        }
        return;
    }
    let on_count = items.iter().filter(|(on, _)| *on).count();
    ui.horizontal(|ui| {
        ui_kit::hud(
            ui,
            &if sets.is_empty() {
                tf!(
                    "Qo'lda qo'shilgan: {a} / {b}",
                    a = on_count,
                    b = items.len()
                )
            } else {
                tf!("{a} / {b} yoqilgan", a = on_count, b = items.len())
            },
            p.muted,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui_kit::button(
                ui,
                Some(icons::TRASH_2),
                tr("Hammasini o'chirish"),
                Kind::Ghost,
            )
            .clicked()
            {
                items.clear();
            }
        });
    });
    ui.add_space(4.0);
    let mut remove: Option<usize> = None;
    let mut changed = false;
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, (on, text)) in items.iter_mut().enumerate() {
                let w = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
                if i > 0 {
                    ui.painter()
                        .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.line));
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(vec2(10.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child.spacing_mut().item_spacing.x = 10.0;
                if ui_kit::toggle(&mut child, on).changed() {
                    changed = true;
                }
                let g = ui_kit::truncated(
                    &child,
                    text,
                    FontId::new(13.0, ui_kit::mono()),
                    if *on { p.text } else { p.faint },
                    w - 130.0,
                );
                child.label(egui::WidgetText::from(g));
                child.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui_kit::icon_cell(ui, icons::X, 30.0, false)
                        .tip(tr("Olib tashlash"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
        });
    if let Some(i) = remove {
        items.remove(i);
        changed = true;
    }
    if changed || items.len() != list_items(list).len() {
        *list = items_text(&items);
    }
}

/// Accordion of block categories: a row per category (whole-category switch, count) that opens
/// to the items inside it, each with its own switch. Imported lists can be removed.
fn categories(
    ui: &mut egui::Ui,
    p: &Palette,
    cats: &mut Vec<crate::blockcat::BlockCategory>,
    defs: &[crate::blockcat::Def],
    open: &mut Option<String>,
) {
    let mut remove: Option<usize> = None;
    let row_h = 44.0;
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, cat) in cats.iter_mut().enumerate() {
                let def = crate::blockcat::def(defs, &cat.id);
                let name = def.map_or(cat.name.as_str(), |d| tr(d.name));
                let icon = def.map_or(icons::FILE_TEXT, |d| d.icon);
                let n_on = cat.blocked().len();
                let expanded = open.as_deref() == Some(cat.id.as_str());
                let w = ui.available_width();
                let (rect, resp) = ui.allocate_exact_size(vec2(w, row_h), Sense::click());
                if i > 0 {
                    ui.painter()
                        .hline(rect.x_range(), rect.top(), Stroke::new(1.0_f32, p.line));
                }
                if resp.hovered() || expanded {
                    ui.painter().rect_filled(
                        rect.shrink2(vec2(0.0, 0.5)),
                        egui::Rounding::ZERO,
                        p.hover,
                    );
                }
                if n_on > 0 {
                    // Accent edge: something in here is blocked.
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, vec2(3.0, row_h)),
                        egui::Rounding::ZERO,
                        if cat.all { p.accent } else { p.warning },
                    );
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(vec2(12.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child.spacing_mut().item_spacing.x = 10.0;
                child.label(
                    RichText::new(if expanded {
                        icons::CHEVRON_DOWN
                    } else {
                        icons::CHEVRON_RIGHT
                    })
                    .size(15.0)
                    .color(p.muted),
                );
                child.label(RichText::new(icon).size(17.0).color(if n_on > 0 {
                    p.accent
                } else {
                    p.muted
                }));
                let g = ui_kit::truncated(
                    &child,
                    name,
                    FontId::new(14.0, ui_kit::bold()),
                    p.strong,
                    w - 250.0,
                );
                child.label(egui::WidgetText::from(g));
                let mut toggled = false;
                child.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let mut all = cat.all;
                    if ui_kit::toggle(ui, &mut all)
                        .tip(if all {
                            tr("Toifa to'liq bloklangan")
                        } else {
                            tr("Butun toifani bloklash")
                        })
                        .changed()
                    {
                        cat.set_all(all);
                        toggled = true;
                    }
                    let (txt, col) = if cat.all {
                        (tr("Hammasi").to_string(), p.accent)
                    } else if n_on > 0 {
                        (format!("{n_on} / {}", cat.items.len()), p.warning)
                    } else {
                        (format!("{}", cat.items.len()), p.faint)
                    };
                    ui_kit::hud(ui, &txt, col);
                    if cat.imported()
                        && ui_kit::icon_cell(ui, icons::TRASH_2, 30.0, false)
                            .tip(tr("Toifani o'chirish"))
                            .clicked()
                    {
                        remove = Some(i);
                        toggled = true;
                    }
                });
                if resp.clicked() && !toggled {
                    *open = if expanded { None } else { Some(cat.id.clone()) };
                }
                if expanded {
                    let items = cat.items.clone();
                    Frame::none()
                        .fill(p.sunken)
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let col_w = (ui.available_width() - 12.0) / 2.0;
                            for pair in items.chunks(2) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    for item in pair {
                                        let (r, _) = ui
                                            .allocate_exact_size(vec2(col_w, 34.0), Sense::hover());
                                        let mut c = ui.new_child(
                                            egui::UiBuilder::new()
                                                .max_rect(r)
                                                .layout(Layout::left_to_right(Align::Center)),
                                        );
                                        c.spacing_mut().item_spacing.x = 10.0;
                                        let mut on = cat.is_on(item);
                                        if ui_kit::toggle(&mut c, &mut on).changed() {
                                            cat.set_item(item, on);
                                        }
                                        let g = ui_kit::truncated(
                                            &c,
                                            item,
                                            FontId::new(13.0, ui_kit::mono()),
                                            if on { p.text } else { p.faint },
                                            col_w - 56.0,
                                        );
                                        c.label(egui::WidgetText::from(g));
                                    }
                                });
                            }
                        });
                }
            }
        });
    if let Some(i) = remove {
        if open.as_deref() == Some(cats[i].id.as_str()) {
            *open = None;
        }
        cats.remove(i);
    }
    ui.add_space(12.0);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn net_form(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut NetTab,
    domains: &mut String,
    keywords: &mut String,
    apps: &mut String,
    site_cats: &mut Vec<crate::blockcat::BlockCategory>,
    app_cats: &mut Vec<crate::blockcat::BlockCategory>,
    open_cat: &mut Option<String>,
    devices: &mut vimora_core::DevicePolicy,
    extra: &mut NetExtra,
    safe: &mut bool,
    entry: &mut String,
    import: &mut bool,
) {
    let before = *tab;
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, ic, label, n) in [
                (
                    NetTab::Domains,
                    icons::GLOBE_LOCK,
                    tr("Saytlar"),
                    Some(crate::blockcat::effective(domains, site_cats).len()),
                ),
                (
                    NetTab::Keywords,
                    icons::TEXT_SEARCH,
                    tr("Kalit so'zlar"),
                    Some(crate::config::active_list(keywords).len()),
                ),
                (
                    NetTab::Apps,
                    icons::APP_WINDOW,
                    tr("Dasturlar"),
                    Some(crate::blockcat::effective(apps, app_cats).len()),
                ),
                (
                    NetTab::Devices,
                    icons::USB,
                    tr("Qurilmalar"),
                    Some(devices_blocked(devices)),
                ),
                (NetTab::Search, icons::SHIELD_CHECK, tr("Tarmoq"), None),
            ] {
                if ui_kit::tab_icon(ui, Some(ic), label, n, *tab == t).clicked() {
                    *tab = t;
                }
            }
        })
    });
    if *tab != before {
        entry.clear();
        *open_cat = None;
    }
    ui.add_space(12.0);
    match tab {
        NetTab::Domains => {
            mode_row(
                ui,
                p,
                &mut extra.web_mode,
                &[
                    (0, tr("Ro'yxatdagilar yopiq")),
                    (1, tr("Faqat ro'yxatdagilar")),
                    (2, tr("Internet o'chiq")),
                ],
            );
            match extra.web_mode {
                1 => block_list(
                    ui,
                    p,
                    &mut extra.allow_sites,
                    entry,
                    import,
                    tr("sayt: edu.uz"),
                    tr("Brauzerda faqat shu saytlar ochiladi, qolganlari yopiq. Masalan, dars yoki test sayti."),
                    &[],
                    |_| {},
                ),
                2 => {
                    ui.label(
                        RichText::new(with_icon(
                            icons::GLOBE_LOCK,
                            tr("Internet butunlay o'chiq. Sinf tarmog'i (konsol bilan aloqa) ishlayveradi."),
                        ))
                        .size(13.0)
                        .color(p.text),
                    );
                }
                _ => block_list(
                    ui,
                    p,
                    domains,
                    entry,
                    import,
                    tr("sayt: tiktok.com"),
                    tr("Toifani to'liq bloklasangiz, undagi barcha saytlar ochilmaydi; ichidan bittasini tanlasangiz — faqat o'sha sayt. Yoqilgan saytlarning subdomenlari ham ochilmaydi. Import qilingan ro'yxat (masalan, respublika ro'yxati) alohida toifa bo'ladi."),
                    &[],
                    |ui| categories(ui, p, site_cats, crate::blockcat::SITES, open_cat),
                ),
            }
        }
        NetTab::Keywords => block_list(
            ui,
            p,
            keywords,
            entry,
            import,
            tr("so'z: kazino"),
            tr("Shu so'z bilan qidirilsa, sahifa ochilmaydi (yopiladi), o'quvchiga ogohlantirish oynasi chiqadi va sizga xabar keladi."),
            &[
                (
                    tr("Qimor"),
                    &[
                        "kazino",
                        "casino",
                        "1xbet",
                        "poker",
                        "stavka",
                        "ставки",
                        "казино",
                    ],
                ),
                (
                    tr("Kattalar uchun"),
                    &["porn", "xxx", "18+", "порно", "sex"],
                ),
                (
                    tr("O'yinlar"),
                    &[
                        "minecraft",
                        "roblox",
                        "fortnite",
                        "o'yin yuklab",
                        "скачать игру",
                    ],
                ),
            ],
            |_| {},
        ),
        NetTab::Apps => {
            mode_row(
                ui,
                p,
                &mut extra.app_mode,
                &[
                    (0, tr("Ro'yxatdagilar yopiq")),
                    (1, tr("Faqat ro'yxatdagilar")),
                ],
            );
            if extra.app_mode == 1 {
                block_list(
                    ui,
                    p,
                    &mut extra.allow_apps,
                    entry,
                    import,
                    tr("dastur: winword.exe"),
                    tr("Faqat shu dasturlar ishlaydi, boshqalari ochilishi bilan yopiladi (Windows). Windows'ning o'z qismlari (Explorer, Sozlamalar) ishlayveradi."),
                    &[],
                    |_| {},
                );
            } else {
                block_list(
                    ui,
                    p,
                    apps,
                    entry,
                    import,
                    tr("dastur: steam.exe"),
                    tr("Ro'yxatdagi dasturlar ochilishi bilan yopiladi. Jarayon nomini yozing (masalan, steam.exe yoki telegram). Toifani to'liq yoki ichidan alohida dasturlarni bloklash mumkin."),
                    &[],
                    |ui| categories(ui, p, app_cats, crate::blockcat::APPS, open_cat),
                );
            }
        }
        NetTab::Devices => devices_form(ui, p, devices),
        NetTab::Search => {
            sgroup(ui, p, |ui| {
                srow(
                    ui,
                    p,
                    true,
                    tr("Xavfsiz qidiruv"),
                    Some(tr(
                        "Nomaqbul natijalar yashiriladi. O'quvchi o'chira olmaydi.",
                    )),
                    |ui| {
                        ui_kit::toggle(ui, safe);
                    },
                );
                srow(
                    ui,
                    p,
                    false,
                    tr("Yopiq portlar"),
                    Some(tr("Shu tarmoq portlari orqali chiqib bo'lmaydi. Vergul bilan: 25, 6881. Bo'sh — cheklov yo'q.")),
                    |ui| {
                        let r = ui.add(
                            ui_kit::text_field(&mut extra.ports)
                                .hint_text("25, 6881")
                                .desired_width(200.0),
                        );
                        if r.changed() {
                            extra.ports.retain(|c| c.is_ascii_digit() || c == ',' || c == ' ');
                        }
                    },
                );
            });
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                for name in ["Google", "Yandex", "Bing", "DuckDuckGo", "YouTube"] {
                    let col = if *safe { p.success } else { p.faint };
                    ui_kit::chip(ui, name, col);
                }
            });
        }
    }
}

/// "PC-01" / "PC-01, PC-02" / "PC-01, PC-02 +5" / "Hammasi (12)".
pub(super) fn recipients_label(all: &[View], targets: &[u64]) -> String {
    let names: Vec<&str> = all
        .iter()
        .filter(|v| targets.contains(&v.id))
        .map(|v| v.label.as_str())
        .collect();
    match names.len() {
        0 => tr("hech kim").into(),
        n if n == all.len() && n > 2 => tf!("Hammasi ({n})", n = n),
        1 | 2 => names.join(", "),
        n => format!("{}, {} +{}", names[0], names[1], n - 2),
    }
}

/// Telegram-style chat: bubbles (teacher right, students left) and a composer at the bottom.
pub(super) fn chat_form(
    ui: &mut egui::Ui,
    p: &Palette,
    text: &mut String,
    send: &mut bool,
    cx: &FormCtx<'_>,
) {
    const COMPOSER_H: f32 = 76.0;
    const RECIPIENTS_H: f32 = 40.0;
    let full = ui.max_rect();

    // Recipients bar
    let bar = Rect::from_min_size(full.min, vec2(full.width(), RECIPIENTS_H));
    ui.painter().hline(
        bar.x_range(),
        bar.bottom() - 0.5,
        Stroke::new(1.0_f32, p.line),
    );
    let to = tf!(
        "{v0}  Kimga: {v1}",
        v0 = icons::USERS,
        v1 = recipients_label(cx.all, cx.targets)
    );
    let g = ui_kit::truncated(
        ui,
        &to,
        FontId::proportional(13.0),
        p.muted,
        bar.width() - 40.0,
    );
    ui.painter().galley(
        pos2(bar.left() + 20.0, bar.center().y - g.size().y / 2.0),
        g,
        p.muted,
    );

    // Messages
    let list = Rect::from_min_max(
        pos2(full.left(), bar.bottom()),
        pos2(full.right(), full.bottom() - COMPOSER_H),
    );
    ui.painter().rect_filled(list, Rounding::ZERO, p.bg);
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                Frame::none()
                    .inner_margin(Margin::symmetric(16.0, 14.0))
                    .show(ui, |ui| {
                        ui.set_width(list.width() - 32.0);
                        let h = cx.hub.lock().unwrap();
                        if h.chat.is_empty() {
                            ui.add_space(list.height() * 0.32);
                            ui.vertical_centered(|ui| {
                                ui.label(ui_kit::icon(icons::MESSAGE_CIRCLE, 34.0, p.faint));
                                ui.add_space(6.0);
                                ui.label(RichText::new(tr("Hali xabar yo'q")).color(p.muted));
                                ui.label(
                                    RichText::new(tr("Pastda yozing va Enter bosing"))
                                        .size(12.0)
                                        .color(p.faint),
                                );
                            });
                            return;
                        }
                        let max_w = (list.width() - 32.0) * 0.78;
                        for m in &h.chat {
                            chat_bubble(ui, p, m, max_w);
                            ui.add_space(8.0);
                        }
                    });
            });
    });

    // Composer: input + square send button, same height, both inside the drawer.
    let comp = Rect::from_min_max(pos2(full.left(), full.bottom() - COMPOSER_H), full.max);
    ui.painter().rect_filled(comp, Rounding::ZERO, p.panel);
    ui.painter().hline(
        comp.x_range(),
        comp.top() + 0.5,
        Stroke::new(1.0_f32, p.line),
    );
    // One height for both; the input's box is painted here (not by TextEdit, whose frame and
    // focus ring came out a few pixels off the button).
    const ROW_H: f32 = 40.0;
    let inner = Rect::from_center_size(comp.center(), vec2(comp.width() - 32.0, ROW_H));
    let send_rect =
        Rect::from_min_size(pos2(inner.right() - ROW_H, inner.top()), vec2(ROW_H, ROW_H));
    let edit_rect = Rect::from_min_max(inner.min, pos2(send_rect.left() - 8.0, inner.bottom()));
    let focused = ui.memory(|m| m.has_focus(egui::Id::new("chat-input")));
    ui.painter().rect(
        edit_rect,
        Rounding::ZERO,
        p.sunken,
        Stroke::new(1.0_f32, if focused { p.accent } else { p.line_strong }),
    );
    let edit = ui
        .put(
            edit_rect.shrink2(vec2(10.0, 1.0)),
            egui::TextEdit::multiline(text)
                .id(egui::Id::new("chat-input"))
                .frame(false)
                .margin(vec2(0.0, 0.0))
                .hint_text(tr("Xabar yozing…"))
                .desired_rows(1)
                .vertical_align(egui::Align::Center)
                .desired_width(edit_rect.width() - 20.0)
                .min_size(edit_rect.shrink2(vec2(10.0, 1.0)).size())
                .return_key(Some(egui::KeyboardShortcut::new(
                    egui::Modifiers::SHIFT,
                    Key::Enter,
                ))),
        )
        .on_hover_text(tr("Enter — yuborish · Shift+Enter — yangi qator"));
    if ui.memory(|m| m.focused().is_none()) {
        edit.request_focus();
    }
    let enter = edit.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
    let resp = ui.interact(send_rect, egui::Id::new("chat-send"), Sense::click());
    let ready = !text.trim().is_empty();
    let fill = if !ready {
        p.hover
    } else if resp.hovered() {
        p.accent_ink
    } else {
        p.accent
    };
    ui.painter().rect_filled(send_rect, Rounding::ZERO, fill);
    let ink = if ready { Color32::WHITE } else { p.faint };
    ui.painter().text(
        send_rect.center(),
        Align2::CENTER_CENTER,
        icons::SEND,
        FontId::proportional(18.0),
        ink,
    );
    if (resp.on_hover_text(tr("Yuborish")).clicked() || enter) && ready {
        *send = true;
    }
}

/// One chat bubble: teacher's on the right (accent), students' on the left with their name.
fn chat_bubble(ui: &mut egui::Ui, p: &Palette, m: &crate::net::ChatMsg, max_w: f32) {
    if m.system {
        ui.vertical_centered(|ui| {
            ui_kit::hud(ui, &format!("{} · {}", m.text, m.time), p.faint);
        });
        return;
    }
    let side = if m.outgoing {
        Layout::top_down(Align::Max)
    } else {
        Layout::top_down(Align::Min)
    };
    ui.with_layout(side, |ui| {
        let (fill, ink, meta_col) = if m.outgoing {
            (p.accent, Color32::WHITE, Color32::from_white_alpha(200))
        } else {
            (p.card_hover, p.text, p.faint)
        };
        Frame::none()
            .fill(fill)
            .inner_margin(Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_max_width(max_w);
                // Text inside a bubble always reads left to right.
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    if !m.outgoing {
                        ui.label(
                            RichText::new(&m.who)
                                .size(12.0)
                                .family(ui_kit::bold())
                                .color(p.accent_ink),
                        );
                    }
                    ui.label(RichText::new(&m.text).size(14.0).color(ink));
                    let meta = if m.outgoing {
                        // Telegram-style: clock = not on their screen yet, ✓ = on every
                        // recipient's screen, ✓✓ = every recipient has seen it.
                        let all = |set: &std::collections::BTreeSet<u64>| {
                            !m.to.is_empty() && m.to.iter().all(|id| set.contains(id))
                        };
                        let tick = if all(&m.read) {
                            icons::CHECK_CHECK
                        } else if all(&m.delivered) {
                            icons::CHECK
                        } else if m.to.is_empty() {
                            ""
                        } else {
                            icons::TIMER
                        };
                        format!("{} · {}  {tick}", m.time, m.who)
                    } else {
                        m.time.clone()
                    };
                    ui.label(RichText::new(meta).size(10.5).color(meta_col));
                });
            });
    });
}

/// How many kinds of device are restricted (the tab's counter).
fn devices_blocked(d: &vimora_core::DevicePolicy) -> usize {
    (d.usb != vimora_core::UsbMode::Allow) as usize
        + [
            d.cd,
            d.printer,
            d.camera,
            d.microphone,
            d.bluetooth,
            d.sound_off,
        ]
        .iter()
        .filter(|b| **b)
        .count()
}

/// "Qurilmalar": one row per device; a switch that is on means "ruxsat".
fn devices_form(ui: &mut egui::Ui, p: &Palette, d: &mut vimora_core::DevicePolicy) {
    use vimora_core::UsbMode;
    sgroup(ui, p, |ui| {
        srow(
            ui,
            p,
            true,
            tr("USB fleshka"),
            Some(tr("\"Faqat o'qish\" — fayllarni ochish mumkin, yozish va nusxa ko'chirish mumkin emas. Yangi sozlama fleshka qayta ulanganda kuchga kiradi.")),
            |ui| {
                ui_kit::segmented(p).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // Right-to-left row: last first, so they read in order.
                    for (m, label) in [
                        (UsbMode::Block, tr("Taqiqlangan")),
                        (UsbMode::ReadOnly, tr("Faqat o'qish")),
                        (UsbMode::Allow, tr("Ruxsat")),
                    ] {
                        if ui_kit::tab(ui, label, None, d.usb == m).clicked() {
                            d.usb = m;
                        }
                    }
                });
            },
        );
        for (blocked, title, help) in [
            (&mut d.cd, "CD/DVD", None),
            (
                &mut d.printer,
                tr("Printer"),
                Some(tr("O'chirilsa, chop etib bo'lmaydi.")),
            ),
            (&mut d.camera, tr("Kamera"), None),
            (&mut d.microphone, tr("Mikrofon"), None),
            (&mut d.bluetooth, "Bluetooth", None),
            (
                &mut d.sound_off,
                tr("Ovoz"),
                Some(tr(
                    "O'chirilsa, kompyuter ovozi o'chiriladi va o'quvchi qayta yoqa olmaydi.",
                )),
            ),
        ] {
            srow(ui, p, false, title, help, |ui| {
                // On = allowed.
                let mut allowed = !*blocked;
                if ui_kit::toggle(ui, &mut allowed).changed() {
                    *blocked = !allowed;
                }
            });
        }
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr("Yoqilgan — ruxsat, o'chirilgan — taqiqlangan. \"Qo'llash\" bosilganda kuchga kiradi va konsol yopiq bo'lsa ham saqlanadi."))
            .size(12.0)
            .color(p.faint),
    );
}

/// One segmented choice above a tab's content (what the list below means).
fn mode_row(ui: &mut egui::Ui, p: &Palette, value: &mut u8, items: &[(u8, &str)]) {
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (v, label) in items {
                if ui_kit::tab(ui, label, None, *value == *v).clicked() {
                    *value = *v;
                }
            }
        });
    });
    ui.add_space(10.0);
}

/// "25, 6881" → ports.
pub(super) fn parse_ports(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s
        .split([',', ' ', ';'])
        .filter_map(|t| t.trim().parse().ok())
        .filter(|p| *p > 0)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// "Skanerlash": computers that belong to no console (a new install, or removed from its
/// console). They show in every console that scans; "Qo'shish" joins one to this console.
fn free_pcs_form(
    ui: &mut egui::Ui,
    p: &Palette,
    add: &mut Vec<String>,
    since: f32,
    rescan: &mut bool,
    cx: &FormCtx<'_>,
) {
    /// The first seconds of a scan: answers are still coming.
    const SCANNING: f32 = 6.0;
    let mut free = cx.hub.lock().unwrap().free_now();
    free.sort_by_key(|f| f.1.label().to_lowercase());
    ui.label(
        RichText::new(tr("Tarmoqdagi hali hech bir konsolga qo'shilmagan kompyuterlar. \"Qo'shish\" ni bosing — kompyuter shu konsolga ulanadi."))
            .size(12.5)
            .color(p.muted),
    );
    ui.add_space(12.0);
    let scanning = since < SCANNING;
    ui.horizontal(|ui| {
        if scanning {
            ui.add(egui::Spinner::new().size(16.0).color(p.accent));
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr("Skanerlanmoqda…"))
                    .size(13.0)
                    .color(p.text),
            );
        } else {
            ui.label(
                RichText::new(tf!("{n} ta kompyuter topildi", n = free.len()))
                    .size(13.0)
                    .color(p.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui_kit::button(
                    ui,
                    Some(icons::REFRESH_CW),
                    tr("Qayta skanerlash"),
                    Kind::Outline,
                )
                .clicked()
                {
                    *rescan = true;
                }
            });
        }
    });
    ui.add_space(12.0);
    if free.is_empty() {
        if !scanning {
            ui.label(
                RichText::new(tr("Yangi kompyuter topilmadi. O'quvchi kompyuterida Vimora o'rnatilgani va u shu tarmoqqa ulangani (Wi-Fi yoki LAN) tekshiring."))
                    .size(13.0)
                    .color(p.faint),
            );
        }
        return;
    }
    let w = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            for (i, (key, info, ip)) in free.iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
                if i > 0 {
                    ui.painter()
                        .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                }
                ui.painter().text(
                    pos2(r.left() + 22.0, r.center().y),
                    Align2::CENTER_CENTER,
                    icons::MONITOR,
                    FontId::proportional(18.0),
                    p.accent,
                );
                ui.painter().text(
                    pos2(r.left() + 44.0, r.center().y - 9.0),
                    Align2::LEFT_CENTER,
                    info.label(),
                    FontId::new(14.0, ui_kit::bold()),
                    p.strong,
                );
                ui.painter().text(
                    pos2(r.left() + 44.0, r.center().y + 10.0),
                    Align2::LEFT_CENTER,
                    format!("{} · {}", info.username, ip),
                    FontId::new(12.0, ui_kit::mono()),
                    p.muted,
                );
                let mut b = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_max(
                            pos2(r.right() - 150.0, r.top()),
                            r.max - vec2(10.0, 0.0),
                        ))
                        .layout(Layout::right_to_left(Align::Center)),
                );
                if ui_kit::button(&mut b, Some(icons::PLUS), tr("Qo'shish"), Kind::Primary)
                    .clicked()
                {
                    add.push(key.clone());
                }
            }
        });
    ui.add_space(12.0);
    if free.len() > 1
        && ui_kit::button(
            ui,
            Some(icons::PLUS),
            &tf!("Hammasini qo'shish ({n})", n = free.len()),
            Kind::Outline,
        )
        .clicked()
    {
        add.extend(free.iter().map(|f| f.0.clone()));
    }
}
