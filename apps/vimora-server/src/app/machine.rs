//! "Kompyuter holati": one computer's technical state, Task Manager style — load, memory,
//! disks, battery, network, and every running process with its PID (and "end task").
//!
//! The agent reports on request; the console asks every [`POLL`] while this drawer is open.

use super::*;
use vimora_core::{ProcInfo, SystemInfo};
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

/// How often the open drawer asks for a fresh report.
pub(super) const POLL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MachineTab {
    Overview,
    Processes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcSort {
    Name,
    Pid,
    Cpu,
    Mem,
}

pub(super) struct MachineForm<'a> {
    pub(super) id: u64,
    pub(super) tab: &'a mut MachineTab,
    pub(super) filter: &'a mut String,
    pub(super) sort: &'a mut ProcSort,
    /// "End task" pressed: (pid, name), asked for confirmation by the app.
    pub(super) kill: &'a mut Option<(u32, String)>,
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1_073_741_824.0)
}

fn mb(b: u64) -> String {
    let m = b as f64 / 1_048_576.0;
    if m >= 1024.0 {
        format!("{:.1} GB", m / 1024.0)
    } else {
        format!("{m:.0} MB")
    }
}

fn rate(b: u64) -> String {
    let k = b as f64 / 1024.0;
    if k >= 1024.0 {
        format!("{:.1} MB/s", k / 1024.0)
    } else {
        format!("{k:.0} KB/s")
    }
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        tf!("{d} kun {h} soat", d = d, h = h)
    } else if h > 0 {
        tf!("{h} soat {m} daqiqa", h = h, m = m)
    } else {
        tf!("{m} daqiqa", m = m)
    }
}

/// Green → amber → red as a load rises.
fn load_color(p: &Palette, pct: f32) -> Color32 {
    if pct >= 90.0 {
        p.danger
    } else if pct >= 70.0 {
        p.warning
    } else {
        p.accent
    }
}

/// A tile: icon + caption, a big value, a bar (optional) and a detail line.
#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut egui::Ui,
    p: &Palette,
    w: f32,
    icon: &str,
    caption: &str,
    value: &str,
    bar: Option<(f32, Color32)>,
    detail: &str,
) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 112.0), Sense::hover());
    ui.painter()
        .rect(r, Rounding::ZERO, p.bg, Stroke::new(1.0_f32, p.line));
    let x = r.left() + 16.0;
    ui.painter().text(
        pos2(x, r.top() + 22.0),
        Align2::LEFT_CENTER,
        icon,
        FontId::proportional(16.0),
        p.muted,
    );
    let cap = ui_kit::caps(ui, caption, 9.5, ui_kit::mono(), p.muted);
    ui.painter().galley(
        pos2(x + 24.0, r.top() + 22.0 - cap.size().y / 2.0),
        cap,
        p.muted,
    );
    let v = ui_kit::truncated(
        ui,
        value,
        FontId::new(22.0, ui_kit::bold()),
        p.strong,
        w - 32.0,
    );
    ui.painter().galley(pos2(x, r.top() + 38.0), v, p.strong);
    if let Some((frac, col)) = bar {
        let b = Rect::from_min_size(pos2(x, r.top() + 72.0), vec2(w - 32.0, 6.0));
        ui.painter().rect_filled(b, Rounding::ZERO, p.sunken);
        ui.painter().rect_filled(
            Rect::from_min_size(b.min, vec2(b.width() * frac.clamp(0.0, 1.0), 6.0)),
            Rounding::ZERO,
            col,
        );
    }
    let d = ui_kit::truncated(ui, detail, FontId::proportional(12.0), p.muted, w - 32.0);
    ui.painter()
        .galley(pos2(x, r.bottom() - 14.0 - d.size().y), d, p.muted);
}

pub(super) fn machine_form(ui: &mut egui::Ui, p: &Palette, f: MachineForm<'_>, cx: &FormCtx<'_>) {
    let report: Option<(SystemInfo, f32)> =
        cx.hub.lock().unwrap().students.get(&f.id).and_then(|s| {
            s.sysinfo
                .as_ref()
                .map(|(i, t)| (i.clone(), t.elapsed().as_secs_f32()))
        });
    let online = cx.all.iter().any(|v| v.id == f.id && v.online);
    let n_procs = report.as_ref().map(|(i, _)| i.processes.len());

    ui_kit::segmented(p).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (t, ic, label, n) in [
                (MachineTab::Overview, icons::GAUGE, tr("Umumiy holat"), None),
                (
                    MachineTab::Processes,
                    icons::LIST,
                    tr("Jarayonlar"),
                    n_procs,
                ),
            ] {
                if ui_kit::tab_icon(ui, Some(ic), label, n, *f.tab == t).clicked() {
                    *f.tab = t;
                }
            }
        });
    });
    ui.add_space(12.0);

    let Some((info, age)) = report else {
        let text = if online {
            tr("Ma'lumot olinmoqda…")
        } else {
            tr("Kompyuter oflayn")
        };
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            if online {
                ui.spinner();
                ui.add_space(8.0);
            }
            ui.label(RichText::new(text).size(13.0).color(p.muted));
        });
        return;
    };
    if !online || age > 10.0 {
        ui.label(
            RichText::new(with_icon(
                icons::TRIANGLE_ALERT,
                tr("Ma'lumot eskirgan: kompyuter javob bermayapti"),
            ))
            .size(12.5)
            .color(p.warning),
        );
        ui.add_space(8.0);
    }

    match f.tab {
        MachineTab::Overview => overview(ui, p, &info),
        MachineTab::Processes => processes(ui, p, &info, f),
    }
}

fn overview(ui: &mut egui::Ui, p: &Palette, s: &SystemInfo) {
    let gap = 10.0;
    let w = (ui.available_width() - gap) / 2.0;
    ui.spacing_mut().item_spacing = vec2(gap, gap);
    let ram = if s.ram_total > 0 {
        s.ram_used as f32 / s.ram_total as f32
    } else {
        0.0
    };
    ui.horizontal(|ui| {
        tile(
            ui,
            p,
            w,
            icons::CPU,
            tr("Protsessor"),
            &format!("{:.0}%", s.cpu),
            Some((s.cpu / 100.0, load_color(p, s.cpu))),
            &tf!("{name} · {n} yadro", name = s.cpu_name.clone(), n = s.cores),
        );
        tile(
            ui,
            p,
            w,
            icons::MEMORY,
            tr("Xotira (RAM)"),
            &format!("{:.0}%", ram * 100.0),
            Some((ram, load_color(p, ram * 100.0))),
            &format!("{} / {}", gb(s.ram_used), gb(s.ram_total)),
        );
    });
    ui.horizontal(|ui| {
        match s.battery {
            Some(b) => {
                let icon = if b.charging {
                    icons::BATTERY_CHARGING
                } else if b.percent <= 20 {
                    icons::BATTERY_LOW
                } else {
                    icons::BATTERY
                };
                let col = if b.percent <= 15 && !b.plugged {
                    p.danger
                } else if b.percent <= 30 && !b.plugged {
                    p.warning
                } else {
                    p.accent
                };
                let state = if b.charging {
                    tr("Zaryadlanmoqda").to_string()
                } else if b.plugged {
                    tr("Tokka ulangan").to_string()
                } else if let Some(left) = b.secs_left {
                    tf!("Batareyada · {t} qoldi", t = duration(left as u64))
                } else {
                    tr("Batareyada").to_string()
                };
                tile(
                    ui,
                    p,
                    w,
                    icon,
                    tr("Batareya"),
                    &format!("{}%", b.percent),
                    Some((b.percent as f32 / 100.0, col)),
                    &state,
                );
            }
            None => tile(
                ui,
                p,
                w,
                icons::BATTERY,
                tr("Batareya"),
                tr("Yo'q"),
                None,
                tr("Statsionar kompyuter: tokdan ishlaydi"),
            ),
        }
        tile(
            ui,
            p,
            w,
            icons::ACTIVITY,
            tr("Tarmoq"),
            &format!("{} {}", icons::ARROW_DOWN, rate(s.net_rx)),
            None,
            &tf!("Yuborish: {r}", r = rate(s.net_tx)),
        );
    });

    ui.add_space(4.0);
    ui_kit::hud(ui, tr("Disklar"), p.muted);
    for d in &s.disks {
        let frac = if d.total > 0 {
            d.used as f32 / d.total as f32
        } else {
            0.0
        };
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        let y = r.center().y;
        ui.painter().text(
            pos2(r.left(), y),
            Align2::LEFT_CENTER,
            with_icon(icons::HARD_DRIVE, &d.mount),
            FontId::new(13.0, ui_kit::medium()),
            p.text,
        );
        let bar = Rect::from_min_size(
            pos2(r.left() + 110.0, y - 3.0),
            vec2((w - 360.0).max(60.0), 6.0),
        );
        ui.painter().rect_filled(bar, Rounding::ZERO, p.sunken);
        ui.painter().rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * frac, 6.0)),
            Rounding::ZERO,
            load_color(p, frac * 100.0),
        );
        ui.painter().text(
            pos2(r.right(), y),
            Align2::RIGHT_CENTER,
            tf!(
                "{free} bo'sh · jami {total}",
                free = gb(d.total - d.used),
                total = gb(d.total)
            ),
            FontId::new(12.0, ui_kit::mono()),
            p.muted,
        );
    }
    ui.add_space(8.0);
    ui_kit::hud(ui, tr("Tizim"), p.muted);
    for (k, v) in [
        (tr("Operatsion tizim"), s.os.clone()),
        (tr("Yoqilganiga"), duration(s.uptime_secs)),
    ] {
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        ui.painter().text(
            pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            k,
            FontId::proportional(13.0),
            p.muted,
        );
        ui.painter().text(
            pos2(r.left() + 150.0, r.center().y),
            Align2::LEFT_CENTER,
            v,
            FontId::proportional(13.0),
            p.text,
        );
    }
}

fn processes(ui: &mut egui::Ui, p: &Palette, s: &SystemInfo, f: MachineForm<'_>) {
    ui.add(
        ui_kit::text_field(f.filter)
            .hint_text(tr("Nomi yoki PID bo'yicha qidirish"))
            .desired_width(ui.available_width()),
    );
    ui.add_space(8.0);
    let q = f.filter.trim().to_lowercase();
    let mut list: Vec<&ProcInfo> = s
        .processes
        .iter()
        .filter(|pr| q.is_empty() || pr.name.to_lowercase().contains(&q) || pr.pid.to_string() == q)
        .collect();
    match f.sort {
        ProcSort::Name => list.sort_by_key(|a| a.name.to_lowercase()),
        ProcSort::Pid => list.sort_by_key(|a| a.pid),
        ProcSort::Cpu => list.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
        ProcSort::Mem => list.sort_by_key(|a| std::cmp::Reverse(a.mem)),
    }

    let w = ui.available_width();
    // Columns: name | PID | CPU | memory | end.
    let (c_pid, c_cpu, c_mem, c_end) = (w - 300.0, w - 220.0, w - 140.0, w - 40.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    let (hr, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
    ui.painter().rect_filled(hr, Rounding::ZERO, p.bg);
    for (x, label, key, right) in [
        (12.0, tr("Nomi"), ProcSort::Name, false),
        (c_pid, "PID", ProcSort::Pid, false),
        (c_cpu, "CPU", ProcSort::Cpu, false),
        (c_mem, tr("Xotira"), ProcSort::Mem, false),
    ] {
        let _ = right;
        let on = *f.sort == key;
        let col = if on { p.accent_ink } else { p.faint };
        let text = if on {
            format!("{label} {}", icons::CHEVRON_DOWN)
        } else {
            label.to_string()
        };
        let g = ui_kit::caps(ui, &text, 9.5, ui_kit::mono(), col);
        let gr = Rect::from_min_size(
            pos2(hr.left() + x, hr.center().y - g.size().y / 2.0),
            g.size(),
        );
        ui.painter().galley(gr.min, g, col);
        let resp = ui.interact(
            gr.expand(6.0),
            ui.id().with(("sort", x as i32)),
            Sense::click(),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            *f.sort = key;
        }
    }
    if list.is_empty() {
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            tr("Hech narsa topilmadi"),
            FontId::proportional(12.5),
            p.faint,
        );
        return;
    }
    for pr in list {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        if !ui.is_rect_visible(r) {
            continue;
        }
        if resp.hovered() {
            ui.painter().rect_filled(r, Rounding::ZERO, p.hover);
        }
        ui.painter()
            .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
        let y = r.center().y;
        let name = ui_kit::truncated(
            ui,
            &pr.name,
            FontId::new(13.0, ui_kit::medium()),
            p.text,
            c_pid - 24.0,
        );
        ui.painter()
            .galley(pos2(r.left() + 12.0, y - name.size().y / 2.0), name, p.text);
        let mono = FontId::new(12.0, ui_kit::mono());
        ui.painter().text(
            pos2(r.left() + c_pid, y),
            Align2::LEFT_CENTER,
            pr.pid.to_string(),
            mono.clone(),
            p.muted,
        );
        ui.painter().text(
            pos2(r.left() + c_cpu, y),
            Align2::LEFT_CENTER,
            format!("{:.1}%", pr.cpu),
            mono.clone(),
            if pr.cpu >= 20.0 {
                load_color(p, pr.cpu * 3.0)
            } else {
                p.text
            },
        );
        ui.painter().text(
            pos2(r.left() + c_mem, y),
            Align2::LEFT_CENTER,
            mb(pr.mem),
            mono,
            p.text,
        );
        let mut end = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(r.left() + c_end, r.top() + 2.0),
                    vec2(28.0, 28.0),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        if ui_kit::icon_cell(&mut end, icons::X, 28.0, false)
            .tip(tr("Jarayonni yopish"))
            .clicked()
        {
            *f.kill = Some((pr.pid, pr.name.clone()));
        }
    }
}
