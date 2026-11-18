//! Modal dialogs and their small state transitions.

use eframe::egui::{self, Context, Sense, Vec2};

use super::{App, Dialog, Pending};
use crate::icons::{self, Icon};
use crate::widgets;

impl App {
    pub(crate) fn show_dialogs(&mut self, ctx: &Context) {
        let th = self.theme;
        let Some(dialog) = self.dialog.clone() else {
            return;
        };

        match dialog {
            Dialog::New => self.new_dialog(ctx, th),
            Dialog::Resize => self.resize_dialog(ctx, th),
            Dialog::About => self.about_dialog(ctx, th),
            Dialog::Shortcuts => self.shortcuts_dialog(ctx, th),
            Dialog::Discard(pending) => self.discard_dialog(ctx, th, pending),
        }
    }

    fn new_dialog(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let (mut width, mut height, mut transparent) =
            (self.new_w, self.new_h, self.new_transparent);
        let mut close = false;
        let mut create = false;

        widgets::modal(ctx, &th, "new", Vec2::new(430.0, 336.0), |ui| {
            if widgets::modal_header(ui, &th, "New image") {
                close = true;
            }
            ui.label(
                egui::RichText::new("Canvas size")
                    .size(11.0)
                    .color(th.text_dim),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (label, preset_width, preset_height) in [
                    ("1280×800", 1280, 800),
                    ("1920×1080", 1920, 1080),
                    ("1080×1080", 1080, 1080),
                    ("800×600", 800, 600),
                ] {
                    if widgets::button(ui, &th, label, false).clicked() {
                        width = preset_width;
                        height = preset_height;
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Width").size(12.0).color(th.text_dim));
                ui.add(egui::DragValue::new(&mut width).range(1..=8192).speed(4.0));
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Height").size(12.0).color(th.text_dim));
                ui.add(egui::DragValue::new(&mut height).range(1..=8192).speed(4.0));
            });
            ui.add_space(6.0);
            let megapixels = (width as f64 * height as f64) / 1_000_000.0;
            let dimension_error =
                crate::doc::validate_dimensions(width as usize, height as usize).err();
            let invalid = dimension_error.is_some();
            ui.label(
                egui::RichText::new(if invalid {
                    format!(
                        "{width} × {height} · {megapixels:.1} MP — {}, pick something smaller",
                        dimension_error.expect("error exists")
                    )
                } else {
                    format!(
                        "{width} × {height} · {megapixels:.1} MP · about {:.0} MB in memory",
                        megapixels * 4.0
                    )
                })
                .size(10.5)
                .color(if invalid { th.danger } else { th.text_dim }),
            );
            ui.add_space(6.0);
            widgets::toggle(ui, &th, &mut transparent, "Transparent background");
            ui.add_space(10.0);
            dialog_actions(ui, |ui| {
                if widgets::button(ui, &th, "Create", !invalid).clicked() && !invalid {
                    create = true;
                }
                if widgets::button(ui, &th, "Cancel", false).clicked() {
                    close = true;
                }
            });
        });

        self.new_w = width;
        self.new_h = height;
        self.new_transparent = transparent;
        if create {
            match self.new_doc(width as usize, height as usize, transparent) {
                Ok(()) => self.dialog = None,
                Err(error) => self.toast(ctx, error.to_string()),
            }
        } else if close {
            self.dialog = None;
        }
    }

    fn resize_dialog(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let (mut width, mut height, mut scale) = (self.resize_w, self.resize_h, self.resize_scale);
        let mut close = false;
        let mut apply = false;

        widgets::modal(ctx, &th, "resize", Vec2::new(420.0, 280.0), |ui| {
            if widgets::modal_header(ui, &th, "Canvas size") {
                close = true;
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Width").size(12.0).color(th.text_dim));
                ui.add(egui::DragValue::new(&mut width).range(1..=8192).speed(4.0));
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Height").size(12.0).color(th.text_dim));
                ui.add(egui::DragValue::new(&mut height).range(1..=8192).speed(4.0));
            });
            ui.add_space(6.0);
            let dimension_error =
                crate::doc::validate_dimensions(width as usize, height as usize).err();
            if let Some(error) = &dimension_error {
                ui.label(
                    egui::RichText::new(error.to_string())
                        .size(10.5)
                        .color(th.danger),
                );
            }
            widgets::toggle(ui, &th, &mut scale, "Scale artwork to fit");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(if scale {
                    "The artwork is resampled to the new size."
                } else {
                    "The canvas is cropped / extended; pixels keep their size."
                })
                .size(10.5)
                .color(th.text_dim),
            );
            ui.add_space(10.0);
            dialog_actions(ui, |ui| {
                if widgets::button(ui, &th, "Apply", dimension_error.is_none()).clicked()
                    && dimension_error.is_none()
                {
                    apply = true;
                }
                if widgets::button(ui, &th, "Cancel", false).clicked() {
                    close = true;
                }
            });
        });

        self.resize_w = width;
        self.resize_h = height;
        self.resize_scale = scale;
        if apply {
            match self.resize_canvas(width as usize, height as usize, scale) {
                Ok(()) => self.dialog = None,
                Err(error) => self.toast(ctx, error.to_string()),
            }
        } else if close {
            self.dialog = None;
        }
    }

    fn about_dialog(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let mut close = false;
        widgets::modal(ctx, &th, "about", Vec2::new(440.0, 300.0), |ui| {
            if widgets::modal_header(ui, &th, "About RustPaint") {
                close = true;
            }
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Sparkle, rect, th.accent, 2.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("RustPaint").size(18.0).strong());
                    ui.label(
                        egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .size(11.5)
                            .color(th.text_dim),
                    );
                });
            });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "A modern take on MS Paint, written in Rust with egui. \
                     One codebase, native builds for Windows, macOS and Linux.",
                )
                .size(12.0)
                .color(th.text),
            );
            ui.add_space(8.0);
            for line in [
                "• Layers: single canvas, straight-alpha RGBA",
                "• GPU accelerated rendering via wgpu/glow",
                "• Undo history is diff based and memory capped",
                "• Fonts and icons are embedded — no runtime assets",
            ] {
                ui.label(egui::RichText::new(line).size(11.5).color(th.text_dim));
            }
            ui.add_space(10.0);
            dialog_actions(ui, |ui| {
                if widgets::button(ui, &th, "Close", true).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.dialog = None;
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let mut close = false;
        widgets::modal(ctx, &th, "shortcuts", Vec2::new(560.0, 470.0), |ui| {
            if widgets::modal_header(ui, &th, "Keyboard shortcuts") {
                close = true;
            }
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    let rows: [(&str, &str); 28] = [
                        ("Tools", ""),
                        ("Select / Brush / Pencil", "V / B / P"),
                        ("Eraser / Airbrush / Fill", "E / A / G"),
                        ("Line / Rect / Ellipse", "L / R / O"),
                        ("Polygon / Text / Picker", "K / T / I"),
                        ("Pan", "H or Space-drag"),
                        ("Brush size", "[ and ]"),
                        ("Swap / reset colours", "X / D"),
                        ("Editing", ""),
                        ("Undo / Redo", "Ctrl Z / Ctrl ⇧ Z"),
                        ("Cut / Copy / Paste", "Ctrl X / C / V"),
                        ("Delete selection", "Del"),
                        ("Select all / Deselect", "Ctrl A / Ctrl D"),
                        ("Nudge selection", "Arrow keys (⇧ = 10 px)"),
                        ("Finish polygon / text", "Enter / Ctrl Enter"),
                        ("Cancel", "Esc"),
                        ("Files", ""),
                        ("New / Open / Save", "Ctrl N / O / S"),
                        ("Save as", "Ctrl ⇧ S"),
                        ("Canvas size", "Ctrl E"),
                        ("View", ""),
                        ("Zoom in / out", "Ctrl + / Ctrl -"),
                        ("Actual size / Fit", "Ctrl 0 / Ctrl 9"),
                        ("Pan view", "Space-drag or wheel"),
                        ("Zoom at cursor", "Ctrl + wheel"),
                        ("Pixel grid", "toolbar button"),
                        ("Pick colour mid-stroke", "Alt + click"),
                        ("Draw with secondary colour", "Right-drag"),
                    ];
                    for (label, shortcut) in rows {
                        if shortcut.is_empty() {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(label.to_uppercase())
                                    .size(10.0)
                                    .color(th.text_dim),
                            );
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(label).size(12.0).color(th.text));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(shortcut)
                                                .size(12.0)
                                                .color(th.text_dim),
                                        );
                                    },
                                );
                            });
                        }
                    }
                });
            ui.add_space(6.0);
            dialog_actions(ui, |ui| {
                if widgets::button(ui, &th, "Close", true).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.dialog = None;
        }
    }

    fn discard_dialog(&mut self, ctx: &Context, th: crate::theme::Theme, pending: Pending) {
        let mut close = false;
        let mut discard = false;
        let mut save_first = false;
        widgets::modal(ctx, &th, "discard", Vec2::new(430.0, 200.0), |ui| {
            if widgets::modal_header(ui, &th, "Unsaved changes") {
                close = true;
            }
            ui.label(
                egui::RichText::new(format!(
                    "“{}” has unsaved changes.",
                    self.doc.display_name()
                ))
                .size(12.5)
                .color(th.text),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Do you want to continue without saving?")
                    .size(11.5)
                    .color(th.text_dim),
            );
            ui.add_space(14.0);
            dialog_actions(ui, |ui| {
                if widgets::button(ui, &th, "Discard", true).clicked() {
                    discard = true;
                }
                if widgets::button(ui, &th, "Save first", false).clicked() {
                    save_first = true;
                }
                if widgets::button(ui, &th, "Cancel", false).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.dialog = None;
        } else if save_first {
            self.dialog = None;
            self.pending_after_save = Some(pending);
            self.save_dialog(ctx);
        } else if discard {
            self.dialog = None;
            self.resume_pending(pending, ctx);
        }
    }
}

fn dialog_actions(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
}
