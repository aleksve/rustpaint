//! File dialogs, EXIF-aware image loading, and crash-safe atomic image saving.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use eframe::egui::Context;
use image::{
    ColorType, DynamicImage, ImageDecoder, ImageEncoder, ImageReader,
    codecs::{bmp::BmpEncoder, jpeg::JpegEncoder, png::PngEncoder},
};

use super::{App, Dialog, Pending};
use crate::doc::Doc;
use crate::filedlg::{FileAction, FileDialog};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn replace_file_atomically(temp: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(temp, destination)
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };

        let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
        // SAFETY: Both UTF-16 buffers are NUL-terminated and outlive the call.
        let ok = unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) };
        if ok != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

/// Create a brand-new sibling temporary file opened with write permission
/// (`O_CREAT | O_EXCL` / `CREATE_NEW`) so symlinks are never followed and
/// `File::sync_all` (`FlushFileBuffers` on Windows) has `GENERIC_WRITE` access.
fn create_sibling_temp(destination: &Path) -> io::Result<(File, PathBuf)> {
    let parent = match destination.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rustpaint");
    let pid = std::process::id();

    for attempt in 0..32u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.rustpaint-{pid}-{nanos}-{seq}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((file, candidate)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 31 => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

/// Write data via an exclusive sibling temp file, flush + `sync_all` on the
/// writable handle, and atomically replace `destination`.
fn atomic_write_with(
    destination: &Path,
    write_fn: impl FnOnce(&mut BufWriter<&File>) -> Result<(), String>,
) -> Result<(), String> {
    let (file, temporary) = create_sibling_temp(destination).map_err(|e| e.to_string())?;
    let outcome = (|| {
        {
            let mut writer = BufWriter::new(&file);
            write_fn(&mut writer)?;
            writer.flush().map_err(|e| e.to_string())?;
        }
        // Sync on the open writable handle (required on Windows for FlushFileBuffers),
        // then close the handle before renaming.
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        replace_file_atomically(&temporary, destination).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            let parent = match destination.parent() {
                Some(p) if !p.as_os_str().is_empty() => p,
                _ => Path::new("."),
            };
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

/// Atomically persist small files (such as `settings.ini`).
pub(crate) fn write_bytes_atomically(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_with(destination, |w| {
        w.write_all(bytes).map_err(|e| e.to_string())
    })
}

/// Encode straight-alpha RGBA bytes directly into `destination` without cloning
/// the full pixel buffer into an intermediate `RgbaImage`.
pub(crate) fn encode_image_atomically(
    px: &[u8],
    w: usize,
    h: usize,
    destination: &Path,
    extension: &str,
) -> Result<(), String> {
    crate::doc::validate_dimensions(w, h).map_err(|e| e.to_string())?;
    let expected = w * h * 4;
    if px.len() != expected {
        return Err("pixel buffer size does not match canvas dimensions".into());
    }
    let (wu, hu) = (w as u32, h as u32);
    atomic_write_with(destination, |writer| match extension {
        "jpg" | "jpeg" => {
            let rgb = crate::doc::flatten_onto_white(px, wu, hu);
            let encoder = JpegEncoder::new_with_quality(writer, 92);
            encoder
                .write_image(rgb.as_raw(), wu, hu, ColorType::Rgb8.into())
                .map_err(|e| e.to_string())
        }
        "bmp" => {
            let encoder = BmpEncoder::new(writer);
            encoder
                .write_image(px, wu, hu, ColorType::Rgba8.into())
                .map_err(|e| e.to_string())
        }
        _ => {
            let encoder = PngEncoder::new(writer);
            encoder
                .write_image(px, wu, hu, ColorType::Rgba8.into())
                .map_err(|e| e.to_string())
        }
    })
}

fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::doc::MAX_DIMENSION as u32);
    limits.max_image_height = Some(crate::doc::MAX_DIMENSION as u32);
    limits.max_alloc = Some(crate::doc::MAX_DECODE_BYTES);
    limits
}

/// Decode an image in a single file open, sniffing format from magic bytes (so
/// misnamed extensions still open), enforcing dimension/memory caps before full
/// allocation, and applying EXIF orientation metadata (e.g. phone photos).
pub(crate) fn load_document(path: &Path) -> Result<Doc, String> {
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|e| e.to_string())?;
    reader.limits(image_limits());

    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if let Err(error) = crate::doc::validate_dimensions(width as usize, height as usize) {
        return Err(format!("{width}×{height} cannot be opened: {error}"));
    }
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Doc::from_image(image.into_rgba8(), Some(path.to_path_buf())).map_err(|e| e.to_string())
}

enum IoOutcome {
    Opened(Result<Doc, String>),
    Saved {
        path: PathBuf,
        saved_on_current_revision: bool,
        result: Result<(), String>,
    },
}

pub(crate) struct ActiveIoJob {
    pub label: String,
    rx: Receiver<IoOutcome>,
}

impl App {
    /// Folder the next file dialog should start in.
    fn start_dir(&self) -> Option<PathBuf> {
        self.doc
            .path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| self.prefs.last_dir.clone())
    }

    pub(crate) fn cancel_dialogs(&mut self) {
        self.dialog = None;
        self.file_dialog = None;
        self.pending_after_save = None;
    }

    pub(super) fn ask_new(&mut self) {
        if self.doc.modified {
            self.dialog = Some(Dialog::Discard(Pending::New));
        } else {
            self.dialog = Some(Dialog::New);
        }
    }

    pub(super) fn ask_open(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        if self.doc.modified {
            self.dialog = Some(Dialog::Discard(Pending::Open));
        } else {
            self.open_dialog(ctx, frame);
        }
    }

    pub(super) fn resume_pending(
        &mut self,
        pending: Pending,
        ctx: &Context,
        frame: Option<&eframe::Frame>,
    ) {
        match pending {
            Pending::New => self.dialog = Some(Dialog::New),
            Pending::Open => self.open_dialog(ctx, frame),
            Pending::OpenPath(path) => self.start_open_file(path, ctx),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
        }
    }

    fn open_dialog(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        self.commit_current();
        let start = self.start_dir();
        if self.native_dialogs && crate::native::available() {
            let started = std::time::Instant::now();
            if let Some(path) = crate::native::pick_open(frame, start) {
                self.start_open_file(path, ctx);
                return;
            }
            if !crate::native::probably_failed(started) {
                return;
            }
            self.native_dialogs = false;
            self.prefs.native_dialogs = Some(false);
            self.prefs.save();
            self.toast(
                ctx,
                "System dialog unavailable — using the built-in browser",
            );
        }
        self.file_dialog = Some(FileDialog::open(self.start_dir()));
    }

    pub(super) fn save_dialog(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        self.commit_current();
        let name = if self.doc.path.is_some() {
            self.doc.display_name()
        } else {
            "untitled.png".to_string()
        };
        let start = self.start_dir();
        if self.native_dialogs && crate::native::available() {
            let started = std::time::Instant::now();
            let picked = crate::native::pick_save(frame, start, &name);
            if picked.is_none() && !crate::native::probably_failed(started) {
                self.pending_after_save = None;
                return;
            }
            if let Some(mut path) = picked {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if !crate::filedlg::is_supported_extension(&ext) {
                    path.set_extension("png");
                    self.start_save_file(path, "png", ctx);
                } else {
                    self.start_save_file(path, &ext, ctx);
                }
                return;
            }
            self.native_dialogs = false;
            self.prefs.native_dialogs = Some(false);
            self.prefs.save();
            self.toast(
                ctx,
                "System dialog unavailable — using the built-in browser",
            );
        }
        self.file_dialog = Some(FileDialog::save(&name, self.start_dir()));
    }

    pub(super) fn quick_save(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        self.commit_current();
        if let Some(path) = self.doc.path.clone() {
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "png".into());
            self.start_save_file(path, &extension, ctx);
        } else {
            self.save_dialog(ctx, frame);
        }
    }

    /// Encode and save on a worker thread for large canvases so the UI stays
    /// responsive, falling back to inline save if thread creation fails or the
    /// canvas is small.
    pub(super) fn start_save_file(&mut self, mut path: PathBuf, extension: &str, ctx: &Context) {
        if self.io_job.is_some() {
            return;
        }
        let extension = if crate::filedlg::is_supported_extension(extension) {
            extension.to_ascii_lowercase()
        } else {
            path.set_extension("png");
            "png".to_string()
        };
        let (w, h) = (self.doc.w, self.doc.h);
        // Small images (< 2 MP) save in a few milliseconds without cloning.
        if w * h <= 2_000_000 {
            let res = encode_image_atomically(&self.doc.px, w, h, &path, &extension);
            self.finish_save(path, true, res, ctx, None);
            return;
        }

        let px = self.doc.px.clone();
        let label = format!(
            "Saving {}…",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "image".into())
        );
        let (tx, rx) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let dest = path.clone();
        let worker_ext = extension.clone();
        match std::thread::Builder::new()
            .name("rustpaint-save".into())
            .spawn(move || {
                let result = encode_image_atomically(&px, w, h, &dest, &worker_ext);
                let _ = tx.send(IoOutcome::Saved {
                    path: dest,
                    saved_on_current_revision: true,
                    result,
                });
                repaint_ctx.request_repaint();
            }) {
            Ok(_) => self.io_job = Some(ActiveIoJob { label, rx }),
            Err(_) => {
                let res = encode_image_atomically(&self.doc.px, w, h, &path, &extension);
                self.finish_save(path, true, res, ctx, None);
            }
        }
    }

    /// Open and decode an image on a worker thread so large PNGs never stall
    /// the window event loop.
    pub(super) fn start_open_file(&mut self, path: PathBuf, ctx: &Context) {
        if self.io_job.is_some() {
            return;
        }
        let label = format!(
            "Opening {}…",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "image".into())
        );
        let (tx, rx) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let target = path.clone();
        match std::thread::Builder::new()
            .name("rustpaint-open".into())
            .spawn(move || {
                let result = load_document(&target);
                let _ = tx.send(IoOutcome::Opened(result));
                repaint_ctx.request_repaint();
            }) {
            Ok(_) => self.io_job = Some(ActiveIoJob { label, rx }),
            Err(_) => self.finish_open(load_document(&path), ctx),
        }
    }

    pub(super) fn poll_io_job(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        let outcome = match self.io_job.as_ref() {
            Some(job) => match job.rx.try_recv() {
                Ok(done) => Some(done),
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.io_job = None;
                    self.pending_after_save = None;
                    self.toast(ctx, "File operation interrupted");
                    return;
                }
            },
            None => return,
        };
        self.io_job = None;
        if let Some(outcome) = outcome {
            match outcome {
                IoOutcome::Opened(result) => self.finish_open(result, ctx),
                IoOutcome::Saved {
                    path,
                    saved_on_current_revision,
                    result,
                } => self.finish_save(path, saved_on_current_revision, result, ctx, frame),
            }
        }
    }

    fn finish_save(
        &mut self,
        path: PathBuf,
        mark_clean: bool,
        result: Result<(), String>,
        ctx: &Context,
        frame: Option<&eframe::Frame>,
    ) {
        match result {
            Ok(()) => {
                self.doc.path = Some(path.clone());
                if mark_clean {
                    self.hist.mark_saved();
                    self.sync_modified();
                }
                self.prefs.push_recent(&path);
                self.prefs.save();
                self.toast(ctx, format!("Saved {}", path.display()));
                if let Some(pending) = self.pending_after_save.take() {
                    self.resume_pending(pending, ctx, frame);
                }
            }
            Err(error) => {
                self.pending_after_save = None;
                self.toast(ctx, format!("Save failed: {error}"));
            }
        }
    }

    fn finish_open(&mut self, result: Result<Doc, String>, ctx: &Context) {
        match result {
            Ok(document) => {
                let opened_path = document.path.clone();
                self.doc = document;
                self.hist.reset_clean();
                self.reset_transient_edit_state();
                self.edit.selection = None;
                self.tex_dirty.mark_all();
                self.view.fit_pending = true;
                if let Some(path) = &opened_path {
                    self.prefs.push_recent(path);
                    self.prefs.save();
                }
                self.toast(ctx, format!("Opened {}", self.doc.display_name()));
            }
            Err(error) => self.toast(ctx, format!("Open failed: {error}")),
        }
    }

    pub(super) fn handle_file_action(&mut self, action: FileAction, ctx: &Context) {
        match action {
            FileAction::None => {}
            FileAction::Cancel => {
                self.cancel_dialogs();
            }
            FileAction::Accept { path, ext, save } => {
                self.file_dialog = None;
                if save {
                    self.start_save_file(path, ext, ctx);
                } else {
                    self.start_open_file(path, ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Color32;

    fn temp_test_dir(tag: &str) -> PathBuf {
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("rustpaint-{tag}-{}-{seq}", std::process::id()));
        fs::create_dir_all(&dir).expect("create test directory");
        dir
    }

    #[test]
    fn save_and_load_roundtrip_png_bmp_jpeg_and_overwrite() {
        let root = temp_test_dir("io-roundtrip");
        let mut doc = Doc::new(4, 3, Some(Color32::WHITE)).unwrap();
        doc.set(1, 2, [20, 140, 230, 128]);

        // PNG (lossless RGBA) + overwrite existing file via writable sync_all.
        let png = root.join("art.png");
        encode_image_atomically(&doc.px, doc.w, doc.h, &png, "png").expect("initial png save");
        doc.set(0, 0, [250, 10, 40, 255]);
        encode_image_atomically(&doc.px, doc.w, doc.h, &png, "png").expect("overwrite png save");
        let loaded_png = load_document(&png).expect("load png");
        assert_eq!((loaded_png.w, loaded_png.h), (4, 3));
        assert_eq!(loaded_png.px, doc.px);

        // BMP (lossless RGBA).
        let bmp = root.join("art.bmp");
        encode_image_atomically(&doc.px, doc.w, doc.h, &bmp, "bmp").expect("save bmp");
        let loaded_bmp = load_document(&bmp).expect("load bmp");
        assert_eq!(loaded_bmp.px, doc.px);

        // JPEG (flattened onto white, opaque).
        let jpg = root.join("art.jpg");
        encode_image_atomically(&doc.px, doc.w, doc.h, &jpg, "jpg").expect("save jpg");
        let loaded_jpg = load_document(&jpg).expect("load jpg");
        assert_eq!((loaded_jpg.w, loaded_jpg.h), (4, 3));
        assert_eq!(loaded_jpg.get(1, 2).unwrap()[3], 255);

        // Misnamed file (PNG bytes stored under `.jpg` extension) opens via magic bytes.
        let misnamed = root.join("actually_png.jpg");
        fs::copy(&png, &misnamed).expect("copy misnamed file");
        let sniffed = load_document(&misnamed).expect("sniff png inside .jpg");
        assert_eq!(sniffed.px, doc.px);

        // No leftover `.tmp` files remain in the directory.
        let leftovers: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn loads_bundled_example_png_and_applies_exif_orientation() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/landscape.png");
        let doc = load_document(&example).expect("load examples/landscape.png");
        assert!(doc.w > 0 && doc.h > 0);
        assert!(!doc.modified);

        // Craft a 2×1 JPEG with an APP1 EXIF Orientation = 6 (Rotate90 CW) segment
        // and verify `load_document` rotates it into a 1×2 canvas.
        let root = temp_test_dir("exif");
        let raw_jpg = root.join("raw.jpg");
        let px = [255u8, 0, 0, 255, 0, 0, 255, 255];
        encode_image_atomically(&px, 2, 1, &raw_jpg, "jpg").expect("encode raw jpeg");
        let jpeg_bytes = fs::read(&raw_jpg).expect("read raw jpeg");
        assert_eq!(&jpeg_bytes[..2], &[0xFF, 0xD8]);

        // Minimal little-endian TIFF header + 1 IFD entry (tag 0x0112 Orientation = 6).
        let exif_payload: &[u8] = &[
            b'E', b'x', b'i', b'f', 0, 0, // Exif header
            b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, // TIFF LE header, IFD0 @ 8
            0x01, 0x00, // 1 entry
            0x12, 0x01, // Tag = Orientation (0x0112)
            0x03, 0x00, // Type = SHORT (3)
            0x01, 0x00, 0x00, 0x00, // Count = 1
            0x06, 0x00, 0x00, 0x00, // Value = 6 (Rotate 90 CW)
            0x00, 0x00, 0x00, 0x00, // Next IFD = 0
        ];
        let seg_len = (exif_payload.len() + 2) as u16;
        let mut rotated_jpeg = Vec::with_capacity(jpeg_bytes.len() + exif_payload.len() + 4);
        rotated_jpeg.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE1]);
        rotated_jpeg.extend_from_slice(&seg_len.to_be_bytes());
        rotated_jpeg.extend_from_slice(exif_payload);
        rotated_jpeg.extend_from_slice(&jpeg_bytes[2..]);

        let exif_jpg = root.join("rotated.jpg");
        fs::write(&exif_jpg, rotated_jpeg).expect("write exif jpeg");
        let oriented = load_document(&exif_jpg).expect("load exif jpeg");
        assert_eq!((oriented.w, oriented.h), (1, 2));

        fs::remove_dir_all(root).expect("remove test directory");
    }
}
