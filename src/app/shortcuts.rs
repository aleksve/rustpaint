//! Keyboard, IME, clipboard, and command-key dispatch for the application shell.

use eframe::egui::{self, Context, ImeEvent, Key};

use super::{App, Dialog};
use crate::paint::TextSession;
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

    // Route all text, IME, navigation, and clipboard input to the live canvas
    // text session whenever it is active and no modal/sidebar field has focus.
    if app.canvas_text_captures_keys() {
        handle_canvas_text(app, ctx, frame);
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
        } else if app.hex_focused {
            // Leave any live canvas text intact and drop focus back to it.
            app.focus_canvas_text(ctx);
            return;
        } else if app.font_filter_focused {
            // Let egui close the font popup without discarding canvas text.
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

    // Never fire single-key tool shortcuts while a modal, sidebar field, or
    // canvas text session is open.
    let typing = ctx.egui_wants_keyboard_input() || app.edit.is_editing_text();
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

/// Dispatch keyboard, text, IME, and clipboard events to the active canvas text
/// session using platform-native editing bindings (macOS Option/Command/Emacs
/// bindings and Windows/Linux Ctrl/Home/End bindings).
fn handle_canvas_text(app: &mut App, ctx: &Context, frame: Option<&eframe::Frame>) {
    let (now, events) = ctx.input(|i| (i.time, i.events.clone()));
    let is_mac = cfg!(target_os = "macos");

    let mut dirty_mask = false;
    let mut moved_caret = false;
    let mut copy_out: Option<String> = None;
    let mut commit = false;
    let mut cancel = false;
    let mut save_as: Option<bool> = None;
    let mut post_zoom: Option<f32> = None;
    let mut fit_view = false;
    let mut reset_zoom = false;
    let mut open_new = false;
    let mut open_file = false;

    for event in events {
        match event {
            egui::Event::Text(s) => {
                // `Key::Enter` and `Key::Tab` handle newlines/tabs directly;
                // strip them here so no platform integration inserts them twice.
                let filtered: String = s
                    .chars()
                    .filter(|&c| c != '\n' && c != '\r' && c != '\t')
                    .collect();
                if !filtered.is_empty()
                    && let Some(live) = app.text_session_mut()
                    && live.insert_str(&filtered)
                {
                    dirty_mask = true;
                    moved_caret = true;
                }
            }
            egui::Event::Paste(s) => {
                if let Some(live) = app.text_session_mut() {
                    live.checkpoint_undo();
                    if live.insert_str(&s) {
                        live.checkpoint_undo();
                        dirty_mask = true;
                        moved_caret = true;
                    }
                }
            }
            egui::Event::Copy => {
                if let Some(live) = app.text_session() {
                    let sel = live.selected_text();
                    if !sel.is_empty() {
                        copy_out = Some(sel.to_owned());
                    }
                }
            }
            egui::Event::Cut => {
                if let Some(live) = app.text_session_mut() {
                    let sel = live.selected_text().to_owned();
                    if !sel.is_empty() {
                        copy_out = Some(sel);
                        if live.delete_selection() {
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                }
            }
            egui::Event::Ime(ime) => match ime {
                #[allow(deprecated)]
                ImeEvent::Enabled | ImeEvent::Disabled => {}
                ImeEvent::Preedit { text, .. } => {
                    if text != "\n"
                        && text != "\r"
                        && let Some(live) = app.text_session_mut()
                        && live.set_preedit(&text)
                    {
                        dirty_mask = true;
                        moved_caret = true;
                    }
                }
                ImeEvent::Commit(text) => {
                    if text != "\n"
                        && text != "\r"
                        && let Some(live) = app.text_session_mut()
                        && (!text.is_empty() || !live.preedit.is_empty())
                        && live.insert_str(&text)
                    {
                        dirty_mask = true;
                        moved_caret = true;
                    }
                }
                ImeEvent::DeleteSurrounding {
                    before_chars,
                    after_chars,
                } => {
                    if let Some(live) = app.text_session_mut()
                        && live.delete_surrounding_chars(before_chars, after_chars)
                    {
                        dirty_mask = true;
                        moved_caret = true;
                    }
                }
            },
            egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } => {
                let by_word = if is_mac {
                    modifiers.alt && !modifiers.command && !modifiers.ctrl
                } else {
                    (modifiers.ctrl || modifiers.command) && !modifiers.alt
                };
                match key {
                    Key::Escape if !repeat => {
                        if let Some(live) = app.text_session_mut()
                            && !live.preedit.is_empty()
                        {
                            live.preedit.clear();
                            dirty_mask = true;
                            moved_caret = true;
                        } else {
                            cancel = true;
                            break;
                        }
                    }
                    Key::Enter if modifiers.command || modifiers.ctrl => {
                        if !repeat {
                            commit = true;
                            break;
                        }
                    }
                    Key::Enter => {
                        if let Some(live) = app.text_session_mut() {
                            let _ = live.commit_preedit();
                            if live.insert_str("\n") {
                                dirty_mask = true;
                                moved_caret = true;
                            }
                        }
                    }
                    Key::Tab if !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
                        // Drop any widget Tab-focus egui may have assigned so
                        // toolbar buttons don't light up while typing.
                        ctx.memory_mut(|m| {
                            if let Some(id) = m.focused() {
                                m.surrender_focus(id);
                            }
                        });
                        if let Some(live) = app.text_session_mut() {
                            let _ = live.commit_preedit();
                            if live.insert_str("\t") {
                                dirty_mask = true;
                                moved_caret = true;
                            }
                        }
                    }
                    Key::Backspace => {
                        let to_line_start = is_mac && modifiers.command;
                        if let Some(live) = app.text_session_mut()
                            && live.backspace(by_word, to_line_start)
                        {
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                    Key::Delete => {
                        let to_line_end = is_mac && modifiers.command;
                        if let Some(live) = app.text_session_mut()
                            && live.delete_forward(by_word, to_line_end)
                        {
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                    Key::ArrowLeft => {
                        let to_line_start = is_mac && modifiers.command && !modifiers.alt;
                        if let Some(live) = app.text_session_mut() {
                            let had_pre = !live.preedit.is_empty();
                            live.move_left(modifiers.shift, by_word, to_line_start);
                            dirty_mask |= had_pre;
                            moved_caret = true;
                        }
                    }
                    Key::ArrowRight => {
                        let to_line_end = is_mac && modifiers.command && !modifiers.alt;
                        if let Some(live) = app.text_session_mut() {
                            let had_pre = !live.preedit.is_empty();
                            live.move_right(modifiers.shift, by_word, to_line_end);
                            dirty_mask |= had_pre;
                            moved_caret = true;
                        }
                    }
                    Key::ArrowUp | Key::ArrowDown => {
                        let delta = if key == Key::ArrowUp { -1 } else { 1 };
                        if is_mac && modifiers.command {
                            if let Some(live) = app.text_session_mut() {
                                dirty_mask |= live.commit_preedit();
                                let end = if delta < 0 { 0 } else { live.text.len() };
                                live.set_cursor(end, modifiers.shift, false);
                                moved_caret = true;
                            }
                        } else {
                            if let Some(live) = app.text_session_mut()
                                && live.commit_preedit()
                            {
                                dirty_mask = true;
                            }
                            if let Some(layout) = app.text_layout()
                                && let Some(live) = app.text_session_mut()
                            {
                                let mut pref = live.preferred_x;
                                let next = layout.vertical_move(live.cursor, delta, &mut pref);
                                live.set_cursor(next, modifiers.shift, true);
                                live.preferred_x = pref;
                                moved_caret = true;
                            }
                        }
                    }
                    Key::Home => {
                        let doc_edge = modifiers.command || (!is_mac && modifiers.ctrl);
                        if let Some(live) = app.text_session_mut() {
                            dirty_mask |= live.commit_preedit();
                            if doc_edge {
                                live.set_cursor(0, modifiers.shift, false);
                            } else {
                                live.move_left(modifiers.shift, false, true);
                            }
                            moved_caret = true;
                        }
                    }
                    Key::End => {
                        let doc_edge = modifiers.command || (!is_mac && modifiers.ctrl);
                        if let Some(live) = app.text_session_mut() {
                            dirty_mask |= live.commit_preedit();
                            if doc_edge {
                                let end = live.text.len();
                                live.set_cursor(end, modifiers.shift, false);
                            } else {
                                live.move_right(modifiers.shift, false, true);
                            }
                            moved_caret = true;
                        }
                    }
                    // macOS standard Cocoa Ctrl+A / Ctrl+E / Ctrl+K line helpers.
                    Key::A if is_mac && modifiers.ctrl && !modifiers.command => {
                        if let Some(live) = app.text_session_mut() {
                            dirty_mask |= live.commit_preedit();
                            live.move_left(modifiers.shift, false, true);
                            moved_caret = true;
                        }
                    }
                    Key::E if is_mac && modifiers.ctrl && !modifiers.command => {
                        if let Some(live) = app.text_session_mut() {
                            dirty_mask |= live.commit_preedit();
                            live.move_right(modifiers.shift, false, true);
                            moved_caret = true;
                        }
                    }
                    Key::K if is_mac && modifiers.ctrl && !modifiers.command => {
                        if let Some(live) = app.text_session_mut()
                            && live.delete_forward(false, true)
                        {
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                    // Command / Ctrl shortcuts safe inside live text editing.
                    Key::Z if modifiers.command => {
                        if let Some(live) = app.text_session_mut() {
                            let ok = if modifiers.shift {
                                live.redo_text()
                            } else {
                                live.undo_text()
                            };
                            if ok {
                                dirty_mask = true;
                                moved_caret = true;
                            }
                        }
                    }
                    Key::Y if modifiers.command => {
                        if let Some(live) = app.text_session_mut()
                            && live.redo_text()
                        {
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                    Key::A if modifiers.command => {
                        if let Some(live) = app.text_session_mut() {
                            dirty_mask |= TextSession::commit_preedit(live);
                            live.select_all();
                            moved_caret = true;
                        }
                    }
                    Key::B if modifiers.command && !repeat => {
                        app.opts.bold = !app.opts.bold;
                        let bold = app.opts.bold;
                        if let Some(live) = app.text_session_mut() {
                            live.bold = bold;
                            dirty_mask = true;
                            moved_caret = true;
                        }
                    }
                    Key::S if modifiers.command && !repeat => {
                        save_as = Some(modifiers.shift);
                        break;
                    }
                    Key::N if modifiers.command && !repeat => {
                        open_new = true;
                        break;
                    }
                    Key::O if modifiers.command && !repeat => {
                        open_file = true;
                        break;
                    }
                    Key::Plus | Key::Equals if modifiers.command => {
                        post_zoom = Some(1.0);
                    }
                    Key::Minus if modifiers.command => {
                        post_zoom = Some(-1.0);
                    }
                    Key::Num0 if modifiers.command && !repeat => {
                        reset_zoom = true;
                    }
                    Key::Num9 if modifiers.command && !repeat => {
                        fit_view = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if let Some(txt) = copy_out {
        ctx.copy_text(txt);
    }
    if cancel {
        app.cancel_current();
        return;
    }
    if let Some(live) = app.text_session_mut() {
        if moved_caret {
            live.blink_origin = now;
        }
        live.feed_undo(now);
    }
    if dirty_mask {
        app.refresh_text();
    }
    if commit {
        app.commit_current();
        return;
    }
    if let Some(shift) = save_as {
        if shift {
            app.save_dialog(ctx, frame);
        } else {
            app.quick_save(ctx, frame);
        }
        return;
    }
    if open_new {
        app.commit_current();
        app.ask_new();
        return;
    }
    if open_file {
        app.commit_current();
        app.ask_open(ctx, frame);
        return;
    }
    let view_rect = app.last_view_rect;
    if let Some(step) = post_zoom {
        app.zoom_step(step, view_rect);
    } else if reset_zoom {
        app.set_zoom(1.0, view_rect);
    } else if fit_view {
        app.fit_view(view_rect);
    }
}
