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

/// (icon, label, shown value, address to open)
fn links() -> [(&'static str, Text, &'static str, &'static str); 4] {
    [
        (
            icons::SEND,
            Text {
                uz: "Aloqa",
                ru: "Связь",
                en: "Contact",
            },
            "@veymayuz",
            "https://t.me/veymayuz",
        ),
        (
            icons::GLOBE,
            Text {
                uz: "Sayt",
                ru: "Сайт",
                en: "Website",
            },
            "veymay.uz",
            "https://veymay.uz",
        ),
        (
            icons::GITHUB,
            Text {
                uz: "GitHub",
                ru: "GitHub",
                en: "GitHub",
            },
            "veymayuz",
            "https://github.com/veymayuz",
        ),
        (
            icons::HEART,
            Text {
                uz: "Qo'llab-quvvatlash",
                ru: "Поддержать",
                en: "Donate",
            },
            "veymay.uz/donate",
            "https://veymay.uz/donate",
        ),
    ]
}

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

/// The page: photo, name, one line, and the ways to reach the author (and to support).
pub fn page(ui: &mut Ui, lang: &str) {
    let p: Palette = palette(ui.ctx());
    let w = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;

    // --- head: photo (left), names and the quote (right) -------------------------------------
    let (head, _) = ui.allocate_exact_size(vec2(w, 112.0), Sense::hover());
    let pic = Rect::from_min_size(head.min, vec2(112.0, 112.0));
    ui.painter().image(
        photo(ui).id(),
        pic,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 112.0 / 150.0)),
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
    let q = ui.painter().layout(
        QUOTE.get(lang).to_string(),
        FontId::proportional(14.0),
        p.text,
        (head.right() - x).max(120.0),
    );
    ui.painter().galley(pos2(x, head.top() + 70.0), q, p.text);

    ui.add_space(18.0);
    // --- links: one row each, a click opens it -----------------------------------------------
    egui::Frame::none()
        .stroke(Stroke::new(1.0_f32, p.line))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, (icon, label, value, url)) in links().iter().enumerate() {
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
                    label.get(lang),
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
