//! Diff-based undo / redo with support for canvas dimension changes.
//!
//! Normal painting edits store only the tight bounding box of modified pixels,
//! while dimension-changing operations (crop, resize, 90° rotation) store the
//! before/after frames so users never lose undo history. Total memory is capped
//! in O(1) using a `VecDeque` and a running byte counter, and monotonic state
//! IDs accurately track whether the document matches its last saved state.

use std::collections::VecDeque;

use crate::doc::Doc;
use crate::ir::IRect;

enum Entry {
    Diff {
        rect: IRect,
        before: Vec<u8>,
        after: Vec<u8>,
    },
    Resize {
        before_w: usize,
        before_h: usize,
        before: Vec<u8>,
        after_w: usize,
        after_h: usize,
        after: Vec<u8>,
    },
}

impl Entry {
    fn bytes(&self) -> usize {
        match self {
            Entry::Diff { before, after, .. } | Entry::Resize { before, after, .. } => {
                before.len() + after.len()
            }
        }
    }
}

struct Step {
    before_state: u64,
    after_state: u64,
    entry: Entry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppliedChange {
    Region(IRect),
    Resized,
}

pub(crate) struct History {
    undo: VecDeque<Step>,
    redo: VecDeque<Step>,
    total_bytes: usize,
    limit: usize,
    max_bytes: usize,
    next_state: u64,
    current_state: u64,
    saved_state: Option<u64>,
}

impl Default for History {
    fn default() -> Self {
        Self::new(80, 192 * 1024 * 1024)
    }
}

impl History {
    pub fn new(limit: usize, max_bytes: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            total_bytes: 0,
            limit,
            max_bytes,
            next_state: 1,
            current_state: 0,
            saved_state: Some(0),
        }
    }

    /// Clear all history and mark the fresh document state as clean (saved).
    pub fn reset_clean(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.total_bytes = 0;
        let s = self.alloc_state();
        self.current_state = s;
        self.saved_state = Some(s);
    }

    /// Mark the current history point as matching the file on disk.
    pub fn mark_saved(&mut self) {
        self.saved_state = Some(self.current_state);
    }

    /// Whether the document differs from the last clean/saved state.
    pub fn is_modified(&self) -> bool {
        self.saved_state != Some(self.current_state)
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

    fn alloc_state(&mut self) -> u64 {
        let s = self.next_state;
        self.next_state = self.next_state.wrapping_add(1);
        s
    }

    fn clear_redo(&mut self) {
        for step in self.redo.drain(..) {
            self.total_bytes = self.total_bytes.saturating_sub(step.entry.bytes());
        }
    }

    fn trim(&mut self) {
        while self.undo.len() > self.limit {
            if let Some(removed) = self.undo.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.entry.bytes());
            }
        }
        while self.total_bytes > self.max_bytes && self.undo.len() > 1 {
            if let Some(removed) = self.undo.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.entry.bytes());
            }
        }
        // In `redo`, the front holds the oldest redo step (furthest from current)
        // and the back holds the immediate next redo step.
        while self.total_bytes > self.max_bytes
            && !self.redo.is_empty()
            && (!self.undo.is_empty() || self.redo.len() > 1)
        {
            if let Some(removed) = self.redo.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.entry.bytes());
            }
        }
    }

    fn push_entry(&mut self, entry: Entry) {
        self.clear_redo();
        let before_state = self.current_state;
        let after_state = self.alloc_state();
        self.current_state = after_state;
        self.total_bytes = self.total_bytes.saturating_add(entry.bytes());
        self.undo.push_back(Step {
            before_state,
            after_state,
            entry,
        });
        self.trim();
    }

    /// Record a change described by the full "before" and "after" buffers.
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

    /// Record a change when `before` covers only `before_rect` while `after` is
    /// the full canvas buffer.
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

    /// Record a canvas-resizing operation (crop, resize, 90° rotation).
    pub fn record_resize(
        &mut self,
        before_w: usize,
        before_h: usize,
        before: Vec<u8>,
        after_w: usize,
        after_h: usize,
        after: &[u8],
    ) -> bool {
        if before_w == after_w && before_h == after_h && before.as_slice() == after {
            return false;
        }
        if before_w == after_w && before_h == after_h {
            return self.record(&before, after, after_w, after_h).is_some();
        }
        self.push_entry(Entry::Resize {
            before_w,
            before_h,
            before,
            after_w,
            after_h,
            after: after.to_vec(),
        });
        true
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
        let span_w = region.w() as usize;
        let span_bytes = span_w * 4;
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for y in region.y0..region.y1 {
            let b_start = ((y - before_rect.y0) as usize * before_w
                + (region.x0 - before_rect.x0) as usize)
                * 4;
            let a_start = (y as usize * w + region.x0 as usize) * 4;
            let b_slice = &before[b_start..b_start + span_bytes];
            let a_slice = &after[a_start..a_start + span_bytes];
            // Vectorised memcmp skips untouched scanlines immediately.
            if b_slice == a_slice {
                continue;
            }
            let b_px = b_slice.as_chunks::<4>().0;
            let a_px = a_slice.as_chunks::<4>().0;
            let Some(first) = b_px.iter().zip(a_px.iter()).position(|(bp, ap)| bp != ap) else {
                continue;
            };
            let last = b_px
                .iter()
                .zip(a_px.iter())
                .rposition(|(bp, ap)| bp != ap)
                .unwrap_or(first);
            min_x = min_x.min(region.x0 + first as i32);
            max_x = max_x.max(region.x0 + last as i32);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
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
        let row_bytes = rw * 4;
        let mut b = Vec::with_capacity(rw * rh * 4);
        let mut a = Vec::with_capacity(rw * rh * 4);
        for y in rect.y0..rect.y1 {
            let before_start = ((y - before_rect.y0) as usize * before_w
                + (rect.x0 - before_rect.x0) as usize)
                * 4;
            let after_start = (y as usize * w + rect.x0 as usize) * 4;
            b.extend_from_slice(&before[before_start..before_start + row_bytes]);
            a.extend_from_slice(&after[after_start..after_start + row_bytes]);
        }
        self.push_entry(Entry::Diff {
            rect,
            before: b,
            after: a,
        });
        Some(rect)
    }

    fn apply_diff(px: &mut [u8], w: usize, rect: IRect, data: &[u8]) {
        let rw = rect.w() as usize;
        let row_bytes = rw * 4;
        for (row, y) in (rect.y0..rect.y1).enumerate() {
            let start = (y as usize * w + rect.x0 as usize) * 4;
            let end = start + row_bytes;
            let src = row * row_bytes;
            if end <= px.len() && src + row_bytes <= data.len() {
                px[start..end].copy_from_slice(&data[src..src + row_bytes]);
            }
        }
    }

    pub fn undo_doc(&mut self, doc: &mut Doc) -> Option<AppliedChange> {
        let step = self.undo.pop_back()?;
        let change = match &step.entry {
            Entry::Diff { rect, before, .. } => {
                Self::apply_diff(&mut doc.px, doc.w, *rect, before);
                AppliedChange::Region(*rect)
            }
            Entry::Resize {
                before_w,
                before_h,
                before,
                ..
            } => {
                doc.w = *before_w;
                doc.h = *before_h;
                doc.px.clone_from(before);
                AppliedChange::Resized
            }
        };
        self.current_state = step.before_state;
        doc.modified = self.is_modified();
        self.redo.push_back(step);
        self.trim();
        Some(change)
    }

    pub fn redo_doc(&mut self, doc: &mut Doc) -> Option<AppliedChange> {
        let step = self.redo.pop_back()?;
        let change = match &step.entry {
            Entry::Diff { rect, after, .. } => {
                Self::apply_diff(&mut doc.px, doc.w, *rect, after);
                AppliedChange::Region(*rect)
            }
            Entry::Resize {
                after_w,
                after_h,
                after,
                ..
            } => {
                doc.w = *after_w;
                doc.h = *after_h;
                doc.px.clone_from(after);
                AppliedChange::Resized
            }
        };
        self.current_state = step.after_state;
        doc.modified = self.is_modified();
        self.undo.push_back(step);
        self.trim();
        Some(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Color32;

    #[test]
    fn undo_roundtrip_and_saved_state_tracking() {
        let mut doc = Doc::new(4, 4, None).unwrap();
        let before = doc.px.clone();
        doc.px[(doc.w + 2) * 4] = 9;
        let after = doc.px.clone();

        let mut hist = History::default();
        assert!(!hist.is_modified());
        let r = hist.record(&before, &after, doc.w, doc.h).unwrap();
        assert_eq!(r, IRect::px(2, 1));
        assert!(hist.is_modified());

        // Undoing back to the initial clean state clears `modified`.
        assert_eq!(hist.undo_doc(&mut doc), Some(AppliedChange::Region(r)));
        assert_eq!(doc.px, before);
        assert!(!doc.modified);

        // Redoing marks it modified again; saving clears it; undoing marks modified.
        assert_eq!(hist.redo_doc(&mut doc), Some(AppliedChange::Region(r)));
        assert_eq!(doc.px, after);
        assert!(doc.modified);
        hist.mark_saved();
        assert!(!hist.is_modified());
        assert!(hist.undo_doc(&mut doc).is_some());
        assert!(doc.modified);
        assert!(!hist.can_undo());
    }

    #[test]
    fn resize_roundtrip_restores_dimensions_and_pixels() {
        let mut doc = Doc::new(4, 3, Some(Color32::WHITE)).unwrap();
        doc.set(1, 1, [200, 10, 20, 255]);
        let before = doc.px.clone();
        let (bw, bh) = (doc.w, doc.h);

        doc.crop(IRect::new(1, 1, 3, 3));
        let mut hist = History::default();
        assert!(hist.record_resize(bw, bh, before.clone(), doc.w, doc.h, &doc.px));

        assert_eq!(hist.undo_doc(&mut doc), Some(AppliedChange::Resized));
        assert_eq!((doc.w, doc.h), (4, 3));
        assert_eq!(doc.px, before);

        assert_eq!(hist.redo_doc(&mut doc), Some(AppliedChange::Resized));
        assert_eq!((doc.w, doc.h), (2, 2));
        assert_eq!(doc.get(0, 0), Some([200, 10, 20, 255]));
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
        let mut doc = Doc::new(4, 4, None).unwrap();
        let before = doc.px.clone();
        doc.px[0] = 9;
        doc.px[(2 * doc.w + 3) * 4] = 7;
        let mut hist = History::default();
        assert!(
            hist.record_region(&before, &doc.px, doc.w, doc.h, IRect::new(2, 1, 4, 4))
                .is_some()
        );
        hist.undo_doc(&mut doc);
        assert_eq!(doc.px[(2 * 4 + 3) * 4], 0);
        assert_eq!(doc.px[0], 9);
    }

    #[test]
    fn local_record_roundtrips_against_full_canvas() {
        let mut doc = Doc::new(5, 4, None).unwrap();
        let base_rect = IRect::new(1, 1, 4, 3);
        let mut before = vec![0u8; base_rect.w() as usize * base_rect.h() as usize * 4];
        before[(base_rect.w() as usize + 1) * 4] = 3;

        for y in base_rect.y0..base_rect.y1 {
            let b_start = (y - base_rect.y0) as usize * base_rect.w() as usize * 4;
            let a_start = (y as usize * doc.w + base_rect.x0 as usize) * 4;
            let row_len = base_rect.w() as usize * 4;
            doc.px[a_start..a_start + row_len].copy_from_slice(&before[b_start..b_start + row_len]);
        }
        let changed = (2 * doc.w + 2) * 4;
        doc.px[changed] = 9;
        doc.px[4] = 7;

        let mut hist = History::default();
        assert_eq!(
            hist.record_local(&before, base_rect, &doc.px, doc.w, doc.h, base_rect),
            Some(IRect::px(2, 2))
        );

        assert!(hist.undo_doc(&mut doc).is_some());
        assert_eq!(doc.px[changed], 3);
        assert_eq!(doc.px[4], 7);
        assert!(hist.redo_doc(&mut doc).is_some());
        assert_eq!(doc.px[changed], 9);
        assert_eq!(doc.px[4], 7);
    }

    #[test]
    fn memory_cap_includes_redo_entries_and_preserves_immediate_redo() {
        let mut doc = Doc::new(1, 1, None).unwrap();
        let s0 = doc.px.clone();
        doc.px[0] = 1;
        let s1 = doc.px.clone();
        doc.px[0] = 2;
        let s2 = doc.px.clone();
        doc.px[0] = 3;

        let mut hist = History::new(10, 24);
        hist.record(&s0, &s1, 1, 1);
        hist.record(&s1, &s2, 1, 1);
        hist.record(&s2, &doc.px, 1, 1);
        // Undo twice so redo holds two entries (s1->s2 is immediate, s2->s3 is oldest).
        assert!(hist.undo_doc(&mut doc).is_some());
        assert!(hist.undo_doc(&mut doc).is_some());
        assert_eq!(doc.px[0], 1);

        hist.max_bytes = 16;
        hist.trim();
        // Immediate redo step (1 -> 2) is kept; oldest redo step (2 -> 3) was dropped.
        assert!(hist.redo_doc(&mut doc).is_some());
        assert_eq!(doc.px[0], 2);
        assert_eq!(hist.total_bytes, 16);
    }
}
