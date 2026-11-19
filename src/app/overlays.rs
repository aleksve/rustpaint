//! Foreground overlays: drag-and-drop hint, WYSIWYG canvas text editing,
//! dialogs, I/O progress, and toasts.

use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind,
    Vec2,
};

use super::App;
use crate::filedlg::FileAction;
use crate::widgets::{self, blend, rr};

impl App {
    pub(crate) fn overlays(&mut self, ctx: &Context, frame: Option<&eframe::Frame>) {
        let th = self.theme;

        // Drop target hint.
        let dragging_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if dragging_file {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("drop-hint"),
            ));
            painter.rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha(90));
            let inner = screen.shrink(18.0);
            painter.rect_stroke(
                inner,
                rr(16),
                Stroke::new(2.0_f32, th.accent),
                StrokeKind::Inside,
            );
            painter.text(
                screen.center(),
                Align2::CENTER_CENTER,
                "Drop an image to open it",
                FontId::proportional(20.0),
                th.on_accent,
            );
        }

        self.text_overlay(ctx);
        self.show_dialogs(ctx, frame);

        // The file dialog owns its state while open. Taking it out avoids
        // borrowing `App` while the dialog builds its modal.
        if let Some(mut file_dialog) = self.file_dialog.take() {
            let action = file_dialog.show(ctx, &th);
            if matches!(action, FileAction::None) {
                self.file_dialog = Some(file_dialog);
            } else {
                self.handle_file_action(action, ctx);
            }
        }

        if let Some(job) = self.io_job.as_ref() {
            let label = job.label.clone();
            widgets::modal(ctx, &th, "io-progress", Vec2::new(300.0, 90.0), |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(label).size(13.0).color(th.text));
                });
            });
        }

        self.toast_overlay(ctx, th);
    }

    /// Paint the WYSIWYG on-canvas text frame, glyph-exact selection highlight,
    /// IME preedit underline, blinking caret, and floating `Move / Apply / Cancel`
    /// action pill — all measured directly from `TextEngine::layout` so the
    /// caret always matches the rendered text 1:1 without wrapping into a box.
    fn text_overlay(&mut self, ctx: &Context) {
        let Some(layout) = self.text_layout() else {
            self.edit.text_toolbar_rect = None;
            self.edit.text_box_rect = None;
            return;
        };
        let Some((pos, size, cursor, anchor, blink_origin, is_empty, display_cursor, preedit_span)) =
            self.text_session().map(|s| {
                let (_, dc, pre) = s.display_text();
                (
                    s.pos,
                    s.size,
                    s.cursor,
                    s.anchor,
                    s.blink_origin,
                    s.is_empty(),
                    dc,
                    pre,
                )
            })
        else {
            return;
        };

        let th = self.theme;
        let zoom = self.view.zoom.max(0.01);
        let captures_keys = self.canvas_text_captures_keys();
        let text_color = self.text_color().unwrap_or(th.text);

        let origin_screen = self.image_to_screen(Pos2::new(pos.0, pos.1));
        let glyph_h_screen = (layout.glyph_h * zoom).max(10.0);
        let block_h_screen = (layout.height * zoom).max(glyph_h_screen);

        // Clip on-canvas decorations strictly to the canvas viewport so they
        // never bleed onto the top bar, tool rail, status bar, or sidebar.
        let canvas_clip = self.last_view_rect.intersect(ctx.content_rect());
        let painter = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::unique("canvas-text-overlay"),
            ))
            .with_clip_rect(canvas_clip);

        // Measure optional placeholder when no characters have been typed yet.
        let hint_font = FontId::proportional((size * zoom * 0.78).clamp(11.0, 48.0));
        let hint_galley = is_empty.then(|| {
            painter.layout_no_wrap(
                "Type text…".to_owned(),
                hint_font,
                Color32::from_rgba_unmultiplied(120, 128, 144, 175),
            )
        });

        let block_w_screen = if let Some(ref g) = hint_galley {
            (g.size().x + 8.0).max(72.0)
        } else {
            (layout.width * zoom + 4.0).max(14.0)
        };
        let text_rect =
            Rect::from_min_size(origin_screen, Vec2::new(block_w_screen, block_h_screen));
        let hit_rect = text_rect.expand(4.0);
        let frame_rect = text_rect.expand(5.0);
        self.edit.text_box_rect = Some(hit_rect);

        // Subtle rounded outline around the actual single- or multi-line text.
        let frame_alpha = if captures_keys { 0.72 } else { 0.38 };
        painter.rect_stroke(
            frame_rect,
            rr(4),
            Stroke::new(1.0, th.accent.gamma_multiply(frame_alpha)),
            StrokeKind::Outside,
        );

        // Placeholder text when the box is freshly placed and empty.
        if let Some(galley) = hint_galley {
            let gy = origin_screen.y + (glyph_h_screen - galley.size().y) * 0.5;
            painter.galley(
                Pos2::new(origin_screen.x + 4.0, gy),
                galley,
                Color32::PLACEHOLDER,
            );
        }

        // Character selection highlight aligned to exact rasterised glyphs.
        if preedit_span.is_none() && anchor != cursor {
            let sel_fill = th.accent.gamma_multiply(0.28);
            for (x0, y0, x1, y1) in layout.selection_rects(anchor, cursor) {
                let p0 = self.image_to_screen(Pos2::new(pos.0 + x0, pos.1 + y0));
                let p1 = self.image_to_screen(Pos2::new(pos.0 + x1, pos.1 + y1));
                painter.rect_filled(Rect::from_min_max(p0, p1), rr(2), sel_fill);
            }
        }

        // Active IME composition underline (dead-key accents, CJK input, etc.).
        if let Some(pre) = preedit_span {
            let fg = Color32::from_rgb(text_color.r(), text_color.g(), text_color.b());
            for (x0, _, x1, y1) in layout.selection_rects(pre.start, pre.end) {
                let a = self.image_to_screen(Pos2::new(pos.0 + x0, pos.1 + y1));
                let b = self.image_to_screen(Pos2::new(pos.0 + x1, pos.1 + y1));
                painter.line_segment([a, b], Stroke::new(1.5, fg));
            }
        }

        // Caret position & platform IME rect.
        let (_, cx, cy) = layout.caret_pos(display_cursor);
        let caret_top = self.image_to_screen(Pos2::new(pos.0 + cx, pos.1 + cy));
        let caret_bot = Pos2::new(caret_top.x, caret_top.y + glyph_h_screen);
        let caret_rect = Rect::from_min_max(
            Pos2::new(caret_top.x - 1.0, caret_top.y),
            Pos2::new(caret_top.x + 2.0, caret_bot.y),
        );

        if captures_keys {
            const HALF_PERIOD: f64 = 0.53;
            let now = ctx.input(|i| i.time);
            let elapsed = (now - blink_origin).max(0.0);
            let phase = elapsed % (HALF_PERIOD * 2.0);
            let visible = phase < HALF_PERIOD;
            if visible {
                let fg = Color32::from_rgb(text_color.r(), text_color.g(), text_color.b());
                let luma = 0.299 * f32::from(fg.r())
                    + 0.587 * f32::from(fg.g())
                    + 0.114 * f32::from(fg.b());
                let halo = if luma < 140.0 {
                    Color32::from_white_alpha(190)
                } else {
                    Color32::from_black_alpha(170)
                };
                painter.line_segment([caret_top, caret_bot], Stroke::new(3.2, halo));
                painter.line_segment([caret_top, caret_bot], Stroke::new(1.8, fg));
            }
            let to_next = if visible {
                HALF_PERIOD - phase
            } else {
                HALF_PERIOD * 2.0 - phase
            };
            ctx.request_repaint_after_secs((to_next as f32 + 0.01).clamp(0.02, 0.55));

            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: hit_rect,
                    cursor_rect: caret_rect,
                    should_interrupt_composition: false,
                });
            });
        }

        if self.modal_open() {
            self.edit.text_toolbar_rect = None;
            return;
        }

        // Floating `Move / Apply / Cancel` pill, placed just above the text box
        // (or just below when near the top of the canvas) and clamped strictly
        // inside the canvas viewport so it never overlaps any edge panel.
        let view = canvas_clip.shrink(6.0);
        let bar_h = 32.0;
        let bar_w_est = 200.0;
        let above_y = frame_rect.top() - bar_h - 6.0;
        let bar_y = if above_y >= view.top() {
            above_y
        } else {
            (frame_rect.bottom() + 6.0).min((view.bottom() - bar_h).max(view.top()))
        };
        let bar_x = frame_rect
            .left()
            .clamp(view.left(), (view.right() - bar_w_est).max(view.left()));

        let mut drag_delta = Vec2::ZERO;
        let mut apply_clicked = false;
        let mut cancel_clicked = false;
        let mut bar_rect = None;

        egui::Area::new(egui::Id::unique("text-toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(bar_x, bar_y))
            .show(ctx, |ui| {
                let resp = egui::Frame::NONE
                    .fill(th.panel)
                    .corner_radius(rr(7))
                    .stroke(Stroke::new(1.0, th.border))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (grip_rect, grip_resp) =
                                ui.allocate_exact_size(Vec2::new(56.0, 20.0), Sense::drag());
                            if grip_resp.hovered() || grip_resp.dragged() {
                                ui.ctx().set_cursor_icon(if grip_resp.dragged() {
                                    egui::CursorIcon::Grabbing
                                } else {
                                    egui::CursorIcon::Grab
                                });
                            }
                            let dots_origin =
                                Pos2::new(grip_rect.left() + 3.0, grip_rect.center().y - 3.5);
                            for row in 0..3 {
                                for col in 0..2 {
                                    ui.painter().circle_filled(
                                        dots_origin + Vec2::new(col as f32 * 4.0, row as f32 * 3.5),
                                        1.1,
                                        th.text_dim,
                                    );
                                }
                            }
                            ui.painter().text(
                                Pos2::new(grip_rect.left() + 13.0, grip_rect.center().y),
                                Align2::LEFT_CENTER,
                                "Move",
                                FontId::proportional(11.0),
                                th.text_dim,
                            );
                            if grip_resp.dragged() && !ui.input(|i| i.pointer.primary_pressed()) {
                                drag_delta += grip_resp.drag_delta();
                            }
                            if ui
                                .small_button(
                                    egui::RichText::new("Apply").color(th.accent).strong(),
                                )
                                .on_hover_text(if cfg!(target_os = "macos") {
                                    "Rasterise text onto canvas (⌘↵)"
                                } else {
                                    "Rasterise text onto canvas (Ctrl+Enter)"
                                })
                                .clicked()
                            {
                                apply_clicked = true;
                            }
                            if ui
                                .small_button(egui::RichText::new("Cancel").color(th.text_dim))
                                .on_hover_text("Discard text (Esc)")
                                .clicked()
                            {
                                cancel_clicked = true;
                            }
                        });
                    })
                    .response;
                bar_rect = Some(resp.rect);
            });

        self.edit.text_toolbar_rect = bar_rect;

        if cancel_clicked {
            self.cancel_current();
        } else if apply_clicked {
            self.commit_current();
        } else if drag_delta != Vec2::ZERO {
            self.focus_canvas_text(ctx);
            self.move_text_by(drag_delta);
        }
    }

    fn toast_overlay(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let Some((message, until)) = self.toast_msg.as_ref() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let remaining = until - now;
        if remaining <= 0.0 {
            self.toast_msg = None;
            return;
        }
        let message = message.clone();

        egui::Area::new(egui::Id::unique("toast"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -44.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(blend(th.panel_alt, th.accent, 0.08))
                    .corner_radius(rr(10))
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(16, 9))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(message).size(12.5).color(th.text));
                    });
            });
        // Wake up once right when the toast expires instead of polling continuously.
        ctx.request_repaint_after_secs((remaining as f32 + 0.02).max(0.02));
    }
}
