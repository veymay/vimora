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
    for e in h.log.iter() {
        let (ic, col) = match e.level {
            Level::Info => (icons::INFO, p.muted),
            Level::Ok => (icons::CIRCLE_CHECK, p.success),
            Level::Warn => (icons::TRIANGLE_ALERT, p.warning),
        };
        Frame::none()
            .fill(p.bg)
            .rounding(Rounding::ZERO)
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(ic).color(col));
                    ui.label(
                        RichText::new(&e.time)
                            .size(11.0)
                            .color(p.faint)
                            .family(ui_kit::mono()),
                    );
                    ui.label(RichText::new(&e.text).size(13.0));
                });
            });
    }
}

/// Body of every modal / drawer.
/// Read-only context the forms need besides the dialog itself.
pub(super) struct FormCtx<'a> {
    pub(super) hub: &'a SharedHub,
    pub(super) all: &'a [View],
    pub(super) targets: &'a [u64],
}

pub(super) fn dialog_body(
    ui: &mut egui::Ui,
    p: &Palette,
    dialog: &mut Dialog,
    confirm: &mut bool,
    clear_help: &mut Option<u64>,
    cx: &FormCtx<'_>,
) {
    let (hub, all, targets) = (cx.hub, cx.all, cx.targets);
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
            picked,
            manual,
            password,
        } => shadow_form(
            ui,
            p,
            ShadowForm {
                chosen,
                tab,
                restart,
                folders,
                request,
                picked,
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
            policy,
            os,
            apps,
            restart,
            request,
        } => updates_form(ui, p, policy, os, apps, restart, request, all, targets),
        Dialog::Message { .. } => {} // laid out by chat_form
        Dialog::Launch {
            web,
            url,
            program,
            args,
        } => {
            ui_kit::segmented(p).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if ui_kit::tab(ui, tr("Sayt"), None, *web).clicked() {
                        *web = true;
                    }
                    if ui_kit::tab(ui, tr("Dastur"), None, !*web).clicked() {
                        *web = false;
                    }
                });
            });
            ui.add_space(10.0);
            if *web {
                let r = wide(ui, url, "https://example.com");
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    *confirm = true;
                }
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
        Dialog::Apps(list) => {
            ui.label(RichText::new(tr("Jarayon nomlari, har biri yangi qatorda yoki vergul bilan. Ro'yxatdagi dasturlar ochilishi bilan yopiladi.")).color(p.muted));
            ui.add_space(4.0);
            ui.add(
                ui_kit::text_area(list)
                    .hint_text(tr("chrome\nsteam.exe\ndiscord"))
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        }
        Dialog::Net {
            tab,
            domains,
            keywords,
            safe,
        } => net_form(ui, p, tab, domains, keywords, safe),
        Dialog::Power(a) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (act, ic, label) in [
                    (PowerAction::Shutdown, icons::POWER, tr("O'chirish")),
                    (PowerAction::Restart, icons::ROTATE_CCW, tr("Qayta yoqish")),
                    (PowerAction::Logoff, icons::LOG_OUT, tr("Seansdan chiqish")),
                ] {
                    if power_tile(ui, p, ic, label, *a == act).clicked() {
                        *a = act;
                    }
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(with_icon(
                    icons::TRIANGLE_ALERT,
                    tr("Saqlanmagan ishlar yo'qolishi mumkin!"),
                ))
                .color(p.warning),
            );
        }
        Dialog::Settings(_) => {}
        Dialog::Room { name, .. } => {
            let r = ui.add(
                ui_kit::text_field(name)
                    .hint_text(tr("Masalan: 204-xona, Informatika"))
                    .desired_width(f32::INFINITY),
            );
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

pub(super) fn power_tile(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    selected: bool,
) -> egui::Response {
    tile(ui, p, icon, label, selected, p.danger)
}

pub(super) fn tile(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    selected: bool,
    accent: Color32,
) -> egui::Response {
    // Width from the whole row (not what is left), so all three tiles match.
    let w = (ui.max_rect().width() - 16.0) / 3.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(120.0), 84.0), Sense::click());
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
    pub(super) picked: &'a mut std::collections::BTreeSet<String>,
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
    let views: Vec<&View> = cx.all.iter().filter(|v| f.chosen.contains(&v.id)).collect();
    let usable = views
        .iter()
        .filter(|v| {
            v.status.service
                && !matches!(
                    v.status.freeze,
                    FreezeState::Unsupported | FreezeState::Unknown
                )
        })
        .count();
    let want_on = |v: &View| {
        matches!(
            v.status.freeze,
            FreezeState::Frozen | FreezeState::FreezePending | FreezeState::Maintenance
        )
    };
    let mut on = usable > 0
        && views.iter().all(|v| {
            !v.status.service
                || matches!(
                    v.status.freeze,
                    FreezeState::Unsupported | FreezeState::Unknown
                )
                || want_on(v)
        });
    let pending = views.iter().any(|v| {
        v.status.service
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
                && matches!(
                    v.status.freeze,
                    FreezeState::Unsupported | FreezeState::Unknown
                )
        })
        .count();
    let any_frozen = views
        .iter()
        .any(|v| v.status.service && v.status.freeze.is_frozen());

    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, label) in [
                (ShadowTab::Protection, tr("Himoya")),
                (ShadowTab::Commit, "Commit"),
                (ShadowTab::Settings, tr("Sozlama")),
            ] {
                if ui_kit::tab(ui, label, None, *f.tab == t).clicked() {
                    *f.tab = t;
                }
            }
        });
    });
    ui.add_space(12.0);

    if *f.tab == ShadowTab::Protection {
        // The switch.
        Frame::none()
        .fill(p.bg)
        .stroke(Stroke::new(1.0_f32, if on { p.accent } else { p.line }))
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icons::SNOWFLAKE)
                        .size(28.0)
                        .color(if on { p.accent_ink } else { p.faint }),
                );
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            RichText::new("Shadow")
                                .family(ui_kit::bold())
                                .size(15.0)
                                .color(p.strong),
                        );
                        ui_kit::info(
                            ui,
                            tr("Yoniqda qayta yuklanganda barcha o'zgarishlar o'chadi, kompyuter toza qaytadi. Konsoldan qilganingiz saqlanadi."),
                        );
                    });
                    let (txt, col) = if views.is_empty() {
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
                    ui.label(RichText::new(txt).size(12.5).color(col));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(usable > 0, |ui| {
                        let was = on;
                        if ui_kit::toggle(ui, &mut on).clicked() && on != was {
                            *f.request = Some(ShadowRequest::Apply(if on {
                                FreezeAction::Freeze
                            } else {
                                FreezeAction::Thaw
                            }));
                        }
                    });
                });
            });
        });

        // Problems, one short line each.
        for (n, text) in [
            (no_service, tr("{n} ta kompyuterda Vimora xizmati yo'q")),
            (
                unsupported,
                tr("{n} ta kompyuterda Shadow yo'q (Windows: Shadow Defender kerak)"),
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

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_enabled_ui(usable > 0, |ui| {
                ui.checkbox(f.restart, tr("Darhol qayta yuklash"));
            });
            ui_kit::info(
                ui,
                tr("Kompyuterlar o'zi qayta yuklanadi. Saqlanmagan ish yo'qolishi mumkin."),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui_kit::info(
                    ui,
                    tr("Shadow'ni vaqtincha o'chirib, yangilanishlarni o'rnatadi va qayta yoqadi."),
                );
                ui.add_enabled_ui(usable > 0, |ui| {
                    if ui_kit::button(
                        ui,
                        Some(icons::WRENCH),
                        tr("Yangilab yoqish"),
                        Kind::Outline,
                    )
                    .clicked()
                    {
                        *f.request = Some(ShadowRequest::Apply(FreezeAction::Maintenance));
                    }
                });
            });
        });

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

    if *f.tab == ShadowTab::Commit {
        commit_section(ui, p, &mut f, cx, any_frozen);
    }

    if *f.tab == ShadowTab::Settings {
        let label = |ui: &mut egui::Ui, text: &str, help: &str| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui_kit::hud(ui, text, p.muted);
                ui_kit::info(ui, help);
            });
        };
        label(
            ui,
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
        label(
            ui,
            tr("Doim saqlanadigan papkalar"),
            tr("Bu papkalar qayta yuklanganda ham o'chmaydi. Har biri yangi qatorda."),
        );
        ui.add(
            ui_kit::text_area(f.folders)
                .hint_text(tr("D:\\Darslar"))
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        ui.add_space(8.0);
        if ui_kit::button(ui, Some(icons::CHECK), tr("Saqlash"), Kind::Primary).clicked() {
            *f.request = Some(ShadowRequest::SaveFolders);
        }
    }
}

/// Pick what must survive restarts, press Commit.
fn commit_section(
    ui: &mut egui::Ui,
    p: &Palette,
    f: &mut ShadowForm<'_>,
    cx: &FormCtx<'_>,
    any_frozen: bool,
) {
    use vimora_core::ChangeKind;
    titled(
        ui,
        p,
        "Commit",
        tr("Belgilangan fayllar doimiy saqlanadi, qolgan o'zgarishlar qayta yuklanganda o'chadi."),
    );
    if !any_frozen {
        ui.label(
            RichText::new(tr("Shadow yoniq emas — o'zgarishlar saqlanmoqda."))
                .size(12.5)
                .color(p.muted),
        );
        return;
    }
    let rows = merged_changes(cx);
    let mut shown: Vec<(String, Option<ChangeRow>)> =
        rows.iter().map(|(k, v)| (k.clone(), Some(*v))).collect();
    for path in f.picked.iter() {
        if !rows.contains_key(path) {
            shown.push((path.clone(), None));
        }
    }
    if shown.is_empty() {
        ui.label(
            RichText::new(tr("Ro'yxat yo'q. Yo'lni o'zingiz qo'shing."))
                .size(12.5)
                .color(p.faint),
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt("shadow-changes")
            .max_height(280.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                Frame::none()
                    .stroke(Stroke::new(1.0_f32, p.line))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, (path, info)) in shown.iter().take(500).enumerate() {
                            let (rect, resp) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 32.0),
                                Sense::click(),
                            );
                            if i > 0 {
                                ui.painter().hline(
                                    rect.x_range(),
                                    rect.top(),
                                    Stroke::new(1.0_f32, p.line),
                                );
                            }
                            if resp.hovered() {
                                ui.painter().rect_filled(rect, Rounding::ZERO, p.hover);
                            }
                            let on = f.picked.contains(path);
                            let bx = Rect::from_center_size(
                                pos2(rect.left() + 18.0, rect.center().y),
                                vec2(14.0, 14.0),
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
                                    FontId::proportional(11.0),
                                    p.on_accent,
                                );
                            }
                            let (tag, col) = match info.map(|i| i.0) {
                                Some(ChangeKind::Added) => ("+", p.accent_ink),
                                Some(ChangeKind::Modified) => ("~", p.warning),
                                Some(ChangeKind::Deleted) => ("\u{2212}", p.danger),
                                None => ("\u{2022}", p.muted),
                            };
                            ui.painter().text(
                                pos2(rect.left() + 38.0, rect.center().y),
                                Align2::CENTER_CENTER,
                                tag,
                                FontId::new(14.0, ui_kit::mono_bold()),
                                col,
                            );
                            let size = info
                                .filter(|i| i.0 != ChangeKind::Deleted)
                                .map(|i| fmt_size(i.1))
                                .unwrap_or_default();
                            let sg = ui_kit::caps(ui, &size, 9.5, ui_kit::mono(), p.faint);
                            let sw = sg.size().x;
                            ui.painter().galley(
                                pos2(
                                    rect.right() - 10.0 - sw,
                                    rect.center().y - sg.size().y / 2.0,
                                ),
                                sg,
                                p.faint,
                            );
                            let name = ui_kit::truncated(
                                ui,
                                path,
                                FontId::new(12.5, ui_kit::medium()),
                                p.text,
                                rect.width() - 62.0 - sw,
                            );
                            ui.painter().galley(
                                pos2(rect.left() + 54.0, rect.center().y - name.size().y / 2.0),
                                name,
                                p.text,
                            );
                            if resp.clicked() {
                                if on {
                                    f.picked.remove(path);
                                } else {
                                    f.picked.insert(path.clone());
                                }
                            }
                        }
                    });
            });
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui_kit::input_frame(p).show(ui, |ui| {
            ui.add(
                ui_kit::text_field(f.manual)
                    .hint_text(tr("Fayl yoki papka yo'li"))
                    .frame(false)
                    .desired_width((ui.available_width() - 250.0).max(120.0)),
            );
        });
        if ui_kit::icon_cell(ui, icons::PLUS, 34.0, false)
            .tip(tr("Yo'lni qo'shish"))
            .clicked()
        {
            let path = f.manual.trim().trim_matches('"').to_string();
            if !path.is_empty() {
                f.picked.insert(path);
                f.manual.clear();
            }
        }
        if ui_kit::icon_cell(ui, icons::REFRESH_CW, 34.0, false)
            .tip(tr("Ro'yxatni yangilash"))
            .clicked()
        {
            *f.request = Some(ShadowRequest::ListChanges);
        }
        let n = f.picked.len();
        ui.add_enabled_ui(n > 0, |ui| {
            if ui_kit::button(
                ui,
                Some(icons::SHIELD_CHECK),
                &tf!("Commit ({n})", n = n),
                Kind::Primary,
            )
            .clicked()
            {
                *f.request = Some(ShadowRequest::Commit);
            }
        });
    });
}

/// Every change reported by the target PCs: path → (kind, size, on how many PCs).
type ChangeRow = (vimora_core::ChangeKind, u64, usize);

fn merged_changes(cx: &FormCtx<'_>) -> BTreeMap<String, ChangeRow> {
    let mut rows = BTreeMap::new();
    let h = cx.hub.lock().unwrap();
    for id in cx.targets {
        let Some(list) = h.students.get(id).and_then(|s| s.changes.as_ref()) else {
            continue;
        };
        for c in list {
            let e = rows.entry(c.path.clone()).or_insert((c.kind, c.size, 0));
            e.2 += 1;
        }
    }
    rows
}

fn fmt_size(b: u64) -> String {
    if b < 1024 {
        format!("{b} B")
    } else if b < 1024 * 1024 {
        format!("{} KB", b / 1024)
    } else {
        format!("{:.1} MB", b as f64 / 1024.0 / 1024.0)
    }
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

#[allow(clippy::too_many_arguments)]
pub(super) fn updates_form(
    ui: &mut egui::Ui,
    p: &Palette,
    policy: &mut UpdatePolicy,
    os: &mut bool,
    apps: &mut bool,
    restart: &mut bool,
    request: &mut Option<UpdateRequest>,
    all: &[View],
    targets: &[u64],
) {
    section(ui, p, tr("Avtomatik yangilanish siyosati"));
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (m, label) in [
                (OsUpdateMode::Auto, tr("Avtomatik")),
                (OsUpdateMode::Manual, tr("Faqat konsoldan")),
                (OsUpdateMode::Disabled, tr("O'chirilgan")),
            ] {
                if ui_kit::tab(ui, label, None, policy.os == m).clicked() {
                    policy.os = m;
                }
            }
        });
    });
    let hint = match policy.os {
        OsUpdateMode::Auto => tr("Windows Update / apt o'zi yuklab, o'zi o'rnatadi."),
        OsUpdateMode::Manual => {
            tr("Kompyuterlar o'zi o'rnatmaydi; siz shu yerdan qachon o'rnatishni hal qilasiz.")
        }
        OsUpdateMode::Disabled => tr("Tizim yangilanishlari butunlay o'chiriladi."),
    };
    ui.label(RichText::new(hint).size(12.5).color(p.muted));
    ui.checkbox(
        &mut policy.apps_auto,
        tr("Dasturlar o'zi yangilansin (Microsoft Store, Chrome, Edge, snap)"),
    );

    section(ui, p, tr("Hozir bajarish"));
    ui.horizontal(|ui| {
        ui.checkbox(os, tr("Tizim (Windows Update / apt)"));
        ui.checkbox(apps, tr("Dasturlar (winget / snap)"));
    });
    ui.checkbox(restart, tr("Kerak bo'lsa qayta yuklash"));
    ui.horizontal(|ui| {
        if ui_kit::button(ui, Some(icons::REFRESH_CW), tr("Tekshirish"), Kind::Outline).clicked() {
            *request = Some(UpdateRequest::Check);
        }
        if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("O'rnatish"), Kind::Primary).clicked()
            && (*os || *apps)
        {
            *request = Some(UpdateRequest::Install);
        }
    });

    section(ui, p, tr("Kompyuterlar"));
    target_rows(ui, p, all, targets, |v| {
        let col = if v.status.busy.is_some() {
            p.warning
        } else if v.status.os_updates.unwrap_or(0) + v.status.app_updates.unwrap_or(0) > 0 {
            p.accent_ink
        } else {
            p.muted
        };
        let mut t = updates_label(&v.status);
        if v.status.freeze.is_frozen() {
            t.push_str(" · Shadow");
        }
        (t, col)
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr("Shadow rejimdagi kompyuterga yangilanish o'rnatilmaydi (qayta yuklanganda yo'qoladi) — Shadow → \"Xizmat rejimi\" ni ishlating."))
            .size(12.0)
            .color(p.faint),
    );
}

pub(super) fn add_preset(list: &mut String, items: &[&str]) {
    let existing: Vec<String> = split_list(list).iter().map(|s| s.to_lowercase()).collect();
    for it in items {
        if !existing.contains(&it.to_lowercase()) {
            if !list.trim().is_empty() && !list.ends_with('\n') {
                list.push('\n');
            }
            list.push_str(it);
            list.push('\n');
        }
    }
}

pub(super) fn net_form(
    ui: &mut egui::Ui,
    p: &Palette,
    tab: &mut NetTab,
    domains: &mut String,
    keywords: &mut String,
    safe: &mut bool,
) {
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, label, n) in [
                (
                    NetTab::Domains,
                    tr("Saytlar"),
                    Some(split_list(domains).len()),
                ),
                (
                    NetTab::Keywords,
                    tr("Kalit so'zlar"),
                    Some(split_list(keywords).len()),
                ),
                (NetTab::Search, tr("Xavfsiz qidiruv"), None),
            ] {
                if ui_kit::tab(ui, label, n, *tab == t).clicked() {
                    *tab = t;
                }
            }
        })
    });
    ui.add_space(12.0);
    let presets = |ui: &mut egui::Ui, list: &mut String, sets: &[(&str, &[&str])]| {
        ui_kit::hud(ui, tr("Tayyor ro'yxatlar"), p.faint);
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for (name, items) in sets {
                if ui_kit::button(ui, Some(icons::PLUS), name, Kind::Outline).clicked() {
                    add_preset(list, items);
                }
            }
        });
    };
    match tab {
        NetTab::Domains => {
            ui.label(
                RichText::new(tr("Bu domenlar va ularning barcha subdomenlari ochilmaydi (hosts fayli + Chrome/Edge/Yandex/Brave siyosatlari)."))
                    .color(p.muted),
            );
            ui.add_space(6.0);
            presets(
                ui,
                domains,
                &[
                    (
                        tr("Ijtimoiy tarmoqlar"),
                        &[
                            "instagram.com",
                            "tiktok.com",
                            "facebook.com",
                            "vk.com",
                            "ok.ru",
                            "x.com",
                            "twitter.com",
                        ],
                    ),
                    (
                        tr("O'yinlar"),
                        &[
                            "roblox.com",
                            "steampowered.com",
                            "epicgames.com",
                            "poki.com",
                            "crazygames.com",
                            "miniclip.com",
                        ],
                    ),
                    (
                        tr("Video"),
                        &["youtube.com", "twitch.tv", "rutube.ru", "kinopoisk.ru"],
                    ),
                ],
            );
            ui.add_space(6.0);
            ui.add(
                ui_kit::text_area(domains)
                    .hint_text(tr("youtube.com\ntiktok.com"))
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        }
        NetTab::Keywords => {
            ui.label(
                RichText::new(
                    tr("Google, Yandex, Bing, YouTube va boshqa qidiruv tizimlarida shu so'zlar qidirilsa, brauzer tabi darhol yopiladi, o'quvchiga ogohlantirish chiqadi va sizga xabar keladi. Har biri yangi qatorda; iboralar ham mumkin."),
                )
                .color(p.muted),
            );
            ui.add_space(6.0);
            presets(
                ui,
                keywords,
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
            );
            ui.add_space(6.0);
            ui.add(
                ui_kit::text_area(keywords)
                    .hint_text(tr("kazino\nreferat yuklab olish"))
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        }
        NetTab::Search => {
            ui.horizontal(|ui| {
                ui.checkbox(safe, "");
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Xavfsiz qidiruvni majburlash")).family(ui_kit::bold()).size(15.0));
                    ui.label(RichText::new(tr("Nomaqbul natijalar qidiruvdan olib tashlanadi; o'quvchi bu sozlamani o'chira olmaydi.")).color(p.muted));
                });
            });
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                for (name, what) in [
                    ("Google", "SafeSearch"),
                    ("Yandex", tr("Oilaviy qidiruv")),
                    ("Bing", "Strict"),
                    ("DuckDuckGo", "Safe"),
                    ("YouTube", tr("Cheklangan rejim")),
                ] {
                    let col = if *safe { p.success } else { p.faint };
                    ui_kit::chip(ui, &format!("{name} · {what}"), col);
                }
            });
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Brauzerlarda DNS-over-HTTPS o'chiriladi, shuning uchun cheklovlarni chetlab bo'lmaydi.")).size(12.5).color(p.faint));
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
    let inner = comp.shrink2(vec2(16.0, 14.0));
    let send_rect = Rect::from_min_size(
        pos2(inner.right() - inner.height(), inner.top()),
        vec2(inner.height(), inner.height()),
    );
    let edit_rect = Rect::from_min_max(inner.min, pos2(send_rect.left() - 8.0, inner.bottom()));
    let edit = ui
        .put(
            edit_rect,
            ui_kit::text_area(text)
                .hint_text(tr("Xabar yozing…"))
                .desired_rows(1)
                .vertical_align(egui::Align::Center)
                .desired_width(edit_rect.width())
                .min_size(edit_rect.size())
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
                        format!("{} · {}", m.time, m.who)
                    } else {
                        m.time.clone()
                    };
                    ui.label(RichText::new(meta).size(10.5).color(meta_col));
                });
            });
    });
}
