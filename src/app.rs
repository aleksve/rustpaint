//! Application state, frame lifecycle, and cross-cutting actions.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{self, Color32, Context, Pos2, Rect, TextureHandle, Vec2};

use crate::clipboard::ImageClipboard;
use crate::doc::Doc;
use crate::filedlg::FileDialog;
use crate::fonts::FontCatalog;
use crate::history::History;
use crate::paint::TexDirty;
use crate::prefs::Prefs;
use crate::text::TextEngine;
use crate::theme::{self, Theme};
use crate::tools::{Colors, Tool, ToolOptions, to_hex};

mod dialogs;
mod files;
mod overlays;
mod shortcuts;
mod transient;
mod ui;

pub(crate) use files::write_bytes_atomically;
pub(crate) use transient::EditState;

const HISTORY_LIMIT: usize = 80;
const HISTORY_MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Btn {
    Primary,
    Secondary,
}

#[derive(Clone)]
pub(crate) enum Dialog {
    New,
    Resize,
    About,
    Shortcuts,
    Discard(Pending),
}

#[derive(Clone)]
pub(crate) enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

pub(crate) struct View {
    pub zoom: f32,
    pub pan: Vec2,
    pub grid: bool,
    pub fit_pending: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            grid: true,
            fit_pending: true,
        }
    }
}

pub struct App {
    pub(crate) doc: Doc,
    pub(crate) hist: History,
    pub(crate) tool: Tool,
    pub(crate) opts: ToolOptions,
    pub(crate) colors: Colors,
    pub(crate) color_target: Btn,
    pub(crate) picker_hsv: (Color32, [f32; 3]),
    pub(crate) hex_edit: String,
    pub(crate) hex_focused: bool,
    pub(crate) font_filter: String,
    pub(crate) font_filter_focused: bool,
    pub(crate) tool_sizes: HashMap<Tool, f32>,

    pub(crate) view: View,
    pub(crate) origin: Pos2,
    pub(crate) last_view_rect: Rect,
    pub(crate) tex: Option<TextureHandle>,
    pub(crate) tex_nearest: bool,
    pub(crate) checker_tex: Option<(bool, TextureHandle)>,
    pub(crate) tex_dirty: TexDirty,

    pub(crate) edit: EditState,
    pub(crate) space_pan: bool,
    pub(crate) cursor_img: Option<(i32, i32)>,
    pub(crate) picked_color: Option<Color32>,

    pub(crate) clipboard: ImageClipboard,
    pub(crate) text_engine: TextEngine,
    pub(crate) fonts: FontCatalog,
    pub(crate) rng: u64,

    pub(crate) prefs: Prefs,
    pub(crate) theme: Theme,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) file_dialog: Option<FileDialog>,
    pub(crate) io_job: Option<files::ActiveIoJob>,
    pub(crate) pending_after_save: Option<Pending>,
    pub(crate) native_dialogs: bool,
    pub(crate) new_w: u32,
    pub(crate) new_h: u32,
    pub(crate) new_transparent: bool,
    pub(crate) resize_w: u32,
    pub(crate) resize_h: u32,
    pub(crate) resize_scale: bool,
    pub(crate) toast_msg: Option<(String, f64)>,
    pub(crate) allow_close: bool,
    pub(crate) last_title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let prefs = Prefs::load();
        let theme = if prefs.theme_dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        theme::apply(&cc.egui_ctx, &theme);

        let initial_file = std::env::args_os().nth(1).map(PathBuf::from);
        let native_dialogs = prefs.native_dialogs.unwrap_or(crate::native::available());
        let preferred_font = prefs.font_family.clone();
        let mut app = Self::build(
            1280,
            800,
            prefs,
            theme,
            FontCatalog::new_async(preferred_font),
            ImageClipboard::new(),
            native_dialogs,
        );

        if let Some(path) = initial_file {
            app.start_open_file(path, &cc.egui_ctx);
        }

        app
    }

    fn build(
        w: usize,
        h: usize,
        prefs: Prefs,
        theme: Theme,
        fonts: FontCatalog,
        clipboard: ImageClipboard,
        native_dialogs: bool,
    ) -> Self {
        let colors = Colors::default();
        let hex_edit = to_hex(colors.primary);
        let hsv = crate::widgets::srgb_to_hsv([
            colors.primary.r(),
            colors.primary.g(),
            colors.primary.b(),
        ]);
        let mut opts = ToolOptions::default();
        if let Some(ref fam) = prefs.font_family
            && fonts.families().iter().any(|f| f == fam)
        {
            opts.font_family = fam.clone();
        }
        let mut tool_sizes = HashMap::new();
        for tool in Tool::ALL {
            if tool.uses_size() {
                tool_sizes.insert(tool, tool.default_size());
            }
        }
        Self {
            doc: Doc::new(w, h, Some(Color32::WHITE))
                .expect("default canvas fits within validated limits"),
            hist: History::new(HISTORY_LIMIT, HISTORY_MAX_BYTES),
            tool: Tool::Brush,
            opts,
            picker_hsv: (colors.primary, hsv),
            colors,
            color_target: Btn::Primary,
            hex_edit,
            hex_focused: false,
            font_filter: String::new(),
            font_filter_focused: false,
            tool_sizes,
            view: View::default(),
            origin: Pos2::ZERO,
            last_view_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
            tex: None,
            tex_nearest: true,
            checker_tex: None,
            tex_dirty: TexDirty::Full,
            edit: EditState::default(),
            space_pan: false,
            cursor_img: None,
            picked_color: None,
            clipboard,
            text_engine: TextEngine::new(),
            fonts,
            rng: 0x9E37_79B9_7F4A_7C15,
            prefs,
            theme,
            dialog: None,
            file_dialog: None,
            io_job: None,
            pending_after_save: None,
            native_dialogs,
            new_w: 1280,
            new_h: 800,
            new_transparent: false,
            resize_w: w as u32,
            resize_h: h as u32,
            resize_scale: false,
            toast_msg: None,
            allow_close: false,
            last_title: String::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_headless(w: usize, h: usize) -> Self {
        Self::build(
            w,
            h,
            Prefs::default(),
            Theme::dark(),
            FontCatalog::bundled_only(),
            ImageClipboard::local_only(),
            false,
        )
    }

    pub(crate) fn set_tool(&mut self, tool: Tool) {
        if self.tool == tool {
            return;
        }
        if self.edit.is_drawing_polygon() {
            self.finish_polygon();
        } else {
            self.commit_current();
        }
        if self.tool.uses_size() {
            self.tool_sizes.insert(self.tool, self.opts.size);
        }
        self.tool = tool;
        if tool.uses_size() {
            let max = tool.max_size();
            let remembered = self
                .tool_sizes
                .get(&tool)
                .copied()
                .unwrap_or_else(|| tool.default_size());
            self.opts.size = remembered.clamp(1.0, max);
        }
    }

    pub(crate) fn open_resize_dialog(&mut self) {
        self.resize_w = self.doc.w as u32;
        self.resize_h = self.doc.h as u32;
        self.dialog = Some(Dialog::Resize);
    }

    pub(crate) fn paste_with_toast(&mut self, ctx: &Context) {
        if let Err(err) = self.paste() {
            self.toast(ctx, format!("Paste failed: {err}"));
        }
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.dialog.is_some() || self.file_dialog.is_some() || self.io_job.is_some()
    }

    /// True when the active on-canvas text session owns keyboard & IME input
    /// (i.e. text editing is open, no modal dialog is active, and neither the
    /// sidebar `Hex` field nor the font search field has focus).
    pub(crate) fn canvas_text_captures_keys(&self) -> bool {
        self.edit.is_editing_text()
            && !self.modal_open()
            && !self.hex_focused
            && !self.font_filter_focused
    }

    /// Return keyboard focus from any sidebar field back to the canvas text
    /// session immediately when clicking/placing text on the canvas.
    pub(crate) fn focus_canvas_text(&mut self, ctx: &Context) {
        self.hex_focused = false;
        self.font_filter_focused = false;
        ctx.memory_mut(|mem| {
            if let Some(id) = mem.focused() {
                mem.surrender_focus(id);
            }
        });
    }

    fn update_title(&mut self, ctx: &Context) {
        let dirty = if self.doc.modified { " •" } else { "" };
        let title = format!("{}{} — RustPaint", self.doc.display_name(), dirty);
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn handle_file_drop(&mut self, ctx: &Context) {
        if self.modal_open() {
            return;
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .find(|p| !p.as_os_str().is_empty())
        });
        if let Some(path) = dropped {
            if self.doc.modified {
                self.dialog = Some(Dialog::Discard(Pending::OpenPath(path)));
            } else {
                self.start_open_file(path, ctx);
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if requested && !self.allow_close && (self.doc.modified || self.io_job.is_some()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.io_job.is_none() {
                self.dialog = Some(Dialog::Discard(Pending::Quit));
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(preferred) = self.fonts.poll_ready()
            && self.opts.font_family == crate::fonts::BUILTIN_INTER
        {
            self.opts.font_family = preferred;
        }
        if self.fonts.is_loading() {
            ctx.request_repaint_after_secs(0.15);
        }
        self.poll_io_job(&ctx, Some(frame));
        self.update_title(&ctx);
        self.handle_close_request(&ctx);
        self.handle_file_drop(&ctx);
        shortcuts::handle(self, &ctx, Some(frame));

        let _ = self.top_bar(ui, Some(frame));
        let _ = self.status_bar(ui);
        let _ = self.tool_rail(ui);
        let _ = self.side_panel(ui);
        let _ = self.canvas(ui);
        self.overlays(&ctx, Some(frame));

        if let Some(color) = self.picked_color.take() {
            self.hex_edit = to_hex(color);
        }
    }
}

pub(crate) fn elide(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_owned()
    } else {
        let take = max_chars.saturating_sub(1);
        let mut out: String = s.chars().take(take).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IRect;
    use crate::paint::ImageOp;

    #[test]
    fn selection_clipboard_operations_round_trip_through_history() {
        let mut app = App::new_headless(8, 8);
        app.doc.set(1, 1, [200, 10, 20, 255]);
        app.doc.set(2, 1, [10, 200, 20, 255]);
        app.edit.selection = Some(IRect::new(1, 1, 3, 2));

        app.cut_selection();
        assert_eq!(app.doc.get(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(app.doc.get(2, 1), Some([0, 0, 0, 0]));
        assert!(app.doc.modified);

        assert!(app.paste().unwrap());
        assert!(app.edit.is_floating());
        app.commit_current();

        assert_eq!(app.doc.get(3, 3), Some([200, 10, 20, 255]));
        assert_eq!(app.doc.get(4, 3), Some([10, 200, 20, 255]));

        app.undo();
        assert_eq!(app.doc.get(3, 3), Some([255, 255, 255, 255]));
        assert!(app.doc.modified);

        app.undo();
        assert_eq!(app.doc.get(1, 1), Some([200, 10, 20, 255]));
        assert_eq!(app.doc.get(2, 1), Some([10, 200, 20, 255]));
        assert!(!app.doc.modified);
    }

    #[test]
    fn dragging_pasted_selection_preserves_backdrop_and_undoes_in_one_step() {
        let mut app = App::new_headless(10, 10);
        let backdrop = [42, 99, 177, 255];
        app.doc
            .fill_all(Color32::from_rgba_unmultiplied(42, 99, 177, 255));
        app.hist.reset_clean();

        app.clipboard.set(crate::clipboard::Clip {
            px: [250, 20, 30, 255].repeat(4),
            w: 2,
            h: 2,
        });

        // Paste lands centred at (4, 4).
        assert!(app.paste().unwrap());
        assert_eq!(app.doc.get(4, 4), Some([250, 20, 30, 255]));

        // Click inside the floating paste, drag to (1, 1), and release.
        app.pointer_press(Btn::Primary, Pos2::new(4.5, 4.5), false, false);
        app.pointer_move(Pos2::new(1.5, 1.5), false, 0.016);
        app.pointer_release();

        // Backdrop under initial paste location (4, 4) MUST remain intact.
        assert_eq!(app.doc.get(4, 4), Some(backdrop));
        assert_eq!(app.doc.get(5, 5), Some(backdrop));
        assert_eq!(app.doc.get(1, 1), Some([250, 20, 30, 255]));
        assert_eq!(app.hist.undo_len(), 1);

        // A single Undo restores the clean backdrop.
        app.undo();
        assert_eq!(app.doc.get(1, 1), Some(backdrop));
        assert_eq!(app.doc.get(4, 4), Some(backdrop));
        assert!(!app.doc.modified);
    }

    #[test]
    fn nudge_then_drag_selection_preserves_intermediate_backdrop() {
        let mut app = App::new_headless(12, 12);
        let bg = [80, 120, 160, 255];
        app.doc
            .fill_all(Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]));
        app.doc.set(1, 1, [255, 0, 0, 255]);
        app.hist.reset_clean();

        app.set_tool(Tool::Select);
        app.edit.selection = Some(IRect::new(1, 1, 2, 2));
        app.nudge_selection(1, 0);
        app.nudge_selection(1, 0);
        app.pointer_press(Btn::Primary, Pos2::new(3.2, 1.2), false, false);
        app.pointer_move(Pos2::new(8.2, 8.2), false, 0.016);
        app.pointer_release();

        assert_eq!(app.doc.get(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(app.doc.get(2, 1), Some(bg));
        assert_eq!(app.doc.get(3, 1), Some(bg));
        assert_eq!(app.doc.get(8, 8), Some([255, 0, 0, 255]));
        assert_eq!(app.hist.undo_len(), 1);
    }

    #[test]
    fn copy_and_cut_floating_paste_preserve_alpha_and_backdrop() {
        let mut app = App::new_headless(8, 8);
        let bg = [10, 20, 30, 255];
        app.doc
            .fill_all(Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]));
        app.hist.reset_clean();

        let translucent = [200, 100, 50, 96];
        app.clipboard.set(crate::clipboard::Clip {
            px: translucent.to_vec(),
            w: 1,
            h: 1,
        });
        assert!(app.paste().unwrap());

        app.copy_selection();
        assert!(app.edit.is_floating());
        let copied = app.clipboard.get().unwrap().unwrap();
        assert_eq!(copied.px, translucent);

        app.cut_selection();
        assert!(!app.edit.is_floating());
        assert_eq!(app.doc.get(3, 3), Some(bg));
        assert!(!app.doc.modified);
    }

    #[test]
    fn crop_resize_rotate_and_flip_are_undoable() {
        let mut app = App::new_headless(6, 4);
        app.doc.set(1, 2, [12, 34, 56, 255]);
        let original = app.doc.px.clone();
        app.hist.reset_clean();

        app.image_op(ImageOp::RotateCw);
        assert_eq!((app.doc.w, app.doc.h), (4, 6));
        assert!(app.doc.modified);

        app.edit.selection = Some(IRect::new(0, 0, 2, 2));
        app.crop_to_selection();
        assert_eq!((app.doc.w, app.doc.h), (2, 2));

        app.resize_canvas(5, 5, false).unwrap();
        assert_eq!((app.doc.w, app.doc.h), (5, 5));

        app.image_op(ImageOp::FlipH);

        app.undo();
        app.undo();
        assert_eq!((app.doc.w, app.doc.h), (2, 2));
        app.undo();
        assert_eq!((app.doc.w, app.doc.h), (4, 6));
        app.undo();
        assert_eq!((app.doc.w, app.doc.h), (6, 4));
        assert_eq!(app.doc.px, original);
        assert!(!app.doc.modified);
    }

    #[test]
    fn shift_select_anchors_at_start_in_every_quadrant() {
        let mut app = App::new_headless(20, 20);
        app.set_tool(Tool::Select);
        app.pointer_press(Btn::Primary, Pos2::new(10.2, 10.2), false, false);

        app.drag_select(Pos2::new(6.2, 8.2), true);
        let sel = app.edit.selection.unwrap();
        assert_eq!(sel, IRect::new(6, 6, 11, 11));
        assert_eq!(sel.w(), sel.h());
    }

    #[test]
    fn escape_cancels_pending_after_save() {
        let ctx = Context::default();
        let mut app = App::new_headless(8, 8);
        app.file_dialog = Some(FileDialog::save("test.png", None));
        app.pending_after_save = Some(Pending::Quit);

        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut out = ctx.run_ui(raw, |ui| {
            shortcuts::handle(&mut app, ui.ctx(), None);
        });
        out.textures_delta.clear();
        assert!(app.file_dialog.is_none());
        assert!(app.pending_after_save.is_none());
    }

    #[test]
    fn long_stroke_across_large_canvas_roundtrips_cleanly() {
        let mut app = App::new_headless(400, 300);
        let initial = app.doc.px.clone();
        app.set_tool(Tool::Brush);
        app.opts.size = 10.0;
        app.colors.primary = Color32::from_rgb(220, 40, 90);

        app.pointer_press(Btn::Primary, Pos2::new(5.0, 5.0), false, false);
        for step in 1..=20 {
            let t = step as f32 / 20.0;
            app.pointer_move(Pos2::new(5.0 + 385.0 * t, 5.0 + 285.0 * t), false, 0.016);
        }
        app.pointer_release();

        assert!(app.doc.modified);
        assert_ne!(app.doc.get(200, 150), Some([255, 255, 255, 255]));
        app.undo();
        assert_eq!(app.doc.px, initial);
        assert!(!app.doc.modified);
    }

    #[test]
    fn full_ui_frame_renders_every_tool_and_modal_without_panicking() {
        let ctx = Context::default();
        theme::install_fonts(&ctx);
        let mut app = App::new_headless(64, 48);
        theme::apply(&ctx, &app.theme);
        app.view.zoom = 12.0; // exercise pixel grid + single-quad checkerboard
        app.view.fit_pending = false;
        app.edit.selection = Some(IRect::new(4, 4, 20, 16));

        let run_frame = |app: &mut App| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                let top = app.top_bar(ui, None);
                let bottom = app.status_bar(ui);
                let left = app.tool_rail(ui);
                let right = app.side_panel(ui);
                let center = app.canvas(ui);
                app.overlays(ui.ctx(), None);

                // Regression check: right and bottom panels must keep their exact
                // reserved edges and the central canvas must never overlap any panel.
                assert!((top.bottom() - ui::topbar::TOP_BAR_HEIGHT).abs() <= 1.0);
                assert!((bottom.top() - (800.0 - ui::statusbar::STATUS_BAR_HEIGHT)).abs() <= 1.0);
                assert!((left.right() - ui::sidebar::TOOL_RAIL_WIDTH).abs() <= 1.0);
                assert!((right.left() - (1280.0 - ui::sidebar::SIDE_PANEL_WIDTH)).abs() <= 1.0);
                assert!(
                    center.right() <= right.left() + 0.5,
                    "central panel ({center:?}) overlapped right panel ({right:?})"
                );
                assert!(
                    center.left() >= left.right() - 0.5,
                    "central panel ({center:?}) overlapped left rail ({left:?})"
                );
                assert!(
                    center.bottom() <= bottom.top() + 0.5,
                    "central panel ({center:?}) overlapped status bar ({bottom:?})"
                );
                assert!(
                    center.top() >= top.bottom() - 0.5,
                    "central panel ({center:?}) overlapped top bar ({top:?})"
                );
            });
            out.textures_delta.clear();
        };

        for tool in Tool::ALL {
            app.set_tool(tool);
            run_frame(&mut app);
        }
        for dialog in [
            Dialog::New,
            Dialog::Resize,
            Dialog::About,
            Dialog::Shortcuts,
            Dialog::Discard(Pending::New),
        ] {
            app.dialog = Some(dialog);
            run_frame(&mut app);
        }
        app.dialog = None;
        app.file_dialog = Some(FileDialog::save("demo.png", None));
        run_frame(&mut app);
        app.file_dialog = None;

        // Also render a light-theme frame to verify checker texture reload.
        app.prefs.theme_dark = false;
        app.theme = Theme::light();
        theme::apply(&ctx, &app.theme);
        run_frame(&mut app);

        // Ensure leaking/dropping GPU handles in headless mode releases cleanly.
        app.tex = None;
        app.checker_tex = None;
        let mut cleanup = ctx.run_ui(egui::RawInput::default(), |_| {});
        cleanup.textures_delta.clear();
    }

    #[test]
    fn panels_never_overlap_across_window_sizes_and_hidpi_scales() {
        let ctx = Context::default();
        theme::install_fonts(&ctx);
        let mut app = App::new_headless(1280, 800);
        theme::apply(&ctx, &app.theme);
        // Populate recent colours and a long font name to stress right-panel widths.
        for i in 0..16u8 {
            app.colors.push_recent(Color32::from_rgb(i * 15, 100, 200));
        }
        app.opts.font_family = "Very Long System Font Family Name Regular".into();

        for &(w, h) in &[
            (940.0_f32, 640.0_f32),
            (1280.0, 800.0),
            (1440.0, 900.0),
            (1920.0, 1080.0),
        ] {
            for &ppp in &[1.0_f32, 1.5, 2.0] {
                for tool in Tool::ALL {
                    app.set_tool(tool);
                    let mut raw = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
                        ..Default::default()
                    };
                    raw.viewports
                        .entry(raw.viewport_id)
                        .or_default()
                        .native_pixels_per_point = Some(ppp);
                    let mut out = ctx.run_ui(raw, |ui| {
                        let screen = ui.ctx().content_rect();
                        let top = app.top_bar(ui, None);
                        let bottom = app.status_bar(ui);
                        let left = app.tool_rail(ui);
                        let right = app.side_panel(ui);
                        let center = app.canvas(ui);

                        assert!(
                            (right.width() - ui::sidebar::SIDE_PANEL_WIDTH).abs() <= 1.0,
                            "right width mismatch at {w}x{h}@{ppp} ({tool:?}): {right:?}"
                        );
                        assert!(
                            (right.right() - screen.right()).abs() <= 1.0,
                            "right edge mismatch at {w}x{h}@{ppp} ({tool:?}): {right:?}"
                        );
                        assert!(
                            (center.right() - right.left()).abs() <= 1.0,
                            "center ({center:?}) did not meet right ({right:?}) at {w}x{h}@{ppp}"
                        );
                        assert!(
                            (center.left() - left.right()).abs() <= 1.0,
                            "center ({center:?}) did not meet left ({left:?}) at {w}x{h}@{ppp}"
                        );
                        assert!(
                            (center.top() - top.bottom()).abs() <= 1.0,
                            "center ({center:?}) did not meet top ({top:?}) at {w}x{h}@{ppp}"
                        );
                        assert!(
                            (center.bottom() - bottom.top()).abs() <= 1.0,
                            "center ({center:?}) did not meet bottom ({bottom:?}) at {w}x{h}@{ppp}"
                        );
                    });
                    out.textures_delta.clear();
                }
            }
        }
        app.tex = None;
        app.checker_tex = None;
        let mut cleanup = ctx.run_ui(egui::RawInput::default(), |_| {});
        cleanup.textures_delta.clear();
    }

    #[test]
    fn oversize_new_doc_leaves_current_state_untouched() {
        let mut app = App::new_headless(8, 8);
        app.doc.set(2, 2, [20, 40, 60, 255]);
        app.doc.modified = true;

        assert!(app.new_doc(9_000, 10, false).is_err());
        assert_eq!((app.doc.w, app.doc.h), (8, 8));
        assert_eq!(app.doc.get(2, 2), Some([20, 40, 60, 255]));
        assert!(app.doc.modified);
    }

    #[test]
    fn text_tool_focuses_on_click_captures_shortcuts_and_aligns_caret() {
        let ctx = Context::default();
        theme::install_fonts(&ctx);
        let mut app = App::new_headless(800, 500);
        theme::apply(&ctx, &app.theme);
        app.set_tool(Tool::Text);
        let initial_primary = app.colors.primary;

        let run_frame = |app: &mut App, events: Vec<egui::Event>, time: f64| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                events,
                time: Some(time),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                let c = ui.ctx().clone();
                shortcuts::handle(app, &c, None);
                let _ = app.top_bar(ui, None);
                let _ = app.status_bar(ui);
                let _ = app.tool_rail(ui);
                let _ = app.side_panel(ui);
                let _ = app.canvas(ui);
                app.overlays(&c, None);
            });
            out.textures_delta.clear();
            out.platform_output
        };

        // Frame 0: initial layout.
        let _ = run_frame(&mut app, vec![], 0.0);

        // Frames 1 & 2: single primary click on the canvas (press + release).
        let click_pos = Pos2::new(320.0, 260.0);
        let _ = run_frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(click_pos),
                egui::Event::PointerButton {
                    pos: click_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            0.1,
        );
        let out_after_release = run_frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: click_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            0.16,
        );

        // Immediately focused with platform IME active — no second click needed.
        assert!(app.edit.is_editing_text());
        assert!(app.canvas_text_captures_keys());
        assert!(out_after_release.ime.is_some());

        // Frame 3: press every single-key tool/colour shortcut + type a long
        // sentence; none may switch tools or swap colours, and the caret must
        // track the end of the single unwrapped line.
        let sample = "bvpeagitlrokhxd [] — long single line that stays on one row æøå";
        let mut key_events = Vec::new();
        for t in Tool::ALL {
            key_events.push(egui::Event::Key {
                key: t.shortcut_key(),
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        for k in [
            egui::Key::X,
            egui::Key::D,
            egui::Key::Space,
            egui::Key::OpenBracket,
            egui::Key::CloseBracket,
        ] {
            key_events.push(egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        key_events.push(egui::Event::Text(sample.to_owned()));
        let _ = run_frame(&mut app, key_events, 0.25);

        assert_eq!(app.tool, Tool::Text, "typing must never switch tools");
        assert_eq!(
            app.colors.primary, initial_primary,
            "X/D must not swap colours"
        );
        assert!(!app.space_pan, "Space must not enter pan mode while typing");
        assert_eq!(app.text_session().unwrap().text, sample);

        let lay = app.text_layout().unwrap();
        assert_eq!(
            lay.lines.len(),
            1,
            "single-line text must not wrap into a box"
        );
        let (li, cx, _) = lay.caret_pos(app.text_session().unwrap().cursor);
        assert_eq!(li, 0);
        assert!((cx - lay.width).abs() < 1e-3);
        let box_rect = app.edit.text_box_rect.unwrap();
        assert!(
            box_rect.width() >= lay.width * app.view.zoom,
            "text frame ({box_rect:?}) must expand with full line width ({})",
            lay.width * app.view.zoom
        );

        // Frame 4: Enter inserts line 2, IME preedit + commit appends é, and
        // Backspace removes the trailing '!'.
        let _ = run_frame(
            &mut app,
            vec![
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Text("Caf".to_owned()),
                egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "´".to_owned(),
                    active_range_chars: None,
                }),
                egui::Event::Ime(egui::ImeEvent::Commit("é!".to_owned())),
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            0.4,
        );
        assert_eq!(app.text_session().unwrap().text, format!("{sample}\nCafé"));
        assert_eq!(app.text_layout().unwrap().lines.len(), 2);

        // Frame 5: clicking elsewhere on the canvas commits the text cleanly
        // as a single undoable action.
        let outside = Pos2::new(220.0, 520.0);
        let _ = run_frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(outside),
                egui::Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            0.6,
        );
        assert!(!app.edit.is_editing_text());
        assert_eq!(app.hist.undo_len(), 1);
        assert!(app.doc.modified);

        app.undo();
        assert!(!app.doc.modified);

        app.tex = None;
        app.checker_tex = None;
        let mut cleanup = ctx.run_ui(egui::RawInput::default(), |_| {});
        cleanup.textures_delta.clear();
    }

    #[test]
    fn text_session_selection_undo_reposition_and_escape_behave_natively() {
        let mut app = App::new_headless(640, 400);
        app.set_tool(Tool::Text);
        app.start_text_session(Pos2::new(80.0, 100.0), 0.0, true);
        assert!(app.canvas_text_captures_keys());

        // Empty session repositions cleanly without losing focus or recording undo.
        app.reposition_empty_text(Pos2::new(140.0, 160.0), 0.2);
        assert!(app.canvas_text_captures_keys());
        assert!((app.text_session().unwrap().pos.0 - 140.0).abs() < 1e-3);
        assert_eq!(app.hist.undo_len(), 0);

        // Hex & font-filter temporarily borrow keys without ending the session.
        app.hex_focused = true;
        assert!(!app.canvas_text_captures_keys());
        app.hex_focused = false;
        app.font_filter_focused = true;
        assert!(!app.canvas_text_captures_keys());
        app.font_filter_focused = false;
        assert!(app.canvas_text_captures_keys());

        // Word selection, line selection, delete-word, and in-session undo/redo.
        {
            let live = app.text_session_mut().unwrap();
            live.insert_str("alpha beta_2\ngamma delta");
            live.checkpoint_undo();

            // Double-click inside "beta_2" selects the whole word.
            live.select_word_at(8);
            assert_eq!(live.selected_text(), "beta_2");

            // Triple-click on line 0 selects "alpha beta_2" without the '\n'.
            live.select_line_at(2);
            assert_eq!(live.selected_text(), "alpha beta_2");

            // Replace line 0 and verify undo/redo restores it.
            live.insert_str("replaced");
            live.checkpoint_undo();
            assert_eq!(live.text, "replaced\ngamma delta");
            assert!(live.undo_text());
            assert_eq!(live.text, "alpha beta_2\ngamma delta");
            assert!(live.redo_text());
            assert_eq!(live.text, "replaced\ngamma delta");

            // Vertical caret motion preserves preferred_x across shorter lines.
            live.insert_str("\nx\n0123456789abcdef");
        }
        let lay = app.text_layout().unwrap();
        let end_idx = app.text_session().unwrap().text.len();
        let (_, end_x, _) = lay.caret_pos(end_idx);
        {
            let live = app.text_session_mut().unwrap();
            live.set_cursor(end_idx, false, false);
            // Up onto short "x" line clamps to end of "x", keeping preferred_x.
            let i1 = lay.vertical_move(live.cursor, -1, &mut live.preferred_x);
            live.set_cursor(i1, false, true);
            assert!((live.preferred_x.unwrap() - end_x).abs() < 1e-3);
            // Down returns right back to the end of "0123456789abcdef".
            let i2 = lay.vertical_move(live.cursor, 1, &mut live.preferred_x);
            assert_eq!(i2, end_idx);
        }

        // Cancel discards the entire session cleanly with zero undo pollution.
        app.cancel_current();
        assert!(!app.edit.is_editing_text());
        assert_eq!(app.hist.undo_len(), 0);
        assert!(!app.doc.modified);
    }
}
