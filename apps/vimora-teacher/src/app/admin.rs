//! Administration: the schedule ("Jadval"), reports (CSV) and the administrator password
//! that separates the teacher's everyday teacher from the set-up tools.
//!
//! With an administrator password set, the teacher starts in teacher mode: Muzlatish,
//! Yangilanish, Quvvat, Sozlamalar, removing computers and the IT tabs of "Kompyuter holati"
//! ask for the password first.

use super::*;
use crate::config::Schedule;
#[allow(unused_imports)]
use vimora_ui::{tf, tr};

/// What a schedule can do (index = `Schedule::action`).
pub(super) fn actions() -> [(&'static str, &'static str); 7] {
    [
        (icons::LOCK, tr("Qulflash")),
        (icons::LOCK_OPEN, tr("Qulfni ochish")),
        (icons::GLOBE_LOCK, tr("Internetni o'chirish")),
        (icons::GLOBE, tr("Internetni yoqish")),
        (icons::POWER_OFF, tr("Kompyuterlarni o'chirish")),
        (icons::POWER, tr("Kompyuterlarni yoqish")),
        (icons::DOWNLOAD, tr("Topshiriqlarni yig'ish")),
    ]
}

pub(super) const DAYS: [&str; 7] = ["Du", "Se", "Ch", "Pa", "Ju", "Sh", "Ya"];

/// Menus that need the administrator (when a password is set).
pub(super) fn restricted(m: Menu) -> bool {
    matches!(
        m,
        Menu::Freeze | Menu::Updates | Menu::Power | Menu::Settings
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub(super) fn check_password(c: &crate::config::TeacherConfig, pw: &str) -> bool {
    let salt: Vec<u8> = (0..c.admin_salt.len() / 2)
        .filter_map(|i| u8::from_str_radix(&c.admin_salt[i * 2..i * 2 + 2], 16).ok())
        .collect();
    !c.admin_hash.is_empty() && hex(&vimora_core::pin::hash(&salt, pw)) == c.admin_hash
}

impl TeacherApp {
    /// Opens the password window; `then` runs after a successful login.
    pub(super) fn ask_admin(&mut self, then: Option<Menu>) {
        self.open_dialog(Dialog::AdminLogin {
            password: String::new(),
            error: false,
            then,
        });
    }

    /// Runs due schedules (checked once a minute).
    pub(super) fn run_schedules(&mut self, all: &[View]) {
        use chrono::{Datelike, Timelike};
        let now = chrono::Local::now();
        let minute = now.format("%Y-%m-%d %H:%M").to_string();
        if self.sched_minute == minute {
            return;
        }
        self.sched_minute = minute;
        let hm = format!("{:02}:{:02}", now.hour(), now.minute());
        let day = now.weekday().num_days_from_monday() as usize;
        let due: Vec<Schedule> = self
            .cfg
            .read()
            .unwrap()
            .schedules
            .iter()
            .filter(|s| s.on && s.days[day] && s.time.trim() == hm)
            .cloned()
            .collect();
        for s in due {
            self.run_action(s.action, all);
        }
    }

    /// One schedule action on every computer.
    fn run_action(&mut self, action: u8, all: &[View]) {
        let online: Vec<u64> = all.iter().filter(|v| v.online).map(|v| v.id).collect();
        let what = tf!(
            "Jadval: {a}",
            a = actions().get(action as usize).map_or("", |a| a.1)
        );
        let access =
            |internet_off: bool, c: &crate::config::TeacherConfig| vimora_core::AccessPolicy {
                internet_off,
                allow_sites: if c.web_mode == 1 {
                    crate::config::active_list(&c.allow_sites)
                } else {
                    vec![]
                },
                ports: parse_ports(&c.blocked_ports),
            };
        match action {
            0 => self.lock(&online),
            1 => self.send(&online, Command::Unlock, ""),
            2 | 3 => {
                let a = {
                    let mut c = self.cfg.write().unwrap();
                    if action == 2 {
                        c.web_mode = 2;
                    } else if c.web_mode == 2 {
                        c.web_mode = 0;
                    }
                    access(action == 2, &c)
                };
                self.send_priv(&online, PrivCommand::SetAccessPolicy(a), "");
            }
            4 => self.send_priv(
                &online,
                PrivCommand::Power(vimora_core::PowerAction::Shutdown),
                "",
            ),
            5 => {
                let keys: Vec<String> = all
                    .iter()
                    .filter(|v| !v.online)
                    .map(|v| v.key.clone())
                    .collect();
                self.wake(&keys);
            }
            6 => {
                self.hub.lock().unwrap().collected.clear();
                self.send(&online, Command::CollectFiles, "");
            }
            _ => return,
        }
        self.hub.lock().unwrap().log(Level::Info, what);
    }

    /// Computers as CSV (Documents/Vimora/Hisobotlar).
    pub(super) fn export_computers(&mut self, all: &[View]) {
        let mut out = format!(
            "{};{};{};{};{};{};{}\n",
            tr("Kompyuter"),
            tr("Xona"),
            "IP",
            tr("Foydalanuvchi"),
            tr("Tizim"),
            tr("Versiya"),
            tr("Holat")
        );
        for v in all {
            out += &format!(
                "{};{};{};{};{};{};{}\n",
                v.label,
                v.room.clone().unwrap_or_default(),
                v.ip,
                v.user,
                v.os,
                v.version,
                if v.online { tr("Onlayn") } else { tr("Oflayn") }
            );
        }
        self.write_report("kompyuterlar", out);
    }

    /// The activity log as CSV.
    pub(super) fn export_log(&mut self) {
        let mut out = format!("{};{}\n", tr("Vaqt"), tr("Hodisa"));
        for e in &self.hub.lock().unwrap().log {
            out += &format!("{};{}\n", e.time, e.text.replace(['\n', ';'], " "));
        }
        self.write_report("faollik", out);
    }

    fn write_report(&mut self, name: &str, text: String) {
        let dir = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("Vimora")
            .join("Hisobotlar");
        let path = dir.join(format!(
            "{name}-{}.csv",
            chrono::Local::now().format("%Y%m%d-%H%M")
        ));
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
}

// ---------------------------------------------------------------------------------------------
// Settings pages

/// "Jadval": rows of time · days · action · on.
pub(super) fn schedule_page(ui: &mut egui::Ui, p: &Palette, c: &mut crate::config::TeacherConfig) {
    ui.add_space(4.0);
    let mut remove = None;
    let w = ui.available_width();
    // One framed list; an entry is one compact row, opened to edit (like every accordion).
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, s) in c.schedules.iter_mut().enumerate() {
                let open_id = egui::Id::new(("sched-open", i));
                let is_open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
                let rw = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(rw, 56.0), Sense::click());
                if i > 0 {
                    ui.painter()
                        .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                }
                if resp.hovered() || is_open {
                    ui.painter()
                        .rect_filled(r.shrink2(vec2(1.0, 0.5)), Rounding::ZERO, p.hover);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let y = r.center().y;
                // The switch.
                let mut sw = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_size(
                            pos2(r.left() + 14.0, y - 11.0),
                            vec2(40.0, 22.0),
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let toggled = ui_kit::toggle(&mut sw, &mut s.on).changed();
                // Time, what it does, on which days.
                let ink = if s.on { p.strong } else { p.faint };
                ui.painter().text(
                    pos2(r.left() + 70.0, y),
                    Align2::LEFT_CENTER,
                    if s.time.is_empty() { "--:--" } else { &s.time },
                    FontId::new(17.0, ui_kit::mono_bold()),
                    ink,
                );
                let acts = actions();
                let what = acts.get(s.action as usize).map_or("", |a| a.1);
                ui.painter().text(
                    pos2(r.left() + 148.0, y - 8.0),
                    Align2::LEFT_CENTER,
                    what,
                    FontId::new(13.5, ui_kit::bold()),
                    if s.on { p.text } else { p.faint },
                );
                ui.painter().text(
                    pos2(r.left() + 148.0, y + 10.0),
                    Align2::LEFT_CENTER,
                    days_summary(&s.days),
                    FontId::new(11.5, ui_kit::mono()),
                    p.muted,
                );
                ui.painter().text(
                    pos2(r.right() - 18.0, y),
                    Align2::CENTER_CENTER,
                    if is_open {
                        icons::CHEVRON_DOWN
                    } else {
                        icons::CHEVRON_RIGHT
                    },
                    FontId::proportional(14.0),
                    p.muted,
                );
                if resp.clicked() && !toggled {
                    ui.data_mut(|d| d.insert_temp(open_id, !is_open));
                }
                // The editor.
                ui_kit::accordion_body(ui, open_id.with("body"), is_open, |ui| {
                    Frame::none()
                        .fill(p.bg)
                        .inner_margin(Margin::symmetric(16.0, 14.0))
                        .show(ui, |ui| {
                            ui.set_width(rw - 32.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                let r = ui.add(
                                    ui_kit::text_field(&mut s.time)
                                        .hint_text("08:00")
                                        .desired_width(70.0),
                                );
                                if r.changed() {
                                    s.time.retain(|ch| ch.is_ascii_digit() || ch == ':');
                                    s.time.truncate(5);
                                }
                                let names: Vec<(u8, &str)> = acts
                                    .iter()
                                    .enumerate()
                                    .map(|(k, a)| (k as u8, a.1))
                                    .collect();
                                super::settings::dropdown(
                                    ui,
                                    p,
                                    &format!("sched-{i}"),
                                    &mut s.action,
                                    &names,
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui_kit::icon_cell(ui, icons::TRASH_2, 30.0, false)
                                        .tip(tr("O'chirish"))
                                        .clicked()
                                    {
                                        remove = Some(i);
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            let cells: Vec<(&str, bool)> = DAYS
                                .iter()
                                .enumerate()
                                .map(|(d, name)| (tr(name), s.days[d]))
                                .collect();
                            if let Some(d) = ui_kit::multi_choices(ui, &cells) {
                                s.days[d] = !s.days[d];
                            }
                        });
                });
            }
        });
    ui.add_space(7.0);
    if let Some(i) = remove {
        c.schedules.remove(i);
    }
    if ui_kit::button(ui, Some(icons::PLUS), tr("Qo'shish"), Kind::Outline).clicked() {
        c.schedules.push(Schedule::default());
    }
}

/// "Har kuni", "Du–Ju", or the short names of the chosen days.
fn days_summary(days: &[bool; 7]) -> String {
    let n = days.iter().filter(|d| **d).count();
    if n == 7 {
        return tr("Har kuni").to_string();
    }
    if n == 0 {
        return "—".to_string();
    }
    if days[..5].iter().all(|d| *d) && !days[5] && !days[6] {
        return format!("{}–{}", tr(DAYS[0]), tr(DAYS[4]));
    }
    DAYS.iter()
        .enumerate()
        .filter(|(d, _)| days[*d])
        .map(|(_, name)| tr(name))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Hisobotlar": exports.
pub(super) fn reports_page(ui: &mut egui::Ui, p: &Palette, out: &mut SettingsOut) {
    ui.add_space(4.0);
    for (label, help, which) in [
        (
            tr("Kompyuterlar ro'yxati"),
            tr("Nomi, xonasi, IP, foydalanuvchi, tizim, versiya, holat."),
            0u8,
        ),
        (
            tr("Faollik jurnali"),
            tr("Bugun dasturda bo'lgan hamma hodisalar."),
            1,
        ),
        (tr("Bugungi davomat"), tr("Kim qachon kelgani."), 2),
    ] {
        super::settings::row(ui, p, label, Some(help), |ui| {
            if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("CSV eksport"), Kind::Outline).clicked()
            {
                out.report = Some(which);
            }
        });
    }
}

/// "Kirish": the program code.
pub(super) fn access_page(ui: &mut egui::Ui, p: &Palette, c: &mut crate::config::TeacherConfig) {
    ui.add_space(4.0);
    // Two protections, each one row: its state and one button. The button opens the field
    // under the row (type, save).
    let pin_set = !c.app_pin_hash.is_empty();
    match secret_row(
        ui,
        p,
        "pin",
        icons::LOCK,
        tr("Dastur kodi"),
        Some(tr(
            "Dastur ochilganda va bir necha daqiqa tegilmasa shu kod so'raladi. 4–8 raqam.",
        )),
        pin_set,
        true,
        vimora_core::pin::valid,
    ) {
        Secret::Save(v) => super::applock::set_pin(c, &v),
        Secret::Remove => super::applock::set_pin(c, ""),
        Secret::None => {}
    }
    if pin_set {
        super::settings::row(
            ui,
            p,
            tr("Avto-qulf"),
            Some(tr("Shuncha daqiqa tegilmasa dastur o'zi qulflanadi.")),
            |ui| {
                super::settings::dropdown(
                    ui,
                    p,
                    "app-lock-min",
                    &mut c.app_lock_min,
                    &[
                        (0, tr("Faqat ochilganda")),
                        (1, tr("1 daqiqa")),
                        (2, tr("2 daqiqa")),
                        (5, tr("5 daqiqa")),
                        (10, tr("10 daqiqa")),
                        (15, tr("15 daqiqa")),
                        (30, tr("30 daqiqa")),
                    ],
                );
            },
        );
    }
}

enum Secret {
    None,
    Save(String),
    Remove,
}

/// One protection as a row: icon, name, "?", its state (on / off) and one button; the button
/// opens a field below the row with save and cancel (and remove when it is on).
#[allow(clippy::too_many_arguments)]
fn secret_row(
    ui: &mut egui::Ui,
    p: &Palette,
    key: &str,
    icon: &str,
    title: &str,
    help: Option<&str>,
    is_set: bool,
    digits_only: bool,
    valid: impl Fn(&str) -> bool,
) -> Secret {
    let open_id = egui::Id::new(("secret-open", key));
    let text_id = egui::Id::new(("secret-text", key));
    let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let mut text: String = ui.data(|d| d.get_temp(text_id)).unwrap_or_default();
    let mut result = Secret::None;
    super::settings::row(ui, p, title, help, |ui| {
        // From the right: the button, then the state.
        let label = if is_set {
            tr("O'zgartirish")
        } else {
            tr("O'rnatish")
        };
        if ui_kit::button(ui, Some(icon), label, Kind::Outline).clicked() {
            open = !open;
            text.clear();
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(if is_set { tr("Yoqilgan") } else { tr("O'chiq") })
                .size(13.0)
                .color(if is_set { p.success } else { p.faint }),
        );
    });
    if open {
        // One line under the row, from the right edge: cancel, remove, save, and the field
        // takes what is left; every control the same height.
        ui.add_space(6.0);
        let w = ui.available_width();
        ui.allocate_ui_with_layout(
            vec2(w, ui_kit::CONTROL_H),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if ui_kit::icon_cell(ui, icons::X, ui_kit::CONTROL_H, false)
                    .tip(tr("Bekor qilish"))
                    .clicked()
                {
                    open = false;
                }
                if is_set
                    && ui_kit::icon_cell(ui, icons::TRASH_2, ui_kit::CONTROL_H, false)
                        .tip(tr("Olib tashlash"))
                        .clicked()
                {
                    result = Secret::Remove;
                    open = false;
                }
                let ok = valid(&text);
                let mut save = false;
                ui.add_enabled_ui(ok, |ui| {
                    save = ui_kit::button(ui, Some(icons::CHECK), tr("Saqlash"), Kind::Primary)
                        .clicked();
                });
                let r = ui.add(
                    ui_kit::password_field(&mut text)
                        .hint_text(if digits_only {
                            tr("4–8 raqam")
                        } else {
                            tr("Kamida 4 ta belgi")
                        })
                        .desired_width(ui.available_width()),
                );
                if r.changed() && digits_only {
                    text.retain(|ch| ch.is_ascii_digit());
                    text.truncate(8);
                }
                if !r.has_focus() && text.is_empty() {
                    r.request_focus();
                }
                let enter = (r.has_focus() || r.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (save || enter) && ok {
                    result = Secret::Save(std::mem::take(&mut text));
                    open = false;
                }
            },
        );
        ui.add_space(6.0);
    }
    ui.data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(text_id, text);
    });
    result
}
