//! The compositor: the screenshot, cropped and redacted, on its backdrop
//! with rounded corners and a soft shadow, then the annotations on top.
//! The editor's preview and the copied PNG come from this same function.

use crate::doc::{Annotation, Backdrop, Bounds, Doc, Pt, Shape, stroke_for};
use crate::text::{self, Fonts};
use image::RgbaImage;
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder, Pattern, Pixmap,
    PixmapPaint, Point, SpreadMode, Stroke, Transform,
};

/// Where the shot sits in the output, so the editor can map pointer
/// positions back to source pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// Add to a source-pixel position to get its output position.
    pub offset_x: f32,
    pub offset_y: f32,
    /// The shot's rectangle in the output.
    pub shot: Bounds,
    pub stroke: f32,
}

pub fn layout(src_w: u32, src_h: u32, doc: &Doc) -> Layout {
    let crop = crop_bounds(src_w, src_h, doc);
    let (w, h) = (crop.width().round().max(1.0), crop.height().round().max(1.0));
    let short = w.min(h);
    let pad = if doc.style.backdrop == Backdrop::None { 0.0 } else { (doc.style.padding.clamp(0.0, 0.3) * short).round() };
    Layout {
        width: (w + 2.0 * pad) as u32,
        height: (h + 2.0 * pad) as u32,
        offset_x: pad - crop.x0,
        offset_y: pad - crop.y0,
        shot: Bounds { x0: pad, y0: pad, x1: pad + w, y1: pad + h },
        stroke: stroke_for(w as u32, h as u32),
    }
}

fn crop_bounds(src_w: u32, src_h: u32, doc: &Doc) -> Bounds {
    let full = Bounds { x0: 0.0, y0: 0.0, x1: src_w as f32, y1: src_h as f32 };
    match doc.crop {
        Some(c) => {
            let b = Bounds { x0: c.x0.max(0.0), y0: c.y0.max(0.0), x1: c.x1.min(src_w as f32), y1: c.y1.min(src_h as f32) };
            if b.width() >= 8.0 && b.height() >= 8.0 { b } else { full }
        }
        None => full,
    }
}

/// Render `doc` over `src`. Returns a premultiplied RGBA pixmap.
pub fn render(src: &RgbaImage, doc: &Doc, fonts: Option<&Fonts>) -> Pixmap {
    let lay = layout(src.width(), src.height(), doc);
    let crop = crop_bounds(src.width(), src.height(), doc);

    // The shot itself: cropped, then redactions pixelated into it.
    let mut shot = image::imageops::crop_imm(src, crop.x0 as u32, crop.y0 as u32, lay.shot.width() as u32, lay.shot.height() as u32).to_image();
    for a in &doc.annotations {
        if let Shape::Redact { a: p, b: q } = &a.shape {
            let b = Bounds::from_corners(Pt::new(p.x - crop.x0, p.y - crop.y0), Pt::new(q.x - crop.x0, q.y - crop.y0));
            pixelate(&mut shot, b, (lay.stroke * 3.0).max(8.0) as u32);
        }
    }
    let shot_pm = to_pixmap(&shot);

    let mut out = Pixmap::new(lay.width.max(1), lay.height.max(1)).expect("non-empty canvas");
    let canvas = Bounds { x0: 0.0, y0: 0.0, x1: lay.width as f32, y1: lay.height as f32 };
    match doc.style.backdrop {
        Backdrop::None => {}
        Backdrop::Solid(c) => out.fill(Color::from_rgba8(c[0], c[1], c[2], 255)),
        Backdrop::Gradient(a, b) => {
            let shader = LinearGradient::new(
                Point::from_xy(0.0, 0.0),
                Point::from_xy(canvas.x1, canvas.y1),
                vec![
                    GradientStop::new(0.0, Color::from_rgba8(a[0], a[1], a[2], 255)),
                    GradientStop::new(1.0, Color::from_rgba8(b[0], b[1], b[2], 255)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            );
            if let (Some(shader), Some(rect)) = (shader, tiny_skia::Rect::from_xywh(0.0, 0.0, canvas.x1, canvas.y1)) {
                let paint = Paint { shader, anti_alias: true, ..Paint::default() };
                out.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    }

    let short = lay.shot.width().min(lay.shot.height());
    let radius = (doc.style.radius.clamp(0.0, 0.08) * short).round();
    if doc.style.shadow && doc.style.backdrop != Backdrop::None && lay.shot.x0 > 0.0 {
        shadow(&mut out, lay.shot, radius, lay.shot.x0);
    }
    if let Some(path) = rounded_rect(lay.shot, radius) {
        let shader = Pattern::new(
            shot_pm.as_ref(),
            SpreadMode::Pad,
            tiny_skia::FilterQuality::Nearest,
            1.0,
            Transform::from_translate(lay.shot.x0, lay.shot.y0),
        );
        let paint = Paint { shader, anti_alias: true, ..Paint::default() };
        out.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    let at = |p: Pt| Pt::new(p.x + lay.offset_x, p.y + lay.offset_y);
    let spots: Vec<Bounds> = doc
        .annotations
        .iter()
        .filter_map(|a| if let Shape::Spotlight { a: p, b: q } = &a.shape { Some(Bounds::from_corners(at(*p), at(*q))) } else { None })
        .collect();
    if !spots.is_empty() {
        spotlight(&mut out, lay.shot, radius, &spots, lay.stroke);
    }
    // Highlights first, so marks drawn after them stay crisp.
    for a in &doc.annotations {
        if let Shape::Highlight { a: p, b: q } = &a.shape {
            highlight(&mut out, Bounds::from_corners(at(*p), at(*q)), a.color);
        }
    }
    for a in &doc.annotations {
        draw(&mut out, a, &at, lay.stroke, fonts);
    }
    out
}

fn draw(out: &mut Pixmap, a: &Annotation, at: &impl Fn(Pt) -> Pt, stroke: f32, fonts: Option<&Fonts>) {
    let stroke = stroke * a.size.clamp(0.2, 4.0);
    match &a.shape {
        Shape::Arrow { from, to } => arrow(out, at(*from), at(*to), stroke, a.color),
        Shape::Rect { a: p, b: q } => frame(out, Bounds::from_corners(at(*p), at(*q)), stroke, a.color),
        Shape::Step { at: p, n } => step(out, at(*p), *n, stroke, a.color, fonts),
        Shape::Text { at: p, text } => label(out, at(*p), text, stroke, a.color, fonts),
        Shape::Line { from, to } => line(out, at(*from), at(*to), stroke, a.color),
        Shape::Ellipse { a: p, b: q } => ellipse(out, Bounds::from_corners(at(*p), at(*q)), stroke, a.color),
        Shape::Pen { points } => pen(out, &points.iter().map(|p| at(*p)).collect::<Vec<_>>(), stroke, a.color),
        Shape::Sticker { at: p, emoji } => sticker(out, at(*p), emoji, sticker_size(stroke)),
        Shape::Highlight { .. } | Shape::Redact { .. } | Shape::Spotlight { .. } => {}
    }
}

/// A sticker's width and height in output pixels, for a stroke width
/// (already scaled by the annotation's size).
pub fn sticker_size(stroke: f32) -> f32 {
    stroke * 13.0
}

/// Stroke `path` with a soft offset shadow under it.
fn stroke_with_shadow(out: &mut Pixmap, path: &Path, stroke: f32, color: [u8; 4], round: bool) {
    let mut line = Stroke { width: stroke, ..Stroke::default() };
    if round {
        line.line_cap = LineCap::Round;
        line.line_join = LineJoin::Round;
    }
    out.stroke_path(path, &solid([0, 0, 0, 55]), &line, Transform::from_translate(0.0, stroke * 0.45), None);
    out.stroke_path(path, &solid(color), &line, Transform::identity(), None);
}

fn line(out: &mut Pixmap, from: Pt, to: Pt, stroke: f32, color: [u8; 4]) {
    let mut p = PathBuilder::new();
    p.move_to(from.x, from.y);
    p.line_to(to.x, to.y);
    if let Some(path) = p.finish() {
        stroke_with_shadow(out, &path, stroke, color, true);
    }
}

fn ellipse(out: &mut Pixmap, b: Bounds, stroke: f32, color: [u8; 4]) {
    if b.width() < 2.0 || b.height() < 2.0 {
        return;
    }
    if let Some(path) = tiny_skia::Rect::from_ltrb(b.x0, b.y0, b.x1, b.y1).and_then(PathBuilder::from_oval) {
        stroke_with_shadow(out, &path, stroke, color, false);
    }
}

/// Freehand, smoothed: quadratic curves through the midpoints.
fn pen(out: &mut Pixmap, points: &[Pt], stroke: f32, color: [u8; 4]) {
    let Some(first) = points.first() else { return };
    let mut p = PathBuilder::new();
    p.move_to(first.x, first.y);
    if points.len() == 1 {
        p.line_to(first.x + 0.01, first.y);
    }
    for i in 1..points.len().saturating_sub(1) {
        let (c, n) = (points[i], points[i + 1]);
        p.quad_to(c.x, c.y, (c.x + n.x) / 2.0, (c.y + n.y) / 2.0);
    }
    if let Some(last) = points.last()
        && points.len() > 1
    {
        p.line_to(last.x, last.y);
    }
    if let Some(path) = p.finish() {
        stroke_with_shadow(out, &path, stroke, color, true);
    }
}

fn sticker(out: &mut Pixmap, at: Pt, emoji: &str, size: f32) {
    let Some(img) = crate::sticker::image(emoji) else { return };
    let pm = to_pixmap(&img);
    let scale = size / img.width().max(img.height()) as f32;
    let (w, h) = (img.width() as f32 * scale, img.height() as f32 * scale);
    let (x, y) = (at.x - w / 2.0, at.y - h / 2.0);
    let shader = Pattern::new(pm.as_ref(), SpreadMode::Pad, tiny_skia::FilterQuality::Bicubic, 1.0, Transform::from_row(scale, 0.0, 0.0, scale, x, y));
    if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        out.fill_rect(rect, &Paint { shader, anti_alias: true, ..Paint::default() }, Transform::identity(), None);
    }
}

/// Dim the shot everywhere except the spotlit boxes.
fn spotlight(out: &mut Pixmap, shot: Bounds, radius: f32, spots: &[Bounds], stroke: f32) {
    let Some(mut layer) = Pixmap::new(out.width(), out.height()) else { return };
    if let Some(path) = rounded_rect(shot, radius) {
        layer.fill_path(&path, &solid([0, 0, 0, 150]), FillRule::Winding, Transform::identity(), None);
    }
    let mut clear = solid([0, 0, 0, 255]);
    clear.blend_mode = BlendMode::Clear;
    for s in spots {
        if let Some(path) = rounded_rect(*s, stroke * 1.4) {
            layer.fill_path(&path, &clear, FillRule::Winding, Transform::identity(), None);
        }
    }
    out.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

/// Text on a colored pill, readable on any screenshot.
pub fn label_size(text: &str, stroke: f32) -> f32 {
    let _ = text;
    stroke * 6.0
}

fn label(out: &mut Pixmap, at: Pt, text: &str, stroke: f32, color: [u8; 4], fonts: Option<&Fonts>) {
    let Some(fonts) = fonts else { return };
    if text.trim().is_empty() {
        return;
    }
    let px = label_size(text, stroke);
    let mask = text::render(fonts, text, px);
    let pad_x = px * 0.45;
    let pad_y = px * 0.22;
    let pill = Bounds { x0: at.x, y0: at.y, x1: at.x + mask.width as f32 + 2.0 * pad_x, y1: at.y + mask.height as f32 + 2.0 * pad_y };
    drop_shadow(out, pill, px * 0.35, stroke);
    if let Some(path) = rounded_rect(pill, px * 0.35) {
        out.fill_path(&path, &solid(color), FillRule::Winding, Transform::identity(), None);
    }
    let ink = ink_on(color);
    blit_mask(out, &mask, pill.x0 + pad_x, pill.y0 + pad_y, ink);
}

fn step(out: &mut Pixmap, at: Pt, n: u32, stroke: f32, color: [u8; 4], fonts: Option<&Fonts>) {
    let r = stroke * 3.4;
    let shadow = PathBuilder::from_circle(at.x, at.y + stroke * 0.5, r + stroke * 0.35);
    if let Some(path) = shadow {
        out.fill_path(&path, &solid([0, 0, 0, 70]), FillRule::Winding, Transform::identity(), None);
    }
    if let Some(path) = PathBuilder::from_circle(at.x, at.y, r + stroke * 0.35) {
        out.fill_path(&path, &solid([255, 255, 255, 255]), FillRule::Winding, Transform::identity(), None);
    }
    if let Some(path) = PathBuilder::from_circle(at.x, at.y, r) {
        out.fill_path(&path, &solid(color), FillRule::Winding, Transform::identity(), None);
    }
    if let Some(fonts) = fonts {
        let mask = text::render(fonts, &n.to_string(), r * 1.15);
        blit_mask(out, &mask, at.x - mask.width as f32 / 2.0, at.y - mask.height as f32 / 2.0 + r * 0.04, ink_on(color));
    }
}

fn arrow(out: &mut Pixmap, from: Pt, to: Pt, stroke: f32, color: [u8; 4]) {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 2.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (stroke * 4.6).min(len * 0.6);
    let half = head * 0.58;
    let base = Pt::new(to.x - ux * head, to.y - uy * head);
    let mut shaft = PathBuilder::new();
    shaft.move_to(from.x, from.y);
    shaft.line_to(base.x + ux * head * 0.2, base.y + uy * head * 0.2);
    let mut tip = PathBuilder::new();
    tip.move_to(to.x, to.y);
    tip.line_to(base.x - uy * half, base.y + ux * half);
    tip.line_to(base.x + uy * half, base.y - ux * half);
    tip.close();
    let line = Stroke { width: stroke, line_cap: LineCap::Round, ..Stroke::default() };
    let off = Transform::from_translate(0.0, stroke * 0.45);
    if let (Some(shaft), Some(tip)) = (shaft.finish(), tip.finish()) {
        out.stroke_path(&shaft, &solid([0, 0, 0, 60]), &line, off, None);
        out.fill_path(&tip, &solid([0, 0, 0, 60]), FillRule::Winding, off, None);
        out.stroke_path(&shaft, &solid(color), &line, Transform::identity(), None);
        out.fill_path(&tip, &solid(color), FillRule::Winding, Transform::identity(), None);
    }
}

fn frame(out: &mut Pixmap, b: Bounds, stroke: f32, color: [u8; 4]) {
    if b.width() < 2.0 || b.height() < 2.0 {
        return;
    }
    let Some(path) = rounded_rect(b, stroke * 1.4) else { return };
    let line = Stroke { width: stroke, ..Stroke::default() };
    out.stroke_path(&path, &solid([0, 0, 0, 55]), &line, Transform::from_translate(0.0, stroke * 0.45), None);
    out.stroke_path(&path, &solid(color), &line, Transform::identity(), None);
}

/// A highlighter: multiplied, so text under it stays dark.
fn highlight(out: &mut Pixmap, b: Bounds, color: [u8; 4]) {
    if let Some(rect) = tiny_skia::Rect::from_ltrb(b.x0, b.y0, b.x1, b.y1) {
        let mut paint = solid([color[0], color[1], color[2], 255]);
        paint.blend_mode = BlendMode::Multiply;
        out.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// Replace `b` with blocks of their average color.
pub fn pixelate(img: &mut RgbaImage, b: Bounds, block: u32) {
    let x0 = b.x0.max(0.0) as u32;
    let y0 = b.y0.max(0.0) as u32;
    let x1 = (b.x1.max(0.0) as u32).min(img.width());
    let y1 = (b.y1.max(0.0) as u32).min(img.height());
    let block = block.max(2);
    let mut by = y0;
    while by < y1 {
        let mut bx = x0;
        while bx < x1 {
            let (ex, ey) = ((bx + block).min(x1), (by + block).min(y1));
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in by..ey {
                for x in bx..ex {
                    let p = img.get_pixel(x, y).0;
                    for i in 0..4 {
                        sum[i] += p[i] as u64;
                    }
                    n += 1;
                }
            }
            let avg = image::Rgba([0, 1, 2, 3].map(|i| (sum[i] / n.max(1)) as u8));
            for y in by..ey {
                for x in bx..ex {
                    img.put_pixel(x, y, avg);
                }
            }
            bx += block;
        }
        by += block;
    }
}

/// A soft shadow under the shot: stacked, widening rounded rects.
fn shadow(out: &mut Pixmap, shot: Bounds, radius: f32, pad: f32) {
    let spread = (pad * 0.45).max(4.0);
    let steps = 14;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let grow = spread * t;
        let b = Bounds { x0: shot.x0 - grow, y0: shot.y0 - grow + spread * 0.35, x1: shot.x1 + grow, y1: shot.y1 + grow + spread * 0.35 };
        if let Some(path) = rounded_rect(b, radius + grow) {
            let alpha = (22.0 * (1.0 - t)) as u8;
            out.fill_path(&path, &solid([0, 0, 0, alpha]), FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn drop_shadow(out: &mut Pixmap, b: Bounds, radius: f32, stroke: f32) {
    let b = Bounds { x0: b.x0, y0: b.y0 + stroke * 0.5, x1: b.x1, y1: b.y1 + stroke * 0.5 };
    if let Some(path) = rounded_rect(b, radius) {
        out.fill_path(&path, &solid([0, 0, 0, 60]), FillRule::Winding, Transform::identity(), None);
    }
}

fn solid(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint { anti_alias: true, ..Paint::default() };
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p
}

/// White text, or near-black on light colors (yellow).
fn ink_on(c: [u8; 4]) -> [u8; 4] {
    let lum = 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
    if lum > 170.0 { [28, 28, 30, 255] } else { [255, 255, 255, 255] }
}

fn blit_mask(out: &mut Pixmap, mask: &text::Mask, x: f32, y: f32, color: [u8; 4]) {
    let (ox, oy) = (x.round() as i32, y.round() as i32);
    let (w, h) = (out.width() as i32, out.height() as i32);
    let data = out.data_mut();
    for my in 0..mask.height as i32 {
        for mx in 0..mask.width as i32 {
            let a = mask.alpha[my as usize * mask.width + mx as usize] * color[3] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let (px, py) = (ox + mx, oy + my);
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let i = (py * w + px) as usize * 4;
            // Source-over in premultiplied space.
            for c in 0..3 {
                let s = color[c] as f32 * a;
                data[i + c] = (s + data[i + c] as f32 * (1.0 - a)).round().min(255.0) as u8;
            }
            data[i + 3] = (a * 255.0 + data[i + 3] as f32 * (1.0 - a)).round().min(255.0) as u8;
        }
    }
}

pub fn rounded_rect(b: Bounds, r: f32) -> Option<Path> {
    let r = r.min(b.width() / 2.0).min(b.height() / 2.0).max(0.0);
    if r < 0.5 {
        return Some(PathBuilder::from_rect(tiny_skia::Rect::from_ltrb(b.x0, b.y0, b.x1, b.y1)?));
    }
    // Circular corners from cubic arcs (k = 0.5523).
    let k = r * 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(b.x0 + r, b.y0);
    p.line_to(b.x1 - r, b.y0);
    p.cubic_to(b.x1 - r + k, b.y0, b.x1, b.y0 + r - k, b.x1, b.y0 + r);
    p.line_to(b.x1, b.y1 - r);
    p.cubic_to(b.x1, b.y1 - r + k, b.x1 - r + k, b.y1, b.x1 - r, b.y1);
    p.line_to(b.x0 + r, b.y1);
    p.cubic_to(b.x0 + r - k, b.y1, b.x0, b.y1 - r + k, b.x0, b.y1 - r);
    p.line_to(b.x0, b.y0 + r);
    p.cubic_to(b.x0, b.y0 + r - k, b.x0 + r - k, b.y0, b.x0 + r, b.y0);
    p.close();
    p.finish()
}

/// Straight-alpha RGBA into a premultiplied pixmap.
fn to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut pm = Pixmap::new(img.width().max(1), img.height().max(1)).expect("non-empty image");
    for (dst, src) in pm.data_mut().chunks_exact_mut(4).zip(img.as_raw().chunks_exact(4)) {
        let a = src[3] as u32;
        dst[0] = (src[0] as u32 * a / 255) as u8;
        dst[1] = (src[1] as u32 * a / 255) as u8;
        dst[2] = (src[2] as u32 * a / 255) as u8;
        dst[3] = src[3];
    }
    pm
}

/// Encode as PNG for the clipboard or a file.
pub fn to_png(pm: &Pixmap) -> Result<Vec<u8>, String> {
    pm.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{BACKDROPS, COLORS, Style};

    fn checker(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| if (x / 4 + y / 4) % 2 == 0 { image::Rgba([255, 255, 255, 255]) } else { image::Rgba([0, 0, 0, 255]) })
    }

    #[test]
    fn layout_adds_padding_only_with_a_backdrop() {
        let mut doc = Doc::default();
        doc.style = Style { backdrop: BACKDROPS[1].1, padding: 0.1, radius: 0.0, shadow: false };
        let lay = layout(400, 200, &doc);
        assert_eq!((lay.width, lay.height), (440, 240));
        assert_eq!((lay.offset_x, lay.offset_y), (20.0, 20.0));
        doc.style.backdrop = Backdrop::None;
        assert_eq!(layout(400, 200, &doc).width, 400);
    }

    #[test]
    fn crop_shrinks_and_shifts() {
        let mut doc = Doc { style: Style { backdrop: Backdrop::None, ..Style::default() }, ..Doc::default() };
        doc.crop = Some(Bounds { x0: 50.0, y0: 10.0, x1: 150.0, y1: 60.0 });
        let lay = layout(400, 200, &doc);
        assert_eq!((lay.width, lay.height, lay.offset_x, lay.offset_y), (100, 50, -50.0, -10.0));
        let out = render(&checker(400, 200), &doc, None);
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn redact_destroys_detail() {
        let mut img = checker(64, 64);
        pixelate(&mut img, Bounds { x0: 0.0, y0: 0.0, x1: 32.0, y1: 32.0 }, 16);
        assert_eq!(img.get_pixel(0, 0), img.get_pixel(15, 15), "one block, one color");
        assert_ne!(img.get_pixel(40, 40), img.get_pixel(44, 40), "outside untouched");
    }

    #[test]
    fn annotations_draw_on_the_canvas() {
        let src = RgbaImage::from_pixel(300, 200, image::Rgba([255, 255, 255, 255]));
        let mut doc = Doc { style: Style { backdrop: Backdrop::None, ..Style::default() }, ..Doc::default() };
        doc.annotations.push(Annotation::new(Shape::Arrow { from: Pt::new(20.0, 20.0), to: Pt::new(200.0, 150.0) }, COLORS[0]));
        doc.annotations.push(Annotation::new(Shape::Rect { a: Pt::new(220.0, 20.0), b: Pt::new(280.0, 80.0) }, COLORS[5]));
        let out = render(&src, &doc, None);
        let px = |x: u32, y: u32| out.pixel(x, y).unwrap();
        // The arrow's middle is red; the box's edge is blue; a blank spot stays white.
        let mid = px(110, 85);
        assert!(mid.red() > 200 && mid.green() < 120, "{mid:?}");
        let edge = px(220, 50);
        assert!(edge.blue() > 200 && edge.red() < 120, "{edge:?}");
        assert_eq!(px(150, 30).red(), 255);
    }

    #[test]
    fn text_and_steps_render_with_fonts() {
        let Some(fonts) = Fonts::system() else { return };
        let src = RgbaImage::from_pixel(400, 300, image::Rgba([255, 255, 255, 255]));
        let mut doc = Doc { style: Style { backdrop: Backdrop::None, ..Style::default() }, ..Doc::default() };
        doc.annotations.push(Annotation::new(Shape::Text { at: Pt::new(20.0, 20.0), text: "ចុចទីនេះ Click".into() }, COLORS[5]));
        doc.annotations.push(Annotation::new(Shape::Step { at: Pt::new(300.0, 200.0), n: 1 }, COLORS[0]));
        let out = render(&src, &doc, Some(&fonts));
        assert!(out.pixel(30, 30).unwrap().red() < 100, "pill is blue");
        let step = out.pixel(300 - 8, 200 - 8).unwrap();
        assert!(step.red() > 200, "step is red: {step:?}");
        assert!(to_png(&out).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn new_tools_draw() {
        let src = RgbaImage::from_pixel(400, 300, image::Rgba([255, 255, 255, 255]));
        let mut doc = Doc { style: Style { backdrop: Backdrop::None, ..Style::default() }, ..Doc::default() };
        doc.annotations.push(Annotation::new(Shape::Line { from: Pt::new(10.0, 290.0), to: Pt::new(390.0, 290.0) }, COLORS[3]));
        doc.annotations.push(Annotation::new(Shape::Ellipse { a: Pt::new(20.0, 20.0), b: Pt::new(120.0, 100.0) }, COLORS[5]));
        let pen: Vec<Pt> = (0..20).map(|i| Pt::new(200.0 + i as f32 * 8.0, 40.0 + (i as f32 * 0.6).sin() * 20.0)).collect();
        doc.annotations.push(Annotation::new(Shape::Pen { points: pen }, COLORS[7]));
        doc.annotations.push(Annotation::new(Shape::Spotlight { a: Pt::new(150.0, 150.0), b: Pt::new(250.0, 250.0) }, COLORS[0]));
        let out = render(&src, &doc, None);
        let px = |x: u32, y: u32| out.pixel(x, y).unwrap();
        assert!(px(200, 290).green() > 150 && px(200, 290).red() < 120, "green line");
        assert!(px(70, 20).blue() > 200 && px(70, 20).red() < 120, "blue ellipse top");
        assert_eq!(px(200, 200).red(), 255, "inside the spotlight stays bright");
        assert!(px(380, 150).red() < 150, "outside it is dimmed");
    }

    #[test]
    fn stickers_draw() {
        if crate::sticker::image("✅").is_none() {
            return;
        }
        let src = RgbaImage::from_pixel(300, 300, image::Rgba([255, 255, 255, 255]));
        let mut doc = Doc { style: Style { backdrop: Backdrop::None, ..Style::default() }, ..Doc::default() };
        doc.annotations.push(Annotation::new(Shape::Sticker { at: Pt::new(150.0, 150.0), emoji: "✅".into() }, COLORS[0]));
        let out = render(&src, &doc, None);
        // The ✅ tile is green; its middle is the white tick, so look near a corner.
        let c = out.pixel(136, 136).unwrap();
        assert!(c.green() as i32 > c.red() as i32 + 40, "a green tile: {c:?}");
    }
}
