//! Built-in themed file browser.
//!
//! Used automatically when no native platform file dialog is available (for
//! example headless sessions or Linux window managers without an XDG desktop
//! portal), or whenever the user unchecks *File → Use system file dialogs*.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Context, FontId, Pos2, Rect, Sense, Vec2};

use crate::icons::{self, Icon};
use crate::theme::Theme;
use crate::widgets::{self, ButtonKind, rr};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Open,
    Save,
}

pub(crate) enum FileAction {
    None,
    Cancel,
    Accept {
        path: PathBuf,
        ext: &'static str,
        save: bool,
    },
}

struct Entry {
    name: String,
    path: PathBuf,
    dir: bool,
    size: u64,
}

pub(crate) struct FileDialog {
    mode: Mode,
    dir: PathBuf,
    name: String,
    entries: Vec<Entry>,
    /// Sidebar places cached on dialog open (never re-probed every frame).
    places: Vec<(String, PathBuf)>,
    sel: Option<PathBuf>,
    error: Option<String>,
    /// Set when the user clicks Save for an already-existing file path.
    confirm_overwrite: Option<(PathBuf, &'static str)>,
    fmt: usize,
    path_edit: String,
}

const SAVE_FORMATS: &[(&str, &str)] = &[("PNG", "png"), ("JPEG", "jpg"), ("BMP", "bmp")];
pub(crate) const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp"];

pub(crate) fn is_supported_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(ext))
}

fn format_index_for_extension(ext: &str) -> Option<usize> {
    if ext.eq_ignore_ascii_case("png") {
        Some(0)
    } else if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
        Some(1)
    } else if ext.eq_ignore_ascii_case("bmp") {
        Some(2)
    } else {
        None
    }
}

fn home_dir() -> PathBuf {
    crate::prefs::home().unwrap_or_else(|| PathBuf::from("/"))
}

fn collect_places() -> Vec<(String, PathBuf)> {
    let home = home_dir();
    let mut out = Vec::with_capacity(10);
    if home.is_dir() {
        out.push(("Home".to_string(), home.clone()));
    }
    for (label, sub) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures", "Pictures"),
    ] {
        let p = home.join(sub);
        if p.is_dir() {
            out.push((label.to_string(), p));
        }
    }
    if let Ok(cwd) = std::env::current_dir()
        && !out.iter().any(|(_, existing)| existing == &cwd)
    {
        out.push(("Working dir".to_string(), cwd));
    }
    #[cfg(windows)]
    {
        for l in 'A'..='Z' {
            let p = PathBuf::from(format!("{l}:\\"));
            if p.is_dir() {
                out.push((format!("{l}:"), p));
            }
        }
    }
    #[cfg(not(windows))]
    {
        out.push(("Root".to_string(), PathBuf::from("/")));
    }
    out
}

/// Resolve the folder a dialog should open in: the caller's suggestion (last
/// used folder) if it still exists, otherwise the usual pictures/home folders.
fn start_dir(hint: Option<PathBuf>) -> PathBuf {
    if let Some(d) = hint
        && d.is_dir()
    {
        return d;
    }
    let d = crate::prefs::default_dir();
    if d.is_dir() {
        return d;
    }
    std::env::current_dir().unwrap_or_else(|_| home_dir())
}

/// Normalise a typed save filename so it ends with `.{ext}` (or `.jpeg` when
/// saving JPEG) without stripping non-image dot suffixes such as `project.v2`.
fn normalize_save_name(raw: &str, ext: &str) -> Option<String> {
    let mut name = raw.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let current_ext = Path::new(&name).extension().and_then(|e| e.to_str());
    let already_matches = match current_ext {
        Some(e) if ext == "jpg" => e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"),
        Some(e) => e.eq_ignore_ascii_case(ext),
        None => false,
    };
    if !already_matches {
        // Only strip a trailing known image extension; leave names like `logo.v2` intact.
        if let Some(e) = current_ext
            && is_supported_extension(e)
            && let Some(dot) = name.rfind('.')
            && !name[dot..].contains(['/', '\\'])
        {
            name.truncate(dot);
        }
        name.push('.');
        name.push_str(ext);
    }
    Some(name)
}

impl FileDialog {
    pub fn open(hint: Option<PathBuf>) -> Self {
        let dir = start_dir(hint);
        let mut d = Self {
            mode: Mode::Open,
            name: String::new(),
            path_edit: dir.display().to_string(),
            dir,
            entries: Vec::new(),
            places: collect_places(),
            sel: None,
            error: None,
            confirm_overwrite: None,
            fmt: 0,
        };
        d.refresh();
        d
    }

    pub fn save(default_name: &str, hint: Option<PathBuf>) -> Self {
        let dir = start_dir(hint);
        let fmt = Path::new(default_name)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(format_index_for_extension)
            .unwrap_or(0);
        let mut d = Self {
            mode: Mode::Save,
            name: default_name.to_string(),
            path_edit: dir.display().to_string(),
            dir,
            entries: Vec::new(),
            places: collect_places(),
            sel: None,
            error: None,
            confirm_overwrite: None,
            fmt,
        };
        d.refresh();
        d
    }

    fn allowed(&self, p: &Path) -> bool {
        if self.mode == Mode::Save {
            return true;
        }
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_supported_extension)
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.confirm_overwrite = None;
        self.path_edit = self.dir.display().to_string();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) => {
                self.error = Some(format!("Cannot read folder: {e}"));
                return;
            }
        };
        self.error = None;
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().map_or_else(|| path.is_dir(), |m| m.is_dir());
            if !is_dir && !self.allowed(&path) {
                continue;
            }
            let size = meta.map_or(0, |m| m.len());
            self.entries.push(Entry {
                name,
                path,
                dir: is_dir,
                size,
            });
        }
        self.entries
            .sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase()));
    }

    fn nav_to(&mut self, p: PathBuf) {
        if p.is_dir() {
            self.dir = p;
            self.sel = None;
            self.refresh();
        } else {
            self.error = Some(format!("Folder not found: {}", p.display()));
        }
    }

    fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            self.nav_to(parent);
        }
    }

    fn accept(&mut self, force_overwrite: bool) -> FileAction {
        let ext = SAVE_FORMATS[self.fmt].1;
        match self.mode {
            Mode::Open => {
                let candidate = match &self.sel {
                    Some(p) if p.is_file() => Some(p.clone()),
                    _ => {
                        let typed = self.name.trim();
                        if typed.is_empty() {
                            None
                        } else {
                            let p = self.dir.join(typed);
                            if p.is_dir() {
                                self.nav_to(p);
                                return FileAction::None;
                            }
                            p.is_file().then_some(p)
                        }
                    }
                };
                match candidate {
                    Some(path) => FileAction::Accept {
                        path,
                        ext,
                        save: false,
                    },
                    None => {
                        self.error = Some("Select a file to open".into());
                        FileAction::None
                    }
                }
            }
            Mode::Save => {
                let Some(name) = normalize_save_name(&self.name, ext) else {
                    self.error = Some("Enter a file name".into());
                    return FileAction::None;
                };
                let path = self.dir.join(name);
                if path.is_dir() {
                    self.error = Some("A folder with that name already exists".into());
                    return FileAction::None;
                }
                if path.exists() && !force_overwrite {
                    self.error = None;
                    self.confirm_overwrite = Some((path, ext));
                    return FileAction::None;
                }
                self.confirm_overwrite = None;
                FileAction::Accept {
                    path,
                    ext,
                    save: true,
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &Context, th: &Theme) -> FileAction {
        let size = Vec2::new(760.0, 520.0);
        let mut action = FileAction::None;
        let mut cancel = false;

        widgets::modal(ctx, th, "file", size, |ui| {
            let title = if self.mode == Mode::Open {
                "Open image"
            } else {
                "Save image"
            };
            if widgets::modal_header(ui, th, title) {
                cancel = true;
            }

            // ---- location row
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, th, Icon::Upload, 28.0, false)
                    .on_hover_text("Parent folder")
                    .clicked()
                {
                    self.up();
                }
                if widgets::icon_button(ui, th, Icon::Folder, 28.0, false)
                    .on_hover_text("Home folder")
                    .clicked()
                {
                    self.nav_to(home_dir());
                }
                let w = (ui.available_width() - 8.0).max(120.0);
                let resp = ui.add_sized(
                    Vec2::new(w, 28.0),
                    egui::TextEdit::singleline(&mut self.path_edit)
                        .desired_width(w)
                        .font(FontId::proportional(12.0)),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let p = PathBuf::from(self.path_edit.trim());
                    self.nav_to(p);
                }
            });
            ui.add_space(6.0);

            // ---- body (places + virtualised file list)
            let body_h = size.y - 216.0;
            let mut go_to: Option<PathBuf> = None;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(130.0, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        places_ui(ui, th, &self.places, &self.dir, &mut go_to);
                    },
                );
                ui.add_space(4.0);
                let list_w = ui.available_width();
                ui.allocate_ui_with_layout(
                    Vec2::new(list_w, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let row_h = 26.0;
                        let mut navigate: Option<PathBuf> = None;
                        let mut pick: Option<(PathBuf, String)> = None;
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(body_h)
                            .show_rows(ui, row_h, self.entries.len(), |ui, row_range| {
                                ui.set_width(ui.available_width());
                                for idx in row_range {
                                    let e = &self.entries[idx];
                                    let selected = self.sel.as_deref() == Some(e.path.as_path());
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), row_h),
                                        Sense::click(),
                                    );
                                    let p = ui.painter();
                                    if selected {
                                        p.rect_filled(rect, rr(6), th.accent_soft);
                                    } else if resp.hovered() {
                                        p.rect_filled(rect, rr(6), th.hover);
                                    }
                                    let ic = if e.dir { Icon::Folder } else { Icon::File };
                                    let ir = Rect::from_center_size(
                                        Pos2::new(rect.left() + 14.0, rect.center().y),
                                        Vec2::splat(14.0),
                                    );
                                    icons::draw(
                                        p,
                                        ic,
                                        ir,
                                        if e.dir { th.accent } else { th.text_dim },
                                        1.4,
                                    );

                                    // Clip the filename column so long names never overwrite size.
                                    let name_right = if e.dir {
                                        rect.right() - 8.0
                                    } else {
                                        rect.right() - 78.0
                                    };
                                    let name_clip = Rect::from_min_max(
                                        Pos2::new(rect.left() + 26.0, rect.top()),
                                        Pos2::new(
                                            name_right.max(rect.left() + 40.0),
                                            rect.bottom(),
                                        ),
                                    );
                                    let name_painter =
                                        p.with_clip_rect(name_clip.intersect(p.clip_rect()));
                                    if e.dir {
                                        name_painter.text(
                                            Pos2::new(rect.left() + 28.0, rect.center().y),
                                            egui::Align2::LEFT_CENTER,
                                            format!("{}/", e.name),
                                            FontId::proportional(12.5),
                                            th.text,
                                        );
                                    } else {
                                        name_painter.text(
                                            Pos2::new(rect.left() + 28.0, rect.center().y),
                                            egui::Align2::LEFT_CENTER,
                                            &e.name,
                                            FontId::proportional(12.5),
                                            th.text,
                                        );
                                        let kb = e.size as f64 / 1024.0;
                                        let txt = if kb >= 1024.0 {
                                            format!("{:.1} MB", kb / 1024.0)
                                        } else {
                                            format!("{:.0} KB", kb.max(1.0))
                                        };
                                        p.text(
                                            Pos2::new(rect.right() - 10.0, rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            txt,
                                            FontId::proportional(11.0),
                                            th.text_dim,
                                        );
                                    }

                                    if resp.double_clicked() {
                                        if e.dir {
                                            navigate = Some(e.path.clone());
                                        } else if self.mode == Mode::Open {
                                            action = FileAction::Accept {
                                                path: e.path.clone(),
                                                ext: "",
                                                save: false,
                                            };
                                        } else {
                                            pick = Some((e.path.clone(), e.name.clone()));
                                        }
                                    } else if resp.clicked() {
                                        if e.dir {
                                            navigate = Some(e.path.clone());
                                        } else {
                                            pick = Some((e.path.clone(), e.name.clone()));
                                        }
                                    }
                                }
                            });
                        if let Some(p) = navigate {
                            self.nav_to(p);
                        }
                        if let Some((path, name)) = pick {
                            self.sel = Some(path);
                            self.name = name;
                            self.confirm_overwrite = None;
                            if self.mode == Mode::Save
                                && let Some(i) = Path::new(&self.name)
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .and_then(format_index_for_extension)
                            {
                                self.fmt = i;
                            }
                        }
                    },
                );
            });

            if let Some(p) = go_to {
                self.nav_to(p);
            }

            ui.add_space(6.0);

            // ---- file name + format + buttons
            let mut submit_from_enter = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Name").size(12.0).color(th.text_dim));
                let reserve = if self.mode == Mode::Save { 200.0 } else { 12.0 };
                let w = (ui.available_width() - reserve).max(140.0);
                let name_resp = ui.add_sized(
                    Vec2::new(w, 28.0),
                    egui::TextEdit::singleline(&mut self.name)
                        .desired_width(w)
                        .hint_text(if self.mode == Mode::Save {
                            "image.png"
                        } else {
                            "file name"
                        })
                        .font(FontId::proportional(12.5)),
                );
                if name_resp.changed() {
                    self.confirm_overwrite = None;
                }
                if name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit_from_enter = true;
                }
                if self.mode == Mode::Save {
                    for (i, (label, _)) in SAVE_FORMATS.iter().enumerate() {
                        let active = self.fmt == i;
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::new(40.0, 28.0), Sense::click());
                        let bg = if active { th.accent } else { th.raised };
                        let fg = if active { th.on_accent } else { th.text_dim };
                        ui.painter().rect_filled(rect, rr(7), bg);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            *label,
                            FontId::proportional(11.0),
                            fg,
                        );
                        if resp.clicked() && self.fmt != i {
                            self.fmt = i;
                            self.confirm_overwrite = None;
                        }
                    }
                }
            });

            if submit_from_enter {
                let a = self.accept(false);
                if !matches!(a, FileAction::None) {
                    action = a;
                }
            }

            if let Some(err) = &self.error {
                ui.label(egui::RichText::new(err).size(11.5).color(th.danger));
            }

            ui.add_space(6.0);
            if let Some((overwrite_path, overwrite_ext)) = self.confirm_overwrite.clone() {
                let fname = overwrite_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| overwrite_path.display().to_string());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Replace existing “{fname}”?"))
                            .size(12.0)
                            .color(th.danger),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button_styled(ui, th, "Replace", ButtonKind::Danger, true)
                            .clicked()
                        {
                            self.confirm_overwrite = None;
                            action = FileAction::Accept {
                                path: overwrite_path,
                                ext: overwrite_ext,
                                save: true,
                            };
                        }
                        if widgets::button(ui, th, "Cancel", false).clicked() {
                            self.confirm_overwrite = None;
                        }
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let go = if self.mode == Mode::Open {
                            "Open"
                        } else {
                            "Save"
                        };
                        if widgets::button(ui, th, go, true).clicked() {
                            let a = self.accept(false);
                            if !matches!(a, FileAction::None) {
                                action = a;
                            }
                        }
                        if widgets::button(ui, th, "Cancel", false).clicked() {
                            cancel = true;
                        }
                    });
                });
            }
        });

        if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return FileAction::Cancel;
        }
        action
    }
}

fn places_ui(
    ui: &mut egui::Ui,
    th: &Theme,
    places: &[(String, PathBuf)],
    dir: &Path,
    go_to: &mut Option<PathBuf>,
) {
    ui.label(egui::RichText::new("PLACES").size(10.0).color(th.text_dim));
    ui.add_space(4.0);
    for (label, path) in places {
        let selected = dir == path.as_path();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
        if resp.hovered() || selected {
            ui.painter().rect_filled(
                rect,
                rr(6),
                if selected { th.accent_soft } else { th.hover },
            );
        }
        ui.painter().text(
            Pos2::new(rect.left() + 6.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label.as_str(),
            FontId::proportional(12.0),
            if selected { th.accent } else { th.text },
        );
        if resp.clicked() {
            *go_to = Some(path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_and_save_name_normalization() {
        assert!(is_supported_extension("PNG"));
        assert!(is_supported_extension("jpeg"));
        assert!(is_supported_extension("Bmp"));
        assert!(!is_supported_extension("gif"));
        assert!(!is_supported_extension("webp"));

        assert_eq!(format_index_for_extension("jpeg"), Some(1));
        assert_eq!(
            normalize_save_name("photo.jpeg", "jpg").as_deref(),
            Some("photo.jpeg")
        );
        assert_eq!(
            normalize_save_name("photo.jpeg", "png").as_deref(),
            Some("photo.png")
        );
        // Non-image suffixes such as `.v2` are preserved rather than truncated.
        assert_eq!(
            normalize_save_name("poster.v2", "png").as_deref(),
            Some("poster.v2.png")
        );
        assert_eq!(normalize_save_name("   ", "png"), None);
    }
}
