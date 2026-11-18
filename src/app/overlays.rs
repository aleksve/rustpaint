//! Foreground overlays: drag-and-drop hint, text editing, dialogs, I/O progress, and toasts.

use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, FontId, Pos2, Sense, Stroke, StrokeKind, Vec2,
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

    fn text_overlay(&mut self, ctx: &Context) {
        let Some(snapshot) = self.text_session().cloned() else {
            return;
        };

        let th = self.theme;
        let zoom = self.view.zoom;
        let screen = self.image_to_screen(Pos2::new(snapshot.pos.0, snapshot.pos.1));
        let mut text = snapshot.text;
        let want_focus = snapshot.just_created;
        let mut editor_rect = None;
        let mut drag_delta = Vec2::ZERO;
        let mut apply_clicked = false;
        let mut cancel_clicked = false;

        // Keep the editor at least as wide as the painted text so egui never
        // wraps lines narrower than the rasterised canvas preview.
        let painted_width = self.text_hit_rect().map_or(0.0, |r| r.w() as f32 * zoom);
        let editor_width = painted_width.max(260.0);
        let bar_h = 26.0;
        let area_pos = Pos2::new(screen.x.max(8.0), (screen.y - bar_h - 4.0).max(8.0));

        let mut text_changed = false;
        egui::Area::new(egui::Id::unique("text-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(area_pos)
            .show(ctx, |ui| {
                // Dedicated move/commit bar above the text input so dragging
                // inside `TextEdit` still selects characters normally.
                let bar_resp = egui::Frame::NONE
                    .fill(th.panel)
                    .corner_radius(rr(7))
                    .stroke(Stroke::new(1.0, th.border))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (grip_rect, grip_resp) =
                                ui.allocate_exact_size(Vec2::new(72.0, 20.0), Sense::drag());
                            if grip_resp.hovered() || grip_resp.dragged() {
                                ui.ctx().set_cursor_icon(if grip_resp.dragged() {
                                    egui::CursorIcon::Grabbing
                                } else {
                                    egui::CursorIcon::Grab
                                });
                            }
                            let dots_origin =
                                Pos2::new(grip_rect.left() + 4.0, grip_rect.center().y - 3.5);
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
                                Pos2::new(grip_rect.left() + 14.0, grip_rect.center().y),
                                Align2::LEFT_CENTER,
                                "Move text",
                                FontId::proportional(11.0),
                                th.text_dim,
                            );
                            if grip_resp.dragged() {
                                drag_delta += grip_resp.drag_delta();
                            }
                            if ui
                                .small_button(
                                    egui::RichText::new("Apply").color(th.accent).strong(),
                                )
                                .on_hover_text("Rasterise text onto canvas (Ctrl+Enter)")
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

                ui.add_space(4.0);
                let response = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .desired_rows(1)
                        .desired_width(editor_width)
                        .hint_text("Type on canvas…")
                        .font(FontId::proportional(
                            (snapshot.size * zoom).clamp(10.0, 240.0),
                        ))
                        .text_color(Color32::TRANSPARENT),
                );
                // Outline the active text box subtly so the placement is clear.
                ui.painter().rect_stroke(
                    response.rect.expand(3.0),
                    rr(4),
                    Stroke::new(1.0, th.accent.gamma_multiply(0.55)),
                    StrokeKind::Outside,
                );
                editor_rect = Some(bar_resp.rect.union(response.rect.expand(4.0)));
                if want_focus {
                    response.request_focus();
                }
                text_changed = response.changed();
            });

        self.edit.text_editor_rect = editor_rect;

        if cancel_clicked {
            self.cancel_current();
            return;
        }
        if apply_clicked {
            self.commit_current();
            return;
        }

        let mut needs_refresh = false;
        let scale = zoom.max(0.0001);
        let max_x = self.doc.w.saturating_sub(4) as f32;
        let max_y = self.doc.h.saturating_sub(4) as f32;
        if let Some(live) = self.text_session_mut() {
            live.just_created = false;
            if drag_delta != Vec2::ZERO {
                let nx = (live.pos.0 + drag_delta.x / scale).clamp(0.0, max_x);
                let ny = (live.pos.1 + drag_delta.y / scale).clamp(0.0, max_y);
                if (nx - live.pos.0).abs() > 1e-4 || (ny - live.pos.1).abs() > 1e-4 {
                    live.pos = (nx, ny);
                    needs_refresh = true;
                }
            }
            if text_changed || text != live.text {
                live.text = text;
                needs_refresh = true;
            }
        }
        if needs_refresh {
            self.refresh_text();
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
