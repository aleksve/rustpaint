//! Canvas text rendering.
//!
//! Text is rasterised with `ab_glyph` straight into the tiled coverage mask the
//! brushes use (with no intermediate per-glyph bitmap allocation), so text gets
//! identical alpha compositing and opacity control. `ab_glyph` sits on top of
//! `ttf-parser`, supporting both TrueType and CFF/OpenType system fonts.

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};

use crate::assets::{FALLBACK_FONT, UI_FONT};
use crate::fonts::{BUILTIN_INTER, FontCatalog};
use crate::mask::Mask;

pub(crate) struct TextEngine {
    font: FontArc,
    /// Used for characters the current face has no glyph for.
    fallback: FontArc,
    family: String,
    bold: bool,
    /// Face has no real bold design → fake it by smearing horizontally.
    synthetic_bold: bool,
}

impl TextEngine {
    pub fn new() -> Self {
        let font = FontArc::try_from_slice(UI_FONT)
            .or_else(|_| FontArc::try_from_slice(FALLBACK_FONT))
            .expect("bundled fonts are valid font data");
        let fallback = FontArc::try_from_slice(FALLBACK_FONT)
            .or_else(|_| FontArc::try_from_slice(UI_FONT))
            .expect("bundled fonts are valid font data");
        Self {
            font,
            fallback,
            family: BUILTIN_INTER.to_string(),
            bold: false,
            synthetic_bold: false,
        }
    }

    /// Switch the active face. Returns `false` (leaving the previous face in
    /// place) if the family could not be loaded.
    pub fn set_style(&mut self, catalog: &mut FontCatalog, family: &str, bold: bool) -> bool {
        if family == self.family && bold == self.bold {
            return true;
        }
        let Some((font, true_bold)) = catalog.face(family, bold) else {
            return false;
        };
        self.font = font;
        self.family.clear();
        self.family.push_str(family);
        self.bold = bold;
        self.synthetic_bold = bold && !true_bold;
        true
    }

    /// Draw `text` with its top-left corner at `(x, y)`, returning the laid-out
    /// block dimensions `(width, height)`.
    pub fn draw(&self, mask: &mut Mask, text: &str, x: f32, y: f32, px: f32) -> (f32, f32) {
        let px = px.max(1.0);
        let scale = PxScale::from(px);
        let scaled = self.font.as_scaled(scale);
        let ascent = scaled.ascent();
        let line_h = scaled.height() + scaled.line_gap();
        // Scale synthetic bold stroke width with font size so both tiny labels
        // and large headlines look balanced.
        let bold_extra = if self.synthetic_bold {
            ((px / 32.0).round() as i32).clamp(1, 6)
        } else {
            0
        };
        let space_adv = scaled.h_advance(self.font.glyph_id(' '));
        let mut max_w: f32 = 0.0;
        let mut lines = 0usize;

        for (li, raw_line) in text.split('\n').enumerate() {
            lines = li + 1;
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let baseline = y + ascent + li as f32 * line_h;
            let mut pen = x;
            // Always stores a `GlyphId` belonging to `self.font` (never fallback).
            let mut prev_primary: Option<GlyphId> = None;

            for ch in line.chars() {
                if ch == '\r' {
                    continue;
                }
                if ch == '\t' {
                    pen += space_adv * 4.0;
                    prev_primary = None;
                    continue;
                }

                let mut font = &self.font;
                let mut id = font.glyph_id(ch);
                let mut using_fallback = false;
                if id == GlyphId(0) {
                    let fb = self.fallback.glyph_id(ch);
                    if fb != GlyphId(0) {
                        font = &self.fallback;
                        id = fb;
                        using_fallback = true;
                    }
                }
                let fscale = font.as_scaled(scale);
                if !using_fallback && let Some(prev) = prev_primary {
                    pen += fscale.kern(prev, id);
                }

                let glyph = id.with_scale_and_position(scale, point(pen, baseline));
                if let Some(outlined) = font.outline_glyph(glyph) {
                    let bb = outlined.px_bounds();
                    if bb.min.x.is_finite() && bb.min.y.is_finite() {
                        let ox = bb.min.x.round() as i32;
                        let oy = bb.min.y.round() as i32;
                        outlined.draw(|gx, gy, c| {
                            let v = (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            if v == 0 {
                                return;
                            }
                            let Ok(gx_i) = i32::try_from(gx) else { return };
                            let Ok(gy_i) = i32::try_from(gy) else { return };
                            let (Some(dx), Some(dy)) = (ox.checked_add(gx_i), oy.checked_add(gy_i))
                            else {
                                return;
                            };
                            for shift in 0..=bold_extra {
                                if let Some(sx) = dx.checked_add(shift) {
                                    mask.put_u8(sx, dy, v);
                                }
                            }
                        });
                    }
                }
                pen += fscale.h_advance(id) + bold_extra as f32;
                prev_primary = if using_fallback { None } else { Some(id) };
            }
            max_w = max_w.max(pen - x);
        }
        (max_w.max(1.0), line_h * lines.max(1) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(mask: &Mask) -> usize {
        let mut n = 0;
        for y in 0..mask.height() as i32 {
            for x in 0..mask.width() as i32 {
                if mask.get(x, y) > 0 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn rasterises_text_and_ignores_carriage_returns() {
        let te = TextEngine::new();
        let mut lf = Mask::new(400, 160);
        let mut crlf = Mask::new(400, 160);
        te.draw(&mut lf, "Hello\nWorld", 10.0, 10.0, 36.0);
        te.draw(&mut crlf, "Hello\r\nWorld", 10.0, 10.0, 36.0);
        assert!(covered(&lf) > 100);
        assert_eq!(covered(&lf), covered(&crlf));
    }

    #[test]
    fn bundled_families_render() {
        let mut cat = FontCatalog::bundled_only();
        let mut te = TextEngine::new();
        for fam in [crate::fonts::BUILTIN_INTER, crate::fonts::BUILTIN_DEJAVU] {
            assert!(te.set_style(&mut cat, fam, false));
            let mut mask = Mask::new(400, 120);
            te.draw(&mut mask, "Rg2\tOK", 8.0, 8.0, 40.0);
            assert!(covered(&mask) > 50, "{fam} produced no pixels");
        }
    }

    #[test]
    fn bold_is_wider_and_covers_more_pixels() {
        let mut cat = FontCatalog::bundled_only();
        let mut te = TextEngine::new();
        te.set_style(&mut cat, crate::fonts::BUILTIN_INTER, false);
        let mut a = Mask::new(600, 120);
        let wa = te.draw(&mut a, "Weight", 8.0, 8.0, 40.0).0;
        te.set_style(&mut cat, crate::fonts::BUILTIN_INTER, true);
        let mut b = Mask::new(600, 120);
        let wb = te.draw(&mut b, "Weight", 8.0, 8.0, 40.0).0;
        assert!(covered(&b) > covered(&a), "bold should cover more pixels");
        assert!(wb > wa, "synthetic bold should widen layout advance");
    }
}
