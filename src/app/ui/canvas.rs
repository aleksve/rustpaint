//! Canvas rendering, pointer interaction, and texture synchronization.

use eframe::egui::{
    self, Color32, Context, Key, Pos2, Rect, Rounding, Sense, Stroke, TextureOptions, Ui, Vec2,
};

use super::super::{App, Btn};
use crate::icons;
use crate::tools::Tool;
use crate::widgets;

impl App {
    // --------------------------------------------------------------- canvas

    pub(crate) fn canvas_ui(&mut self, ui: &mut Ui) {
        let th = self.theme;
        let rect = ui.available_rect_before_wrap();
        self.last_view_rect = rect;
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter().with_clip_rect(rect);

        painter.rect_filled(rect, Rounding::ZERO, th.canvas_bg);

        if self.view.fit_pending {
            self.fit_view(rect);
            self.view.fit_pending = false;
        }

        let zoom = self.view.zoom;
        self.scale = zoom;
        let size = Vec2::new(self.doc.w as f32, self.doc.h as f32) * zoom;
        let origin = rect.center() - size * 0.5 + self.view.pan;
        self.origin = origin;
        let canvas_rect = Rect::from_min_size(origin, size);

        if !self.modal_open() {
            self.canvas_input(ui, &resp, rect, canvas_rect);
        }

        self.ensure_texture(ui.ctx());

        painter.rect_filled(
            canvas_rect.expand(8.0),
            Rounding::same(6.0),
            Color32::from_black_alpha(if th.is_dark() { 110 } else { 34 }),
        );
        let (checker_a, checker_b) = if th.is_dark() {
            (
                Color32::from_rgb(0x2a, 0x2d, 0x33),
                Color32::from_rgb(0x32, 0x36, 0x3d),
            )
        } else {
            (
                Color32::from_rgb(0xe4, 0xe7, 0xec),
                Color32::from_rgb(0xf3, 0xf5, 0xf8),
            )
        };
        widgets::checkerboard(&painter, canvas_rect, 8.0, checker_a, checker_b);
        if let Some(texture) = &self.tex {
            painter.image(
                texture.id(),
                canvas_rect,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        painter.rect_stroke(canvas_rect, Rounding::ZERO, Stroke::new(1.0_f32, th.border));

        if self.view.grid && zoom >= 6.0 {
            let grid_color = Color32::from_black_alpha(if th.is_dark() { 70 } else { 40 });
            let x0 = (((rect.left() - origin.x) / zoom).ceil()).max(0.0) as i32;
            let x1 = (((rect.right() - origin.x) / zoom).floor()).min(self.doc.w as f32) as i32;
            for i in x0..=x1 {
                let x = origin.x + i as f32 * zoom;
                painter.line_segment(
                    [
                        Pos2::new(x, canvas_rect.top()),
                        Pos2::new(x, canvas_rect.bottom()),
                    ],
                    Stroke::new(1.0_f32, grid_color),
                );
            }
            let y0 = (((rect.top() - origin.y) / zoom).ceil()).max(0.0) as i32;
            let y1 = (((rect.bottom() - origin.y) / zoom).floor()).min(self.doc.h as f32) as i32;
            for i in y0..=y1 {
                let y = origin.y + i as f32 * zoom;
                painter.line_segment(
                    [
                        Pos2::new(canvas_rect.left(), y),
                        Pos2::new(canvas_rect.right(), y),
                    ],
                    Stroke::new(1.0_f32, grid_color),
                );
            }
        }

        if let Some(selection) = self.edit.selection {
            let selection = selection.clamp(self.doc.w as i32, self.doc.h as i32);
            if !selection.is_empty() {
                let selection_rect = Rect::from_min_max(
                    self.image_to_screen(Pos2::new(selection.x0 as f32, selection.y0 as f32)),
                    self.image_to_screen(Pos2::new(selection.x1 as f32, selection.y1 as f32)),
                );
                let time = ui.ctx().input(|i| i.time) as f32 * 28.0;
                icons::marching_ants(&painter, selection_rect, time, th.is_dark());
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(60));
            }
        }

        if !self.edit.polygon.is_empty() {
            let points: Vec<Pos2> = self
                .edit
                .polygon
                .iter()
                .map(|p| self.image_to_screen(Pos2::new(p.0, p.1)))
                .collect();
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(1.5_f32, th.accent),
            ));
            if let Some(hover) = ui.ctx().input(|i| i.pointer.hover_pos()) {
                if let Some(last) = points.last() {
                    painter.line_segment(
                        [*last, hover],
                        Stroke::new(1.0_f32, th.accent.gamma_multiply(0.6)),
                    );
                }
            }
            for point in &points {
                painter.circle_filled(*point, 3.0, th.accent);
            }
        }

        if let Some(hover) = ui.ctx().input(|i| i.pointer.hover_pos()) {
            if rect.contains(hover) && self.edit.session.is_none() && self.tool.uses_size() {
                let radius = (self.opts.size * 0.5 * zoom).clamp(2.0, 3000.0);
                painter.circle_stroke(
                    hover,
                    radius,
                    Stroke::new(1.0_f32, Color32::from_black_alpha(140)),
                );
                painter.circle_stroke(
                    hover,
                    radius + 1.0,
                    Stroke::new(1.0_f32, Color32::from_white_alpha(160)),
                );
            }
        }
    }

    pub(crate) fn canvas_input(
        &mut self,
        ui: &mut Ui,
        response: &egui::Response,
        view_rect: Rect,
        canvas: Rect,
    ) {
        let ctx = ui.ctx().clone();
        let (
            hover,
            scroll,
            modifiers,
            primary_down,
            secondary_down,
            middle_down,
            primary_pressed,
            secondary_pressed,
            primary_released,
            secondary_released,
            space,
        ) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.raw_scroll_delta,
                i.modifiers,
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Secondary),
                i.key_down(Key::Space),
            )
        });
        let hovered = response.hovered();
        let canvas_hovered = hover.is_some_and(|point| canvas.contains(point));

        if hovered && scroll != Vec2::ZERO {
            if modifiers.ctrl || modifiers.command {
                let anchor = hover.unwrap_or(view_rect.center());
                let factor = (scroll.y * 0.0016).exp();
                self.zoom_at(self.view.zoom * factor, Some(anchor), view_rect);
            } else if modifiers.shift {
                self.view.pan.x += scroll.y;
            } else {
                self.view.pan += scroll;
            }
        }
        let pinch = ctx.input(|i| i.zoom_delta());
        if hovered && (pinch - 1.0).abs() > 1e-4 {
            self.zoom_at(self.view.zoom * pinch, hover, view_rect);
        }

        if let Some(hover) = hover {
            if canvas.contains(hover) {
                let image = self.screen_to_image(hover);
                self.cursor_img = Some((image.x.floor() as i32, image.y.floor() as i32));
            } else {
                self.cursor_img = None;
            }
        }

        let pan_mode = space || self.tool == Tool::Pan || middle_down;
        if pan_mode && (primary_down || middle_down) {
            self.view.pan += ctx.input(|i| i.pointer.delta());
            if hovered {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            return;
        }

        if canvas_hovered {
            ctx.set_cursor_icon(match self.tool {
                Tool::Text => egui::CursorIcon::Text,
                Tool::Pan => egui::CursorIcon::Grab,
                _ => egui::CursorIcon::Crosshair,
            });
        }

        if canvas_hovered && self.edit.dragging.is_none() {
            if primary_pressed {
                if let Some(hover) = hover {
                    self.edit.dragging = Some(Btn::Primary);
                    self.pointer_press(Btn::Primary, hover, modifiers.shift, modifiers.alt);
                }
            } else if secondary_pressed {
                if let Some(hover) = hover {
                    self.edit.dragging = Some(Btn::Secondary);
                    self.pointer_press(Btn::Secondary, hover, modifiers.shift, modifiers.alt);
                }
            }
        }

        if let Some(button) = self.edit.dragging {
            if let Some(hover) = hover {
                if self.tool == Tool::Select
                    && self.edit.select_drag_start.is_some()
                    && self.edit.session.is_none()
                {
                    self.drag_select(self.screen_to_image(hover), modifiers.shift);
                } else if primary_down || secondary_down {
                    self.pointer_move(hover, modifiers.shift);
                }
            }
            let released = (button == Btn::Primary && primary_released)
                || (button == Btn::Secondary && secondary_released);
            if released {
                self.pointer_release();
                self.edit.select_drag_start = None;
                self.edit.move_float_origin = None;
                self.edit.dragging = None;
            }
        }

        if self.tool == Tool::Polygon
            && !self.edit.polygon.is_empty()
            && self.edit.session.is_some()
            && self.edit.dragging.is_none()
        {
            if let Some(hover) = hover {
                self.update_polygon_preview(self.screen_to_image(hover));
            }
        }

        if canvas_hovered && self.tool == Tool::Polygon && response.double_clicked() {
            self.finish_polygon();
        }

        if self.tool == Tool::Airbrush && self.edit.session.is_some() {
            ctx.request_repaint();
        }
    }

    pub(crate) fn ensure_texture(&mut self, ctx: &Context) {
        use crate::paint::TexDirty;

        let nearest = self.view.zoom >= 1.0;
        let sampling_changed = nearest != self.tex_nearest;
        if self.tex_dirty == TexDirty::Clean && !sampling_changed && self.tex.is_some() {
            return;
        }
        let options = if nearest {
            TextureOptions::NEAREST
        } else {
            TextureOptions::LINEAR
        };
        let partial = match self.tex_dirty {
            TexDirty::Rect(rect) if !sampling_changed && self.tex.is_some() => {
                Some(rect.clamp(self.doc.w as i32, self.doc.h as i32))
            }
            _ => None,
        };

        if let Some(rect) = partial {
            if !rect.is_empty() {
                let image = self.doc.rect_color_image(rect);
                if let Some(texture) = self.tex.as_mut() {
                    texture.set_partial([rect.x0 as usize, rect.y0 as usize], image, options);
                }
            }
        } else {
            let image = self.doc.to_color_image();
            match &mut self.tex {
                Some(texture) => texture.set(image, options),
                None => self.tex = Some(ctx.load_texture("canvas", image, options)),
            }
        }
        self.tex_nearest = nearest;
        self.tex_dirty = TexDirty::Clean;
    }
}
