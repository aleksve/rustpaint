//! Top toolbar and application menus.

use std::path::PathBuf;

use eframe::egui::{self, Context, Ui, Vec2};

use super::super::{App, Dialog, Pending};
use crate::icons::{self, Icon};
use crate::paint::ImageOp;
use crate::theme::{self, Mode, Theme};
use crate::widgets;

impl App {
    // ------------------------------------------------------------------- top

    pub(crate) fn top_bar(&mut self, ctx: &Context) {
        let th = self.theme;
        egui::TopBottomPanel::top("topbar")
            .exact_height(54.0)
            .frame(
                egui::Frame::none()
                    .fill(th.panel)
                    .stroke(egui::Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin {
                        left: 10.0,
                        right: 10.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (lr, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                    icons::draw(ui.painter(), Icon::Sparkle, lr, th.accent, 1.4);
                    ui.label(
                        egui::RichText::new("RustPaint")
                            .size(14.0)
                            .strong()
                            .color(th.text),
                    );
                    ui.add_space(6.0);

                    // Collapse the quick actions to icons on narrow windows.
                    let wide = ui.available_width() > 760.0;
                    let new_resp = if wide {
                        widgets::icon_text_button(ui, &th, Icon::New, "New", false)
                    } else {
                        widgets::icon_button(ui, &th, Icon::New, 32.0, false)
                    };
                    if new_resp.on_hover_text("New image (Ctrl N)").clicked() {
                        self.ask_new();
                    }
                    let open_resp = if wide {
                        widgets::icon_text_button(ui, &th, Icon::Open, "Open", false)
                    } else {
                        widgets::icon_button(ui, &th, Icon::Open, 32.0, false)
                    };
                    if open_resp.on_hover_text("Open image (Ctrl O)").clicked() {
                        self.ask_open(ctx);
                    }
                    let save_resp = if wide {
                        widgets::icon_text_button(ui, &th, Icon::Save, "Save", false)
                    } else {
                        widgets::icon_button(ui, &th, Icon::Save, 32.0, false)
                    };
                    if save_resp.on_hover_text("Save (Ctrl S)").clicked() {
                        self.quick_save(ctx);
                    }

                    let (r, _) =
                        ui.allocate_exact_size(Vec2::new(11.0, 22.0), egui::Sense::hover());
                    ui.painter().line_segment(
                        [
                            egui::Pos2::new(r.center().x, r.top() + 2.0),
                            egui::Pos2::new(r.center().x, r.bottom() - 2.0),
                        ],
                        egui::Stroke::new(1.0_f32, th.border),
                    );

                    let undo_ok = self.hist.can_undo();
                    if widgets::icon_button(ui, &th, Icon::Undo, 32.0, false).clicked() && undo_ok {
                        self.undo();
                    }
                    let redo_ok = self.hist.can_redo();
                    if widgets::icon_button(ui, &th, Icon::Redo, 32.0, false).clicked() && redo_ok {
                        self.redo();
                    }

                    ui.add_space(4.0);
                    ui.menu_button("File", |ui| self.menu_file(ui, ctx));
                    ui.menu_button("Edit", |ui| self.menu_edit(ui));
                    ui.menu_button("Image", |ui| self.menu_image(ui));
                    ui.menu_button("View", |ui| self.menu_view(ui));
                    ui.menu_button("Help", |ui| self.menu_help(ui));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let icon = if self.theme.is_dark() {
                            Icon::Sun
                        } else {
                            Icon::Moon
                        };
                        if widgets::icon_button(ui, &th, icon, 32.0, false)
                            .on_hover_text("Toggle light / dark theme")
                            .clicked()
                        {
                            self.theme = match self.theme.mode {
                                Mode::Dark => Theme::light(),
                                Mode::Light => Theme::dark(),
                            };
                            theme::apply(ctx, &self.theme);
                            self.prefs.theme_dark = self.theme.is_dark();
                            self.prefs.save();
                        }
                        if widgets::icon_button(ui, &th, Icon::Grid, 32.0, self.view.grid)
                            .on_hover_text("Pixel grid (G)")
                            .clicked()
                        {
                            self.view.grid = !self.view.grid;
                        }
                        if widgets::icon_button(ui, &th, Icon::Fit, 32.0, false)
                            .on_hover_text("Fit canvas to window (Ctrl+9)")
                            .clicked()
                        {
                            self.fit_view(self.last_view_rect);
                        }
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(self.doc.display_name())
                                .size(12.0)
                                .color(th.text_dim),
                        );
                        if self.doc.modified {
                            ui.label(egui::RichText::new("•").size(14.0).color(th.accent));
                        }
                    });
                });
            });
    }

    pub(crate) fn menu_file(&mut self, ui: &mut Ui, ctx: &Context) {
        let th = self.theme;
        if widgets::menu_item(ui, &th, "New image…", "Ctrl N", true) {
            self.ask_new();
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Open image…", "Ctrl O", true) {
            self.ask_open(ctx);
            ui.close_menu();
        }
        let recent = self.prefs.recent.clone();
        if !recent.is_empty() {
            let mut want: Option<PathBuf> = None;
            ui.menu_button("Open recent", |ui| {
                for p in &recent {
                    let label = crate::prefs::shorten(p, 42);
                    if widgets::menu_item(ui, &th, &label, "", true) {
                        want = Some(p.clone());
                        ui.close_menu();
                    }
                }
                widgets::menu_sep(ui, &th);
                if widgets::menu_item(ui, &th, "Clear list", "", true) {
                    self.prefs.clear_recent();
                    self.prefs.save();
                    ui.close_menu();
                }
            });
            if let Some(p) = want {
                if self.doc.modified {
                    self.dialog = Some(Dialog::Discard(Pending::OpenPath(p)));
                } else {
                    self.read_file(p, ctx);
                }
            }
        }
        if widgets::menu_item(ui, &th, "Save", "Ctrl S", true) {
            self.quick_save(ctx);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Save as…", "Ctrl ⇧ S", true) {
            self.save_dialog(ctx);
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        let native_ok = crate::native::available();
        let native_label = if !native_ok {
            "Use system file dialogs (unavailable)"
        } else if self.native_dialogs {
            "Use system file dialogs  ✓"
        } else {
            "Use system file dialogs"
        };
        if widgets::menu_item(ui, &th, native_label, "", native_ok) {
            self.native_dialogs = !self.native_dialogs;
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Canvas size…", "Ctrl E", true) {
            self.resize_w = self.doc.w as u32;
            self.resize_h = self.doc.h as u32;
            self.dialog = Some(Dialog::Resize);
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Quit", "", true) {
            if self.doc.modified {
                self.dialog = Some(Dialog::Discard(Pending::Quit));
            } else {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.close_menu();
        }
    }

    pub(crate) fn menu_edit(&mut self, ui: &mut Ui) {
        let th = self.theme;
        let undo = self.hist.can_undo();
        let redo = self.hist.can_redo();
        let has_sel = self.edit.has_selection();
        if widgets::menu_item(ui, &th, "Undo", "Ctrl Z", undo) {
            self.undo();
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Redo", "Ctrl ⇧ Z", redo) {
            self.redo();
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Cut", "Ctrl X", has_sel) {
            self.cut_selection();
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Copy", "Ctrl C", has_sel) {
            self.copy_selection();
            ui.close_menu();
        }
        let can_paste = self.edit.clipboard.is_some();
        if widgets::menu_item(ui, &th, "Paste", "Ctrl V", can_paste) {
            self.paste();
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Delete selection", "Del", has_sel) {
            self.delete_selection();
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Select all", "Ctrl A", true) {
            self.select_all();
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Deselect", "Ctrl D", has_sel) {
            self.deselect();
            ui.close_menu();
        }
    }

    pub(crate) fn menu_image(&mut self, ui: &mut Ui) {
        let th = self.theme;
        if widgets::menu_item(ui, &th, "Flip horizontal", "", true) {
            self.image_op(ImageOp::FlipH);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Flip vertical", "", true) {
            self.image_op(ImageOp::FlipV);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Rotate left", "", true) {
            self.image_op(ImageOp::RotateCcw);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Rotate right", "", true) {
            self.image_op(ImageOp::RotateCw);
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Invert colours", "", true) {
            self.image_op(ImageOp::Invert);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Grayscale", "", true) {
            self.image_op(ImageOp::Grayscale);
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Clear canvas", "", true) {
            self.clear_canvas();
            ui.close_menu();
        }
        let has_sel = self.edit.selection.is_some();
        if widgets::menu_item(ui, &th, "Crop to selection", "", has_sel) {
            self.crop_to_selection();
            ui.close_menu();
        }
    }

    pub(crate) fn menu_view(&mut self, ui: &mut Ui) {
        let th = self.theme;
        let r = self.last_view_rect;
        if widgets::menu_item(ui, &th, "Zoom in", "Ctrl +", true) {
            self.zoom_step(1.0, r);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Zoom out", "Ctrl -", true) {
            self.zoom_step(-1.0, r);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Actual size", "Ctrl 0", true) {
            self.set_zoom(1.0, r);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "Fit to window", "Ctrl 9", true) {
            self.fit_view(r);
            ui.close_menu();
        }
        widgets::menu_sep(ui, &th);
        let grid = self.view.grid;
        if widgets::menu_item(
            ui,
            &th,
            if grid {
                "Hide pixel grid"
            } else {
                "Show pixel grid"
            },
            "",
            true,
        ) {
            self.view.grid = !grid;
            ui.close_menu();
        }
        let dark = self.theme.is_dark();
        if widgets::menu_item(
            ui,
            &th,
            if dark { "Light theme" } else { "Dark theme" },
            "",
            true,
        ) {
            self.theme = if dark { Theme::light() } else { Theme::dark() };
            self.prefs.theme_dark = self.theme.is_dark();
            self.prefs.save();
            theme::apply(ui.ctx(), &self.theme);
            ui.close_menu();
        }
    }

    pub(crate) fn menu_help(&mut self, ui: &mut Ui) {
        let th = self.theme;
        if widgets::menu_item(ui, &th, "Keyboard shortcuts", "F1", true) {
            self.dialog = Some(Dialog::Shortcuts);
            ui.close_menu();
        }
        if widgets::menu_item(ui, &th, "About RustPaint", "", true) {
            self.dialog = Some(Dialog::About);
            ui.close_menu();
        }
    }
}
