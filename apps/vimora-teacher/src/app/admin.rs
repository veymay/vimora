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

/// Settings → "Kirish": set or remove the administrator password.
pub(super) fn set_password(c: &mut crate::config::TeacherConfig, pw: &str) {
    if pw.is_empty() {
        c.admin_salt.clear();
        c.admin_hash.clear();
        return;
    }
    let salt = vimora_core::auth::nonce();
    c.admin_hash = hex(&vimora_core::pin::hash(&salt, pw));
    c.admin_salt = hex(&salt);
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
    ui.add_space(10.0);
    ui_kit::hud(ui, tr("Jadval"), p.muted);
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr("O'qituvchi ochiq bo'lsa, belgilangan vaqtda o'zi bajaradi: masalan, har kuni 08:00 da kompyuterlarni yoqish, 13:00 da qulflash."))
            .size(12.5)
            .color(p.muted),
    );
    ui.add_space(10.0);
    let mut remove = None;
    for (i, s) in c.schedules.iter_mut().enumerate() {
        Frame::none()
            .stroke(Stroke::new(1.0_f32, p.line))
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui_kit::toggle(ui, &mut s.on);
                    let r = ui.add(
                        ui_kit::text_field(&mut s.time)
                            .hint_text("08:00")
                            .desired_width(64.0),
                    );
                    if r.changed() {
                        s.time.retain(|ch| ch.is_ascii_digit() || ch == ':');
                        s.time.truncate(5);
                    }
                    let acts = actions();
                    let label = acts.get(s.action as usize).map_or("", |a| a.1);
                    egui::ComboBox::from_id_salt(("sched-action", i))
                        .selected_text(label)
                        .width(210.0)
                        .show_ui(ui, |ui| {
                            for (k, (_, name)) in acts.iter().enumerate() {
                                ui.selectable_value(&mut s.action, k as u8, *name);
                            }
                        });
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
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (d, name) in DAYS.iter().enumerate() {
                        if ui_kit::tab(ui, tr(name), None, s.days[d]).clicked() {
                            s.days[d] = !s.days[d];
                        }
                    }
                });
            });
        ui.add_space(8.0);
    }
    if let Some(i) = remove {
        c.schedules.remove(i);
    }
    if ui_kit::button(ui, Some(icons::PLUS), tr("Qo'shish"), Kind::Outline).clicked() {
        c.schedules.push(Schedule::default());
    }
}

/// "Hisobotlar": exports.
pub(super) fn reports_page(ui: &mut egui::Ui, p: &Palette, out: &mut SettingsOut) {
    ui.add_space(10.0);
    ui_kit::hud(ui, tr("Hisobotlar"), p.muted);
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(
            "CSV fayl Excel'da ochiladi. Hujjatlar/Vimora papkasiga saqlanadi.",
        ))
        .size(12.5)
        .color(p.muted),
    );
    ui.add_space(10.0);
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
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(13.5).color(p.text));
                ui.label(RichText::new(help).size(12.0).color(p.faint));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui_kit::button(ui, Some(icons::DOWNLOAD), tr("CSV eksport"), Kind::Outline)
                    .clicked()
                {
                    out.report = Some(which);
                }
            });
        });
        ui.add_space(10.0);
    }
}

/// "Kirish": the administrator password.
pub(super) fn access_page(
    ui: &mut egui::Ui,
    p: &Palette,
    c: &mut crate::config::TeacherConfig,
    out: &mut SettingsOut,
) {
    ui.add_space(10.0);
    ui_kit::hud(ui, tr("Administrator paroli"), p.muted);
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr("Parol qo'yilsa, dastur o'qituvchi rejimida ochiladi: Muzlatish, Yangilanish, Quvvat, Sozlamalar, kompyuterni o'chirib tashlash va IT bo'limlari parol so'raydi. Pastki paneldagi qulf belgisi orqali administrator bo'lib kirasiz yoki chiqasiz."))
            .size(12.5)
            .color(p.muted),
    );
    ui.add_space(12.0);
    let id = egui::Id::new("admin-new-pw");
    let mut pw: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let r = ui.add(
            ui_kit::text_field(&mut pw)
                .password(true)
                .hint_text(if c.admin_hash.is_empty() {
                    tr("Yangi parol")
                } else {
                    tr("Yangi parol (almashtirish)")
                })
                .desired_width(240.0),
        );
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(id, pw.clone()));
        }
        ui.add_enabled_ui(pw.len() >= 4, |ui| {
            if ui_kit::button(ui, Some(icons::CHECK), tr("Saqlash"), Kind::Primary).clicked() {
                set_password(c, &pw);
                ui.data_mut(|d| d.remove::<String>(id));
                out.password_set = true;
            }
        });
    });
    if !pw.is_empty() && pw.len() < 4 {
        ui.label(
            RichText::new(tr("Kamida 4 ta belgi"))
                .size(12.0)
                .color(p.warning),
        );
    }
    ui.add_space(10.0);
    if !c.admin_hash.is_empty() {
        ui.label(
            RichText::new(with_icon(icons::CIRCLE_CHECK, tr("Parol o'rnatilgan")))
                .size(13.0)
                .color(p.success),
        );
        ui.add_space(6.0);
        if ui_kit::button(
            ui,
            Some(icons::TRASH_2),
            tr("Parolni olib tashlash"),
            Kind::Ghost,
        )
        .clicked()
        {
            set_password(c, "");
        }
    }
}
