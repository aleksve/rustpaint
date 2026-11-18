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
        let hsva = egui::ecolor::Hsva::from_srgb([
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
            picker_hsv: (colors.primary, [hsva.h, hsva.s, hsva.v]),
            colors,
            color_target: Btn::Primary,
            hex_edit,
            hex_focused: false,
            font_filter: String::new(),
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

    pub(crate) fn theme_for_prefs(&self) -> Theme {
        if self.prefs.theme_dark {
            Theme::dark()
        } else {
            Theme::light()
        }
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

        self.top_bar(ui, Some(frame));
        self.status_bar(ui);
        self.tool_dock(ui);
        self.right_panel(ui);
        self.canvas(ui);
        self.overlays(&ctx, Some(frame));

        if let Some(color) = self.picked_color.take() {
            self.hex_edit = to_hex(color);
        }
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
                app.top_bar(ui, None);
                app.status_bar(ui);
                app.tool_dock(ui);
                app.right_panel(ui);
                app.canvas(ui);
                app.overlays(ui.ctx(), None);
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
    fn oversize_new_doc_leaves_current_state_untouched() {
        let mut app = App::new_headless(8, 8);
        app.doc.set(2, 2, [20, 40, 60, 255]);
        app.doc.modified = true;

        assert!(app.new_doc(9_000, 10, false).is_err());
        assert_eq!((app.doc.w, app.doc.h), (8, 8));
        assert_eq!(app.doc.get(2, 2), Some([20, 40, 60, 255]));
        assert!(app.doc.modified);
    }
}
