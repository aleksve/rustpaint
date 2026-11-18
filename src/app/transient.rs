//! State that belongs to an in-progress editing gesture.

use eframe::egui::{Pos2, Rect};

use crate::app::Btn;
use crate::ir::IRect;
use crate::paint::{Session, SessionKind};

/// Editing state that is transient with respect to the current document.
#[derive(Default)]
pub(crate) struct EditState {
    pub(crate) session: Option<Session>,
    pub(crate) selection: Option<IRect>,
    pub(crate) select_drag_start: Option<(i32, i32)>,
    pub(crate) move_float_origin: Option<(i32, i32)>,
    pub(crate) text_editor_rect: Option<Rect>,
    pub(crate) dragging: Option<Btn>,
    /// Last `(pointer_pos, shift)` processed during a drag, avoiding redundant
    /// re-rasterisation and texture uploads when the pointer is stationary.
    pub(crate) last_drag_input: Option<(Pos2, bool)>,
    /// Last hover position used to preview an open polygon.
    pub(crate) last_polygon_hover: Option<Pos2>,
}

impl EditState {
    /// Clear any active gesture without discarding a committed marquee selection.
    pub(crate) fn reset_gesture(&mut self) {
        self.session = None;
        self.select_drag_start = None;
        self.move_float_origin = None;
        self.text_editor_rect = None;
        self.dragging = None;
        self.last_drag_input = None;
        self.last_polygon_hover = None;
    }

    pub(crate) fn is_floating(&self) -> bool {
        matches!(
            self.session.as_ref().map(|s| &s.kind),
            Some(SessionKind::Move { .. })
        )
    }

    pub(crate) fn is_editing_text(&self) -> bool {
        matches!(
            self.session.as_ref().map(|s| &s.kind),
            Some(SessionKind::Text { .. })
        )
    }

    pub(crate) fn is_drawing_polygon(&self) -> bool {
        matches!(
            self.session.as_ref().map(|s| &s.kind),
            Some(SessionKind::Polygon { pts, .. }) if !pts.is_empty()
        )
    }

    /// Whether there is a marquee or a floating selection to act on.
    pub(crate) fn has_selection(&self) -> bool {
        self.selection.is_some() || self.is_floating()
    }
}
