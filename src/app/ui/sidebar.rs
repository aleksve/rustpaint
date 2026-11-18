//! Left tool dock and right options / colour panel.

use eframe::egui::{self, Color32, FontId, Sense, Stroke, StrokeKind, Vec2};

use crate::app::{App, Btn};
use crate::icons::Icon;
use crate::tools::{PALETTE, Tool, parse_hex, to_hex};
use crate::widgets::{self, rr};

impl App {
    pub(crate) fn tool_dock(&mut self, ui: &mut egui::Ui) {
        let th = self.theme;
        egui::Panel::left("tools")
            .exact_size(56.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .inner_margin(egui::Margin::symmetric(9, 10))
                    .stroke(Stroke::new(1.0_f32, th.border)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let mut select = None;
                        for (index, tool) in Tool::ALL.iter().copied().enumerate() {
                            if matches!(index, 1 | 6 | 8 | 12) {
                                widgets::divider(ui, &th);
                            }
                            let tip = format!("{}  ({})", tool.label(), tool.shortcut());
                            if widgets::icon_button(ui, &th, tool.icon(), 38.0, self.tool == tool)
                                .on_hover_text(tip)
                                .clicked()
                            {
                                select = Some(tool);
                            }
                        }
                        if let Some(tool) = select {
                            self.set_tool(tool);
                        }
                    });
            });
    }

    pub(crate) fn right_panel(&mut self, ui: &mut egui::Ui) {
        let th = self.theme;
        egui::Panel::right("right")
            .exact_size(278.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .inner_margin(egui::Margin::symmetric(12, 12))
                    .stroke(Stroke::new(1.0_f32, th.border)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        self.tool_options_card(ui, th);
                        self.colors_card(ui, th);
                    });
            });
    }

    fn tool_options_card(&mut self, ui: &mut egui::Ui, th: crate::theme::Theme) {
        let tool = self.tool;
        widgets::card(ui, &th, |ui| {
            widgets::section(ui, &th, &format!("{} options", tool.label()));
            ui.spacing_mut().item_spacing.y = 8.0;

            if tool.uses_size() {
                let max = tool.max_size();
                if widgets::slider(ui, &th, "Size (px)", &mut self.opts.size, 1.0..=max, 0) {
                    self.tool_sizes.insert(tool, self.opts.size);
                }
            }
            if tool.uses_softness() {
                widgets::slider_pct(ui, &th, "Hardness", &mut self.opts.hardness);
            }
            if tool.uses_opacity()
                && widgets::slider_pct(ui, &th, "Opacity", &mut self.opts.opacity)
            {
                self.sync_live_text_color_and_opacity();
            }
            if tool == Tool::Fill {
                let mut tol = self.opts.tolerance as f32;
                if widgets::slider(ui, &th, "Tolerance", &mut tol, 0.0..=255.0, 0) {
                    self.opts.tolerance = tol.round().clamp(0.0, 255.0) as u8;
                }
            }
            if tool.uses_fill_toggle() {
                widgets::toggle(ui, &th, &mut self.opts.filled, "Fill interior");
            }
            if tool.uses_antialias() {
                widgets::toggle(ui, &th, &mut self.opts.antialias, "Smooth edges");
            }

            if tool == Tool::Text {
                self.text_options(ui, th);
            }

            if tool == Tool::Select {
                ui.label(
                    egui::RichText::new(
                        "Drag to select · Shift for square · drag inside to move · arrows nudge",
                    )
                    .size(11.0)
                    .color(th.text_dim),
                );
                if self.edit.has_selection() {
                    ui.horizontal(|ui| {
                        if widgets::button(ui, &th, "Crop", false).clicked() {
                            self.crop_to_selection();
                        }
                        if widgets::button(ui, &th, "Clear", false).clicked() {
                            self.delete_selection();
                        }
                        if widgets::button(ui, &th, "Done", false).clicked() {
                            self.deselect();
                        }
                    });
                }
            }

            if tool == Tool::Polygon {
                ui.label(
                    egui::RichText::new(
                        "Click to add corners · double-click, right-click or Enter to close",
                    )
                    .size(11.0)
                    .color(th.text_dim),
                );
                if self.edit.is_drawing_polygon()
                    && widgets::button(ui, &th, "Finish shape", true).clicked()
                {
                    self.finish_polygon();
                }
            }

            if matches!(tool, Tool::Picker | Tool::Pan) {
                let message = if tool == Tool::Picker {
                    "Left-click sets the primary colour, right-click sets the secondary. Hold Alt to sample with any tool."
                } else {
                    "Drag to pan. Hold Space anytime to pan without switching tools; Ctrl + scroll zooms at the cursor."
                };
                ui.label(egui::RichText::new(message).size(11.0).color(th.text_dim));
            }
        });
    }

    fn text_options(&mut self, ui: &mut egui::Ui, th: crate::theme::Theme) {
        let mut restyle = false;

        ui.label(egui::RichText::new("Font").size(11.5).color(th.text_dim));
        let loading_fonts = self.fonts.is_loading();
        let current_label = self.opts.font_family.clone();
        let combo = widgets::drop_field(ui, &th, &current_label);
        let mut chosen: Option<String> = None;
        egui::Popup::from_toggle_button_response(&combo)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(236.0);
                ui.add_sized(
                    Vec2::new(ui.available_width(), 24.0),
                    egui::TextEdit::singleline(&mut self.font_filter).hint_text("Filter fonts…"),
                );
                let query = self.font_filter.trim().to_lowercase();
                ui.add_space(4.0);
                if loading_fonts {
                    ui.label(
                        egui::RichText::new("Scanning system fonts…")
                            .size(10.5)
                            .color(th.text_dim),
                    );
                    ui.separator();
                }
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for family in self.fonts.families() {
                            if !query.is_empty() && !family.to_lowercase().contains(&query) {
                                continue;
                            }
                            let selected = family == &self.opts.font_family;
                            if ui.selectable_label(selected, family).clicked() {
                                chosen = Some(family.clone());
                                ui.close();
                            }
                        }
                    });
            });
        if let Some(family) = chosen {
            self.opts.font_family = family.clone();
            self.prefs.font_family = Some(family);
            self.prefs.save();
            restyle = true;
        }

        if widgets::slider(
            ui,
            &th,
            "Font size (px)",
            &mut self.opts.font_size,
            8.0..=200.0,
            0,
        ) {
            restyle = true;
        }
        if widgets::toggle(ui, &th, &mut self.opts.bold, "Bold").clicked() {
            restyle = true;
        }

        if restyle {
            let (family, size, bold) = (
                self.opts.font_family.clone(),
                self.opts.font_size,
                self.opts.bold,
            );
            if let Some(session) = self.text_session_mut() {
                session.size = size;
                session.bold = bold;
                session.family = family;
                self.refresh_text();
            }
        }

        if self.edit.is_editing_text() {
            ui.horizontal(|ui| {
                if widgets::button(ui, &th, "Apply text", true).clicked() {
                    self.commit_current();
                }
                if widgets::button(ui, &th, "Cancel", false).clicked() {
                    self.cancel_current();
                }
            });
        }
        ui.label(
            egui::RichText::new(
                "Click canvas to type · drag handle or letters to move · Ctrl+Enter to finish",
            )
            .size(10.5)
            .color(th.text_dim),
        );
    }

    fn colors_card(&mut self, ui: &mut egui::Ui, th: crate::theme::Theme) {
        widgets::card(ui, &th, |ui| {
            widgets::section(ui, &th, "Colour");
            ui.spacing_mut().item_spacing.y = 8.0;

            // Active swatch pair + swap / reset.
            ui.horizontal(|ui| {
                let (pair_rect, _) = ui.allocate_exact_size(Vec2::new(62.0, 44.0), Sense::hover());
                let primary_rect = egui::Rect::from_min_size(pair_rect.min, Vec2::new(34.0, 30.0));
                let secondary_rect = egui::Rect::from_min_size(
                    pair_rect.min + Vec2::new(22.0, 12.0),
                    Vec2::new(34.0, 30.0),
                );
                let secondary_click = ui.interact(
                    secondary_rect,
                    ui.make_persistent_id("sec_sw"),
                    Sense::click(),
                );
                let primary_click = ui.interact(
                    primary_rect,
                    ui.make_persistent_id("pri_sw"),
                    Sense::click(),
                );
                if primary_click.clicked() {
                    self.color_target = Btn::Primary;
                    self.picked_color = Some(self.colors.primary);
                } else if secondary_click.clicked() {
                    self.color_target = Btn::Secondary;
                    self.picked_color = Some(self.colors.secondary);
                }

                // Draw inactive swatch first so the active swatch always sits on top
                // with an unmistakable accent ring (and checkerboard backing for alpha).
                let draw_swatch = |p: &egui::Painter, r: egui::Rect, c: Color32, active: bool| {
                    widgets::paint_color_chip(p, &th, r, rr(7), c);
                    p.rect_stroke(
                        r,
                        rr(7),
                        Stroke::new(
                            if active { 2.0 } else { 1.2 },
                            if active { th.accent } else { th.border },
                        ),
                        StrokeKind::Outside,
                    );
                };
                if self.color_target == Btn::Secondary {
                    draw_swatch(ui.painter(), primary_rect, self.colors.primary, false);
                    draw_swatch(ui.painter(), secondary_rect, self.colors.secondary, true);
                } else {
                    draw_swatch(ui.painter(), secondary_rect, self.colors.secondary, false);
                    draw_swatch(ui.painter(), primary_rect, self.colors.primary, true);
                }
                primary_click.on_hover_text("Primary colour (left-click)");
                secondary_click.on_hover_text("Secondary colour (right-click)");

                ui.add_space(4.0);
                if widgets::icon_button(ui, &th, Icon::Swap, 28.0, false)
                    .on_hover_text("Swap primary & secondary (X)")
                    .clicked()
                {
                    self.colors.swap();
                    self.picked_color = Some(self.active_color(self.color_target));
                    self.sync_live_text_color_and_opacity();
                }
                if widgets::button(ui, &th, "B/W", false)
                    .on_hover_text("Reset to black & white (D)")
                    .clicked()
                {
                    self.colors.reset();
                    self.picked_color = Some(self.active_color(self.color_target));
                    self.sync_live_text_color_and_opacity();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active_label = match self.color_target {
                        Btn::Primary => "Primary",
                        Btn::Secondary => "Secondary",
                    };
                    ui.label(
                        egui::RichText::new(active_label)
                            .size(11.0)
                            .color(th.text_dim),
                    );
                });
            });

            // Colour picker updates whichever swatch is active; hue/saturation are
            // remembered across black/white/grey via `self.picker_hsv`.
            let mut active = self.active_color(self.color_target);
            if widgets::color_picker(ui, &th, &mut active, &mut self.picker_hsv) {
                self.set_active_color(self.color_target, active);
                self.hex_edit = to_hex(active);
                self.sync_live_text_color_and_opacity();
            }

            // Hex readout / entry.
            if !self.hex_focused {
                let target_hex = to_hex(active);
                if self.hex_edit != target_hex {
                    self.hex_edit = target_hex;
                }
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Hex").size(11.0).color(th.text_dim));
                let edit = ui.add_sized(
                    Vec2::new(ui.available_width(), 24.0),
                    egui::TextEdit::singleline(&mut self.hex_edit)
                        .font(FontId::monospace(12.0))
                        .align(egui::Align2::RIGHT_CENTER),
                );
                self.hex_focused = edit.has_focus();
                if edit.changed()
                    && let Some(parsed) = parse_hex(&self.hex_edit)
                {
                    self.set_active_color(self.color_target, parsed);
                    self.sync_live_text_color_and_opacity();
                }
                if edit.lost_focus() {
                    let cur = self.active_color(self.color_target);
                    self.hex_edit = to_hex(cur);
                    self.colors.push_recent(cur);
                }
            });

            // Palette grid (zero per-frame allocation).
            ui.add_space(2.0);
            widgets::section(ui, &th, "Palette");
            let mut chosen: Option<(Btn, Color32)> = None;
            egui::Grid::new("palette")
                .spacing(Vec2::splat(4.0))
                .show(ui, |ui| {
                    for (index, &color) in PALETTE.iter().enumerate() {
                        let response = widgets::swatch(ui, &th, color, 19.0, color == active)
                            .on_hover_text(to_hex(color));
                        if response.clicked() {
                            chosen = Some((self.color_target, color));
                        } else if response.secondary_clicked() {
                            chosen = Some((Btn::Secondary, color));
                        }
                        if index % 10 == 9 {
                            ui.end_row();
                        }
                    }
                });

            if !self.colors.recent.is_empty() {
                ui.add_space(2.0);
                widgets::section(ui, &th, "Recent");
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(4.0);
                    for &color in &self.colors.recent {
                        let response = widgets::swatch(ui, &th, color, 18.0, color == active)
                            .on_hover_text(to_hex(color));
                        if response.clicked() {
                            chosen = Some((self.color_target, color));
                        } else if response.secondary_clicked() {
                            chosen = Some((Btn::Secondary, color));
                        }
                    }
                });
            }

            if let Some((target, color)) = chosen {
                self.set_active_color(target, color);
                self.colors.push_recent(color);
                self.sync_live_text_color_and_opacity();
            }
        });
    }
}
