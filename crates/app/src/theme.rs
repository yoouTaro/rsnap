//! A quiet graphite palette with one violet accent (rgrip is blue, rjot
//! copper). Dark and light follow the macOS appearance.

#![allow(non_snake_case)]

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Theme,
    ThemePreference, Visuals, vec2,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Palette {
    bg: Color32,
    surface: Color32,
    raise: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    faint: Color32,
    accent: Color32,
    input: Color32,
    widget: [Color32; 3],
    strong: Color32,
    canvas: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

const DARK: Palette = Palette {
    bg: hex(0x141519),
    surface: hex(0x1B1C21),
    raise: hex(0x24252B),
    border: hex(0x2C2E35),
    text: hex(0xE8E9ED),
    muted: hex(0x9FA2AD),
    faint: hex(0x6F727C),
    accent: hex(0x9B8CFF),
    input: hex(0x1E1F24),
    widget: [hex(0x25262C), hex(0x2F3037), hex(0x393A42)],
    strong: hex(0xFFFFFF),
    canvas: hex(0x0E0F12),
};

const LIGHT: Palette = Palette {
    bg: hex(0xF5F5F7),
    surface: hex(0xFFFFFF),
    raise: hex(0xEBEBEF),
    border: hex(0xDFE0E5),
    text: hex(0x1D1E22),
    muted: hex(0x5E616B),
    faint: hex(0x8F929C),
    accent: hex(0x6A55E8),
    input: hex(0xFFFFFF),
    widget: [hex(0xE9E9ED), hex(0xE0E0E5), hex(0xD5D5DB)],
    strong: hex(0x000000),
    canvas: hex(0xE4E4E9),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

fn p() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn BG() -> Color32 { p().bg }
pub fn SURFACE() -> Color32 { p().surface }
pub fn RAISE() -> Color32 { p().raise }
pub fn BORDER() -> Color32 { p().border }
pub fn TEXT() -> Color32 { p().text }
pub fn MUTED() -> Color32 { p().muted }
pub fn FAINT() -> Color32 { p().faint }
pub fn ACCENT() -> Color32 { p().accent }
pub fn CANVAS() -> Color32 { p().canvas }

/// A faint accent wash for banners.
pub fn tint_accent() -> Color32 {
    let (s, a) = (p().surface, p().accent);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * 0.08).round() as u8;
    Color32::from_rgb(mix(s.r(), a.r()), mix(s.g(), a.g()), mix(s.b(), a.b()))
}

pub fn sync(ctx: &egui::Context) {
    IS_DARK.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}

/// SF Pro and SF Mono, with a Khmer font (Kantumruy Pro or Khmer Sangam
/// MN) after them; the same Khmer font shapes text (see shaping.rs).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (family, path, name) in [
        (FontFamily::Proportional, "/System/Library/Fonts/SFNS.ttf", "ui"),
        (FontFamily::Monospace, "/System/Library/Fonts/SFNSMono.ttf", "mono"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            fonts.families.entry(family).or_default().insert(0, name.into());
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let khmer = [
        format!("{home}/Library/Fonts/KantumruyPro-VariableFont_wght.ttf"),
        "/Library/Fonts/KantumruyPro-VariableFont_wght.ttf".to_string(),
        "/System/Library/Fonts/Supplemental/Khmer Sangam MN.ttf".to_string(),
    ];
    if let Some(bytes) = khmer.iter().find_map(|p| std::fs::read(p).ok()) {
        crate::shaping::init(bytes.clone());
        fonts.font_data.insert("khmer".into(), Arc::new(FontData::from_owned(bytes)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let list = fonts.families.entry(family).or_default();
            list.insert(1.min(list.len()), "khmer".into());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn init(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.style_mut_of(Theme::Dark, |s| style_for(s, &DARK, Visuals::dark()));
    ctx.style_mut_of(Theme::Light, |s| style_for(s, &LIGHT, Visuals::light()));
    ctx.set_theme(ThemePreference::System);
    sync(ctx);
}

fn style_for(style: &mut egui::Style, c: &Palette, base: Visuals) {
    let mut v = base;
    v.override_text_color = Some(c.text);
    v.weak_text_color = Some(c.faint);
    v.panel_fill = c.bg;
    v.window_fill = c.surface;
    v.extreme_bg_color = c.input;
    v.window_stroke = Stroke::new(1.0_f32, c.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.hyperlink_color = c.accent;
    v.selection.bg_fill = c.accent.gamma_multiply(0.32);
    v.selection.stroke = Stroke::new(1.0_f32, c.accent);
    v.text_cursor.stroke = Stroke::new(1.5_f32, c.accent);
    v.slider_trailing_fill = true;
    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.text);
    w.noninteractive.corner_radius = r;
    for (state, fill) in [&mut w.inactive, &mut w.hovered, &mut w.active].into_iter().zip(c.widget) {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = r;
        state.expansion = 0.0;
    }
    w.open = w.active;
    w.inactive.fg_stroke = Stroke::new(1.0_f32, c.text);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, c.strong);
    w.active.fg_stroke = Stroke::new(1.0_f32, c.strong);
    w.active.bg_fill = c.accent;
    style.visuals = v;
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(10.0, 4.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.slider_width = 150.0;
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(17.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
    ]
    .into();
}
