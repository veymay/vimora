//! Vimora design system for egui.
//!
//! Visual language: Claude-style warm neutrals (dark `#262624`, light `#faf9f5`) with the clay
//! accent, soft rounded corners, hairline borders, Geist text with Geist Mono upper-case labels
//! (ArtCraft-inspired) and Lucide outline icons.

pub mod icons;

use egui::{
    epaint::Shadow, text::LayoutJob, vec2, Align2, Color32, FontData, FontDefinitions, FontFamily,
    FontId, Frame, Galley, Margin, Rect, Response, RichText, Rounding, Sense, Stroke, TextFormat,
    TextStyle, Ui, Visuals,
};
use std::sync::Arc;

const LUCIDE_TTF: &[u8] = include_bytes!("../assets/lucide.ttf");
const GEIST: &[u8] = include_bytes!("../assets/Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../assets/Geist-Medium.ttf");
const GEIST_BOLD: &[u8] = include_bytes!("../assets/Geist-Bold.ttf");
const GEIST_MONO_MEDIUM: &[u8] = include_bytes!("../assets/GeistMono-Medium.ttf");
const GEIST_MONO_BOLD: &[u8] = include_bytes!("../assets/GeistMono-Bold.ttf");

/// Height of buttons, inputs and filter cells.
pub const CONTROL_H: f32 = 34.0;
/// Minimum width of a labelled button.
pub const BUTTON_MIN_W: f32 = 120.0;
/// Mono label size and letter spacing.
pub const HUD: f32 = 11.0;
pub const HUD_SPACING: f32 = HUD * 0.14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    pub fn palette(self) -> Palette {
        match self {
            ThemeMode::Dark => DARK,
            ThemeMode::Light => LIGHT,
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }
}

/// Colour tokens for one appearance.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Page and card surface.
    pub bg: Color32,
    /// Wells, inputs, hovers.
    pub sunken: Color32,
    pub hover: Color32,
    /// Body text / headings.
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    pub faint: Color32,
    /// Hairlines and stronger rules.
    pub line: Color32,
    pub line_strong: Color32,
    pub accent: Color32,
    pub accent_ink: Color32,
    /// Primary buttons (inverted).
    pub invert_bg: Color32,
    pub invert_fg: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    // Legacy aliases used by older call sites.
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub on_accent: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

// Claude desktop palette (warm neutrals + clay accent).
pub const DARK: Palette = Palette {
    dark: true,
    bg: hex(0x262624),
    sunken: hex(0x1f1e1d),
    hover: hex(0x3a3936),
    text: hex(0xf5f4ee),
    strong: hex(0xfaf9f5),
    muted: hex(0xb3b0a6),
    faint: hex(0x85837b),
    line: hex(0x3d3c38),
    line_strong: hex(0x5c5a54),
    accent: hex(0xd97757),
    accent_ink: hex(0xe8987c),
    invert_bg: hex(0xd97757),
    invert_fg: hex(0xffffff),
    success: hex(0x6fbf73),
    warning: hex(0xe9b44c),
    danger: hex(0xe5685a),
    panel: hex(0x1f1e1d),
    card: hex(0x30302e),
    card_hover: hex(0x363633),
    border: hex(0x3d3c38),
    on_accent: hex(0xffffff),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: hex(0xfaf9f5),
    sunken: hex(0xf0eee6),
    hover: hex(0xebe9df),
    text: hex(0x141413),
    strong: hex(0x000000),
    muted: hex(0x5e5d59),
    faint: hex(0x8f8d85),
    line: hex(0xe3e1d7),
    line_strong: hex(0xc4c1b5),
    accent: hex(0xc96442),
    accent_ink: hex(0xae5230),
    invert_bg: hex(0xc96442),
    invert_fg: hex(0xffffff),
    success: hex(0x2e7d32),
    warning: hex(0x9a6700),
    danger: hex(0xc0392b),
    panel: hex(0xf5f4ed),
    card: hex(0xffffff),
    card_hover: hex(0xfbfaf7),
    border: hex(0xe3e1d7),
    on_accent: hex(0xffffff),
};

/// Colours for numbered tags (one per student, cycling).
pub const TAG_COLORS: [Color32; 7] = [
    hex(0x2d81ff),
    hex(0xe5533d),
    hex(0x8b5cf6),
    hex(0xf5a524),
    hex(0x12a594),
    hex(0xe0418f),
    hex(0x6bbf3a),
];

/// Palette matching the context's current theme.
pub fn palette(ctx: &egui::Context) -> Palette {
    if ctx.style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}
pub fn mono() -> FontFamily {
    FontFamily::Name("mono".into())
}
pub fn mono_bold() -> FontFamily {
    FontFamily::Name("mono-bold".into())
}

/// Geist + Geist Mono, with Lucide icons and egui's default fonts as fallbacks.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .font_data
        .insert("lucide".into(), FontData::from_static(LUCIDE_TTF));
    let faces: [(&str, &'static [u8], FontFamily); 5] = [
        ("Geist", GEIST, FontFamily::Proportional),
        ("Geist-Medium", GEIST_MEDIUM, medium()),
        ("Geist-Bold", GEIST_BOLD, bold()),
        ("GeistMono-Medium", GEIST_MONO_MEDIUM, mono()),
        ("GeistMono-Bold", GEIST_MONO_BOLD, mono_bold()),
    ];
    for (name, bytes, family) in faces {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes));
        let mut stack = vec![name.to_string()];
        stack.extend(fallback.iter().cloned());
        stack.push("lucide".into());
        fonts.families.insert(family, stack);
    }
    let mut mono_stack = vec!["GeistMono-Medium".to_string()];
    mono_stack.extend(fallback.iter().cloned());
    mono_stack.push("lucide".into());
    fonts.families.insert(FontFamily::Monospace, mono_stack);
    ctx.set_fonts(fonts);
}

/// Apply the full Vimora theme.
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    let p = mode.palette();
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.sunken;
    v.faint_bg_color = p.sunken;
    v.code_bg_color = p.sunken;
    v.window_rounding = Rounding::ZERO;
    v.menu_rounding = Rounding::ZERO;
    v.window_stroke = Stroke::new(1.0_f32, p.line);
    let sh = Shadow {
        offset: vec2(0.0, 8.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if p.dark { 140 } else { 36 }),
    };
    v.window_shadow = sh;
    v.popup_shadow = sh;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent_ink;
    v.selection.bg_fill = p.accent;
    // Square everything: slider handles too.
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    // Also the focus ring of text inputs.
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    for (w, fill, stroke, fg) in [
        (&mut v.widgets.noninteractive, p.card, p.line, p.muted),
        (&mut v.widgets.inactive, p.sunken, p.line, p.text),
        (&mut v.widgets.hovered, p.hover, p.line_strong, p.strong),
        (&mut v.widgets.active, p.sunken, p.line_strong, p.strong),
        (&mut v.widgets.open, p.sunken, p.line_strong, p.strong),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.rounding = Rounding::ZERO;
        w.fg_stroke = Stroke::new(1.0_f32, fg);
        w.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.icon_spacing = 8.0;
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = CONTROL_H;
        s.spacing.menu_margin = Margin::same(6.0);
        s.spacing.window_margin = Margin::same(18.0);
        s.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, bold())),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, medium())),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

// ---------------------------------------------------------------------------------------------
// Text helpers

/// Text with a leading Lucide icon.
pub fn with_icon(icon: &str, label: &str) -> String {
    if label.is_empty() {
        icon.to_string()
    } else {
        format!("{icon}  {label}")
    }
}

/// A standalone icon glyph.
pub fn icon(icon: &str, size: f32, color: Color32) -> RichText {
    RichText::new(icon).size(size).color(color)
}

pub fn muted(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(palette(ui.ctx()).muted)
}

/// `text` laid out upper case in `family`, with letter spacing.
pub fn caps(ui: &Ui, text: &str, size: f32, family: FontFamily, color: Color32) -> Arc<Galley> {
    let format = TextFormat {
        font_id: FontId::new(size, family),
        color,
        extra_letter_spacing: size * 0.14,
        ..Default::default()
    };
    ui.painter()
        .layout_job(LayoutJob::single_section(text.to_uppercase(), format))
}

/// Single-line text truncated with "…" at `max_w`.
pub fn truncated(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_string(),
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
    ui.painter().layout_job(job)
}

/// A mono upper-case label.
pub fn hud(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let g = caps(ui, text, HUD, mono(), color);
    let (rect, resp) = ui.allocate_exact_size(g.size(), Sense::hover());
    ui.painter().galley(rect.min, g, color);
    resp
}

/// "01 / TEXT" eyebrow.
pub fn eyebrow(ui: &mut Ui, index: &str, label: &str) {
    let p = palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        hud(ui, index, p.faint);
        hud(ui, " / ", p.faint);
        hud(ui, label, p.muted);
    });
}

// ---------------------------------------------------------------------------------------------
// Widgets

/// Solid chip with bold mono text (number tags, the room name).
pub fn tag(ui: &mut Ui, text: &str, fill: Color32, ink: Color32) -> Response {
    let g = caps(ui, text, HUD, mono_bold(), ink);
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(14.0, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, fill);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, ink);
    resp
}

/// Outlined status chip ("QULFLANGAN", "ONLAYN").
pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let g = caps(ui, text, 10.0, mono(), color);
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(12.0, 6.0), Sense::hover());
    paint_chip(ui.painter(), rect, g, color);
    resp
}

pub fn paint_chip(painter: &egui::Painter, rect: Rect, g: Arc<Galley>, color: Color32) {
    painter.rect(
        rect,
        Rounding::ZERO,
        color.gamma_multiply(0.14),
        Stroke::new(1.0_f32, color.gamma_multiply(0.5)),
    );
    painter.galley(rect.center() - g.size() / 2.0, g, color);
}

/// Legacy pill badge, now a chip with an icon.
pub fn badge(ui: &mut Ui, icon: &str, text: &str, color: Color32) {
    chip(ui, &with_icon(icon, text), color);
}

/// Vertical hairline `height` tall.
pub fn rule(ui: &mut Ui, height: f32) {
    let p = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0_f32, p.line),
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Inverted: white on dark, black on light.
    Primary,
    /// Hairline outline.
    Outline,
    /// No frame until hovered.
    Ghost,
    Danger,
}

/// Square mono-caps button, `CONTROL_H` tall.
pub fn button(ui: &mut Ui, icon: Option<&str>, label: &str, kind: Kind) -> Response {
    let p = palette(ui.ctx());
    let (fill, ink, stroke) = match kind {
        Kind::Primary => (p.invert_bg, p.invert_fg, Stroke::NONE),
        Kind::Outline => (
            Color32::TRANSPARENT,
            p.text,
            Stroke::new(1.0_f32, p.line_strong),
        ),
        Kind::Ghost => (Color32::TRANSPARENT, p.text, Stroke::NONE),
        Kind::Danger => (p.danger, Color32::WHITE, Stroke::NONE),
    };
    let g = caps(ui, label, HUD, mono_bold(), ink);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    // Labelled buttons share one minimum width so rows of actions line up.
    let w = if label.is_empty() {
        CONTROL_H
    } else {
        (g.size().x + icon_w + 32.0).max(BUTTON_MIN_W)
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, CONTROL_H), Sense::click());
    let hovered = resp.hovered();
    let fill = match (kind, hovered) {
        (Kind::Primary, true) => p.accent_ink,
        (Kind::Danger, true) => p.danger.gamma_multiply(0.88),
        (Kind::Outline | Kind::Ghost, true) => p.hover,
        _ => fill,
    };
    let stroke = if hovered && kind == Kind::Ghost {
        Stroke::new(1.0_f32, p.line)
    } else {
        stroke
    };
    ui.painter().rect(rect, Rounding::ZERO, fill, stroke);
    let total = g.size().x + icon_w;
    let mut x = rect.center().x - total / 2.0;
    if let Some(ic) = icon {
        ui.painter().text(
            egui::pos2(x + 7.0, rect.center().y),
            Align2::CENTER_CENTER,
            ic,
            FontId::proportional(14.0),
            ink,
        );
        x += icon_w;
    }
    ui.painter()
        .galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, ink);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn primary_button(ui: &mut Ui, icon: &str, label: &str) -> Response {
    button(ui, Some(icon), label, Kind::Primary)
}

pub fn danger_button(ui: &mut Ui, icon: &str, label: &str) -> Response {
    button(ui, Some(icon), label, Kind::Danger)
}

/// Square icon-only header cell.
pub fn icon_cell(ui: &mut Ui, icon: &str, height: f32, active: bool) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(height, height), Sense::click());
    if resp.hovered() || active {
        let r = rect;
        ui.painter()
            .rect_filled(r, Rounding::ZERO, if active { p.sunken } else { p.hover });
    }
    let col = if active {
        p.accent_ink
    } else if resp.hovered() {
        p.strong
    } else {
        p.muted
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(17.0),
        col,
    );
    resp
}

/// Menu item: outline icon above an upper-case bold label, flat until hovered.
/// All items in a menu should get the same `width`; below ~72 px only the icon is drawn.
pub fn nav_item(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    width: f32,
    height: f32,
    active: bool,
    tint: Option<Color32>,
) -> Response {
    let p = palette(ui.ctx());
    let base = tint.unwrap_or(p.text);
    let g = caps(ui, label, 10.0, mono_bold(), base);
    let compact = width < 72.0;
    let w = width;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::click());
    let hovered = resp.hovered();
    // Flat cell, full height; items are divided by hairlines (no gaps, no padding).
    if hovered || active {
        ui.painter()
            .rect_filled(rect, Rounding::ZERO, if active { p.card } else { p.hover });
    }
    if active {
        let bar = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.0), rect.max);
        ui.painter()
            .rect_filled(bar, Rounding::ZERO, tint.unwrap_or(p.accent));
    }
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        Stroke::new(1.0_f32, p.line),
    );
    let icon_col = if hovered || active {
        tint.unwrap_or(p.strong)
    } else {
        tint.unwrap_or(p.text)
    };
    let icon_y = if compact {
        rect.center().y
    } else {
        rect.top() + height * 0.36
    };
    ui.painter().text(
        egui::pos2(rect.center().x, icon_y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(21.0),
        icon_col,
    );
    if !compact {
        let label_col = if hovered || active {
            tint.unwrap_or(p.strong)
        } else {
            tint.unwrap_or(p.muted)
        };
        ui.painter().galley(
            egui::pos2(
                rect.center().x - g.size().x / 2.0,
                rect.top() + height * 0.64,
            ),
            g,
            label_col,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if compact {
        resp.on_hover_text(label.to_uppercase())
    } else {
        resp
    }
}

/// Selectable mono tab (filter cells).
pub fn tab(ui: &mut Ui, label: &str, count: Option<usize>, selected: bool) -> Response {
    let p = palette(ui.ctx());
    let ink = if selected { p.strong } else { p.muted };
    let g = caps(ui, label, HUD, mono_bold(), ink);
    let cg = count.map(|c| {
        caps(
            ui,
            &c.to_string(),
            HUD,
            mono(),
            if selected { p.accent_ink } else { p.faint },
        )
    });
    let extra = cg.as_ref().map_or(0.0, |c| c.size().x + 8.0);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(g.size().x + extra + 24.0, CONTROL_H), Sense::click());
    let fill = if selected {
        p.card
    } else if resp.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if selected {
        Stroke::new(1.0_f32, p.line_strong)
    } else {
        Stroke::NONE
    };
    let _ = stroke;
    ui.painter().rect_filled(rect, Rounding::ZERO, fill);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        Stroke::new(1.0_f32, p.line),
    );
    let y = rect.center().y - g.size().y / 2.0;
    let gx = g.size().x;
    ui.painter()
        .galley(egui::pos2(rect.left() + 12.0, y), g, ink);
    if let Some(c) = cg {
        let col = if selected { p.accent_ink } else { p.faint };
        ui.painter()
            .galley(egui::pos2(rect.left() + 12.0 + gx + 8.0, y), c, col);
    }
    resp
}

/// Segmented-control container for [`tab`]s.
pub fn segmented(p: &Palette) -> Frame {
    Frame::none()
        .fill(p.sunken)
        .stroke(Stroke::new(1.0_f32, p.line))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::same(0.0))
}

/// Hairline-bordered surface.
pub fn card(p: &Palette, selected: bool, hovered: bool) -> Frame {
    Frame::none()
        .fill(if hovered { p.card_hover } else { p.card })
        .rounding(Rounding::ZERO)
        .stroke(Stroke::new(
            1.0_f32,
            if selected { p.accent } else { p.line },
        ))
        .inner_margin(Margin::same(12.0))
}

/// Inner padding of every text input (same height as buttons: [`CONTROL_H`]).
pub const INPUT_MARGIN: egui::Vec2 = egui::Vec2::new(10.0, 8.5);

/// Single-line text input, exactly [`CONTROL_H`] tall.
pub fn text_field(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(INPUT_MARGIN)
        .min_size(vec2(0.0, CONTROL_H))
        .vertical_align(egui::Align::Center)
}

/// Multi-line text input with the same padding as [`text_field`].
pub fn text_area(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text).margin(INPUT_MARGIN)
}

/// A bordered text input frame (search boxes).
pub fn input_frame(p: &Palette) -> Frame {
    Frame::none()
        .fill(p.sunken)
        .stroke(Stroke::new(1.0_f32, p.line))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin {
            left: 10.0,
            right: 4.0,
            top: 0.0,
            bottom: 0.0,
        })
}
