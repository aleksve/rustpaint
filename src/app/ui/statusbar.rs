//! Bottom status bar.

use eframe::egui::{self, Rect, Sense, Stroke, Ui, UiBuilder};

use crate::app::App;
use crate::icons::Icon;
use crate::widgets;

pub(crate) const STATUS_BAR_HEIGHT: f32 = 26.0;

impl App {
    pub(crate) fn status_bar(&mut self, ui: &mut Ui) -> Rect {
        let th = self.theme;
        egui::Panel::bottom("status")
            .exact_size(STATUS_BAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                let body = ui.max_rect();
                let mut row = ui.new_child(UiBuilder::new().max_rect(body).layout(*ui.layout()));
                row.set_clip_rect(body.intersect(ui.clip_rect()));
                row.horizontal_centered(|ui| {
                    let position = match self.cursor_img {
                        Some((x, y))
                            if x >= 0
                                && y >= 0
                                && (x as usize) < self.doc.w
                                && (y as usize) < self.doc.h =>
                        {
                            format!("{x}, {y} px")
                        }
                        _ => "—".to_string(),
                    };
                    ui.label(egui::RichText::new(position).size(11.0).color(th.text_dim));
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("{} × {}", self.doc.w, self.doc.h))
                            .size(11.0)
                            .color(th.text_dim),
                    );
                    let right_reserve = if self.edit.selection.is_some_and(|s| !s.is_empty()) {
                        210.0
                    } else {
                        116.0
                    };
                    let hint_budget = ui.available_width() - right_reserve;
                    if hint_budget > 60.0 {
                        ui.add_space(12.0);
                        let max_chars = ((hint_budget / 6.2) as usize).max(8);
                        ui.label(
                            egui::RichText::new(super::super::elide(self.tool.hint(), max_chars))
                                .size(11.0)
                                .color(th.text_dim),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let view_rect = self.last_view_rect;
                        if widgets::icon_button(ui, &th, Icon::ZoomIn, 20.0, false)
                            .on_hover_text(if cfg!(target_os = "macos") {
                                "Zoom in (⌘+)"
                            } else {
                                "Zoom in (Ctrl +)"
                            })
                            .clicked()
                        {
                            self.zoom_step(1.0, view_rect);
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{:.0}%", self.view.zoom * 100.0))
                                        .size(11.0)
                                        .color(th.text),
                                )
                                .frame(false),
                            )
                            .on_hover_text(if cfg!(target_os = "macos") {
                                "Actual size (⌘0)"
                            } else {
                                "Actual size (Ctrl 0)"
                            })
                            .clicked()
                        {
                            self.set_zoom(1.0, view_rect);
                        }
                        if widgets::icon_button(ui, &th, Icon::ZoomOut, 20.0, false)
                            .on_hover_text(if cfg!(target_os = "macos") {
                                "Zoom out (⌘-)"
                            } else {
                                "Zoom out (Ctrl -)"
                            })
                            .clicked()
                        {
                            self.zoom_step(-1.0, view_rect);
                        }
                        if let Some(selection) = self.edit.selection
                            && !selection.is_empty()
                        {
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "sel {} × {}",
                                    selection.w(),
                                    selection.h()
                                ))
                                .size(11.0)
                                .color(th.accent),
                            );
                        }
                    });
                });
                ui.allocate_rect(body, Sense::hover());
            })
            .response
            .rect
    }
}
