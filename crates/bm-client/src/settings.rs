//! The few things the client remembers between runs: the last account name
//! and server. Never the password: it is typed each time (or passed on the
//! command line for testing) and lives only in memory and inside QUIC's TLS.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "client.toml";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub server: String,
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
    fn settings_round_trip_and_never_hold_a_password() {
        let dir = std::env::temp_dir().join(format!("bm-settings-{}", std::process::id()));
        let path = dir.join("nested").join(FILE);
        let s = Settings {
            name: "Pen \"dragon\"".into(),
            server: "example.org:7777".into(),
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
