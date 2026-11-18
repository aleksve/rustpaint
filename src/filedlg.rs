//! A dependency free file browser.
//!
//! Native file dialogs would mean pulling in GTK on Linux / extra platform
//! crates, so RustPaint ships its own small browser: it looks and behaves the
//! same on Windows, macOS and Linux.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Context, FontId, Pos2, Rect, Rounding, Sense, Vec2};

use crate::icons::{self, Icon};
use crate::theme::Theme;
use crate::widgets;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    Save,
}

pub enum FileAction {
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

pub struct FileDialog {
    mode: Mode,
    dir: PathBuf,
    name: String,
    entries: Vec<Entry>,
    sel: Option<PathBuf>,
    error: Option<String>,
    formats: Vec<(&'static str, &'static str)>,
    fmt: usize,
    path_edit: String,
}

const SAVE_FORMATS: &[(&str, &str)] = &[("PNG", "png"), ("JPEG", "jpg"), ("BMP", "bmp")];
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp"];

pub fn is_supported_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(ext))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Resolve the folder a dialog should open in: the caller's suggestion (last
/// used folder) if it still exists, otherwise the usual pictures/home folders.
fn start_dir(hint: Option<PathBuf>) -> PathBuf {
    if let Some(d) = hint {
        if d.is_dir() {
            return d;
        }
    }
    let d = crate::prefs::default_dir();
    if d.is_dir() {
        return d;
    }
    std::env::current_dir().unwrap_or_else(|_| home_dir())
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
            sel: None,
            error: None,
            formats: SAVE_FORMATS.to_vec(),
            fmt: 0,
        };
        d.refresh();
        d
    }

    pub fn save(default_name: &str, hint: Option<PathBuf>) -> Self {
        let dir = start_dir(hint);
        let mut d = Self {
            mode: Mode::Save,
            name: default_name.to_string(),
            path_edit: dir.display().to_string(),
            dir,
            entries: Vec::new(),
            sel: None,
            error: None,
            formats: SAVE_FORMATS.to_vec(),
            fmt: 0,
        };
        if let Some(ext) = default_name.rsplit('.').next() {
            if let Some(i) = d.formats.iter().position(|(_, e)| *e == ext) {
                d.fmt = i;
            }
        }
        d.refresh();
        d
    }

    fn allowed(&self, p: &Path) -> bool {
        if self.mode == Mode::Save {
            return true;
        }
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        is_supported_extension(&ext)
    }

    fn refresh(&mut self) {
        self.entries.clear();
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
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = path.is_dir();
            if !is_dir && !self.allowed(&path) {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            self.entries.push(Entry {
                name,
                path,
                dir: is_dir,
                size,
            });
        }
        self.entries.sort_by(|a, b| {
            b.dir
                .cmp(&a.dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    fn nav_to(&mut self, p: PathBuf) {
        if p.is_dir() {
            self.dir = p;
            self.sel = None;
            self.refresh();
        }
    }

    fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let parent = parent.to_path_buf();
            self.nav_to(parent);
        }
    }

    fn accept(&mut self) -> FileAction {
        let ext = self.formats[self.fmt].1;
        let path = match self.mode {
            Mode::Open => match &self.sel {
                Some(p) if p.is_file() => p.clone(),
                _ => {
                    // fall back to the typed name
                    let p = self.dir.join(self.name.trim());
                    if p.is_file() {
                        p
                    } else {
                        self.error = Some("Select a file to open".into());
                        return FileAction::None;
                    }
                }
            },
            Mode::Save => {
                let mut name = self.name.trim().to_string();
                if name.is_empty() {
                    self.error = Some("Enter a file name".into());
                    return FileAction::None;
                }
                if !name.to_lowercase().ends_with(&format!(".{ext}")) {
                    // replace a foreign extension instead of stacking two
                    if let Some(dot) = name.rfind('.') {
                        if !name[dot..].contains('/') && name.len() - dot <= 6 {
                            name.truncate(dot);
                        }
                    }
                    name.push('.');
                    name.push_str(ext);
                }
                self.dir.join(name)
            }
        };
        FileAction::Accept {
            path,
            ext,
            save: self.mode == Mode::Save,
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
                    let h = home_dir();
                    self.nav_to(h);
                }
                let w = ui.available_width() - 40.0;
                let resp = ui.add_sized(
                    Vec2::new(w.max(120.0), 28.0),
                    egui::TextEdit::singleline(&mut self.path_edit)
                        .font(FontId::proportional(12.0)),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let p = PathBuf::from(self.path_edit.trim());
                    self.nav_to(p);
                }
            });
            ui.add_space(6.0);

            // ---- body
            let body_h = size.y - 210.0;
            let mut go_to: Option<PathBuf> = None;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(130.0, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        places_ui(ui, th, &self.dir, &mut go_to);
                    },
                );
                ui.add_space(4.0);
                let list_w = ui.available_width();
                ui.allocate_ui_with_layout(
                    Vec2::new(list_w, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(body_h)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let mut navigate: Option<PathBuf> = None;
                                let mut pick: Option<PathBuf> = None;
                                for e in &self.entries {
                                    let label = if e.dir {
                                        format!("{}/", e.name)
                                    } else {
                                        e.name.clone()
                                    };
                                    let selected = self.sel.as_deref() == Some(e.path.as_path());
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 26.0),
                                        Sense::click(),
                                    );
                                    if resp.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            Rounding::same(6.0),
                                            th.hover,
                                        );
                                    }
                                    if selected {
                                        ui.painter().rect_filled(
                                            rect,
                                            Rounding::same(6.0),
                                            th.accent_soft,
                                        );
                                    }
                                    let ic = if e.dir { Icon::Folder } else { Icon::File };
                                    let ir = Rect::from_center_size(
                                        Pos2::new(rect.left() + 14.0, rect.center().y),
                                        Vec2::splat(14.0),
                                    );
                                    icons::draw(
                                        ui.painter(),
                                        ic,
                                        ir,
                                        if e.dir { th.accent } else { th.text_dim },
                                        1.4,
                                    );
                                    ui.painter().text(
                                        Pos2::new(rect.left() + 28.0, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &label,
                                        FontId::proportional(12.5),
                                        th.text,
                                    );
                                    if !e.dir {
                                        let kb = e.size as f64 / 1024.0;
                                        let txt = if kb > 1024.0 {
                                            format!("{:.1} MB", kb / 1024.0)
                                        } else {
                                            format!("{:.0} KB", kb.max(1.0))
                                        };
                                        ui.painter().text(
                                            Pos2::new(rect.right() - 10.0, rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            txt,
                                            FontId::proportional(11.0),
                                            th.text_dim,
                                        );
                                    }
                                    if resp.clicked() {
                                        if e.dir {
                                            navigate = Some(e.path.clone());
                                        } else {
                                            pick = Some(e.path.clone());
                                            self.name = e.name.clone();
                                        }
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
                                            return;
                                        }
                                    }
                                }
                                if let Some(p) = navigate {
                                    self.nav_to(p);
                                }
                                if let Some(p) = pick {
                                    self.sel = Some(p);
                                }
                            });
                    },
                );
            });

            if let Some(p) = go_to {
                self.nav_to(p);
            }

            ui.add_space(6.0);

            // ---- file name + format + buttons
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Name").size(12.0).color(th.text_dim));
                let w = (ui.available_width() - 200.0).max(140.0);
                ui.add_sized(
                    Vec2::new(w, 28.0),
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text(if self.mode == Mode::Save {
                            "image.png"
                        } else {
                            "file name"
                        })
                        .font(FontId::proportional(12.5)),
                );
                if self.mode == Mode::Save {
                    for (i, (label, _)) in self.formats.iter().enumerate() {
                        let active = self.fmt == i;
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::new(40.0, 28.0), Sense::click());
                        let bg = if active { th.accent } else { th.raised };
                        let fg = if active { th.on_accent } else { th.text_dim };
                        ui.painter().rect_filled(rect, Rounding::same(7.0), bg);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            *label,
                            FontId::proportional(11.0),
                            fg,
                        );
                        if resp.clicked() {
                            self.fmt = i;
                        }
                    }
                }
            });

            if let Some(err) = &self.error {
                ui.label(egui::RichText::new(err).size(11.5).color(th.danger));
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let go = if self.mode == Mode::Open {
                        "Open"
                    } else {
                        "Save"
                    };
                    if widgets::button(ui, th, go, true).clicked() {
                        let a = self.accept();
                        if !matches!(a, FileAction::None) {
                            action = a;
                        }
                    }
                    if widgets::button(ui, th, "Cancel", false).clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if cancel {
            return FileAction::Cancel;
        }
        if ui_escape(ctx) {
            return FileAction::Cancel;
        }
        action
    }
}

fn ui_escape(ctx: &Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

fn places_ui(ui: &mut egui::Ui, th: &Theme, dir: &Path, go_to: &mut Option<PathBuf>) {
    let home = home_dir();
    let mut candidates: Vec<(String, PathBuf)> = vec![("Home".to_string(), home.clone())];
    for (label, sub) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures", "Pictures"),
    ] {
        let p = home.join(sub);
        if p.is_dir() {
            candidates.push((label.to_string(), p));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(("Working dir".to_string(), cwd));
    }
    #[cfg(windows)]
    {
        for l in 'A'..='Z' {
            let p = PathBuf::from(format!("{l}:\\"));
            if p.is_dir() {
                candidates.push((format!("{}:", l), p));
            }
        }
    }
    #[cfg(not(windows))]
    {
        candidates.push(("Root".to_string(), PathBuf::from("/")));
    }

    ui.label(egui::RichText::new("PLACES").size(10.0).color(th.text_dim));
    ui.add_space(4.0);
    for (label, path) in candidates {
        let selected = dir == path.as_path();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
        if resp.hovered() || selected {
            ui.painter().rect_filled(
                rect,
                Rounding::same(6.0),
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
            *go_to = Some(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_match_enabled_codecs() {
        assert!(is_supported_extension("PNG"));
        assert!(is_supported_extension("jpeg"));
        assert!(is_supported_extension("Bmp"));
        assert!(!is_supported_extension("gif"));
        assert!(!is_supported_extension("webp"));
        assert!(!is_supported_extension("tiff"));
    }
}
