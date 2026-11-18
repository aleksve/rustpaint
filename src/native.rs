//! Native "Open / Save" panels.
//!
//! `rfd` maps onto the real platform dialogs — `IFileDialog` on Windows,
//! `NSOpenPanel`/`NSSavePanel` on macOS and the XDG desktop portal on Linux.
//! Everything here is best-effort: the caller keeps the built-in themed browser
//! as a fallback, so a machine without a desktop portal (headless boxes, plain
//! WMs, CI) still gets a working file picker.

use std::path::PathBuf;

use crate::filedlg::SUPPORTED_EXTENSIONS;

/// Whether the platform can be expected to show a native panel.
///
/// Windows and macOS always can. On Linux the dialog is HTTP-free but goes
/// through `xdg-desktop-portal` over D-Bus, so we only offer it when a session
/// bus is present.
pub fn available() -> bool {
    if cfg!(target_os = "windows") || cfg!(target_os = "macos") {
        return true;
    }
    if cfg!(target_os = "linux") {
        return std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
    }
    false
}

/// A native panel that comes back instantly with "nothing" almost certainly
/// never appeared; a human cancelling normally takes longer. This is the only
/// failure signal exposed by `rfd`, so use it consistently on every platform.
pub fn probably_failed(started: std::time::Instant) -> bool {
    started.elapsed() < std::time::Duration::from_millis(300)
}

fn filters(dialog: rfd::FileDialog) -> rfd::FileDialog {
    dialog
        .add_filter("Images (PNG, JPEG, BMP)", SUPPORTED_EXTENSIONS)
        .add_filter("PNG image", &["png"])
        .add_filter("JPEG image", &["jpg", "jpeg"])
        .add_filter("BMP image", &["bmp"])
        .add_filter("All files", &["*"])
}

/// Show the system open panel. `None` means the user cancelled (or the panel
/// could not be shown).
pub fn pick_open(start_dir: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = filters(rfd::FileDialog::new().set_title("Open image"));
    if let Some(dir) = start_dir {
        if dir.is_dir() {
            dialog = dialog.set_directory(dir);
        }
    }
    dialog.pick_file()
}

/// Show the system save panel. `start_dir` is the directory to browse and
/// `file_name` the pre-filled name, e.g. `landscape.png`.
pub fn pick_save(start_dir: Option<PathBuf>, file_name: &str) -> Option<PathBuf> {
    let mut dialog = filters(
        rfd::FileDialog::new()
            .set_title("Save image")
            .set_file_name(file_name),
    );
    if let Some(dir) = start_dir {
        if dir.is_dir() {
            dialog = dialog.set_directory(dir);
        }
    }
    dialog.save_file()
}
