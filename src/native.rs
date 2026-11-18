//! Native "Open / Save" panels.
//!
//! `rfd` maps onto the real platform dialogs — `IFileDialog` on Windows,
//! `NSOpenPanel`/`NSSavePanel` on macOS and the XDG desktop portal on Linux.
//! Everything here is best-effort: the caller keeps the built-in themed browser
//! as a fallback, so a machine without a desktop portal (headless boxes, plain
//! WMs, CI, or other Unix targets) still gets a working file picker.

use std::path::PathBuf;

/// Whether the platform can be expected to show a native panel.
pub(crate) fn available() -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        // `xdg-desktop-portal` speaks over the user D-Bus session bus.
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
            return true;
        }
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(|dir| PathBuf::from(dir).join("bus").exists())
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// On Linux XDG portals, `rfd` signals a missing portal backend by returning
/// `None` immediately. Restrict this heuristic to Linux so pressing `Escape`
/// quickly on Windows or macOS never disables native dialogs.
pub(crate) fn probably_failed(started: std::time::Instant) -> bool {
    cfg!(target_os = "linux") && started.elapsed() < std::time::Duration::from_millis(250)
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn filters(dialog: rfd::FileDialog) -> rfd::FileDialog {
    dialog
        .add_filter(
            "Images (PNG, JPEG, BMP)",
            crate::filedlg::SUPPORTED_EXTENSIONS,
        )
        .add_filter("PNG image", &["png"])
        .add_filter("JPEG image", &["jpg", "jpeg"])
        .add_filter("BMP image", &["bmp"])
        .add_filter("All files", &["*"])
}

/// Show the system open panel, parented to `frame` when available.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub(crate) fn pick_open(
    parent: Option<&eframe::Frame>,
    start_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    let mut dialog = filters(rfd::FileDialog::new().set_title("Open image"));
    if let Some(frame) = parent {
        dialog = dialog.set_parent(frame);
    }
    if let Some(dir) = start_dir
        && dir.is_dir()
    {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn pick_open(
    _parent: Option<&eframe::Frame>,
    _start_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    None
}

/// Show the system save panel, parented to `frame` when available.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub(crate) fn pick_save(
    parent: Option<&eframe::Frame>,
    start_dir: Option<PathBuf>,
    file_name: &str,
) -> Option<PathBuf> {
    let mut dialog = filters(
        rfd::FileDialog::new()
            .set_title("Save image")
            .set_file_name(file_name),
    );
    if let Some(frame) = parent {
        dialog = dialog.set_parent(frame);
    }
    if let Some(dir) = start_dir
        && dir.is_dir()
    {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) fn pick_save(
    _parent: Option<&eframe::Frame>,
    _start_dir: Option<PathBuf>,
    _file_name: &str,
) -> Option<PathBuf> {
    None
}
