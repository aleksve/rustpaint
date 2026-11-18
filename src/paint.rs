//! Painting behaviour: the "wet stroke" session, tools, selection handling,
//! and document level actions (undo, clipboard, image transforms).

use eframe::egui::{Color32, Pos2, Vec2};

use crate::app::{App, Btn};
use crate::doc::{self, color_to_rgba};
use crate::ir::IRect;
use crate::mask::Mask;
use crate::tools::Tool;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Freehand brush / pencil / eraser / airbrush.
    Stroke,
    /// A shape dragged out between two points.
    Shape(Tool),
    /// Live text being typed.
    Text,
    /// A lifted selection (or pasted block) being dragged around.
    Move(Option<IRect>),
}

pub struct Session {
    pub base: Vec<u8>,
    pub base_rect: Option<IRect>,
    pub mask: Mask,
    pub kind: Kind,
    pub color: Color32,
    pub erase: bool,
    pub opacity: f32,
    pub prev: Option<IRect>,
    pub last: (f32, f32),
    pub start: (f32, f32),
    /// Painting is confined to this rectangle (an active marquee selection).
    pub clip: Option<IRect>,
}

pub struct Floating {
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

/// What part of the canvas texture is stale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TexDirty {
    Clean,
    /// Whole image (open, resize, undo, image ops…).
    Full,
    /// Only this rectangle changed (the common case: a stroke).
    Rect(IRect),
}

impl TexDirty {
    pub fn add(&mut self, r: IRect) {
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

pub struct Clip {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

#[derive(Clone)]
pub struct TextSession {
    pub pos: (f32, f32),
    pub text: String,
    pub size: f32,
    pub bold: bool,
    pub family: String,
    pub just_created: bool,
}

impl App {
    pub fn active_color(&self, btn: Btn) -> Color32 {
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

    /// Small deterministic RNG so we don't need a dependency.
    pub fn rand(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng >> 33) as f32) / (u32::MAX as f32 * 0.5)
    }

    pub fn toast(&mut self, ctx: &eframe::egui::Context, msg: impl Into<String>) {
        self.toast_msg = Some((msg.into(), ctx.input(|i| i.time) + 2.6));
    }

    pub fn push_history(&mut self, before: Vec<u8>) {
        if self
            .hist
            .record(&before, &self.doc.px, self.doc.w, self.doc.h)
            .is_some()
        {
            self.doc.modified = true;
            self.tex_dirty.mark_all();
        }
    }

    pub fn push_history_region(&mut self, before: Vec<u8>, region: IRect) {
        if self
            .hist
            .record_region(&before, &self.doc.px, self.doc.w, self.doc.h, region)
            .is_some()
        {
            self.doc.modified = true;
            self.tex_dirty.add(region);
        }
    }

    pub fn undo(&mut self) {
        self.commit_current();
        self.edit.selection = self
            .edit
            .selection
            .map(|r| r.clamp(self.doc.w as i32, self.doc.h as i32));
        if self.hist.undo(&mut self.doc.px, self.doc.w).is_some() {
            self.doc.modified = true;
            self.tex_dirty.mark_all();
        }
    }

    pub fn redo(&mut self) {
        self.commit_current();
        if self.hist.redo(&mut self.doc.px, self.doc.w).is_some() {
            self.doc.modified = true;
            self.tex_dirty.mark_all();
        }
    }

    // ---------------------------------------------------------------- sessions

    fn begin_session(&mut self, kind: Kind, color: Color32, erase: bool, opacity: f32, pos: Pos2) {
        let mask = Mask::new(self.doc.w, self.doc.h);
        self.edit.session = Some(Session {
            base: Vec::new(),
            base_rect: None,
            mask,
            kind,
            color,
            erase,
            opacity,
            prev: None,
            last: (pos.x, pos.y),
            start: (pos.x, pos.y),
            clip: self.edit.selection,
        });
    }

    fn snapshot_rect(&self, region: IRect) -> IRect {
        const PAD: i32 = 16;
        let canvas = IRect::new(0, 0, self.doc.w as i32, self.doc.h as i32);
        IRect::new(
            region.x0.saturating_sub(PAD),
            region.y0.saturating_sub(PAD),
            region.x1.saturating_add(PAD),
            region.y1.saturating_add(PAD),
        )
        .clamp(canvas.x1, canvas.y1)
    }

    /// Drop the in-progress edit state when the current document is replaced.
    pub(crate) fn reset_transient_edit_state(&mut self) {
        self.edit.reset_gesture();
    }

    fn ensure_session_base(&mut self, region: IRect) -> Option<IRect> {
        let region = region.clamp(self.doc.w as i32, self.doc.h as i32);
        if region.is_empty() {
            return None;
        }
        let target = self.snapshot_rect(region);
        let (old_rect, old_base) = self
            .edit
            .session
            .as_ref()
            .map(|s| (s.base_rect, s.base.clone()))
            .unwrap_or((None, Vec::new()));
        if let Some(old) = old_rect {
            if old.x0 <= target.x0
                && old.y0 <= target.y0
                && old.x1 >= target.x1
                && old.y1 >= target.y1
            {
                return Some(old);
            }
        }

        let mut base = self.doc.region(target);
        if let Some(old) = old_rect {
            let overlap = old.intersect(target);
            if !overlap.is_empty() {
                let old_w = old.w() as usize;
                let new_w = target.w() as usize;
                for y in overlap.y0..overlap.y1 {
                    let old_start =
                        ((y - old.y0) as usize * old_w + (overlap.x0 - old.x0) as usize) * 4;
                    let new_start =
                        ((y - target.y0) as usize * new_w + (overlap.x0 - target.x0) as usize) * 4;
                    let len = overlap.w() as usize * 4;
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

    /// Finish whatever edit is in flight (stroke, shape, text, floating block).
    pub fn commit_current(&mut self) {
        if let Some(s) = self.edit.session.take() {
            if let Kind::Move(_) = s.kind {
                if let Some(f) = self.edit.floating.take() {
                    self.edit.selection = Some(f.rect());
                }
            }
            if let Some(base_rect) = s.base_rect {
                if self
                    .hist
                    .record_local(
                        &s.base,
                        base_rect,
                        &self.doc.px,
                        self.doc.w,
                        self.doc.h,
                        base_rect,
                    )
                    .is_some()
                {
                    self.doc.modified = true;
                    self.tex_dirty.add(base_rect);
                }
            }
        }
        self.edit.text = None;
        self.edit.text_editor_rect = None;
        self.edit.polygon.clear();
    }

    pub fn cancel_current(&mut self) {
        if let Some(s) = self.edit.session.take() {
            let move_selection = match s.kind {
                Kind::Move(original) => Some(original),
                _ => None,
            };
            if let Some(base_rect) = s.base_rect {
                doc::restore_region(
                    &mut self.doc.px,
                    self.doc.w,
                    self.doc.h,
                    &s.base,
                    base_rect,
                    base_rect,
                );
                self.tex_dirty.add(base_rect);
            }
            self.edit.floating = None;
            if let Some(original) = move_selection {
                self.edit.selection = original;
            }
        }
        self.edit.text = None;
        self.edit.text_editor_rect = None;
        self.edit.polygon.clear();
    }

    /// Recomposite the canvas from `base` + the session mask.
    fn refresh_masked(&mut self) {
        let (w, h) = (self.doc.w, self.doc.h);
        let (region, color, opacity, erase) = {
            let Some(s) = self.edit.session.as_ref() else {
                return;
            };
            let region = match (s.mask.bbox(), s.prev) {
                (Some(a), Some(b)) => a.union(b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => return,
            };
            let region = match s.clip {
                Some(c) if !c.intersect(region).is_empty() => c.intersect(region),
                Some(_) => return,
                None => region,
            };
            (region, color_to_rgba(s.color), s.opacity, s.erase)
        };
        let Some(base_rect) = self.ensure_session_base(region) else {
            return;
        };
        let Some(s) = self.edit.session.as_ref() else {
            return;
        };
        let (mask, base) = (&s.mask, &s.base);
        doc::restore_region(&mut self.doc.px, w, h, base, base_rect, region);
        doc::composite_mask(
            &mut self.doc.px,
            w,
            h,
            region,
            doc::Composite {
                base,
                base_rect,
                mask,
                color,
                opacity,
                erase,
            },
        );
        let bb = self.edit.session.as_ref().and_then(|s| s.mask.bbox());
        if let Some(s) = self.edit.session.as_mut() {
            s.prev = bb;
        }
        self.tex_dirty.add(region);
    }

    fn refresh_float(&mut self) {
        let Some((clear, previous, current)) = self.edit.session.as_ref().and_then(|s| {
            let clear = match s.kind {
                Kind::Move(c) => c,
                _ => None,
            };
            let current = self.edit.floating.as_ref().map(Floating::rect)?;
            Some((clear, s.prev, current))
        }) else {
            return;
        };
        let mut region = current;
        if let Some(r) = clear {
            region = region.union(r);
        }
        if let Some(r) = previous {
            region = region.union(r);
        }
        let Some(base_rect) = self.ensure_session_base(region) else {
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
            self.doc.clear_rect(c);
        }
        if let Some(f) = self.edit.floating.as_ref() {
            self.doc.blit_over(&f.px, f.w, f.h, f.pos.0, f.pos.1);
        }
        if let Some(s) = self.edit.session.as_mut() {
            s.prev = Some(current);
        }
        self.tex_dirty.add(region);
    }

    // ------------------------------------------------------------------ tools

    pub fn pointer_press(&mut self, btn: Btn, pos: Pos2, shift: bool, alt: bool) {
        let img = self.screen_to_image(pos);
        let color = self.active_color(btn);
        self.colors.push_recent(color);
        let tool = self.tool;

        if alt || tool == Tool::Picker {
            let c = self
                .doc
                .color_at(img.x.floor() as i32, img.y.floor() as i32);
            self.set_active_color(btn, c);
            self.picked_color = Some(c);
            if tool == Tool::Picker {
                return;
            }
        }

        // Any new gesture on the canvas first closes an open text block.
        if self.edit.text.is_some() && tool != Tool::Text {
            self.commit_current();
        }

        // A marquee selection confines painting to itself. Starting a stroke
        // outside it drops the selection instead, so nobody gets stuck.
        if let Some(r) = self.edit.selection {
            let paints = !matches!(tool, Tool::Select | Tool::Pan | Tool::Picker);
            let p = (img.x.floor() as i32, img.y.floor() as i32);
            if paints && self.edit.floating.is_none() && !r.contains(p.0, p.1) {
                self.edit.selection = None;
            }
        }

        match tool {
            Tool::Select => self.press_select(img, shift),
            Tool::Fill => {
                let before = self.doc.px.clone();
                doc::flood_fill(
                    &mut self.doc,
                    img.x.floor() as i32,
                    img.y.floor() as i32,
                    color,
                    self.opts.tolerance,
                    self.edit.selection,
                );
                self.push_history(before);
            }
            Tool::Text => {
                self.commit_current();
                self.edit.text = Some(TextSession {
                    pos: (img.x, img.y),
                    text: String::new(),
                    size: self.opts.font_size,
                    bold: self.opts.bold,
                    family: self.opts.font_family.clone(),
                    just_created: true,
                });
                self.begin_session(Kind::Text, color, false, 1.0, img);
            }
            Tool::Polygon => {
                if self.edit.polygon.is_empty() {
                    self.begin_session(
                        Kind::Shape(Tool::Polygon),
                        color,
                        false,
                        self.opts.opacity,
                        img,
                    );
                }
                self.edit.polygon.push((img.x, img.y));
                self.update_polygon_preview(img);
            }
            _ if tool.is_shape() => {
                self.begin_session(Kind::Shape(tool), color, false, self.opts.opacity, img);
                self.update_shape(img, shift);
            }
            _ if tool.is_freehand() => {
                let erase = tool == Tool::Eraser;
                self.begin_session(Kind::Stroke, color, erase, self.opts.opacity, img);
                self.stamp(img);
            }
            _ => {}
        }
    }

    pub fn pointer_move(&mut self, pos: Pos2, shift: bool) {
        let img = self.screen_to_image(pos);
        let Some(kind) = self.edit.session.as_ref().map(|s| s.kind) else {
            if self.tool == Tool::Polygon && !self.edit.polygon.is_empty() {
                self.update_polygon_preview(img);
            }
            return;
        };
        match kind {
            Kind::Stroke => self.stamp(img),
            Kind::Shape(t) => {
                if t == Tool::Polygon {
                    self.update_polygon_preview(img);
                } else {
                    self.update_shape(img, shift);
                }
            }
            Kind::Move(_) => self.drag_float(img),
            Kind::Text => {}
        }
    }

    pub fn pointer_release(&mut self) {
        if let Some(s) = self.edit.session.as_ref() {
            match s.kind {
                Kind::Text => return, // text stays live until committed
                Kind::Shape(Tool::Polygon) => return,
                _ => {}
            }
        }
        self.commit_current();
    }

    fn stamp(&mut self, img: Pos2) {
        let tool = self.tool;
        let size = self.opts.size.max(0.5);
        let radius = match tool {
            Tool::Pencil => (size - 1.0).max(0.0) * 0.5 + 0.5,
            _ => size * 0.5,
        };
        let (aa, hardness) = match tool {
            Tool::Pencil => (false, 1.0),
            Tool::Brush => (self.opts.antialias, self.opts.hardness),
            Tool::Eraser => (true, self.opts.hardness.max(0.55)),
            Tool::Airbrush => (true, 0.0),
            _ => (true, 1.0),
        };
        let last = self
            .edit
            .session
            .as_ref()
            .map(|s| s.last)
            .unwrap_or((img.x, img.y));

        // Air brush: scatter a cloud of dots along the movement (RNG sampled
        // before we borrow the session mutably).
        let mut dots: Vec<(i32, i32, f32)> = Vec::new();
        if tool == Tool::Airbrush {
            let dist = ((img.x - last.0).powi(2) + (img.y - last.1).powi(2)).sqrt();
            let n = ((dist * 0.9 + size * 0.55) as i32).clamp(3, 500);
            for _ in 0..n {
                let ang = self.rand() * std::f32::consts::TAU;
                let rad = radius * self.rand().sqrt();
                let x = (img.x + ang.cos() * rad).floor() as i32;
                let y = (img.y + ang.sin() * rad).floor() as i32;
                let c = 0.10 + self.rand() * 0.30;
                dots.push((x, y, c));
            }
        }

        let Some(s) = self.edit.session.as_mut() else {
            return;
        };
        if tool == Tool::Airbrush {
            for (x, y, c) in dots {
                s.mask.add(x, y, c);
            }
        } else {
            s.mask
                .capsule((last.0, last.1), (img.x, img.y), radius, hardness, aa);
            // guarantee a dot on a single click
            s.mask.disc(img.x, img.y, radius, hardness, aa);
        }
        s.last = (img.x, img.y);
        self.refresh_masked();
    }

    fn update_shape(&mut self, img: Pos2, shift: bool) {
        let Some(s) = self.edit.session.as_mut() else {
            return;
        };
        let tool = match s.kind {
            Kind::Shape(t) => t,
            _ => return,
        };
        let (sx, sy) = s.start;
        let (mut ex, mut ey) = (img.x, img.y);
        if shift {
            let d = Vec2::new(ex - sx, ey - sy);
            match tool {
                Tool::Line => {
                    let len = d.length();
                    let step = std::f32::consts::FRAC_PI_4;
                    let a = (d.y.atan2(d.x) / step).round() * step;
                    ex = sx + a.cos() * len;
                    ey = sy + a.sin() * len;
                }
                _ => {
                    let m = d.x.abs().max(d.y.abs());
                    ex = sx + m * d.x.signum();
                    ey = sy + m * d.y.signum();
                }
            }
        }
        s.mask.clear();
        let filled = self.opts.filled;
        let width = self.opts.size.max(0.5);
        match tool {
            Tool::Line => {
                let r = width * 0.5;
                s.mask
                    .capsule((sx, sy), (ex, ey), r, 1.0, self.opts.antialias);
            }
            Tool::Rect => {
                s.mask.rect(
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
                s.mask.ellipse(cx, cy, rx, ry, width, filled);
            }
            _ => {}
        }
        self.refresh_masked();
    }

    pub fn update_polygon_preview(&mut self, cursor: Pos2) {
        let Some(s) = self.edit.session.as_mut() else {
            return;
        };
        if !matches!(s.kind, Kind::Shape(Tool::Polygon)) {
            return;
        }
        let mut pts = self.edit.polygon.clone();
        if pts.is_empty() {
            return;
        }
        pts.push((cursor.x, cursor.y));
        s.mask.clear();
        let width = self.opts.size.max(0.5);
        if self.opts.filled && pts.len() > 2 {
            s.mask.polygon_fill(&pts[..pts.len() - 1]);
        }
        s.mask.polyline(&pts, width, false);
        self.refresh_masked();
    }

    pub fn finish_polygon(&mut self) {
        if self.edit.polygon.len() < 2 {
            self.cancel_current();
            return;
        }
        let pts = std::mem::take(&mut self.edit.polygon);
        if let Some(s) = self.edit.session.as_mut() {
            s.mask.clear();
            let width = self.opts.size.max(0.5);
            if self.opts.filled && pts.len() > 2 {
                s.mask.polygon_fill(&pts);
            }
            s.mask.polyline(&pts, width, true);
        }
        self.refresh_masked();
        self.commit_current();
    }

    pub fn refresh_text(&mut self) {
        let Some(t) = self.edit.text.as_ref() else {
            return;
        };
        let (x, y, size, bold, family, text) = (
            t.pos.0,
            t.pos.1,
            t.size,
            t.bold,
            t.family.clone(),
            t.text.clone(),
        );
        self.text_engine.set_style(&self.fonts, &family, bold);
        let (Some(s), te) = (self.edit.session.as_mut(), &self.text_engine) else {
            return;
        };
        if !matches!(s.kind, Kind::Text) {
            return;
        }
        s.mask.clear();
        te.draw(&mut s.mask, &text, x, y, size);
        self.refresh_masked();
    }

    /// Return the painted bounds of the live text, with a small grab margin.
    pub(crate) fn text_hit_rect(&self) -> Option<IRect> {
        let session = self
            .edit
            .session
            .as_ref()
            .filter(|s| matches!(s.kind, Kind::Text))?;
        let b = session.mask.bbox()?;
        Some(IRect::new(b.x0 - 6, b.y0 - 6, b.x1 + 6, b.y1 + 6))
    }

    /// Move the live text by one frame's screen-space drag delta, keeping its
    /// anchor on the canvas so it can always be grabbed again.
    pub(crate) fn move_text_by(&mut self, delta: Vec2) {
        let scale = self.scale.max(0.0001);
        let (max_x, max_y) = (self.doc.w as f32, self.doc.h as f32);
        let Some(text) = self.edit.text.as_mut() else {
            return;
        };
        text.pos.0 = (text.pos.0 + delta.x / scale).clamp(0.0, max_x);
        text.pos.1 = (text.pos.1 + delta.y / scale).clamp(0.0, max_y);
        self.refresh_text();
    }

    // -------------------------------------------------------------- selection

    fn press_select(&mut self, img: Pos2, shift: bool) {
        let p = (img.x.floor() as i32, img.y.floor() as i32);
        if let Some(f) = self.edit.floating.as_ref() {
            if f.rect().contains(p.0, p.1) {
                let original = f.rect();
                let base = Vec::new();
                let (w, h) = (self.doc.w, self.doc.h);
                self.edit.session = Some(Session {
                    base,
                    base_rect: None,
                    mask: Mask::new(w, h),
                    kind: Kind::Move(Some(original)),
                    color: self.colors.primary,
                    erase: false,
                    opacity: 1.0,
                    prev: None,
                    last: (img.x, img.y),
                    start: (img.x, img.y),
                    clip: None,
                });
                self.edit.move_float_origin = Some((p.0 - f.pos.0, p.1 - f.pos.1));
                return;
            }
            self.commit_current();
        }
        if !shift {
            if let Some(sel) = self.edit.selection {
                if sel.contains(p.0, p.1) && !sel.is_empty() {
                    let grab = (p.0 - sel.x0, p.1 - sel.y0);
                    self.start_move_selection();
                    self.edit.move_float_origin = Some(grab);
                    return;
                }
            }
        }
        self.commit_current();
        self.edit.selection = Some(IRect::px(p.0, p.1));
        self.edit.select_drag_start = Some(p);
    }

    pub fn drag_select(&mut self, img: Pos2, shift: bool) {
        let Some(start) = self.edit.select_drag_start else {
            return;
        };
        let (x, y) = (img.x.floor() as i32, img.y.floor() as i32);
        let mut r = IRect::new(
            start.0.min(x),
            start.1.min(y),
            start.0.max(x) + 1,
            start.1.max(y) + 1,
        );
        if shift {
            let m = r.w().max(r.h());
            r.x1 = r.x0 + m;
            r.y1 = r.y0 + m;
        }
        self.edit.selection = Some(r.clamp(self.doc.w as i32, self.doc.h as i32));
    }

    pub fn start_move_selection(&mut self) {
        let Some(sel) = self.edit.selection else {
            return;
        };
        let sel = sel.clamp(self.doc.w as i32, self.doc.h as i32);
        if sel.is_empty() {
            return;
        }
        let base = Vec::new();
        let px = self.doc.region(sel);
        let (w, h) = (self.doc.w, self.doc.h);
        self.edit.session = Some(Session {
            base,
            base_rect: None,
            mask: Mask::new(w, h),
            kind: Kind::Move(Some(sel)),
            color: self.colors.primary,
            erase: false,
            opacity: 1.0,
            prev: None,
            last: (0.0, 0.0),
            start: (0.0, 0.0),
            clip: None,
        });
        self.edit.floating = Some(Floating {
            px,
            w: sel.w() as usize,
            h: sel.h() as usize,
            pos: (sel.x0, sel.y0),
        });
        self.refresh_float();
    }

    fn drag_float(&mut self, img: Pos2) {
        let Some(origin) = self.edit.move_float_origin else {
            return;
        };
        let Some(f) = self.edit.floating.as_mut() else {
            return;
        };
        let nx = img.x.floor() as i32 - origin.0;
        let ny = img.y.floor() as i32 - origin.1;
        f.pos = (nx, ny);
        let r = f.rect();
        self.edit.selection = Some(r);
        self.refresh_float();
    }

    pub fn select_all(&mut self) {
        self.commit_current();
        self.tool = Tool::Select;
        self.edit.selection = Some(IRect::new(0, 0, self.doc.w as i32, self.doc.h as i32));
    }

    pub fn deselect(&mut self) {
        self.commit_current();
        self.edit.selection = None;
        self.edit.move_float_origin = None;
        self.edit.select_drag_start = None;
    }

    pub fn delete_selection(&mut self) {
        self.commit_current();
        if let Some(r) = self.edit.selection {
            let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
            if !r.is_empty() {
                let before = self.doc.px.clone();
                self.doc.clear_rect(r);
                self.push_history_region(before, r);
            }
        }
    }

    pub fn copy_selection(&mut self) {
        self.commit_current();
        if let Some(f) = self.edit.floating.as_ref() {
            self.edit.clipboard = Some(Clip {
                px: f.px.clone(),
                w: f.w,
                h: f.h,
            });
        } else if let Some(r) = self.edit.selection {
            let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
            if !r.is_empty() {
                self.edit.clipboard = Some(Clip {
                    px: self.doc.region(r),
                    w: r.w() as usize,
                    h: r.h() as usize,
                });
            }
        }
        if let Some(c) = self.edit.clipboard.as_ref() {
            // egui only reports Ctrl+V when the *text* clipboard is non-empty,
            // so leave a marker there to keep pasting responsive.
            self.pending_clip_text = Some(format!("RustPaint image {}×{}", c.w, c.h));
        }
    }

    pub fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    pub fn paste(&mut self) {
        self.commit_current();
        let Some(c) = self.edit.clipboard.as_ref() else {
            return;
        };
        let px = c.px.clone();
        let (w, h) = (c.w, c.h);
        let pos = (
            ((self.doc.w as i32 - w as i32) / 2).max(0),
            ((self.doc.h as i32 - h as i32) / 2).max(0),
        );
        let base = Vec::new();
        let (dw, dh) = (self.doc.w, self.doc.h);
        self.tool = Tool::Select;
        self.edit.session = Some(Session {
            base,
            base_rect: None,
            mask: Mask::new(dw, dh),
            kind: Kind::Move(None),
            color: self.colors.primary,
            erase: false,
            opacity: 1.0,
            prev: None,
            last: (0.0, 0.0),
            start: (0.0, 0.0),
            clip: None,
        });
        self.edit.floating = Some(Floating { px, w, h, pos });
        self.edit.selection = Some(IRect::from_min_size(pos.0, pos.1, w as i32, h as i32));
        self.edit.move_float_origin = Some((0, 0));
        self.refresh_float();
    }

    pub fn nudge_selection(&mut self, dx: i32, dy: i32) {
        if self.edit.floating.is_none() {
            if self.edit.selection.is_none() {
                return;
            }
            self.start_move_selection();
            self.edit.move_float_origin = Some((0, 0));
        }
        if let Some(f) = self.edit.floating.as_mut() {
            f.pos = (f.pos.0 + dx, f.pos.1 + dy);
            self.edit.selection = Some(f.rect());
        }
        self.refresh_float();
    }

    // -------------------------------------------------------------- document

    pub fn new_doc(
        &mut self,
        w: usize,
        h: usize,
        transparent: bool,
    ) -> Result<(), doc::DimensionError> {
        let next = doc::Doc::new(
            w,
            h,
            if transparent {
                None
            } else {
                Some(Color32::WHITE)
            },
        )?;
        self.reset_transient_edit_state();
        self.edit.selection = None;
        self.doc = next;
        self.hist.clear();
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
        Ok(())
    }

    pub fn resize_canvas(
        &mut self,
        w: usize,
        h: usize,
        scale_content: bool,
    ) -> Result<(), doc::DimensionError> {
        doc::validate_dimensions(w, h)?;
        self.commit_current();
        self.doc.resize_canvas(w, h, scale_content, None)?;
        self.reset_transient_edit_state();
        // Canvas dimensions are not part of the undo stack, so history resets.
        self.hist.clear();
        self.doc.modified = true;
        self.edit.selection = None;
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
        Ok(())
    }

    pub fn crop_to_selection(&mut self) {
        self.commit_current();
        let Some(r) = self.edit.selection else { return };
        let r = r.clamp(self.doc.w as i32, self.doc.h as i32);
        if r.is_empty() {
            return;
        }
        self.doc.crop(r);
        self.reset_transient_edit_state();
        self.hist.clear();
        self.doc.modified = true;
        self.edit.selection = None;
        self.tex_dirty.mark_all();
        self.view.fit_pending = true;
    }

    pub fn clear_canvas(&mut self) {
        self.commit_current();
        let before = self.doc.px.clone();
        let c = self.colors.secondary;
        self.doc.fill_all(c);
        self.push_history(before);
    }

    pub fn image_op(&mut self, op: ImageOp) {
        self.commit_current();
        let before = self.doc.px.clone();
        let (w, h) = (self.doc.w, self.doc.h);
        match op {
            ImageOp::FlipH => self.doc.flip_h(),
            ImageOp::FlipV => self.doc.flip_v(),
            ImageOp::RotateCw => self.doc.rotate90(true),
            ImageOp::RotateCcw => self.doc.rotate90(false),
            ImageOp::Invert => self.doc.invert(),
            ImageOp::Grayscale => self.doc.grayscale(),
        }
        match op {
            ImageOp::Invert | ImageOp::Grayscale => {
                self.hist.record(&before, &self.doc.px, w, h);
                self.doc.modified = true;
            }
            _ => {
                self.hist.clear();
                self.doc.modified = true;
                self.view.fit_pending = true;
            }
        }
        self.tex_dirty.mark_all();
    }

    // ------------------------------------------------------------------- view

    pub fn zoom_at(&mut self, new_zoom: f32, anchor: Option<Pos2>, view_rect: eframe::egui::Rect) {
        let new_zoom = new_zoom.clamp(0.05, 32.0);
        let old = self.scale.max(0.01);
        let anchor = anchor.unwrap_or_else(|| view_rect.center());
        let img_pt = (anchor - self.origin) / old;
        let size = Vec2::new(self.doc.w as f32, self.doc.h as f32) * new_zoom;
        let center = view_rect.center();
        let new_origin = Vec2::new(anchor.x, anchor.y) - img_pt * new_zoom;
        self.view.pan = new_origin - (center.to_vec2() - size * 0.5);
        self.scale = new_zoom;
        self.view.zoom = new_zoom;
    }

    pub fn zoom_step(&mut self, steps: f32, view_rect: eframe::egui::Rect) {
        let z = self.view.zoom * 1.25f32.powf(steps);
        self.zoom_at(z, None, view_rect);
    }

    pub fn set_zoom(&mut self, z: f32, view_rect: eframe::egui::Rect) {
        self.zoom_at(z, None, view_rect);
    }

    pub fn fit_view(&mut self, view_rect: eframe::egui::Rect) {
        let pad = 48.0;
        let sx = (view_rect.width() - pad) / self.doc.w as f32;
        let sy = (view_rect.height() - pad) / self.doc.h as f32;
        let z = sx.min(sy).clamp(0.05, 8.0);
        self.view.zoom = z;
        self.scale = z;
        self.view.pan = Vec2::ZERO;
    }

    pub fn screen_to_image(&self, p: Pos2) -> Pos2 {
        ((p - self.origin) / self.scale.max(0.0001)).to_pos2()
    }

    pub fn image_to_screen(&self, p: Pos2) -> Pos2 {
        self.origin + p.to_vec2() * self.scale
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageOp {
    FlipH,
    FlipV,
    RotateCw,
    RotateCcw,
    Invert,
    Grayscale,
}
