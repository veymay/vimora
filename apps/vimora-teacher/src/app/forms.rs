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
    /// Automatic updates: per computer (key) and for everyone else.
    pub(super) policies: &'a (
        std::collections::BTreeMap<String, UpdatePolicy>,
        UpdatePolicy,
    ),
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
            ..
        } => rename_form(ui, p, alias, system, system_name, key),
        Dialog::Updates { tab, open, request } => updates_form(ui, p, tab, open, request, cx),
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
            terminal,
            shell,
            code,
            run,
            url,
            program,
            args,
            bookmarks,
            recent,
        } => {
            // The tabs span the whole width.
            let cur = if *terminal {
                2
            } else if *web {
                0
            } else {
                1
            };
            let items = [
                ui_kit::TabItem {
                    icon: Some(icons::GLOBE),
                    label: tr("Sayt"),
                    count: None,
                },
                ui_kit::TabItem {
                    icon: Some(icons::PLAY),
                    label: tr("Dastur"),
                    count: None,
                },
                ui_kit::TabItem {
                    icon: Some(icons::TERMINAL),
                    label: tr("Terminal"),
                    count: None,
                },
            ];
            if let Some(i) = ui_kit::tab_strip(ui, &items, cur) {
                *web = i == 0;
                *terminal = i == 2;
            }
            ui.add_space(10.0);
            if *terminal {
                terminal_tab(ui, p, shell, code, run, cx);
            } else if *web {
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
                RichText::new(tr(
                    "O'quvchi kompyuterida Vimora papkasidagi Files ichiga saqlanadi.",
                ))
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
            word_cats,
            open_cat,
            devices,
            extra,
            safe,
            entry,
            import,
        } => net_form(
            ui, p, tab, domains, keywords, apps, site_cats, app_cats, word_cats, open_cat, devices,
            extra, safe, entry, import,
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
                    let r = power_tile(ui, p, ic, label, *a == act, 4.0, col);
                    if r.clicked() {
                        *a = act;
                    }
                    // No confirm button: a double click is the confirmation.
                    if r.double_clicked() {
                        *a = act;
                        *confirm = true;
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

/// The "Terminal" tab: a shell, a command, and the answer of every chosen computer.
fn terminal_tab(
    ui: &mut egui::Ui,
    p: &Palette,
    shell: &mut u8,
    code: &mut String,
    run: &mut bool,
    cx: &FormCtx<'_>,
) {
    if let Some(i) = ui_kit::choices(
        ui,
        &[(None, "PowerShell"), (None, "CMD"), (None, "Bash")],
        *shell as usize,
    ) {
        *shell = i as u8;
    }
    let chosen: Vec<&View> = cx
        .all
        .iter()
        .filter(|v| cx.targets.contains(&v.id))
        .collect();
    let fits = |v: &&&View| v.os.to_lowercase().contains("windows") == (*shell != 2);
    let n = chosen.iter().filter(fits).count();
    ui.add_space(6.0);
    ui.label(
        RichText::new(if *shell == 2 {
            tf!("Ubuntu kompyuterlarida bajariladi: {n} ta", n = n)
        } else {
            tf!("Windows kompyuterlarida bajariladi: {n} ta", n = n)
        })
        .size(12.5)
        .color(p.muted),
    );
    ui.label(
        RichText::new(tr(
            "Administrator huquqida. Har bir buyruq faollik jurnaliga yoziladi.",
        ))
        .size(12.0)
        .color(p.faint),
    );
    ui.add_space(8.0);
    ui.add(
        ui_kit::text_area(code)
            .hint_text(if *shell == 1 {
                "ipconfig /all"
            } else if *shell == 2 {
                "uname -a"
            } else {
                "Get-Process"
            })
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace),
    );
    ui.add_space(8.0);
    ui.add_enabled_ui(n > 0 && !code.trim().is_empty(), |ui| {
        if ui_kit::button(ui, Some(icons::PLAY), tr("Bajarish"), Kind::Primary).clicked() {
            *run = true;
        }
    });
    // The answers, one frame per computer.
    let answers: Vec<(String, (bool, String))> = {
        let h = cx.hub.lock().unwrap();
        chosen
            .iter()
            .filter(fits)
            .filter_map(|v| {
                h.students
                    .get(&v.id)
                    .and_then(|s| s.last_result.clone())
                    .map(|r| (v.label.clone(), r))
            })
            .collect()
    };
    for (name, (ok, text)) in answers {
        ui.add_space(10.0);
        ui_kit::hud(ui, &name, if ok { p.muted } else { p.danger });
        ui.add_space(4.0);
        Frame::none()
            .fill(p.sunken)
            .stroke(Stroke::new(1.0_f32, p.line))
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(text)
                        .family(ui_kit::mono())
                        .size(12.0)
                        .color(if ok { p.text } else { p.danger }),
                );
            });
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

/// The "Sayt" tab: one box (Enter opens), and one list of sites: saved ones first (marked),
/// then the recent ones. A click opens a site on the students' screens.
fn launch_site(
    ui: &mut egui::Ui,
    p: &Palette,
    url: &mut String,
    bookmarks: &mut Vec<String>,
    recent: &mut Vec<String>,
    confirm: &mut bool,
) {
    let r = ui.add(
        ui_kit::text_field(url)
            .hint_text(tr("Sayt nomi yoki qidiruv so'zi, keyin Enter"))
            .desired_width(f32::INFINITY),
    );
    if ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    // Enter opens the site (the field gives up focus on Enter, so look at both states).
    if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
        *confirm = true;
    }
    ui.add_space(12.0);

    // Saved sites first, then the recent ones that are not saved.
    let rows: Vec<(String, bool)> = bookmarks
        .iter()
        .map(|b| (b.clone(), true))
        .chain(
            recent
                .iter()
                .filter(|r| !bookmarks.iter().any(|b| site_label(b) == site_label(r)))
                .map(|r| (r.clone(), false)),
        )
        .collect();
    if rows.is_empty() {
        ui.label(
            RichText::new(tr("Ochilgan saytlar shu yerda ko'rinadi."))
                .size(12.5)
                .color(p.faint),
        );
        return;
    }
    let mut toggle: Option<usize> = None;
    let mut remove: Option<usize> = None;
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, (site, saved)) in rows.iter().enumerate() {
                let (open, act) = site_row(
                    ui,
                    p,
                    i == 0,
                    &site_label(site),
                    &[
                        (
                            if *saved {
                                icons::BOOKMARK_CHECK
                            } else {
                                icons::BOOKMARK_PLUS
                            },
                            p.accent_ink,
                            if *saved {
                                tr("Saralangandan olib tashlash")
                            } else {
                                tr("Saralanganlarga")
                            },
                        ),
                        (icons::X, p.danger, tr("Olib tashlash")),
                    ],
                );
                match act {
                    Some(0) => toggle = Some(i),
                    Some(_) => remove = Some(i),
                    None if open => {
                        *url = site.clone();
                        *confirm = true;
                    }
                    None => {}
                }
            }
        });
    if let Some(i) = toggle {
        let (site, saved) = &rows[i];
        let l = site_label(site);
        if *saved {
            bookmarks.retain(|b| site_label(b) != l);
            if !recent.iter().any(|r| site_label(r) == l) {
                recent.insert(0, site.clone());
            }
        } else {
            bookmarks.push(site.clone());
        }
    }
    if let Some(i) = remove {
        let l = site_label(&rows[i].0);
        bookmarks.retain(|b| site_label(b) != l);
        recent.retain(|r| site_label(r) != l);
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
    pub(super) folders: &'a mut String,
    pub(super) request: &'a mut Option<ShadowRequest>,
    pub(super) manual: &'a mut String,
    pub(super) password: &'a mut String,
}

pub(super) fn shadow_form(ui: &mut egui::Ui, p: &Palette, mut f: ShadowForm<'_>, cx: &FormCtx<'_>) {
    // Two tabs only: each filter has its computers, kept files and settings in one page.
    if matches!(*f.tab, ShadowTab::Commit | ShadowTab::Settings) {
        *f.tab = ShadowTab::Uwf;
    }
    // Shadow Defender (1) or UWF (2). A computer counts here when it uses this filter, or is
    // not tied to one (Ubuntu, older agents).
    let engine: u8 = if *f.tab == ShadowTab::Uwf { 2 } else { 1 };
    let mine = |v: &View| {
        let e = engine_of(v);
        e == 0 || e == engine
    };

    let tabs = [
        (ShadowTab::Uwf, icons::SHIELD, "UWF"),
        (ShadowTab::Protection, icons::SNOWFLAKE, "Shadow Defender"),
    ];
    let items: Vec<ui_kit::TabItem> = tabs
        .iter()
        .map(|(_, ic, label)| ui_kit::TabItem {
            icon: Some(*ic),
            label,
            count: None,
        })
        .collect();
    let current = tabs.iter().position(|x| x.0 == *f.tab).unwrap_or(0);
    if let Some(i) = ui_kit::tab_strip(ui, &items, current) {
        *f.tab = tabs[i].0;
    }
    ui.add_space(12.0);

    // The computers picked in the grid (all of them when none is picked): each has its own
    // switch, so the filter goes on or off for exactly that one.
    let views: Vec<&View> = if f.chosen.is_empty() {
        cx.all.iter().collect()
    } else {
        cx.all.iter().filter(|v| f.chosen.contains(&v.id)).collect()
    };
    if views.is_empty() {
        ui.label(
            RichText::new(tr("Kompyuter yo'q."))
                .size(12.5)
                .color(p.faint),
        );
    } else {
        let now = ui.input(|i| i.time);
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, v) in views.iter().enumerate() {
                    let rw = ui.available_width();
                    let (rect, _) = ui.allocate_exact_size(vec2(rw, 38.0), Sense::hover());
                    if i > 0 {
                        ui.painter().hline(
                            rect.x_range(),
                            rect.top(),
                            Stroke::new(1.0_f32, p.line),
                        );
                    }
                    let frozen = matches!(
                        v.status.freeze,
                        FreezeState::Frozen | FreezeState::FreezePending | FreezeState::Maintenance
                    );
                    // A switch flipped a moment ago shows where it is going until the computer
                    // reports back.
                    let wait_id = egui::Id::new(("freeze-wait", v.id));
                    let mut waiting: Option<(bool, f64)> = ui.data(|d| d.get_temp(wait_id));
                    if let Some((target, since)) = waiting {
                        if (mine(v) && frozen == target) || now - since > 20.0 {
                            waiting = None;
                            ui.data_mut(|d| d.remove::<(bool, f64)>(wait_id));
                        } else {
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(250));
                        }
                    }
                    let usable = v.online
                        && v.status.service
                        && mine(v)
                        && (engine == 2
                            || !matches!(
                                v.status.freeze,
                                FreezeState::Unsupported | FreezeState::Unknown
                            ));
                    let (status, col) = if !v.online {
                        (tr("Oflayn").to_string(), p.faint)
                    } else if !v.status.service {
                        (tr("Xizmat yo'q").to_string(), p.danger)
                    } else if !mine(v) {
                        (
                            if engine == 2 {
                                "Shadow Defender"
                            } else {
                                "UWF"
                            }
                            .to_string(),
                            p.faint,
                        )
                    } else if v.status.freeze == FreezeState::Unsupported && engine == 1 {
                        (tr("Mavjud emas").to_string(), p.faint)
                    } else if waiting.is_some() {
                        (tr("Kutilmoqda…").to_string(), p.warning)
                    } else {
                        let mut t = freeze_label(v.status.freeze).to_string();
                        if let Some(u) = v.status.shield_usage {
                            t.push_str(&format!(" · {}%", u.percent()));
                        }
                        (t, freeze_color(p, v.status.freeze))
                    };
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    child.spacing_mut().item_spacing.x = 10.0;
                    let mut on = waiting.map_or(mine(v) && frozen, |(t, _)| t);
                    // Frozen: a snowflake; not frozen: a plain computer.
                    let is_frozen = mine(v) && frozen;
                    child.label(ui_kit::icon(
                        if is_frozen {
                            icons::SNOWFLAKE
                        } else {
                            icons::MONITOR
                        },
                        16.0,
                        if is_frozen {
                            p.accent
                        } else if usable {
                            p.muted
                        } else {
                            p.faint
                        },
                    ));
                    let g = ui_kit::truncated(
                        &child,
                        &v.label,
                        FontId::new(13.0, ui_kit::mono()),
                        if usable { p.text } else { p.faint },
                        rw - 300.0,
                    );
                    child.label(egui::WidgetText::from(g));
                    // Right end: the switch, the state just before it.
                    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.add_enabled_ui(usable && waiting.is_none(), |ui| {
                            let was = on;
                            if ui_kit::toggle(ui, &mut on).changed() && on != was {
                                *f.request = Some(ShadowRequest::Set { id: v.id, on });
                                ui.data_mut(|d| d.insert_temp(wait_id, (on, now)));
                            }
                        });
                        ui_kit::chip(ui, &status.to_uppercase(), col);
                    });
                }
            });
    }

    // Files and folders that always survive restarts.
    freeze_section(
        ui,
        p,
        tr("Doim saqlanadigan fayllar"),
        tr("Ro'yxatdagi fayl va papkalar qayta yuklangandan keyin ham qoladi."),
    );
    commit_section(ui, p, &mut f, cx);

    // Settings of this filter (Shadow Defender only: UWF has none).
    if engine == 1 {
        freeze_section(
            ui,
            p,
            tr("Shadow Defender paroli"),
            tr("Parol yo'q bo'lsa, bo'sh qoldiring."),
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

/// Files and folders that always survive restarts (a list like the block lists: add, remove;
/// every change goes to the computers at once) and one button that commits them now.
fn commit_section(ui: &mut egui::Ui, p: &Palette, f: &mut ShadowForm<'_>, cx: &FormCtx<'_>) {
    let usable = cx
        .all
        .iter()
        .any(|v| f.chosen.contains(&v.id) && v.status.service);
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
    key: &str,
) {
    section(ui, p, tr("Dasturdagi nomi"));
    ui.add(
        ui_kit::text_field(alias)
            .hint_text(key)
            .desired_width(f32::INFINITY),
    );
    note(ui, p, tr("Faqat shu dasturda ko'rinadi, darhol o'zgaradi. Bo'sh qoldirsangiz — tizim nomi ko'rsatiladi."));
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
    // The control in a strip only as tall as a choice: frames around it (a segmented choice)
    // don't stretch to the row's height.
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_center_size(
                rect.center(),
                vec2(rect.width() - 24.0, ui_kit::CHOICE_H + 2.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    control(&mut c);
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

/// The Updates drawer: three tabs — programs, Windows and drivers, Vimora — each a list of the
/// computers, one accordion each. Closed: name and what is going on. Open: that computer's own
/// automatic-update switch and its items, every one with the installed and the new version and
/// a button that installs it at once.
pub(super) fn updates_form(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut UpdateTab,
    open: &mut Option<u64>,
    request: &mut Option<UpdateRequest>,
    cx: &FormCtx<'_>,
) {
    let tabs = [
        (UpdateTab::Apps, icons::PACKAGE, tr("Dasturlar")),
        (
            UpdateTab::System,
            icons::MONITOR,
            tr("Windows va drayverlar"),
        ),
        (UpdateTab::Vimora, icons::CLOUD_DOWNLOAD, "Vimora"),
    ];
    let items: Vec<ui_kit::TabItem> = tabs
        .iter()
        .map(|(_, ic, label)| ui_kit::TabItem {
            icon: Some(*ic),
            label,
            count: None,
        })
        .collect();
    let current = tabs.iter().position(|x| x.0 == *tab).unwrap_or(0);
    if let Some(i) = ui_kit::tab_strip(ui, &items, current) {
        *tab = tabs[i].0;
    }
    ui.add_space(14.0);

    let views: Vec<&View> = cx
        .all
        .iter()
        .filter(|v| v.online && cx.targets.contains(&v.id))
        .collect();
    let ids: Vec<u64> = views.iter().map(|v| v.id).collect();
    let behind: Vec<u64> = views
        .iter()
        .filter(|v| v.version.is_empty() || vimora_update::is_newer(cx.latest, &v.version))
        .map(|v| v.id)
        .collect();

    // Actions for every computer of the tab.
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!ids.is_empty(), |ui| match *tab {
            UpdateTab::Apps => {
                if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline)
                    .clicked()
                {
                    *request = Some(UpdateRequest::ListApps(ids.clone()));
                }
            }
            UpdateTab::System => {
                if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline)
                    .clicked()
                {
                    *request = Some(UpdateRequest::Check(ids.clone()));
                }
            }
            UpdateTab::Vimora => {
                ui.add_enabled_ui(!behind.is_empty(), |ui| {
                    if ui_kit::button(
                        ui,
                        Some(icons::CLOUD_DOWNLOAD),
                        &tf!("Vimora'ni yangilash ({n})", n = behind.len()),
                        Kind::Primary,
                    )
                    .clicked()
                    {
                        *request = Some(UpdateRequest::Vimora(behind.clone()));
                    }
                });
            }
        });
        if *tab == UpdateTab::Vimora {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(tf!("Eng so'nggi: {v}", v = cx.latest))
                        .size(12.5)
                        .color(p.muted),
                );
            });
        }
    });
    ui.add_space(12.0);
    if views.is_empty() {
        ui.label(
            RichText::new(tr("Onlayn kompyuter yo'q"))
                .size(12.5)
                .color(p.faint),
        );
        return;
    }

    let w = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            for (i, v) in views.iter().enumerate() {
                let (apps, info, reboot, prev) = {
                    let h = cx.hub.lock().unwrap();
                    let s = h.students.get(&v.id);
                    (
                        s.and_then(|s| s.apps.clone()),
                        s.and_then(|s| s.updates.clone()),
                        s.is_some_and(|s| s.reboot_needed),
                        s.map(|s| s.prev_version.clone()).unwrap_or_default(),
                    )
                };
                let policy = cx.policies.0.get(&v.key).copied().unwrap_or(cx.policies.1);
                let busy = v.status.busy.as_deref();
                let vimora = vimora_state(v, cx);
                let app_updates: Vec<&vimora_core::AppInfo> = apps
                    .iter()
                    .flatten()
                    .filter(|a| a.available.is_some())
                    .collect();
                let pending_text;
                // --- header: what is going on, in the tab's terms -------------------------
                let summary = if let Some(b) = busy {
                    match percent_of(b) {
                        Some(pct) => ui_kit::Progress::Percent(pct),
                        None => ui_kit::Progress::Waiting(b),
                    }
                } else {
                    match *tab {
                        UpdateTab::Apps => match &apps {
                            None => ui_kit::Progress::Idle(tr("tekshirilmagan"), p.faint),
                            Some(_) if app_updates.is_empty() => {
                                ui_kit::Progress::Done(tr("eng so'nggi"))
                            }
                            Some(_) => {
                                pending_text = tf!("{n} ta yangilanish", n = app_updates.len());
                                ui_kit::Progress::Idle(&pending_text, p.warning)
                            }
                        },
                        UpdateTab::System => match &info {
                            _ if reboot => {
                                ui_kit::Progress::Idle(tr("qayta yuklash kerak"), p.warning)
                            }
                            None => ui_kit::Progress::Idle(tr("tekshirilmagan"), p.faint),
                            Some(u) if u.items.is_empty() => {
                                ui_kit::Progress::Done(tr("eng so'nggi"))
                            }
                            Some(u) => {
                                pending_text = tf!("{n} ta yangilanish", n = u.items.len());
                                ui_kit::Progress::Idle(&pending_text, p.warning)
                            }
                        },
                        UpdateTab::Vimora => match vimora.as_ref() {
                            Some(s) => s.as_progress(),
                            None => ui_kit::Progress::Done(tr("eng so'nggi")),
                        },
                    }
                };
                let sub = match *tab {
                    UpdateTab::Vimora => {
                        if v.version.is_empty() {
                            "—".to_string()
                        } else {
                            v.version.clone()
                        }
                    }
                    UpdateTab::System => {
                        info.as_ref().map(|u| u.os_name.clone()).unwrap_or_default()
                    }
                    UpdateTab::Apps => apps
                        .as_ref()
                        .map(|a| tf!("{n} ta dastur", n = a.len()))
                        .unwrap_or_default(),
                };
                let is_open = *open == Some(v.id);
                // The one thing to do on this computer, as a button in the title.
                let (action_label, action_req): (Option<&str>, Option<UpdateRequest>) = if busy
                    .is_some()
                {
                    (None, None)
                } else {
                    match *tab {
                        UpdateTab::Apps if !app_updates.is_empty() => (
                            Some(tr("Hammasini yangilash")),
                            Some(UpdateRequest::UpgradeApps {
                                id: v.id,
                                ids: app_updates.iter().map(|a| a.id.clone()).collect(),
                            }),
                        ),
                        UpdateTab::System
                            if !reboot && info.as_ref().is_some_and(|u| !u.items.is_empty()) =>
                        {
                            (
                                Some(tr("Hammasini o'rnatish")),
                                Some(UpdateRequest::InstallIds {
                                    id: v.id,
                                    ids: info
                                        .iter()
                                        .flat_map(|u| u.items.iter().map(|i| i.id.clone()))
                                        .collect(),
                                }),
                            )
                        }
                        UpdateTab::Vimora if behind.contains(&v.id) => (
                            Some(tr("Yangilash")),
                            Some(UpdateRequest::Vimora(vec![v.id])),
                        ),
                        _ => (None, None),
                    }
                };
                let head = AccHead {
                    first: i == 0,
                    open: is_open,
                    name: &v.label,
                    sub: &sub,
                    state: summary,
                    action: action_label,
                };
                let out = acc_header(ui, p, head);
                if out.action {
                    *request = action_req.clone();
                }
                if out.clicked {
                    *open = if is_open { None } else { Some(v.id) };
                }
                ui_kit::accordion_body(
                    ui,
                    egui::Id::new(("upd-acc", v.id, *tab as u8)),
                    is_open,
                    |ui| {
                        // --- body ------------------------------------------------------------------
                        let auto = match *tab {
                            UpdateTab::Apps => policy.apps_auto,
                            UpdateTab::System => policy.os == OsUpdateMode::Auto,
                            UpdateTab::Vimora => policy.vimora,
                        };
                        acc_row(
                            ui,
                            p,
                            icons::REFRESH_CW,
                            p.muted,
                            tr("Avtomatik yangilanish"),
                            None,
                            |ui| {
                                if *tab == UpdateTab::System && reboot {
                                    // Installed updates wait for a restart: that comes first.
                                    if ui_kit::button(
                                        ui,
                                        Some(icons::ROTATE_CCW),
                                        tr("Qayta yuklash"),
                                        Kind::Primary,
                                    )
                                    .clicked()
                                    {
                                        *request = Some(UpdateRequest::Restart(v.id));
                                    }
                                } else {
                                    let mut on = auto;
                                    if ui_kit::toggle(ui, &mut on).changed() {
                                        *request = Some(UpdateRequest::Auto {
                                            id: v.id,
                                            kind: *tab,
                                            on,
                                        });
                                    }
                                }
                            },
                        );
                        match *tab {
                            UpdateTab::Apps => match &apps {
                                None => {
                                    ask_once(ui, ("upd-apps", v.id), || {
                                        *request = Some(UpdateRequest::ListApps(vec![v.id]))
                                    });
                                    acc_waiting(ui, p);
                                }
                                Some(list) => {
                                    // Those with an update first.
                                    let mut list: Vec<&vimora_core::AppInfo> =
                                        list.iter().collect();
                                    list.sort_by_key(|a| a.available.is_none());
                                    for a in list {
                                        let (sub, col) = match &a.available {
                                            Some(n) => {
                                                (format!("{}  →  {n}", a.version), p.accent_ink)
                                            }
                                            None => (a.version.clone(), p.faint),
                                        };
                                        acc_row(
                                            ui,
                                            p,
                                            icons::PACKAGE,
                                            if a.available.is_some() {
                                                p.accent
                                            } else {
                                                p.faint
                                            },
                                            &a.name,
                                            Some((&sub, col)),
                                            |ui| {
                                                if a.available.is_some()
                                                    && ui_kit::button(
                                                        ui,
                                                        Some(icons::DOWNLOAD),
                                                        tr("Yangilash"),
                                                        Kind::Outline,
                                                    )
                                                    .clicked()
                                                {
                                                    *request = Some(UpdateRequest::UpgradeApps {
                                                        id: v.id,
                                                        ids: vec![a.id.clone()],
                                                    });
                                                }
                                            },
                                        );
                                    }
                                }
                            },
                            UpdateTab::System => match &info {
                                None => {
                                    ask_once(ui, ("upd-sys", v.id), || {
                                        *request = Some(UpdateRequest::Check(vec![v.id]))
                                    });
                                    acc_waiting(ui, p);
                                }
                                Some(u) => {
                                    let build = tf!("Versiya {b}", b = u.os_build.clone());
                                    acc_row(
                                        ui,
                                        p,
                                        icons::MONITOR,
                                        p.accent,
                                        &u.os_name,
                                        Some((&build, p.muted)),
                                        |_| {},
                                    );
                                    if u.items.is_empty() {
                                        acc_row(
                                            ui,
                                            p,
                                            icons::CIRCLE_CHECK,
                                            p.success,
                                            tr("Hamma yangilanishlar o'rnatilgan"),
                                            None,
                                            |_| {},
                                        );
                                    }
                                    // The system's own updates first, then the drivers.
                                    let mut items: Vec<&vimora_core::UpdateItem> =
                                        u.items.iter().collect();
                                    items.sort_by_key(|i| i.driver);
                                    for it in items {
                                        let mut sub =
                                            match (it.current.is_empty(), it.new.is_empty()) {
                                                (false, false) => {
                                                    format!("{}  →  {}", it.current, it.new)
                                                }
                                                (true, false) => format!("→  {}", it.new),
                                                (false, true) => it.current.clone(),
                                                (true, true) => String::new(),
                                            };
                                        if it.size_mb > 0 {
                                            sub.push_str(&format!("  ·  {} MB", it.size_mb));
                                        }
                                        acc_row(
                                            ui,
                                            p,
                                            if it.driver {
                                                icons::CPU
                                            } else {
                                                icons::CIRCLE_ARROW_UP
                                            },
                                            p.accent,
                                            &it.title,
                                            Some((&sub, p.accent_ink)),
                                            |ui| {
                                                if ui_kit::button(
                                                    ui,
                                                    Some(icons::DOWNLOAD),
                                                    tr("O'rnatish"),
                                                    Kind::Outline,
                                                )
                                                .clicked()
                                                {
                                                    *request = Some(UpdateRequest::InstallIds {
                                                        id: v.id,
                                                        ids: vec![it.id.clone()],
                                                    });
                                                }
                                            },
                                        );
                                    }
                                }
                            },
                            UpdateTab::Vimora => {
                                let now = tf!(
                                    "O'rnatilgan {v}  ·  eng so'nggi {l}",
                                    v = if v.version.is_empty() {
                                        "—".to_string()
                                    } else {
                                        v.version.clone()
                                    },
                                    l = cx.latest.to_string()
                                );
                                acc_row(
                                    ui,
                                    p,
                                    icons::CLOUD_DOWNLOAD,
                                    p.accent,
                                    "Vimora",
                                    Some((&now, p.muted)),
                                    |_| {},
                                );
                                let before = if prev.is_empty() {
                                    tr("ma'lum emas").to_string()
                                } else {
                                    format!("{prev}  →  {}", v.version)
                                };
                                acc_row(
                                    ui,
                                    p,
                                    icons::HISTORY,
                                    p.muted,
                                    tr("Oldingi versiya"),
                                    Some((&before, p.muted)),
                                    |_| {},
                                );
                            }
                        }
                        acc_rail_finish(ui, p);
                    },
                );
                // The list is long and scrolled: the computer's title stays at the top, so it
                // is clear whose rows these are.
                if is_open {
                    let clip = ui.clip_rect();
                    let body_end = ui.cursor().top();
                    if out.rect.top() < clip.top() - 0.5 && body_end > clip.top() + 12.0 {
                        let y = clip.top().min(body_end - ACC_H);
                        let r = Rect::from_min_size(
                            pos2(out.rect.left(), y),
                            vec2(out.rect.width(), ACC_H),
                        );
                        let pin =
                            acc_header_pinned(ui, p, r, egui::Id::new(("upd-pin", v.id)), head);
                        if pin.action {
                            *request = action_req.clone();
                        }
                        if pin.clicked {
                            *open = None;
                        }
                    }
                }
            }
        });
}

/// Asks once per opening (a flag in the ui's memory), e.g. for a computer's list.
fn ask_once(ui: &mut egui::Ui, key: (&str, u64), ask: impl FnOnce()) {
    let id = egui::Id::new(key);
    if !ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
        ui.data_mut(|d| d.insert_temp(id, true));
        ask();
    }
}

/// What an accordion header shows.
#[derive(Clone, Copy)]
struct AccHead<'a> {
    first: bool,
    open: bool,
    name: &'a str,
    sub: &'a str,
    state: ui_kit::Progress<'a>,
    /// A button at the right end ("Hammasini yangilash").
    action: Option<&'a str>,
}

/// What a header reported.
struct AccOut {
    rect: Rect,
    clicked: bool,
    action: bool,
}

const ACC_H: f32 = 54.0;

/// An accordion's header: chevron, computer icon, name and a short line under it, the state and
/// (when there is something to do) one button on the right. One line above it (none above the
/// first: the frame has it).
fn acc_header(ui: &mut egui::Ui, p: &Palette, h: AccHead<'_>) -> AccOut {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, ACC_H), Sense::click());
    let action = paint_acc_header(ui, p, r, resp.hovered(), false, h);
    AccOut {
        rect: r,
        clicked: resp.clicked() && !action,
        action,
    }
}

/// The header again, pinned to the top of the visible area while its list is scrolled: opaque,
/// so what scrolls below it does not show through.
fn acc_header_pinned(
    ui: &mut egui::Ui,
    p: &Palette,
    r: Rect,
    id: egui::Id,
    h: AccHead<'_>,
) -> AccOut {
    let resp = ui.interact(r, id, Sense::click());
    let action = paint_acc_header(ui, p, r, resp.hovered(), true, h);
    AccOut {
        rect: r,
        clicked: resp.clicked() && !action,
        action,
    }
}

fn paint_acc_header(
    ui: &mut egui::Ui,
    p: &Palette,
    r: Rect,
    hovered: bool,
    solid: bool,
    h: AccHead<'_>,
) -> bool {
    let w = r.width();
    if solid {
        ui.painter().rect_filled(r, Rounding::ZERO, p.card);
        ui.painter().hline(
            r.x_range(),
            r.bottom() - 0.5,
            Stroke::new(1.0_f32, p.line_strong),
        );
    } else {
        if !h.first {
            ui.painter()
                .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
        }
        let fill = if h.open {
            p.card
        } else if hovered {
            p.hover
        } else {
            Color32::TRANSPARENT
        };
        let inner = if h.first {
            r.shrink(1.0)
        } else {
            Rect::from_min_max(r.min + vec2(1.0, 1.0), r.max - vec2(1.0, 0.0))
        };
        ui.painter().rect_filled(inner, Rounding::ZERO, fill);
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let y = r.center().y;
    ui.painter().text(
        pos2(r.left() + 18.0, y),
        Align2::CENTER_CENTER,
        if h.open {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        },
        FontId::proportional(14.0),
        if h.open { p.accent_ink } else { p.muted },
    );
    ui.painter().text(
        pos2(r.left() + 44.0, y),
        Align2::CENTER_CENTER,
        icons::MONITOR,
        FontId::proportional(17.0),
        if h.open { p.accent } else { p.muted },
    );
    let nw = w * 0.34;
    let g = ui_kit::truncated(ui, h.name, FontId::new(13.5, ui_kit::bold()), p.strong, nw);
    let gy = if h.sub.is_empty() {
        y - g.size().y / 2.0
    } else {
        y - 9.0 - g.size().y / 2.0
    };
    ui.painter().galley(pos2(r.left() + 64.0, gy), g, p.strong);
    if !h.sub.is_empty() {
        let s = ui_kit::truncated(ui, h.sub, FontId::new(11.5, ui_kit::mono()), p.muted, nw);
        ui.painter().galley(
            pos2(r.left() + 64.0, y + 9.0 - s.size().y / 2.0),
            s,
            p.muted,
        );
    }
    // The button first (it takes its width from the right), then the state in what is left.
    let mut clicked = false;
    let mut right = r.right() - 14.0;
    if let Some(label) = h.action {
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(r.left() + 64.0 + nw, r.top() + 8.0),
                    pos2(r.right() - 14.0, r.bottom() - 8.0),
                ))
                .layout(Layout::right_to_left(Align::Center)),
        );
        let b = ui_kit::button(&mut c, Some(icons::DOWNLOAD), label, Kind::Primary);
        clicked = b.clicked();
        right = b.rect.left() - 14.0;
    }
    let cell = Rect::from_min_max(
        pos2(r.left() + 64.0 + nw + 12.0, r.top()),
        pos2(right.max(r.left() + 64.0 + nw + 12.0), r.bottom()),
    );
    ui_kit::progress(ui, cell, h.state);
    clicked
}

/// The tree line of an open accordion, under the computer's icon: every row hangs on it with a
/// short branch (like folders in a tree), and it ends at the last branch. Each row draws its
/// branch and notes where it is; [`acc_rail_finish`] draws the trunk once all rows are there.
fn acc_rail(ui: &egui::Ui, p: &Palette, r: Rect) {
    let id = egui::Id::new("acc-rail");
    let top = ui
        .data(|d| d.get_temp::<(f32, f32, f32)>(id))
        .map_or(r.top(), |c| c.1);
    ui.data_mut(|d| d.insert_temp(id, (r.left() + 44.0, top, r.center().y)));
    ui.painter().hline(
        (r.left() + 44.0)..=(r.left() + 66.0),
        r.center().y,
        Stroke::new(2.0_f32, p.accent.gamma_multiply(0.55)),
    );
}

/// The trunk: from the top of the first row to the branch of the last one.
fn acc_rail_finish(ui: &mut egui::Ui, p: &Palette) {
    let id = egui::Id::new("acc-rail");
    if let Some((x, top, end)) = ui.data(|d| d.get_temp::<(f32, f32, f32)>(id)) {
        ui.painter().vline(
            x,
            top..=end,
            Stroke::new(2.0_f32, p.accent.gamma_multiply(0.55)),
        );
    }
    ui.data_mut(|d| d.remove::<(f32, f32, f32)>(id));
}

/// A row inside an open accordion: icon, a title and a line under it, a control on the right.
/// Indented under the header's name; one line above it.
fn acc_row(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    icon_color: Color32,
    title: &str,
    sub: Option<(&str, Color32)>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 50.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_max(r.min + vec2(1.0, 0.0), r.max - vec2(1.0, 0.0)),
        Rounding::ZERO,
        p.bg,
    );
    ui.painter().hline(
        (r.left() + 56.0)..=r.right(),
        r.top(),
        Stroke::new(1.0_f32, p.line),
    );
    acc_rail(ui, p, r);
    let y = r.center().y;
    ui.painter().text(
        pos2(r.left() + 76.0, y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        icon_color,
    );
    let tw = w - 96.0 - 160.0;
    let g = ui_kit::truncated(ui, title, FontId::proportional(13.0), p.text, tw);
    let gy = if sub.is_some() {
        y - 9.0 - g.size().y / 2.0
    } else {
        y - g.size().y / 2.0
    };
    ui.painter().galley(pos2(r.left() + 96.0, gy), g, p.text);
    if let Some((s, col)) = sub {
        let sg = ui_kit::truncated(ui, s, FontId::new(11.5, ui_kit::mono()), col, tw);
        ui.painter()
            .galley(pos2(r.left() + 96.0, y + 9.0 - sg.size().y / 2.0), sg, col);
    }
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(r.right() - 170.0, r.top()),
                r.max - vec2(16.0, 0.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    control(&mut c);
}

/// "Getting the list…" inside an open accordion.
fn acc_waiting(ui: &mut egui::Ui, p: &Palette) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_max(r.min + vec2(1.0, 0.0), r.max - vec2(1.0, 0.0)),
        Rounding::ZERO,
        p.bg,
    );
    ui.painter().hline(
        (r.left() + 56.0)..=r.right(),
        r.top(),
        Stroke::new(1.0_f32, p.line),
    );
    ui_kit::progress(
        ui,
        Rect::from_min_max(pos2(r.left() + 68.0, r.top()), r.max - vec2(16.0, 0.0)),
        ui_kit::Progress::Waiting(tr("Ro'yxat olinmoqda…")),
    );
}

/// A Vimora update the teacher started on this computer, or one it is behind with.
enum VimoraState {
    Done,
    Waiting,
    Failed(String),
    Behind,
}

impl VimoraState {
    fn as_progress(&self) -> ui_kit::Progress<'_> {
        match self {
            VimoraState::Done => ui_kit::Progress::Done(tr("o'rnatildi")),
            VimoraState::Waiting => ui_kit::Progress::Waiting(tr("o'rnatilmoqda")),
            VimoraState::Failed(why) => ui_kit::Progress::Failed(why),
            VimoraState::Behind => ui_kit::Progress::Idle(tr("yangilash kerak"), WARN_INK),
        }
    }
}

/// Amber for "needs an update" (the palette's warning colour, usable without a context).
const WARN_INK: Color32 = Color32::from_rgb(0xE0, 0xA1, 0x2F);

fn vimora_state(v: &View, cx: &FormCtx<'_>) -> Option<VimoraState> {
    let current = !v.version.is_empty() && !vimora_update::is_newer(cx.latest, &v.version);
    let job = cx.hub.lock().unwrap().vimora_jobs.get(&v.key).cloned();
    // The computer says it is installing: wait for it, with its own words.
    let installing = v
        .status
        .busy
        .as_deref()
        .is_some_and(|b| b.contains("Vimora"));
    match (current, job) {
        (true, Some(_)) => Some(VimoraState::Done),
        (true, None) => None,
        (false, Some(j)) => Some(match j.failed {
            Some(why) => VimoraState::Failed(why),
            None if installing || j.at.elapsed() < std::time::Duration::from_secs(150) => {
                VimoraState::Waiting
            }
            // Nothing running any more and the new version has not shown up: not an endless
            // "installing", a plain failure.
            None => VimoraState::Failed(tr("o'rnatilmadi — qayta urinib ko'ring").into()),
        }),
        (false, None) => Some(VimoraState::Behind),
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
    allow: bool,
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
                        .tip(match (all, allow) {
                            (true, false) => tr("Toifa to'liq bloklangan"),
                            (false, false) => tr("Butun toifani bloklash"),
                            (true, true) => tr("Toifa to'liq ruxsat etilgan"),
                            (false, true) => tr("Butun toifaga ruxsat berish"),
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
                let items = cat.items.clone();
                ui_kit::accordion_body(
                    ui,
                    egui::Id::new(("cat-acc", cat.id.clone())),
                    expanded,
                    |ui| {
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
                                            let (r, _) = ui.allocate_exact_size(
                                                vec2(col_w, 34.0),
                                                Sense::hover(),
                                            );
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
                    },
                );
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
    word_cats: &mut Vec<crate::blockcat::BlockCategory>,
    open_cat: &mut Option<String>,
    devices: &mut vimora_core::DevicePolicy,
    extra: &mut NetExtra,
    safe: &mut bool,
    entry: &mut String,
    import: &mut bool,
) {
    let before = *tab;
    let tabs = [
        (
            NetTab::Domains,
            icons::GLOBE_LOCK,
            tr("Saytlar"),
            Some(crate::blockcat::effective(domains, site_cats).len()),
        ),
        (
            NetTab::Keywords,
            icons::TEXT_SEARCH,
            tr("So'zlar"),
            Some(crate::blockcat::effective(keywords, word_cats).len()),
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
    ];
    let items: Vec<ui_kit::TabItem> = tabs
        .iter()
        .map(|(_, ic, label, n)| ui_kit::TabItem {
            icon: Some(*ic),
            label,
            count: *n,
        })
        .collect();
    let current = tabs.iter().position(|x| x.0 == *tab).unwrap_or(0);
    if let Some(i) = ui_kit::tab_strip(ui, &items, current) {
        *tab = tabs[i].0;
    }
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
                    (0, tr("Bloklash")),
                    (1, tr("Faqat ruxsat")),
                    (2, tr("Internet yo'q")),
                ],
            );
            if extra.web_mode == 2 {
                ui.label(
                    RichText::new(with_icon(
                        icons::GLOBE_LOCK,
                        tr("Internet butunlay o'chiq. Sinf tarmog'i (o'qituvchi bilan aloqa) ishlayveradi."),
                    ))
                    .size(13.0)
                    .color(p.text),
                );
            } else {
                // One list for both: "Bloklash" closes what is on it, "Faqat ruxsat" opens only
                // what is on it.
                let allow = extra.web_mode == 1;
                block_list(
                    ui,
                    p,
                    domains,
                    entry,
                    import,
                    tr("sayt: tiktok.com"),
                    if allow {
                        tr("Brauzerda faqat yoqilgan saytlar ochiladi, qolganlari yopiq (masalan, dars yoki test sayti). Ro'yxat bloklashdagi bilan bir xil.")
                    } else {
                        tr("Yoqilgan saytlar ochilmaydi (subdomenlari ham). Toifani to'liq yoki ichidan alohida saytlarni yoqish mumkin. Import qilingan ro'yxat alohida toifa bo'ladi.")
                    },
                    &[],
                    |ui| categories(ui, p, site_cats, crate::blockcat::SITES, open_cat, allow),
                );
            }
        }
        NetTab::Keywords => block_list(
            ui,
            p,
            keywords,
            entry,
            import,
            tr("so'z: kazino"),
            tr("Shu so'z bilan qidirilsa, sahifa yopiladi, o'quvchiga ogohlantirish chiqadi va sizga xabar keladi."),
            &[],
            |ui| categories(ui, p, word_cats, crate::blockcat::WORDS, open_cat, false),
        ),
        NetTab::Apps => {
            mode_row(
                ui,
                p,
                &mut extra.app_mode,
                &[(0, tr("Bloklash")), (1, tr("Faqat ruxsat"))],
            );
            let allow = extra.app_mode == 1;
            block_list(
                ui,
                p,
                apps,
                entry,
                import,
                tr("dastur: steam.exe"),
                if allow {
                    tr("Faqat yoqilgan dasturlar ishlaydi, boshqalari ochilishi bilan yopiladi (Windows). Explorer va Sozlamalar ishlayveradi. Ro'yxat bloklashdagi bilan bir xil.")
                } else {
                    tr("Yoqilgan dasturlar ochilishi bilan yopiladi. Jarayon nomini yozing (steam.exe yoki telegram). Toifani to'liq yoki ichidan alohida dasturlarni yoqish mumkin.")
                },
                &[],
                |ui| categories(ui, p, app_cats, crate::blockcat::APPS, open_cat, allow),
            );
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
                let modes = [UsbMode::Allow, UsbMode::ReadOnly, UsbMode::Block];
                if let Some(i) = ui_kit::choices(
                    ui,
                    &[
                        (None, tr("Ruxsat")),
                        (None, tr("Faqat o'qish")),
                        (None, tr("Taqiqlangan")),
                    ],
                    modes.iter().position(|m| *m == d.usb).unwrap_or(0),
                ) {
                    d.usb = modes[i];
                }
            },
        );
        for (blocked, title, help) in [
            (
                &mut d.cd,
                "CD/DVD",
                Some(tr("Yoqilgan — ruxsat, o'chirilgan — taqiqlangan. \"Qo'llash\" bosilganda kuchga kiradi va dastur yopiq bo'lsa ham saqlanadi.")),
            ),
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
}

/// One segmented choice above a tab's content (what the list below means).
fn mode_row(ui: &mut egui::Ui, _p: &Palette, value: &mut u8, items: &[(u8, &str)]) {
    let cells: Vec<(Option<&str>, &str)> = items.iter().map(|(_, l)| (None, *l)).collect();
    let current = items.iter().position(|(v, _)| v == value).unwrap_or(0);
    if let Some(i) = ui_kit::choices(ui, &cells, current) {
        *value = items[i].0;
    }
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

/// "Skanerlash": computers that belong to no teacher (a new install, or removed from its
/// teacher). They show in every teacher that scans; "Qo'shish" joins one to this teacher.
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
    let scanning = since < SCANNING;
    // One status row: what the scan is doing, "?" with the hint, and "scan again".
    let w = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(vec2(w, 46.0), Sense::hover());
    ui.painter()
        .rect(bar, Rounding::ZERO, p.bg, Stroke::new(1.0_f32, p.line));
    let status = tf!("{n} ta kompyuter topildi", n = free.len());
    ui_kit::progress(
        ui,
        Rect::from_min_max(
            pos2(bar.left() + 14.0, bar.top()),
            pos2(bar.right() - 220.0, bar.bottom()),
        ),
        if scanning {
            ui_kit::Progress::Waiting(tr("Skanerlanmoqda…"))
        } else if free.is_empty() {
            ui_kit::Progress::Idle(tr("Yangi kompyuter topilmadi"), p.muted)
        } else {
            ui_kit::Progress::Idle(&status, p.text)
        },
    );
    let mut ctl = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(6.0, 6.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    ctl.add_enabled_ui(!scanning, |ui| {
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
    ctl.add_space(6.0);
    ui_kit::info(
        &mut ctl,
        tr("Vimora o'rnatilgan, lekin hali hech bir o'qituvchiga qo'shilmagan kompyuterlar shu yerda chiqadi. Kompyuter shu tarmoqda (Wi-Fi yoki LAN) bo'lishi kerak."),
    );
    ui.add_space(12.0);
    if free.is_empty() {
        // An empty list: a calm picture instead of a paragraph.
        ui.add_space(46.0);
        ui.vertical_centered(|ui| {
            ui.label(ui_kit::icon(
                if scanning {
                    icons::WIFI
                } else {
                    icons::MONITOR
                },
                44.0,
                p.faint,
            ));
        });
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
