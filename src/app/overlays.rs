//! Foreground overlays: drag-and-drop feedback, text editing, dialogs, and toasts.

use eframe::egui::{self, Align2, Color32, Context, FontId, Rounding, Stroke, Vec2};

use super::App;
use crate::filedlg::FileAction;
use crate::widgets::{self, blend};

impl App {
    pub(crate) fn overlays(&mut self, ctx: &Context) {
        let th = self.theme;

        // Drop target hint.
        let dragging_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if dragging_file {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop-hint"),
            ));
            painter.rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(90));
            let inner = screen.shrink(18.0);
            painter.rect_stroke(inner, Rounding::same(16.0), Stroke::new(2.0_f32, th.accent));
            painter.text(
                screen.center(),
                Align2::CENTER_CENTER,
                "Drop an image to open it",
                FontId::proportional(20.0),
                th.on_accent,
            );
            ctx.request_repaint();
        }

        self.text_overlay(ctx, th);
        self.show_dialogs(ctx);

        // The file dialog owns its state while it is open. Taking it out avoids
        // borrowing the App while the dialog builds its egui window.
        if let Some(mut file_dialog) = self.file_dialog.take() {
            let action = file_dialog.show(ctx, &th);
            if matches!(action, FileAction::None) {
                self.file_dialog = Some(file_dialog);
            } else {
                self.handle_file_action(action, ctx);
            }
        }

        self.toast_overlay(ctx, th);
    }

    fn text_overlay(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let Some(text_session) = self.edit.text.clone() else {
            return;
        };

        let screen = self.image_to_screen(egui::Pos2::new(text_session.pos.0, text_session.pos.1));
        let mut text = text_session.text;
        let mut changed = false;
        let mut commit = false;
        let mut cancel = false;
        let want_focus = text_session.just_created;

        egui::Area::new(egui::Id::new("text-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::new(
                screen.x.max(8.0),
                (screen.y + 18.0 * self.scale).max(8.0),
            ))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(th.panel)
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(1.0_f32, th.accent))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .desired_rows(2)
                                .desired_width(244.0)
                                .hint_text("Type text…")
                                .font(FontId::proportional(13.0)),
                        );
                        if want_focus {
                            response.request_focus();
                        }
                        changed = response.changed();
                        ui.horizontal(|ui| {
                            if widgets::button(ui, &th, "Apply", true).clicked() {
                                commit = true;
                            }
                            if widgets::button(ui, &th, "Cancel", false).clicked() {
                                cancel = true;
                            }
                        });
                    });
            });

        if let Some(session) = self.edit.text.as_mut() {
            session.just_created = false;
            if text != session.text {
                session.text = text;
                changed = true;
            }
        }
        if changed {
            self.refresh_text();
        }
        if commit {
            self.commit_current();
        }
        if cancel {
            self.cancel_current();
        }
    }

    fn toast_overlay(&mut self, ctx: &Context, th: crate::theme::Theme) {
        let Some((message, until)) = self.toast_msg.clone() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now > until {
            self.toast_msg = None;
            return;
        }

        egui::Area::new(egui::Id::new("toast"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -44.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(blend(th.panel_alt, th.accent, 0.08))
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(1.0_f32, th.border))
                    .inner_margin(egui::Margin::symmetric(16.0, 9.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(message).size(12.5).color(th.text));
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
}
