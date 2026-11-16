//! Canvas text rendering.
//!
//! Text is rasterised with `ab_glyph` into the same coverage mask the brushes
//! use, so text gets the exact same alpha compositing as everything else.
//! `ab_glyph` sits on top of `ttf-parser`, so both TrueType and CFF/OpenType
//! outlines work — which is what makes arbitrary system fonts usable.

use ab_glyph::{point, Font, FontArc, GlyphId, PxScale, ScaleFont};

use crate::assets::{FALLBACK_FONT, UI_FONT};
use crate::fonts::{FontCatalog, BUILTIN_INTER};
use crate::mask::Mask;

pub struct TextEngine {
    font: FontArc,
    /// Used for characters the current face has no glyph for.
    fallback: FontArc,
    family: String,
    bold: bool,
    /// Face has no real bold design → fake it by double-striking.
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
    pub fn set_style(&mut self, catalog: &FontCatalog, family: &str, bold: bool) -> bool {
        if family == self.family && bold == self.bold {
            return true;
        }
        let Some((font, true_bold)) = catalog.face(family, bold) else {
            return false;
        };
        self.font = font;
        self.family = family.to_string();
        self.bold = bold;
        self.synthetic_bold = bold && !true_bold;
        true
    }

    /// Draw `text` with its top-left corner at (x, y), returning the size of
    /// the laid-out block.
    pub fn draw(&self, mask: &mut Mask, text: &str, x: f32, y: f32, px: f32) -> (f32, f32) {
        let scale = PxScale::from(px.max(1.0));
        let scaled = self.font.as_scaled(scale);
        let ascent = scaled.ascent();
        let line_h = scaled.height() + scaled.line_gap();
        let mut max_w: f32 = 0.0;
        let mut lines = 0;

        for (li, line) in text.split('\n').enumerate() {
            lines = li + 1;
            let baseline = y + ascent + li as f32 * line_h;
            let mut pen = x;
            let mut prev: Option<GlyphId> = None;

            for ch in line.chars() {
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
                if !using_fallback {
                    if let Some(p) = prev {
                        pen += fscale.kern(p, id);
                    }
                }

                let glyph = id.with_scale_and_position(scale, point(pen, baseline));
                if let Some(outlined) = font.outline_glyph(glyph) {
                    let bb = outlined.px_bounds();
                    let w = bb.width().ceil().max(0.0) as usize;
                    let h = bb.height().ceil().max(0.0) as usize;
                    if w > 0 && h > 0 {
                        let mut cov = vec![0u8; w * h];
                        outlined.draw(|gx, gy, c| {
                            let (gx, gy) = (gx as usize, gy as usize);
                            if gx < w && gy < h {
                                let v = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                                if v > cov[gy * w + gx] {
                                    cov[gy * w + gx] = v;
                                }
                            }
                        });
                        let gx = bb.min.x.round() as i32;
                        let gy = bb.min.y.round() as i32;
                        mask.blit_cov(gx, gy, w, h, &cov);
                        if self.synthetic_bold {
                            // Fake bold: a second pass one pixel to the right.
                            mask.blit_cov(gx + 1, gy, w, h, &cov);
                        }
                    }
                }
                pen += fscale.h_advance(id);
                prev = Some(id);
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
        for y in 0..mask.h as i32 {
            for x in 0..mask.w as i32 {
                if mask.get(x, y) > 0 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn rasterises_something() {
        let te = TextEngine::new();
        let mut mask = Mask::new(400, 120);
        te.draw(&mut mask, "Hello", 10.0, 10.0, 48.0);
        assert!(covered(&mask) > 100);
    }

    #[test]
    fn bundled_families_render() {
        let cat = FontCatalog::new();
        let mut te = TextEngine::new();
        for fam in [crate::fonts::BUILTIN_INTER, crate::fonts::BUILTIN_DEJAVU] {
            assert!(te.set_style(&cat, fam, false));
            let mut mask = Mask::new(400, 120);
            te.draw(&mut mask, "Rg2", 8.0, 8.0, 40.0);
            assert!(covered(&mask) > 50, "{fam} produced no pixels");
        }
    }

    #[test]
    fn bold_is_wider() {
        let cat = FontCatalog::new();
        let mut te = TextEngine::new();
        te.set_style(&cat, crate::fonts::BUILTIN_INTER, false);
        let mut a = Mask::new(600, 120);
        let wa = te.draw(&mut a, "Weight", 8.0, 8.0, 40.0).0;
        te.set_style(&cat, crate::fonts::BUILTIN_INTER, true);
        let mut b = Mask::new(600, 120);
        te.draw(&mut b, "Weight", 8.0, 8.0, 40.0);
        assert!(covered(&b) > covered(&a), "bold should cover more pixels");
        assert!(wa > 0.0);
    }
}
