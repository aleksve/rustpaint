//! The document: a straight-alpha RGBA pixel buffer plus file bookkeeping.

use std::{fmt, path::PathBuf};

use eframe::egui::{Color32, ColorImage};
use image::{imageops, RgbImage, RgbaImage};

use crate::ir::IRect;

/// Upper bound on canvas size (40 MP ≈ 160 MB of RGBA). Keeps a stray
/// 20000×20000 PNG from eating all the memory.
pub const MAX_PIXELS: usize = 40_000_000;
/// Maximum width or height accepted by the document layer.
pub const MAX_DIMENSION: usize = 8192;
/// Maximum size of a single straight-alpha RGBA document buffer.
pub const MAX_RGBA_BYTES: usize = MAX_PIXELS * 4;
/// Decoder allocation budget: the RGBA output plus room for codec working data.
pub const MAX_DECODE_BYTES: u64 = MAX_RGBA_BYTES as u64 + 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionError {
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

pub fn validate_dimensions(w: usize, h: usize) -> Result<(), DimensionError> {
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

pub struct Doc {
    pub w: usize,
    pub h: usize,
    /// Straight (non-premultiplied) RGBA, 4 bytes per pixel, row major.
    pub px: Vec<u8>,
    pub path: Option<PathBuf>,
    pub modified: bool,
}

#[inline]
pub fn color_to_rgba(c: Color32) -> [u8; 4] {
    c.to_srgba_unmultiplied()
}

#[inline]
pub fn rgba_to_color(p: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
}

/// Flatten straight-alpha RGBA onto white for formats such as JPEG.
pub fn flatten_onto_white(img: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(img.width(), img.height());
    for (src, dst) in img.pixels().zip(out.pixels_mut()) {
        let a = u32::from(src[3]);
        let inv = 255 - a;
        dst[0] = ((u32::from(src[0]) * a + 255 * inv + 127) / 255) as u8;
        dst[1] = ((u32::from(src[1]) * a + 255 * inv + 127) / 255) as u8;
        dst[2] = ((u32::from(src[2]) * a + 255 * inv + 127) / 255) as u8;
    }
    out
}

impl Doc {
    pub fn new(w: usize, h: usize, bg: Option<Color32>) -> Result<Self, DimensionError> {
        validate_dimensions(w, h)?;
        let mut px = vec![0u8; w * h * 4];
        if let Some(c) = bg {
            let [r, g, b, a] = color_to_rgba(c);
            for chunk in px.as_chunks_mut::<4>().0 {
                chunk.copy_from_slice(&[r, g, b, a]);
            }
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
        for chunk in self.px.as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&v);
        }
    }

    pub fn clear_rect(&mut self, r: IRect) {
        let r = r.clamp(self.w as i32, self.h as i32);
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let i = (y as usize * self.w + x as usize) * 4;
                self.px[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    /// Copy a rectangle out of the canvas (straight RGBA).
    pub fn region(&self, r: IRect) -> Vec<u8> {
        let r = r.clamp(self.w as i32, self.h as i32);
        let (w, h) = (r.w().max(0) as usize, r.h().max(0) as usize);
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = ((r.y0 as usize + y) * self.w + r.x0 as usize) * 4;
            let dst = y * w * 4;
            out[dst..dst + w * 4].copy_from_slice(&self.px[src..src + w * 4]);
        }
        out
    }

    /// Write a buffer back into the canvas with *over* compositing.
    pub fn blit_over(&mut self, src: &[u8], sw: usize, sh: usize, dx: i32, dy: i32) {
        for y in 0..sh as i32 {
            for x in 0..sw as i32 {
                let si = (y as usize * sw + x as usize) * 4;
                let s = [src[si], src[si + 1], src[si + 2], src[si + 3]];
                if s[3] == 0 {
                    continue;
                }
                let (tx, ty) = (dx + x, dy + y);
                if let Some(di) = self.idx(tx, ty) {
                    let d = [
                        self.px[di],
                        self.px[di + 1],
                        self.px[di + 2],
                        self.px[di + 3],
                    ];
                    let o = over(d, s);
                    self.px[di..di + 4].copy_from_slice(&o);
                }
            }
        }
    }

    /// Crop a rectangle out of the canvas as a `ColorImage` (texture uploads).
    pub fn rect_color_image(&self, r: IRect) -> ColorImage {
        let r = r.clamp(self.w as i32, self.h as i32);
        let (w, h) = (r.w().max(0) as usize, r.h().max(0) as usize);
        ColorImage::from_rgba_unmultiplied([w, h], &self.region(r))
    }

    pub fn to_color_image(&self) -> ColorImage {
        ColorImage::from_rgba_unmultiplied([self.w, self.h], &self.px)
    }

    pub fn to_rgba_image(&self) -> RgbaImage {
        RgbaImage::from_raw(self.w as u32, self.h as u32, self.px.clone())
            .expect("buffer size matches dimensions")
    }

    pub fn replace_image(&mut self, img: RgbaImage) {
        let (w, h) = img.dimensions();
        self.w = w as usize;
        self.h = h as usize;
        self.px = img.into_raw();
    }

    pub fn resize_canvas(
        &mut self,
        nw: usize,
        nh: usize,
        scale_content: bool,
        bg: Option<Color32>,
    ) -> Result<(), DimensionError> {
        validate_dimensions(nw, nh)?;
        let img = self.to_rgba_image();
        let fitted = if scale_content {
            let mut dst = RgbaImage::new(nw as u32, nh as u32);
            // Preserve aspect ratio of the artwork: scale to fit, centre it.
            let sx = nw as f32 / self.w as f32;
            let sy = nh as f32 / self.h as f32;
            let s = sx.min(sy).clamp(0.01, 64.0);
            let tw = ((self.w as f32 * s).round() as u32).max(1);
            let th = ((self.h as f32 * s).round() as u32).max(1);
            let scaled = imageops::resize(&img, tw, th, imageops::FilterType::Triangle);
            let ox = (nw as i64 - tw as i64) / 2;
            let oy = (nh as i64 - th as i64) / 2;
            imageops::replace(&mut dst, &scaled, ox.max(0), oy.max(0));
            dst
        } else {
            let mut dst = RgbaImage::new(nw as u32, nh as u32);
            imageops::replace(&mut dst, &img, 0, 0);
            dst
        };
        let mut out = Doc::from_image(fitted, self.path.clone())?;
        if let Some(c) = bg {
            let v = color_to_rgba(c);
            for chunk in out.px.as_chunks_mut::<4>().0 {
                if chunk[3] == 0 {
                    chunk.copy_from_slice(&v);
                }
            }
        }
        out.modified = true;
        let path = self.path.clone();
        *self = out;
        self.path = path;
        Ok(())
    }

    pub fn flip_h(&mut self) {
        let img = imageops::flip_horizontal(&self.to_rgba_image());
        self.replace_image(img);
    }

    pub fn flip_v(&mut self) {
        let img = imageops::flip_vertical(&self.to_rgba_image());
        self.replace_image(img);
    }

    pub fn rotate90(&mut self, clockwise: bool) {
        let img = if clockwise {
            imageops::rotate90(&self.to_rgba_image())
        } else {
            imageops::rotate270(&self.to_rgba_image())
        };
        self.replace_image(img);
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
        for chunk in self.px.as_chunks_mut::<4>().0 {
            if chunk[3] == 0 {
                continue;
            }
            let y = (0.299 * chunk[0] as f32 + 0.587 * chunk[1] as f32 + 0.114 * chunk[2] as f32)
                .round() as u8;
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
        let (nw, nh) = (r.w() as usize, r.h() as usize);
        self.w = nw;
        self.h = nh;
        self.px = region;
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }
}

/// Straight-alpha "source over destination".
#[inline]
pub fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = src[3] as f32 / 255.0;
    if sa <= 0.0 {
        return dst;
    }
    if sa >= 1.0 {
        return src;
    }
    let da = dst[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 1e-6 {
        return [0, 0, 0, 0];
    }
    let mix = |s: u8, d: u8| -> u8 {
        (((s as f32 * sa) + (d as f32 * da * (1.0 - sa))) / oa)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        (oa * 255.0).round() as u8,
    ]
}

pub struct Composite<'a> {
    pub base: &'a [u8],
    pub base_rect: IRect,
    pub mask: &'a crate::mask::Mask,
    pub color: [u8; 4],
    pub opacity: f32,
    pub erase: bool,
}

/// Composite a coverage mask onto the canvas relative to a local snapshot.
pub fn composite_mask(px: &mut [u8], w: usize, h: usize, region: IRect, opts: Composite<'_>) {
    let r = region.clamp(w as i32, h as i32).intersect(opts.base_rect);
    let op = opts.opacity.clamp(0.0, 1.0);
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let cov = opts.mask.get(x, y) as f32 / 255.0 * op;
            let i = (y as usize * w + x as usize) * 4;
            let bi = ((y - opts.base_rect.y0) as usize * opts.base_rect.w() as usize
                + (x - opts.base_rect.x0) as usize)
                * 4;
            if cov <= 0.0 {
                px[i..i + 4].copy_from_slice(&opts.base[bi..bi + 4]);
                continue;
            }
            let b = [
                opts.base[bi],
                opts.base[bi + 1],
                opts.base[bi + 2],
                opts.base[bi + 3],
            ];
            let out = if opts.erase {
                let a = (b[3] as f32 * (1.0 - cov)).round().clamp(0.0, 255.0) as u8;
                [b[0], b[1], b[2], a]
            } else {
                let src = [
                    opts.color[0],
                    opts.color[1],
                    opts.color[2],
                    (opts.color[3] as f32 * cov).round().clamp(0.0, 255.0) as u8,
                ];
                over(b, src)
            };
            px[i..i + 4].copy_from_slice(&out);
        }
    }
}

/// Restore a rectangle of the canvas from a snapshot buffer.
pub fn restore_region(px: &mut [u8], w: usize, h: usize, base: &[u8], base_rect: IRect, r: IRect) {
    let r = r.clamp(w as i32, h as i32).intersect(base_rect);
    let bw = base_rect.w().max(0) as usize;
    let bh = base_rect.h().max(0) as usize;
    if r.is_empty() || base.len() < bw.saturating_mul(bh).saturating_mul(4) {
        return;
    }
    for y in r.y0..r.y1 {
        let s = ((y - base_rect.y0) as usize * bw + (r.x0 - base_rect.x0) as usize) * 4;
        let e = s + r.w() as usize * 4;
        let ds = (y as usize * w + r.x0 as usize) * 4;
        px[ds..ds + r.w() as usize * 4].copy_from_slice(&base[s..e]);
    }
}

/// Scanline flood fill with tolerance (4-connected).
pub fn flood_fill(
    doc: &mut Doc,
    x: i32,
    y: i32,
    color: Color32,
    tolerance: u8,
    clip: Option<IRect>,
) {
    let Some(target) = doc.get(x, y) else { return };
    // Filling is confined to the active selection, like MS Paint.
    let bounds = clip
        .unwrap_or(IRect::new(0, 0, doc.w as i32, doc.h as i32))
        .clamp(doc.w as i32, doc.h as i32);
    if !bounds.contains(x, y) {
        return;
    }
    let (bx0, by0) = (bounds.x0, bounds.y0);
    let (bx1, by1) = (bounds.x0 + bounds.w(), bounds.y0 + bounds.h());
    let fill = color_to_rgba(color);
    if target == fill && tolerance == 0 {
        return;
    }
    let tol = tolerance as i32;
    let matches = |p: [u8; 4]| -> bool {
        (p[0] as i32 - target[0] as i32).abs() <= tol
            && (p[1] as i32 - target[1] as i32).abs() <= tol
            && (p[2] as i32 - target[2] as i32).abs() <= tol
            && (p[3] as i32 - target[3] as i32).abs() <= tol
    };
    let bounds_w = bounds.w() as usize;
    let mut visited = vec![false; bounds_w * bounds.h() as usize];
    let visited_index = |px: i32, py: i32| ((py - by0) as usize) * bounds_w + (px - bx0) as usize;
    let mut stack: Vec<(i32, i32)> = vec![(x, y)];
    while let Some((sx, sy)) = stack.pop() {
        let mut cx = sx;
        // walk left
        while cx >= bx0 && !visited[visited_index(cx, sy)] && doc.get(cx, sy).is_some_and(matches) {
            cx -= 1;
        }
        cx += 1;
        let mut span_up = false;
        let mut span_down = false;
        while cx < bx1 && !visited[visited_index(cx, sy)] && doc.get(cx, sy).is_some_and(matches) {
            visited[visited_index(cx, sy)] = true;
            doc.set(cx, sy, fill);
            if sy > by0 {
                let up_ok =
                    !visited[visited_index(cx, sy - 1)] && doc.get(cx, sy - 1).is_some_and(matches);
                if up_ok && !span_up {
                    stack.push((cx, sy - 1));
                    span_up = true;
                } else if !up_ok {
                    span_up = false;
                }
            }
            if sy < by1 - 1 {
                let dn_ok =
                    !visited[visited_index(cx, sy + 1)] && doc.get(cx, sy + 1).is_some_and(matches);
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
        let img =
            RgbaImage::from_raw(3, 1, vec![0, 0, 0, 0, 10, 20, 30, 255, 0, 100, 200, 128]).unwrap();
        let rgb = flatten_onto_white(&img);
        assert_eq!(rgb.as_raw(), &[255, 255, 255, 10, 20, 30, 127, 177, 227]);
    }

    #[test]
    fn flood_fill_respects_canvas_and_clip_edges() {
        let mut doc = Doc::new(4, 3, Some(Color32::WHITE)).unwrap();
        flood_fill(
            &mut doc,
            0,
            0,
            Color32::BLACK,
            0,
            Some(IRect::new(0, 0, 2, 2)),
        );
        assert_eq!(doc.color_at(0, 0), Color32::BLACK);
        assert_eq!(doc.color_at(1, 1), Color32::BLACK);
        assert_eq!(doc.color_at(2, 0), Color32::WHITE);
        assert_eq!(doc.color_at(0, 2), Color32::WHITE);
    }

    #[test]
    fn flood_fill_supports_offset_clip_bounds() {
        let mut doc = Doc::new(6, 5, Some(Color32::WHITE)).unwrap();
        flood_fill(
            &mut doc,
            2,
            1,
            Color32::BLACK,
            0,
            Some(IRect::new(2, 1, 5, 4)),
        );
        assert_eq!(doc.color_at(2, 1), Color32::BLACK);
        assert_eq!(doc.color_at(4, 3), Color32::BLACK);
        assert_eq!(doc.color_at(1, 1), Color32::WHITE);
        assert_eq!(doc.color_at(5, 3), Color32::WHITE);
        assert_eq!(doc.color_at(2, 4), Color32::WHITE);
    }
}
