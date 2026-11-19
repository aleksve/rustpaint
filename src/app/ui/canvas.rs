//! Central canvas: single-quad checkerboard, GPU texture upload, input, overlays.

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, Key, PointerButton, Pos2, Rect, Sense, Stroke,
    StrokeKind, TextureFilter, TextureOptions, TextureWrapMode, Vec2,
};

use crate::app::{App, Btn};
use crate::icons;
use crate::paint::TexDirty;
use crate::tools::Tool;
use crate::widgets::{self, rr};

impl App {
    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) -> Rect {
        let th = self.theme;
        let dark = th.is_dark();
        let blocked = self.modal_open();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(th.canvas_bg))
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.last_view_rect = rect;
                let response = ui.allocate_rect(rect, Sense::click_and_drag());

                if self.view.fit_pending && rect.width() > 32.0 && rect.height() > 32.0 {
                    self.fit_view(rect);
                    self.view.fit_pending = false;
                }

                // Scroll to pan; Ctrl/Cmd + scroll or trackpad pinch to zoom at cursor.
                if response.hovered() && !blocked {
                    let (scroll, pinch, command) =
                        ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
                    if (pinch - 1.0).abs() > 0.001 {
                        let new_zoom = (self.view.zoom * pinch).clamp(0.05, 32.0);
                        self.zoom_at(new_zoom, response.hover_pos(), rect);
                    } else if command && scroll.y.abs() > 0.0 {
                        let factor = (scroll.y * 0.004).exp();
                        let new_zoom = (self.view.zoom * factor).clamp(0.05, 32.0);
                        self.zoom_at(new_zoom, response.hover_pos(), rect);
                    } else if scroll != Vec2::ZERO {
                        self.view.pan += scroll;
                    }
                }

                let zoom = self.view.zoom;
                let canvas_size = Vec2::new(self.doc.w as f32, self.doc.h as f32) * zoom;
                let origin = rect.center() - canvas_size * 0.5 + self.view.pan;
                self.origin = origin;
                let canvas_rect = Rect::from_min_size(origin, canvas_size);
                let visible = canvas_rect.intersect(rect);

                let painter = ui.painter().with_clip_rect(rect);

                // Subtle rounded drop shadow framing the canvas.
                painter.rect_filled(
                    canvas_rect.expand(8.0),
                    rr(6),
                    Color32::from_black_alpha(if dark { 110 } else { 34 }),
                );

                // Hardware-repeated 2×2 checkerboard texture (single quad regardless of
                // canvas size or zoom level; 8 pt squares = 16 pt 2×2 period).
                let checker_id = self.ensure_checker_texture(ui.ctx());
                if visible.is_positive() {
                    let period = 16.0;
                    let u0 = (visible.min.x - canvas_rect.min.x) / period;
                    let v0 = (visible.min.y - canvas_rect.min.y) / period;
                    let u1 = (visible.max.x - canvas_rect.min.x) / period;
                    let v1 = (visible.max.y - canvas_rect.min.y) / period;
                    painter.image(
                        checker_id,
                        visible,
                        Rect::from_min_max(Pos2::new(u0, v0), Pos2::new(u1, v1)),
                        Color32::WHITE,
                    );
                }

                // Keep the GPU texture in sync with the document and zoom filter.
                let want_nearest = zoom >= 1.0;
                let texture_options = if want_nearest {
                    TextureOptions::NEAREST
                } else {
                    TextureOptions::LINEAR
                };
                if self.tex.is_some() && self.tex_nearest != want_nearest {
                    self.tex_dirty.mark_all();
                }
                self.tex_nearest = want_nearest;
                match std::mem::replace(&mut self.tex_dirty, TexDirty::Clean) {
                    TexDirty::Clean => {
                        if self.tex.is_none() {
                            let image = self.doc.to_color_image();
                            self.tex =
                                Some(ui.ctx().load_texture("canvas", image, texture_options));
                        }
                    }
                    TexDirty::Full => {
                        let image = self.doc.to_color_image();
                        match &mut self.tex {
                            Some(texture) => texture.set(image, texture_options),
                            None => {
                                self.tex =
                                    Some(ui.ctx().load_texture("canvas", image, texture_options));
                            }
                        }
                    }
                    TexDirty::Rect(dirty_rect) => {
                        let clipped = dirty_rect.clamp(self.doc.w as i32, self.doc.h as i32);
                        match &mut self.tex {
                            Some(texture)
                                if texture.size() == [self.doc.w, self.doc.h]
                                    && !clipped.is_empty() =>
                            {
                                let patch = self.doc.rect_color_image(clipped);
                                let offset = [clipped.x0 as usize, clipped.y0 as usize];
                                texture.set_partial(offset, patch, texture_options);
                            }
                            _ => {
                                let image = self.doc.to_color_image();
                                self.tex =
                                    Some(ui.ctx().load_texture("canvas", image, texture_options));
                            }
                        }
                    }
                }

                if let Some(texture) = &self.tex
                    && visible.is_positive()
                {
                    painter.image(
                        texture.id(),
                        canvas_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // Crisp border around the canvas.
                painter.rect_stroke(
                    canvas_rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0_f32, th.border),
                    StrokeKind::Outside,
                );

                // Viewport-clipped pixel grid at high zoom.
                if self.view.grid && zoom >= 6.0 && visible.is_positive() {
                    let grid_color = Color32::from_black_alpha(if dark { 70 } else { 40 });
                    let stroke = Stroke::new(1.0_f32, grid_color);
                    let x0 = (((visible.min.x - origin.x) / zoom).ceil() as usize).max(1);
                    let x1 = (((visible.max.x - origin.x) / zoom).floor() as usize)
                        .min(self.doc.w.saturating_sub(1));
                    if x0 <= x1 {
                        for x in x0..=x1 {
                            let sx = origin.x + x as f32 * zoom;
                            painter.line_segment(
                                [Pos2::new(sx, visible.min.y), Pos2::new(sx, visible.max.y)],
                                stroke,
                            );
                        }
                    }
                    let y0 = (((visible.min.y - origin.y) / zoom).ceil() as usize).max(1);
                    let y1 = (((visible.max.y - origin.y) / zoom).floor() as usize)
                        .min(self.doc.h.saturating_sub(1));
                    if y0 <= y1 {
                        for y in y0..=y1 {
                            let sy = origin.y + y as f32 * zoom;
                            painter.line_segment(
                                [Pos2::new(visible.min.x, sy), Pos2::new(visible.max.x, sy)],
                                stroke,
                            );
                        }
                    }
                }

                // Marching ants around an active selection (clipped to viewport via `painter`).
                if let Some(selection) = self.edit.selection {
                    let selection = selection.clamp(self.doc.w as i32, self.doc.h as i32);
                    if !selection.is_empty() {
                        let selection_rect = Rect::from_min_max(
                            origin + Vec2::new(selection.x0 as f32, selection.y0 as f32) * zoom,
                            origin + Vec2::new(selection.x1 as f32, selection.y1 as f32) * zoom,
                        );
                        let phase = (ui.input(|i| i.time) * 28.0) as f32;
                        icons::marching_ants(&painter, selection_rect, phase, dark);
                        ui.ctx().request_repaint_after_secs(0.06);
                    }
                }

                // Polygon construction vertices.
                let poly_pts = self.polygon_points();
                if !poly_pts.is_empty() {
                    for &(px, py) in poly_pts {
                        let screen_point = origin + Vec2::new(px, py) * zoom;
                        painter.circle_filled(screen_point, 4.0, th.on_accent);
                        painter.circle_stroke(screen_point, 4.0, Stroke::new(1.6_f32, th.accent));
                    }
                }

                self.cursor_img = response.hover_pos().map(|pos| {
                    let image_pos = self.screen_to_image(pos);
                    (image_pos.x.floor() as i32, image_pos.y.floor() as i32)
                });

                self.handle_canvas_input(ui, &response, blocked);

                // Brush preview circle under the cursor.
                if !blocked
                    && response.hovered()
                    && !self.space_pan
                    && self.tool.uses_size()
                    && !self.tool.is_shape()
                    && self.tool != Tool::Polygon
                    && let Some(pos) = response.hover_pos()
                {
                    let radius = (self.opts.size * 0.5 * zoom).max(1.5);
                    painter.circle_stroke(
                        pos,
                        radius + 0.8,
                        Stroke::new(1.0_f32, Color32::from_black_alpha(150)),
                    );
                    painter.circle_stroke(
                        pos,
                        radius,
                        Stroke::new(1.0_f32, Color32::from_white_alpha(210)),
                    );
                }
            })
            .response
            .rect
    }

    fn ensure_checker_texture(&mut self, ctx: &egui::Context) -> egui::TextureId {
        let dark = self.theme.is_dark();
        let opts = TextureOptions {
            magnification: TextureFilter::Nearest,
            minification: TextureFilter::Nearest,
            wrap_mode: TextureWrapMode::Repeat,
            mipmap_mode: None,
        };
        if let Some((cached_dark, ref handle)) = self.checker_tex
            && cached_dark == dark
        {
            return handle.id();
        }
        let (a, b) = self.theme.checker_colors();
        let img = widgets::checker_texture_image(a, b);
        let handle = match self.checker_tex.take() {
            Some((_, mut h)) => {
                h.set(img, opts);
                h
            }
            None => ctx.load_texture("checker", img, opts),
        };
        let id = handle.id();
        self.checker_tex = Some((dark, handle));
        id
    }

    fn handle_canvas_input(&mut self, ui: &mut egui::Ui, response: &egui::Response, blocked: bool) {
        if blocked {
            if self.edit.dragging.take().is_some() {
                self.pointer_release();
            }
            return;
        }

        let ctx = ui.ctx().clone();
        let typing = ctx.egui_wants_keyboard_input() || self.edit.is_editing_text();
        let space = !typing && ui.input(|i| i.key_down(Key::Space));
        self.space_pan = space;

        let (
            pointer,
            primary_down,
            secondary_down,
            middle_down,
            primary_pressed,
            secondary_pressed,
            double_clicked,
            triple_clicked,
            shift,
            alt,
            delta,
            dt,
            now,
        ) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_down(PointerButton::Primary),
                i.pointer.button_down(PointerButton::Secondary),
                i.pointer.button_down(PointerButton::Middle),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.pointer.button_double_clicked(PointerButton::Primary),
                i.pointer.button_triple_clicked(PointerButton::Primary),
                i.modifiers.shift,
                i.modifiers.alt,
                i.pointer.delta(),
                i.unstable_dt,
                i.time,
            )
        });

        let over_text_bar = pointer.is_some_and(|p| {
            self.edit
                .text_toolbar_rect
                .is_some_and(|bar| bar.contains(p))
        });
        let over_text_inner = !over_text_bar
            && pointer.is_some_and(|p| self.edit.text_box_rect.is_some_and(|r| r.contains(p)));
        let over_text_border = !over_text_bar
            && !over_text_inner
            && pointer.is_some_and(|p| {
                self.edit
                    .text_box_rect
                    .is_some_and(|r| r.expand(9.0).contains(p))
            });

        let panning = self.tool == Tool::Pan || space || middle_down;
        if response.hovered() && !over_text_bar {
            let over_float_or_sel = self.tool == Tool::Select
                && !shift
                && pointer.is_some_and(|p| {
                    let img = self.screen_to_image(p);
                    self.edit
                        .selection
                        .is_some_and(|r| r.contains(img.x.floor() as i32, img.y.floor() as i32))
                });
            let icon = if panning {
                if primary_down || middle_down {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Grab
                }
            } else if self.edit.is_editing_text() {
                match self.edit.text_drag {
                    crate::app::transient::TextDrag::Move => CursorIcon::Grabbing,
                    crate::app::transient::TextDrag::Select => CursorIcon::Text,
                    _ if over_text_border => CursorIcon::Move,
                    _ => CursorIcon::Text,
                }
            } else if over_float_or_sel {
                if primary_down {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Move
                }
            } else if self.tool == Tool::Text {
                CursorIcon::Text
            } else {
                CursorIcon::Crosshair
            };
            ctx.set_cursor_icon(icon);
        }

        // Always honour button release before pan short-circuit so pressing Space
        // mid-stroke cannot leave `self.edit.dragging` stranded.
        if !primary_down {
            self.edit.text_drag = crate::app::transient::TextDrag::None;
        }
        if let Some(button) = self.edit.dragging {
            let still_down = match button {
                Btn::Primary => primary_down,
                Btn::Secondary => secondary_down,
            };
            if !still_down {
                self.edit.dragging = None;
                self.edit.select_drag_start = None;
                self.edit.move_float_origin = None;
                self.edit.last_drag_input = None;
                self.pointer_release();
            }
        }

        if panning {
            if (response.dragged() || (response.hovered() && (primary_down || middle_down)))
                && delta != Vec2::ZERO
            {
                self.view.pan += delta;
            }
            return;
        }

        // Dedicated pointer handling for live on-canvas text editing (caret
        // placement, double/triple-click, drag-selection, border move, and
        // click-outside-to-apply).
        if self.edit.is_editing_text() {
            use crate::app::transient::TextDrag;
            match self.edit.text_drag {
                TextDrag::Move => {
                    if primary_down && delta != Vec2::ZERO {
                        self.move_text_by(delta);
                    }
                    return;
                }
                TextDrag::Select => {
                    if primary_down && let Some(pos) = pointer {
                        let img = self.screen_to_image(pos);
                        if let Some(origin) = self.text_session().map(|t| t.pos)
                            && let Some(layout) = self.text_layout()
                            && let Some(live) = self.text_session_mut()
                        {
                            let idx = layout.hit_test(img.x - origin.0, img.y - origin.1);
                            live.set_cursor(idx, true, false);
                            live.blink_origin = now;
                        }
                    }
                    return;
                }
                TextDrag::Swallowed => return,
                TextDrag::None => {}
            }

            if response.hovered()
                && !over_text_bar
                && primary_pressed
                && let Some(pos) = pointer
            {
                self.focus_canvas_text(&ctx);
                if over_text_inner {
                    if self
                        .text_session_mut()
                        .is_some_and(crate::paint::TextSession::commit_preedit)
                    {
                        self.refresh_text();
                    }
                    let img = self.screen_to_image(pos);
                    if let Some(origin) = self.text_session().map(|t| t.pos)
                        && let Some(layout) = self.text_layout()
                        && let Some(live) = self.text_session_mut()
                    {
                        let idx = layout.hit_test(img.x - origin.0, img.y - origin.1);
                        let next_drag = if triple_clicked {
                            live.select_line_at(idx);
                            TextDrag::Swallowed
                        } else if double_clicked {
                            live.select_word_at(idx);
                            TextDrag::Swallowed
                        } else {
                            live.set_cursor(idx, shift, false);
                            TextDrag::Select
                        };
                        live.blink_origin = now;
                        self.edit.text_drag = next_drag;
                    }
                } else if over_text_border {
                    self.edit.text_drag = TextDrag::Move;
                } else if self.text_session().is_some_and(|t| t.is_empty()) {
                    let img = self.screen_to_image(pos);
                    self.reposition_empty_text(img, now);
                    self.edit.text_drag = TextDrag::Swallowed;
                } else {
                    self.commit_current();
                    self.edit.text_drag = TextDrag::Swallowed;
                }
            }
            return;
        }

        // Prevent the tail of a click that just committed text from spawning a
        // new box while the button is still held.
        if self.edit.text_drag == crate::app::transient::TextDrag::Swallowed {
            return;
        }

        // Clicking with the Text tool starts a focused session immediately so
        // the very next keystroke types straight onto the canvas.
        if self.tool == Tool::Text {
            if response.hovered()
                && primary_pressed
                && let Some(pos) = pointer
            {
                self.focus_canvas_text(&ctx);
                let img = self.screen_to_image(pos);
                self.start_text_session(img, now, true);
                self.edit.text_drag = crate::app::transient::TextDrag::Swallowed;
            }
            return;
        }

        // Polygon tool finishes on double-click or right-click.
        if self.tool == Tool::Polygon && self.edit.is_drawing_polygon() {
            if response.double_clicked() || (response.hovered() && secondary_pressed) {
                self.finish_polygon();
                return;
            }
            if let Some(pos) = response.hover_pos()
                && self.edit.last_polygon_hover != Some(pos)
            {
                self.edit.last_polygon_hover = Some(pos);
                let img = self.screen_to_image(pos);
                self.update_polygon_preview(img);
            }
        }

        if response.hovered() && self.edit.dragging.is_none() {
            let pressed_button = if primary_pressed {
                Some(Btn::Primary)
            } else if secondary_pressed {
                Some(Btn::Secondary)
            } else {
                None
            };
            if let (Some(button), Some(pos)) = (pressed_button, pointer) {
                self.edit.dragging = Some(button);
                self.edit.last_drag_input = Some((pos, shift));
                self.pointer_press(button, pos, shift, alt);
            }
        }

        if self.edit.dragging.is_some()
            && let Some(pos) = pointer
        {
            let input_changed = self.edit.last_drag_input != Some((pos, shift));
            let continuous_spray = self.tool == Tool::Airbrush;
            if self.tool == Tool::Select && self.edit.select_drag_start.is_some() {
                if input_changed {
                    self.edit.last_drag_input = Some((pos, shift));
                    let image_pos = self.screen_to_image(pos);
                    self.drag_select(image_pos, shift);
                }
            } else if input_changed || continuous_spray {
                self.edit.last_drag_input = Some((pos, shift));
                self.pointer_move(pos, shift, dt);
                if continuous_spray {
                    ui.ctx().request_repaint_after_secs(1.0 / 60.0);
                }
            }
        }
    }
}
