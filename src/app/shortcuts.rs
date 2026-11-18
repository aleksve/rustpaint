//! Keyboard, clipboard, and command-key dispatch for the application shell.

use eframe::egui::{self, Color32, Context, Key};

use super::{App, Dialog};
use crate::tools::Tool;

pub(super) fn handle(app: &mut App, ctx: &Context) {
    // Read the modifiers that came *with* each key event: this keeps
    // shortcuts working even when the modifier state itself is stale
    // (remote desktops, synthetic input, IMEs, ...).
    let pressed: Vec<(Key, egui::Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    let with_cmd = |k: Key| -> Option<egui::Modifiers> {
        pressed
            .iter()
            .find(|(pk, m)| *pk == k && m.command)
            .map(|(_, m)| *m)
    };
    let plain = |k: Key| -> Option<egui::Modifiers> {
        pressed
            .iter()
            .find(|(pk, m)| *pk == k && !m.command)
            .map(|(_, m)| *m)
    };

    let (ev_copy, ev_cut, ev_paste) = ctx.input(|i| {
        let (mut c, mut x, mut p) = (false, false, false);
        for e in &i.events {
            match e {
                egui::Event::Copy => c = true,
                egui::Event::Cut => x = true,
                egui::Event::Paste(_) => p = true,
                _ => {}
            }
        }
        (c, x, p)
    });

    let typing = ctx.wants_keyboard_input();

    if pressed.iter().any(|(k, _)| *k == Key::Escape) {
        if app.edit.text.is_some() || !app.edit.polygon.is_empty() || app.edit.session.is_some() {
            app.cancel_current();
        } else if app.edit.selection.is_some() {
            app.deselect();
        } else if app.dialog.is_some() {
            app.dialog = None;
        } else if app.file_dialog.is_some() {
            app.file_dialog = None;
        }
    }

    if app.edit.text.is_some() && with_cmd(Key::Enter).is_some() {
        app.commit_current();
    }

    if typing || app.modal_open() {
        return;
    }

    if ev_copy {
        app.copy_selection();
        return;
    }
    if ev_cut {
        app.cut_selection();
        return;
    }
    if ev_paste {
        app.paste();
        return;
    }

    // ---- with Ctrl / Cmd
    if with_cmd(Key::N).is_some() {
        app.ask_new();
    } else if with_cmd(Key::O).is_some() {
        app.ask_open(ctx);
    } else if let Some(m) = with_cmd(Key::S) {
        if m.shift {
            app.save_dialog(ctx);
        } else {
            app.quick_save(ctx);
        }
    } else if let Some(m) = with_cmd(Key::Z) {
        if m.shift {
            app.redo();
        } else {
            app.undo();
        }
    } else if with_cmd(Key::Y).is_some() {
        app.redo();
    } else if with_cmd(Key::A).is_some() {
        app.select_all();
    } else if with_cmd(Key::C).is_some() {
        app.copy_selection();
    } else if with_cmd(Key::X).is_some() {
        app.cut_selection();
    } else if with_cmd(Key::V).is_some() {
        app.paste();
    } else if with_cmd(Key::D).is_some() {
        app.deselect();
    } else if with_cmd(Key::Num0).is_some() {
        let r = app.last_view_rect;
        app.set_zoom(1.0, r);
    } else if with_cmd(Key::Num9).is_some() {
        let r = app.last_view_rect;
        app.fit_view(r);
    } else if with_cmd(Key::Plus).is_some() || with_cmd(Key::Equals).is_some() {
        let r = app.last_view_rect;
        app.zoom_step(1.0, r);
    } else if with_cmd(Key::Minus).is_some() {
        let r = app.last_view_rect;
        app.zoom_step(-1.0, r);
    } else if with_cmd(Key::E).is_some() {
        app.resize_w = app.doc.w as u32;
        app.resize_h = app.doc.h as u32;
        app.dialog = Some(Dialog::Resize);
    }
    if pressed.iter().any(|(_, m)| m.command) {
        return;
    }

    // ---- plain keys
    for t in Tool::ALL {
        if plain(t.shortcut_key()).is_some() {
            app.set_tool(t);
        }
    }

    if plain(Key::Delete).is_some() || plain(Key::Backspace).is_some() {
        app.delete_selection();
    }
    if plain(Key::OpenBracket).is_some() {
        app.opts.size = (app.opts.size - if app.opts.size > 10.0 { 5.0 } else { 1.0 }).max(1.0);
    }
    if plain(Key::CloseBracket).is_some() {
        app.opts.size = (app.opts.size + if app.opts.size >= 10.0 { 5.0 } else { 1.0 }).min(300.0);
    }
    if plain(Key::X).is_some() {
        app.colors.swap();
    }
    if plain(Key::D).is_some() {
        app.colors.primary = Color32::from_rgb(0x1a, 0x1c, 0x22);
        app.colors.secondary = Color32::WHITE;
    }
    if plain(Key::Enter).is_some() {
        if !app.edit.polygon.is_empty() {
            app.finish_polygon();
        } else if app.edit.floating.is_some() {
            app.commit_current();
        }
    }
    for (k, m) in [
        (Key::ArrowLeft, (-1, 0)),
        (Key::ArrowRight, (1, 0)),
        (Key::ArrowUp, (0, -1)),
        (Key::ArrowDown, (0, 1)),
    ] {
        if let Some(md) = plain(k) {
            let step = if md.shift { 10 } else { 1 };
            app.nudge_selection(m.0 * step, m.1 * step);
        }
    }
}
