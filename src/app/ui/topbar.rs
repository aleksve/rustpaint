//! Top toolbar and application menus.

use std::path::PathBuf;

use eframe::egui::{self, Context, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

use crate::app::{App, Dialog, Pending};
use crate::icons::{self, Icon};
use crate::paint::ImageOp;
use crate::theme::{self, Mode, Theme};
use crate::widgets;

macro_rules! sc {
    ($k:literal) => {
        if cfg!(target_os = "macos") {
            concat!("⌘", $k)
        } else {
            concat!("Ctrl ", $k)
        }
    };
    (shift $k:literal) => {
        if cfg!(target_os = "macos") {
            concat!("⇧⌘", $k)
        } else {
            concat!("Ctrl ⇧ ", $k)
        }
    };
    (tip $label:literal, $k:literal) => {
        if cfg!(target_os = "macos") {
            concat!($label, " (⌘", $k, ")")
        } else {
            concat!($label, " (Ctrl ", $k, ")")
        }
    };
    (tip_shift $label:literal, $k:literal) => {
        if cfg!(target_os = "macos") {
            concat!($label, " (⇧⌘", $k, ")")
        } else {
            concat!($label, " (Ctrl ⇧ ", $k, ")")
        }
    };
}

pub(crate) const TOP_BAR_HEIGHT: f32 = 54.0;

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut Ui, frame: Option<&eframe::Frame>) -> Rect {
        let th = self.theme;
        let ctx = ui.ctx().clone();
        egui::Panel::top("topbar")
            .exact_size(TOP_BAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                let body = ui.max_rect();
                let mut row = ui.new_child(UiBuilder::new().max_rect(body).layout(*ui.layout()));
                row.set_clip_rect(body.intersect(ui.clip_rect()));
                row.horizontal_centered(|ui| {
                    let (lr, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
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
                    if new_resp.on_hover_text(sc!(tip "New image", "N")).clicked() {
                        self.ask_new();
                    }
                    let open_resp = if wide {
                        widgets::icon_text_button(ui, &th, Icon::Open, "Open", false)
                    } else {
                        widgets::icon_button(ui, &th, Icon::Open, 32.0, false)
                    };
                    if open_resp
                        .on_hover_text(sc!(tip "Open image", "O"))
                        .clicked()
                    {
                        self.ask_open(&ctx, frame);
                    }
                    let save_resp = if wide {
                        widgets::icon_text_button(ui, &th, Icon::Save, "Save", false)
                    } else {
                        widgets::icon_button(ui, &th, Icon::Save, 32.0, false)
                    };
                    if save_resp.on_hover_text(sc!(tip "Save", "S")).clicked() {
                        self.quick_save(&ctx, frame);
                    }

                    let (sep, _) = ui.allocate_exact_size(Vec2::new(11.0, 22.0), Sense::hover());
                    ui.painter().line_segment(
                        [
                            egui::Pos2::new(sep.center().x, sep.top() + 2.0),
                            egui::Pos2::new(sep.center().x, sep.bottom() - 2.0),
                        ],
                        Stroke::new(1.0_f32, th.border),
                    );

                    let undo_ok = self.hist.can_undo() || self.edit.session.is_some();
                    if widgets::icon_button_enabled(ui, &th, Icon::Undo, 32.0, false, undo_ok)
                        .on_hover_text(sc!(tip "Undo", "Z"))
                        .clicked()
                    {
                        self.undo();
                    }
                    let redo_ok = self.hist.can_redo();
                    if widgets::icon_button_enabled(ui, &th, Icon::Redo, 32.0, false, redo_ok)
                        .on_hover_text(sc!(tip_shift "Redo", "Z"))
                        .clicked()
                    {
                        self.redo();
                    }

                    ui.add_space(4.0);
                    ui.menu_button("File", |ui| self.menu_file(ui, &ctx, frame));
                    ui.menu_button("Edit", |ui| self.menu_edit(ui, &ctx));
                    ui.menu_button("Image", |ui| self.menu_image(ui));
                    ui.menu_button("View", |ui| self.menu_view(ui, &ctx));
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
                            theme::apply(&ctx, &self.theme);
                            self.prefs.theme_dark = self.theme.is_dark();
                            self.prefs.save();
                        }
                        if widgets::icon_button(ui, &th, Icon::Grid, 32.0, self.view.grid)
                            .on_hover_text(if cfg!(target_os = "macos") {
                                "Pixel grid (⌘')"
                            } else {
                                "Pixel grid (Ctrl ')"
                            })
                            .clicked()
                        {
                            self.view.grid = !self.view.grid;
                        }
                        if widgets::icon_button(ui, &th, Icon::Fit, 32.0, false)
                            .on_hover_text(if cfg!(target_os = "macos") {
                                "Fit canvas to window (⌘9)"
                            } else {
                                "Fit canvas to window (Ctrl 9)"
                            })
                            .clicked()
                        {
                            self.fit_view(self.last_view_rect);
                        }
                        let rem = ui.available_width() - 14.0;
                        if rem > 52.0 {
                            ui.add_space(6.0);
                            let reserve = if self.doc.modified { 16.0 } else { 0.0 };
                            let max_chars =
                                (((rem - reserve).max(0.0) / 7.2) as usize).clamp(6, 36);
                            let name = self.doc.display_name();
                            ui.label(
                                egui::RichText::new(super::super::elide(&name, max_chars))
                                    .size(12.0)
                                    .color(th.text_dim),
                            );
                            if self.doc.modified {
                                ui.label(egui::RichText::new("•").size(14.0).color(th.accent));
                            }
                        }
                    });
                });
                ui.allocate_rect(body, Sense::hover());
            })
            .response
            .rect
    }

    fn menu_file(&mut self, ui: &mut Ui, ctx: &Context, frame: Option<&eframe::Frame>) {
        let th = self.theme;
        ui.set_width(248.0);
        if widgets::menu_item(ui, &th, "New image…", sc!("N"), true) {
            self.ask_new();
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Open image…", sc!("O"), true) {
            self.ask_open(ctx, frame);
            ui.close();
        }
        if !self.prefs.recent.is_empty() {
            let mut want: Option<PathBuf> = None;
            let mut clear = false;
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
                ui.menu_button("Open recent", |ui| {
                    ui.set_width(268.0);
                    for p in &self.prefs.recent {
                        let label = crate::prefs::shorten(p, 38);
                        if widgets::menu_item(ui, &th, &label, "", true) {
                            want = Some(p.clone());
                            ui.close();
                        }
                    }
                    widgets::menu_sep(ui, &th);
                    if widgets::menu_item(ui, &th, "Clear list", "", true) {
                        clear = true;
                        ui.close();
                    }
                });
            });
            if let Some(p) = want {
                if self.doc.modified {
                    self.dialog = Some(Dialog::Discard(Pending::OpenPath(p)));
                } else {
                    self.start_open_file(p, ctx);
                }
            } else if clear {
                self.prefs.clear_recent();
                self.prefs.save();
            }
        }
        if widgets::menu_item(ui, &th, "Save", sc!("S"), true) {
            self.quick_save(ctx, frame);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Save as…", sc!(shift "S"), true) {
            self.save_dialog(ctx, frame);
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        let native_ok = crate::native::available();
        let native_label = if !native_ok {
            "System file dialogs (unavailable)"
        } else if self.native_dialogs {
            "Use system file dialogs  ✓"
        } else {
            "Use system file dialogs"
        };
        if widgets::menu_item(ui, &th, native_label, "", native_ok) {
            self.native_dialogs = !self.native_dialogs;
            self.prefs.native_dialogs = Some(self.native_dialogs);
            self.prefs.save();
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Canvas size…", sc!("E"), true) {
            self.open_resize_dialog();
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Quit", sc!("Q"), true) {
            ui.close();
            if self.doc.modified {
                self.dialog = Some(Dialog::Discard(Pending::Quit));
            } else {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn menu_edit(&mut self, ui: &mut Ui, ctx: &Context) {
        let th = self.theme;
        ui.set_width(236.0);
        let undo = self.hist.can_undo() || self.edit.session.is_some();
        let redo = self.hist.can_redo();
        let has_sel = self.edit.has_selection();
        if widgets::menu_item(ui, &th, "Undo", sc!("Z"), undo) {
            self.undo();
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Redo", sc!(shift "Z"), redo) {
            self.redo();
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Cut", sc!("X"), has_sel) {
            self.cut_selection();
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Copy", sc!("C"), has_sel) {
            self.copy_selection();
            ui.close();
        }
        let can_paste = self.clipboard.can_paste();
        if widgets::menu_item(ui, &th, "Paste", sc!("V"), can_paste) {
            self.paste_with_toast(ctx);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Delete selection", "Del", has_sel) {
            self.delete_selection();
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Select all", sc!("A"), true) {
            self.select_all();
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Deselect", sc!("D"), has_sel) {
            self.deselect();
            ui.close();
        }
    }

    fn menu_image(&mut self, ui: &mut Ui) {
        let th = self.theme;
        ui.set_width(228.0);
        if widgets::menu_item(ui, &th, "Canvas size…", sc!("E"), true) {
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
        if widgets::menu_item(ui, &th, "Rotate left", "", true) {
            self.image_op(ImageOp::RotateCcw);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Rotate right", "", true) {
            self.image_op(ImageOp::RotateCw);
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Invert colours", "", true) {
            self.image_op(ImageOp::Invert);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Grayscale", "", true) {
            self.image_op(ImageOp::Grayscale);
            ui.close();
        }
        widgets::menu_sep(ui, &th);
        if widgets::menu_item(ui, &th, "Clear canvas", "", true) {
            self.clear_canvas();
            ui.close();
        }
    }

    fn menu_view(&mut self, ui: &mut Ui, ctx: &Context) {
        let th = self.theme;
        ui.set_width(228.0);
        let r = self.last_view_rect;
        if widgets::menu_item(ui, &th, "Zoom in", sc!("+"), true) {
            self.zoom_step(1.0, r);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Zoom out", sc!("-"), true) {
            self.zoom_step(-1.0, r);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Actual size", sc!("0"), true) {
            self.set_zoom(1.0, r);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "Fit to window", sc!("9"), true) {
            self.fit_view(r);
            ui.close();
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
            sc!("'"),
            true,
        ) {
            self.view.grid = !grid;
            ui.close();
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
            theme::apply(ctx, &self.theme);
            ui.close();
        }
    }

    fn menu_help(&mut self, ui: &mut Ui) {
        let th = self.theme;
        ui.set_width(220.0);
        if widgets::menu_item(ui, &th, "Keyboard shortcuts", "F1", true) {
            self.dialog = Some(Dialog::Shortcuts);
            ui.close();
        }
        if widgets::menu_item(ui, &th, "About RustPaint", "", true) {
            self.dialog = Some(Dialog::About);
            ui.close();
        }
    }
}
