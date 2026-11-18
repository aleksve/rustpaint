//! Bottom status bar.

use eframe::egui::{self, Stroke};

use crate::app::App;
use crate::icons::Icon;
use crate::widgets;

impl App {
    pub(crate) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let th = self.theme;
        egui::Panel::bottom("status")
            .exact_size(28.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .stroke(Stroke::new(1.0_f32, th.border)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    let doc_label = format!(
                        "{} × {} px{}",
                        self.doc.w,
                        self.doc.h,
                        if self.doc.modified {
                            "  •  unsaved"
                        } else {
                            ""
                        }
                    );
                    ui.label(egui::RichText::new(doc_label).size(11.5).color(th.text_dim));

                    if let Some((x, y)) = self.cursor_img
                        && x >= 0
                        && y >= 0
                        && (x as usize) < self.doc.w
                        && (y as usize) < self.doc.h
                    {
                        ui.label(
                            egui::RichText::new(format!("x {x}, y {y}"))
                                .size(11.5)
                                .color(th.text),
                        );
                    }

                    if let Some(selection) = self.edit.selection
                        && !selection.is_empty()
                    {
                        ui.label(
                            egui::RichText::new(format!(
                                "sel {} × {}",
                                selection.w(),
                                selection.h()
                            ))
                            .size(11.5)
                            .color(th.accent),
                        );
                    }

                    ui.label(
                        egui::RichText::new(self.tool.hint())
                            .size(11.0)
                            .color(th.text_dim),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let rect = self.last_view_rect;
                        if widgets::icon_button(ui, &th, Icon::ZoomIn, 22.0, false)
                            .on_hover_text("Zoom in (Ctrl +)")
                            .clicked()
                        {
                            self.zoom_step(1.0, rect);
                        }
                        let zoom_label = format!("{:.0}%", self.view.zoom * 100.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(zoom_label).size(11.5).color(th.text),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Click for 100% (Ctrl+0)")
                            .clicked()
                        {
                            self.set_zoom(1.0, rect);
                        }
                        if widgets::icon_button(ui, &th, Icon::ZoomOut, 22.0, false)
                            .on_hover_text("Zoom out (Ctrl -)")
                            .clicked()
                        {
                            self.zoom_step(-1.0, rect);
                        }
                        let memory_mb = self.hist.total_bytes() as f32 / (1024.0 * 1024.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} undo · {memory_mb:.1} MB",
                                self.hist.undo_len()
                            ))
                            .size(11.0)
                            .color(th.text_dim),
                        );
                    });
                });
            });
    }
}
