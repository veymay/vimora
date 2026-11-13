//! The other "Dars" tabs: attendance, polls, tests, and the lesson timer with rewards.

use super::lesson::{Lesson, LessonRequest};
use super::*;
use vimora_core::TestQuestion;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

/// A table: header captions with their column widths (fractions of the width), coloured cells.
pub(super) fn table(
    ui: &mut egui::Ui,
    p: &Palette,
    cols: &[(&str, f32)],
    rows: &[Vec<(String, Color32)>],
) {
    let w = ui.available_width();
    let xs: Vec<f32> = cols
        .iter()
        .scan(12.0, |x, (_, f)| {
            let at = *x;
            *x += f * (w - 24.0);
            Some(at)
        })
        .collect();
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            ui.painter().rect_filled(hr, Rounding::ZERO, p.bg);
            for ((t, _), x) in cols.iter().zip(&xs) {
                let g = ui_kit::caps(ui, t, 9.5, ui_kit::mono(), p.faint);
                ui.painter().galley(
                    pos2(hr.left() + x, hr.center().y - g.size().y / 2.0),
                    g,
                    p.faint,
                );
            }
            if rows.is_empty() {
                let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    tr("Hali ma'lumot yo'q"),
                    FontId::proportional(12.5),
                    p.faint,
                );
            }
            for row in rows {
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                for (i, ((text, col), x)) in row.iter().zip(&xs).enumerate() {
                    let max = cols[i].1 * (w - 24.0) - 8.0;
                    let font = if i == 0 {
                        FontId::new(13.0, ui_kit::bold())
                    } else {
                        FontId::proportional(12.5)
                    };
                    let g = ui_kit::truncated(ui, text, font, *col, max);
                    ui.painter().galley(
                        pos2(r.left() + x, r.center().y - g.size().y / 2.0),
                        g,
                        *col,
                    );
                }
            }
        });
}

fn caption(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui_kit::hud(ui, text, p.muted);
    ui.add_space(6.0);
}

fn online_targets<'a>(cx: &'a FormCtx<'_>) -> Vec<&'a View> {
    cx.all
        .iter()
        .filter(|v| v.online && cx.targets.contains(&v.id))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Attendance

pub(super) fn attendance(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    let att = cx.hub.lock().unwrap().attendance.1.clone();
    let came = cx.all.iter().filter(|v| att.contains_key(&v.key)).count();
    let online = cx.all.iter().filter(|v| v.online).count();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tf!(
                "Bugun keldi: {a} / {b} · Hozir darsda: {c}",
                a = came,
                b = cx.all.len(),
                c = online
            ))
            .size(13.5)
            .color(p.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("CSV eksport"), Kind::Outline).clicked()
            {
                l.request = Some(LessonRequest::ExportAttendance);
            }
        });
    });
    ui.add_space(10.0);
    let rows: Vec<Vec<(String, Color32)>> = cx
        .all
        .iter()
        .map(|v| {
            let rec = att.get(&v.key);
            let (state, col) = match (rec, v.online) {
                (_, true) => (tr("Darsda"), p.success),
                (Some(_), false) => (tr("Ketgan"), p.muted),
                (None, false) => (tr("Kelmagan"), p.danger),
            };
            vec![
                (v.label.clone(), p.text),
                (
                    rec.map(|r| r.1.clone()).unwrap_or_else(|| v.user.clone()),
                    p.text,
                ),
                (rec.map(|r| r.2.clone()).unwrap_or("—".into()), p.muted),
                (state.to_string(), col),
            ]
        })
        .collect();
    table(
        ui,
        p,
        &[
            (tr("Kompyuter"), 0.3),
            (tr("O'quvchi"), 0.3),
            (tr("Kelgan vaqti"), 0.2),
            (tr("Holat"), 0.2),
        ],
        &rows,
    );
}

// ---------------------------------------------------------------------------------------------
// Polls

pub(super) fn poll(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    let running = cx
        .hub
        .lock()
        .unwrap()
        .poll
        .as_ref()
        .map(|s| (s.question.clone(), s.options.clone(), s.answers.clone()));
    if let Some((question, options, answers)) = running {
        poll_results(ui, p, l, cx, &question, &options, &answers);
        return;
    }
    let kinds = [
        (0, tr("Variantlar")),
        (1, tr("Ha / Yo'q")),
        (2, tr("Matnli javob")),
        (3, tr("Dars haqida fikr")),
    ];
    let before = l.poll_kind;
    let cells: Vec<(Option<&str>, &str)> = kinds.iter().map(|(_, l)| (None, *l)).collect();
    let current = kinds
        .iter()
        .position(|(k, _)| *k == l.poll_kind)
        .unwrap_or(0);
    if let Some(i) = ui_kit::choices(ui, &cells, current) {
        l.poll_kind = kinds[i].0;
    }
    if l.poll_kind == 3 && (before != 3 || l.poll_q.is_empty()) {
        l.poll_q = tr("Bugungi dars sizga qanday bo'ldi?").to_string();
    }
    ui.add_space(12.0);
    caption(ui, p, tr("Savol"));
    ui.add(
        ui_kit::text_field(&mut l.poll_q)
            .hint_text(tr("Masalan: 7 × 8 nechaga teng?"))
            .desired_width(ui.available_width()),
    );
    ui.add_space(12.0);
    if l.poll_kind == 0 {
        caption(ui, p, tr("Javob variantlari"));
        while l.poll_opts.len() < 2 {
            l.poll_opts.push(String::new());
        }
        let mut remove = None;
        for (i, o) in l.poll_opts.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}", (b'A' + i as u8) as char))
                        .family(ui_kit::bold())
                        .color(p.muted),
                );
                ui.add(ui_kit::text_field(o).desired_width(ui.available_width() - 40.0));
                if i >= 2
                    && ui_kit::icon_cell(ui, icons::X, 30.0, false)
                        .tip(tr("Olib tashlash"))
                        .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            l.poll_opts.remove(i);
        }
        if l.poll_opts.len() < 6
            && ui_kit::button(ui, Some(icons::PLUS), tr("Variant qo'shish"), Kind::Ghost).clicked()
        {
            l.poll_opts.push(String::new());
        }
        ui.add_space(8.0);
    }
    let n = online_targets(cx).len();
    let ready = !l.poll_q.trim().is_empty()
        && (l.poll_kind != 0 || l.poll_opts.iter().filter(|o| !o.trim().is_empty()).count() >= 2);
    ui.add_enabled_ui(ready && n > 0, |ui| {
        if ui_kit::button(
            ui,
            Some(icons::SEND),
            &tf!("Yuborish ({n})", n = n),
            Kind::Primary,
        )
        .clicked()
        {
            l.request = Some(LessonRequest::StartPoll);
        }
    });
}

/// The options a poll kind sends.
pub(super) fn poll_options(l: &Lesson) -> Vec<String> {
    match l.poll_kind {
        0 => l
            .poll_opts
            .iter()
            .map(|o| o.trim().to_string())
            .filter(|o| !o.is_empty())
            .collect(),
        1 => vec![tr("Ha").into(), tr("Yo'q").into()],
        3 => vec![
            tr("Juda yaxshi").into(),
            tr("Yaxshi").into(),
            tr("O'rtacha").into(),
            tr("Tushunmadim").into(),
        ],
        _ => vec![],
    }
}

fn poll_results(
    ui: &mut egui::Ui,
    p: &Palette,
    l: &mut Lesson,
    cx: &FormCtx<'_>,
    question: &str,
    options: &[String],
    answers: &[(u64, String, Option<u32>, String)],
) {
    ui.label(
        RichText::new(question)
            .family(ui_kit::bold())
            .size(16.0)
            .color(p.strong),
    );
    ui.add_space(4.0);
    let total = online_targets(cx).len().max(answers.len());
    ui.label(
        RichText::new(tf!("Javob berdi: {a} / {b}", a = answers.len(), b = total))
            .size(12.5)
            .color(p.muted),
    );
    ui.add_space(12.0);
    let w = ui.available_width();
    for (i, o) in options.iter().enumerate() {
        let n = answers.iter().filter(|a| a.2 == Some(i as u32)).count();
        let frac = n as f32 / answers.len().max(1) as f32;
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        ui.painter().rect_filled(r, Rounding::ZERO, p.bg);
        ui.painter().rect_filled(
            Rect::from_min_size(r.min, vec2(r.width() * frac, r.height())),
            Rounding::ZERO,
            p.accent.gamma_multiply(0.35),
        );
        ui.painter().text(
            pos2(r.left() + 12.0, r.center().y),
            Align2::LEFT_CENTER,
            o,
            FontId::new(13.5, ui_kit::medium()),
            p.strong,
        );
        ui.painter().text(
            pos2(r.right() - 12.0, r.center().y),
            Align2::RIGHT_CENTER,
            format!("{n} · {:.0}%", frac * 100.0),
            FontId::new(12.5, ui_kit::mono()),
            p.text,
        );
        ui.add_space(4.0);
    }
    ui.add_space(10.0);
    caption(ui, p, tr("Kim nima javob berdi (birinchi javob tepada)"));
    let rows: Vec<Vec<(String, Color32)>> = answers
        .iter()
        .enumerate()
        .map(|(i, (_, name, choice, text))| {
            let ans = match choice {
                Some(c) => options.get(*c as usize).cloned().unwrap_or_default(),
                None => text.clone(),
            };
            vec![
                (
                    format!("{}", i + 1),
                    if i == 0 { p.accent_ink } else { p.muted },
                ),
                (name.clone(), p.text),
                (ans, p.text),
            ]
        })
        .collect();
    table(
        ui,
        p,
        &[("#", 0.1), (tr("Kompyuter"), 0.35), (tr("Javob"), 0.55)],
        &rows,
    );
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui_kit::button(ui, Some(icons::SQUARE), tr("Yakunlash"), Kind::Outline).clicked() {
            l.request = Some(LessonRequest::EndPoll);
        }
        if ui_kit::button(ui, Some(icons::PLUS), tr("Yangi so'rov"), Kind::Primary).clicked() {
            l.request = Some(LessonRequest::ClearPoll);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Tests

/// Saved tests: <config>/tests/<name>.json.
pub(super) fn tests_dir() -> Option<std::path::PathBuf> {
    crate::config::dir().map(|d| d.join("tests"))
}

pub(super) fn saved_tests() -> Vec<String> {
    let mut v: Vec<String> = tests_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    v.sort();
    v
}

pub(super) fn test(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    let running = cx.hub.lock().unwrap().test.as_ref().map(|t| {
        (
            t.spec.def.title.clone(),
            t.spec.def.questions.len(),
            t.started.elapsed().as_secs(),
            t.sent_to.len(),
            t.results
                .values()
                .map(|(n, _, pts)| (n.clone(), *pts))
                .collect::<Vec<_>>(),
        )
    });
    if let Some((title, nq, secs, sent, mut results)) = running {
        ui.label(
            RichText::new(&title)
                .family(ui_kit::bold())
                .size(16.0)
                .color(p.strong),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(tf!(
                "Topshirdi: {a} / {b} · {m} daqiqa o'tdi",
                a = results.len(),
                b = sent,
                m = secs / 60
            ))
            .size(12.5)
            .color(p.muted),
        );
        ui.add_space(10.0);
        results.sort_by_key(|r| std::cmp::Reverse(r.1));
        let rows: Vec<Vec<(String, Color32)>> = results
            .iter()
            .enumerate()
            .map(|(i, (name, pts))| {
                let pct = *pts as f32 / nq.max(1) as f32 * 100.0;
                let col = if pct >= 80.0 {
                    p.success
                } else if pct >= 50.0 {
                    p.warning
                } else {
                    p.danger
                };
                vec![
                    (format!("{}", i + 1), p.muted),
                    (name.clone(), p.text),
                    (format!("{pts} / {nq}"), p.text),
                    (format!("{pct:.0}%"), col),
                ]
            })
            .collect();
        table(
            ui,
            p,
            &[
                (tr("O'rin"), 0.12),
                (tr("Kompyuter"), 0.43),
                (tr("Ball"), 0.25),
                (tr("Foiz"), 0.2),
            ],
            &rows,
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui_kit::button(
                ui,
                Some(icons::SQUARE),
                tr("Testni yakunlash"),
                Kind::Danger,
            )
            .clicked()
            {
                l.request = Some(LessonRequest::EndTest);
            }
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("CSV eksport"), Kind::Outline).clicked()
            {
                l.request = Some(LessonRequest::ExportTest);
            }
            if ui_kit::button(ui, Some(icons::X), tr("Yopish"), Kind::Ghost).clicked() {
                l.request = Some(LessonRequest::CloseTest);
            }
        });
        return;
    }

    // Saved tests.
    let saved = saved_tests();
    if !saved.is_empty() {
        caption(ui, p, tr("Saqlangan testlar"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for name in &saved {
                let r = ui_kit::button(ui, Some(icons::FILE_TEXT), name, Kind::Outline)
                    .tip(tr("Ochish (o'ng tugma — o'chirish)"));
                if r.clicked() {
                    l.request = Some(LessonRequest::LoadTest(name.clone()));
                }
                if r.secondary_clicked() {
                    l.request = Some(LessonRequest::DeleteTest(name.clone()));
                }
            }
        });
        ui.add_space(12.0);
    }

    let t = &mut l.test;
    caption(ui, p, tr("Test nomi"));
    ui.add(
        ui_kit::text_field(&mut t.def.title)
            .hint_text(tr("Masalan: 5-sinf, Informatika, 3-mavzu"))
            .desired_width(ui.available_width()),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Vaqt")).size(13.0).color(p.text));
        for m in [0u32, 5, 10, 15, 20, 30, 45] {
            let label = if m == 0 {
                tr("Cheksiz").to_string()
            } else {
                format!("{m} min")
            };
            if ui_kit::tab(ui, &label, None, t.def.minutes == m).clicked() {
                t.def.minutes = m;
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui_kit::toggle(ui, &mut t.def.shuffle);
        ui.label(
            RichText::new(tr("Savollar va variantlar har o'quvchida aralash tartibda"))
                .size(13.0)
                .color(p.text),
        );
    });
    ui.add_space(14.0);
    t.correct.resize(t.def.questions.len(), String::new());
    let mut remove_q = None;
    for (qi, q) in t.def.questions.iter_mut().enumerate() {
        let correct = &mut t.correct[qi];
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui_kit::hud(ui, &tf!("{n}-savol", n = qi + 1), p.muted);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui_kit::icon_cell(ui, icons::TRASH_2, 28.0, false)
                            .tip(tr("Savolni o'chirish"))
                            .clicked()
                        {
                            remove_q = Some(qi);
                        }
                        let written = q.options.is_empty();
                        if ui_kit::tab(ui, tr("Matnli"), None, written).clicked() && !written {
                            q.options.clear();
                            correct.clear();
                        }
                        if ui_kit::tab(ui, tr("Variantli"), None, !written).clicked() && written {
                            q.options = vec![String::new(), String::new()];
                            *correct = "0".into();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.add(
                    ui_kit::text_field(&mut q.text)
                        .hint_text(tr("Savol matni"))
                        .desired_width(ui.available_width()),
                );
                ui.add_space(6.0);
                if q.options.is_empty() {
                    ui.add(
                        ui_kit::text_field(correct)
                            .hint_text(tr("To'g'ri javob (bir nechta bo'lsa: Toshkent | Tashkent)"))
                            .desired_width(ui.available_width()),
                    );
                } else {
                    let mut remove_o = None;
                    for (oi, o) in q.options.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let right = correct.trim() == oi.to_string();
                            if ui_kit::icon_cell(
                                ui,
                                if right {
                                    icons::CIRCLE_CHECK
                                } else {
                                    icons::CIRCLE
                                },
                                30.0,
                                right,
                            )
                            .tip(tr("To'g'ri javob deb belgilash"))
                            .clicked()
                            {
                                *correct = oi.to_string();
                            }
                            ui.add(
                                ui_kit::text_field(o)
                                    .hint_text(tr("Variant"))
                                    .desired_width(ui.available_width() - 40.0),
                            );
                            if oi >= 2 && ui_kit::icon_cell(ui, icons::X, 30.0, false).clicked() {
                                remove_o = Some(oi);
                            }
                        });
                    }
                    if let Some(oi) = remove_o {
                        q.options.remove(oi);
                        if correct.trim() == oi.to_string() {
                            *correct = "0".into();
                        }
                    }
                    if q.options.len() < 6
                        && ui_kit::button(
                            ui,
                            Some(icons::PLUS),
                            tr("Variant qo'shish"),
                            Kind::Ghost,
                        )
                        .clicked()
                    {
                        q.options.push(String::new());
                    }
                }
            });
        ui.add_space(8.0);
    }
    if let Some(qi) = remove_q {
        t.def.questions.remove(qi);
        t.correct.remove(qi);
    }
    if ui_kit::button(ui, Some(icons::PLUS), tr("Savol qo'shish"), Kind::Outline).clicked() {
        t.def.questions.push(TestQuestion {
            text: String::new(),
            options: vec![String::new(), String::new()],
        });
        t.correct.push("0".into());
    }
    ui.add_space(14.0);
    let n = online_targets(cx).len();
    let ready = !t.def.title.trim().is_empty()
        && !t.def.questions.is_empty()
        && t.def.questions.iter().all(|q| !q.text.trim().is_empty());
    ui.horizontal(|ui| {
        ui.add_enabled_ui(ready, |ui| {
            if ui_kit::button(ui, Some(icons::CHECK), tr("Saqlash"), Kind::Outline).clicked() {
                l.request = Some(LessonRequest::SaveTest);
            }
        });
        ui.add_enabled_ui(ready && n > 0, |ui| {
            if ui_kit::button(
                ui,
                Some(icons::PLAY),
                &tf!("Boshlash ({n})", n = n),
                Kind::Primary,
            )
            .clicked()
            {
                l.request = Some(LessonRequest::StartTest);
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Timer, goal, rewards

pub(super) fn timer(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    caption(ui, p, tr("Dars maqsadi"));
    ui.add(
        ui_kit::text_field(&mut l.goal)
            .hint_text(tr("Masalan: Python'da sikllar — 3 ta mashq"))
            .desired_width(ui.available_width()),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Taymer")).size(13.0).color(p.text));
        for m in [0u32, 5, 10, 15, 20, 30, 45] {
            let label = if m == 0 {
                tr("Yo'q").to_string()
            } else {
                format!("{m} min")
            };
            if ui_kit::tab(ui, &label, None, l.minutes == m).clicked() {
                l.minutes = m;
            }
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let ok = !l.goal.trim().is_empty() || l.minutes > 0;
        ui.add_enabled_ui(ok, |ui| {
            if ui_kit::button(
                ui,
                Some(icons::PLAY),
                tr("Ekranlarda ko'rsatish"),
                Kind::Primary,
            )
            .clicked()
            {
                l.request = Some(LessonRequest::Banner(true));
            }
        });
        if l.banner_on
            && ui_kit::button(ui, Some(icons::X), tr("Olib tashlash"), Kind::Outline).clicked()
        {
            l.request = Some(LessonRequest::Banner(false));
        }
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(
            "O'quvchi ekranining yuqori o'ng burchagida maqsad va qolgan vaqt ko'rinadi.",
        ))
        .size(12.0)
        .color(p.faint),
    );

    ui.add_space(22.0);
    caption(ui, p, tr("Rag'batlantirish"));
    let stars = cx.hub.lock().unwrap().stars.clone();
    let w = ui.available_width();
    for v in online_targets(cx) {
        let n = stars.get(&v.key).map_or(0, |s| s.1);
        let (r, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
        ui.painter()
            .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, p.line));
        ui.painter().text(
            pos2(r.left() + 4.0, r.center().y),
            Align2::LEFT_CENTER,
            &v.label,
            FontId::new(13.0, ui_kit::bold()),
            p.text,
        );
        ui.painter().text(
            pos2(r.left() + w * 0.45, r.center().y),
            Align2::LEFT_CENTER,
            format!("{} {n}", icons::STAR),
            FontId::proportional(14.0),
            if n > 0 { p.warning } else { p.faint },
        );
        let mut b = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(r.right() - 120.0, r.top()), r.max))
                .layout(Layout::right_to_left(Align::Center)),
        );
        if ui_kit::button(&mut b, Some(icons::STAR), tr("Yulduzcha"), Kind::Ghost)
            .tip(tr("O'quvchiga \"Ofarin!\" xabari boradi"))
            .clicked()
        {
            l.request = Some(LessonRequest::Star(v.id));
        }
    }
}
