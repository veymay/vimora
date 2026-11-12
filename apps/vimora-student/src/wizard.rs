//! `vimora-student setup-wizard <installer>`: Vimora's own setup window, started by the
//! Windows installer instead of its plain pages.
//!
//! Language (with flags) → teacher or student → it runs the installer silently with those
//! choices and shows the progress. Nothing else is asked: the program starts by itself.
//!
//! Exit code: 0 installed, 1 closed by the user, 2 no window could be opened (the installer then
//! shows its own pages). A window is tried with OpenGL first, then Direct3D / Vulkan, each in a
//! process of its own (a failed graphics start can't be retried inside one process).

use crate::smallwin;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke};
use std::process::{Child, Command};
use vimora_ui::{icons, Kind};

const SIZE: [f32; 2] = [440.0, 330.0];
/// Exit code of a window process whose graphics could not start.
const NO_WINDOW: i32 = 3;

pub fn run(installer: String) -> anyhow::Result<()> {
    // The window process itself.
    if std::env::var_os("VIMORA_WIZARD_WINDOW").is_some() {
        let code = match window(installer) {
            Ok(code) => code,
            Err(_) => NO_WINDOW,
        };
        std::process::exit(code);
    }
    let me = std::env::current_exe()?;
    for renderer in ["glow", "wgpu"] {
        let code = Command::new(&me)
            .args(["setup-wizard", &installer])
            .env("VIMORA_WIZARD_WINDOW", "1")
            .env("VIMORA_RENDERER", renderer)
            .status()
            .ok()
            .and_then(|s| s.code())
            .unwrap_or(NO_WINDOW);
        if code != NO_WINDOW {
            std::process::exit(code);
        }
    }
    std::process::exit(2);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Uz,
    Ru,
    En,
}

impl Lang {
    fn code(self) -> &'static str {
        match self {
            Lang::Uz => "uz",
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }
}

/// The window's few texts in the three languages.
fn t(l: Lang, key: &str) -> &'static str {
    match (key, l) {
        ("lang", _) => "Til · Язык · Language",
        ("who", Lang::Uz) => "Qaysi dastur o'rnatilsin?",
        ("who", Lang::Ru) => "Какую программу установить?",
        ("who", Lang::En) => "Which program?",
        ("teacher", Lang::Uz) => "O'qituvchi",
        ("teacher", Lang::Ru) => "Учитель",
        ("teacher", Lang::En) => "Teacher",
        ("teacher_d", Lang::Uz) => "Sinfni boshqarish",
        ("teacher_d", Lang::Ru) => "Управление классом",
        ("teacher_d", Lang::En) => "Runs the classroom",
        ("student", Lang::Uz) => "O'quvchi",
        ("student", Lang::Ru) => "Ученик",
        ("student", Lang::En) => "Student",
        ("student_d", Lang::Uz) => "Sinfdagi kompyuter",
        ("student_d", Lang::Ru) => "Компьютер в классе",
        ("student_d", Lang::En) => "A classroom computer",
        ("back", Lang::Uz) => "Orqaga",
        ("back", Lang::Ru) => "Назад",
        ("back", Lang::En) => "Back",
        ("installing", Lang::Uz) => "O'rnatilmoqda…",
        ("installing", Lang::Ru) => "Установка…",
        ("installing", Lang::En) => "Installing…",
        ("done", Lang::Uz) => "Tayyor",
        ("done", Lang::Ru) => "Готово",
        ("done", Lang::En) => "Done",
        ("done_d", Lang::Uz) => "Vimora o'rnatildi va ishga tushdi.",
        ("done_d", Lang::Ru) => "Vimora установлена и запущена.",
        ("done_d", Lang::En) => "Vimora is installed and running.",
        ("close", Lang::Uz) => "Yopish",
        ("close", Lang::Ru) => "Закрыть",
        ("close", Lang::En) => "Close",
        ("failed", Lang::Uz) => "O'rnatib bo'lmadi",
        ("failed", Lang::Ru) => "Не удалось установить",
        ("failed", Lang::En) => "Could not install",
        ("retry", Lang::Uz) => "Qayta urinish",
        ("retry", Lang::Ru) => "Повторить",
        ("retry", Lang::En) => "Try again",
        _ => "",
    }
}

enum Step {
    Language,
    Role,
    Installing(Child, bool),
    Done,
    Failed(bool),
}

struct Wizard {
    installer: String,
    lang: Lang,
    step: Step,
    exit: i32,
}

fn window(installer: String) -> anyhow::Result<i32> {
    let code = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(1));
    let code2 = code.clone();
    eframe::run_native(
        "vimora-setup",
        smallwin::options("vimora-setup", SIZE, None),
        Box::new(move |cc| {
            smallwin::style(&cc.egui_ctx);
            let mut w = Wizard {
                installer,
                lang: Lang::Uz,
                step: Step::Language,
                exit: 1,
            };
            // Dev aid (screenshots): VIMORA_WIZARD_STEP=role|done.
            match std::env::var("VIMORA_WIZARD_STEP").as_deref() {
                Ok("role") => w.step = Step::Role,
                Ok("done") => w.step = Step::Done,
                _ => {}
            }
            Ok(Box::new(Exit(w, code2)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(code.load(std::sync::atomic::Ordering::Relaxed))
}

/// Hands the exit code out of the window when it closes.
struct Exit(Wizard, std::sync::Arc<std::sync::atomic::AtomicI32>);

impl eframe::App for Exit {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.0.update(ctx, frame);
        self.1
            .store(self.0.exit, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Wizard {
    fn start(&mut self, teacher: bool) {
        let role = if teacher { "teacher" } else { "student" };
        let comp = if teacher { "teacher" } else { "agent" };
        let child = Command::new(&self.installer)
            .args([
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                &format!("/TYPE={role}"),
                &format!("/COMPONENTS={comp}"),
                &format!("/LANG={}", self.lang.code()),
                "/LAUNCH",
            ])
            .spawn();
        self.step = match child {
            Ok(c) => Step::Installing(c, teacher),
            Err(_) => Step::Failed(teacher),
        };
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        smallwin::screenshot(ctx);
        let busy = matches!(self.step, Step::Installing(..));
        if smallwin::title_bar(ctx, "Vimora") && !busy {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // The installer finished?
        if let Step::Installing(child, teacher) = &mut self.step {
            let teacher = *teacher;
            match child.try_wait() {
                Ok(Some(st)) if st.success() => {
                    self.exit = 0;
                    self.step = Step::Done;
                }
                Ok(Some(_)) | Err(_) => self.step = Step::Failed(teacher),
                Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        let p = vimora_ui::palette(ctx);
        let l = self.lang;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(22.0)),
            )
            .show(ctx, |ui| match &self.step {
                Step::Language => {
                    heading(ui, &p, t(l, "lang"));
                    for (lang, name) in [
                        (Lang::Uz, "O'zbekcha"),
                        (Lang::Ru, "Русский"),
                        (Lang::En, "English"),
                    ] {
                        if choice(ui, &p, |painter, r| flag(painter, r, lang), name, None) {
                            self.lang = lang;
                            self.step = Step::Role;
                        }
                        ui.add_space(8.0);
                    }
                }
                Step::Role => {
                    heading(ui, &p, t(l, "who"));
                    let mut picked = None;
                    for (teacher, icon) in [(true, icons::GRADUATION_CAP), (false, icons::MONITOR)]
                    {
                        let (name, desc) = if teacher {
                            (t(l, "teacher"), t(l, "teacher_d"))
                        } else {
                            (t(l, "student"), t(l, "student_d"))
                        };
                        let draw = |painter: &egui::Painter, r: Rect| {
                            painter.text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                icon,
                                FontId::proportional(22.0),
                                p.accent,
                            );
                        };
                        if choice(ui, &p, draw, name, Some(desc)) {
                            picked = Some(teacher);
                        }
                        ui.add_space(8.0);
                    }
                    ui.add_space(4.0);
                    if vimora_ui::button(ui, Some(icons::ARROW_LEFT), t(l, "back"), Kind::Ghost)
                        .clicked()
                    {
                        self.step = Step::Language;
                    }
                    if let Some(teacher) = picked {
                        self.start(teacher);
                    }
                }
                Step::Installing(..) => {
                    ui.add_space(70.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(34.0).color(p.accent));
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new(t(l, "installing"))
                                .family(vimora_ui::bold())
                                .size(16.0)
                                .color(p.strong),
                        );
                    });
                }
                Step::Done => {
                    ui.add_space(46.0);
                    ui.vertical_centered(|ui| {
                        ui.label(vimora_ui::icon(icons::CIRCLE_CHECK, 46.0, p.accent));
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(t(l, "done"))
                                .family(vimora_ui::bold())
                                .size(20.0)
                                .color(p.strong),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(t(l, "done_d"))
                                .size(13.0)
                                .color(p.muted),
                        );
                        ui.add_space(18.0);
                        if vimora_ui::button(ui, Some(icons::CHECK), t(l, "close"), Kind::Primary)
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Step::Failed(teacher) => {
                    let teacher = *teacher;
                    ui.add_space(46.0);
                    ui.vertical_centered(|ui| {
                        ui.label(vimora_ui::icon(icons::TRIANGLE_ALERT, 42.0, p.danger));
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(t(l, "failed"))
                                .family(vimora_ui::bold())
                                .size(18.0)
                                .color(p.strong),
                        );
                        ui.add_space(18.0);
                        if vimora_ui::button(
                            ui,
                            Some(icons::REFRESH_CW),
                            t(l, "retry"),
                            Kind::Primary,
                        )
                        .clicked()
                        {
                            self.start(teacher);
                        }
                    });
                }
            });
    }
}

fn heading(ui: &mut egui::Ui, p: &vimora_ui::Palette, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(vimora_ui::bold())
            .size(17.0)
            .color(p.strong),
    );
    ui.add_space(14.0);
}

/// One big clickable row: a picture on the left (flag / icon), a title and a short line.
fn choice(
    ui: &mut egui::Ui,
    p: &vimora_ui::Palette,
    picture: impl FnOnce(&egui::Painter, Rect),
    title: &str,
    desc: Option<&str>,
) -> bool {
    let h = if desc.is_some() { 64.0 } else { 52.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let hover = resp.hovered();
    ui.painter().rect(
        r,
        0.0,
        if hover { p.hover } else { p.card },
        Stroke::new(1.0_f32, if hover { p.accent } else { p.line }),
    );
    let pic = Rect::from_center_size(pos2(r.left() + 34.0, r.center().y), vec2(36.0, 24.0));
    picture(ui.painter(), pic);
    let x = r.left() + 68.0;
    match desc {
        Some(d) => {
            ui.painter().text(
                pos2(x, r.center().y - 9.0),
                Align2::LEFT_CENTER,
                title,
                FontId::new(15.0, vimora_ui::bold()),
                p.strong,
            );
            ui.painter().text(
                pos2(x, r.center().y + 11.0),
                Align2::LEFT_CENTER,
                d,
                FontId::proportional(12.5),
                p.muted,
            );
        }
        None => {
            ui.painter().text(
                pos2(x, r.center().y),
                Align2::LEFT_CENTER,
                title,
                FontId::new(15.0, vimora_ui::bold()),
                p.strong,
            );
        }
    }
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// The three flags, drawn (no image files): Uzbekistan, Russia, United Kingdom.
fn flag(painter: &egui::Painter, r: Rect, lang: Lang) {
    let painter = painter.with_clip_rect(r);
    let band = |a: f32, b: f32, c: Color32| {
        painter.rect_filled(
            Rect::from_min_max(
                pos2(r.left(), r.top() + r.height() * a),
                pos2(r.right(), r.top() + r.height() * b),
            ),
            0.0,
            c,
        );
    };
    match lang {
        Lang::Uz => {
            let blue = Color32::from_rgb(0x00, 0x99, 0xB5);
            band(0.0, 0.32, blue);
            band(0.32, 0.35, Color32::from_rgb(0xCE, 0x11, 0x26));
            band(0.35, 0.65, Color32::WHITE);
            band(0.65, 0.68, Color32::from_rgb(0xCE, 0x11, 0x26));
            band(0.68, 1.0, Color32::from_rgb(0x1E, 0xB5, 0x3A));
            // Crescent and a row of stars.
            let c = pos2(r.left() + r.width() * 0.16, r.top() + r.height() * 0.16);
            let rad = r.height() * 0.12;
            painter.circle_filled(c, rad, Color32::WHITE);
            painter.circle_filled(c + vec2(rad * 0.42, 0.0), rad * 0.86, blue);
            for i in 0..3 {
                painter.circle_filled(
                    pos2(
                        r.left() + r.width() * (0.34 + i as f32 * 0.09),
                        r.top() + r.height() * 0.16,
                    ),
                    r.height() * 0.035,
                    Color32::WHITE,
                );
            }
        }
        Lang::Ru => {
            band(0.0, 1.0 / 3.0, Color32::WHITE);
            band(1.0 / 3.0, 2.0 / 3.0, Color32::from_rgb(0x00, 0x39, 0xA6));
            band(2.0 / 3.0, 1.0, Color32::from_rgb(0xD5, 0x2B, 0x1E));
        }
        Lang::En => {
            let red = Color32::from_rgb(0xC8, 0x10, 0x2E);
            painter.rect_filled(r, 0.0, Color32::from_rgb(0x01, 0x21, 0x69));
            let h = r.height();
            for (a, b) in [
                (r.left_top(), r.right_bottom()),
                (r.left_bottom(), r.right_top()),
            ] {
                painter.line_segment([a, b], Stroke::new(h * 0.2, Color32::WHITE));
                painter.line_segment([a, b], Stroke::new(h * 0.07, red));
            }
            painter.line_segment(
                [pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())],
                Stroke::new(h * 0.33, Color32::WHITE),
            );
            painter.line_segment(
                [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)],
                Stroke::new(h * 0.33, Color32::WHITE),
            );
            painter.line_segment(
                [pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())],
                Stroke::new(h * 0.2, red),
            );
            painter.line_segment(
                [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)],
                Stroke::new(h * 0.2, red),
            );
        }
    }
    painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_black_alpha(40)));
}
