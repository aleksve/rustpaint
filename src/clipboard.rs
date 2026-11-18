//! Cross-platform image clipboard with an automatic in-process fallback.
//!
//! Uses `arboard` on Windows, macOS, and Linux (X11 + Wayland data-control) so
//! images can be copied to and pasted from external apps and screenshots, while
//! keeping a persistent `Clipboard` handle alive so X11/Wayland selections are
//! not dropped immediately. When no system clipboard is reachable (headless
//! sessions, tests, or unsupported platforms), the in-app buffer still works.

use crate::doc::{self, DimensionError};

#[derive(Clone)]
pub(crate) struct Clip {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

#[derive(Default)]
pub(crate) struct ImageClipboard {
    local: Option<Clip>,
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    system: Option<arboard::Clipboard>,
}

impl ImageClipboard {
    pub fn new() -> Self {
        Self {
            local: None,
            #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
            system: arboard::Clipboard::new().ok(),
        }
    }

    /// Headless instance that never touches the OS clipboard (used by unit tests).
    #[cfg(test)]
    pub fn local_only() -> Self {
        Self::default()
    }

    /// Store an RGBA image into the local buffer and (best-effort) the OS clipboard.
    pub fn set(&mut self, clip: Clip) {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            if self.system.is_none() {
                self.system = arboard::Clipboard::new().ok();
            }
            if let Some(sys) = self.system.as_mut() {
                let data = arboard::ImageData {
                    width: clip.w,
                    height: clip.h,
                    bytes: std::borrow::Cow::Borrowed(&clip.px),
                };
                let _ = sys.set_image(data);
            }
        }
        self.local = Some(clip);
    }

    /// Retrieve an RGBA image, preferring the current OS clipboard so external
    /// screenshots/images paste cleanly, falling back to the in-app buffer.
    pub fn get(&mut self) -> Result<Option<Clip>, DimensionError> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            if self.system.is_none() {
                self.system = arboard::Clipboard::new().ok();
            }
            if let Some(sys) = self.system.as_mut()
                && let Ok(img) = sys.get_image()
            {
                doc::validate_dimensions(img.width, img.height)?;
                let expected = img.width * img.height * 4;
                let mut bytes = img.bytes.into_owned();
                if bytes.len() >= expected {
                    bytes.truncate(expected);
                    return Ok(Some(Clip {
                        px: bytes,
                        w: img.width,
                        h: img.height,
                    }));
                }
            }
        }
        if let Some(c) = self.local.as_ref() {
            doc::validate_dimensions(c.w, c.h)?;
            return Ok(Some(c.clone()));
        }
        Ok(None)
    }

    /// Whether *Edit → Paste* should be enabled. On platforms with an OS image
    /// clipboard an external app may hold an image even before any in-app copy.
    pub fn can_paste(&self) -> bool {
        self.local.is_some()
            || cfg!(any(
                target_os = "windows",
                target_os = "macos",
                target_os = "linux"
            )) && self.has_system_backend()
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn has_system_backend(&self) -> bool {
        self.system.is_some()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    fn has_system_backend(&self) -> bool {
        false
    }
}
