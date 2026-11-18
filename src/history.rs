//! Diff based undo / redo.
//!
//! Only the bounding box of the changed pixels is stored, so a two pixel
//! pencil dab costs a handful of bytes while a full canvas fill costs two
//! copies of the canvas.  Total memory is capped and the oldest entries are
//! dropped first.

use crate::ir::IRect;

pub struct Diff {
    pub rect: IRect,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

impl Diff {
    fn bytes(&self) -> usize {
        self.before.len() + self.after.len()
    }
}

pub struct History {
    undo: Vec<Diff>,
    redo: Vec<Diff>,
    limit: usize,
    max_bytes: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(80, 192 * 1024 * 1024)
    }
}

impl History {
    pub fn new(limit: usize, max_bytes: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
            max_bytes,
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    fn total_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(|d| d.bytes())
            .sum()
    }

    fn trim(&mut self) {
        while self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        while self.total_bytes() > self.max_bytes && self.undo.len() > 1 {
            self.undo.remove(0);
        }
        while self.total_bytes() > self.max_bytes
            && !self.redo.is_empty()
            && (!self.undo.is_empty() || self.redo.len() > 1)
        {
            self.redo.remove(0);
        }
    }

    /// Record a change described by the full "before" and "after" buffers.
    /// Returns the changed rectangle, or `None` when nothing actually changed.
    pub fn record(&mut self, before: &[u8], after: &[u8], w: usize, h: usize) -> Option<IRect> {
        self.record_region(before, after, w, h, IRect::new(0, 0, w as i32, h as i32))
    }

    /// Record only the part of the image that could have changed.
    pub fn record_region(
        &mut self,
        before: &[u8],
        after: &[u8],
        w: usize,
        h: usize,
        region: IRect,
    ) -> Option<IRect> {
        self.record_with_rects(
            before,
            IRect::new(0, 0, w as i32, h as i32),
            after,
            w,
            h,
            region,
        )
    }

    /// Record a change when the before buffer contains only `before_rect`.
    /// The after buffer remains a full-canvas image, so local painting sessions
    /// do not need to retain a second full-canvas snapshot for undo history.
    pub fn record_local(
        &mut self,
        before: &[u8],
        before_rect: IRect,
        after: &[u8],
        w: usize,
        h: usize,
        region: IRect,
    ) -> Option<IRect> {
        self.record_with_rects(before, before_rect, after, w, h, region)
    }

    fn record_with_rects(
        &mut self,
        before: &[u8],
        before_rect: IRect,
        after: &[u8],
        w: usize,
        h: usize,
        region: IRect,
    ) -> Option<IRect> {
        let canvas_w = i32::try_from(w).ok()?;
        let canvas_h = i32::try_from(h).ok()?;
        let canvas_len = w.checked_mul(h)?.checked_mul(4)?;
        if after.len() != canvas_len {
            return None;
        }
        let before_rect = before_rect.clamp(canvas_w, canvas_h);
        let before_len = (before_rect.w() as usize)
            .checked_mul(before_rect.h() as usize)?
            .checked_mul(4)?;
        if before.len() != before_len {
            return None;
        }
        let region = region.clamp(canvas_w, canvas_h).intersect(before_rect);
        if region.is_empty() {
            return None;
        }
        let before_w = before_rect.w() as usize;
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        for y in region.y0..region.y1 {
            let before_row = (y - before_rect.y0) as usize * before_w * 4;
            let after_row = y as usize * w * 4;
            for x in region.x0..region.x1 {
                let before_i = before_row + (x - before_rect.x0) as usize * 4;
                let after_i = after_row + x as usize * 4;
                if before[before_i..before_i + 4] != after[after_i..after_i + 4] {
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
        if min_x > max_x {
            return None;
        }
        let rect = IRect {
            x0: min_x,
            y0: min_y,
            x1: max_x + 1,
            y1: max_y + 1,
        };
        let (rw, rh) = (rect.w() as usize, rect.h() as usize);
        let mut b = Vec::with_capacity(rw * rh * 4);
        let mut a = Vec::with_capacity(rw * rh * 4);
        for y in rect.y0..rect.y1 {
            let before_start = ((y - before_rect.y0) as usize * before_w
                + (rect.x0 - before_rect.x0) as usize)
                * 4;
            let after_start = (y as usize * w + rect.x0 as usize) * 4;
            b.extend_from_slice(&before[before_start..before_start + rw * 4]);
            a.extend_from_slice(&after[after_start..after_start + rw * 4]);
        }
        self.undo.push(Diff {
            rect,
            before: b,
            after: a,
        });
        self.redo.clear();
        self.trim();
        Some(rect)
    }

    fn apply(px: &mut [u8], w: usize, d: &Diff, use_after: bool) {
        let data = if use_after { &d.after } else { &d.before };
        let rw = d.rect.w().max(0) as usize;
        for (row, y) in (d.rect.y0..d.rect.y1).enumerate() {
            let start = ((y as usize) * w + d.rect.x0 as usize) * 4;
            let end = start + rw * 4;
            let src = row * rw * 4;
            if end <= px.len() && src + rw * 4 <= data.len() {
                px[start..end].copy_from_slice(&data[src..src + rw * 4]);
            }
        }
    }

    pub fn undo(&mut self, px: &mut [u8], w: usize) -> Option<IRect> {
        let d = self.undo.pop()?;
        History::apply(px, w, &d, false);
        let rect = d.rect;
        self.redo.push(d);
        self.trim();
        Some(rect)
    }

    pub fn redo(&mut self, px: &mut [u8], w: usize) -> Option<IRect> {
        let d = self.redo.pop()?;
        History::apply(px, w, &d, true);
        let rect = d.rect;
        self.undo.push(d);
        self.trim();
        Some(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_roundtrip() {
        let w = 4;
        let h = 4;
        let before = vec![0u8; w * h * 4];
        let mut after = before.clone();
        after[(w + 2) * 4] = 9;
        let mut hist = History::default();
        let r = hist.record(&before, &after, w, h).unwrap();
        assert_eq!(
            r,
            IRect {
                x0: 2,
                y0: 1,
                x1: 3,
                y1: 2
            }
        );

        let mut buf = after.clone();
        assert!(hist.undo(&mut buf, w).is_some());
        assert_eq!(buf, before);
        assert!(hist.redo(&mut buf, w).is_some());
        assert_eq!(buf, after);
        assert!(hist.undo(&mut buf, w).is_some());
        assert!(!hist.can_undo());
    }

    #[test]
    fn no_change_records_nothing() {
        let before = vec![1u8; 8 * 8 * 4];
        let mut hist = History::default();
        assert!(hist.record(&before, &before, 8, 8).is_none());
        assert!(!hist.can_undo());
    }

    #[test]
    fn region_record_scans_only_the_supplied_area() {
        let w = 4;
        let h = 4;
        let before = vec![0u8; w * h * 4];
        let mut after = before.clone();
        after[0] = 9;
        after[(2 * w + 3) * 4] = 7;
        let mut hist = History::default();
        assert!(hist
            .record_region(&before, &after, w, h, IRect::new(2, 1, 4, 4))
            .is_some());
        let mut buf = after;
        hist.undo(&mut buf, w);
        assert_eq!(buf[(2 * w + 3) * 4], 0);
        assert_eq!(buf[0], 9);
    }

    #[test]
    fn local_record_roundtrips_against_full_canvas() {
        let w = 5;
        let h = 4;
        let base_rect = IRect::new(1, 1, 4, 3);
        let mut before = vec![0u8; base_rect.w() as usize * base_rect.h() as usize * 4];
        before[(base_rect.w() as usize + 1) * 4] = 3;

        let mut after = vec![0u8; w * h * 4];
        for y in base_rect.y0..base_rect.y1 {
            let before_start = ((y - base_rect.y0) as usize * base_rect.w() as usize) * 4;
            let after_start = (y as usize * w + base_rect.x0 as usize) * 4;
            let row_len = base_rect.w() as usize * 4;
            after[after_start..after_start + row_len]
                .copy_from_slice(&before[before_start..before_start + row_len]);
        }
        let changed = ((2 * w) + 2) * 4;
        after[changed] = 9;
        after[4] = 7;

        let mut hist = History::default();
        assert_eq!(
            hist.record_local(&before, base_rect, &after, w, h, base_rect),
            Some(IRect::px(2, 2))
        );

        let mut canvas = after.clone();
        assert!(hist.undo(&mut canvas, w).is_some());
        assert_eq!(canvas[changed], 3);
        assert_eq!(canvas[4], 7);
        assert!(hist.redo(&mut canvas, w).is_some());
        assert_eq!(canvas[changed], 9);
        assert_eq!(canvas[4], 7);
    }

    #[test]
    fn memory_cap_includes_redo_entries() {
        let before = vec![0u8; 4];
        let mut first = before.clone();
        first[0] = 1;
        let mut second = first.clone();
        second[0] = 2;

        let mut hist = History::new(10, 16);
        hist.record(&before, &first, 1, 1);
        hist.record(&first, &second, 1, 1);
        assert!(hist.undo(&mut second.clone(), 1).is_some());

        hist.max_bytes = 8;
        let mut canvas = first;
        assert!(hist.redo(&mut canvas, 1).is_some());
        assert_eq!(hist.undo_len(), 1);
    }
}
