//! What rsnap remembers between captures: the frame style and the last
//! color, in one small JSON file.

use crate::doc::Style;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub style: Style,
    /// Index into [`crate::doc::COLORS`].
    pub color: usize,
    /// Play a sound when a shot is copied.
    pub sound: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { style: Style::default(), color: 0, sound: true }
    }
}

/// `~/Library/Application Support/dev.rsnap.rsnap/settings.json` on macOS.
pub fn path() -> PathBuf {
    directories::ProjectDirs::from("dev", "rsnap", "rsnap")
        .map(|d| d.data_dir().join("settings.json"))
        .unwrap_or_else(|| PathBuf::from("settings.json"))
}

pub fn load() -> Settings {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
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
        let s: Settings = serde_json::from_str(r#"{"color":3}"#).unwrap();
        assert_eq!(s.color, 3);
        assert!(s.sound);
        assert_eq!(s.style, Style::default());
    }
}
