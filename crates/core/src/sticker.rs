//! Stickers: emoji from the system's color emoji font. The font is large
//! (about 190 MB), so it's memory-mapped on first use and only the glyphs
//! drawn are read; each emoji's bitmap is decoded once and kept.

use image::RgbaImage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const EMOJI_FONT: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";

fn font() -> Option<&'static [u8]> {
    static FONT: OnceLock<Option<memmap2::Mmap>> = OnceLock::new();
    FONT.get_or_init(|| {
        let file = std::fs::File::open(EMOJI_FONT).ok()?;
        // Safety: a system font file, read-only and not rewritten while we run.
        unsafe { memmap2::Mmap::map(&file) }.ok()
    })
    .as_deref()
}

/// The emoji's largest bitmap (sequences like 👍🏽 or ❤️ shape to one glyph),
/// or None if the font isn't there or doesn't have it.
pub fn image(emoji: &str) -> Option<Arc<RgbaImage>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<RgbaImage>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(emoji) {
        return hit.clone();
    }
    let img = load(emoji).map(Arc::new);
    cache.lock().ok()?.insert(emoji.to_string(), img.clone());
    img
}

fn load(emoji: &str) -> Option<RgbaImage> {
    let face = rustybuzz::Face::from_slice(font()?, 0)?;
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(emoji.trim());
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(&face, &[], buffer);
    let glyph = shaped.glyph_infos().iter().map(|g| g.glyph_id).find(|&g| g != 0)?;
    let raster = face.glyph_raster_image(rustybuzz::ttf_parser::GlyphId(glyph as u16), u16::MAX)?;
    if raster.format != rustybuzz::ttf_parser::RasterImageFormat::PNG {
        return None;
    }
    Some(image::load_from_memory_with_format(raster.data, image::ImageFormat::Png).ok()?.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_have_bitmaps() {
        if font().is_none() {
            return;
        }
        for e in crate::doc::STICKERS {
            let img = image(e).unwrap_or_else(|| panic!("{e} has a bitmap"));
            assert!(img.width() >= 64, "{e} is big enough");
        }
        assert!(image("not an emoji").is_none());
    }
}
