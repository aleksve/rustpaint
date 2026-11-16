//! Tool rail and document/tool properties panel.

use eframe::egui::{self, Color32, Context, FontId, Rect, Sense, Stroke, Ui, Vec2};

use super::super::{elide, App, Btn};
use crate::icons::Icon;
use crate::tools::{self, Tool};
use crate::widgets;

impl App {
    // ------------------------------------------------------------- left rail

    pub(crate) fn tool_rail(&mut self, ctx: &Context) {
        let th = self.theme;
        egui::SidePanel::left("rail")
            .exact_width(62.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin {
                        left: 0.0,
                        right: 0.0,
                        top: 8.0,
                        bottom: 8.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    // Shrink the swatches on short windows so every tool stays
                    // reachable without scrolling; scroll only as a last resort.
                    let n = Tool::ALL.len() as f32;
                    let gap = 4.0;
                    let btn = (((ui.available_height() - 16.0) / n) - gap).clamp(28.0, 44.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.vertical_centered(|ui| {
                                for t in Tool::ALL {
                                    ui.add_space(2.0);
                                    let active = self.tool == t;
                                    let resp = widgets::icon_button(ui, &th, t.icon(), btn, active);
                                    if resp.clicked() {
                                        self.set_tool(t);
                                    }
                                    resp.on_hover_text(format!(
                                        "{}  ({})",
                                        t.label(),
                                        t.shortcut()
                                    ));
                                }
                            });
                        });
                });
            });
    }

    // ------------------------------------------------------------ right panel

    pub(crate) fn side_panel(&mut self, ctx: &Context) {
        let th = self.theme;
        egui::SidePanel::right("props")
            .exact_width(258.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(12.0, 10.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.props_ui(ui));
            });
    }

    pub(crate) fn props_ui(&mut self, ui: &mut Ui) {
        let th = self.theme;
        widgets::section(ui, &th, "Colour");

        ui.horizontal(|ui| {
            // Overlapping primary / secondary swatches.
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(78.0, 52.0), Sense::click());
            let front = Rect::from_min_size(rect.min, Vec2::splat(34.0));
            let back = Rect::from_min_size(rect.min + Vec2::new(22.0, 18.0), Vec2::splat(34.0));
            let p = ui.painter();
            p.rect_filled(back, widgets::rr(6.0), self.colors.secondary);
            p.rect_stroke(back, widgets::rr(6.0), Stroke::new(1.0_f32, th.border));
            p.rect_filled(front, widgets::rr(6.0), self.colors.primary);
            let ring = if self.color_target == Btn::Primary {
                th.accent
            } else {
                th.border
            };
            p.rect_stroke(front, widgets::rr(6.0), Stroke::new(1.6_f32, ring));
            if resp.clicked() {
                let pos = resp.interact_pointer_pos().unwrap_or(rect.center());
                self.color_target = if back.contains(pos) && !front.contains(pos) {
                    Btn::Secondary
                } else {
                    Btn::Primary
                };
            }
            ui.vertical(|ui| {
                ui.add_space(2.0);
                let hex = tools::to_hex(self.active_color(self.color_target));
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
                    }
                    if widgets::icon_button(ui, &th, Icon::Close, 22.0, false)
                        .on_hover_text("Reset to black / white (D)")
                        .clicked()
                    {
                        self.colors.primary = Color32::from_rgb(0x1a, 0x1c, 0x22);
                        self.colors.secondary = Color32::WHITE;
                    }
                });
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Hex").size(11.0).color(th.text_dim));
            if !self.hex_edit_active {
                self.hex_buf = tools::to_hex(self.active_color(self.color_target));
            }
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.hex_buf)
                    .desired_width(78.0)
                    .font(FontId::monospace(12.0)),
            );
            self.hex_edit_active = resp.has_focus();
            if resp.changed() {
                if let Some(c) = tools::parse_hex(&self.hex_buf) {
                    self.set_active_color(self.color_target, c);
                }
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

        let palette = tools::palette();
        let columns = 10;
        let swatch_size = ((ui.available_width() - 9.0 * 4.0) / columns as f32).clamp(14.0, 22.0);
        for row in palette.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for color in row {
                    let selected = *color == self.active_color(self.color_target);
                    if widgets::swatch(ui, &th, *color, swatch_size, selected).clicked() {
                        self.set_active_color(self.color_target, *color);
                    }
                }
            });
            ui.add_space(2.0);
        }

        ui.add_space(2.0);
        ui.label(egui::RichText::new("Recent").size(10.0).color(th.text_dim));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for color in self.colors.recent.clone().iter().take(16) {
                if widgets::swatch(ui, &th, *color, 16.0, false).clicked() {
                    self.set_active_color(self.color_target, *color);
                }
            }
        });

        ui.add_space(6.0);
        let mut edit = self.active_color(self.color_target);
        let before = edit;
        widgets::color_picker(ui, &th, &mut edit);
        if edit != before {
            self.set_active_color(self.color_target, edit);
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

        match self.tool {
            Tool::Select => {
                ui.horizontal(|ui| {
                    if widgets::button(ui, &th, "Select all", false).clicked() {
                        self.select_all();
                    }
                    if widgets::button(ui, &th, "Crop", false).clicked() {
                        self.crop_to_selection();
                    }
                });
            }
            Tool::Text => {
                ui.label(egui::RichText::new("Font").size(10.0).color(th.text_dim));
                let label = elide(&self.opts.font_family, 24);
                let resp = widgets::drop_field(ui, &th, &label);
                let popup_id = ui.make_persistent_id("font-popup");
                let just_opened = resp.clicked();
                if just_opened {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }
                let mut picked: Option<String> = None;
                let mut search = std::mem::take(&mut self.font_search);
                {
                    let fonts = &self.fonts;
                    let current = self.opts.font_family.clone();
                    egui::popup::popup_below_widget(
                        ui,
                        popup_id,
                        &resp,
                        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(252.0);
                            let hint = format!("Search {} fonts…", fonts.family_count());
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut search)
                                    .hint_text(hint)
                                    .desired_width(244.0),
                            );
                            if just_opened {
                                edit.request_focus();
                            }
                            ui.add_space(2.0);
                            let needle = search.to_lowercase();
                            egui::ScrollArea::vertical()
                                .max_height(250.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    for family in fonts.families() {
                                        if !needle.is_empty()
                                            && !family.to_lowercase().contains(&needle)
                                        {
                                            continue;
                                        }
                                        if ui.selectable_label(family == &current, family).clicked()
                                        {
                                            picked = Some(family.clone());
                                        }
                                    }
                                });
                        },
                    );
                }
                self.font_search = search;
                if let Some(family) = picked {
                    self.opts.font_family = family.clone();
                    self.font_search.clear();
                    ui.memory_mut(|m| m.close_popup());
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

                let mut changed = false;
                if let Some(text) = self.edit.text.as_mut() {
                    if (text.size - self.opts.font_size).abs() > 0.01 {
                        text.size = self.opts.font_size;
                        changed = true;
                    }
                    if text.bold != self.opts.bold {
                        text.bold = self.opts.bold;
                        changed = true;
                    }
                    if text.family != self.opts.font_family {
                        text.family = self.opts.font_family.clone();
                        changed = true;
                    }
                }
                if changed {
                    self.refresh_text();
                }
            }
            Tool::Fill => {
                let mut tolerance = self.opts.tolerance as f32;
                if widgets::slider(ui, &th, "Tolerance", &mut tolerance, 0.0..=160.0, 0) {
                    self.opts.tolerance = tolerance as u8;
                }
            }
            _ => {}
        }

        if self.tool.uses_size() {
            let max = if self.tool == Tool::Pencil {
                64.0
            } else {
                300.0
            };
            widgets::slider(ui, &th, "Size", &mut self.opts.size, 1.0..=max, 0);
        }
        if self.tool.uses_softness() {
            widgets::slider(ui, &th, "Hardness", &mut self.opts.hardness, 0.0..=1.0, 2);
        }
        if !matches!(
            self.tool,
            Tool::Select | Tool::Fill | Tool::Picker | Tool::Pan | Tool::Text
        ) {
            widgets::slider(ui, &th, "Opacity", &mut self.opts.opacity, 0.02..=1.0, 2);
        }
        if self.tool.uses_fill_toggle() {
            widgets::toggle(ui, &th, &mut self.opts.filled, "Filled shape");
        }
        if matches!(self.tool, Tool::Pencil | Tool::Brush) {
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
}
