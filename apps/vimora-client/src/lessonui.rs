//! Lesson windows on the student PC, each its own process driven by the agent:
//! * `poll`   — a quick question: option buttons or a text box; the answer is printed to stdout;
//! * `banner` — the lesson goal and a countdown at the top of the screen;
//! * `test`   — the teacher's test, fullscreen and always on top; answers are printed to stdout
//!   when handed in (by the student, when time is up, or when the agent writes "end").
//!
//! Poll and test read their content as one JSON line from stdin.

use eframe::egui::{self, vec2, Align, Color32, FontId, Layout, RichText};
use serde::{Deserialize, Serialize};
use std::io::BufRead;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vimora_core::{TestAnswer, TestDef};
use vimora_ui::{icons, ThemeMode};

#[derive(Serialize, Deserialize)]
pub struct PollIn {
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PollOut {
    pub choice: Option<u32>,
    pub text: String,
}

/// stdin after the first line: "end" closes / hands in.
fn stdin_lines() -> (String, mpsc::Receiver<String>) {
    let mut first = String::new();
    let stdin = std::io::stdin();
    let _ = stdin.lock().read_line(&mut first);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (first, rx)
}

fn print_json<T: Serialize>(v: &T) {
    if let Ok(j) = serde_json::to_string(v) {
        println!("{j}");
    }
}

fn setup(cc: &eframe::CreationContext<'_>) {
    vimora_ui::install_fonts(&cc.egui_ctx);
    vimora_ui::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
}

// ---------------------------------------------------------------------------------------------
// Poll

pub fn run_poll() -> anyhow::Result<()> {
    let (first, rx) = stdin_lines();
    let input: PollIn = serde_json::from_str(first.trim())?;
    let h = 210.0 + 46.0 * input.options.len().max(2) as f32;
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora — savol")
            .with_inner_size([460.0, h])
            .with_always_on_top()
            .with_resizable(false),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-poll",
        opts,
        Box::new(move |cc| {
            setup(cc);
            Ok(Box::new(Poll {
                input,
                text: String::new(),
                rx,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("poll: {e}"))
}

struct Poll {
    input: PollIn,
    text: String,
    rx: mpsc::Receiver<String>,
}

impl eframe::App for Poll {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        if self.rx.try_recv().is_ok_and(|l| l.trim() == "end") {
            std::process::exit(0);
        }
        ctx.request_repaint_after(Duration::from_millis(300));
        let p = vimora_ui::DARK;
        let mut answer: Option<PollOut> = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.card).inner_margin(24.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(icons::MESSAGE_SQUARE_TEXT)
                        .size(26.0)
                        .color(p.accent),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(&self.input.question)
                        .family(vimora_ui::bold())
                        .size(18.0)
                        .color(p.strong),
                );
                ui.add_space(16.0);
                if self.input.options.is_empty() {
                    ui.add(
                        vimora_ui::text_area(&mut self.text)
                            .hint_text("Javobingiz…")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(12.0);
                    if vimora_ui::primary_button(ui, icons::SEND, "Yuborish").clicked()
                        && !self.text.trim().is_empty()
                    {
                        answer = Some(PollOut {
                            choice: None,
                            text: self.text.trim().to_string(),
                        });
                    }
                } else {
                    for (i, o) in self.input.options.iter().enumerate() {
                        let b = egui::Button::new(RichText::new(o).size(15.0).color(p.strong))
                            .min_size(vec2(ui.available_width(), 38.0))
                            .rounding(0.0);
                        if ui.add(b).clicked() {
                            answer = Some(PollOut {
                                choice: Some(i as u32),
                                text: o.clone(),
                            });
                        }
                        ui.add_space(6.0);
                    }
                }
            });
        if let Some(a) = answer {
            print_json(&a);
            std::process::exit(0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Banner

pub fn run_banner(goal: String, end_unix: u64) -> anyhow::Result<()> {
    const W: f32 = 420.0;
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_resizable(false)
            .with_inner_size([W, 52.0])
            .with_position([0.0, 0.0]),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-banner",
        opts,
        Box::new(move |cc| {
            setup(cc);
            Ok(Box::new(Banner {
                goal,
                end_unix,
                placed: false,
                w: W,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("banner: {e}"))
}

struct Banner {
    goal: String,
    end_unix: u64,
    placed: bool,
    w: f32,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl eframe::App for Banner {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        if !self.placed {
            if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    m.x - self.w - 12.0,
                    12.0,
                )));
                self.placed = true;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        let p = vimora_ui::DARK;
        let left = (self.end_unix > 0).then(|| self.end_unix.saturating_sub(now_unix()));
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().rect(
                    r,
                    egui::Rounding::ZERO,
                    Color32::from_black_alpha(225),
                    egui::Stroke::new(1.0_f32, p.accent),
                );
                let mut x = r.left() + 14.0;
                if let Some(s) = left {
                    let txt = format!("{:02}:{:02}", s / 60, s % 60);
                    let col = if s == 0 {
                        p.danger
                    } else if s < 60 {
                        p.warning
                    } else {
                        p.accent
                    };
                    ui.painter().text(
                        egui::pos2(x, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        txt,
                        FontId::new(22.0, vimora_ui::bold()),
                        col,
                    );
                    x += 86.0;
                }
                let g = vimora_ui::truncated(
                    ui,
                    &self.goal,
                    FontId::proportional(14.0),
                    Color32::WHITE,
                    r.right() - x - 12.0,
                );
                ui.painter().galley(
                    egui::pos2(x, r.center().y - g.size().y / 2.0),
                    g,
                    Color32::WHITE,
                );
            });
    }
}

// ---------------------------------------------------------------------------------------------
// Test

pub fn run_test() -> anyhow::Result<()> {
    let (first, rx) = stdin_lines();
    let def: TestDef = serde_json::from_str(first.trim())?;
    // The student's own order of questions and options (the answers keep the teacher's order).
    let mut order: Vec<usize> = (0..def.questions.len()).collect();
    let mut opt_order: Vec<Vec<usize>> = def
        .questions
        .iter()
        .map(|q| (0..q.options.len()).collect())
        .collect();
    if def.shuffle {
        let mut seed = now_unix() ^ std::process::id() as u64;
        let mut rnd = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n.max(1) as u64) as usize
        };
        for i in (1..order.len()).rev() {
            order.swap(i, rnd(i + 1));
        }
        for o in &mut opt_order {
            for i in (1..o.len()).rev() {
                o.swap(i, rnd(i + 1));
            }
        }
    }
    let n = def.questions.len();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vimora — test")
            .with_fullscreen(true)
            .with_always_on_top()
            .with_decorations(false),
        renderer: crate::ui_renderer(),
        ..Default::default()
    };
    eframe::run_native(
        "vimora-test",
        opts,
        Box::new(move |cc| {
            setup(cc);
            Ok(Box::new(Test {
                started: Instant::now(),
                def,
                order,
                opt_order,
                choice: vec![None; n],
                text: vec![String::new(); n],
                page: 0,
                rx,
                last_focus: Instant::now(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("test: {e}"))
}

struct Test {
    def: TestDef,
    started: Instant,
    order: Vec<usize>,
    opt_order: Vec<Vec<usize>>,
    /// Indexed by the teacher's question order; values are the teacher's option index.
    choice: Vec<Option<u32>>,
    text: Vec<String>,
    page: usize,
    rx: mpsc::Receiver<String>,
    last_focus: Instant,
}

impl Test {
    fn hand_in(&self) -> ! {
        let answers: Vec<TestAnswer> = (0..self.def.questions.len())
            .map(|q| TestAnswer {
                question: q as u32,
                choice: self.choice[q],
                text: self.text[q].trim().to_string(),
            })
            .collect();
        print_json(&answers);
        std::process::exit(0);
    }
}

impl eframe::App for Test {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        if self.rx.try_recv().is_ok_and(|l| l.trim() == "end") {
            self.hand_in();
        }
        // Exam mode: stay in front.
        if self.last_focus.elapsed() > Duration::from_millis(800) {
            self.last_focus = Instant::now();
            if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        let left = (self.def.minutes > 0).then(|| {
            (self.def.minutes as u64 * 60).saturating_sub(self.started.elapsed().as_secs())
        });
        if left == Some(0) {
            self.hand_in();
        }
        let p = vimora_ui::DARK;
        let n = self.order.len();
        let mut submit = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let col_w = 760.0f32.min(full.width() - 48.0);
                let rect =
                    egui::Rect::from_center_size(full.center(), vec2(col_w, full.height() - 48.0));
                let mut ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.def.title)
                            .family(vimora_ui::bold())
                            .size(20.0)
                            .color(p.strong),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(s) = left {
                            let col = if s < 60 { p.danger } else { p.accent };
                            ui.label(
                                RichText::new(format!(
                                    "{} {:02}:{:02}",
                                    icons::TIMER,
                                    s / 60,
                                    s % 60
                                ))
                                .size(18.0)
                                .color(col),
                            );
                        }
                    });
                });
                ui.add_space(6.0);
                let answered = (0..n)
                    .filter(|q| self.choice[*q].is_some() || !self.text[*q].trim().is_empty())
                    .count();
                ui.add(
                    egui::ProgressBar::new(answered as f32 / n.max(1) as f32)
                        .desired_height(6.0)
                        .fill(p.accent),
                );
                ui.add_space(18.0);
                if n == 0 {
                    return;
                }
                let qi = self.order[self.page];
                let q = &self.def.questions[qi];
                ui.label(
                    RichText::new(format!("{} / {n}", self.page + 1))
                        .size(13.0)
                        .color(p.muted),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(&q.text).size(19.0).color(p.strong));
                ui.add_space(18.0);
                if q.options.is_empty() {
                    ui.add(
                        vimora_ui::text_area(&mut self.text[qi])
                            .hint_text("Javobingiz…")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                } else {
                    for &oi in &self.opt_order[qi] {
                        let on = self.choice[qi] == Some(oi as u32);
                        let label = format!(
                            "{}  {}",
                            if on {
                                icons::CIRCLE_CHECK
                            } else {
                                icons::CIRCLE_DOT
                            },
                            q.options[oi]
                        );
                        let b = egui::Button::new(RichText::new(label).size(16.0).color(if on {
                            p.on_accent
                        } else {
                            p.strong
                        }))
                        .fill(if on { p.accent } else { p.card })
                        .min_size(vec2(ui.available_width(), 44.0))
                        .rounding(0.0);
                        if ui.add(b).clicked() {
                            self.choice[qi] = Some(oi as u32);
                        }
                        ui.add_space(8.0);
                    }
                }
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    if self.page > 0
                        && vimora_ui::button(
                            ui,
                            Some(icons::CHEVRON_LEFT),
                            "Oldingi",
                            vimora_ui::Kind::Outline,
                        )
                        .clicked()
                    {
                        self.page -= 1;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.page + 1 < n {
                            if vimora_ui::primary_button(ui, icons::CHEVRON_RIGHT, "Keyingi")
                                .clicked()
                            {
                                self.page += 1;
                            }
                        } else if vimora_ui::primary_button(ui, icons::CHECK, "Topshirish")
                            .clicked()
                        {
                            submit = true;
                        }
                    });
                });
            });
        if submit {
            self.hand_in();
        }
    }
}
