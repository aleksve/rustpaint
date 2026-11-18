//! Top menu bar and quick-action toolbar.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Sense, Stroke, Vec2};

use crate::app::{App, Dialog, Pending};
use crate::icons::{self, Icon};
use crate::paint::ImageOp;
use crate::theme;
use crate::widgets;

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui, frame: Option<&eframe::Frame>) {
        let th = self.theme;
        let ctx = ui.ctx().clone();
        egui::Panel::top("topbar")
            .exact_size(46.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .stroke(Stroke::new(1.0_f32, th.border)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Small brand mark.
                    let (mark, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    icons::draw(ui.painter(), Icon::Sparkle, mark, th.accent, 1.9);
                    ui.label(
                        egui::RichText::new("RustPaint")
                            .size(13.5)
                            .strong()
                            .color(th.text),
                    );
                    ui.add_space(6.0);

                    self.menus(ui, &ctx, frame);

                    widgets::vsep(ui, &th);

                    // Quick actions.
                    if widgets::icon_button(ui, &th, Icon::New, 30.0, false)
                        .on_hover_text("New (Ctrl+N)")
                        .clicked()
                    {
                        self.ask_new();
                    }
                    if widgets::icon_button(ui, &th, Icon::Open, 30.0, false)
                        .on_hover_text("Open (Ctrl+O)")
                        .clicked()
                    {
                        self.ask_open(&ctx, frame);
                    }
                    if widgets::icon_button(ui, &th, Icon::Save, 30.0, false)
                        .on_hover_text("Save (Ctrl+S)")
                        .clicked()
                    {
                        self.quick_save(&ctx, frame);
                    }

                    widgets::vsep(ui, &th);

                    let can_undo = self.hist.can_undo() || self.edit.session.is_some();
                    if widgets::icon_button_enabled(ui, &th, Icon::Undo, 30.0, false, can_undo)
                        .on_hover_text("Undo (Ctrl+Z)")
                        .clicked()
                    {
                        self.undo();
                    }
                    let can_redo = self.hist.can_redo();
                    if widgets::icon_button_enabled(ui, &th, Icon::Redo, 30.0, false, can_redo)
                        .on_hover_text("Redo (Ctrl+Y)")
                        .clicked()
                    {
                        self.redo();
                    }

                    widgets::vsep(ui, &th);

                    if widgets::icon_button(ui, &th, Icon::Grid, 30.0, self.view.grid)
                        .on_hover_text("Toggle pixel grid (Ctrl+')")
                        .clicked()
                    {
                        self.view.grid = !self.view.grid;
                    }
                    if widgets::icon_button(ui, &th, Icon::Fit, 30.0, false)
                        .on_hover_text("Fit canvas to window (Ctrl+9)")
                        .clicked()
                    {
                        let rect = self.last_view_rect;
                        self.fit_view(rect);
                    }

                    // Right-aligned theme switch.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let icon = if self.prefs.theme_dark {
                            Icon::Sun
                        } else {
                            Icon::Moon
                        };
                        let tip = if self.prefs.theme_dark {
                            "Switch to light theme"
                        } else {
                            "Switch to dark theme"
                        };
                        if widgets::icon_button(ui, &th, icon, 30.0, false)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.prefs.theme_dark = !self.prefs.theme_dark;
                            self.prefs.save();
                            self.theme = self.theme_for_prefs();
                            theme::apply(&ctx, &self.theme);
                        }
                    });
                });
            });
    }

    fn menus(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        let th = self.theme;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;

        ui.menu_button(menu_label("File", th.text), |ui| {
            ui.set_min_width(220.0);
            if widgets::menu_item(ui, &th, "New…", "Ctrl+N", true) {
                self.ask_new();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Open…", "Ctrl+O", true) {
                self.ask_open(ctx, frame);
                ui.close();
            }
            if !self.prefs.recent.is_empty() {
                ui.menu_button("Open recent", |ui| {
                    ui.set_min_width(260.0);
                    let mut chosen: Option<PathBuf> = None;
                    let mut clear = false;
                    for path in &self.prefs.recent {
                        let name = crate::prefs::shorten(path, 38);
                        if ui
                            .button(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            chosen = Some(path.clone());
                            ui.close();
                        }
                    }
                    widgets::menu_sep(ui, &th);
                    if ui.button("Clear list").clicked() {
                        clear = true;
                        ui.close();
                    }
                    if let Some(path) = chosen {
                        if self.doc.modified {
                            self.dialog = Some(Dialog::Discard(Pending::OpenPath(path)));
                        } else {
                            self.start_open_file(path, ctx);
                        }
                    } else if clear {
                        self.prefs.clear_recent();
                        self.prefs.save();
                    }
                });
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Save", "Ctrl+S", true) {
                self.quick_save(ctx, frame);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Save as…", "Ctrl+⇧+S", true) {
                self.save_dialog(ctx, frame);
                ui.close();
            }
            if crate::native::available() {
                widgets::menu_sep(ui, &th);
                let mut use_native = self.native_dialogs;
                if ui
                    .checkbox(&mut use_native, "Use system file dialogs")
                    .on_hover_text("Turn off to use the built-in file browser instead")
                    .changed()
                {
                    self.native_dialogs = use_native;
                    self.prefs.native_dialogs = Some(use_native);
                    self.prefs.save();
                }
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Quit", "Ctrl+Q", true) {
                ui.close();
                if self.doc.modified {
                    self.dialog = Some(Dialog::Discard(Pending::Quit));
                } else {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });

        ui.menu_button(menu_label("Edit", th.text), |ui| {
            ui.set_min_width(220.0);
            let can_undo = self.hist.can_undo() || self.edit.session.is_some();
            if widgets::menu_item(ui, &th, "Undo", "Ctrl+Z", can_undo) {
                self.undo();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Redo", "Ctrl+Y", self.hist.can_redo()) {
                self.redo();
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            let has_selection = self.edit.has_selection();
            if widgets::menu_item(ui, &th, "Cut", "Ctrl+X", has_selection) {
                self.cut_selection();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Copy", "Ctrl+C", has_selection) {
                self.copy_selection();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Paste", "Ctrl+V", self.clipboard.can_paste()) {
                self.paste_with_toast(ctx);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Delete selection", "Del", has_selection) {
                self.delete_selection();
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Select all", "Ctrl+A", true) {
                self.select_all();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Deselect", "Ctrl+D", has_selection) {
                self.deselect();
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Clear canvas", "", true) {
                self.clear_canvas();
                ui.close();
            }
        });

        ui.menu_button(menu_label("Image", th.text), |ui| {
            ui.set_min_width(220.0);
            if widgets::menu_item(ui, &th, "Canvas size…", "Ctrl+E", true) {
                self.open_resize_dialog();
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Crop to selection", "", self.edit.has_selection()) {
                self.crop_to_selection();
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Flip horizontal", "", true) {
                self.image_op(ImageOp::FlipH);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Flip vertical", "", true) {
                self.image_op(ImageOp::FlipV);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Rotate 90° clockwise", "", true) {
                self.image_op(ImageOp::RotateCw);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Rotate 90° counter-clockwise", "", true) {
                self.image_op(ImageOp::RotateCcw);
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            if widgets::menu_item(ui, &th, "Invert colours", "", true) {
                self.image_op(ImageOp::Invert);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Convert to grayscale", "", true) {
                self.image_op(ImageOp::Grayscale);
                ui.close();
            }
        });

        ui.menu_button(menu_label("View", th.text), |ui| {
            ui.set_min_width(220.0);
            let rect = self.last_view_rect;
            if widgets::menu_item(ui, &th, "Zoom in", "Ctrl++", true) {
                self.zoom_step(1.0, rect);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Zoom out", "Ctrl+-", true) {
                self.zoom_step(-1.0, rect);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Actual size (100%)", "Ctrl+0", true) {
                self.set_zoom(1.0, rect);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "Fit canvas", "Ctrl+9", true) {
                self.fit_view(rect);
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            if ui
                .checkbox(&mut self.view.grid, "Pixel grid (Ctrl+')")
                .clicked()
            {
                ui.close();
            }
            widgets::menu_sep(ui, &th);
            let theme_label = if self.prefs.theme_dark {
                "Light theme"
            } else {
                "Dark theme"
            };
            if widgets::menu_item(ui, &th, theme_label, "", true) {
                self.prefs.theme_dark = !self.prefs.theme_dark;
                self.prefs.save();
                self.theme = self.theme_for_prefs();
                theme::apply(ctx, &self.theme);
                ui.close();
            }
        });

        ui.menu_button(menu_label("Help", th.text), |ui| {
            ui.set_min_width(200.0);
            if widgets::menu_item(ui, &th, "Keyboard shortcuts", "F1", true) {
                self.dialog = Some(Dialog::Shortcuts);
                ui.close();
            }
            if widgets::menu_item(ui, &th, "About RustPaint", "", true) {
                self.dialog = Some(Dialog::About);
                ui.close();
            }
        });
    }
}

fn menu_label(text: &str, color: Color32) -> egui::RichText {
    egui::RichText::new(text).size(12.5).color(color)
}
