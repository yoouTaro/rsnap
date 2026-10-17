//! What rsnap remembers between captures: the frame style and the last
//! color, in one small JSON file.

use crate::doc::Style;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub style: Style,
    /// Index into [`crate::doc::COLORS`]; one past the end means `custom`.
    pub color: usize,
    pub custom: [u8; 3],
    /// Index into [`crate::doc::SIZES`].
    pub size: usize,
    /// The sticker the Sticker tool places.
    pub sticker: String,
    /// Play a sound when a shot is copied.
    pub sound: bool,
    /// 2 once `color` indexes the ten-color palette (0.0.2); older files
    /// index the first six-color one.
    #[serde(default)]
    pub palette: u8,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { style: Style::default(), color: 0, custom: [94, 92, 230], size: 1, sticker: "👉".into(), sound: true, palette: 2 }
    }
}

/// `~/Library/Application Support/dev.rsnap.rsnap/settings.json` on macOS.
pub fn path() -> PathBuf {
    directories::ProjectDirs::from("dev", "rsnap", "rsnap")
        .map(|d| d.data_dir().join("settings.json"))
        .unwrap_or_else(|| PathBuf::from("settings.json"))
}

impl Settings {
    /// The current annotation color.
    pub fn rgba(&self) -> [u8; 4] {
        let [r, g, b] = self.custom;
        crate::doc::COLORS.get(self.color).copied().unwrap_or([r, g, b, 255])
    }

    pub fn stroke(&self) -> f32 {
        crate::doc::SIZES.get(self.size).map_or(1.0, |s| s.1)
    }
}

pub fn load() -> Settings {
    std::fs::read(path()).ok().and_then(|b| parse(&b)).unwrap_or_default()
}

fn parse(bytes: &[u8]) -> Option<Settings> {
    let mut s: Settings = serde_json::from_slice(bytes).ok()?;
    if s.palette < 2 {
        // Blue and ink moved when teal, violet, pink and white came in.
        s.color = match s.color {
            4 => 5,
            5 => 9,
            c => c,
        };
        s.palette = 2;
    }
    Some(s)
}

pub fn save(s: &Settings) -> std::io::Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_or_partial_files_get_defaults() {
        let s = parse(br#"{"color":3}"#).unwrap();
        assert_eq!(s.color, 3);
        assert_eq!(s.sticker, "👉");
        assert_eq!(parse(br#"{"color":4}"#).unwrap().color, 5, "old blue stays blue");
        assert_eq!(parse(br#"{"color":5}"#).unwrap().color, 9, "old ink stays ink");
        assert_eq!(parse(br#"{"color":4,"palette":2}"#).unwrap().color, 4);
        assert!(s.sound);
        assert_eq!(s.style, Style::default());
    }
}
