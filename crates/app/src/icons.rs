//! Lucide line icons (ISC), rasterised once per size with resvg and drawn
//! tinted.

use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle, TextureOptions, Ui};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Svg {
    Arrow,
    Rect,
    Highlight,
    Redact,
    Step,
    Text,
    Crop,
    Line,
    Ellipse,
    Pen,
    Spotlight,
    Sticker,
    Undo,
    Redo,
    Copy,
    Save,
    Camera,
}

impl Svg {
    fn body(self) -> &'static str {
        match self {
            Svg::Arrow => r#"<path d="M7 7h10v10"/><path d="M7 17 17 7"/>"#,
            Svg::Rect => r#"<rect width="18" height="18" x="3" y="3" rx="2"/>"#,
            Svg::Highlight => r#"<path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/>"#,
            Svg::Redact => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/><path d="M9 3v18"/><path d="M15 3v18"/>"#,
            Svg::Step => r#"<circle cx="12" cy="12" r="9"/><path d="M10.5 9 12 8v8"/>"#,
            Svg::Text => r#"<polyline points="4 7 4 4 20 4 20 7"/><line x1="9" x2="15" y1="20" y2="20"/><line x1="12" x2="12" y1="4" y2="20"/>"#,
            Svg::Crop => r#"<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>"#,
            Svg::Line => r#"<path d="M5 19 19 5"/>"#,
            Svg::Ellipse => r#"<ellipse cx="12" cy="12" rx="10" ry="7"/>"#,
            Svg::Pen => r#"<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>"#,
            Svg::Spotlight => r#"<circle cx="12" cy="12" r="3"/><path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>"#,
            Svg::Sticker => r#"<circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" x2="9.01" y1="9" y2="9"/><line x1="15" x2="15.01" y1="9" y2="9"/>"#,
            Svg::Undo => r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>"#,
            Svg::Redo => r#"<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"/>"#,
            Svg::Copy => r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#,
            Svg::Save => r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/>"#,
            Svg::Camera => r#"<path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/>"#,
        }
    }

    fn source(self) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
            self.body()
        )
    }
}

fn rasterize(svg: Svg, px: u32) -> Option<ColorImage> {
    let tree = usvg::Tree::from_str(&svg.source(), &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data()))
}

fn texture(ctx: &egui::Context, svg: Svg, size: f32) -> Option<TextureHandle> {
    let px = (size * ctx.pixels_per_point()).round().max(1.0) as u32;
    let id = egui::Id::new(("svg-icon", svg, px));
    ctx.data_mut(|d| d.get_temp::<TextureHandle>(id)).or_else(|| {
        let image = rasterize(svg, px)?;
        let texture = ctx.load_texture(format!("icon-{svg:?}-{px}"), image, TextureOptions::LINEAR);
        ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
        Some(texture)
    })
}

/// The icon as an egui image, for buttons with an icon and a label.
pub fn image(ctx: &egui::Context, svg: Svg, size: f32, tint: Color32) -> egui::Image<'static> {
    match texture(ctx, svg, size) {
        Some(t) => egui::Image::new((t.id(), egui::vec2(size, size))).tint(tint),
        None => egui::Image::new((egui::TextureId::default(), egui::Vec2::ZERO)),
    }
}

pub fn paint(ui: &Ui, svg: Svg, rect: Rect, tint: Color32) {
    if let Some(t) = texture(ui.ctx(), svg, rect.width().max(rect.height())) {
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(t.id(), rect, uv, tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_rasterize() {
        for svg in [Svg::Arrow, Svg::Rect, Svg::Highlight, Svg::Redact, Svg::Step, Svg::Text, Svg::Crop, Svg::Line, Svg::Ellipse, Svg::Pen, Svg::Spotlight, Svg::Sticker, Svg::Undo, Svg::Redo, Svg::Copy, Svg::Save, Svg::Camera] {
            let img = rasterize(svg, 32).expect("valid svg");
            assert!(img.pixels.iter().any(|p| p.a() > 200), "{svg:?} drew something");
        }
    }
}
