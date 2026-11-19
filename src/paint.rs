//! Painting behaviour: the "wet stroke" session, tools, selection handling,
//! and document-level actions (undo, clipboard, image transforms, zoom/pan).

use eframe::egui::{Color32, Pos2, Rect, Vec2};

use crate::app::{App, Btn};
use crate::clipboard::Clip;
use crate::doc::{self, color_to_rgba};
use crate::history::AppliedChange;
use crate::ir::IRect;
use crate::mask::Mask;
use crate::tools::Tool;

pub(crate) struct Floating {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub pos: (i32, i32),
}

impl Floating {
    pub fn rect(&self) -> IRect {
        IRect::from_min_size(self.pos.0, self.pos.1, self.w as i32, self.h as i32)
    }
}

#[derive(Clone)]
pub(crate) struct TextSession {
    pub pos: (f32, f32),
    pub text: String,
    /// Active caret byte index into `text` (always on a UTF-8 boundary).
    pub cursor: usize,
    /// Selection anchor byte index into `text` (`anchor == cursor` => no selection).
    pub anchor: usize,
    /// Remembered x-offset (in image px) for consecutive Up/Down caret moves.
    pub preferred_x: Option<f32>,
    /// Active IME composition string rendered inline at `cursor` until committed.
    pub preedit: String,
    /// Input timestamp when the caret last moved or edited (resets blink phase).
    pub blink_origin: f64,
    pub size: f32,
    pub bold: bool,
    pub family: String,
    undoer: eframe::egui::util::undoer::Undoer<(String, usize, usize)>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn sanitize_text_input(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                let _ = chars.next();
            }
            out.push('\n');
        } else if c == '\n' || c == '\t' || !c.is_control() {
            out.push(c);
        }
    }
    out
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    let idx = s.floor_char_boundary(idx.min(s.len()));
    s[..idx].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    let idx = s.floor_char_boundary(idx.min(s.len()));
    s[idx..]
        .chars()
        .next()
        .map_or(s.len(), |c| idx + c.len_utf8())
}

fn line_start_boundary(s: &str, idx: usize) -> usize {
    let idx = s.floor_char_boundary(idx.min(s.len()));
    s[..idx].rfind('\n').map_or(0, |p| p + 1)
}

fn line_end_boundary(s: &str, idx: usize) -> usize {
    let idx = s.floor_char_boundary(idx.min(s.len()));
    s[idx..].find('\n').map_or(s.len(), |off| idx + off)
}

fn prev_word_boundary(s: &str, idx: usize) -> usize {
    let mut i = s.floor_char_boundary(idx.min(s.len()));
    if i == 0 {
        return 0;
    }
    // Step over a single newline if sitting right at a line start.
    if s[..i].ends_with('\n') {
        return i - 1;
    }
    while let Some((pi, ch)) = s[..i].char_indices().next_back() {
        if ch == '\n' || !ch.is_whitespace() {
            break;
        }
        i = pi;
    }
    let Some((_, last)) = s[..i].char_indices().next_back() else {
        return 0;
    };
    if last == '\n' {
        return i;
    }
    let want_word = is_word_char(last);
    while let Some((pi, ch)) = s[..i].char_indices().next_back() {
        if ch == '\n' || ch.is_whitespace() || is_word_char(ch) != want_word {
            break;
        }
        i = pi;
    }
    i
}

fn next_word_boundary(s: &str, idx: usize) -> usize {
    let mut i = s.floor_char_boundary(idx.min(s.len()));
    if i >= s.len() {
        return s.len();
    }
    if s[i..].starts_with('\n') {
        return i + 1;
    }
    while let Some(ch) = s[i..].chars().next() {
        if ch == '\n' || !ch.is_whitespace() {
            break;
        }
        i += ch.len_utf8();
    }
    let Some(first) = s[i..].chars().next() else {
        return s.len();
    };
    if first == '\n' {
        return i;
    }
    let want_word = is_word_char(first);
    while let Some(ch) = s[i..].chars().next() {
        if ch == '\n' || ch.is_whitespace() || is_word_char(ch) != want_word {
            break;
        }
        i += ch.len_utf8();
    }
    i
}

impl TextSession {
    pub fn new(pos: (f32, f32), size: f32, bold: bool, family: String, now: f64) -> Self {
        let mut undoer = eframe::egui::util::undoer::Undoer::default();
        undoer.add_undo(&(String::new(), 0, 0));
        Self {
            pos,
            text: String::new(),
            cursor: 0,
            anchor: 0,
            preferred_x: None,
            preedit: String::new(),
            blink_origin: now,
            size,
            bold,
            family,
            undoer,
        }
    }

    pub fn feed_undo(&mut self, now: f64) {
        if self.preedit.is_empty() {
            let state = (self.text.clone(), self.cursor, self.anchor);
            self.undoer.feed_state(now, &state);
        }
    }

    pub fn checkpoint_undo(&mut self) {
        if self.preedit.is_empty() {
            let state = (self.text.clone(), self.cursor, self.anchor);
            self.undoer.add_undo(&state);
        }
    }

    pub fn undo_text(&mut self) -> bool {
        let _ = self.commit_preedit();
        let cur = (self.text.clone(), self.cursor, self.anchor);
        if let Some((txt, c, a)) = self.undoer.undo(&cur).cloned() {
            self.text = txt;
            self.cursor = self.text.floor_char_boundary(c.min(self.text.len()));
            self.anchor = self.text.floor_char_boundary(a.min(self.text.len()));
            self.preferred_x = None;
            true
        } else {
            false
        }
    }

    pub fn redo_text(&mut self) -> bool {
        let _ = self.commit_preedit();
        let cur = (self.text.clone(), self.cursor, self.anchor);
        if let Some((txt, c, a)) = self.undoer.redo(&cur).cloned() {
            self.text = txt;
            self.cursor = self.text.floor_char_boundary(c.min(self.text.len()));
            self.anchor = self.text.floor_char_boundary(a.min(self.text.len()));
            self.preferred_x = None;
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.preedit.is_empty()
    }

    pub fn selection_range(&self) -> Option<std::ops::Range<usize>> {
        let a = self
            .text
            .floor_char_boundary(self.anchor.min(self.text.len()));
        let b = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        (a != b).then(|| a.min(b)..a.max(b))
    }

    pub fn selected_text(&self) -> &str {
        self.selection_range()
            .map_or("", |r| &self.text[r.start..r.end])
    }

    /// Return `(rendered_text, display_caret_byte, preedit_byte_range)`.
    pub fn display_text(
        &self,
    ) -> (
        std::borrow::Cow<'_, str>,
        usize,
        Option<std::ops::Range<usize>>,
    ) {
        let cur = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        if self.preedit.is_empty() {
            return (std::borrow::Cow::Borrowed(&self.text), cur, None);
        }
        let mut combined = String::with_capacity(self.text.len() + self.preedit.len());
        combined.push_str(&self.text[..cur]);
        let start = combined.len();
        combined.push_str(&self.preedit);
        let end = combined.len();
        combined.push_str(&self.text[cur..]);
        (std::borrow::Cow::Owned(combined), end, Some(start..end))
    }

    pub fn set_cursor(&mut self, idx: usize, extend: bool, keep_preferred_x: bool) {
        let idx = self.text.floor_char_boundary(idx.min(self.text.len()));
        self.cursor = idx;
        if !extend {
            self.anchor = idx;
        }
        if !keep_preferred_x {
            self.preferred_x = None;
        }
    }

    pub fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection_range() else {
            return false;
        };
        self.text.replace_range(range.clone(), "");
        self.cursor = range.start;
        self.anchor = range.start;
        self.preferred_x = None;
        true
    }

    pub fn insert_str(&mut self, raw: &str) -> bool {
        let clean = sanitize_text_input(raw);
        let had_preedit = !self.preedit.is_empty();
        self.preedit.clear();
        let deleted = self.delete_selection();
        if clean.is_empty() {
            return deleted || had_preedit;
        }
        let at = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        self.text.insert_str(at, &clean);
        let next = at + clean.len();
        self.cursor = next;
        self.anchor = next;
        self.preferred_x = None;
        true
    }

    pub fn set_preedit(&mut self, raw: &str) -> bool {
        let clean = sanitize_text_input(raw);
        if clean.is_empty() && self.preedit.is_empty() {
            return false;
        }
        let deleted = if !clean.is_empty() {
            self.delete_selection()
        } else {
            false
        };
        if self.preedit == clean && !deleted {
            return false;
        }
        self.preedit = clean;
        self.preferred_x = None;
        true
    }

    pub fn commit_preedit(&mut self) -> bool {
        if self.preedit.is_empty() {
            return false;
        }
        let pending = std::mem::take(&mut self.preedit);
        self.insert_str(&pending)
    }

    pub fn delete_surrounding_chars(&mut self, before: usize, after: usize) -> bool {
        if before == 0 && after == 0 {
            return false;
        }
        let _ = self.delete_selection();
        let cur = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        let mut start = cur;
        for _ in 0..before {
            start = prev_char_boundary(&self.text, start);
        }
        let mut end = cur;
        for _ in 0..after {
            end = next_char_boundary(&self.text, end);
        }
        if start == end {
            return false;
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = start;
        self.preferred_x = None;
        true
    }

    pub fn backspace(&mut self, by_word: bool, to_line_start: bool) -> bool {
        if !self.preedit.is_empty() {
            self.preedit.pop();
            return true;
        }
        if self.delete_selection() {
            return true;
        }
        let cur = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        if cur == 0 {
            return false;
        }
        let start = if to_line_start {
            let ls = line_start_boundary(&self.text, cur);
            if ls < cur {
                ls
            } else {
                prev_char_boundary(&self.text, cur)
            }
        } else if by_word {
            prev_word_boundary(&self.text, cur)
        } else {
            prev_char_boundary(&self.text, cur)
        };
        self.text.replace_range(start..cur, "");
        self.cursor = start;
        self.anchor = start;
        self.preferred_x = None;
        true
    }

    pub fn delete_forward(&mut self, by_word: bool, to_line_end: bool) -> bool {
        if !self.preedit.is_empty() {
            self.preedit.clear();
            return true;
        }
        if self.delete_selection() {
            return true;
        }
        let cur = self
            .text
            .floor_char_boundary(self.cursor.min(self.text.len()));
        if cur >= self.text.len() {
            return false;
        }
        let end = if to_line_end {
            let le = line_end_boundary(&self.text, cur);
            if le > cur {
                le
            } else {
                next_char_boundary(&self.text, cur)
            }
        } else if by_word {
            next_word_boundary(&self.text, cur)
        } else {
            next_char_boundary(&self.text, cur)
        };
        self.text.replace_range(cur..end, "");
        self.preferred_x = None;
        true
    }

    pub fn move_left(&mut self, extend: bool, by_word: bool, to_line_start: bool) {
        self.commit_preedit();
        if !extend
            && let Some(range) = self.selection_range()
            && !by_word
            && !to_line_start
        {
            self.set_cursor(range.start, false, false);
            return;
        }
        let target = if to_line_start {
            line_start_boundary(&self.text, self.cursor)
        } else if by_word {
            prev_word_boundary(&self.text, self.cursor)
        } else {
            prev_char_boundary(&self.text, self.cursor)
        };
        self.set_cursor(target, extend, false);
    }

    pub fn move_right(&mut self, extend: bool, by_word: bool, to_line_end: bool) {
        self.commit_preedit();
        if !extend
            && let Some(range) = self.selection_range()
            && !by_word
            && !to_line_end
        {
            self.set_cursor(range.end, false, false);
            return;
        }
        let target = if to_line_end {
            line_end_boundary(&self.text, self.cursor)
        } else if by_word {
            next_word_boundary(&self.text, self.cursor)
        } else {
            next_char_boundary(&self.text, self.cursor)
        };
        self.set_cursor(target, extend, false);
    }

    pub fn select_all(&mut self) {
        self.commit_preedit();
        self.anchor = 0;
        self.cursor = self.text.len();
        self.preferred_x = None;
    }

    pub fn select_word_at(&mut self, idx: usize) {
        self.commit_preedit();
        let idx = self.text.floor_char_boundary(idx.min(self.text.len()));
        let ls = line_start_boundary(&self.text, idx);
        let le = line_end_boundary(&self.text, idx);
        let line = &self.text[ls..le];
        let rel = idx - ls;
        let probe = line[rel..]
            .chars()
            .next()
            .or_else(|| line[..rel].chars().next_back());
        let Some(ch) = probe else {
            self.set_cursor(idx, false, false);
            return;
        };
        let cls = |c: char| -> u8 {
            if is_word_char(c) {
                1
            } else if c.is_whitespace() {
                2
            } else {
                3
            }
        };
        let target_cls = cls(ch);
        let mut start = rel;
        while let Some((pi, pc)) = line[..start].char_indices().next_back() {
            if cls(pc) != target_cls {
                break;
            }
            start = pi;
        }
        let mut end = rel;
        while let Some(nc) = line[end..].chars().next() {
            if cls(nc) != target_cls {
                break;
            }
            end += nc.len_utf8();
        }
        self.anchor = ls + start;
        self.cursor = ls + end;
        self.preferred_x = None;
    }

    pub fn select_line_at(&mut self, idx: usize) {
        self.commit_preedit();
        self.anchor = line_start_boundary(&self.text, idx);
        self.cursor = line_end_boundary(&self.text, idx);
        self.preferred_x = None;
    }
}

/// Mode-specific state owned directly by `Session` so mode payloads (such as a
/// floating selection, polygon vertex list, or live text buffer) can never fall
/// out of sync with the underlying snapshot.
pub(crate) enum SessionKind {
    /// Freehand brush / pencil / eraser / airbrush (incremental coverage).
    Stroke {
        mask: Mask,
        color: Color32,
        erase: bool,
        opacity: f32,
        last: (f32, f32),
        clip: Option<IRect>,
    },
    /// A dragged two-point shape (line, rectangle, ellipse).
    Shape {
        tool: Tool,
        mask: Mask,
        color: Color32,
        opacity: f32,
        start: (f32, f32),
        prev_bbox: Option<IRect>,
        clip: Option<IRect>,
    },
    /// Multi-segment polygon built click-by-click.
    Polygon {
        pts: Vec<(f32, f32)>,
        mask: Mask,
        color: Color32,
        opacity: f32,
        prev_bbox: Option<IRect>,
        clip: Option<IRect>,
    },
    /// Live text being typed or repositioned on the canvas.
    Text {
        data: Box<TextSession>,
        mask: Mask,
        color: Color32,
        opacity: f32,
        prev_bbox: Option<IRect>,
        clip: Option<IRect>,
    },
    /// A lifted selection (`original: Some(_)`) or pasted block (`original: None`).
    Move {
        original: Option<IRect>,
        floating: Floating,
        prev_rect: Option<IRect>,
    },
}

pub(crate) struct Session {
    pub base: Vec<u8>,
    pub base_rect: Option<IRect>,
    pub touched: Option<IRect>,
    pub kind: SessionKind,
}

/// What part of the canvas texture is stale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TexDirty {
    Clean,
    /// Whole image (open, resize, rotate, initial upload).
    Full,
    /// Only this rectangle changed (strokes, fills, local undo/redo).
    Rect(IRect),
}

impl TexDirty {
    pub fn add(&mut self, r: IRect) {
        if r.is_empty() {
            return;
        }
        *self = match *self {
            TexDirty::Full => TexDirty::Full,
            TexDirty::Clean => TexDirty::Rect(r),
            TexDirty::Rect(a) => TexDirty::Rect(a.union(r)),
        };
    }

    pub fn mark_all(&mut self) {
        *self = TexDirty::Full;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ImageOp {
    FlipH,
    FlipV,
    RotateCw,
    RotateCcw,
    Invert,
    Grayscale,
}

impl App {
    pub(crate) fn active_color(&self, btn: Btn) -> Color32 {
        match btn {
            Btn::Primary => self.colors.primary,
            Btn::Secondary => self.colors.secondary,
        }
    }

    pub(crate) fn set_active_color(&mut self, btn: Btn, color: Color32) {
        match btn {
            Btn::Primary => self.colors.primary = color,
            Btn::Secondary => self.colors.secondary = color,
        }
    }

    /// Small deterministic PRNG returning a uniform `f32` in `[0.0, 1.0)`.
    pub(crate) fn rand(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng >> 40) as u32 as f32) * (1.0 / ((1u32 << 24) as f32))
    }

    pub(crate) fn toast(&mut self, ctx: &eframe::egui::Context, msg: impl Into<String>) {
        self.toast_msg = Some((msg.into(), ctx.input(|i| i.time) + 2.6));
    }

    pub(crate) fn sync_modified(&mut self) {
        self.doc.modified = self.hist.is_modified();
    }

    pub(crate) fn push_history(&mut self, before: &[u8]) {
        if let Some(changed) = self
            .hist
            .record(before, &self.doc.px, self.doc.w, self.doc.h)
        {
            self.sync_modified();
            self.tex_dirty.add(changed);
        }
    }

    pub(crate) fn push_history_region(&mut self, before: &[u8], region: IRect) {
        if let Some(changed) =
            self.hist
                .record_region(before, &self.doc.px, self.doc.w, self.doc.h, region)
        {
            self.sync_modified();
            self.tex_dirty.add(changed);
        }
    }

    pub(crate) fn push_history_local(&mut self, before: &[u8], before_rect: IRect, region: IRect) {
        if let Some(changed) = self.hist.record_local(
            before,
            before_rect,
            &self.doc.px,
            self.doc.w,
            self.doc.h,
            region,
        ) {
            self.sync_modified();
            self.tex_dirty.add(changed);
        }
    }

    pub(crate) fn undo(&mut self) {
        self.commit_current();
        if let Some(change) = self.hist.undo_doc(&mut self.doc) {
            self.apply_history_change(change);
        }
    }

    pub(crate) fn redo(&mut self) {
        self.commit_current();
        if let Some(change) = self.hist.redo_doc(&mut self.doc) {
            self.apply_history_change(change);
        }
    }

    fn apply_history_change(&mut self, change: AppliedChange) {
        match change {
            AppliedChange::Region(rect) => {
                self.tex_dirty.add(rect);
            }
            AppliedChange::Resized => {
                self.edit.selection = None;
                self.tex_dirty.mark_all();
                self.view.fit_pending = true;
            }
        }
    }

    // ---------------------------------------------------------------- sessions

    fn begin_session(&mut self, kind: SessionKind) {
        self.edit.session = Some(Session {
            base: Vec::new(),
            base_rect: None,
            touched: None,
            kind,
        });
    }

    /// Drop any in-progress edit state when the current document is replaced.
    pub(crate) fn reset_transient_edit_state(&mut self) {
        self.edit.reset_gesture();
    }

    /// Ensure `s.base` covers `region` without cloning on cache hits and with
    /// geometric growth on misses so long strokes copy in amortised O(area).
    fn ensure_session_base(&mut self, region: IRect) -> Option<IRect> {
        let cw = self.doc.w as i32;
        let ch = self.doc.h as i32;
        let region = region.clamp(cw, ch);
        if region.is_empty() {
            return None;
        }

        let old_rect = {
            let s = self.edit.session.as_mut()?;
            s.touched = Some(s.touched.map_or(region, |t| t.union(region)));
            if let Some(old) = s.base_rect
                && old.contains_rect(region)
            {
                return Some(old);
            }
            s.base_rect
        };

        // Expand geometrically around the union of `old` and `region` so we
        // never forget earlier pixels and reallocate only O(log N) times.
        let target = match old_rect {
            None => region.expand(64).clamp(cw, ch),
            Some(old) => {
                let merged = old.union(region);
                let grow_x = (old.w() / 2).max(64);
                let grow_y = (old.h() / 2).max(64);
                IRect {
                    x0: if region.x0 < old.x0 {
                        merged.x0.saturating_sub(grow_x)
                    } else {
                        merged.x0
                    },
                    y0: if region.y0 < old.y0 {
                        merged.y0.saturating_sub(grow_y)
                    } else {
                        merged.y0
                    },
                    x1: if region.x1 > old.x1 {
                        merged.x1.saturating_add(grow_x)
                    } else {
                        merged.x1
                    },
                    y1: if region.y1 > old.y1 {
                        merged.y1.saturating_add(grow_y)
                    } else {
                        merged.y1
                    },
                }
                .clamp(cw, ch)
            }
        };

        let old_base = self
            .edit
            .session
            .as_mut()
            .map(|s| std::mem::take(&mut s.base))
            .unwrap_or_default();

        let mut base = self.doc.region(target);
        if let Some(old) = old_rect {
            let overlap = old.intersect(target);
            if !overlap.is_empty() {
                let old_w = old.w() as usize;
                let new_w = target.w() as usize;
                let len = overlap.w() as usize * 4;
                for y in overlap.y0..overlap.y1 {
                    let old_start =
                        ((y - old.y0) as usize * old_w + (overlap.x0 - old.x0) as usize) * 4;
                    let new_start =
                        ((y - target.y0) as usize * new_w + (overlap.x0 - target.x0) as usize) * 4;
                    if old_start + len <= old_base.len() && new_start + len <= base.len() {
                        base[new_start..new_start + len]
                            .copy_from_slice(&old_base[old_start..old_start + len]);
                    }
                }
            }
        }
        let s = self.edit.session.as_mut()?;
        s.base = base;
        s.base_rect = Some(target);
        Some(target)
    }

    /// Finish whatever edit is in flight (stroke, shape, polygon, text, move).
    pub(crate) fn commit_current(&mut self) {
        if self
            .text_session_mut()
            .is_some_and(TextSession::commit_preedit)
        {
            self.refresh_text();
        }
        if let Some(s) = self.edit.session.take() {
            if let SessionKind::Move { floating, .. } = &s.kind {
                self.edit.selection = Some(floating.rect());
            }
            if let Some(base_rect) = s.base_rect {
                let scan = s.touched.unwrap_or(base_rect);
                if self
                    .hist
                    .record_local(
                        &s.base,
                        base_rect,
                        &self.doc.px,
                        self.doc.w,
                        self.doc.h,
                        scan,
                    )
                    .is_some()
                {
                    self.sync_modified();
                }
            }
        }
        self.edit.clear_text_ui();
        self.edit.move_float_origin = None;
        self.edit.last_drag_input = None;
        self.edit.last_polygon_hover = None;
    }

    pub(crate) fn cancel_current(&mut self) {
        if let Some(s) = self.edit.session.take() {
            let restore_selection = match &s.kind {
                SessionKind::Move { original, .. } => Some(*original),
                _ => None,
            };
            if let Some(base_rect) = s.base_rect {
                let restore = s.touched.unwrap_or(base_rect);
                doc::restore_region(
                    &mut self.doc.px,
                    self.doc.w,
                    self.doc.h,
                    &s.base,
                    base_rect,
                    restore,
                );
                self.tex_dirty.add(restore);
            }
            if let Some(original) = restore_selection {
                self.edit.selection = original;
            }
        }
        self.edit.clear_text_ui();
        self.edit.move_float_origin = None;
        self.edit.last_drag_input = None;
        self.edit.last_polygon_hover = None;
    }

    /// Composite the affected mask region against `s.base`.
    ///
    /// For `Stroke`, only the newly dirtied capsule/spray rectangle is
    /// recomposited and uploaded. For `Shape`/`Polygon`/`Text`, the union of
    /// the previous and current bounding boxes is updated so moved or shrunk
    /// pixels are cleanly restored from `s.base`.
    fn refresh_masked(&mut self) {
        let (w, h) = (self.doc.w, self.doc.h);
        let (region, color, opacity, erase, new_bbox) = {
            let Some(s) = self.edit.session.as_mut() else {
                return;
            };
            match &mut s.kind {
                SessionKind::Stroke {
                    mask,
                    color,
                    erase,
                    opacity,
                    clip,
                    ..
                } => {
                    let Some(dirty) = mask.take_dirty() else {
                        return;
                    };
                    let region = apply_clip(dirty, *clip);
                    if region.is_empty() {
                        return;
                    }
                    (region, color_to_rgba(*color), *opacity, *erase, None)
                }
                SessionKind::Shape {
                    mask,
                    color,
                    opacity,
                    prev_bbox,
                    clip,
                    ..
                }
                | SessionKind::Polygon {
                    mask,
                    color,
                    opacity,
                    prev_bbox,
                    clip,
                    ..
                }
                | SessionKind::Text {
                    mask,
                    color,
                    opacity,
                    prev_bbox,
                    clip,
                    ..
                } => {
                    let cur = mask.bbox();
                    let raw = match (cur, *prev_bbox) {
                        (Some(a), Some(b)) => a.union(b),
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => return,
                    };
                    let clipped_cur = cur.map(|c| apply_clip(c, *clip)).filter(|r| !r.is_empty());
                    let region = apply_clip(raw, *clip);
                    if region.is_empty() {
                        *prev_bbox = clipped_cur;
                        return;
                    }
                    (
                        region,
                        color_to_rgba(*color),
                        *opacity,
                        false,
                        Some(clipped_cur),
                    )
                }
                SessionKind::Move { .. } => return,
            }
        };

        let Some(base_rect) = self.ensure_session_base(region) else {
            return;
        };
        let Some(s) = self.edit.session.as_mut() else {
            return;
        };
        let mask = match &s.kind {
            SessionKind::Stroke { mask, .. }
            | SessionKind::Shape { mask, .. }
            | SessionKind::Polygon { mask, .. }
            | SessionKind::Text { mask, .. } => mask,
            SessionKind::Move { .. } => return,
        };
        doc::composite_mask(
            &mut self.doc.px,
            w,
            h,
            region,
            doc::Composite {
                base: &s.base,
                base_rect,
                mask,
                color,
                opacity,
                erase,
            },
        );
        if let Some(updated_prev) = new_bbox
            && let SessionKind::Shape { prev_bbox, .. }
            | SessionKind::Polygon { prev_bbox, .. }
            | SessionKind::Text { prev_bbox, .. } = &mut s.kind
        {
            *prev_bbox = updated_prev;
        }
        self.tex_dirty.add(region);
    }

    fn refresh_float(&mut self) {
        let (clear, previous, current) = {
            let Some(s) = self.edit.session.as_ref() else {
                return;
            };
            let SessionKind::Move {
                original,
                floating,
                prev_rect,
            } = &s.kind
            else {
                return;
            };
            (*original, *prev_rect, floating.rect())
        };
        let mut region = current;
        if let Some(r) = clear {
            region = region.union(r);
        }
        if let Some(r) = previous {
            region = region.union(r);
        }
        let Some(base_rect) = self.ensure_session_base(region) else {
            // Floating selection moved completely outside the canvas; still record pos.
            if let Some(Session {
                kind: SessionKind::Move { prev_rect, .. },
                ..
            }) = self.edit.session.as_mut()
            {
                *prev_rect = Some(current);
            }
            return;
        };
        let Some(s) = self.edit.session.as_ref() else {
            return;
        };
        doc::restore_region(
            &mut self.doc.px,
            self.doc.w,
            self.doc.h,
            &s.base,
            base_rect,
            region,
        );
        if let Some(c) = clear {
            // Only need to re-clear the part of `original` that overlapped `region`.
            let c_part = c.intersect(region);
            if !c_part.is_empty() {
                self.doc.clear_rect(c_part);
            }
        }
        if let Some(Session {
            kind:
                SessionKind::Move {
                    floating,
                    prev_rect,
                    ..
                },
            ..
        }) = self.edit.session.as_mut()
        {
            self.doc.blit_over(
                &floating.px,
                floating.w,
                floating.h,
                floating.pos.0,
                floating.pos.1,
            );
            *prev_rect = Some(current);
        }
        self.tex_dirty.add(region);
    }

    // ------------------------------------------------------------------ tools

    pub(crate) fn pointer_press(&mut self, btn: Btn, pos: Pos2, shift: bool, alt: bool) {
        let img = self.screen_to_image(pos);
        let tool = self.tool;

        if alt || tool == Tool::Picker {
            let c = self
                .doc
                .color_at(img.x.floor() as i32, img.y.floor() as i32);
            self.set_active_color(btn, c);
            self.colors.push_recent(c);
            self.picked_color = Some(c);
            if tool == Tool::Picker {
                return;
            }
        }

        if tool == Tool::Pan {
            return;
        }

        // Any new gesture on the canvas closes an open text block first.
        if self.edit.is_editing_text() && tool != Tool::Text {
            self.commit_current();
        }

        // A marquee selection confines painting to itself. Starting a stroke
        // outside it drops the selection instead, so nobody gets stuck.
        let paints = !matches!(tool, Tool::Select | Tool::Pan | Tool::Picker);
        if paints
            && let Some(r) = self.edit.selection
            && !self.edit.is_floating()
        {
            let p = (img.x.floor() as i32, img.y.floor() as i32);
            if !r.contains(p.0, p.1) {
                self.edit.selection = None;
            }
        }

        let color = self.active_color(btn);
        if paints && tool != Tool::Eraser {
            self.colors.push_recent(color);
        }

        match tool {
            Tool::Select => self.press_select(img, shift),
            Tool::Fill => {
                let fx = img.x.floor() as i32;
                let fy = img.y.floor() as i32;
                if let Some(sel) = self.edit.selection {
                    let clip = sel.clamp(self.doc.w as i32, self.doc.h as i32);
                    let before = self.doc.region(clip);
                    if let Some(dirty) = doc::flood_fill(
                        &mut self.doc,
                        fx,
                        fy,
                        color,
                        self.opts.tolerance,
                        Some(clip),
                    ) {
                        self.push_history_local(&before, clip, dirty);
                    }
                } else {
                    let before = self.doc.px.clone();
                    if let Some(dirty) =
                        doc::flood_fill(&mut self.doc, fx, fy, color, self.opts.tolerance, None)
                    {
                        self.push_history_region(&before, dirty);
                    }
                }
            }
            Tool::Text => {
                self.start_text_session(img, 0.0, false);
            }
            Tool::Polygon => {
                let already_open = matches!(
                    self.edit.session.as_ref().map(|s| &s.kind),
                    Some(SessionKind::Polygon { .. })
                );
                if !already_open {
                    let mask = Mask::new(self.doc.w, self.doc.h);
                    let clip = self.edit.selection;
                    let opacity = self.opts.opacity;
                    self.begin_session(SessionKind::Polygon {
                        pts: Vec::new(),
                        mask,
                        color,
                        opacity,
                        prev_bbox: None,
                        clip,
                    });
                }
                if let Some(Session {
                    kind: SessionKind::Polygon { pts, .. },
                    ..
                }) = self.edit.session.as_mut()
                {
                    // Avoid duplicate vertices from the second click of a double-click.
                    let is_dup = pts.last().is_some_and(|&(lx, ly)| {
                        (lx - img.x).abs() < 0.5 && (ly - img.y).abs() < 0.5
                    });
                    if !is_dup {
                        pts.push((img.x, img.y));
                    }
                }
                self.update_polygon_preview(img);
            }
            _ if tool.is_shape() => {
                let mask = Mask::new(self.doc.w, self.doc.h);
                let clip = self.edit.selection;
                let opacity = self.opts.opacity;
                self.begin_session(SessionKind::Shape {
                    tool,
                    mask,
                    color,
                    opacity,
                    start: (img.x, img.y),
                    prev_bbox: None,
                    clip,
                });
                self.update_shape(img, shift);
            }
            _ if tool.is_freehand() => {
                let erase = tool == Tool::Eraser;
                let mask = Mask::new(self.doc.w, self.doc.h);
                let clip = self.edit.selection;
                let opacity = self.opts.opacity;
                self.begin_session(SessionKind::Stroke {
                    mask,
                    color,
                    erase,
                    opacity,
                    last: (img.x, img.y),
                    clip,
                });
                self.stamp(img, 0.0);
            }
            _ => {}
        }
    }

    pub(crate) fn pointer_move(&mut self, pos: Pos2, shift: bool, dt: f32) {
        let img = self.screen_to_image(pos);
        enum Dispatch {
            Stroke,
            Shape,
            Polygon,
            Move,
            None,
        }
        let dispatch = match self.edit.session.as_ref().map(|s| &s.kind) {
            Some(SessionKind::Stroke { .. }) => Dispatch::Stroke,
            Some(SessionKind::Shape { .. }) => Dispatch::Shape,
            Some(SessionKind::Polygon { .. }) => Dispatch::Polygon,
            Some(SessionKind::Move { .. }) => Dispatch::Move,
            Some(SessionKind::Text { .. }) | None => Dispatch::None,
        };
        match dispatch {
            Dispatch::Stroke => self.stamp(img, dt),
            Dispatch::Shape => self.update_shape(img, shift),
            Dispatch::Polygon => self.update_polygon_preview(img),
            Dispatch::Move => self.drag_float(img),
            Dispatch::None => {}
        }
    }

    pub(crate) fn pointer_release(&mut self) {
        if let Some(s) = self.edit.session.as_ref() {
            match s.kind {
                SessionKind::Text { .. } | SessionKind::Polygon { .. } => return,
                _ => {}
            }
        }
        self.commit_current();
    }

    fn stamp(&mut self, img: Pos2, dt: f32) {
        let tool = self.tool;
        let size = self.opts.size.max(0.5);
        let radius = match tool {
            Tool::Pencil => (size - 1.0).max(0.0) * 0.5 + 0.5,
            _ => size * 0.5,
        };
        let (aa, hardness) = match tool {
            Tool::Pencil => (false, 1.0),
            Tool::Brush => (self.opts.antialias, self.opts.hardness),
            Tool::Eraser => (true, self.opts.hardness),
            Tool::Airbrush => (true, self.opts.hardness),
            _ => (true, 1.0),
        };
        let last = match self.edit.session.as_ref().map(|s| &s.kind) {
            Some(SessionKind::Stroke { last, .. }) => *last,
            _ => return,
        };

        // Airbrush: interpolate particles along `last -> img`, scale stationary
        // build-up by `dt` for frame-rate independence, and shape radial
        // density/falloff using `hardness`.
        if tool == Tool::Airbrush {
            let dx = img.x - last.0;
            let dy = img.y - last.1;
            let dist = (dx * dx + dy * dy).sqrt();
            let hold = if dt <= 0.0 {
                1.0
            } else {
                (dt * 60.0).clamp(0.05, 3.0)
            };
            let n = ((dist * 0.9 + size * 0.55 * hold).round() as i32).clamp(1, 600);
            let hard = hardness.clamp(0.0, 1.0);
            let radial_exp = 1.35 - 0.85 * hard; // 0.5 = uniform disc, 1.35 = soft centre
            let mut dots = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let along = if dist > 0.5 { self.rand() } else { 1.0 };
                let cx = last.0 + dx * along;
                let cy = last.1 + dy * along;
                let ang = self.rand() * std::f32::consts::TAU;
                let r_norm = self.rand().powf(radial_exp);
                let rad = radius * r_norm;
                let x = (cx + ang.cos() * rad).floor() as i32;
                let y = (cy + ang.sin() * rad).floor() as i32;
                let falloff = 1.0 - r_norm * (1.0 - hard) * 0.7;
                let c = (0.10 + self.rand() * 0.30) * falloff;
                dots.push((x, y, c));
            }
            if let Some(Session {
                kind:
                    SessionKind::Stroke {
                        mask, last: s_last, ..
                    },
                ..
            }) = self.edit.session.as_mut()
            {
                for (x, y, c) in dots {
                    mask.add(x, y, c);
                }
                *s_last = (img.x, img.y);
            }
        } else if let Some(Session {
            kind: SessionKind::Stroke {
                mask, last: s_last, ..
            },
            ..
        }) = self.edit.session.as_mut()
        {
            // `capsule` already includes both round end-caps and handles `last == img`.
            mask.capsule(last, (img.x, img.y), radius, hardness, aa);
            *s_last = (img.x, img.y);
        }
        self.refresh_masked();
    }

    fn update_shape(&mut self, img: Pos2, shift: bool) {
        let filled = self.opts.filled;
        let width = self.opts.size.max(0.5);
        let aa = self.opts.antialias;
        if let Some(Session {
            kind: SessionKind::Shape {
                tool, mask, start, ..
            },
            ..
        }) = self.edit.session.as_mut()
        {
            let (sx, sy) = *start;
            let (mut ex, mut ey) = (img.x, img.y);
            if shift {
                let d = Vec2::new(ex - sx, ey - sy);
                match *tool {
                    Tool::Line => {
                        let len = d.length();
                        let step = std::f32::consts::FRAC_PI_4;
                        let a = (d.y.atan2(d.x) / step).round() * step;
                        ex = sx + a.cos() * len;
                        ey = sy + a.sin() * len;
                    }
                    _ => {
                        let m = d.x.abs().max(d.y.abs());
                        let sign_x = if d.x < 0.0 { -1.0 } else { 1.0 };
                        let sign_y = if d.y < 0.0 { -1.0 } else { 1.0 };
                        ex = sx + m * sign_x;
                        ey = sy + m * sign_y;
                    }
                }
            }
            mask.clear();
            match *tool {
                Tool::Line => {
                    mask.capsule((sx, sy), (ex, ey), width * 0.5, 1.0, aa);
                }
                Tool::Rect => {
                    mask.rect(
                        sx.min(ex),
                        sy.min(ey),
                        sx.max(ex),
                        sy.max(ey),
                        width,
                        filled,
                    );
                }
                Tool::Ellipse => {
                    let cx = (sx + ex) * 0.5;
                    let cy = (sy + ey) * 0.5;
                    let rx = (ex - sx).abs() * 0.5;
                    let ry = (ey - sy).abs() * 0.5;
                    mask.ellipse(cx, cy, rx, ry, width, filled);
                }
                _ => {}
            }
        } else {
            return;
        }
        self.refresh_masked();
    }

    pub(crate) fn update_polygon_preview(&mut self, cursor: Pos2) {
        let filled = self.opts.filled;
        let width = self.opts.size.max(0.5);
        if let Some(Session {
            kind: SessionKind::Polygon { pts, mask, .. },
            ..
        }) = self.edit.session.as_mut()
        {
            if pts.is_empty() {
                return;
            }
            mask.clear();
            if filled && pts.len() >= 3 {
                mask.polygon_fill(pts);
            }
            mask.polyline(pts, width, false);
            if let Some(&last) = pts.last() {
                mask.capsule(
                    last,
                    (cursor.x, cursor.y),
                    (width * 0.5).max(0.4),
                    1.0,
                    true,
                );
            }
        } else {
            return;
        }
        self.refresh_masked();
    }

    pub(crate) fn finish_polygon(&mut self) {
        let filled = self.opts.filled;
        let width = self.opts.size.max(0.5);
        let enough = match self.edit.session.as_mut() {
            Some(Session {
                kind: SessionKind::Polygon { pts, mask, .. },
                ..
            }) => {
                if pts.len() < 2 {
                    false
                } else {
                    mask.clear();
                    if filled && pts.len() > 2 {
                        mask.polygon_fill(pts);
                    }
                    mask.polyline(pts, width, true);
                    true
                }
            }
            _ => return,
        };
        if !enough {
            self.cancel_current();
            return;
        }
        self.refresh_masked();
        self.commit_current();
    }

    pub(crate) fn polygon_points(&self) -> &[(f32, f32)] {
        match self.edit.session.as_ref().map(|s| &s.kind) {
            Some(SessionKind::Polygon { pts, .. }) => pts.as_slice(),
            _ => &[],
        }
    }

    /// Begin a new focused canvas text session at `img`. When `center_line` is
    /// true, the first line is vertically centred on the I-beam hotspot.
    pub(crate) fn start_text_session(&mut self, img: Pos2, now: f64, center_line: bool) {
        self.commit_current();
        let family = self.opts.font_family.clone();
        let bold = self.opts.bold;
        let size = self.opts.font_size;
        self.text_engine.set_style(&mut self.fonts, &family, bold);
        let y_shift = if center_line {
            self.text_engine.line_height(size) * 0.5
        } else {
            0.0
        };
        let max_x = self.doc.w.saturating_sub(4) as f32;
        let max_y = self.doc.h.saturating_sub(4) as f32;
        let pos = (img.x.clamp(0.0, max_x), (img.y - y_shift).clamp(0.0, max_y));
        let mask = Mask::new(self.doc.w, self.doc.h);
        let data = TextSession::new(pos, size, bold, family, now);
        let color = self.active_color(Btn::Primary);
        let clip = self.edit.selection;
        let opacity = self.opts.opacity;
        self.begin_session(SessionKind::Text {
            data: Box::new(data),
            mask,
            color,
            opacity,
            prev_bbox: None,
            clip,
        });
    }

    /// Move an empty live text session to a fresh I-beam click location while
    /// keeping keyboard focus active.
    pub(crate) fn reposition_empty_text(&mut self, img: Pos2, now: f64) {
        let Some((family, bold, size)) = self
            .text_session()
            .map(|t| (t.family.clone(), t.bold, t.size))
        else {
            return;
        };
        self.text_engine.set_style(&mut self.fonts, &family, bold);
        let y_shift = self.text_engine.line_height(size) * 0.5;
        let max_x = self.doc.w.saturating_sub(4) as f32;
        let max_y = self.doc.h.saturating_sub(4) as f32;
        if let Some(live) = self.text_session_mut() {
            live.pos = (img.x.clamp(0.0, max_x), (img.y - y_shift).clamp(0.0, max_y));
            live.blink_origin = now;
        }
    }

    pub(crate) fn text_session(&self) -> Option<&TextSession> {
        match self.edit.session.as_ref().map(|s| &s.kind) {
            Some(SessionKind::Text { data, .. }) => Some(data),
            _ => None,
        }
    }

    pub(crate) fn text_session_mut(&mut self) -> Option<&mut TextSession> {
        match self.edit.session.as_mut().map(|s| &mut s.kind) {
            Some(SessionKind::Text { data, .. }) => Some(data),
            _ => None,
        }
    }

    pub(crate) fn text_color(&self) -> Option<Color32> {
        match self.edit.session.as_ref().map(|s| &s.kind) {
            Some(SessionKind::Text { color, .. }) => Some(*color),
            _ => None,
        }
    }

    /// Compute the exact glyph/caret layout for the active text session
    /// (including any in-progress IME composition string).
    pub(crate) fn text_layout(&mut self) -> Option<crate::text::TextLayout> {
        let (size, bold, family) = {
            let data = self.text_session()?;
            (data.size, data.bold, data.family.clone())
        };
        self.text_engine.set_style(&mut self.fonts, &family, bold);
        let data = self.text_session()?;
        let (rendered, _, _) = data.display_text();
        Some(self.text_engine.layout(&rendered, size))
    }

    pub(crate) fn sync_live_text_color_and_opacity(&mut self) {
        let active = self.active_color(self.color_target);
        let op = self.opts.opacity;
        let changed = match self.edit.session.as_mut() {
            Some(Session {
                kind: SessionKind::Text { color, opacity, .. },
                ..
            }) => {
                let mut c = false;
                if *color != active {
                    *color = active;
                    c = true;
                }
                if (*opacity - op).abs() > 1e-4 {
                    *opacity = op;
                    c = true;
                }
                c
            }
            _ => false,
        };
        if changed {
            self.refresh_text();
        }
    }

    pub(crate) fn refresh_text(&mut self) {
        let Some(Session {
            kind: SessionKind::Text { data, .. },
            ..
        }) = self.edit.session.as_ref()
        else {
            return;
        };
        let (x, y, size, bold, family) = (
            data.pos.0,
            data.pos.1,
            data.size,
            data.bold,
            data.family.clone(),
        );
        self.text_engine.set_style(&mut self.fonts, &family, bold);
        if let Some(Session {
            kind: SessionKind::Text { data, mask, .. },
            ..
        }) = self.edit.session.as_mut()
        {
            mask.clear();
            let (rendered, _, _) = data.display_text();
            if !rendered.is_empty() {
                self.text_engine.draw(mask, &rendered, x, y, size);
            }
        } else {
            return;
        }
        self.refresh_masked();
    }

    /// Move the live text by one frame's screen-space drag delta, keeping its
    /// anchor strictly inside the canvas so it can always be reached again.
    pub(crate) fn move_text_by(&mut self, delta: Vec2) {
        let scale = self.view.zoom.max(0.0001);
        let max_x = self.doc.w.saturating_sub(4) as f32;
        let max_y = self.doc.h.saturating_sub(4) as f32;
        let Some(text) = self.text_session_mut() else {
            return;
        };
        let nx = (text.pos.0 + delta.x / scale).clamp(0.0, max_x);
        let ny = (text.pos.1 + delta.y / scale).clamp(0.0, max_y);
        if (nx - text.pos.0).abs() < 1e-4 && (ny - text.pos.1).abs() < 1e-4 {
            return;
        }
        text.pos = (nx, ny);
        self.refresh_text();
    }

    // -------------------------------------------------------------- selection

    fn press_select(&mut self, img: Pos2, shift: bool) {
        let p = (img.x.floor() as i32, img.y.floor() as i32);

        // If a pasted or nudged selection is already floating and the user
        // clicks inside it, continue moving the *same* session so its original
        // backdrop snapshot and `original` hole semantics remain intact.
        if let Some(Session {
            kind: SessionKind::Move { floating, .. },
            ..
        }) = self.edit.session.as_ref()
        {
            if floating.rect().contains(p.0, p.1) {
                self.edit.move_float_origin = Some((p.0 - floating.pos.0, p.1 - floating.pos.1));
                return;
            }
            self.commit_current();
        }

        if !shift
            && let Some(sel) = self.edit.selection
            && sel.contains(p.0, p.1)
            && !sel.is_empty()
        {
            let grab = (p.0 - sel.x0, p.1 - sel.y0);
            self.start_move_selection();
            self.edit.move_float_origin = Some(grab);
            return;
        }

        self.commit_current();
        let clamped_x = p.0.clamp(0, self.doc.w.saturating_sub(1) as i32);
        let clamped_y = p.1.clamp(0, self.doc.h.saturating_sub(1) as i32);
        self.edit.selection = Some(IRect::px(clamped_x, clamped_y));
        self.edit.select_drag_start = Some((clamped_x, clamped_y));
    }

    pub(crate) fn drag_select(&mut self, img: Pos2, shift: bool) {
        let Some(start) = self.edit.select_drag_start else {
            return;
        };
        let x = img.x.floor() as i32;
        let y = img.y.floor() as i32;
        let r = if shift {
            let dx = x.saturating_sub(start.0);
            let dy = y.saturating_sub(start.1);
            let span = dx.saturating_abs().max(dy.saturating_abs());
            let ex = if dx < 0 {
                start.0.saturating_sub(span)
            } else {
                start.0.saturating_add(span)
            };
            let ey = if dy < 0 {
                start.1.saturating_sub(span)
            } else {
                start.1.saturating_add(span)
            };
            IRect::new(
                start.0.min(ex),
                start.1.min(ey),
                start.0.max(ex).saturating_add(1),
                start.1.max(ey).saturating_add(1),
            )
        } else {
            IRect::new(
                start.0.min(x),
                start.1.min(y),
                start.0.max(x).saturating_add(1),
                start.1.max(y).saturating_add(1),
            )
        };
        self.edit.selection = Some(r.clamp(self.doc.w as i32, self.doc.h as i32));
    }

    pub(crate) fn start_move_selection(&mut self) {
        if self.edit.is_floating() {
            return;
        }
        let Some(sel) = self.edit.selection else {
            return;
        };
        let sel = sel.clamp(self.doc.w as i32, self.doc.h as i32);
        if sel.is_empty() {
            return;
        }
        let px = self.doc.region(sel);
        let floating = Floating {
            px,
            w: sel.w() as usize,
            h: sel.h() as usize,
            pos: (sel.x0, sel.y0),
        };
        self.begin_session(SessionKind::Move {
            original: Some(sel),
            floating,
            prev_rect: None,
        });
        self.refresh_float();
    }

    fn drag_float(&mut self, img: Pos2) {
        let Some(origin) = self.edit.move_float_origin else {
            return;
        };
        let new_rect = match self.edit.session.as_mut() {
            Some(Session {
                kind: SessionKind::Move { floating, .. },
                ..
            }) => {
                let nx = (img.x.floor() as i32).saturating_sub(origin.0);
                let ny = (img.y.floor() as i32).saturating_sub(origin.1);
                if floating.pos == (nx, ny) {
                    return;
                }
                floating.pos = (nx, ny);
                floating.rect()
            }
            _ => return,
        };
        self.edit.selection = Some(new_rect);
        self.refresh_float();
    }

    pub(crate) fn select_all(&mut self) {
        self.commit_current();
        self.set_tool(Tool::Select);
        self.edit.selection = Some(IRect::new(0, 0, self.doc.w as i32, self.doc.h as i32));
    }

    pub(crate) fn deselect(&mut self) {
        self.commit_current();
        self.edit.selection = None;
        self.edit.move_float_origin = None;
        self.edit.select_drag_start = None;
    }

    /// Delete the current selection without damaging pixels under a floating paste
    /// and without cloning the entire canvas for a small marquee.
    pub(crate) fn delete_selection(&mut self) {
        if let Some(Session {
            kind: SessionKind::Move { original, .. },
            ..
        }) = self.edit.session.as_ref()
        {
            let orig = *original;
            // Restore the clean backdrop where the floating layer hovered.
            self.cancel_current();
            if let Some(r) = orig {
                let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
                if !r.is_empty() {
                    let before = self.doc.region(r);
                    self.doc.clear_rect(r);
                    self.push_history_local(&before, r, r);
                }
            }
            self.edit.selection = None;
            return;
        }

        self.commit_current();
        if let Some(r) = self.edit.selection {
            let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
            if !r.is_empty() {
                let before = self.doc.region(r);
                self.doc.clear_rect(r);
                self.push_history_local(&before, r, r);
            }
        }
    }

    /// Copy the active floating layer (preserving its uncomposited alpha) or
    /// the current marquee region into both the local and system clipboards.
    pub(crate) fn copy_selection(&mut self) {
        if let Some(Session {
            kind: SessionKind::Move { floating, .. },
            ..
        }) = self.edit.session.as_ref()
        {
            let clip = Clip {
                px: floating.px.clone(),
                w: floating.w,
                h: floating.h,
            };
            self.clipboard.set(clip);
            return;
        }

        self.commit_current();
        if let Some(r) = self.edit.selection {
            let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
            if !r.is_empty() {
                let clip = Clip {
                    px: self.doc.region(r),
                    w: r.w() as usize,
                    h: r.h() as usize,
                };
                self.clipboard.set(clip);
            }
        }
    }

    pub(crate) fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    pub(crate) fn paste(&mut self) -> Result<bool, doc::DimensionError> {
        self.commit_current();
        let Some(clip) = self.clipboard.get()? else {
            return Ok(false);
        };
        let (w, h) = (clip.w, clip.h);
        let pos = (
            ((self.doc.w as i32 - w as i32) / 2).max(0),
            ((self.doc.h as i32 - h as i32) / 2).max(0),
        );
        self.set_tool(Tool::Select);
        let floating = Floating {
            px: clip.px,
            w,
            h,
            pos,
        };
        let rect = floating.rect();
        self.begin_session(SessionKind::Move {
            original: None,
            floating,
            prev_rect: None,
        });
        self.edit.selection = Some(rect);
        self.edit.move_float_origin = Some((0, 0));
        self.refresh_float();
        Ok(true)
    }

    pub(crate) fn nudge_selection(&mut self, dx: i32, dy: i32) {
        if !self.edit.is_floating() {
            if self.edit.selection.is_none() {
                return;
            }
            self.start_move_selection();
            self.edit.move_float_origin = Some((0, 0));
        }
        if let Some(Session {
            kind: SessionKind::Move { floating, .. },
            ..
        }) = self.edit.session.as_mut()
        {
            floating.pos = (
                floating.pos.0.saturating_add(dx),
                floating.pos.1.saturating_add(dy),
            );
            self.edit.selection = Some(floating.rect());
        } else {
            return;
        }
        self.refresh_float();
    }

    // -------------------------------------------------------------- document

    pub(crate) fn new_doc(
        &mut self,
        w: usize,
        h: usize,
        transparent: bool,
    ) -> Result<(), doc::DimensionError> {
        let next = doc::Doc::new(w, h, (!transparent).then_some(Color32::WHITE))?;
        self.reset_transient_edit_state();
        self.edit.selection = None;
        self.doc = next;
        self.hist.reset_clean();
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
        Ok(())
    }

    pub(crate) fn resize_canvas(
        &mut self,
        w: usize,
        h: usize,
        scale_content: bool,
    ) -> Result<(), doc::DimensionError> {
        doc::validate_dimensions(w, h)?;
        self.commit_current();
        let (bw, bh) = (self.doc.w, self.doc.h);
        let before = self.doc.px.clone();
        self.doc.resize_canvas(w, h, scale_content, None)?;
        self.reset_transient_edit_state();
        self.hist
            .record_resize(bw, bh, before, self.doc.w, self.doc.h, &self.doc.px);
        self.sync_modified();
        self.edit.selection = None;
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
        Ok(())
    }

    pub(crate) fn crop_to_selection(&mut self) {
        self.commit_current();
        let Some(r) = self.edit.selection else { return };
        let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
        if r.is_empty() || (r.w() as usize == self.doc.w && r.h() as usize == self.doc.h) {
            return;
        }
        let (bw, bh) = (self.doc.w, self.doc.h);
        let before = self.doc.px.clone();
        self.doc.crop(r);
        self.reset_transient_edit_state();
        self.hist
            .record_resize(bw, bh, before, self.doc.w, self.doc.h, &self.doc.px);
        self.sync_modified();
        self.edit.selection = None;
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
    }

    pub(crate) fn clear_canvas(&mut self) {
        self.commit_current();
        let before = self.doc.px.clone();
        let c = self.colors.secondary;
        self.doc.fill_all(c);
        self.push_history(&before);
    }

    pub(crate) fn image_op(&mut self, op: ImageOp) {
        self.commit_current();
        let (bw, bh) = (self.doc.w, self.doc.h);
        let before = self.doc.px.clone();
        match op {
            ImageOp::FlipH => {
                self.doc.flip_h();
                if let Some(sel) = self.edit.selection {
                    let s = sel.clamp(bw as i32, bh as i32);
                    self.edit.selection = (!s.is_empty())
                        .then(|| IRect::new(bw as i32 - s.x1, s.y0, bw as i32 - s.x0, s.y1));
                }
                self.push_history(&before);
            }
            ImageOp::FlipV => {
                self.doc.flip_v();
                if let Some(sel) = self.edit.selection {
                    let s = sel.clamp(bw as i32, bh as i32);
                    self.edit.selection = (!s.is_empty())
                        .then(|| IRect::new(s.x0, bh as i32 - s.y1, s.x1, bh as i32 - s.y0));
                }
                self.push_history(&before);
            }
            ImageOp::Invert => {
                self.doc.invert();
                self.push_history(&before);
            }
            ImageOp::Grayscale => {
                self.doc.grayscale();
                self.push_history(&before);
            }
            ImageOp::RotateCw => {
                self.doc.rotate90(true);
                self.edit.selection = None;
                if self
                    .hist
                    .record_resize(bw, bh, before, self.doc.w, self.doc.h, &self.doc.px)
                {
                    self.sync_modified();
                }
                self.tex_dirty.mark_all();
                self.view.fit_pending = true;
            }
            ImageOp::RotateCcw => {
                self.doc.rotate90(false);
                self.edit.selection = None;
                if self
                    .hist
                    .record_resize(bw, bh, before, self.doc.w, self.doc.h, &self.doc.px)
                {
                    self.sync_modified();
                }
                self.tex_dirty.mark_all();
                self.view.fit_pending = true;
            }
        }
    }

    // ------------------------------------------------------------------- view

    pub(crate) fn zoom_at(&mut self, new_zoom: f32, anchor: Option<Pos2>, view_rect: Rect) {
        let new_zoom = new_zoom.clamp(0.05, 32.0);
        let old = self.view.zoom.max(0.0001);
        let anchor = anchor.unwrap_or_else(|| view_rect.center());
        let img_pt = (anchor - self.origin) / old;
        let size = Vec2::new(self.doc.w as f32, self.doc.h as f32) * new_zoom;
        let center = view_rect.center();
        let new_origin = anchor.to_vec2() - img_pt * new_zoom;
        self.view.pan = new_origin - (center.to_vec2() - size * 0.5);
        self.view.zoom = new_zoom;
        self.origin = center - size * 0.5 + self.view.pan;
    }

    pub(crate) fn zoom_step(&mut self, steps: f32, view_rect: Rect) {
        let z = self.view.zoom * 1.25f32.powf(steps);
        self.zoom_at(z, None, view_rect);
    }

    pub(crate) fn set_zoom(&mut self, z: f32, view_rect: Rect) {
        self.zoom_at(z, None, view_rect);
    }

    pub(crate) fn fit_view(&mut self, view_rect: Rect) {
        let pad = 48.0;
        let sx = (view_rect.width() - pad) / self.doc.w.max(1) as f32;
        let sy = (view_rect.height() - pad) / self.doc.h.max(1) as f32;
        let z = sx.min(sy).clamp(0.05, 8.0);
        self.view.zoom = z;
        self.view.pan = Vec2::ZERO;
        let size = Vec2::new(self.doc.w as f32, self.doc.h as f32) * z;
        self.origin = view_rect.center() - size * 0.5;
    }

    #[inline]
    pub(crate) fn screen_to_image(&self, p: Pos2) -> Pos2 {
        ((p - self.origin) / self.view.zoom.max(0.0001)).to_pos2()
    }

    #[inline]
    pub(crate) fn image_to_screen(&self, p: Pos2) -> Pos2 {
        self.origin + p.to_vec2() * self.view.zoom
    }
}

#[inline]
fn apply_clip(region: IRect, clip: Option<IRect>) -> IRect {
    match clip {
        Some(c) => region.intersect(c),
        None => region,
    }
}
