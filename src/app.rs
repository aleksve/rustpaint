//! Application shell: layout, menus, panels, canvas view, dialogs.

mod dialogs;
mod files;
mod overlays;
mod shortcuts;
mod transient;
mod ui;

use std::path::PathBuf;

use eframe::egui::{self, Color32, Context, Pos2, Rect, TextureHandle, Vec2};

use crate::doc::Doc;
use crate::filedlg::FileDialog;
use crate::fonts::FontCatalog;
use crate::history::History;
use crate::prefs::Prefs;
use crate::text::TextEngine;
use crate::theme::{self, Theme};
use crate::tools::{Colors, Tool, ToolOptions};

use self::transient::EditState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Primary,
    Secondary,
}

pub struct View {
    pub zoom: f32,
    pub pan: Vec2,
    pub grid: bool,
    pub fit_pending: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Dialog {
    New,
    Resize,
    About,
    Shortcuts,
    Discard(Pending),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Pending {
    New,
    Open,
    /// Open this file (drag & drop) once the current one is dealt with.
    OpenPath(PathBuf),
    Quit,
}

pub struct App {
    pub doc: Doc,
    pub hist: History,
    pub tool: Tool,
    pub opts: ToolOptions,
    pub colors: Colors,
    pub color_target: Btn,
    pub theme: Theme,
    pub view: View,

    pub edit: EditState,

    pub tex: Option<TextureHandle>,
    pub tex_dirty: crate::paint::TexDirty,
    pub tex_nearest: bool,
    pub origin: Pos2,
    pub scale: f32,
    pub cursor_img: Option<(i32, i32)>,
    pub text_engine: TextEngine,
    pub fonts: FontCatalog,
    /// Native OS file panels (rfd) instead of the built-in browser.
    pub native_dialogs: bool,
    pub font_search: String,
    pub prefs: Prefs,
    /// Per-tool brush sizes, so switching tools keeps each tool's own size.
    pub tool_sizes: std::collections::HashMap<Tool, f32>,

    pub file_dialog: Option<FileDialog>,
    pub dialog: Option<Dialog>,
    pub pending_after_save: Option<Pending>,
    pub toast_msg: Option<(String, f64)>,
    pub rng: u64,
    pub picked_color: Option<Color32>,
    pub last_view_rect: Rect,
    pub title_cache: String,
    pub hex_buf: String,
    pub hex_edit_active: bool,
    pub pending_clip_text: Option<String>,

    pub new_w: u32,
    pub new_h: u32,
    pub new_transparent: bool,
    pub resize_w: u32,
    pub resize_h: u32,
    pub resize_scale: bool,
    pub allow_close: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // Ctrl +/- and Ctrl+wheel zoom the *canvas*, not the whole UI.
        cc.egui_ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
        });
        let prefs = Prefs::load();
        let th = if prefs.theme_dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        theme::apply(&cc.egui_ctx, &th);

        let fonts = FontCatalog::new();
        let mut opts = ToolOptions {
            size: 12.0,
            ..Default::default()
        };
        if let Some(f) = &prefs.font_family {
            if fonts.families().iter().any(|x| x == f) {
                opts.font_family = f.clone();
            }
        }

        Self {
            doc: Doc::new(1280, 800, Some(Color32::WHITE))
                .expect("default canvas dimensions are valid"),
            hist: History::default(),
            tool: Tool::Brush,
            opts,
            colors: Colors::default(),
            color_target: Btn::Primary,
            theme: th,
            view: View {
                zoom: 1.0,
                pan: Vec2::ZERO,
                grid: false,
                fit_pending: true,
            },
            edit: EditState::default(),
            tex: None,
            tex_dirty: crate::paint::TexDirty::Full,
            tex_nearest: true,
            origin: Pos2::ZERO,
            scale: 1.0,
            cursor_img: None,
            text_engine: TextEngine::new(),
            fonts,
            native_dialogs: crate::native::available(),
            font_search: String::new(),
            prefs,
            tool_sizes: default_tool_sizes(),
            file_dialog: None,
            dialog: None,
            pending_after_save: None,
            toast_msg: None,
            rng: 0x2545F4914F6CDD1D,
            picked_color: None,
            last_view_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
            title_cache: String::new(),
            hex_buf: String::new(),
            hex_edit_active: false,
            pending_clip_text: None,
            new_w: 1280,
            new_h: 800,
            new_transparent: false,
            resize_w: 1280,
            resize_h: 800,
            resize_scale: false,
            allow_close: false,
        }
    }

    fn modal_open(&self) -> bool {
        self.dialog.is_some() || self.file_dialog.is_some()
    }

    pub fn set_tool(&mut self, t: Tool) {
        if self.tool == t {
            return;
        }
        self.commit_current();
        // Remember the size this tool was last used with.
        if self.tool.uses_size() {
            self.tool_sizes.insert(self.tool, self.opts.size);
        }
        self.tool = t;
        if let Some(sz) = self.tool_sizes.get(&t) {
            self.opts.size = *sz;
        }
        if t.uses_size() {
            self.opts.size = self.opts.size.max(1.0);
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.prefs.save();
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Cursor output is not reset by egui between frames. Start with the
        // platform default, then let widgets and the canvas override it while
        // they are actually hovered. This also covers modal frames, where
        // canvas_input is intentionally skipped.
        ctx.set_cursor_icon(egui::CursorIcon::Default);

        // close request with unsaved changes
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.doc.modified && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Discard(Pending::Quit));
        }

        // A file dragged onto the window behaves like File ▸ Open.
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            if self.doc.modified {
                self.dialog = Some(Dialog::Discard(Pending::OpenPath(path)));
            } else {
                self.read_file(path, ctx);
            }
        }

        shortcuts::handle(self, ctx);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.tool_rail(ctx);
        self.side_panel(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(self.theme.canvas_bg))
            .show(ctx, |ui| {
                self.canvas_ui(ui);
            });

        self.overlays(ctx);

        if let Some(text) = self.pending_clip_text.take() {
            ctx.copy_text(text);
        }

        // window title
        let title = format!(
            "{}{} · RustPaint",
            self.doc.display_name(),
            if self.doc.modified { " •" } else { "" }
        );
        if title != self.title_cache {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_cache = title;
        }

        if self.edit.has_in_progress_edit() || self.toast_msg.is_some() {
            ctx.request_repaint();
        }
    }
}

/// Sensible starting brush size per tool (remembered per tool afterwards).
fn default_tool_sizes() -> std::collections::HashMap<Tool, f32> {
    use Tool::*;
    let mut m = std::collections::HashMap::new();
    for (t, s) in [
        (Pencil, 1.0),
        (Brush, 12.0),
        (Eraser, 24.0),
        (Airbrush, 32.0),
        (Line, 4.0),
        (Rect, 4.0),
        (Ellipse, 4.0),
        (Polygon, 4.0),
    ] {
        m.insert(t, s);
    }
    m
}

/// Shorten a label for the narrow side panel.
fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IRect;
    use crate::mask::Mask;
    use crate::paint::{Clip, Floating, Kind, Session, TextSession};

    fn test_app() -> App {
        App {
            doc: Doc::new(8, 6, Some(Color32::WHITE)).expect("test dimensions are valid"),
            hist: History::default(),
            tool: Tool::Brush,
            opts: ToolOptions::default(),
            colors: Colors::default(),
            color_target: Btn::Primary,
            theme: Theme::dark(),
            view: View {
                zoom: 1.0,
                pan: Vec2::ZERO,
                grid: false,
                fit_pending: false,
            },
            edit: EditState::default(),
            tex: None,
            tex_dirty: crate::paint::TexDirty::Full,
            tex_nearest: true,
            origin: Pos2::ZERO,
            scale: 1.0,
            cursor_img: None,
            text_engine: TextEngine::new(),
            fonts: FontCatalog::new(),
            native_dialogs: false,
            font_search: String::new(),
            prefs: Prefs::default(),
            tool_sizes: default_tool_sizes(),
            file_dialog: None,
            dialog: None,
            pending_after_save: None,
            toast_msg: None,
            rng: 0x2545F4914F6CDD1D,
            picked_color: None,
            last_view_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0)),
            title_cache: String::new(),
            hex_buf: String::new(),
            hex_edit_active: false,
            pending_clip_text: None,
            new_w: 8,
            new_h: 6,
            new_transparent: false,
            resize_w: 8,
            resize_h: 6,
            resize_scale: false,
            allow_close: false,
        }
    }

    fn paint_rect(app: &mut App, rect: IRect, color: Color32) {
        let rgba = crate::doc::color_to_rgba(color);
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                app.doc.set(x, y, rgba);
            }
        }
    }

    fn assert_region_color(app: &App, rect: IRect, color: Color32) {
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                assert_eq!(app.doc.color_at(x, y), color, "pixel ({x}, {y})");
            }
        }
    }

    #[test]
    fn tool_sizes_follow_the_selected_tool() {
        let mut app = test_app();
        app.opts.size = 19.0;
        app.set_tool(Tool::Pencil);
        app.opts.size = 3.0;

        app.set_tool(Tool::Brush);

        assert_eq!(app.opts.size, 19.0);
        assert_eq!(app.tool_sizes.get(&Tool::Pencil), Some(&3.0));
    }

    #[test]
    fn selection_clipboard_operations_round_trip_through_history() {
        let mut app = test_app();
        let selection = IRect::new(1, 1, 3, 3);
        let red = Color32::from_rgb(220, 30, 40);
        paint_rect(&mut app, selection, red);
        let copied = app.doc.region(selection);
        app.edit.selection = Some(selection);

        app.cut_selection();

        assert_eq!(
            app.edit.clipboard.as_ref().map(|clip| clip.px.clone()),
            Some(copied.clone())
        );
        assert_region_color(&app, selection, Color32::TRANSPARENT);

        app.undo();
        assert_region_color(&app, selection, red);
        app.redo();

        app.paste();
        let pasted = app.edit.selection.expect("paste creates a selection");
        assert!(app.edit.floating.is_some());
        app.commit_current();
        assert_eq!(app.doc.region(pasted), copied);

        app.delete_selection();
        assert_region_color(&app, pasted, Color32::TRANSPARENT);
    }

    #[test]
    fn cancelling_a_floating_move_restores_pixels_and_selection() {
        let mut app = test_app();
        let original_selection = IRect::new(1, 1, 3, 3);
        paint_rect(
            &mut app,
            original_selection,
            Color32::from_rgb(40, 100, 220),
        );
        let before = app.doc.px.clone();
        app.edit.selection = Some(original_selection);

        app.start_move_selection();
        app.nudge_selection(2, 1);
        assert_ne!(app.edit.selection, Some(original_selection));

        app.cancel_current();

        assert_eq!(app.doc.px, before);
        assert_eq!(app.edit.selection, Some(original_selection));
        assert!(app.edit.floating.is_none());
        assert!(app.edit.session.is_none());
    }

    #[test]
    fn document_replacement_clears_gesture_state_but_keeps_clipboard() {
        let mut app = test_app();
        let (w, h) = (app.doc.w, app.doc.h);
        app.edit.session = Some(Session {
            base: Vec::new(),
            base_rect: None,
            mask: Mask::new(w, h),
            kind: Kind::Stroke,
            color: Color32::BLACK,
            erase: false,
            opacity: 1.0,
            prev: None,
            last: (0.0, 0.0),
            start: (0.0, 0.0),
            clip: None,
        });
        app.edit.floating = Some(Floating {
            px: vec![1, 2, 3, 4],
            w: 1,
            h: 1,
            pos: (0, 0),
        });
        app.edit.text = Some(TextSession {
            pos: (1.0, 1.0),
            text: "draft".to_string(),
            size: 12.0,
            bold: false,
            family: crate::fonts::BUILTIN_INTER.to_string(),
            just_created: false,
        });
        app.edit.polygon.push((1.0, 1.0));
        app.edit.selection = Some(IRect::new(1, 1, 2, 2));
        app.edit.select_drag_start = Some((1, 1));
        app.edit.move_float_origin = Some((0, 0));
        app.edit.dragging = Some(Btn::Primary);
        app.edit.clipboard = Some(Clip {
            px: vec![9, 8, 7, 6],
            w: 1,
            h: 1,
        });

        app.new_doc(4, 3, true)
            .expect("replacement dimensions are valid");

        assert_eq!((app.doc.w, app.doc.h), (4, 3));
        assert!(app.edit.session.is_none());
        assert!(app.edit.floating.is_none());
        assert!(app.edit.text.is_none());
        assert!(app.edit.polygon.is_empty());
        assert!(app.edit.selection.is_none());
        assert!(app.edit.select_drag_start.is_none());
        assert!(app.edit.move_float_origin.is_none());
        assert!(app.edit.dragging.is_none());
        assert!(app.edit.clipboard.is_some());
    }

    #[test]
    fn live_text_can_move_before_commit() {
        let mut app = test_app();
        app.doc = Doc::new(200, 100, Some(Color32::WHITE)).expect("test dimensions are valid");
        app.tool = Tool::Text;
        app.pointer_press(Btn::Primary, Pos2::new(10.0, 10.0), false, false);
        app.edit.text.as_mut().expect("text is live").text = "Move me".to_string();
        app.edit.text.as_mut().expect("text is live").just_created = false;
        app.refresh_text();
        let before = app.doc.px.clone();

        app.move_text_by(Vec2::new(2.0, 1.0));
        app.move_text_by(Vec2::new(3.0, 4.0));

        assert_eq!(
            app.edit.text.as_ref().expect("text is still live").pos,
            (15.0, 15.0)
        );
        app.scale = 2.0;
        app.move_text_by(Vec2::new(4.0, 6.0));
        assert_eq!(
            app.edit.text.as_ref().expect("text is still live").pos,
            (17.0, 18.0)
        );
        app.move_text_by(Vec2::new(-10_000.0, 10_000.0));
        assert_eq!(
            app.edit.text.as_ref().expect("text is still live").pos,
            (0.0, 100.0)
        );
        assert_ne!(app.doc.px, before);
        assert!(app.edit.session.is_some());
        app.commit_current();
        assert!(app.edit.text.is_none());
    }

    #[test]
    fn clear_canvas_supports_undo_and_redo() {
        let mut app = test_app();
        app.colors.secondary = Color32::BLACK;

        app.clear_canvas();
        assert_eq!(app.doc.color_at(0, 0), Color32::BLACK);

        app.undo();
        assert_eq!(app.doc.color_at(0, 0), Color32::WHITE);
        app.redo();
        assert_eq!(app.doc.color_at(0, 0), Color32::BLACK);
    }

    #[test]
    fn zoom_at_preserves_the_image_point_under_the_anchor() {
        let mut app = test_app();
        let view = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let anchor = Pos2::new(20.0, 30.0);
        let image_point = app.screen_to_image(anchor);

        app.zoom_at(2.0, Some(anchor), view);

        let canvas_size = Vec2::new(app.doc.w as f32, app.doc.h as f32) * app.view.zoom;
        let origin = view.center() - canvas_size * 0.5 + app.view.pan;
        let remapped = origin + image_point.to_vec2() * app.view.zoom;
        assert!((remapped.x - anchor.x).abs() < 1e-4);
        assert!((remapped.y - anchor.y).abs() < 1e-4);
    }
}
