//! Bundled fonts (see `assets/`), embedded so the binary is self contained on
//! every platform.

/// Inter (OFL) — used for all UI text and as the canvas text face.
pub static UI_FONT: &[u8] = include_bytes!("../assets/Inter-Variable.ttf");
/// DejaVu Sans — fallback for glyphs Inter does not cover.
pub static FALLBACK_FONT: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
