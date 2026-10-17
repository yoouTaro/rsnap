//! What the editor builds on top of a screenshot: annotations in image
//! pixels, an optional crop, and the frame style (backdrop, padding,
//! corners, shadow). Plain data, so undo is a clone.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub fn new(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }
}

/// An axis-aligned box from two corners, in any order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Bounds {
    pub fn from_corners(a: Pt, b: Pt) -> Bounds {
        Bounds { x0: a.x.min(b.x), y0: a.y.min(b.y), x1: a.x.max(b.x), y1: a.y.max(b.y) }
    }
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Arrow { from: Pt, to: Pt },
    Rect { a: Pt, b: Pt },
    Highlight { a: Pt, b: Pt },
    /// Pixelated so it can't be read.
    Redact { a: Pt, b: Pt },
    /// A numbered marker.
    Step { at: Pt, n: u32 },
    Text { at: Pt, text: String },
    Line { from: Pt, to: Pt },
    Ellipse { a: Pt, b: Pt },
    /// Freehand, in the order drawn.
    Pen { points: Vec<Pt> },
    /// Dims the shot except this box.
    Spotlight { a: Pt, b: Pt },
    /// An emoji, centered on `at`.
    Sticker { at: Pt, emoji: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub shape: Shape,
    /// RGBA.
    pub color: [u8; 4],
    /// Line width, step and label size, as a multiple of the default.
    #[serde(default = "one")]
    pub size: f32,
}

fn one() -> f32 {
    1.0
}

impl Annotation {
    pub fn new(shape: Shape, color: [u8; 4]) -> Annotation {
        Annotation { shape, color, size: 1.0 }
    }
}

/// Stroke sizes, in toolbar order.
pub const SIZES: [(&str, f32); 3] = [("S", 0.6), ("M", 1.0), ("L", 1.7)];

/// The sticker palette, in panel order.
pub const STICKERS: [&str; 30] = [
    "👉", "👈", "👆", "👇", "✅", "❌", "⚠️", "❗", "❓", "💡", "👍", "👎", "👀", "🤔", "🙏",
    "🔥", "⭐", "❤️", "🎉", "🚀", "🐛", "💯", "✨", "📌", "🆕", "😂", "😍", "😭", "🤯", "💀",
];

/// The annotation colors, in toolbar order.
pub const COLORS: [[u8; 4]; 10] = [
    [255, 69, 58, 255],   // red
    [255, 159, 10, 255],  // orange
    [255, 214, 10, 255],  // yellow
    [48, 209, 88, 255],   // green
    [64, 200, 224, 255],  // teal
    [10, 132, 255, 255],  // blue
    [155, 140, 255, 255], // violet
    [255, 55, 145, 255],  // pink
    [255, 255, 255, 255], // white
    [28, 28, 30, 255],    // ink
];

/// Backgrounds the shot sits on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Backdrop {
    None,
    Solid([u8; 3]),
    /// Top-left to bottom-right.
    Gradient([u8; 3], [u8; 3]),
}

/// The backdrop presets, in panel order: none, then a few gradients and
/// plain colors.
pub const BACKDROPS: [(&str, Backdrop); 9] = [
    ("None", Backdrop::None),
    ("Dusk", Backdrop::Gradient([131, 96, 195], [46, 191, 145])),
    ("Peach", Backdrop::Gradient([255, 154, 139], [255, 106, 136])),
    ("Ocean", Backdrop::Gradient([33, 147, 176], [109, 213, 237])),
    ("Lime", Backdrop::Gradient([168, 224, 99], [86, 171, 47])),
    ("Night", Backdrop::Gradient([35, 37, 46], [67, 71, 92])),
    ("Paper", Backdrop::Solid([242, 240, 235])),
    ("Graphite", Backdrop::Solid([40, 42, 48])),
    ("White", Backdrop::Solid([255, 255, 255])),
];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub backdrop: Backdrop,
    /// Around the shot, as a share of its shorter side (0..=0.3).
    pub padding: f32,
    /// Corner radius of the shot, as a share of its shorter side (0..=0.08).
    pub radius: f32,
    pub shadow: bool,
}

impl Default for Style {
    fn default() -> Style {
        Style { backdrop: BACKDROPS[1].1, padding: 0.08, radius: 0.02, shadow: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub annotations: Vec<Annotation>,
    /// In source image pixels.
    pub crop: Option<Bounds>,
    pub style: Style,
}

impl Doc {
    /// The next step number to place.
    pub fn next_step(&self) -> u32 {
        self.annotations
            .iter()
            .filter_map(|a| if let Shape::Step { n, .. } = a.shape { Some(n) } else { None })
            .max()
            .unwrap_or(0)
            + 1
    }
}

/// Stroke width for an image of this size: readable at any resolution.
pub fn stroke_for(width: u32, height: u32) -> f32 {
    (width.min(height) as f32 / 140.0).clamp(3.0, 16.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_up() {
        let mut d = Doc::default();
        assert_eq!(d.next_step(), 1);
        d.annotations.push(Annotation::new(Shape::Step { at: Pt::new(1.0, 1.0), n: 1 }, COLORS[0]));
        d.annotations.push(Annotation::new(Shape::Step { at: Pt::new(2.0, 2.0), n: 4 }, COLORS[0]));
        assert_eq!(d.next_step(), 5);
    }

    #[test]
    fn bounds_normalise() {
        let b = Bounds::from_corners(Pt::new(10.0, 2.0), Pt::new(4.0, 8.0));
        assert_eq!((b.x0, b.y0, b.width(), b.height()), (4.0, 2.0, 6.0, 6.0));
    }
}
