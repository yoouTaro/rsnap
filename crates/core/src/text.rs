//! Text for annotations, shaped with rustybuzz (so Khmer stacks and orders
//! correctly) and rasterized with ab_glyph into an alpha mask. Latin and
//! Khmer runs use their own fonts.

use ab_glyph::{Font, FontRef, GlyphId, PxScale, point};

pub struct Fonts {
    latin: &'static [u8],
    khmer: Option<&'static [u8]>,
}

impl Fonts {
    /// Takes the font files' bytes (leaked: fonts live for the whole run).
    pub fn new(latin: Vec<u8>, khmer: Option<Vec<u8>>) -> Fonts {
        Fonts {
            latin: Box::leak(latin.into_boxed_slice()),
            khmer: khmer.map(|k| &*Box::leak(k.into_boxed_slice())),
        }
    }

    /// The macOS system fonts: SF for Latin, Kantumruy Pro or Khmer Sangam
    /// MN for Khmer.
    pub fn system() -> Option<Fonts> {
        let latin = ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]
            .iter()
            .find_map(|p| std::fs::read(p).ok())?;
        let home = std::env::var("HOME").unwrap_or_default();
        let khmer = [
            format!("{home}/Library/Fonts/KantumruyPro-VariableFont_wght.ttf"),
            "/Library/Fonts/KantumruyPro-VariableFont_wght.ttf".to_string(),
            "/System/Library/Fonts/Supplemental/Khmer Sangam MN.ttf".to_string(),
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok());
        Some(Fonts::new(latin, khmer))
    }
}

pub fn is_khmer(c: char) -> bool {
    matches!(c, '\u{1780}'..='\u{17FF}' | '\u{19E0}'..='\u{19FF}')
}

/// Maximal Khmer / non-Khmer runs; spaces and zero-width characters stay
/// in the run they're in.
fn runs(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut current: Option<bool> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() || matches!(c, '\u{200B}'..='\u{200D}') {
            continue;
        }
        let k = is_khmer(c);
        if let Some(cur) = current
            && cur != k
        {
            out.push((&text[start..i], cur));
            start = i;
        }
        current = Some(k);
    }
    if start < text.len() {
        out.push((&text[start..], current.unwrap_or(false)));
    }
    out
}

/// One line of text as coverage (0..1) per pixel, with its size and the
/// baseline's distance from the top.
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<f32>,
}

/// Lay out and rasterize `text` (one line per `\n`) at `px` pixels.
pub fn render(fonts: &Fonts, text: &str, px: f32) -> Mask {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_h = px * 1.3;
    let mut placed: Vec<(&'static [u8], ab_glyph::Glyph)> = Vec::new();
    let mut width: f32 = 0.0;
    for (row, line) in lines.iter().enumerate() {
        let baseline = px * 1.02 + row as f32 * line_h;
        let mut pen = 0.0;
        for (run, khmer) in runs(line) {
            let bytes = if khmer { fonts.khmer.unwrap_or(fonts.latin) } else { fonts.latin };
            let Some(face) = rustybuzz::Face::from_slice(bytes, 0) else { continue };
            let Ok(font) = FontRef::try_from_slice(bytes) else { continue };
            let scale = px / face.units_per_em() as f32;
            let glyph_scale = PxScale::from(px * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0));
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(run);
            buffer.guess_segment_properties();
            buffer.set_flags(rustybuzz::BufferFlags::REMOVE_DEFAULT_IGNORABLES | rustybuzz::BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE);
            let shaped = rustybuzz::shape(&face, &[], buffer);
            for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                if info.glyph_id != 0 {
                    let x = pen + pos.x_offset as f32 * scale;
                    let y = baseline - pos.y_offset as f32 * scale;
                    placed.push((bytes, GlyphId(info.glyph_id as u16).with_scale_and_position(glyph_scale, point(x, y))));
                }
                pen += pos.x_advance as f32 * scale;
            }
        }
        width = width.max(pen);
    }
    let w = width.ceil().max(1.0) as usize + 2;
    let h = (lines.len() as f32 * line_h + px * 0.2).ceil().max(1.0) as usize;
    let mut alpha = vec![0.0f32; w * h];
    for (bytes, glyph) in placed {
        let Ok(font) = FontRef::try_from_slice(bytes) else { continue };
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let b = outline.px_bounds();
        outline.draw(|gx, gy, cov| {
            let x = b.min.x as i32 + gx as i32;
            let y = b.min.y as i32 + gy as i32;
            if (0..w as i32).contains(&x) && (0..h as i32).contains(&y) {
                let a = &mut alpha[y as usize * w + x as usize];
                *a = (*a + cov).min(1.0);
            }
        });
    }
    Mask { width: w, height: h, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_runs() {
        let r: Vec<(&str, bool)> = runs("Fix ខ្មែរ here");
        assert_eq!(r, [("Fix ", false), ("ខ្មែរ ", true), ("here", false)]);
    }

    #[test]
    fn renders_latin_and_khmer() {
        let Some(fonts) = Fonts::system() else { return };
        let m = render(&fonts, "Hello ខ្ញុំ\nsecond line", 40.0);
        assert!(m.width > 100 && m.height > 80, "{}x{}", m.width, m.height);
        assert!(m.alpha.iter().filter(|a| **a > 0.5).count() > 200, "drew glyphs");
    }
}
