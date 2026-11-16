//! Pixel coverage mask.
//!
//! Every painting operation (brush, pencil, shapes, text) is first rasterised
//! into an 8-bit coverage mask and then composited onto the canvas *from the
//! snapshot taken when the stroke started*.  That is what keeps a semi
//! transparent stroke from building up at self-overlaps, and it gives every
//! tool the same anti-aliased quality.

use std::collections::HashMap;

use crate::{geom::distance_to_segment, ir::IRect};

pub struct Mask {
    pub w: usize,
    pub h: usize,
    d: HashMap<usize, u8>,
    bb: Option<IRect>,
}

#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Mask {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            d: HashMap::new(),
            bb: None,
        }
    }

    /// Zero the mask. Sparse storage makes this proportional to touched pixels.
    pub fn clear(&mut self) {
        self.d.clear();
        self.bb = None;
    }

    pub fn bbox(&self) -> Option<IRect> {
        self.bb
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.w as i32 && y < self.h as i32)
            .then_some(y as usize * self.w + x as usize)
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u8 {
        self.index(x, y)
            .and_then(|i| self.d.get(&i))
            .copied()
            .unwrap_or(0)
    }

    #[inline]
    fn touch(&mut self, x: i32, y: i32) {
        self.bb = Some(match self.bb {
            None => IRect::px(x, y),
            Some(b) => b.union_px(x, y),
        });
    }

    /// Keep the strongest coverage (normal painting).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, cov: f32) {
        if cov <= 0.0 {
            return;
        }
        let Some(i) = self.index(x, y) else {
            return;
        };
        let v = (cov.min(1.0) * 255.0 + 0.5) as u8;
        if v == 0 {
            return;
        }
        if v > self.d.get(&i).copied().unwrap_or(0) {
            self.d.insert(i, v);
            self.touch(x, y);
        }
    }

    /// Accumulate coverage (air brush).
    #[inline]
    pub fn add(&mut self, x: i32, y: i32, cov: f32) {
        if cov <= 0.0 {
            return;
        }
        let old = self.get(x, y) as f32 / 255.0;
        self.put(x, y, (old + cov).min(1.0));
    }

    /// Anti-aliased (or hard) stroke between two points.
    pub fn capsule(&mut self, a: (f32, f32), b: (f32, f32), radius: f32, hardness: f32, aa: bool) {
        let (ax, ay) = a;
        let (bx, by) = b;
        let r = radius.max(0.15);
        let x0 = (ax.min(bx) - r - 2.0).floor() as i32;
        let x1 = (ax.max(bx) + r + 2.0).ceil() as i32;
        let y0 = (ay.min(by) - r - 2.0).floor() as i32;
        let y1 = (ay.max(by) + r + 2.0).ceil() as i32;
        let inner = r * hardness.clamp(0.0, 0.999);
        let inv_soft = 1.0 / (r - inner).max(0.0015);
        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let d = distance_to_segment(px, py, ax, ay, bx, by);
                let cov = if aa {
                    let edge = (r + 0.5 - d).clamp(0.0, 1.0);
                    if edge <= 0.0 {
                        continue;
                    }
                    if d <= inner {
                        edge
                    } else {
                        edge * smoothstep((r - d) * inv_soft).max(0.05)
                    }
                } else if d <= r {
                    1.0
                } else {
                    0.0
                };
                if cov > 0.0 {
                    self.put(x, y, cov);
                }
            }
        }
    }

    pub fn disc(&mut self, cx: f32, cy: f32, radius: f32, hardness: f32, aa: bool) {
        self.capsule((cx, cy), (cx, cy), radius, hardness, aa);
    }

    /// Filled or stroked rectangle, coordinates in image space.
    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, filled: bool) {
        let (x0, y0, x1, y1) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
        let cx = (x0 + x1) * 0.5;
        let cy = (y0 + y1) * 0.5;
        let hw = (x1 - x0) * 0.5;
        let hh = (y1 - y0) * 0.5;
        let pad = width * 0.5 + 2.0;
        let ix0 = (x0 - pad).floor() as i32;
        let ix1 = (x1 + pad).ceil() as i32;
        let iy0 = (y0 - pad).floor() as i32;
        let iy1 = (y1 + pad).ceil() as i32;
        for y in iy0..=iy1 {
            let py = y as f32 + 0.5;
            for x in ix0..=ix1 {
                let px = x as f32 + 0.5;
                let qx = (px - cx).abs() - hw;
                let qy = (py - cy).abs() - hh;
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let sd = outside + qx.max(qy).min(0.0);
                let cov = if filled {
                    (0.5 - sd).clamp(0.0, 1.0)
                } else {
                    let half = (width * 0.5).max(0.35);
                    (half + 0.5 - sd.abs()).clamp(0.0, 1.0)
                };
                if cov > 0.0 {
                    self.put(x, y, cov);
                }
            }
        }
    }

    /// Filled or stroked ellipse (centre + radii).
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, width: f32, filled: bool) {
        let (rx, ry) = (rx.abs().max(0.4), ry.abs().max(0.4));
        let pad = width * 0.5 + 2.0;
        let x0 = (cx - rx - pad).floor() as i32;
        let x1 = (cx + rx + pad).ceil() as i32;
        let y0 = (cy - ry - pad).floor() as i32;
        let y1 = (cy + ry + pad).ceil() as i32;
        for y in y0..=y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..=x1 {
                let px = x as f32 + 0.5 - cx;
                let u = px / rx;
                let v = py / ry;
                let rr = (u * u + v * v).sqrt();
                let sd = if rr < 1e-5 {
                    -rx.min(ry)
                } else {
                    // first order distance: (f - 1) / |grad f|
                    let gx = u / (rr * rx);
                    let gy = v / (rr * ry);
                    let g = (gx * gx + gy * gy).sqrt().max(1e-6);
                    (rr - 1.0) / g
                };
                let cov = if filled {
                    (0.5 - sd).clamp(0.0, 1.0)
                } else {
                    let half = (width * 0.5).max(0.35);
                    (half + 0.5 - sd.abs()).clamp(0.0, 1.0)
                };
                if cov > 0.0 {
                    self.put(x, y, cov);
                }
            }
        }
    }

    /// Filled polygon using 4x supersampled scanlines with exact horizontal coverage.
    pub fn polygon_fill(&mut self, pts: &[(f32, f32)]) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let (mut minx, mut maxx, mut miny, mut maxy) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in pts {
            minx = minx.min(p.0);
            maxx = maxx.max(p.0);
            miny = miny.min(p.1);
            maxy = maxy.max(p.1);
        }
        let bx0 = (minx - 1.0).floor() as i32;
        let bx1 = (maxx + 1.0).ceil() as i32;
        let by0 = (miny - 1.0).floor() as i32;
        let by1 = (maxy + 1.0).ceil() as i32;
        let bw = (bx1 - bx0).max(1) as usize;
        const SS: i32 = 4;
        let mut acc = vec![0f32; bw];
        let mut xs: Vec<f32> = Vec::with_capacity(16);
        for y in by0..by1 {
            for v in acc.iter_mut() {
                *v = 0.0;
            }
            for s in 0..SS {
                let sy = y as f32 + (s as f32 + 0.5) / SS as f32;
                xs.clear();
                for i in 0..n {
                    let a = pts[i];
                    let b = pts[(i + 1) % n];
                    if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                        let t = (sy - a.1) / (b.1 - a.1);
                        xs.push(a.0 + t * (b.0 - a.0));
                    }
                }
                if xs.len() < 2 {
                    continue;
                }
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mut i = 0;
                while i + 1 < xs.len() {
                    add_span(&mut acc, bx0, xs[i], xs[i + 1], 1.0 / SS as f32);
                    i += 2;
                }
            }
            for (i, a) in acc.iter().enumerate() {
                if *a > 0.004 {
                    self.put(bx0 + i as i32, y, *a);
                }
            }
        }
    }

    /// Stroke a poly-line (optional closed).
    pub fn polyline(&mut self, pts: &[(f32, f32)], width: f32, closed: bool) {
        let r = (width * 0.5).max(0.4);
        if pts.len() < 2 {
            if let Some(p) = pts.first() {
                self.disc(p.0, p.1, r, 1.0, true);
            }
            return;
        }
        for i in 0..pts.len() - 1 {
            let a = pts[i];
            let b = pts[i + 1];
            self.capsule(a, b, r, 1.0, true);
        }
        if closed {
            let a = pts[pts.len() - 1];
            let b = pts[0];
            self.capsule(a, b, r, 1.0, true);
        }
    }

    /// Copy an 8-bit coverage bitmap (fonts) into the mask.
    pub fn blit_cov(&mut self, ox: i32, oy: i32, w: usize, h: usize, cov: &[u8]) {
        for y in 0..h {
            for x in 0..w {
                let v = cov[y * w + x];
                if v > 0 {
                    self.put(ox + x as i32, oy + y as i32, v as f32 / 255.0);
                }
            }
        }
    }
}

fn add_span(acc: &mut [f32], ox: i32, xa: f32, xb: f32, w: f32) {
    let lo = ox as f32;
    let hi = ox as f32 + acc.len() as f32;
    let xa = xa.max(lo);
    let xb = xb.min(hi);
    if xb <= xa {
        return;
    }
    let i0 = xa.floor() as i32;
    let i1 = xb.floor() as i32;
    if i0 == i1 {
        let idx = (i0 - ox).clamp(0, acc.len() as i32 - 1) as usize;
        acc[idx] += (xb - xa) * w;
        return;
    }
    let idx0 = (i0 - ox).clamp(0, acc.len() as i32 - 1) as usize;
    acc[idx0] += (i0 as f32 + 1.0 - xa) * w;
    for i in (i0 + 1)..i1 {
        let idx = i - ox;
        if idx >= 0 && (idx as usize) < acc.len() {
            acc[idx as usize] += w;
        }
    }
    let idx1 = (i1 - ox).clamp(0, acc.len() as i32 - 1) as usize;
    acc[idx1] += (xb - i1 as f32) * w;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_tracks_drawing() {
        let mut m = Mask::new(32, 32);
        assert!(m.bbox().is_none());
        m.disc(10.0, 10.0, 3.0, 1.0, true);
        let b = m.bbox().unwrap();
        assert!(b.x0 < 10 && b.x1 > 10 && b.y0 < 10 && b.y1 > 10);
        m.clear();
        assert!(m.bbox().is_none());
        assert_eq!(m.get(10, 10), 0);
    }

    #[test]
    fn polygon_fill_covers_interior() {
        let mut m = Mask::new(40, 40);
        let pts = [(5.0, 5.0), (35.0, 5.0), (20.0, 35.0)];
        m.polygon_fill(&pts);
        assert_eq!(m.get(20, 10), 255);
        assert_eq!(m.get(1, 1), 0);
        // edge is anti-aliased, so it must be partial but non-zero
        let e = m.get(12, 20);
        assert!(e > 0 && e < 255, "edge coverage {}", e);
    }

    #[test]
    fn large_canvas_mask_is_sparse() {
        let mut m = Mask::new(8192, 8192);
        m.disc(10.0, 10.0, 2.0, 1.0, false);
        assert!(m.d.len() < 64);
    }
}
