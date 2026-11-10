//! "Dars": the lesson tools in one drawer, one tab each — files (send / collect assignments),
//! attendance, quick polls, tests, and the lesson timer with goals and rewards.

use super::*;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum LessonTab {
    #[default]
    Files,
    Attendance,
    Poll,
    Test,
    Timer,
}

/// Inline actions of the "Dars" drawer (it stays open).
#[derive(Clone, PartialEq)]
pub(super) enum LessonRequest {
    /// Open a file (false) or folder (true) picker for "send".
    Pick(bool),
    Send,
    Collect,
    OpenCollected,
    StartPoll,
    EndPoll,
    ClearPoll,
    /// Show (true) or remove the lesson banner.
    Banner(bool),
    /// Give a star to this computer.
    Star(u64),
    StartTest,
    EndTest,
    CloseTest,
    SaveTest,
    LoadTest(String),
    DeleteTest(String),
    ExportTest,
    ExportAttendance,
}

#[derive(Clone, Default)]
pub(super) struct Lesson {
    pub(super) tab: LessonTab,
    /// File or folder to send.
    pub(super) path: String,
    pub(super) open_after: bool,
    pub(super) request: Option<LessonRequest>,
    /// A collection round was started (shows "kutilmoqda" until answers arrive).
    pub(super) collecting: bool,
    /// Poll: 0 = choices, 1 = yes / no, 2 = written answer, 3 = lesson feedback.
    pub(super) poll_kind: u8,
    pub(super) poll_q: String,
    pub(super) poll_opts: Vec<String>,
    /// Lesson banner: goal text and minutes of the countdown (0 = no countdown).
    pub(super) goal: String,
    pub(super) minutes: u32,
    pub(super) banner_on: bool,
    /// The test being made.
    pub(super) test: crate::net::TestSpec,
}

pub(super) fn lesson_form(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, ic, label) in [
                (LessonTab::Files, icons::FOLDER, tr("Fayllar")),
                (LessonTab::Attendance, icons::USERS, tr("Davomat")),
                (LessonTab::Poll, icons::MESSAGE_SQUARE_TEXT, tr("So'rov")),
                (LessonTab::Test, icons::FILE_TEXT, tr("Test")),
                (LessonTab::Timer, icons::TIMER, tr("Taymer")),
            ] {
                if ui_kit::tab_icon(ui, Some(ic), label, None, l.tab == t).clicked() {
                    l.tab = t;
                }
            }
        });
    });
    ui.add_space(12.0);
    match l.tab {
        LessonTab::Files => files(ui, p, l, cx),
        LessonTab::Attendance => super::lesson_extra::attendance(ui, p, l, cx),
        LessonTab::Poll => super::lesson_extra::poll(ui, p, l, cx),
        LessonTab::Test => super::lesson_extra::test(ui, p, l, cx),
        LessonTab::Timer => super::lesson_extra::timer(ui, p, l, cx),
    }
}

fn caption(ui: &mut egui::Ui, p: &Palette, text: &str, help: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui_kit::hud(ui, text, p.muted);
        ui_kit::info(ui, help);
    });
    ui.add_space(6.0);
}

fn files(ui: &mut egui::Ui, p: &Palette, l: &mut Lesson, cx: &FormCtx<'_>) {
    let online: Vec<&View> = cx
        .all
        .iter()
        .filter(|v| v.online && cx.targets.contains(&v.id))
        .collect();

    caption(
        ui,
        p,
        tr("Fayl yoki papka yuborish"),
        tr("O'quvchi kompyuterida Yuklanmalar/Vimora papkasiga tushadi. Papka ichidagi hamma narsa bilan yuboriladi (jami 100 MB gacha)."),
    );
    let w = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(w, ui_kit::CONTROL_H),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui_kit::button(ui, Some(icons::FOLDER), tr("Papka"), Kind::Outline).clicked() {
                l.request = Some(LessonRequest::Pick(true));
            }
            if ui_kit::button(ui, Some(icons::FILE_UP), tr("Fayl"), Kind::Outline).clicked() {
                l.request = Some(LessonRequest::Pick(false));
            }
            ui.add(
                ui_kit::text_field(&mut l.path)
                    .hint_text(tr("Fayl yoki papka yo'li"))
                    .desired_width(ui.available_width()),
            );
        },
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui_kit::toggle(ui, &mut l.open_after);
        ui.label(
            RichText::new(tr("Yuborilgach o'quvchida ochilsin"))
                .size(13.0)
                .color(p.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ok = !l.path.trim().is_empty() && !online.is_empty();
            ui.add_enabled_ui(ok, |ui| {
                if ui_kit::button(
                    ui,
                    Some(icons::SEND),
                    &tf!("Yuborish ({n})", n = online.len()),
                    Kind::Primary,
                )
                .clicked()
                {
                    l.request = Some(LessonRequest::Send);
                }
            });
        });
    });

    ui.add_space(22.0);
    caption(
        ui,
        p,
        tr("Topshiriqlarni yig'ish"),
        tr("O'quvchilar ishini Hujjatlar → Topshiriq papkasiga saqlaydi. \"Yig'ish\" bosilganda hammasi sizning Hujjatlar/Vimora/Topshiriqlar papkangizga kompyuter nomi bilan tushadi."),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_enabled_ui(!online.is_empty(), |ui| {
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("Yig'ish"), Kind::Primary).clicked() {
                l.request = Some(LessonRequest::Collect);
            }
        });
        if ui_kit::button(ui, Some(icons::FOLDER), tr("Papkani ochish"), Kind::Outline).clicked() {
            l.request = Some(LessonRequest::OpenCollected);
        }
    });
    ui.add_space(10.0);
    let collected = cx.hub.lock().unwrap().collected.clone();
    let rows: Vec<(String, String, Color32)> = online
        .iter()
        .map(|v| match collected.get(&v.id) {
            Some(0) => (v.label.clone(), tr("Papka bo'sh").to_string(), p.warning),
            Some(n) => (
                v.label.clone(),
                tf!("Topshirdi · {n} ta fayl", n = *n),
                p.success,
            ),
            None if l.collecting => (v.label.clone(), tr("Kutilmoqda…").to_string(), p.muted),
            None => (v.label.clone(), "—".to_string(), p.faint),
        })
        .collect();
    simple_table(ui, p, &[tr("Kompyuter"), tr("Holat")], &rows);
}

/// Two-column table: name | state (coloured).
pub(super) fn simple_table(
    ui: &mut egui::Ui,
    p: &Palette,
    head: &[&str; 2],
    rows: &[(String, String, Color32)],
) {
    let w = ui.available_width();
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            ui.painter().rect_filled(hr, Rounding::ZERO, p.bg);
            for (x, t) in [(12.0, head[0]), (w * 0.45, head[1])] {
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
                    tr("Onlayn kompyuter yo'q"),
                    FontId::proportional(12.5),
                    p.faint,
                );
            }
            for (name, state, col) in rows {
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                let n = ui_kit::truncated(
                    ui,
                    name,
                    FontId::new(13.0, ui_kit::bold()),
                    p.text,
                    w * 0.45 - 24.0,
                );
                ui.painter().galley(
                    pos2(r.left() + 12.0, r.center().y - n.size().y / 2.0),
                    n,
                    p.text,
                );
                let s =
                    ui_kit::truncated(ui, state, FontId::proportional(12.5), *col, w * 0.55 - 12.0);
                ui.painter().galley(
                    pos2(r.left() + w * 0.45, r.center().y - s.size().y / 2.0),
                    s,
                    *col,
                );
            }
        });
}

impl ConsoleApp {
    /// Runs what the "Dars" drawer asked for this frame.
    pub(super) fn lesson_requests(&mut self, targets: &[u64]) {
        let Some(Dialog::Lesson(l)) = &mut self.dialog else {
            return;
        };
        let Some(req) = l.request.take() else { return };
        match req {
            LessonRequest::Pick(folder) => {
                if self.file_job.is_none() {
                    self.file_job = Some(super::overlay::spawn_file_job(move || {
                        let d = rfd::FileDialog::new();
                        let picked = if folder {
                            d.pick_folder()
                        } else {
                            d.pick_file()
                        }?;
                        Some(FileResult::LessonPath(
                            picked.to_string_lossy().into_owned(),
                        ))
                    }));
                }
            }
            LessonRequest::Send => {
                let path = std::path::PathBuf::from(l.path.trim().trim_matches('"'));
                let open = l.open_after;
                self.send_path(targets, &path, open);
            }
            LessonRequest::Collect => {
                l.collecting = true;
                self.hub.lock().unwrap().collected.clear();
                self.send(
                    targets,
                    Command::CollectFiles,
                    tr("Topshiriqlar yig'ilmoqda"),
                );
            }
            LessonRequest::OpenCollected => {
                let dir = crate::net::collected_root();
                let _ = std::fs::create_dir_all(&dir);
                config::open(&dir.to_string_lossy());
            }
            LessonRequest::StartPoll => {
                let id = fresh_id();
                let options = super::lesson_extra::poll_options(l);
                let question = l.poll_q.trim().to_string();
                self.hub.lock().unwrap().poll = Some(crate::net::PollState {
                    id,
                    question: question.clone(),
                    options: options.clone(),
                    answers: vec![],
                });
                self.send(
                    targets,
                    Command::Poll {
                        id,
                        question,
                        options,
                    },
                    tr("So'rov yuborildi"),
                );
            }
            LessonRequest::EndPoll | LessonRequest::ClearPoll => {
                let all: Vec<u64> = self.hub.lock().unwrap().students.keys().copied().collect();
                self.send(&all, Command::PollEnd, "");
                if req == LessonRequest::ClearPoll {
                    self.hub.lock().unwrap().poll = None;
                }
            }
            LessonRequest::Banner(on) => {
                let end_unix = if on && l.minutes > 0 {
                    chrono::Local::now().timestamp() as u64 + l.minutes as u64 * 60
                } else {
                    0
                };
                let goal = if on {
                    l.goal.trim().to_string()
                } else {
                    String::new()
                };
                l.banner_on = on;
                self.send(targets, Command::LessonBanner { goal, end_unix }, "");
            }
            LessonRequest::Star(id) => {
                let total = {
                    let mut h = self.hub.lock().unwrap();
                    let Some(s) = h.students.get(&id) else { return };
                    let (key, name) = (s.info.key(), s.info.label().to_string());
                    let e = h.stars.entry(key).or_insert((name, 0));
                    e.1 += 1;
                    e.1
                };
                self.send(
                    &[id],
                    Command::Message {
                        title: tr("Ofarin!").to_string(),
                        text: tf!(
                            "O'qituvchi sizni rag'batlantirdi: +1 yulduzcha. Bugun jami: {n}",
                            n = total
                        ),
                    },
                    "",
                );
            }
            LessonRequest::StartTest => {
                let mut spec = l.test.clone();
                spec.def.id = fresh_id();
                spec.correct.resize(spec.def.questions.len(), String::new());
                let def = spec.def.clone();
                self.hub.lock().unwrap().test = Some(crate::net::TestRun {
                    spec,
                    started: Instant::now(),
                    results: Default::default(),
                    sent_to: targets.to_vec(),
                });
                self.send(
                    targets,
                    Command::StartTest(def.clone()),
                    &tf!("Test boshlandi: {t}", t = def.title),
                );
            }
            LessonRequest::EndTest | LessonRequest::CloseTest => {
                let sent = self
                    .hub
                    .lock()
                    .unwrap()
                    .test
                    .as_ref()
                    .map(|t| t.sent_to.clone())
                    .unwrap_or_default();
                self.send(&sent, Command::EndTest, "");
                if req == LessonRequest::CloseTest {
                    self.hub.lock().unwrap().test = None;
                }
            }
            LessonRequest::SaveTest => {
                let name = file_safe(&l.test.def.title);
                let res = super::lesson_extra::tests_dir()
                    .ok_or_else(|| "config".to_string())
                    .and_then(|d| {
                        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
                        let j = serde_json::to_string_pretty(&l.test).map_err(|e| e.to_string())?;
                        std::fs::write(d.join(format!("{name}.json")), j).map_err(|e| e.to_string())
                    });
                let mut h = self.hub.lock().unwrap();
                match res {
                    Ok(()) => h.toast(true, tf!("Test saqlandi: {name}", name = name)),
                    Err(e) => h.toast(false, tf!("Saqlab bo'lmadi: {e}", e = e)),
                }
            }
            LessonRequest::LoadTest(name) => {
                if let Some(t) = super::lesson_extra::tests_dir()
                    .and_then(|d| std::fs::read_to_string(d.join(format!("{name}.json"))).ok())
                    .and_then(|j| serde_json::from_str(&j).ok())
                {
                    l.test = t;
                }
            }
            LessonRequest::DeleteTest(name) => {
                self.ask(
                    tr("Test o'chirilsinmi?"),
                    tf!(
                        "\"{name}\" testi butunlay o'chiriladi.",
                        name = name.clone()
                    ),
                    tr("O'chirish"),
                    ConfirmAction::DeleteTest(name),
                );
            }
            LessonRequest::ExportTest => self.export_test(),
            LessonRequest::ExportAttendance => self.export_attendance(),
        }
    }

    /// Test results as CSV in Documents/Vimora/Testlar.
    fn export_test(&mut self) {
        let csv = {
            let h = self.hub.lock().unwrap();
            let Some(t) = &h.test else { return };
            let qs = &t.spec.def.questions;
            let mut rows: Vec<_> = t.results.values().collect();
            rows.sort_by_key(|r| std::cmp::Reverse(r.2));
            let mut out = format!(
                "{};{};{};{}",
                tr("O'rin"),
                tr("Kompyuter"),
                tr("Ball"),
                tr("Foiz")
            );
            for i in 0..qs.len() {
                out += &format!(";{}", tf!("{n}-savol", n = i + 1));
            }
            out.push('\n');
            for (i, (name, answers, pts)) in rows.iter().enumerate() {
                let pct = *pts as f32 / qs.len().max(1) as f32 * 100.0;
                out += &format!("{};{};{};{:.0}%", i + 1, csv_cell(name), pts, pct);
                for (q, question) in qs.iter().enumerate() {
                    let a = answers.iter().find(|a| a.question as usize == q);
                    let text = match a {
                        Some(a) => match a.choice {
                            Some(c) => question
                                .options
                                .get(c as usize)
                                .cloned()
                                .unwrap_or_default(),
                            None => a.text.clone(),
                        },
                        None => String::new(),
                    };
                    out += &format!(";{}", csv_cell(&text));
                }
                out.push('\n');
            }
            (file_safe(&t.spec.def.title), out)
        };
        self.write_export("Testlar", &csv.0, csv.1);
    }

    /// Today's attendance as CSV in Documents/Vimora/Davomat.
    fn export_attendance(&mut self) {
        let (day, att) = self.hub.lock().unwrap().attendance.clone();
        let mut out = format!(
            "{};{};{}\n",
            tr("Kompyuter"),
            tr("O'quvchi"),
            tr("Kelgan vaqti")
        );
        let mut rows: Vec<_> = att.values().collect();
        rows.sort_by(|a, b| a.2.cmp(&b.2));
        for (pc, user, time) in rows {
            out += &format!("{};{};{}\n", csv_cell(pc), csv_cell(user), time);
        }
        let name = if day.is_empty() {
            "davomat".to_string()
        } else {
            day
        };
        self.write_export("Davomat", &name, out);
    }

    fn write_export(&mut self, folder: &str, name: &str, text: String) {
        let dir = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("Vimora")
            .join(folder);
        let path = dir.join(format!(
            "{name}-{}.csv",
            chrono::Local::now().format("%Y%m%d-%H%M")
        ));
        // UTF-8 BOM: Excel opens Uzbek / Russian letters correctly.
        let res = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&path, format!("\u{feff}{text}")));
        let mut h = self.hub.lock().unwrap();
        match res {
            Ok(()) => {
                h.toast(
                    true,
                    tf!("Saqlandi: {path}", path = path.display().to_string()),
                );
                drop(h);
                config::open(&dir.to_string_lossy());
            }
            Err(e) => h.toast(false, tf!("Saqlab bo'lmadi: {e}", e = e.to_string())),
        }
    }

    /// Sends a file, or a folder with everything in it (100 MB at most in all).
    fn send_path(&mut self, targets: &[u64], path: &std::path::Path, open: bool) {
        const LIMIT: u64 = 100 * 1024 * 1024;
        let Some(top) = path.file_name().map(|s| s.to_string_lossy().into_owned()) else {
            return;
        };
        let mut files: Vec<(String, std::path::PathBuf)> = vec![];
        if path.is_dir() {
            let mut stack = vec![path.to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if let Ok(rel) = p.strip_prefix(path) {
                        let rel = rel.to_string_lossy().replace('\\', "/");
                        files.push((format!("{top}/{rel}"), p));
                    }
                }
            }
        } else {
            files.push((top.clone(), path.to_path_buf()));
        }
        let total: u64 = files
            .iter()
            .filter_map(|(_, p)| std::fs::metadata(p).ok().map(|m| m.len()))
            .sum();
        if files.is_empty() || total > LIMIT {
            let msg = if files.is_empty() {
                tr("Yuboriladigan fayl topilmadi")
            } else {
                tr("Jami 100 MB dan katta")
            };
            self.hub.lock().unwrap().toast(false, msg);
            return;
        }
        let n = files.len();
        for (name, p) in files {
            match std::fs::read(&p) {
                Ok(data) => self.send(targets, Command::SendFile { name, data }, ""),
                Err(e) => {
                    self.hub
                        .lock()
                        .unwrap()
                        .log(Level::Warn, tf!("Faylni o'qib bo'lmadi: {e}", e = e));
                }
            }
        }
        if open {
            self.send(targets, Command::OpenFile(top.clone()), "");
        }
        self.hub
            .lock()
            .unwrap()
            .toast(true, tf!("{top}: {n} ta fayl yuborildi", top = top, n = n));
    }
}

/// A new id for a poll / test (seconds and nanoseconds mixed: unique enough for one console).
fn fresh_id() -> u32 {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (n.as_secs() as u32).wrapping_mul(1_000_003) ^ n.subsec_nanos()
}

/// A name usable as a file name.
pub(super) fn file_safe(s: &str) -> String {
    let t: String = s
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if t.trim().is_empty() {
        "test".into()
    } else {
        t
    }
}

/// One CSV cell (";"-separated, quoted when needed).
fn csv_cell(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
