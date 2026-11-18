//! Sparse tiled 8-bit pixel coverage mask.
//!
//! Every painting operation (brush, pencil, shapes, text) is first rasterised
//! into an 8-bit coverage mask and then composited onto the canvas *from the
//! snapshot taken when the stroke started*. That keeps a semi-transparent
//! stroke from building up at self-overlaps and gives every tool consistent
//! anti-aliasing.
//!
//! Storage uses fixed 64×64 tiles allocated on demand: untouched regions of an
//! 8192×8192 canvas cost zero pixel storage, while active areas get O(1)
//! bit-shift indexing, dense ~1 byte/pixel memory use, and contiguous row spans
//! during compositing (no per-pixel hashing).

use crate::{geom::distance_to_segment, ir::IRect};

pub(crate) const TILE: usize = 64;
const TILE_SHIFT: usize = 6;
const TILE_MASK: usize = TILE - 1;
const TILE_LEN: usize = TILE * TILE;
/// Keep up to this many cleared tiles in a small free-list (256 × 4 KB = 1 MB)
/// so interactive shape/text drags avoid repeated heap traffic without holding
/// onto tens of megabytes after a full-canvas fill.
const MAX_POOLED_TILES: usize = 256;

type Tile = Box<[u8; TILE_LEN]>;

pub(crate) struct Mask {
    w: usize,
    h: usize,
    tiles_x: usize,
    tiles: Vec<Option<Tile>>,
    used: Vec<usize>,
    pool: Vec<Tile>,
    bb: Option<IRect>,
    dirty: Option<IRect>,
}

#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn cov_to_u8(cov: f32) -> u8 {
    if cov <= 0.0 || !cov.is_finite() {
        0
    } else {
        (cov.min(1.0) * 255.0 + 0.5) as u8
    }
}

impl Mask {
    pub fn new(w: usize, h: usize) -> Self {
        let tiles_x = w.div_ceil(TILE);
        let tiles_y = h.div_ceil(TILE);
        let tile_count = tiles_x.saturating_mul(tiles_y);
        let mut tiles = Vec::with_capacity(tile_count);
        tiles.resize_with(tile_count, || None);
        Self {
            w,
            h,
            tiles_x,
            tiles,
            used: Vec::new(),
            pool: Vec::new(),
            bb: None,
            dirty: None,
        }
    }

    #[cfg(test)]
    pub fn width(&self) -> usize {
        self.w
    }

    #[cfg(test)]
    pub fn height(&self) -> usize {
        self.h
    }

    #[cfg(test)]
    pub fn allocated_tiles(&self) -> usize {
        self.used.len()
    }

    /// Zero the mask in time proportional to touched tiles.
    pub fn clear(&mut self) {
        for ti in self.used.drain(..) {
            if let Some(mut tile) = self.tiles[ti].take()
                && self.pool.len() < MAX_POOLED_TILES
            {
                tile.fill(0);
                self.pool.push(tile);
            }
        }
        self.bb = None;
        self.dirty = None;
    }

    #[inline]
    pub fn bbox(&self) -> Option<IRect> {
        self.bb
    }

    /// Return and clear the rectangle modified since the last `take_dirty`.
    #[inline]
    pub fn take_dirty(&mut self) -> Option<IRect> {
        self.dirty.take()
    }

    #[inline]
    fn coords(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if x < 0 || y < 0 {
            return None;
        }
        let (ux, uy) = (x as usize, y as usize);
        if ux >= self.w || uy >= self.h {
            return None;
        }
        let tile_idx = (uy >> TILE_SHIFT) * self.tiles_x + (ux >> TILE_SHIFT);
        let cell_idx = ((uy & TILE_MASK) << TILE_SHIFT) | (ux & TILE_MASK);
        Some((tile_idx, cell_idx))
    }

    /// Convert a floating-point rectangle to inclusive pixel bounds clipped to
    /// the mask. Invalid or wholly off-canvas rectangles produce no work.
    #[inline]
    fn pixel_bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<(i32, i32, i32, i32)> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let (x0, y0, x1, y1) = (x0.floor(), y0.floor(), x1.ceil(), y1.ceil());
        let max_x = self.w.saturating_sub(1).min(i32::MAX as usize) as f32;
        let max_y = self.h.saturating_sub(1).min(i32::MAX as usize) as f32;
        if !x0.is_finite()
            || !y0.is_finite()
            || !x1.is_finite()
            || !y1.is_finite()
            || x0 > x1
            || y0 > y1
            || x1 < 0.0
            || y1 < 0.0
            || x0 > max_x
            || y0 > max_y
        {
            return None;
        }
        let clip = |v: f32, max: f32| v.clamp(0.0, max) as i32;
        Some((
            clip(x0, max_x),
            clip(y0, max_y),
            clip(x1, max_x),
            clip(y1, max_y),
        ))
    }

    #[cfg(test)]
    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u8 {
        let Some((ti, ci)) = self.coords(x, y) else {
            return 0;
        };
        self.tiles[ti].as_ref().map_or(0, |tile| tile[ci])
    }

    /// Visit contiguous horizontal chunks across `[x0, x1)` on row `y`.
    /// Untouched tiles yield `None` so callers can copy snapshot spans in bulk.
    #[inline]
    pub fn for_each_row_span(
        &self,
        y: i32,
        x0: i32,
        x1: i32,
        mut f: impl FnMut(i32, usize, Option<&[u8]>),
    ) {
        if y < 0 || (y as usize) >= self.h || x0 >= x1 {
            return;
        }
        let start = x0.max(0) as usize;
        let end = (x1.max(0) as usize).min(self.w);
        if start >= end {
            return;
        }
        let ty = y as usize >> TILE_SHIFT;
        let row_in_tile = (y as usize & TILE_MASK) << TILE_SHIFT;
        let mut x = start;
        while x < end {
            let tx = x >> TILE_SHIFT;
            let in_tile_x = x & TILE_MASK;
            let len = (TILE - in_tile_x).min(end - x);
            let tile_idx = ty * self.tiles_x + tx;
            let slice = self.tiles[tile_idx]
                .as_ref()
                .map(|t| &t[row_in_tile + in_tile_x..row_in_tile + in_tile_x + len]);
            f(x as i32, len, slice);
            x += len;
        }
    }

    #[inline]
    fn touch(&mut self, x: i32, y: i32) {
        self.bb = Some(match self.bb {
            None => IRect::px(x, y),
            Some(b) => b.union_px(x, y),
        });
        self.dirty = Some(match self.dirty {
            None => IRect::px(x, y),
            Some(b) => b.union_px(x, y),
        });
    }

    /// Keep the strongest 8-bit coverage (normal painting & glyph rasterisation).
    #[inline]
    pub fn put_u8(&mut self, x: i32, y: i32, v: u8) {
        if v == 0 {
            return;
        }
        let Some((ti, ci)) = self.coords(x, y) else {
            return;
        };
        let slot = match self.tiles[ti].as_mut() {
            Some(tile) => &mut tile[ci],
            None => {
                let tile = self.pool.pop().unwrap_or_else(|| Box::new([0u8; TILE_LEN]));
                self.used.push(ti);
                &mut self.tiles[ti].insert(tile)[ci]
            }
        };
        if v > *slot {
            *slot = v;
            self.touch(x, y);
        }
    }

    /// Keep the strongest coverage from a normalised `[0, 1]` float.
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, cov: f32) {
        let v = cov_to_u8(cov);
        if v > 0 {
            self.put_u8(x, y, v);
        }
    }

    /// Accumulate coverage (airbrush) in a single tile lookup.
    #[inline]
    pub fn add(&mut self, x: i32, y: i32, cov: f32) {
        let delta = cov_to_u8(cov);
        if delta == 0 {
            return;
        }
        let Some((ti, ci)) = self.coords(x, y) else {
            return;
        };
        let slot = match self.tiles[ti].as_mut() {
            Some(tile) => &mut tile[ci],
            None => {
                let tile = self.pool.pop().unwrap_or_else(|| Box::new([0u8; TILE_LEN]));
                self.used.push(ti);
                &mut self.tiles[ti].insert(tile)[ci]
            }
        };
        let next = slot.saturating_add(delta);
        if next > *slot {
            *slot = next;
            self.touch(x, y);
        }
    }

    /// Anti-aliased (or hard) capsule stroke between two points, clipped per
    /// scanline so long diagonal lines do not visit the entire bounding box.
    pub fn capsule(&mut self, a: (f32, f32), b: (f32, f32), radius: f32, hardness: f32, aa: bool) {
        let (ax, ay) = a;
        let (bx, by) = b;
        if !ax.is_finite()
            || !ay.is_finite()
            || !bx.is_finite()
            || !by.is_finite()
            || !radius.is_finite()
            || !hardness.is_finite()
        {
            return;
        }
        let r = radius.max(0.15);
        let reach = r + if aa { 0.5 } else { 0.0 };
        let Some((x0, y0, x1, y1)) = self.pixel_bounds(
            ax.min(bx) - reach - 1.0,
            ay.min(by) - reach - 1.0,
            ax.max(bx) + reach + 1.0,
            ay.max(by) + reach + 1.0,
        ) else {
            return;
        };
        let hard = hardness.clamp(0.0, 1.0);
        let inner = r * hard;
        let inv_soft = if hard < 0.999 {
            1.0 / (r - inner).max(0.0015)
        } else {
            0.0
        };
        let dx = bx - ax;
        let dy = by - ay;
        let flat_y = dy.abs() <= 1e-6;

        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            // Any segment point q(t) within `reach` of (px, py) satisfies
            // |q_y(t) - py| <= reach, constraining t to [t0, t1] and px to
            // [min(q_x(t0), q_x(t1)) - reach, max(q_x(t0), q_x(t1)) + reach].
            let (sx0, sx1) = if flat_y {
                if (py - ay).abs() > reach {
                    continue;
                }
                (x0, x1)
            } else {
                let ta = ((py - reach - ay) / dy).clamp(0.0, 1.0);
                let tb = ((py + reach - ay) / dy).clamp(0.0, 1.0);
                let xa = ax + dx * ta;
                let xb = ax + dx * tb;
                let lo = ((xa.min(xb) - reach - 0.5).floor() as i32).max(x0);
                let hi = ((xa.max(xb) + reach - 0.5).ceil() as i32).min(x1);
                if lo > hi {
                    continue;
                }
                (lo, hi)
            };

            for x in sx0..=sx1 {
                let px = x as f32 + 0.5;
                let d = distance_to_segment(px, py, ax, ay, bx, by);
                let cov = if aa {
                    let edge = (r + 0.5 - d).clamp(0.0, 1.0);
                    if edge <= 0.0 {
                        continue;
                    }
                    if inv_soft == 0.0 || d <= inner {
                        edge
                    } else {
                        edge * smoothstep((r + 0.5 - d) * inv_soft)
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
        if !x0.is_finite()
            || !y0.is_finite()
            || !x1.is_finite()
            || !y1.is_finite()
            || !width.is_finite()
        {
            return;
        }
        let (x0, y0, x1, y1) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
        let cx = (x0 + x1) * 0.5;
        let cy = (y0 + y1) * 0.5;
        let hw = (x1 - x0) * 0.5;
        let hh = (y1 - y0) * 0.5;
        let half = (width * 0.5).max(0.35);
        let pad = if filled { 1.5 } else { half + 1.5 };
        let Some((ix0, iy0, ix1, iy1)) = self.pixel_bounds(x0 - pad, y0 - pad, x1 + pad, y1 + pad)
        else {
            return;
        };

        // For hollow rectangles, pixels with qx <= -(half + 0.5) and
        // qy <= -(half + 0.5) have sd <= -(half + 0.5), so coverage is zero.
        let hole_hw = hw - (half + 0.5);
        let hole_hh = hh - (half + 0.5);

        let mut eval = |x: i32, y: i32, py: f32| {
            let px = x as f32 + 0.5;
            let qx = (px - cx).abs() - hw;
            let qy = (py - cy).abs() - hh;
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let sd = outside + qx.max(qy).min(0.0);
            let cov = if filled {
                (0.5 - sd).clamp(0.0, 1.0)
            } else {
                (half + 0.5 - sd.abs()).clamp(0.0, 1.0)
            };
            if cov > 0.0 {
                self.put(x, y, cov);
            }
        };

        for y in iy0..=iy1 {
            let py = y as f32 + 0.5;
            if !filled && hole_hw > 1.0 && hole_hh > 0.0 && (py - cy).abs() <= hole_hh {
                let skip0 = ((cx - hole_hw - 0.5).ceil() as i32).max(ix0);
                let skip1 = ((cx + hole_hw - 0.5).floor() as i32).min(ix1);
                if skip0 <= skip1 {
                    for x in ix0..skip0 {
                        eval(x, y, py);
                    }
                    for x in (skip1 + 1)..=ix1 {
                        eval(x, y, py);
                    }
                    continue;
                }
            }
            for x in ix0..=ix1 {
                eval(x, y, py);
            }
        }
    }

    /// Filled or stroked ellipse (centre + radii).
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, width: f32, filled: bool) {
        if !cx.is_finite()
            || !cy.is_finite()
            || !rx.is_finite()
            || !ry.is_finite()
            || !width.is_finite()
        {
            return;
        }
        let (rx, ry) = (rx.abs().max(0.4), ry.abs().max(0.4));
        let half = (width * 0.5).max(0.35);
        let pad = if filled { 1.5 } else { half + 1.5 };
        let Some((x0, y0, x1, y1)) =
            self.pixel_bounds(cx - rx - pad, cy - ry - pad, cx + rx + pad, cy + ry + pad)
        else {
            return;
        };

        // Since |grad r| <= 1 / min(rx, ry), any point with r <= inner_k has
        // sd = (r - 1) / |grad r| <= -(half + 1.0) < -(half + 0.5) => cov == 0.
        let min_r = rx.min(ry);
        let inner_k = if !filled {
            (1.0 - (half + 1.0) / min_r).max(0.0)
        } else {
            0.0
        };

        let mut eval = |x: i32, y: i32, py: f32, v: f32| {
            let _ = py;
            let px = x as f32 + 0.5 - cx;
            let u = px / rx;
            let rr = (u * u + v * v).sqrt();
            let sd = if rr < 1e-5 {
                -min_r
            } else {
                let gx = u / (rr * rx);
                let gy = v / (rr * ry);
                let g = (gx * gx + gy * gy).sqrt().max(1e-6);
                (rr - 1.0) / g
            };
            let cov = if filled {
                (0.5 - sd).clamp(0.0, 1.0)
            } else {
                (half + 0.5 - sd.abs()).clamp(0.0, 1.0)
            };
            if cov > 0.0 {
                self.put(x, y, cov);
            }
        };

        for y in y0..=y1 {
            let py = y as f32 + 0.5 - cy;
            let v = py / ry;
            if inner_k > 0.0 && v.abs() < inner_k {
                let dx = rx * (inner_k * inner_k - v * v).sqrt();
                let skip0 = ((cx - dx - 0.5).ceil() as i32).max(x0);
                let skip1 = ((cx + dx - 0.5).floor() as i32).min(x1);
                if skip0 <= skip1 {
                    for x in x0..skip0 {
                        eval(x, y, py, v);
                    }
                    for x in (skip1 + 1)..=x1 {
                        eval(x, y, py, v);
                    }
                    continue;
                }
            }
            for x in x0..=x1 {
                eval(x, y, py, v);
            }
        }
    }

    /// Filled polygon using 4x supersampled scanlines with exact horizontal coverage.
    pub fn polygon_fill(&mut self, pts: &[(f32, f32)]) {
        let n = pts.len();
        if n < 3 || pts.iter().any(|&(x, y)| !x.is_finite() || !y.is_finite()) {
            return;
        }
        let (mut minx, mut maxx, mut miny, mut maxy) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in pts {
            minx = minx.min(p.0);
            maxx = maxx.max(p.0);
            miny = miny.min(p.1);
            maxy = maxy.max(p.1);
        }
        let (mut bx0, mut bx1) = ((minx - 1.0).floor(), (maxx + 1.0).ceil());
        let (mut by0, mut by1) = ((miny - 1.0).floor(), (maxy + 1.0).ceil());
        let canvas_x1 = self.w.min(i32::MAX as usize) as f32;
        let canvas_y1 = self.h.min(i32::MAX as usize) as f32;
        if self.w == 0
            || self.h == 0
            || bx1 <= 0.0
            || by1 <= 0.0
            || bx0 >= canvas_x1
            || by0 >= canvas_y1
        {
            return;
        }
        bx0 = bx0.clamp(0.0, canvas_x1);
        bx1 = bx1.clamp(0.0, canvas_x1);
        by0 = by0.clamp(0.0, canvas_y1);
        by1 = by1.clamp(0.0, canvas_y1);
        let (bx0, bx1, by0, by1) = (bx0 as i32, bx1 as i32, by0 as i32, by1 as i32);
        if bx0 >= bx1 || by0 >= by1 {
            return;
        }
        let bw = (bx1 - bx0) as usize;
        const SS: i32 = 4;
        let mut acc = vec![0f32; bw];
        let mut xs: Vec<f32> = Vec::with_capacity(16);
        for y in by0..by1 {
            acc.fill(0.0);
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
                xs.sort_by(|a, b| a.total_cmp(b));
                let mut i = 0;
                while i + 1 < xs.len() {
                    add_span(&mut acc, bx0, xs[i], xs[i + 1], 1.0 / SS as f32);
                    i += 2;
                }
            }
            for (i, &a) in acc.iter().enumerate() {
                if a > 0.0 {
                    self.put(bx0 + i as i32, y, a);
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
        for seg in pts.windows(2) {
            self.capsule(seg[0], seg[1], r, 1.0, true);
        }
        if closed && pts.len() > 2 {
            self.capsule(pts[pts.len() - 1], pts[0], r, 1.0, true);
        }
    }

    /// Copy an 8-bit coverage bitmap into the mask without float conversion.
    #[cfg(test)]
    pub fn blit_cov(&mut self, ox: i32, oy: i32, w: usize, h: usize, cov: &[u8]) {
        let Some(len) = w.checked_mul(h) else {
            return;
        };
        if cov.len() < len {
            return;
        }
        for sy in 0..h {
            let Ok(sy_offset) = i32::try_from(sy) else {
                break;
            };
            let Some(dy) = oy.checked_add(sy_offset) else {
                continue;
            };
            for sx in 0..w {
                let Ok(sx_offset) = i32::try_from(sx) else {
                    break;
                };
                let Some(dx) = ox.checked_add(sx_offset) else {
                    continue;
                };
                let v = cov[sy * w + sx];
                if v > 0 {
                    self.put_u8(dx, dy, v);
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
    let fill_start = (i0 + 1 - ox).max(0) as usize;
    let fill_end = ((i1 - ox).max(0) as usize).min(acc.len());
    for slot in &mut acc[fill_start..fill_end] {
        *slot += w;
    }
    let rem = xb - i1 as f32;
    if rem > 0.0 {
        let idx1 = (i1 - ox).clamp(0, acc.len() as i32 - 1) as usize;
        acc[idx1] += rem * w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_and_dirty_track_drawing() {
        let mut m = Mask::new(128, 128);
        assert!(m.bbox().is_none());
        assert!(m.take_dirty().is_none());

        m.disc(10.0, 10.0, 3.0, 1.0, true);
        let first = m.take_dirty().unwrap();
        assert_eq!(m.bbox(), Some(first));
        assert!(m.take_dirty().is_none());

        m.disc(90.0, 90.0, 2.0, 1.0, true);
        let second = m.take_dirty().unwrap();
        assert!(second.x0 >= 86 && second.y0 >= 86);
        assert_eq!(m.bbox(), Some(first.union(second)));

        m.clear();
        assert!(m.bbox().is_none());
        assert!(m.take_dirty().is_none());
        assert_eq!(m.get(10, 10), 0);
    }

    #[test]
    fn polygon_fill_covers_interior() {
        let mut m = Mask::new(40, 40);
        let pts = [(5.0, 5.0), (35.0, 5.0), (20.0, 35.0)];
        m.polygon_fill(&pts);
        assert_eq!(m.get(20, 10), 255);
        assert_eq!(m.get(1, 1), 0);
        let e = m.get(12, 20);
        assert!(e > 0 && e < 255, "edge coverage {e}");
    }

    #[test]
    fn large_canvas_mask_is_sparse() {
        let mut m = Mask::new(8192, 8192);
        m.disc(10.0, 10.0, 2.0, 1.0, false);
        assert_eq!(m.allocated_tiles(), 1);
    }

    #[test]
    fn scanline_clipped_primitives_match_brute_force() {
        let (w, h) = (96, 80);

        // Capsule across shallow, steep, and horizontal angles.
        for (a, b, r, hard, aa) in [
            ((4.3, 7.2), (88.7, 71.9), 3.5, 0.5, true),
            ((70.1, 3.0), (12.4, 75.6), 1.5, 1.0, false),
            ((10.0, 40.2), (80.0, 40.2), 5.0, 0.0, true),
        ] {
            let mut fast = Mask::new(w, h);
            fast.capsule(a, b, r, hard, aa);
            let inner = r * hard;
            let inv_soft = if hard < 0.999 {
                1.0 / (r - inner).max(0.0015)
            } else {
                0.0
            };
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let d = distance_to_segment(x as f32 + 0.5, y as f32 + 0.5, a.0, a.1, b.0, b.1);
                    let cov = if aa {
                        let edge = (r + 0.5 - d).clamp(0.0, 1.0);
                        if inv_soft == 0.0 || d <= inner {
                            edge
                        } else {
                            edge * smoothstep((r + 0.5 - d) * inv_soft)
                        }
                    } else if d <= r {
                        1.0
                    } else {
                        0.0
                    };
                    assert_eq!(fast.get(x, y), cov_to_u8(cov), "capsule ({x}, {y})");
                }
            }
        }

        // Hollow rectangle and hollow high-aspect ellipse.
        let mut m_rect = Mask::new(w, h);
        m_rect.rect(8.2, 6.7, 86.4, 72.1, 4.0, false);
        assert_eq!(m_rect.get(48, 40), 0);
        assert_eq!(m_rect.get(8, 40), 255);

        let mut m_ell = Mask::new(w, h);
        let (cx, cy, rx, ry, width) = (48.3, 39.6, 38.2, 16.7, 3.5);
        m_ell.ellipse(cx, cy, rx, ry, width, false);
        let half = width * 0.5;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let u = (x as f32 + 0.5 - cx) / rx;
                let v = (y as f32 + 0.5 - cy) / ry;
                let rr = (u * u + v * v).sqrt();
                let sd = if rr < 1e-5 {
                    -rx.min(ry)
                } else {
                    let g = ((u / (rr * rx)).powi(2) + (v / (rr * ry)).powi(2))
                        .sqrt()
                        .max(1e-6);
                    (rr - 1.0) / g
                };
                let expected = cov_to_u8((half + 0.5 - sd.abs()).clamp(0.0, 1.0));
                assert_eq!(m_ell.get(x, y), expected, "ellipse ({x}, {y})");
            }
        }
    }

    #[test]
    fn soft_brush_fades_smoothly_without_edge_plateau() {
        let mut m = Mask::new(64, 64);
        m.disc(32.0, 32.0, 20.0, 0.0, true);
        let near_edge = m.get(51, 32); // distance 19.5 from center
        assert!(near_edge < 4, "expected near-zero edge, got {near_edge}");
    }

    #[test]
    fn out_of_bounds_pixels_are_ignored_without_overflow() {
        let mut m = Mask::new(8, 8);
        assert_eq!(m.get(-1, 1), 0);
        assert_eq!(m.get(0, -1), 0);
        m.put(-1, 0, 1.0);
        m.add(0, -1, 1.0);
        assert!(m.bbox().is_none());
        m.disc(0.0, 0.0, 4.0, 1.0, true);
        assert!(m.get(0, 0) > 0);
        assert_eq!(m.get(7, 7), 0);
    }

    #[test]
    fn extreme_strokes_are_clipped_before_iteration() {
        let mut m = Mask::new(8, 8);
        m.capsule(
            (-100_000_000.0, 4.0),
            (100_000_000.0, 4.0),
            16.0,
            1.0,
            false,
        );
        assert!(m.get(0, 4) > 0);
        assert!(m.get(7, 4) > 0);
    }

    #[test]
    fn undersized_coverage_is_ignored() {
        let mut m = Mask::new(8, 8);
        m.blit_cov(0, 0, 2, 2, &[255, 255, 255]);
        assert!(m.bbox().is_none());
    }
}
