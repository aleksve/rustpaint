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
pub(crate) struct Prefs {
    pub theme_dark: bool,
    pub native_dialogs: Option<bool>,
    pub last_dir: Option<PathBuf>,
    pub font_family: Option<String>,
    pub recent: Vec<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme_dark: true,
            native_dialogs: None,
            last_dir: None,
            font_family: None,
            recent: Vec::new(),
        }
    }
}

fn config_base() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    }
}

pub(crate) fn config_path() -> Option<PathBuf> {
    let base = config_base()?;
    let dir = if cfg!(any(target_os = "windows", target_os = "macos")) {
        "RustPaint"
    } else {
        "rustpaint"
    };
    Some(base.join(dir).join("settings.ini"))
}

/// On case-sensitive macOS volumes, fall back to the older lowercase folder if
/// the canonical `RustPaint` file has not been written yet.
fn load_path() -> Option<PathBuf> {
    let primary = config_path()?;
    if primary.exists() {
        return Some(primary);
    }
    if cfg!(target_os = "macos")
        && let Some(base) = config_base()
    {
        let legacy = base.join("rustpaint").join("settings.ini");
        if legacy.exists() {
            return Some(legacy);
        }
    }
    Some(primary)
}

pub(crate) fn home() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    } else {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

/// Where file dialogs should start when nothing better is known.
pub(crate) fn default_dir() -> PathBuf {
    if let Some(h) = home() {
        for sub in ["Pictures", "Bilder", "Images"] {
            let p = h.join(sub);
            if p.is_dir() {
                return p;
            }
        }
        if h.is_dir() {
            return h;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Quote a value only when it contains control characters, quotes, or
/// leading/trailing whitespace so ordinary `settings.ini` files stay clean and
/// old unquoted files remain backwards-compatible.
fn encode_value(s: &str) -> String {
    let needs_quotes = s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.starts_with('"')
        || s.contains(['\n', '\r']);
    if !needs_quotes {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn decode_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(ch);
            }
        }
        out
    } else {
        trimmed.to_string()
    }
}

impl Prefs {
    pub fn load() -> Self {
        let Some(path) = load_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::parse(&text, true)
    }

    fn parse(text: &str, check_dirs: bool) -> Self {
        let mut p = Prefs::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, raw_v)) = line.split_once('=') else {
                continue;
            };
            let v = decode_value(raw_v);
            match k.trim() {
                "theme" => p.theme_dark = v != "light",
                "native_dialogs" => {
                    p.native_dialogs = match v.as_str() {
                        "true" | "1" => Some(true),
                        "false" | "0" => Some(false),
                        _ => None,
                    };
                }
                "last_dir" if !v.is_empty() => {
                    let d = PathBuf::from(v);
                    if !check_dirs || d.is_dir() {
                        p.last_dir = Some(d);
                    }
                }
                "font" if !v.is_empty() => {
                    p.font_family = Some(v);
                }
                "recent" if !v.is_empty() => {
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

    fn format_ini(&self) -> String {
        let mut out = String::from("# RustPaint settings\n");
        out.push_str(&format!(
            "theme={}\n",
            if self.theme_dark { "dark" } else { "light" }
        ));
        if let Some(native) = self.native_dialogs {
            out.push_str(&format!("native_dialogs={native}\n"));
        }
        if let Some(d) = &self.last_dir {
            out.push_str(&format!(
                "last_dir={}\n",
                encode_value(&d.to_string_lossy())
            ));
        }
        if let Some(f) = &self.font_family {
            out.push_str(&format!("font={}\n", encode_value(f)));
        }
        for r in &self.recent {
            out.push_str(&format!("recent={}\n", encode_value(&r.to_string_lossy())));
        }
        out
    }

    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            return;
        }
        let _ = crate::app::write_bytes_atomically(&path, self.format_ini().as_bytes());
    }

    /// Remember a file the user just opened or saved.
    pub fn push_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
        self.last_dir = self.recent[0].parent().map(Path::to_path_buf);
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

/// Shorten a path for a menu row: `holiday/photo.png`.
pub(crate) fn shorten(p: &Path, max: usize) -> String {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string());
    let parent = p
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned());
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
        assert!(p.recent[0].ends_with("img11.png"));
        p.push_recent(Path::new("/tmp/img9.png"));
        assert_eq!(p.recent[0], PathBuf::from("/tmp/img9.png"));
        assert_eq!(
            p.recent.iter().filter(|x| x.ends_with("img9.png")).count(),
            1
        );
    }

    #[test]
    fn ini_roundtrips_whitespace_windows_paths_and_dialog_pref() {
        let orig = Prefs {
            theme_dark: false,
            native_dialogs: Some(false),
            last_dir: Some(PathBuf::from(r"C:\Users\Me\My Pictures ")),
            font_family: Some(" Noto Sans \nSpecial ".into()),
            recent: vec![
                PathBuf::from("/tmp/ trailing space.png "),
                PathBuf::from(r"C:\art\sunset.png"),
            ],
        };
        let parsed = Prefs::parse(&orig.format_ini(), false);
        assert!(!parsed.theme_dark);
        assert_eq!(parsed.native_dialogs, Some(false));
        assert_eq!(parsed.last_dir, orig.last_dir);
        assert_eq!(parsed.font_family, orig.font_family);
        assert_eq!(parsed.recent, orig.recent);
    }

    #[test]
    fn shorten_uses_parent_folder() {
        let s = shorten(Path::new("/home/me/Pictures/holiday/photo.png"), 40);
        assert_eq!(s, "holiday/photo.png");
    }
}
