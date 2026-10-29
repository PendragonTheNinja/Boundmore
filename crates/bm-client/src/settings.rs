//! What the client remembers between runs: the last account name and
//! server, and the player's options (mouse, view, frame rate). Never the
//! password: it is typed each time (or passed on the command line for
//! testing) and lives only in memory and inside QUIC's TLS.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "client.toml";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub server: String,
    /// The options window's choices (missing in an older file: defaults).
    #[serde(default)]
    pub options: Options,
    /// Players whose chat is hidden here (`/ignore`), lower case.
    #[serde(default)]
    pub ignored: Vec<String>,
}

/// The player's options (Esc twice, or the options window).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Mouse look speed, times the default.
    pub sensitivity: f64,
    /// Mouse forward looks down.
    pub invert_y: bool,
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// Draw the far views (the distant planet in coarse copies).
    pub far_views: bool,
    /// Wait for the screen's refresh (no tearing, steadier frames).
    pub vsync: bool,
    /// Most frames a second; 0 for no cap.
    pub frame_cap: u32,
    /// Shift (or Ctrl) starts a sprint that lasts until you stop moving
    /// forward, instead of being held.
    pub sprint_toggle: bool,
    /// The view widens a little while sprinting, so the sprint shows.
    pub sprint_widen: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            sensitivity: 1.0,
            invert_y: false,
            fov: 75.0,
            far_views: true,
            vsync: true,
            frame_cap: 0,
            sprint_toggle: false,
            sprint_widen: true,
        }
    }
}

impl Options {
    /// The choices held to what the game can use (a hand-edited file may
    /// hold anything).
    pub fn clamped(&self) -> Self {
        Options {
            sensitivity: if self.sensitivity.is_finite() {
                self.sensitivity.clamp(0.1, 5.0)
            } else {
                1.0
            },
            fov: if self.fov.is_finite() {
                self.fov.clamp(50.0, 110.0)
            } else {
                75.0
            },
            frame_cap: if self.frame_cap == 0 {
                0
            } else {
                self.frame_cap.clamp(20, 500)
            },
            ..self.clone()
        }
    }
}

impl Settings {
    /// Reads the file, or defaults if it is missing or unreadable (a broken
    /// settings file must never stop the game from starting).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!("ignoring unreadable settings {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        // Write then rename, so a crash mid-write leaves the old file whole.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

/// Where the settings live: the user's config folder (`%APPDATA%\Boundmore`
/// on Windows, `$XDG_CONFIG_HOME/boundmore` or `~/.config/boundmore`
/// elsewhere), or next to the executable when none is known.
pub fn default_path() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        env("APPDATA").map(|d| d.join("Boundmore"))
    } else {
        env("XDG_CONFIG_HOME")
            .map(|d| d.join("boundmore"))
            .or_else(|| env("HOME").map(|h| h.join(".config").join("boundmore")))
    };
    let dir = dir.unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
            .unwrap_or_default()
    });
    dir.join(FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_file_reads_with_default_options_and_odd_values_are_held_in() {
        let s: Settings = toml::from_str("name = \"a\"\nserver = \"b\"\n").unwrap();
        assert_eq!(s.options, Options::default());
        let odd = Options {
            sensitivity: f64::NAN,
            fov: 400.0,
            frame_cap: 3,
            ..Options::default()
        }
        .clamped();
        assert_eq!((odd.sensitivity, odd.fov, odd.frame_cap), (1.0, 110.0, 20));
    }

    #[test]
    fn settings_round_trip_and_never_hold_a_password() {
        let dir = std::env::temp_dir().join(format!("bm-settings-{}", std::process::id()));
        let path = dir.join("nested").join(FILE);
        let s = Settings {
            name: "Pen \"dragon\"".into(),
            server: "example.org:7777".into(),
            options: Options {
                sensitivity: 1.5,
                invert_y: true,
                fov: 90.0,
                far_views: false,
                vsync: false,
                frame_cap: 144,
                sprint_toggle: true,
                sprint_widen: false,
            },
            ignored: vec!["pest".into()],
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.to_lowercase().contains("password"));
        // A damaged file reads as defaults rather than failing.
        std::fs::write(&path, "name = [").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert_eq!(Settings::load(&dir.join("missing.toml")), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
