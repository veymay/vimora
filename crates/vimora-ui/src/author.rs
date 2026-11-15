//! The "Muallif" (author) page, shared by the teacher's settings and the student's window.
//! The texts come from veymay.uz, in the three languages of the programs.

use crate::{icons, palette, Palette};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Rounding, Sense, Stroke, Ui};

const PHOTO: &[u8] = include_bytes!("../assets/author.jpg");

struct Text {
    uz: &'static str,
    ru: &'static str,
    en: &'static str,
}

impl Text {
    fn get(&self, lang: &str) -> &'static str {
        match lang {
            "ru" => self.ru,
            "en" => self.en,
            _ => self.uz,
        }
    }
}

const QUOTE: Text = Text {
    uz: "G'oyangizni — natija beradigan tizimga aylantiraman.",
    ru: "Превращу вашу идею в систему, которая даёт результат.",
    en: "I turn your idea into a system that delivers.",
};

const BIO: Text = Text {
    uz: "6 yildan beri IT sohasidaman: frontend, backend va nocode yechimlar bilan sayt hamda veb-ilovalarni noldan yarataman, tayyor loyihalarni esa kiberxavfsizlik nuqtai nazaridan zaifliklarga tekshiraman. IT-CENTER NAVOIYda 500 dan ortiq talabaga dasturlashdan dars berganman.",
    ru: "Я в IT уже 6 лет: с нуля создаю сайты и веб-приложения на frontend, backend и no-code, а готовые проекты проверяю на уязвимости с точки зрения кибербезопасности. В IT-CENTER NAVOIY я преподавал программирование более чем 500 студентам.",
    en: "I have been in IT for 6 years: I build websites and web apps from scratch with frontend, backend and no-code, and test finished projects for vulnerabilities. At IT-CENTER NAVOIY I taught programming to more than 500 students.",
};

const STATS: [(&str, Text); 3] = [
    (
        "6+",
        Text {
            uz: "yil IT sohasida",
            ru: "лет в IT",
            en: "years in IT",
        },
    ),
    (
        "500+",
        Text {
            uz: "talaba",
            ru: "студентов",
            en: "students",
        },
    ),
    (
        "100+",
        Text {
            uz: "loyiha",
            ru: "проектов",
            en: "projects",
        },
    ),
];

const WINS: [Text; 2] = [
    Text {
        uz: "National AI Hackathon: Navoiy — 2-o'rin",
        ru: "National AI Hackathon: Навои — 2 место",
        en: "National AI Hackathon: Navoiy — 2nd place",
    },
    Text {
        uz: "Davlat tizimida xavfsizlik auditi — 4 ta jiddiy zaiflik yopildi",
        ru: "Аудит безопасности госсистемы — закрыты 4 серьёзные уязвимости",
        en: "Security audit of a state system — 4 serious flaws fixed",
    },
];

/// (icon, label, shown value, address to open)
const LINKS: [(&str, &str, &str, &str); 8] = [
    (icons::GLOBE, "Sayt", "veymay.uz", "https://veymay.uz"),
    (
        icons::SEND,
        "Telegram",
        "@veymayuz",
        "https://t.me/veymayuz",
    ),
    (
        icons::EXTERNAL_LINK,
        "Instagram",
        "@veymayuz",
        "https://instagram.com/veymayuz",
    ),
    (
        icons::GITHUB,
        "GitHub",
        "veymayuz",
        "https://github.com/veymayuz",
    ),
    (
        icons::EXTERNAL_LINK,
        "LinkedIn",
        "in/veymay",
        "https://www.linkedin.com/in/veymay",
    ),
    (
        icons::EXTERNAL_LINK,
        "YouTube",
        "@veymayuz",
        "https://youtube.com/veymayuz",
    ),
    (
        icons::EXTERNAL_LINK,
        "Email",
        "info@veymay.uz",
        "mailto:info@veymay.uz",
    ),
    (
        icons::EXTERNAL_LINK,
        "Telefon",
        "+998 90 620 29 19",
        "tel:+998906202919",
    ),
];

fn photo(ui: &Ui) -> egui::TextureHandle {
    let ctx = ui.ctx();
    let id = egui::Id::new("vimora-author-photo");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let img = image::load_from_memory_with_format(PHOTO, image::ImageFormat::Jpeg)
        .map(|i| i.to_rgba8())
        .unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let t = ctx.load_texture(
        "vimora-author",
        egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// The page: photo and name, a short story, the numbers, wins and the ways to reach the author.
pub fn page(ui: &mut Ui, lang: &str) {
    let p: Palette = palette(ui.ctx());
    let w = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;

    // --- head: photo (left), names and the quote (right) -------------------------------------
    let (head, _) = ui.allocate_exact_size(vec2(w, 150.0), Sense::hover());
    let pic = Rect::from_min_size(head.min, vec2(112.0, 150.0));
    ui.painter().image(
        photo(ui).id(),
        pic,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter()
        .rect_stroke(pic, Rounding::ZERO, Stroke::new(1.0_f32, p.line_strong));
    let x = pic.right() + 18.0;
    ui.painter().text(
        pos2(x, head.top() + 14.0),
        Align2::LEFT_CENTER,
        "VEYMAY",
        FontId::new(26.0, crate::brand()),
        p.accent,
    );
    ui.painter().text(
        pos2(x, head.top() + 44.0),
        Align2::LEFT_CENTER,
        "Muxriddin Rustamov",
        FontId::new(17.0, crate::bold()),
        p.strong,
    );
    ui.painter().text(
        pos2(x, head.top() + 66.0),
        Align2::LEFT_CENTER,
        "Frontend · Backend · Nocode · Cybersecurity",
        FontId::new(11.0, crate::mono()),
        p.muted,
    );
    let q = ui.painter().layout(
        QUOTE.get(lang).to_string(),
        FontId::proportional(14.0),
        p.text,
        (head.right() - x).max(120.0),
    );
    ui.painter().galley(pos2(x, head.top() + 86.0), q, p.text);

    // --- the story ---------------------------------------------------------------------------
    ui.add_space(16.0);
    ui.add(egui::Label::new(egui::RichText::new(BIO.get(lang)).size(13.5).color(p.muted)).wrap());

    // --- numbers -----------------------------------------------------------------------------
    ui.add_space(16.0);
    let (row, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
    let cell = row.width() / 3.0;
    for (i, (num, label)) in STATS.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(row.left() + i as f32 * cell, row.top()),
            vec2(cell, row.height()),
        );
        ui.painter()
            .rect(r, Rounding::ZERO, p.sunken, Stroke::new(1.0_f32, p.line));
        ui.painter().text(
            pos2(r.center().x, r.top() + 24.0),
            Align2::CENTER_CENTER,
            num,
            FontId::new(22.0, crate::mono_bold()),
            p.accent,
        );
        ui.painter().text(
            pos2(r.center().x, r.top() + 47.0),
            Align2::CENTER_CENTER,
            label.get(lang),
            FontId::proportional(12.0),
            p.muted,
        );
    }

    // --- wins --------------------------------------------------------------------------------
    ui.add_space(14.0);
    for win in &WINS {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(egui::RichText::new(icons::CHECK).size(13.0).color(p.accent));
            ui.add(
                egui::Label::new(egui::RichText::new(win.get(lang)).size(13.0).color(p.text))
                    .wrap(),
            );
        });
        ui.add_space(4.0);
    }

    // --- links: one row each, a click opens it -----------------------------------------------
    ui.add_space(14.0);
    egui::Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, (icon, label, value, url)) in LINKS.iter().enumerate() {
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                if i > 0 {
                    ui.painter()
                        .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.line));
                }
                if resp.hovered() {
                    ui.painter().rect_filled(r, Rounding::ZERO, p.hover);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.painter().text(
                    pos2(r.left() + 20.0, r.center().y),
                    Align2::CENTER_CENTER,
                    icon,
                    FontId::proportional(15.0),
                    p.accent_ink,
                );
                ui.painter().text(
                    pos2(r.left() + 42.0, r.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::new(13.5, crate::medium()),
                    p.text,
                );
                ui.painter().text(
                    pos2(r.right() - 14.0, r.center().y),
                    Align2::RIGHT_CENTER,
                    value,
                    FontId::new(12.5, crate::mono()),
                    if resp.hovered() { p.strong } else { p.muted },
                );
                if resp.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(*url));
                }
            }
        });
    ui.add_space(8.0);
}
