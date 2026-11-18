//! Bottom status bar.

use eframe::egui::{self, Context, Stroke};

use super::super::App;
use crate::icons::Icon;
use crate::widgets;

impl App {
    pub(crate) fn status_bar(&mut self, ctx: &Context) {
        let th = self.theme;
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let position = match self.cursor_img {
                        Some((x, y)) => format!("{x}, {y} px"),
                        None => "—".to_string(),
                    };
                    ui.label(egui::RichText::new(position).size(11.0).color(th.text_dim));
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("{} × {}", self.doc.w, self.doc.h))
                            .size(11.0)
                            .color(th.text_dim),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(self.tool.hint())
                            .size(11.0)
                            .color(th.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let view_rect = self.last_view_rect;
                        if widgets::icon_button(ui, &th, Icon::ZoomIn, 20.0, false)
                            .on_hover_text("Zoom in (Ctrl +)")
                            .clicked()
                        {
                            self.zoom_step(1.0, view_rect);
                        }
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", self.view.zoom * 100.0))
                                .size(11.0)
                                .color(th.text),
                        );
                        if widgets::icon_button(ui, &th, Icon::ZoomOut, 20.0, false)
                            .on_hover_text("Zoom out (Ctrl -)")
                            .clicked()
                        {
                            self.zoom_step(-1.0, view_rect);
                        }
                        if let Some(selection) = self.edit.selection {
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "sel {} × {}",
                                    selection.w().max(0),
                                    selection.h().max(0)
                                ))
                                .size(11.0)
                                .color(th.accent),
                            );
                        }
                    });
                });
            });
    }
}
