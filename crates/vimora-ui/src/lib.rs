//! Vimora design system for egui.
//!
//! Visual language: Claude-style warm neutrals (dark `#262624`, light `#faf9f5`) with the clay
//! accent, soft rounded corners, hairline borders, Geist text with Geist Mono upper-case labels
//! (ArtCraft-inspired) and Lucide outline icons.

pub mod i18n;
pub mod icons;

pub use i18n::{tr, Lang};

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

/// Font files, for code that draws without egui (the Windows lock screen uses GDI).
pub mod font_files {
    pub const ICONS: &[u8] = super::LUCIDE_TTF;
    pub const REGULAR: &[u8] = super::GEIST;
    pub const BOLD: &[u8] = super::GEIST_BOLD;
    pub const MONO_BOLD: &[u8] = super::GEIST_MONO_BOLD;
}

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
    accent: hex(0x7dba28),
    accent_ink: hex(0x9ad14a),
    invert_bg: hex(0x7dba28),
    invert_fg: hex(0x1a2a08),
    success: hex(0x6fbf73),
    warning: hex(0xe9b44c),
    danger: hex(0xe5685a),
    panel: hex(0x1f1e1d),
    card: hex(0x30302e),
    card_hover: hex(0x363633),
    border: hex(0x3d3c38),
    on_accent: hex(0x1a2a08),
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
    accent: hex(0x7dba28),
    accent_ink: hex(0x4f7a12),
    invert_bg: hex(0x7dba28),
    invert_fg: hex(0x1a2a08),
    success: hex(0x2e7d32),
    warning: hex(0x9a6700),
    danger: hex(0xc0392b),
    panel: hex(0xf5f4ed),
    card: hex(0xffffff),
    card_hover: hex(0xfbfaf7),
    border: hex(0xe3e1d7),
    on_accent: hex(0x1a2a08),
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
        // Lucide right after the text face: egui's emoji fallback also uses the private-use
        // area and would otherwise shadow some icons.
        let mut stack = vec![name.to_string(), "lucide".into()];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(family, stack);
    }
    let mut mono_stack = vec!["GeistMono-Medium".to_string(), "lucide".into()];
    mono_stack.extend(fallback.iter().cloned());
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
        // Tooltips appear at once, even while the pointer is still moving.
        s.interaction.tooltip_delay = 0.0;
        // Texts are not selectable: a row or a title behaves like the button it is.
        s.interaction.selectable_labels = false;
        s.interaction.show_tooltips_only_when_still = false;
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.icon_spacing = 8.0;
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = CONTROL_H;
        s.spacing.menu_margin = Margin::same(6.0);
        s.spacing.window_margin = Margin::same(18.0);
        // Thin, quiet scrollbars: a 2 px line that widens to 6 px under the pointer, drawn in
        // the text colour at low opacity (no light track, also in dark mode).
        s.spacing.scroll = egui::style::ScrollStyle {
            floating: true,
            bar_width: 6.0,
            floating_width: 2.0,
            floating_allocated_width: 0.0,
            handle_min_length: 24.0,
            bar_inner_margin: 2.0,
            bar_outer_margin: 2.0,
            foreground_color: true,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.18,
            active_handle_opacity: 0.32,
            interact_handle_opacity: 0.5,
        };
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
    if label.is_empty() {
        // Icon-only: centre the glyph in the square.
        if let Some(ic) = icon {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                ic,
                FontId::proportional(15.0),
                ink,
            );
        }
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        return resp;
    }
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

/// Height of the compact choice cells ([`tab`]): they sit inside rows and leave room around.
pub const CHOICE_H: f32 = 28.0;

/// Selectable mono cell of a small choice (allowed / read only / blocked, minutes …): compact,
/// the chosen one lifted with an accent line below.
pub fn tab(ui: &mut Ui, label: &str, count: Option<usize>, selected: bool) -> Response {
    let parts = TabParts::new(ui, None, label, count, selected);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(parts.content_w(false) + 23.0, CHOICE_H),
        Sense::click(),
    );
    // 1.5 px each side: 3 px between neighbours, as inside the track.
    parts.paint(ui, rect.shrink2(vec2(1.5, 0.0)), &resp, false, true);
    resp
}

/// [`tab`] with an optional icon before the label.
pub fn tab_icon(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> Response {
    let parts = TabParts::new(ui, icon, label, count, selected);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(parts.content_w(true) + 27.0, CHOICE_H), Sense::click());
    parts.paint(ui, rect.shrink2(vec2(1.5, 0.0)), &resp, true, true);
    resp
}

/// One tab of a strip: its measured texts.
struct TabParts<'a> {
    icon: Option<&'a str>,
    g: Arc<Galley>,
    cg: Option<Arc<Galley>>,
    selected: bool,
}

impl<'a> TabParts<'a> {
    fn new(
        ui: &Ui,
        icon: Option<&'a str>,
        label: &str,
        count: Option<usize>,
        selected: bool,
    ) -> Self {
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
        Self {
            icon,
            g,
            cg,
            selected,
        }
    }

    fn content_w(&self, with_icon: bool) -> f32 {
        let extra = self.cg.as_ref().map_or(0.0, |c| c.size().x + 8.0);
        let icon_w = if with_icon && self.icon.is_some() {
            20.0
        } else {
            0.0
        };
        self.g.size().x + extra + icon_w
    }

    /// Content centred in `rect` (the cell itself). The chosen one and the one under the
    /// pointer look the same: a raised surface with a thin border, bright text and an accent
    /// icon. No lines underneath.
    fn paint(&self, ui: &Ui, rect: Rect, resp: &Response, with_icon: bool, _divider: bool) {
        let p = palette(ui.ctx());
        let lit = self.selected || resp.hovered();
        if lit {
            ui.painter().rect(
                rect,
                Rounding::ZERO,
                p.card,
                Stroke::new(1.0_f32, p.line_strong),
            );
        }
        let ink = if lit { p.strong } else { p.muted };
        let mut x = rect.center().x - self.content_w(with_icon) / 2.0;
        let y = rect.center().y - self.g.size().y / 2.0;
        if let (Some(ic), true) = (self.icon, with_icon) {
            ui.painter().text(
                egui::pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                ic,
                FontId::proportional(13.0),
                if lit { p.accent } else { ink },
            );
            x += 20.0;
        }
        let gx = self.g.size().x;
        ui.painter()
            .galley_with_override_text_color(egui::pos2(x, y), self.g.clone(), ink);
        if let Some(c) = &self.cg {
            let col = if lit { p.accent_ink } else { p.faint };
            ui.painter().galley_with_override_text_color(
                egui::pos2(x + gx + 8.0, y),
                c.clone(),
                col,
            );
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
}

/// Text and icons on an accent-filled surface (a chosen tab, a switched-on switch's knob).
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x14, 0x18, 0x10);

/// A tab of [`tab_strip`].
pub struct TabItem<'a> {
    pub icon: Option<&'a str>,
    pub label: &'a str,
    pub count: Option<usize>,
}

/// The tabs of a drawer: one row across the whole width, never wider than it. Spare room is
/// shared out; when room is short the side padding shrinks first, then the icons go. Returns
/// the index of a clicked tab.
pub fn tab_strip(ui: &mut Ui, items: &[TabItem<'_>], selected: usize) -> Option<usize> {
    let p = palette(ui.ctx());
    let parts: Vec<TabParts> = items
        .iter()
        .enumerate()
        .map(|(i, t)| TabParts::new(ui, t.icon, t.label, t.count, i == selected))
        .collect();
    let base = ui.next_auto_id();
    let avail = ui.available_width();
    let n = parts.len().max(1) as f32;
    let mut with_icon = true;
    let mut content: f32 = parts.iter().map(|t| t.content_w(true)).sum();
    if content + n * 12.0 > avail {
        with_icon = false;
        content = parts.iter().map(|t| t.content_w(false)).sum();
    }
    // A track with 3 px inside and 3 px between the cells: the same space everywhere.
    const PAD: f32 = 3.0;
    let (row, _) = ui.allocate_exact_size(vec2(avail, CONTROL_H), Sense::hover());
    ui.painter()
        .rect(row, Rounding::ZERO, p.sunken, Stroke::new(1.0_f32, p.line));
    let inner = row.shrink(PAD);
    let gaps = PAD * (n - 1.0);
    let spare = ((inner.width() - gaps - content) / n).max(4.0);
    let mut clicked = None;
    let mut x = inner.left();
    for (i, t) in parts.iter().enumerate() {
        let w = if i + 1 == parts.len() {
            inner.right() - x
        } else {
            t.content_w(with_icon) + spare
        };
        let rect = Rect::from_min_size(egui::pos2(x, inner.top()), vec2(w, inner.height()));
        let resp = ui.interact(rect, base.with(("tab-strip", i)), Sense::click());
        t.paint(ui, rect, &resp, with_icon, false);
        if resp.clicked() {
            clicked = Some(i);
        }
        x += w + PAD;
    }
    clicked
}

/// A small choice (allowed / read only / blocked, site / program …): its own track, exactly as
/// big as its cells need, with the same 3 px inside and between them everywhere. Returns the
/// index of a clicked cell.
pub fn choices(ui: &mut Ui, items: &[(Option<&str>, &str)], selected: usize) -> Option<usize> {
    const PAD: f32 = 3.0;
    let p = palette(ui.ctx());
    let parts: Vec<TabParts> = items
        .iter()
        .enumerate()
        .map(|(i, (icon, label))| TabParts::new(ui, *icon, label, None, i == selected))
        .collect();
    // Unique per control (several share one panel): from the id the ui hands out next.
    let base = ui.next_auto_id();
    let widths: Vec<f32> = parts.iter().map(|t| t.content_w(true) + 22.0).collect();
    let total = widths.iter().sum::<f32>() + PAD * (parts.len() as f32 + 1.0);
    let (track, _) = ui.allocate_exact_size(vec2(total, CHOICE_H + 2.0 * PAD), Sense::hover());
    ui.painter().rect(
        track,
        Rounding::ZERO,
        p.sunken,
        Stroke::new(1.0_f32, p.line),
    );
    let mut clicked = None;
    let mut x = track.left() + PAD;
    for (i, (t, w)) in parts.iter().zip(&widths).enumerate() {
        let rect = Rect::from_min_size(egui::pos2(x, track.top() + PAD), vec2(*w, CHOICE_H));
        let resp = ui.interact(rect, base.with(("choice", i)), Sense::click());
        t.paint(ui, rect, &resp, true, false);
        if resp.clicked() {
            clicked = Some(i);
        }
        x += w + PAD;
    }
    clicked
}

/// Several cells that can each be on or off (days of the week …), in the same track as
/// [`choices`]. Returns the index of a clicked cell.
pub fn multi_choices(ui: &mut Ui, items: &[(&str, bool)]) -> Option<usize> {
    const PAD: f32 = 3.0;
    let p = palette(ui.ctx());
    let base = ui.next_auto_id();
    let parts: Vec<TabParts> = items
        .iter()
        .map(|(label, on)| TabParts::new(ui, None, label, None, *on))
        .collect();
    let widths: Vec<f32> = parts.iter().map(|t| t.content_w(false) + 20.0).collect();
    let total = widths.iter().sum::<f32>() + PAD * (parts.len() as f32 + 1.0);
    let (track, _) = ui.allocate_exact_size(vec2(total, CHOICE_H + 2.0 * PAD), Sense::hover());
    ui.painter().rect(
        track,
        Rounding::ZERO,
        p.sunken,
        Stroke::new(1.0_f32, p.line),
    );
    let mut clicked = None;
    let mut x = track.left() + PAD;
    for (i, (t, w)) in parts.iter().zip(&widths).enumerate() {
        let rect = Rect::from_min_size(egui::pos2(x, track.top() + PAD), vec2(*w, CHOICE_H));
        let resp = ui.interact(rect, base.with(("multi", i)), Sense::click());
        t.paint(ui, rect, &resp, false, false);
        if resp.clicked() {
            clicked = Some(i);
        }
        x += w + PAD;
    }
    clicked
}

/// Segmented-control container for [`tab`]s.
pub fn segmented(p: &Palette) -> Frame {
    Frame::none()
        .fill(p.sunken)
        .stroke(Stroke::new(1.0_f32, p.line))
        .rounding(Rounding::ZERO)
        // 3 px around the cells (their own 1.5 px inset makes up the sides).
        .inner_margin(Margin {
            left: 1.5,
            right: 1.5,
            top: 3.0,
            bottom: 3.0,
        })
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

// ---------------------------------------------------------------------------------------------
// Brand mark, info tips, switches and dropdown rows.

/// Paints the Vimora mark (clay tile, two-tone V, square dot) into `rect`.
/// Same geometry as `assets/brand/vimora.svg` (256 grid).
pub fn logo(painter: &egui::Painter, rect: Rect) {
    use egui::{epaint::PathShape, pos2, Pos2};
    let top = hex(0x93cf3f);
    let bottom = hex(0x69a31c);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    let k = rect.width() / 256.0;
    let at = |x: f32, y: f32| -> Pos2 { pos2(rect.left() + x * k, rect.top() + (y + 6.0) * k) };
    let poly = |pts: &[(f32, f32)], c: Color32| {
        let pts = pts.iter().map(|&(x, y)| at(x, y)).collect();
        painter.add(PathShape::convex_polygon(pts, c, Stroke::NONE));
    };
    poly(
        &[(160.0, 70.0), (206.0, 70.0), (147.0, 196.0), (109.0, 196.0)],
        hex(0xe6f4cc),
    );
    poly(
        &[(50.0, 70.0), (96.0, 70.0), (147.0, 196.0), (109.0, 196.0)],
        Color32::WHITE,
    );
    poly(
        &[(110.0, 32.0), (146.0, 32.0), (146.0, 68.0), (110.0, 68.0)],
        Color32::WHITE,
    );
}

/// Small "?" that explains a setting; the tooltip appears at once, with an arrow.
pub fn info(ui: &mut Ui, text: &str) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    let col = if resp.hovered() {
        p.accent_ink
    } else {
        p.faint
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icons::CIRCLE_HELP,
        FontId::proportional(14.0),
        col,
    );
    resp.tip(text)
}

/// On/off switch (square, like the rest of the kit). Returns true when toggled.
/// On: an accent track with a dark knob on the right. Off: an outlined track with a grey knob.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = palette(ui.ctx());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let hover = resp.hovered() && ui.is_enabled();
    if *on {
        ui.painter().rect_filled(rect, Rounding::ZERO, p.accent);
    } else {
        ui.painter().rect(
            rect,
            Rounding::ZERO,
            if hover { p.hover } else { p.sunken },
            Stroke::new(1.0_f32, if hover { p.muted } else { p.line_strong }),
        );
    }
    let knob = Rect::from_min_size(
        egui::pos2(
            rect.left() + 4.0 + t * (rect.width() - 22.0),
            rect.top() + 4.0,
        ),
        vec2(14.0, 14.0),
    );
    ui.painter()
        .rect_filled(knob, Rounding::ZERO, lerp_color(p.faint, ON_ACCENT, t));
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Dock item: icon above a small label inside the navbar dock. Hover and active states animate.
/// Active: lifted surface, 2 px accent bar on top, accent icon.
pub fn dock_item(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    size: egui::Vec2,
    active: bool,
    tint: Option<Color32>,
) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let on = ctx.animate_bool_with_time(resp.id.with("active"), active, 0.15);
    let accent = tint.unwrap_or(p.accent_ink);
    // Surface: transparent → hover → lifted card.
    let base = Color32::TRANSPARENT;
    let hover_fill = p.hover.gamma_multiply(0.9);
    let fill = lerp_color(lerp_color(base, hover_fill, hover), p.card, on);
    ui.painter().rect_filled(rect, Rounding::ZERO, fill);
    if on > 0.0 {
        let w = (rect.width() - 20.0) * on;
        let bar =
            Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 1.0), vec2(w, 2.0));
        ui.painter().rect_filled(bar, Rounding::ZERO, accent);
    }
    let rest_icon = tint.unwrap_or(p.muted);
    let icon_col = lerp_color(
        lerp_color(rest_icon, tint.unwrap_or(p.strong), hover),
        accent,
        on,
    );
    let label_col = lerp_color(lerp_color(p.muted, p.strong, hover), p.strong, on);
    let room = rect.width() - 8.0;
    let mut font = FontId::new(10.5, medium());
    let mut g = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), label_col);
    if g.size().x > room {
        font.size = (10.5 * room / g.size().x).max(8.0);
        g = ui
            .painter()
            .layout_no_wrap(label.to_string(), font, label_col);
    }
    const ICON: f32 = 18.0;
    const GAP: f32 = 4.0;
    let block = ICON + GAP + g.size().y;
    let top = rect.center().y - block / 2.0 + 1.0;
    ui.painter().text(
        egui::pos2(rect.center().x, top + ICON / 2.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(ICON),
        icon_col,
    );
    ui.painter().galley(
        egui::pos2(rect.center().x - g.size().x / 2.0, top + ICON + GAP),
        g,
        label_col,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Linear blend of two colours (premultiplied, so transparent blends cleanly).
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

/// Sidebar row: icon, label and an optional count on the right. Animated hover; the active row
/// is lifted with a 2 px accent bar on its left edge.
pub fn side_row(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    count: Option<usize>,
    active: bool,
    tint: Option<Color32>,
) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let on = ctx.animate_bool_with_time(resp.id.with("active"), active, 0.15);
    let fill = lerp_color(lerp_color(Color32::TRANSPARENT, p.hover, hover), p.card, on);
    ui.painter().rect_filled(rect, Rounding::ZERO, fill);
    if on > 0.0 {
        let h = (rect.height() - 12.0) * on;
        ui.painter().rect_filled(
            Rect::from_center_size(egui::pos2(rect.left() + 1.0, rect.center().y), vec2(2.0, h)),
            Rounding::ZERO,
            p.accent,
        );
    }
    let ink = lerp_color(lerp_color(p.muted, p.strong, hover), p.strong, on);
    let icon_ink = tint.unwrap_or(lerp_color(ink, p.accent_ink, on));
    ui.painter().text(
        egui::pos2(rect.left() + 22.0, rect.center().y),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        icon_ink,
    );
    let mut right = rect.right() - 12.0;
    if let Some(n) = count {
        let g = ui.painter().layout_no_wrap(
            n.to_string(),
            FontId::new(11.5, mono()),
            if active { p.accent_ink } else { p.faint },
        );
        right -= g.size().x;
        ui.painter().galley(
            egui::pos2(right, rect.center().y - g.size().y / 2.0),
            g,
            p.faint,
        );
        right -= 8.0;
    }
    let t = truncated(
        ui,
        label,
        FontId::new(13.5, medium()),
        ink,
        right - rect.left() - 44.0,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 42.0, rect.center().y - t.size().y / 2.0),
        t,
        ink,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

// ---------------------------------------------------------------------------------------------
// Popups with an arrow pointing at the thing they belong to (tooltips and context menus).

/// Height of the arrow and the gap it leaves between the popup and the anchor.
const ARROW_H: f32 = 7.0;

/// Shows a popup next to `anchor` with a small arrow pointing at it. The popup opens below the
/// anchor when it fits there, otherwise above. `x` is where the arrow points horizontally
/// (clamped to the anchor). Returns the popup's rectangle.
pub fn arrow_popup(
    ctx: &egui::Context,
    id: egui::Id,
    anchor: Rect,
    x: f32,
    interactive: bool,
    add_contents: impl FnOnce(&mut Ui),
) -> Rect {
    let p = palette(ctx);
    let screen = ctx.screen_rect();
    let size = ctx
        .data(|d| d.get_temp::<egui::Vec2>(id))
        .unwrap_or(vec2(240.0, 200.0));
    let gap = ARROW_H + 2.0;
    let below = screen.bottom() - anchor.bottom() - gap > size.y + 6.0
        || anchor.top() - screen.top() < size.y + gap + 6.0;
    let ax = x.clamp(
        anchor.left() + 8.0,
        (anchor.right() - 8.0).max(anchor.left() + 8.0),
    );
    let left = (ax - 22.0).clamp(
        screen.left() + 6.0,
        (screen.right() - size.x - 6.0).max(6.0),
    );
    let (pos, pivot) = if below {
        (egui::pos2(left, anchor.bottom() + gap), Align2::LEFT_TOP)
    } else {
        (egui::pos2(left, anchor.top() - gap), Align2::LEFT_BOTTOM)
    };
    let out = egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .pivot(pivot)
        .interactable(interactive)
        .show(ctx, |ui| {
            Frame::none()
                .fill(p.card)
                .stroke(Stroke::new(1.0_f32, p.line_strong))
                .shadow(Shadow {
                    offset: vec2(0.0, 6.0),
                    blur: 18.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(90),
                })
                .inner_margin(Margin::same(6.0))
                .show(ui, add_contents);
        });
    let rect = out.response.rect;
    ctx.data_mut(|d| d.insert_temp(id, rect.size()));

    // Arrow on the edge facing the anchor.
    let ax = ax.clamp(
        rect.left() + 12.0,
        (rect.right() - 12.0).max(rect.left() + 12.0),
    );
    let painter = ctx
        .layer_painter(out.response.layer_id)
        .with_clip_rect(screen.expand(20.0));
    let (edge, tip) = if below {
        (rect.top(), rect.top() - ARROW_H)
    } else {
        (rect.bottom(), rect.bottom() + ARROW_H)
    };
    let (l, r, t) = (
        egui::pos2(ax - ARROW_H, edge),
        egui::pos2(ax + ARROW_H, edge),
        egui::pos2(ax, tip),
    );
    painter.add(egui::epaint::PathShape::convex_polygon(
        vec![l, t, r],
        p.card,
        Stroke::NONE,
    ));
    let line = Stroke::new(1.0_f32, p.line_strong);
    painter.line_segment([l, t], line);
    painter.line_segment([t, r], line);
    // Hide the frame's border under the arrow's base.
    painter.line_segment(
        [
            egui::pos2(ax - ARROW_H + 1.0, edge),
            egui::pos2(ax + ARROW_H - 1.0, edge),
        ],
        Stroke::new(2.0_f32, p.card),
    );
    rect
}

/// `response.tip("text")`: a tooltip with an arrow, shown at once while hovering.
pub trait ResponseExt: Sized {
    /// Tooltip anchored to the widget; the arrow points at the pointer (or the widget's centre).
    fn tip(self, text: impl AsRef<str>) -> Self;
}

impl ResponseExt for Response {
    fn tip(self, text: impl AsRef<str>) -> Self {
        if self.hovered() && !self.ctx.input(|i| i.pointer.any_down()) {
            let p = palette(&self.ctx);
            let x = self.rect.center().x;
            arrow_popup(
                &self.ctx,
                egui::Id::new(("arrow-tip", self.id)),
                self.rect,
                x,
                false,
                |ui| {
                    ui.set_max_width(340.0);
                    ui.label(RichText::new(text.as_ref()).size(12.5).color(p.text));
                },
            );
        }
        self
    }
}

/// Like [`ResponseExt::tip`] but the arrow follows the pointer along the widget (big cards).
pub fn tip_at_pointer(resp: &Response, text: &str) {
    if resp.hovered() && !resp.ctx.input(|i| i.pointer.any_down()) {
        let p = palette(&resp.ctx);
        let x = resp
            .ctx
            .input(|i| i.pointer.hover_pos())
            .map(|pos| pos.x)
            .unwrap_or(resp.rect.center().x);
        arrow_popup(
            &resp.ctx,
            egui::Id::new(("arrow-tip", resp.id)),
            resp.rect,
            x,
            false,
            |ui| {
                ui.set_max_width(340.0);
                ui.label(RichText::new(text).size(12.5).color(p.text));
            },
        );
    }
}

/// The state of a job, drawn the same way everywhere ([`progress`]).
pub enum Progress<'a> {
    /// Running, how far unknown: a spinner and a word.
    Waiting(&'a str),
    /// Running, this far (0–100).
    Percent(f32),
    /// Finished: the bar full.
    Done(&'a str),
    /// Did not work: red, with why.
    Failed(&'a str),
    /// Nothing running: just a word in a colour.
    Idle(&'a str, Color32),
}

/// One progress cell inside `rect` (a table cell, a row's right part).
pub fn progress(ui: &Ui, rect: Rect, state: Progress<'_>) {
    let p = palette(ui.ctx());
    let y = rect.center().y;
    let bar_w = (rect.width() - 64.0).clamp(40.0, 220.0);
    let bar = Rect::from_min_size(egui::pos2(rect.left(), y - 3.0), vec2(bar_w, 6.0));
    let label = |text: &str, color: Color32, x: f32| {
        let g = truncated(
            ui,
            text,
            FontId::proportional(12.5),
            color,
            rect.right() - x - 4.0,
        );
        ui.painter()
            .galley(egui::pos2(x, y - g.size().y / 2.0), g, color);
    };
    match state {
        Progress::Waiting(text) => {
            let s = Rect::from_center_size(egui::pos2(rect.left() + 8.0, y), vec2(14.0, 14.0));
            egui::Spinner::new()
                .size(14.0)
                .color(p.accent)
                .paint_at(ui, s);
            label(text, p.text, rect.left() + 24.0);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
        Progress::Percent(pct) => {
            ui.painter().rect_filled(bar, Rounding::ZERO, p.sunken);
            ui.painter().rect_filled(
                Rect::from_min_size(
                    bar.min,
                    vec2(bar.width() * pct.clamp(0.0, 100.0) / 100.0, 6.0),
                ),
                Rounding::ZERO,
                p.accent,
            );
            ui.painter().text(
                egui::pos2(bar.right() + 10.0, y),
                Align2::LEFT_CENTER,
                format!("{pct:.0}%"),
                FontId::new(12.0, mono()),
                p.accent_ink,
            );
        }
        Progress::Done(text) => {
            ui.painter().rect_filled(bar, Rounding::ZERO, p.accent);
            ui.painter().text(
                egui::pos2(bar.right() + 10.0, y),
                Align2::LEFT_CENTER,
                crate::icons::CHECK,
                FontId::proportional(13.0),
                p.accent_ink,
            );
            if !text.is_empty() {
                label(text, p.success, bar.right() + 28.0);
            }
        }
        Progress::Failed(text) => {
            ui.painter().text(
                egui::pos2(rect.left(), y),
                Align2::LEFT_CENTER,
                crate::icons::TRIANGLE_ALERT,
                FontId::proportional(13.0),
                p.danger,
            );
            label(text, p.danger, rect.left() + 20.0);
        }
        Progress::Idle(text, color) => label(text, color, rect.left()),
    }
}

/// The body of an accordion: opens and closes smoothly (about 0.2 s) instead of appearing at
/// once. The content is drawn in full, measured, and shown through a window whose height follows
/// the open state; fully closed it costs nothing. `id` must be unique per accordion.
pub fn accordion_body(ui: &mut Ui, id: egui::Id, open: bool, add: impl FnOnce(&mut Ui)) {
    let t = ui.ctx().animate_bool_with_time(id, open, 0.2);
    if t <= 0.0 && !open {
        return;
    }
    let height_id = id.with("height");
    let full: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let w = ui.available_width();
    // Ease-out: quick at first, settling softly.
    let eased = 1.0 - (1.0 - t).powi(3);
    let shown = if t >= 1.0 { full } else { full * eased };
    let (window, _) = ui.allocate_exact_size(vec2(w, shown), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(window.min, vec2(w, 6000.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(window.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing.y = 0.0;
    add(&mut child);
    let measured = child.min_rect().height();
    if (measured - full).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(height_id, measured));
        ui.ctx().request_repaint();
    }
    if t > 0.0 && t < 1.0 {
        ui.ctx().request_repaint();
    }
}
