//! State that belongs to an in-progress editing gesture.

use crate::app::Btn;
use crate::ir::IRect;
use crate::paint::{Clip, Floating, Session, TextSession};

/// Editing state that is transient with respect to the current document.
///
/// The clipboard intentionally survives document replacement, matching the old
/// application behavior. `reset_gesture` clears only work that is currently in
/// flight; callers decide separately whether a marquee selection should stay.
#[derive(Default)]
pub(crate) struct EditState {
    pub(crate) session: Option<Session>,
    pub(crate) floating: Option<Floating>,
    pub(crate) clipboard: Option<Clip>,
    pub(crate) text: Option<TextSession>,
    pub(crate) polygon: Vec<(f32, f32)>,
    pub(crate) selection: Option<IRect>,
    pub(crate) select_drag_start: Option<(i32, i32)>,
    pub(crate) move_float_origin: Option<(i32, i32)>,
    pub(crate) dragging: Option<Btn>,
}

impl EditState {
    /// Clear a gesture without discarding the clipboard or marquee selection.
    pub(crate) fn reset_gesture(&mut self) {
        self.session = None;
        self.floating = None;
        self.text = None;
        self.polygon.clear();
        self.select_drag_start = None;
        self.move_float_origin = None;
        self.dragging = None;
    }

    /// Whether an edit still needs to be committed or cancelled.
    pub(crate) fn has_in_progress_edit(&self) -> bool {
        self.session.is_some()
            || self.floating.is_some()
            || self.text.is_some()
            || !self.polygon.is_empty()
    }

    /// Whether there is a marquee or a floating selection to act on.
    pub(crate) fn has_selection(&self) -> bool {
        self.selection.is_some() || self.floating.is_some()
    }
}
