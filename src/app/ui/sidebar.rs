//! Tool rail and document/tool properties panel.

use eframe::egui::{self, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

use crate::app::{App, Btn, elide};
use crate::icons::Icon;
use crate::tools::{PALETTE, Tool, parse_hex, to_hex};
use crate::widgets::{self, rr};

pub(crate) const TOOL_RAIL_WIDTH: f32 = 62.0;
pub(crate) const SIDE_PANEL_WIDTH: f32 = 286.0;
const SIDE_PAD_X: i8 = 14;
const SIDE_PAD_Y: i8 = 10;

impl App {
    // ------------------------------------------------------------- left rail

    pub(crate) fn tool_rail(&mut self, ui: &mut Ui) -> Rect {
        let th = self.theme;
        egui::Panel::left("rail")
            .exact_size(TOOL_RAIL_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(0, 8)),
            )
            .show(ui, |ui| {
                let body = ui.max_rect();
                let mut col = ui.new_child(UiBuilder::new().max_rect(body).layout(*ui.layout()));
                col.set_clip_rect(body.intersect(ui.clip_rect()));
                // Shrink icons slightly on short windows so all 13 tools fit
                // comfortably; fall back to scrolling only on extreme heights.
                let n = Tool::ALL.len() as f32;
                let gap = 4.0;
                let btn = (((body.height() - 16.0) / n) - gap).clamp(28.0, 44.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(&mut col, |ui| {
                        ui.set_width(body.width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.vertical_centered(|ui| {
                            for tool in Tool::ALL {
                                ui.add_space(2.0);
                                let active = self.tool == tool;
                                let resp = widgets::icon_button(ui, &th, tool.icon(), btn, active);
                                if resp.clicked() {
                                    self.set_tool(tool);
                                }
                                resp.on_hover_text(format!(
                                    "{}  ({})",
                                    tool.label(),
                                    tool.shortcut()
                                ));
                            }
                        });
                    });
                ui.allocate_rect(body, Sense::hover());
            })
            .response
            .rect
    }

    // ------------------------------------------------------------ right panel

    pub(crate) fn side_panel(&mut self, ui: &mut Ui) -> Rect {
        let th = self.theme;
        egui::Panel::right("props")
            .exact_size(SIDE_PANEL_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ui, |ui| {
                // Render into a strictly bounded child `Ui` and allocate exactly
                // `body` on `ui` so `egui 0.37`'s `Panel::right` never sees child
                // overflow and can never shift the central panel's right edge.
                // Padding lives inside the `ScrollArea` so outside focus/active
                // rings and edge colour-picker knobs are never clipped and the
                // floating scrollbar sits cleanly in the outer gutter.
                let body = ui.max_rect();
                let mut col = ui.new_child(UiBuilder::new().max_rect(body).layout(*ui.layout()));
                col.set_clip_rect(body.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(&mut col, |ui| {
                        let content_w = (body.width() - 2.0 * f32::from(SIDE_PAD_X)).max(64.0);
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(SIDE_PAD_X, SIDE_PAD_Y))
                            .show(ui, |ui| {
                                ui.set_width(content_w);
                                ui.set_max_width(content_w);
                                self.props_ui(ui);
                            });
                    });
                ui.allocate_rect(body, Sense::hover());
            })
            .response
            .rect
    }

    fn props_ui(&mut self, ui: &mut Ui) {
        let th = self.theme;
        widgets::section(ui, &th, "Colour");

        ui.horizontal(|ui| {
            // Overlapping primary / secondary swatches with alpha checkerboard
            // and an accent ring on whichever swatch is currently selected.
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(72.0, 52.0), Sense::click());
            let front = Rect::from_min_size(rect.min, Vec2::splat(34.0));
            let back = Rect::from_min_size(rect.min + Vec2::new(22.0, 18.0), Vec2::splat(34.0));
            if resp.clicked() {
                let pos = resp.interact_pointer_pos().unwrap_or(rect.center());
                self.color_target = if back.contains(pos) && !front.contains(pos) {
                    Btn::Secondary
                } else {
                    Btn::Primary
                };
            }
            let p = ui.painter();
            let draw_chip = |r: Rect, c: Color32, active: bool| {
                widgets::paint_color_chip(p, &th, r, rr(6), c);
                let (w, ring) = if active {
                    (1.8_f32, th.accent)
                } else {
                    (1.0_f32, th.border)
                };
                p.rect_stroke(r, rr(6), Stroke::new(w, ring), StrokeKind::Outside);
            };
            draw_chip(
                back,
                self.colors.secondary,
                self.color_target == Btn::Secondary,
            );
            draw_chip(
                front,
                self.colors.primary,
                self.color_target == Btn::Primary,
            );
            resp.on_hover_text("Click front for primary, back for secondary");

            ui.vertical(|ui| {
                ui.add_space(2.0);
                let hex = to_hex(self.active_color(self.color_target));
                ui.label(
                    egui::RichText::new(if self.color_target == Btn::Primary {
                        "Primary"
                    } else {
                        "Secondary"
                    })
                    .size(10.5)
                    .color(th.text_dim),
                );
                ui.label(egui::RichText::new(hex).size(12.0).color(th.text));
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, &th, Icon::Swap, 22.0, false)
                        .on_hover_text("Swap colours (X)")
                        .clicked()
                    {
                        self.colors.swap();
                        self.sync_live_text_color_and_opacity();
                    }
                    if widgets::icon_button(ui, &th, Icon::Close, 22.0, false)
                        .on_hover_text("Reset to black / white (D)")
                        .clicked()
                    {
                        self.colors.reset();
                        self.sync_live_text_color_and_opacity();
                    }
                });
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Hex").size(11.0).color(th.text_dim));
            if !self.hex_focused {
                let current = to_hex(self.active_color(self.color_target));
                if self.hex_edit != current {
                    self.hex_edit = current;
                }
            }
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.hex_edit)
                    .desired_width(94.0)
                    .font(FontId::monospace(12.0)),
            );
            self.hex_focused = resp.has_focus();
            if resp.changed()
                && let Some(c) = parse_hex(&self.hex_edit)
            {
                self.set_active_color(self.color_target, c);
                self.sync_live_text_color_and_opacity();
            }
            if resp.lost_focus() {
                let c = self.active_color(self.color_target);
                self.hex_edit = to_hex(c);
                self.colors.push_recent(c);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{}%",
                    (self.active_color(self.color_target).a() as f32 / 2.55).round() as u8
                ))
                .size(10.5)
                .color(th.text_dim),
            );
        });

        ui.add_space(6.0);

        // Static 3×10 palette grid (left-click sets active slot, right-click sets secondary).
        let active_col = self.active_color(self.color_target);
        let columns = 10;
        let gap = 4.0;
        let full_w = ui.available_width();
        let swatch_size =
            ((full_w - (columns - 1) as f32 * gap) / columns as f32).clamp(14.0, 24.0);
        let mut chosen: Option<(Btn, Color32)> = None;
        for row in PALETTE.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &color in row {
                    let resp = widgets::swatch(ui, &th, color, swatch_size, color == active_col);
                    if resp.clicked() {
                        chosen = Some((self.color_target, color));
                    } else if resp.secondary_clicked() {
                        chosen = Some((Btn::Secondary, color));
                    }
                }
            });
            ui.add_space(2.0);
        }

        if !self.colors.recent.is_empty() {
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Recent").size(10.0).color(th.text_dim));
            let recent_cols = 13;
            let recent_size =
                ((full_w - (recent_cols - 1) as f32 * gap) / recent_cols as f32).clamp(12.0, 18.0);
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = gap;
                for row in self.colors.recent.chunks(recent_cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for &color in row {
                            let resp = widgets::swatch(ui, &th, color, recent_size, false);
                            if resp.clicked() {
                                chosen = Some((self.color_target, color));
                            } else if resp.secondary_clicked() {
                                chosen = Some((Btn::Secondary, color));
                            }
                        }
                    });
                }
            });
        }

        if let Some((target, color)) = chosen {
            self.set_active_color(target, color);
            self.colors.push_recent(color);
            self.sync_live_text_color_and_opacity();
        }

        ui.add_space(6.0);
        let mut edit = self.active_color(self.color_target);
        if widgets::color_picker(ui, &th, &mut edit, &mut self.picker_hsv) {
            self.set_active_color(self.color_target, edit);
            self.sync_live_text_color_and_opacity();
        }

        widgets::divider(ui, &th);
        widgets::section(ui, &th, "Tool");
        ui.label(
            egui::RichText::new(self.tool.label())
                .size(13.0)
                .strong()
                .color(th.text),
        );
        ui.label(
            egui::RichText::new(self.tool.hint())
                .size(10.5)
                .color(th.text_dim),
        );
        ui.add_space(4.0);

        let tool = self.tool;
        if tool != Tool::Text {
            self.font_filter_focused = false;
        }
        match tool {
            Tool::Select => {
                ui.horizontal(|ui| {
                    if widgets::button(ui, &th, "Select all", false).clicked() {
                        self.select_all();
                    }
                    if widgets::button_styled(
                        ui,
                        &th,
                        "Crop",
                        widgets::ButtonKind::Secondary,
                        self.edit.has_selection(),
                    )
                    .clicked()
                    {
                        self.crop_to_selection();
                    }
                });
            }
            Tool::Text => self.text_tool_ui(ui, th),
            Tool::Fill => {
                let mut tolerance = self.opts.tolerance as f32;
                if widgets::slider(ui, &th, "Tolerance", &mut tolerance, 0.0..=255.0, 0) {
                    self.opts.tolerance = tolerance.round().clamp(0.0, 255.0) as u8;
                }
            }
            Tool::Polygon
                if self.edit.is_drawing_polygon()
                    && widgets::button(ui, &th, "Finish shape", true).clicked() =>
            {
                self.finish_polygon();
            }
            _ => {}
        }

        if tool.uses_size() {
            let max = tool.max_size();
            if widgets::slider(ui, &th, "Size", &mut self.opts.size, 1.0..=max, 0) {
                self.tool_sizes.insert(tool, self.opts.size);
            }
        }
        if tool.uses_softness() {
            widgets::slider(ui, &th, "Hardness", &mut self.opts.hardness, 0.0..=1.0, 2);
        }
        if tool.uses_opacity()
            && widgets::slider(ui, &th, "Opacity", &mut self.opts.opacity, 0.02..=1.0, 2)
        {
            self.sync_live_text_color_and_opacity();
        }
        if tool.uses_fill_toggle() {
            widgets::toggle(ui, &th, &mut self.opts.filled, "Filled shape");
        }
        if tool.uses_antialias() {
            widgets::toggle(ui, &th, &mut self.opts.antialias, "Anti-alias");
        }

        widgets::divider(ui, &th);
        widgets::section(ui, &th, "Canvas");
        widgets::toggle(ui, &th, &mut self.view.grid, "Pixel grid");
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "{} × {} px   ·   undo {}/{}",
                self.doc.w,
                self.doc.h,
                self.hist.undo_len(),
                self.hist.redo_len()
            ))
            .size(10.5)
            .color(th.text_dim),
        );
        ui.label(
            egui::RichText::new(format!("{:.0}% zoom", self.view.zoom * 100.0))
                .size(10.5)
                .color(th.text_dim),
        );
        ui.add_space(10.0);
    }

    fn text_tool_ui(&mut self, ui: &mut Ui, th: crate::theme::Theme) {
        ui.label(egui::RichText::new("Font").size(10.0).color(th.text_dim));
        let label = elide(&self.opts.font_family, 28);
        let resp = widgets::drop_field(ui, &th, &label);
        let popup_w = resp.rect.width().max(220.0);
        let just_opened = resp.clicked();
        let loading = self.fonts.is_loading();
        let count = self.fonts.families().len();
        let current = self.opts.font_family.clone();
        let mut picked: Option<String> = None;
        let mut filter_focused = false;

        egui::Popup::from_toggle_button_response(&resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(popup_w);
                ui.set_max_width(popup_w);
                let hint = if loading {
                    "Scanning system fonts…".to_string()
                } else {
                    format!("Search {count} fonts…")
                };
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.font_filter)
                        .hint_text(hint)
                        .desired_width((popup_w - 8.0).max(140.0)),
                );
                if just_opened {
                    edit.request_focus();
                }
                filter_focused = edit.has_focus();
                ui.add_space(2.0);
                let needle = self.font_filter.trim().to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for family in self.fonts.families() {
                            if !needle.is_empty() && !family.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui.selectable_label(family == &current, family).clicked() {
                                picked = Some(family.clone());
                                ui.close();
                            }
                        }
                    });
            });

        self.font_filter_focused = filter_focused && picked.is_none();

        if let Some(family) = picked {
            self.opts.font_family = family.clone();
            self.font_filter.clear();
            self.prefs.font_family = Some(family);
            self.prefs.save();
        }

        widgets::slider(
            ui,
            &th,
            "Font size",
            &mut self.opts.font_size,
            6.0..=200.0,
            0,
        );
        widgets::toggle(ui, &th, &mut self.opts.bold, "Bold");

        let (want_size, want_bold, want_family) = (
            self.opts.font_size,
            self.opts.bold,
            self.opts.font_family.clone(),
        );
        let mut changed = false;
        if let Some(text) = self.text_session_mut() {
            if (text.size - want_size).abs() > 0.01 {
                text.size = want_size;
                changed = true;
            }
            if text.bold != want_bold {
                text.bold = want_bold;
                changed = true;
            }
            if text.family != want_family {
                text.family = want_family;
                changed = true;
            }
        }
        if changed {
            self.refresh_text();
        }

        if self.edit.is_editing_text() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if widgets::button(ui, &th, "Apply", true).clicked() {
                    self.commit_current();
                }
                if widgets::button(ui, &th, "Cancel", false).clicked() {
                    self.cancel_current();
                }
            });
        }
    }
}
