//! The document: a straight-alpha RGBA pixel buffer plus image operations.

use std::{fmt, path::PathBuf};

use eframe::egui::{Color32, ColorImage};
use image::{RgbImage, RgbaImage, imageops};

use crate::ir::IRect;

/// Upper bound on canvas size (40 MP ≈ 160 MB of RGBA). Keeps a stray
/// 20000×20000 PNG from eating all the memory.
pub(crate) const MAX_PIXELS: usize = 40_000_000;
/// Maximum width or height accepted by the document layer.
pub(crate) const MAX_DIMENSION: usize = 8192;
/// Maximum size of a single straight-alpha RGBA document buffer.
pub(crate) const MAX_RGBA_BYTES: usize = MAX_PIXELS * 4;
/// Decoder allocation budget: the RGBA output plus room for codec working data.
pub(crate) const MAX_DECODE_BYTES: u64 = MAX_RGBA_BYTES as u64 + 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DimensionError {
    Empty,
    WidthTooLarge,
    HeightTooLarge,
    TooManyPixels,
}

impl fmt::Display for DimensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "canvas dimensions must be at least 1×1"),
            Self::WidthTooLarge => write!(f, "canvas width exceeds {MAX_DIMENSION} pixels"),
            Self::HeightTooLarge => write!(f, "canvas height exceeds {MAX_DIMENSION} pixels"),
            Self::TooManyPixels => write!(f, "canvas exceeds the {MAX_PIXELS} pixel limit"),
        }
    }
}

pub(crate) fn validate_dimensions(w: usize, h: usize) -> Result<(), DimensionError> {
    if w == 0 || h == 0 {
        return Err(DimensionError::Empty);
    }
    if w > MAX_DIMENSION {
        return Err(DimensionError::WidthTooLarge);
    }
    if h > MAX_DIMENSION {
        return Err(DimensionError::HeightTooLarge);
    }
    let pixels = w.checked_mul(h).ok_or(DimensionError::TooManyPixels)?;
    if pixels > MAX_PIXELS || pixels.checked_mul(4).is_none() {
        return Err(DimensionError::TooManyPixels);
    }
    Ok(())
}

pub(crate) struct Doc {
    pub(crate) w: usize,
    pub(crate) h: usize,
    /// Straight (non-premultiplied) RGBA, 4 bytes per pixel, row major.
    pub(crate) px: Vec<u8>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) modified: bool,
}

#[inline]
pub(crate) fn color_to_rgba(c: Color32) -> [u8; 4] {
    c.to_srgba_unmultiplied()
}

#[inline]
pub(crate) fn rgba_to_color(p: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
}

/// Flatten a straight-alpha RGBA buffer onto opaque white (used for JPEG export).
pub(crate) fn flatten_onto_white(px: &[u8], w: u32, h: u32) -> RgbImage {
    let pixel_count = (w as usize) * (h as usize);
    let mut raw = vec![255u8; pixel_count * 3];
    for (src, dst) in px
        .as_chunks::<4>()
        .0
        .iter()
        .zip(raw.as_chunks_mut::<3>().0.iter_mut())
    {
        let a = u32::from(src[3]);
        if a == 255 {
            *dst = [src[0], src[1], src[2]];
        } else if a > 0 {
            let inv = 255 - a;
            dst[0] = ((u32::from(src[0]) * a + 255 * inv + 127) / 255) as u8;
            dst[1] = ((u32::from(src[1]) * a + 255 * inv + 127) / 255) as u8;
            dst[2] = ((u32::from(src[2]) * a + 255 * inv + 127) / 255) as u8;
        }
    }
    RgbImage::from_raw(w, h, raw).expect("RGB buffer matches image dimensions")
}

/// Premultiply an RGBA buffer in place so linear resampling never bleeds black
/// from `[0, 0, 0, 0]` transparent neighbours into opaque edges.
fn premultiply_in_place(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a == 0 {
            *p = [0, 0, 0, 0];
        } else if a < 255 {
            p[0] = ((u32::from(p[0]) * a + 127) / 255) as u8;
            p[1] = ((u32::from(p[1]) * a + 127) / 255) as u8;
            p[2] = ((u32::from(p[2]) * a + 127) / 255) as u8;
        }
    }
}

fn unpremultiply_in_place(px: &mut [u8]) {
    for p in px.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a == 0 {
            *p = [0, 0, 0, 0];
        } else if a < 255 {
            p[0] = ((u32::from(p[0]) * 255 + a / 2) / a).min(255) as u8;
            p[1] = ((u32::from(p[1]) * 255 + a / 2) / a).min(255) as u8;
            p[2] = ((u32::from(p[2]) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

impl Doc {
    pub fn new(w: usize, h: usize, bg: Option<Color32>) -> Result<Self, DimensionError> {
        validate_dimensions(w, h)?;
        let mut px = vec![0u8; w * h * 4];
        if let Some(c) = bg {
            let v = color_to_rgba(c);
            px.as_chunks_mut::<4>().0.fill(v);
        }
        Ok(Self {
            w,
            h,
            px,
            path: None,
            modified: false,
        })
    }

    pub fn from_image(img: RgbaImage, path: Option<PathBuf>) -> Result<Self, DimensionError> {
        let (w, h) = img.dimensions();
        let (w, h) = (w as usize, h as usize);
        validate_dimensions(w, h)?;
        Ok(Self {
            w,
            h,
            px: img.into_raw(),
            path,
            modified: false,
        })
    }

    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            None
        } else {
            Some((y as usize * self.w + x as usize) * 4)
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<[u8; 4]> {
        let i = self.idx(x, y)?;
        Some([self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]])
    }

    #[cfg(test)]
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if let Some(i) = self.idx(x, y) {
            self.px[i..i + 4].copy_from_slice(&c);
        }
    }

    pub fn color_at(&self, x: i32, y: i32) -> Color32 {
        rgba_to_color(self.get(x, y).unwrap_or([0, 0, 0, 0]))
    }

    pub fn fill_all(&mut self, c: Color32) {
        let v = color_to_rgba(c);
        self.px.as_chunks_mut::<4>().0.fill(v);
    }

    pub fn clear_rect(&mut self, r: IRect) {
        let r = r.clamp(self.w as i32, self.h as i32);
        if r.is_empty() {
            return;
        }
        let row_bytes = r.w() as usize * 4;
        for y in r.y0..r.y1 {
            let start = (y as usize * self.w + r.x0 as usize) * 4;
            self.px[start..start + row_bytes].fill(0);
        }
    }

    /// Copy a rectangle out of the canvas (straight RGBA).
    pub fn region(&self, r: IRect) -> Vec<u8> {
        let r = r.clamp(self.w as i32, self.h as i32);
        let (w, h) = (r.w() as usize, r.h() as usize);
        let mut out = vec![0u8; w * h * 4];
        let row_bytes = w * 4;
        for y in 0..h {
            let src = ((r.y0 as usize + y) * self.w + r.x0 as usize) * 4;
            let dst = y * row_bytes;
            out[dst..dst + row_bytes].copy_from_slice(&self.px[src..src + row_bytes]);
        }
        out
    }

    /// Write a buffer back into the canvas with pre-clipped *over* compositing.
    pub fn blit_over(&mut self, src: &[u8], sw: usize, sh: usize, dx: i32, dy: i32) {
        let Some(required) = sw.checked_mul(sh).and_then(|n| n.checked_mul(4)) else {
            return;
        };
        if src.len() < required || sw == 0 || sh == 0 || self.w == 0 || self.h == 0 {
            return;
        }
        let dx64 = i64::from(dx);
        let dy64 = i64::from(dy);
        let tx0 = dx64.max(0);
        let ty0 = dy64.max(0);
        let tx1 = (dx64 + sw as i64).min(self.w as i64);
        let ty1 = (dy64 + sh as i64).min(self.h as i64);
        if tx0 >= tx1 || ty0 >= ty1 {
            return;
        }
        let copy_w = (tx1 - tx0) as usize;
        let sx0 = (tx0 - dx64) as usize;
        let sy0 = (ty0 - dy64) as usize;
        let tx0 = tx0 as usize;
        let ty0 = ty0 as usize;
        let rows = (ty1 - ty0 as i64) as usize;

        for row in 0..rows {
            let s_start = ((sy0 + row) * sw + sx0) * 4;
            let d_start = ((ty0 + row) * self.w + tx0) * 4;
            let s_row = src[s_start..s_start + copy_w * 4].as_chunks::<4>().0;
            let d_row = self.px[d_start..d_start + copy_w * 4]
                .as_chunks_mut::<4>()
                .0;
            for (&s, d) in s_row.iter().zip(d_row.iter_mut()) {
                if s[3] != 0 {
                    *d = over(*d, s);
                }
            }
        }
    }

    /// Crop a rectangle out of the canvas directly into a `ColorImage` without
    /// allocating an intermediate byte buffer.
    pub fn rect_color_image(&self, r: IRect) -> ColorImage {
        let r = r.clamp(self.w as i32, self.h as i32);
        let (w, h) = (r.w() as usize, r.h() as usize);
        let mut pixels = Vec::with_capacity(w * h);
        for y in 0..h {
            let start = ((r.y0 as usize + y) * self.w + r.x0 as usize) * 4;
            let row = self.px[start..start + w * 4].as_chunks::<4>().0;
            pixels.extend(
                row.iter()
                    .map(|&p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])),
            );
        }
        ColorImage {
            size: [w, h],
            source_size: eframe::egui::Vec2::new(w as f32, h as f32),
            pixels,
        }
    }

    pub fn to_color_image(&self) -> ColorImage {
        ColorImage::from_rgba_unmultiplied([self.w, self.h], &self.px)
    }

    pub fn resize_canvas(
        &mut self,
        nw: usize,
        nh: usize,
        scale_content: bool,
        bg: Option<Color32>,
    ) -> Result<(), DimensionError> {
        validate_dimensions(nw, nh)?;
        let mut out = Doc::new(nw, nh, bg)?;
        if scale_content {
            let sx = nw as f32 / self.w as f32;
            let sy = nh as f32 / self.h as f32;
            let s = sx.min(sy).clamp(0.001, MAX_DIMENSION as f32);
            let tw = ((self.w as f32 * s).round() as usize).clamp(1, nw) as u32;
            let th = ((self.h as f32 * s).round() as usize).clamp(1, nh) as u32;

            let mut src_buf = self.px.clone();
            premultiply_in_place(&mut src_buf);
            let src_img = RgbaImage::from_raw(self.w as u32, self.h as u32, src_buf)
                .expect("buffer size matches dimensions");
            let scaled = imageops::resize(&src_img, tw, th, imageops::FilterType::Triangle);
            let mut scaled_raw = scaled.into_raw();
            unpremultiply_in_place(&mut scaled_raw);

            let ox = ((nw as i32) - (tw as i32)) / 2;
            let oy = ((nh as i32) - (th as i32)) / 2;
            if bg.is_some() {
                out.blit_over(&scaled_raw, tw as usize, th as usize, ox, oy);
            } else {
                copy_raw_rect(
                    &mut out.px,
                    nw,
                    nh,
                    &scaled_raw,
                    tw as usize,
                    th as usize,
                    ox.max(0) as usize,
                    oy.max(0) as usize,
                );
            }
        } else {
            // Preserve the artwork verbatim (including intentional alpha holes)
            // inside its original top-left region; `bg` only fills newly added area.
            copy_raw_rect(&mut out.px, nw, nh, &self.px, self.w, self.h, 0, 0);
        }
        self.w = nw;
        self.h = nh;
        self.px = out.px;
        self.modified = true;
        Ok(())
    }

    /// Flip horizontally in place with zero heap allocation.
    pub fn flip_h(&mut self) {
        let row_bytes = self.w * 4;
        for row in self.px.chunks_exact_mut(row_bytes) {
            row.as_chunks_mut::<4>().0.reverse();
        }
    }

    /// Flip vertically in place with zero heap allocation.
    pub fn flip_v(&mut self) {
        if self.h < 2 || self.w == 0 {
            return;
        }
        let row_bytes = self.w * 4;
        let half = self.h / 2;
        for y in 0..half {
            let top = y * row_bytes;
            let bot = (self.h - 1 - y) * row_bytes;
            let (head, tail) = self.px.split_at_mut(bot);
            head[top..top + row_bytes].swap_with_slice(&mut tail[..row_bytes]);
        }
    }

    /// Rotate 90 degrees without cloning `self.px` first.
    pub fn rotate90(&mut self, clockwise: bool) {
        let (ow, oh) = (self.w, self.h);
        let (nw, nh) = (oh, ow);
        let mut dst = vec![0u8; nw * nh * 4];
        let src_px = self.px.as_chunks::<4>().0;
        let dst_px = dst.as_chunks_mut::<4>().0;
        if clockwise {
            for y in 0..oh {
                let row = y * ow;
                let dx = oh - 1 - y;
                for x in 0..ow {
                    dst_px[x * nw + dx] = src_px[row + x];
                }
            }
        } else {
            for y in 0..oh {
                let row = y * ow;
                for x in 0..ow {
                    let dy = ow - 1 - x;
                    dst_px[dy * nw + y] = src_px[row + x];
                }
            }
        }
        self.w = nw;
        self.h = nh;
        self.px = dst;
    }

    pub fn invert(&mut self) {
        for chunk in self.px.as_chunks_mut::<4>().0 {
            if chunk[3] == 0 {
                continue;
            }
            chunk[0] = 255 - chunk[0];
            chunk[1] = 255 - chunk[1];
            chunk[2] = 255 - chunk[2];
        }
    }

    pub fn grayscale(&mut self) {
        // Integer sRGB / BT.709 luma: 0.2126 R + 0.7152 G + 0.0722 B.
        for chunk in self.px.as_chunks_mut::<4>().0 {
            if chunk[3] == 0 {
                continue;
            }
            let y = ((54 * u32::from(chunk[0])
                + 183 * u32::from(chunk[1])
                + 19 * u32::from(chunk[2])
                + 128)
                >> 8)
                .min(255) as u8;
            chunk[0] = y;
            chunk[1] = y;
            chunk[2] = y;
        }
    }

    pub fn crop(&mut self, r: IRect) {
        let r = r.clamp(self.w as i32, self.h as i32);
        if r.is_empty() {
            return;
        }
        let region = self.region(r);
        self.w = r.w() as usize;
        self.h = r.h() as usize;
        self.px = region;
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_raw_rect(
    dst: &mut [u8],
    dw: usize,
    dh: usize,
    src: &[u8],
    sw: usize,
    sh: usize,
    ox: usize,
    oy: usize,
) {
    if ox >= dw || oy >= dh {
        return;
    }
    let cw = sw.min(dw - ox);
    let ch = sh.min(dh - oy);
    let row_bytes = cw * 4;
    for y in 0..ch {
        let s = y * sw * 4;
        let d = ((oy + y) * dw + ox) * 4;
        dst[d..d + row_bytes].copy_from_slice(&src[s..s + row_bytes]);
    }
}

/// Straight-alpha "source over destination" using exact integer rounding.
#[inline]
pub(crate) fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = u32::from(src[3]);
    if sa == 0 {
        return dst;
    }
    let da = u32::from(dst[3]);
    if sa == 255 || da == 0 {
        return src;
    }
    let inv_sa = 255 - sa;
    // Fast path for the common case: compositing over an opaque backdrop.
    if da == 255 {
        return [
            ((u32::from(src[0]) * sa + u32::from(dst[0]) * inv_sa + 127) / 255) as u8,
            ((u32::from(src[1]) * sa + u32::from(dst[1]) * inv_sa + 127) / 255) as u8,
            ((u32::from(src[2]) * sa + u32::from(dst[2]) * inv_sa + 127) / 255) as u8,
            255,
        ];
    }
    let sa_w = sa * 255;
    let da_w = da * inv_sa;
    let oa_w = sa_w + da_w;
    let half = oa_w / 2;
    [
        ((u32::from(src[0]) * sa_w + u32::from(dst[0]) * da_w + half) / oa_w) as u8,
        ((u32::from(src[1]) * sa_w + u32::from(dst[1]) * da_w + half) / oa_w) as u8,
        ((u32::from(src[2]) * sa_w + u32::from(dst[2]) * da_w + half) / oa_w) as u8,
        ((oa_w + 127) / 255) as u8,
    ]
}

pub(crate) struct Composite<'a> {
    pub base: &'a [u8],
    pub base_rect: IRect,
    pub mask: &'a crate::mask::Mask,
    pub color: [u8; 4],
    pub opacity: f32,
    pub erase: bool,
}

/// Composite a tiled coverage mask onto the canvas relative to a local snapshot.
/// Untouched 64-pixel tile spans are restored via direct slice copies.
pub(crate) fn composite_mask(
    px: &mut [u8],
    w: usize,
    h: usize,
    region: IRect,
    opts: Composite<'_>,
) {
    let r = region.clamp(w as i32, h as i32).intersect(opts.base_rect);
    let bw = opts.base_rect.w() as usize;
    let bh = opts.base_rect.h() as usize;
    let Some(canvas_need) = w.checked_mul(h).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    let Some(base_need) = bw.checked_mul(bh).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    if r.is_empty() || px.len() < canvas_need || opts.base.len() < base_need {
        return;
    }

    let op = (opts.opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let src_alpha = u32::from(opts.color[3]);
    let [cr, cg, cb, _] = opts.color;

    for y in r.y0..r.y1 {
        let base_row = (y - opts.base_rect.y0) as usize * bw;
        let dst_row = y as usize * w;
        opts.mask
            .for_each_row_span(y, r.x0, r.x1, |sx, len, cov_span| {
                let bi = (base_row + (sx - opts.base_rect.x0) as usize) * 4;
                let di = (dst_row + sx as usize) * 4;
                let byte_len = len * 4;
                let base_slice = &opts.base[bi..bi + byte_len];
                let dst_slice = &mut px[di..di + byte_len];

                let Some(covs) = cov_span else {
                    dst_slice.copy_from_slice(base_slice);
                    return;
                };
                if op == 0 {
                    dst_slice.copy_from_slice(base_slice);
                    return;
                }

                let base_px = base_slice.as_chunks::<4>().0;
                let dst_px = dst_slice.as_chunks_mut::<4>().0;
                if opts.erase {
                    for ((&m, &b), d) in covs.iter().zip(base_px.iter()).zip(dst_px.iter_mut()) {
                        if m == 0 || b[3] == 0 {
                            *d = b;
                            continue;
                        }
                        let eff = (u32::from(m) * op + 127) / 255;
                        let a = ((u32::from(b[3]) * (255 - eff) + 127) / 255) as u8;
                        *d = [b[0], b[1], b[2], a];
                    }
                } else {
                    // Combine opacity and source colour alpha once (range 0..=65025).
                    let op_sa = op * src_alpha;
                    for ((&m, &b), d) in covs.iter().zip(base_px.iter()).zip(dst_px.iter_mut()) {
                        if m == 0 {
                            *d = b;
                            continue;
                        }
                        let a = ((u32::from(m) * op_sa + 32512) / 65025) as u8;
                        *d = over(b, [cr, cg, cb, a]);
                    }
                }
            });
    }
}

/// Restore a rectangle of the canvas from a snapshot buffer.
pub(crate) fn restore_region(
    px: &mut [u8],
    w: usize,
    h: usize,
    base: &[u8],
    base_rect: IRect,
    r: IRect,
) {
    let r = r.clamp(w as i32, h as i32).intersect(base_rect);
    let bw = base_rect.w() as usize;
    let bh = base_rect.h() as usize;
    let Some(canvas_need) = w.checked_mul(h).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    let Some(base_need) = bw.checked_mul(bh).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    if r.is_empty() || px.len() < canvas_need || base.len() < base_need {
        return;
    }
    let row_bytes = r.w() as usize * 4;
    for y in r.y0..r.y1 {
        let s = ((y - base_rect.y0) as usize * bw + (r.x0 - base_rect.x0) as usize) * 4;
        let d = (y as usize * w + r.x0 as usize) * 4;
        px[d..d + row_bytes].copy_from_slice(&base[s..s + row_bytes]);
    }
}

struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    fn new(bits: usize) -> Self {
        Self {
            words: vec![0u64; bits.div_ceil(64)],
        }
    }

    #[inline]
    fn get(&self, idx: usize) -> bool {
        (self.words[idx >> 6] >> (idx & 63)) & 1 != 0
    }

    #[inline]
    fn set(&mut self, idx: usize) {
        self.words[idx >> 6] |= 1u64 << (idx & 63);
    }
}

/// Scanline flood fill with tolerance (4-connected). Returns the bounding box
/// of pixels actually modified so callers only diff and upload that region.
pub(crate) fn flood_fill(
    doc: &mut Doc,
    x: i32,
    y: i32,
    color: Color32,
    tolerance: u8,
    clip: Option<IRect>,
) -> Option<IRect> {
    let target = doc.get(x, y)?;
    // Filling is confined to the active selection, like MS Paint.
    let bounds = clip
        .unwrap_or_else(|| IRect::new(0, 0, doc.w as i32, doc.h as i32))
        .clamp(doc.w as i32, doc.h as i32);
    if !bounds.contains(x, y) {
        return None;
    }
    let fill = color_to_rgba(color);
    if target == fill && tolerance == 0 {
        return None;
    }

    let (bx0, by0, bx1, by1) = (bounds.x0, bounds.y0, bounds.x1, bounds.y1);
    let doc_w = doc.w;
    let pixels = doc.px.as_chunks_mut::<4>().0;
    let mut changed: Option<IRect> = None;

    // Fast path for exact fills (`tolerance == 0`): since `target != fill`,
    // writing `fill` marks visited pixels directly with zero auxiliary memory.
    if tolerance == 0 {
        let mut stack = vec![(x, y)];
        while let Some((sx, sy)) = stack.pop() {
            let row = sy as usize * doc_w;
            let mut cx = sx;
            if pixels[row + cx as usize] != target {
                continue;
            }
            while cx > bx0 && pixels[row + (cx - 1) as usize] == target {
                cx -= 1;
            }
            let span_start = cx;
            let mut span_up = false;
            let mut span_down = false;
            while cx < bx1 && pixels[row + cx as usize] == target {
                pixels[row + cx as usize] = fill;
                if sy > by0 {
                    let up_ok = pixels[(sy - 1) as usize * doc_w + cx as usize] == target;
                    if up_ok && !span_up {
                        stack.push((cx, sy - 1));
                        span_up = true;
                    } else if !up_ok {
                        span_up = false;
                    }
                }
                if sy < by1 - 1 {
                    let dn_ok = pixels[(sy + 1) as usize * doc_w + cx as usize] == target;
                    if dn_ok && !span_down {
                        stack.push((cx, sy + 1));
                        span_down = true;
                    } else if !dn_ok {
                        span_down = false;
                    }
                }
                cx += 1;
            }
            let span = IRect::new(span_start, sy, cx, sy + 1);
            changed = Some(changed.map_or(span, |r| r.union(span)));
        }
        return changed;
    }

    let tol = i32::from(tolerance);
    let matches = |p: [u8; 4]| -> bool {
        (i32::from(p[0]) - i32::from(target[0])).abs() <= tol
            && (i32::from(p[1]) - i32::from(target[1])).abs() <= tol
            && (i32::from(p[2]) - i32::from(target[2])).abs() <= tol
            && (i32::from(p[3]) - i32::from(target[3])).abs() <= tol
    };

    let bounds_w = bounds.w() as usize;
    let mut visited = BitSet::new(bounds_w * bounds.h() as usize);
    let v_idx = |px: i32, py: i32| (py - by0) as usize * bounds_w + (px - bx0) as usize;

    let mut stack = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        if visited.get(v_idx(sx, sy)) {
            continue;
        }
        let row = sy as usize * doc_w;
        if !matches(pixels[row + sx as usize]) {
            continue;
        }
        let mut cx = sx;
        while cx > bx0
            && !visited.get(v_idx(cx - 1, sy))
            && matches(pixels[row + (cx - 1) as usize])
        {
            cx -= 1;
        }
        let mut span_up = false;
        let mut span_down = false;
        while cx < bx1 && !visited.get(v_idx(cx, sy)) && matches(pixels[row + cx as usize]) {
            visited.set(v_idx(cx, sy));
            let slot = &mut pixels[row + cx as usize];
            if *slot != fill {
                *slot = fill;
                changed = Some(changed.map_or_else(|| IRect::px(cx, sy), |r| r.union_px(cx, sy)));
            }
            if sy > by0 {
                let up_ok = !visited.get(v_idx(cx, sy - 1))
                    && matches(pixels[(sy - 1) as usize * doc_w + cx as usize]);
                if up_ok && !span_up {
                    stack.push((cx, sy - 1));
                    span_up = true;
                } else if !up_ok {
                    span_up = false;
                }
            }
            if sy < by1 - 1 {
                let dn_ok = !visited.get(v_idx(cx, sy + 1))
                    && matches(pixels[(sy + 1) as usize * doc_w + cx as usize]);
                if dn_ok && !span_down {
                    stack.push((cx, sy + 1));
                    span_down = true;
                } else if !dn_ok {
                    span_down = false;
                }
            }
            cx += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_enforce_pixel_and_axis_limits() {
        assert_eq!(validate_dimensions(0, 10), Err(DimensionError::Empty));
        assert_eq!(
            validate_dimensions(MAX_DIMENSION + 1, 1),
            Err(DimensionError::WidthTooLarge)
        );
        assert!(validate_dimensions(8192, 4882).is_ok());
        assert_eq!(
            validate_dimensions(8192, 4883),
            Err(DimensionError::TooManyPixels)
        );
        assert!(Doc::new(4, 3, None).is_ok());
    }

    #[test]
    fn flatten_onto_white_handles_alpha() {
        let px = [0, 0, 0, 0, 10, 20, 30, 255, 0, 100, 200, 128];
        let rgb = flatten_onto_white(&px, 3, 1);
        assert_eq!(rgb.as_raw(), &[255, 255, 255, 10, 20, 30, 127, 177, 227]);
    }

    #[test]
    fn flood_fill_respects_canvas_and_clip_edges() {
        let mut doc = Doc::new(4, 3, Some(Color32::WHITE)).unwrap();
        let dirty = flood_fill(
            &mut doc,
            0,
            0,
            Color32::BLACK,
            0,
            Some(IRect::new(0, 0, 2, 2)),
        );
        assert_eq!(dirty, Some(IRect::new(0, 0, 2, 2)));
        assert_eq!(doc.color_at(0, 0), Color32::BLACK);
        assert_eq!(doc.color_at(1, 1), Color32::BLACK);
        assert_eq!(doc.color_at(2, 0), Color32::WHITE);
        assert_eq!(doc.color_at(0, 2), Color32::WHITE);
    }

    #[test]
    fn flood_fill_supports_offset_clip_bounds_and_tolerance() {
        let mut doc = Doc::new(6, 5, Some(Color32::WHITE)).unwrap();
        doc.set(3, 2, [250, 250, 250, 255]);
        let dirty = flood_fill(
            &mut doc,
            2,
            1,
            Color32::BLACK,
            10,
            Some(IRect::new(2, 1, 5, 4)),
        );
        assert_eq!(dirty, Some(IRect::new(2, 1, 5, 4)));
        assert_eq!(doc.color_at(3, 2), Color32::BLACK);
        assert_eq!(doc.color_at(1, 1), Color32::WHITE);
        assert_eq!(doc.color_at(5, 3), Color32::WHITE);
        assert_eq!(doc.color_at(2, 4), Color32::WHITE);
    }

    #[test]
    fn in_place_flips_and_rotations_match_expected_pixels() {
        let mut doc = Doc::new(3, 2, Some(Color32::BLACK)).unwrap();
        doc.set(0, 0, [1, 0, 0, 255]);
        doc.set(2, 0, [2, 0, 0, 255]);
        doc.set(0, 1, [3, 0, 0, 255]);
        doc.set(2, 1, [4, 0, 0, 255]);

        doc.flip_h();
        assert_eq!(doc.get(0, 0), Some([2, 0, 0, 255]));
        assert_eq!(doc.get(2, 0), Some([1, 0, 0, 255]));
        doc.flip_v();
        assert_eq!(doc.get(0, 0), Some([4, 0, 0, 255]));
        assert_eq!(doc.get(2, 1), Some([1, 0, 0, 255]));

        doc.rotate90(true);
        assert_eq!((doc.w, doc.h), (2, 3));
        doc.rotate90(false);
        assert_eq!((doc.w, doc.h), (3, 2));
        assert_eq!(doc.get(0, 0), Some([4, 0, 0, 255]));
        assert_eq!(doc.get(2, 1), Some([1, 0, 0, 255]));
    }

    #[test]
    fn resize_canvas_preserves_transparent_holes_and_avoids_dark_fringes() {
        let mut doc = Doc::new(4, 4, Some(Color32::WHITE)).unwrap();
        doc.set(1, 1, [0, 0, 0, 0]);
        doc.resize_canvas(6, 6, false, Some(Color32::BLACK))
            .unwrap();
        // Original transparent pixel stays transparent; new margin gets bg.
        assert_eq!(doc.get(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(doc.get(5, 5), Some([0, 0, 0, 255]));

        // Scaling white next to transparent [0,0,0,0] must stay white (RGB=255),
        // not grey from interpolating zeroed straight-alpha channels.
        let mut fringe = Doc::new(2, 1, None).unwrap();
        fringe.set(0, 0, [255, 255, 255, 255]);
        fringe.set(1, 0, [0, 0, 0, 0]);
        fringe.resize_canvas(4, 2, true, None).unwrap();
        for x in 0..4 {
            let p = fringe.get(x, 0).unwrap();
            if p[3] > 0 {
                assert!(p[0] >= 254, "dark fringe detected at x={x}: {p:?}");
            }
        }
    }

    #[test]
    fn integer_over_matches_reference_float_formula() {
        for sa in [0u8, 1, 17, 128, 200, 254, 255] {
            for da in [0u8, 1, 64, 128, 254, 255] {
                let src = [220, 45, 130, sa];
                let dst = [30, 190, 80, da];
                let got = over(dst, src);

                let saf = sa as f32 / 255.0;
                let daf = da as f32 / 255.0;
                let oaf = saf + daf * (1.0 - saf);
                let exp = if sa == 0 {
                    dst
                } else if sa == 255 || oaf <= 1e-6 {
                    if sa == 255 { src } else { [0; 4] }
                } else {
                    let ch = |s: u8, d: u8| {
                        (((s as f32 * saf) + (d as f32 * daf * (1.0 - saf))) / oaf)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                    [
                        ch(src[0], dst[0]),
                        ch(src[1], dst[1]),
                        ch(src[2], dst[2]),
                        (oaf * 255.0).round() as u8,
                    ]
                };
                for i in 0..4 {
                    assert!(
                        got[i].abs_diff(exp[i]) <= 1,
                        "sa={sa} da={da} ch={i}: got={got:?} exp={exp:?}"
                    );
                }
            }
        }
    }
}
