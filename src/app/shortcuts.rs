//! Keyboard, clipboard, and command-key dispatch for the application shell.

use eframe::egui::{self, Context, Key};

use super::{App, Dialog};
use crate::tools::Tool;

#[derive(Clone, Copy)]
struct KeyPress {
    key: Key,
    modifiers: egui::Modifiers,
    repeat: bool,
}

pub(super) fn handle(app: &mut App, ctx: &Context, frame: Option<&eframe::Frame>) {
    if app.io_job.is_some() {
        return;
    }

    // Read the modifiers carried by each key event so shortcuts stay reliable
    // across remote desktops, IMEs, and synthetic input.
    let pressed: Vec<KeyPress> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match *e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => Some(KeyPress {
                    key,
                    modifiers,
                    repeat,
                }),
                _ => None,
            })
            .collect()
    });

    let cmd_key = |k: Key, allow_repeat: bool| -> Option<egui::Modifiers> {
        pressed
            .iter()
            .find(|ev| ev.key == k && ev.modifiers.command && (allow_repeat || !ev.repeat))
            .map(|ev| ev.modifiers)
    };
    // Plain tool/editor keys must not fire when Ctrl, Cmd, or Alt is held
    // (avoiding collisions with Alt menu mnemonics or macOS Ctrl shortcuts).
    let plain_key = |k: Key, allow_repeat: bool| -> Option<egui::Modifiers> {
        pressed
            .iter()
            .find(|ev| {
                ev.key == k
                    && !ev.modifiers.command
                    && !ev.modifiers.ctrl
                    && !ev.modifiers.alt
                    && (allow_repeat || !ev.repeat)
            })
            .map(|ev| ev.modifiers)
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

    if pressed.iter().any(|ev| ev.key == Key::Escape && !ev.repeat) {
        if app.modal_open() {
            // Always clear `pending_after_save` alongside any cancelled dialog.
            app.cancel_dialogs();
            return;
        } else if app.edit.session.is_some() {
            app.cancel_current();
            return;
        } else if app.edit.selection.is_some() {
            app.deselect();
            return;
        }
    }

    if app.edit.is_editing_text() && cmd_key(Key::Enter, false).is_some() {
        app.commit_current();
        return;
    }

    let typing = ctx.egui_wants_keyboard_input();
    if typing || app.modal_open() {
        return;
    }

    if plain_key(Key::F1, false).is_some() {
        app.dialog = Some(Dialog::Shortcuts);
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
        app.paste_with_toast(ctx);
        return;
    }

    // ---- with Ctrl / Cmd
    if cmd_key(Key::N, false).is_some() {
        app.ask_new();
    } else if cmd_key(Key::O, false).is_some() {
        app.ask_open(ctx, frame);
    } else if let Some(m) = cmd_key(Key::S, false) {
        if m.shift {
            app.save_dialog(ctx, frame);
        } else {
            app.quick_save(ctx, frame);
        }
    } else if let Some(m) = cmd_key(Key::Z, true) {
        if m.shift {
            app.redo();
        } else {
            app.undo();
        }
    } else if cmd_key(Key::Y, true).is_some() {
        app.redo();
    } else if cmd_key(Key::A, false).is_some() {
        app.select_all();
    } else if cmd_key(Key::C, false).is_some() {
        app.copy_selection();
    } else if cmd_key(Key::X, false).is_some() {
        app.cut_selection();
    } else if cmd_key(Key::V, false).is_some() {
        app.paste_with_toast(ctx);
    } else if cmd_key(Key::D, false).is_some() {
        app.deselect();
    } else if cmd_key(Key::Num0, false).is_some() {
        let r = app.last_view_rect;
        app.set_zoom(1.0, r);
    } else if cmd_key(Key::Num9, false).is_some() {
        let r = app.last_view_rect;
        app.fit_view(r);
    } else if cmd_key(Key::Plus, true).is_some() || cmd_key(Key::Equals, true).is_some() {
        let r = app.last_view_rect;
        app.zoom_step(1.0, r);
    } else if cmd_key(Key::Minus, true).is_some() {
        let r = app.last_view_rect;
        app.zoom_step(-1.0, r);
    } else if cmd_key(Key::Quote, false).is_some() {
        app.view.grid = !app.view.grid;
    } else if cmd_key(Key::E, false).is_some() {
        app.open_resize_dialog();
    }
    if pressed.iter().any(|ev| ev.modifiers.command) {
        return;
    }

    // ---- plain keys
    for t in Tool::ALL {
        if plain_key(t.shortcut_key(), false).is_some() {
            app.set_tool(t);
        }
    }

    if plain_key(Key::Delete, false).is_some() || plain_key(Key::Backspace, false).is_some() {
        app.delete_selection();
    }
    if app.tool.uses_size() {
        let max = app.tool.max_size();
        if plain_key(Key::OpenBracket, true).is_some() {
            let step = if app.opts.size > 10.0 { 5.0 } else { 1.0 };
            app.opts.size = (app.opts.size - step).clamp(1.0, max);
            app.tool_sizes.insert(app.tool, app.opts.size);
        }
        if plain_key(Key::CloseBracket, true).is_some() {
            let step = if app.opts.size >= 10.0 { 5.0 } else { 1.0 };
            app.opts.size = (app.opts.size + step).clamp(1.0, max);
            app.tool_sizes.insert(app.tool, app.opts.size);
        }
    }
    if plain_key(Key::X, false).is_some() {
        app.colors.swap();
        app.sync_live_text_color_and_opacity();
    }
    if plain_key(Key::D, false).is_some() {
        app.colors.reset();
        app.sync_live_text_color_and_opacity();
    }
    if plain_key(Key::Enter, false).is_some() {
        if app.edit.is_drawing_polygon() {
            app.finish_polygon();
        } else if app.edit.is_floating() {
            app.commit_current();
        }
    }
    for (k, dir) in [
        (Key::ArrowLeft, (-1, 0)),
        (Key::ArrowRight, (1, 0)),
        (Key::ArrowUp, (0, -1)),
        (Key::ArrowDown, (0, 1)),
    ] {
        if let Some(md) = plain_key(k, true) {
            let step = if md.shift { 10 } else { 1 };
            app.nudge_selection(dir.0 * step, dir.1 * step);
        }
    }
}
