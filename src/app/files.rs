//! File dialogs, image loading, and atomic image saving.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use eframe::egui::Context;

use super::{App, Dialog, Pending};
use crate::doc::Doc;
use crate::filedlg::{FileAction, FileDialog};

fn replace_file_atomically(temp: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(temp, destination)
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn temporary_save_path(destination: &Path) -> PathBuf {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("rustpaint");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    parent.join(format!(
        ".{name}.rustpaint-{nonce}-{}.tmp",
        std::process::id()
    ))
}

fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::doc::MAX_DIMENSION as u32);
    limits.max_image_height = Some(crate::doc::MAX_DIMENSION as u32);
    limits.max_alloc = Some(crate::doc::MAX_DECODE_BYTES);
    limits
}

impl App {
    /// Folder the next file dialog should start in.
    fn start_dir(&self) -> Option<PathBuf> {
        self.doc
            .path
            .as_ref()
            .and_then(|path| path.parent().map(|directory| directory.to_path_buf()))
            .or_else(|| self.prefs.last_dir.clone())
    }

    pub(super) fn ask_new(&mut self) {
        if self.doc.modified {
            self.dialog = Some(Dialog::Discard(Pending::New));
        } else {
            self.dialog = Some(Dialog::New);
        }
    }

    pub(super) fn ask_open(&mut self, ctx: &Context) {
        if self.doc.modified {
            self.dialog = Some(Dialog::Discard(Pending::Open));
        } else {
            self.open_dialog(ctx);
        }
    }

    pub(super) fn resume_pending(&mut self, pending: Pending, ctx: &Context) {
        match pending {
            Pending::New => self.dialog = Some(Dialog::New),
            Pending::Open => self.open_dialog(ctx),
            Pending::OpenPath(path) => self.read_file(path, ctx),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
        }
    }

    fn open_dialog(&mut self, ctx: &Context) {
        self.commit_current();
        let start = self.start_dir();
        if self.native_dialogs && crate::native::available() {
            let started = std::time::Instant::now();
            if let Some(path) = crate::native::pick_open(start) {
                self.read_file(path, ctx);
                return;
            }
            if !crate::native::probably_failed(started) {
                return;
            }
            self.native_dialogs = false;
            self.toast(
                ctx,
                "System dialog unavailable — using the built-in browser",
            );
        }
        self.file_dialog = Some(FileDialog::open(self.start_dir()));
    }

    pub(super) fn save_dialog(&mut self, ctx: &Context) {
        self.commit_current();
        let name = if self.doc.path.is_some() {
            self.doc.display_name()
        } else {
            "untitled.png".to_string()
        };
        let start = self.start_dir();
        if self.native_dialogs && crate::native::available() {
            let started = std::time::Instant::now();
            let picked = crate::native::pick_save(start, &name);
            if picked.is_none() && !crate::native::probably_failed(started) {
                self.pending_after_save = None;
                return;
            }
            if let Some(mut path) = picked {
                let extension = path
                    .extension()
                    .map(|extension| extension.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "bmp") {
                    path.set_extension("png");
                    self.write_file(path, "png", ctx);
                } else {
                    self.write_file(path, &extension, ctx);
                }
                return;
            }
            self.native_dialogs = false;
            self.toast(
                ctx,
                "System dialog unavailable — using the built-in browser",
            );
        }
        self.file_dialog = Some(FileDialog::save(&name, self.start_dir()));
    }

    pub(super) fn quick_save(&mut self, ctx: &Context) {
        self.commit_current();
        if let Some(path) = self.doc.path.clone() {
            let extension = path
                .extension()
                .map(|extension| extension.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| "png".into());
            self.write_file(path, &extension, ctx);
        } else {
            self.save_dialog(ctx);
        }
    }

    fn write_file(&mut self, mut path: PathBuf, extension: &str, ctx: &Context) -> bool {
        let extension = if crate::filedlg::is_supported_extension(extension) {
            extension.to_ascii_lowercase()
        } else {
            path.set_extension("png");
            "png".to_string()
        };
        let image = self.doc.to_rgba_image();
        let temporary = temporary_save_path(&path);
        let result: Result<(), String> = (|| {
            let encoded = match extension.as_str() {
                "jpg" | "jpeg" => {
                    let rgb = crate::doc::flatten_onto_white(&image);
                    rgb.save_with_format(&temporary, image::ImageFormat::Jpeg)
                }
                "bmp" => image.save_with_format(&temporary, image::ImageFormat::Bmp),
                _ => image.save_with_format(&temporary, image::ImageFormat::Png),
            };
            encoded.map_err(|error| error.to_string())?;
            fs::File::open(&temporary)
                .and_then(|file| file.sync_all())
                .map_err(|error| error.to_string())?;
            replace_file_atomically(&temporary, &path).map_err(|error| error.to_string())
        })();
        let _ = fs::remove_file(&temporary);
        match result {
            Ok(()) => {
                self.doc.path = Some(path.clone());
                self.doc.modified = false;
                self.prefs.push_recent(&path);
                self.prefs.save();
                self.toast(ctx, format!("Saved {}", path.display()));
                if let Some(pending) = self.pending_after_save.take() {
                    self.resume_pending(pending, ctx);
                }
                true
            }
            Err(error) => {
                self.pending_after_save = None;
                self.toast(ctx, format!("Save failed: {error}"));
                false
            }
        }
    }

    pub(super) fn read_file(&mut self, path: PathBuf, ctx: &Context) {
        let dimensions = image::ImageReader::open(&path)
            .map_err(|error| error.to_string())
            .and_then(|mut reader| {
                reader.limits(image_limits());
                reader.into_dimensions().map_err(|error| error.to_string())
            });
        let (width, height) = match dimensions {
            Ok((width, height)) => (width as usize, height as usize),
            Err(error) => {
                self.toast(ctx, format!("Open failed: {error}"));
                return;
            }
        };
        if let Err(error) = crate::doc::validate_dimensions(width, height) {
            self.toast(ctx, format!("{width}×{height} cannot be opened: {error}"));
            return;
        }
        let decoded = image::ImageReader::open(&path)
            .map_err(|error| error.to_string())
            .and_then(|mut reader| {
                reader.limits(image_limits());
                reader.decode().map_err(|error| error.to_string())
            });
        match decoded {
            Ok(image) => match Doc::from_image(image.into_rgba8(), Some(path.clone())) {
                Ok(document) => {
                    self.doc = document;
                    self.hist.clear();
                    self.reset_transient_edit_state();
                    self.edit.selection = None;
                    self.tex_dirty.mark_all();
                    self.view.fit_pending = true;
                    self.prefs.push_recent(&path);
                    self.prefs.save();
                    self.toast(ctx, format!("Opened {}", self.doc.display_name()));
                }
                Err(error) => self.toast(ctx, format!("Open failed: {error}")),
            },
            Err(error) => self.toast(ctx, format!("Open failed: {error}")),
        }
    }

    pub(super) fn handle_file_action(&mut self, action: FileAction, ctx: &Context) {
        match action {
            FileAction::None => {}
            FileAction::Cancel => {
                self.file_dialog = None;
                self.pending_after_save = None;
            }
            FileAction::Accept { path, ext, save } => {
                self.file_dialog = None;
                if save {
                    self.write_file(path, ext, ctx);
                } else {
                    self.read_file(path, ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replace_replaces_destination() {
        let root = std::env::temp_dir().join(format!(
            "rustpaint-atomic-save-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create test directory");
        let destination = root.join("image.png");
        let temporary = root.join(".image.png.rustpaint.tmp");
        fs::write(&destination, b"old").expect("write old file");
        fs::write(&temporary, b"new").expect("write temporary file");

        replace_file_atomically(&temporary, &destination).expect("replace destination");
        assert_eq!(fs::read(&destination).expect("read destination"), b"new");
        assert!(!temporary.exists());
        fs::remove_dir_all(root).expect("remove test directory");
    }
}
