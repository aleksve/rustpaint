//! Canvas text rendering and exact WYSIWYG caret/selection layout.
//!
//! Text is rasterised with `ab_glyph` straight into the tiled coverage mask the
//! brushes use (with no intermediate per-glyph bitmap allocation), so text gets
//! identical alpha compositing and opacity control. `TextEngine::layout` walks
//! the exact same glyph, fallback, kerning, tab, and synthetic-bold pipeline so
//! the on-canvas caret, selection highlight, and hit-testing align 1:1 with the
//! painted pixels in every font, weight, size, and zoom level.

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont, point};

use crate::assets::{FALLBACK_FONT, UI_FONT};
use crate::fonts::{BUILTIN_INTER, FontCatalog};
use crate::mask::Mask;

/// Laid-out character boundaries for a single explicit (`\n`-delimited) line.
#[derive(Clone, Debug)]
pub(crate) struct LineLayout {
    /// Top `y` offset from the text block origin (`line_index * line_h`).
    pub top: f32,
    /// `(byte_index, x_px)` for each character boundary on this line, always
    /// containing at least one entry `(line_byte_start, 0.0)` and ending with
    /// `(line_byte_end, line_width_px)`.
    pub stops: Vec<(usize, f32)>,
}

impl LineLayout {
    pub fn start_byte(&self) -> usize {
        self.stops.first().map_or(0, |s| s.0)
    }

    pub fn end_byte(&self) -> usize {
        self.stops.last().map_or(0, |s| s.0)
    }

    pub fn width(&self) -> f32 {
        self.stops.last().map_or(0.0, |s| s.1)
    }

    fn x_for_byte(&self, byte_idx: usize) -> f32 {
        let clamped = byte_idx.clamp(self.start_byte(), self.end_byte());
        match self.stops.binary_search_by_key(&clamped, |&(b, _)| b) {
            Ok(i) => self.stops[i].1,
            Err(0) => 0.0,
            Err(i) => self.stops[i - 1].1,
        }
    }

    fn nearest_byte(&self, rel_x: f32) -> usize {
        let mut best_byte = self.start_byte();
        let mut best_dist = f32::INFINITY;
        for &(b, x) in &self.stops {
            let d = (x - rel_x).abs();
            if d < best_dist {
                best_dist = d;
                best_byte = b;
            }
        }
        best_byte
    }
}

/// Exact multi-line (explicit `\n` only — never soft-wrapped) layout metrics.
#[derive(Clone, Debug)]
pub(crate) struct TextLayout {
    /// Baseline-to-baseline vertical stride in image pixels.
    pub line_h: f32,
    /// Visual height of a single line (`ascent - descent`) in image pixels.
    pub glyph_h: f32,
    /// Maximum line width across all lines (0.0 for empty text).
    pub width: f32,
    /// Total block height (`lines.len() * line_h`, always >= `line_h`).
    pub height: f32,
    pub lines: Vec<LineLayout>,
}

impl TextLayout {
    /// Return `(line_index, x_px, top_y_px)` relative to the text origin for
    /// `byte_idx`.
    pub fn caret_pos(&self, byte_idx: usize) -> (usize, f32, f32) {
        for (li, line) in self.lines.iter().enumerate() {
            let is_last = li + 1 == self.lines.len();
            if byte_idx <= line.end_byte() || is_last {
                return (li, line.x_for_byte(byte_idx), line.top);
            }
        }
        (0, 0.0, 0.0)
    }

    /// Snap a point `(rel_x, rel_y)` (in image px relative to text origin) to
    /// the closest character boundary.
    pub fn hit_test(&self, rel_x: f32, rel_y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let step = self.line_h.max(1.0);
        let raw = (rel_y / step).floor() as isize;
        let li = raw.clamp(0, self.lines.len() as isize - 1) as usize;
        self.lines[li].nearest_byte(rel_x)
    }

    /// Move `byte_idx` vertically by `delta_lines` (e.g. `-1` for Up, `+1` for
    /// Down), preserving `preferred_x` across consecutive vertical moves.
    pub fn vertical_move(
        &self,
        byte_idx: usize,
        delta_lines: isize,
        preferred_x: &mut Option<f32>,
    ) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let (li, cur_x, _) = self.caret_pos(byte_idx);
        let target_x = *preferred_x.get_or_insert(cur_x);
        let target_line = li as isize + delta_lines;
        if target_line < 0 {
            return 0;
        }
        if target_line >= self.lines.len() as isize {
            return self.lines.last().map_or(0, LineLayout::end_byte);
        }
        self.lines[target_line as usize].nearest_byte(target_x)
    }

    /// Return `(x0, y0, x1, y1)` highlight rectangles (in image px relative to
    /// text origin) covering the byte span between `a` and `b`.
    pub fn selection_rects(&self, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        let (lo, hi) = (a.min(b), a.max(b));
        if lo == hi || self.lines.is_empty() {
            return Vec::new();
        }
        let (l0, x0, _) = self.caret_pos(lo);
        let (l1, x1, _) = self.caret_pos(hi);
        let newline_stub = (self.line_h * 0.25).max(3.0);
        let mut out = Vec::with_capacity(l1.saturating_sub(l0) + 1);
        for li in l0..=l1 {
            let line = &self.lines[li];
            let sx0 = if li == l0 { x0 } else { 0.0 };
            let sx1 = if li == l1 {
                x1
            } else {
                line.width() + newline_stub
            };
            if sx1 > sx0 {
                out.push((sx0, line.top, sx1, line.top + self.glyph_h));
            }
        }
        out
    }
}

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

    /// Visual height of a single line at `px` in image pixels.
    pub fn line_height(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(PxScale::from(px.max(1.0)));
        (scaled.height() + scaled.line_gap()).max(1.0)
    }

    /// Compute exact per-character caret stops and line metrics for `text` at
    /// font size `px`, using the exact same glyph advances and kerning as
    /// [`Self::draw`].
    pub fn layout(&self, text: &str, px: f32) -> TextLayout {
        self.walk(None, text, 0.0, 0.0, px)
    }

    /// Draw `text` with its top-left corner at `(x, y)`, returning the laid-out
    /// block dimensions `(width, height)`.
    pub fn draw(&self, mask: &mut Mask, text: &str, x: f32, y: f32, px: f32) -> (f32, f32) {
        let layout = self.walk(Some(mask), text, x, y, px);
        (layout.width.max(1.0), layout.height)
    }

    fn walk(&self, mut mask: Option<&mut Mask>, text: &str, x: f32, y: f32, px: f32) -> TextLayout {
        let px = px.max(1.0);
        let scale = PxScale::from(px);
        let scaled = self.font.as_scaled(scale);
        let ascent = scaled.ascent();
        let glyph_h = scaled.height().max(1.0);
        let line_h = (glyph_h + scaled.line_gap()).max(1.0);
        // Scale synthetic bold stroke width with font size so both tiny labels
        // and large headlines look balanced.
        let bold_extra = if self.synthetic_bold {
            ((px / 32.0).round() as i32).clamp(1, 6)
        } else {
            0
        };
        let space_adv = scaled.h_advance(self.font.glyph_id(' '));
        let want_stops = mask.is_none();

        let mut max_w: f32 = 0.0;
        let mut lines = Vec::new();
        let mut line_byte_offset = 0usize;

        for (li, raw_line) in text.split('\n').enumerate() {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let top = li as f32 * line_h;
            let baseline = y + ascent + top;
            let mut pen = 0.0_f32;
            // Always stores a `GlyphId` belonging to `self.font` (never fallback).
            let mut prev_primary: Option<GlyphId> = None;
            let mut stops = if want_stops {
                let mut v = Vec::with_capacity(line.len() + 1);
                v.push((line_byte_offset, 0.0));
                v
            } else {
                Vec::new()
            };

            for (rel_byte, ch) in line.char_indices() {
                let next_byte = line_byte_offset + rel_byte + ch.len_utf8();
                if ch == '\r' {
                    if want_stops {
                        stops.push((next_byte, pen));
                    }
                    continue;
                }
                if ch == '\t' {
                    pen += space_adv * 4.0;
                    prev_primary = None;
                    if want_stops {
                        stops.push((next_byte, pen));
                    }
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

                if let Some(mask) = mask.as_deref_mut() {
                    let glyph = id.with_scale_and_position(scale, point(x + pen, baseline));
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
                                let (Some(dx), Some(dy)) =
                                    (ox.checked_add(gx_i), oy.checked_add(gy_i))
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
                }

                pen += fscale.h_advance(id) + bold_extra as f32;
                prev_primary = if using_fallback { None } else { Some(id) };
                if want_stops {
                    stops.push((next_byte, pen));
                }
            }

            max_w = max_w.max(pen);
            if want_stops {
                lines.push(LineLayout { top, stops });
            }
            line_byte_offset += raw_line.len() + 1;
        }

        let line_count = if want_stops {
            lines.len().max(1)
        } else {
            text.bytes().filter(|&b| b == b'\n').count() + 1
        };

        TextLayout {
            line_h,
            glyph_h,
            width: max_w,
            height: line_h * line_count as f32,
            lines,
        }
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

    #[test]
    fn layout_matches_draw_never_soft_wraps_and_hit_tests_multibyte_lines() {
        let mut cat = FontCatalog::bundled_only();
        let mut te = TextEngine::new();
        te.set_style(&mut cat, crate::fonts::BUILTIN_INTER, false);

        // A very long single line must stay on line 0 with caret at full width.
        let long =
            "A long single line with AV To kerning and æøå ÆØÅ that must never wrap in a box";
        let mut mask = Mask::new(2400, 120);
        let (drawn_w, drawn_h) = te.draw(&mut mask, long, 4.0, 4.0, 36.0);
        let lay = te.layout(long, 36.0);
        assert_eq!(lay.lines.len(), 1);
        assert!((lay.width - drawn_w).abs() < 1e-4);
        assert!((lay.height - drawn_h).abs() < 1e-4);
        let (li, cx, cy) = lay.caret_pos(long.len());
        assert_eq!(li, 0);
        assert!((cx - drawn_w).abs() < 1e-4);
        assert_eq!(cy, 0.0);

        // Explicit `\n` advances the caret to line 1 at x = 0; hit-test snaps to
        // valid UTF-8 boundaries on both lines.
        let multi = "Blåbær\nGrønn\n";
        let mlay = te.layout(multi, 32.0);
        assert_eq!(mlay.lines.len(), 3);
        let (l2, x2, y2) = mlay.caret_pos(multi.len());
        assert_eq!((l2, x2), (2, 0.0));
        assert!((y2 - 2.0 * mlay.line_h).abs() < 1e-4);
        for &(byte_idx, stop_x) in &mlay.lines[0].stops {
            assert!(multi.is_char_boundary(byte_idx));
            assert_eq!(mlay.hit_test(stop_x, mlay.line_h * 0.4), byte_idx);
        }
        assert_eq!(mlay.selection_rects(0, multi.len()).len(), 2);
    }
}
