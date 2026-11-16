//! Tiny persistent settings store.
//!
//! Plain `key=value` text in the platform config directory — no `serde`, no
//! `dirs` crate, no registry, and easy to inspect or delete by hand:
//!
//! * Windows  `%APPDATA%\RustPaint\settings.ini`
//! * macOS    `~/Library/Application Support/RustPaint/settings.ini`
//! * Linux    `$XDG_CONFIG_HOME/rustpaint/settings.ini` (or `~/.config/…`)

use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 8;

#[derive(Clone, Debug)]
pub struct Prefs {
    pub theme_dark: bool,
    pub last_dir: Option<PathBuf>,
    pub font_family: Option<String>,
    pub recent: Vec<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme_dark: true,
            last_dir: None,
            font_family: None,
            recent: Vec::new(),
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    }?;
    let dir = base.join(if cfg!(target_os = "windows") {
        "RustPaint"
    } else {
        "rustpaint"
    });
    Some(dir.join("settings.ini"))
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Where file dialogs should start when nothing better is known.
pub fn default_dir() -> PathBuf {
    if let Some(h) = home() {
        for sub in ["Pictures", "Bilder", "Images"] {
            let p = h.join(sub);
            if p.is_dir() {
                return p;
            }
        }
        return h;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

impl Prefs {
    pub fn load() -> Self {
        let mut p = Prefs::default();
        let Some(path) = config_path() else { return p };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return p;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "theme" => p.theme_dark = v != "light",
                "last_dir" => {
                    let d = PathBuf::from(v);
                    if d.is_dir() {
                        p.last_dir = Some(d);
                    }
                }
                "font" => {
                    if !v.is_empty() {
                        p.font_family = Some(v.to_string());
                    }
                }
                "recent" => {
                    let r = PathBuf::from(v);
                    if p.recent.len() < MAX_RECENT && !p.recent.contains(&r) {
                        p.recent.push(r);
                    }
                }
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut out = String::from("# RustPaint settings\n");
        out.push_str(&format!(
            "theme={}\n",
            if self.theme_dark { "dark" } else { "light" }
        ));
        if let Some(d) = &self.last_dir {
            out.push_str(&format!("last_dir={}\n", d.display()));
        }
        if let Some(f) = &self.font_family {
            out.push_str(&format!("font={f}\n"));
        }
        for r in &self.recent {
            out.push_str(&format!("recent={}\n", r.display()));
        }
        let _ = std::fs::write(&path, out);
    }

    /// Remember a file the user just opened or saved.
    pub fn push_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
        self.last_dir = path_of_dir(&self.recent[0]);
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

fn path_of_dir(p: &Path) -> Option<PathBuf> {
    p.parent().map(|d| d.to_path_buf())
}

/// Shorten a path for a menu row: `…/holiday/photo.png`.
pub fn shorten(p: &Path, max: usize) -> String {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string());
    let parent = p
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string());
    match parent {
        Some(dir) if name.chars().count() + dir.chars().count() + 4 <= max => {
            format!("{dir}/{name}")
        }
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_is_deduped_and_capped() {
        let mut p = Prefs::default();
        for i in 0..(MAX_RECENT + 4) {
            p.push_recent(Path::new(&format!("/tmp/img{i}.png")));
        }
        assert_eq!(p.recent.len(), MAX_RECENT);
        // newest first
        assert!(p.recent[0].ends_with("img11.png"));
        // re-opening an old file moves it to the front instead of duplicating
        p.push_recent(Path::new("/tmp/img9.png"));
        assert_eq!(p.recent[0], PathBuf::from("/tmp/img9.png"));
        assert_eq!(
            p.recent.iter().filter(|x| x.ends_with("img9.png")).count(),
            1
        );
    }

    #[test]
    fn shorten_uses_parent_folder() {
        let s = shorten(Path::new("/home/me/Pictures/holiday/photo.png"), 40);
        assert_eq!(s, "holiday/photo.png");
    }
}
