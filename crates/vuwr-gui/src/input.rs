//! Keys to commands, for the GUI.
//!
//! The GUI's counterpart to the TUI's keymap. Bindings match where they
//! can, and add the platform shortcuts people expect in a window
//! (Ctrl/Cmd-S, Ctrl/Cmd-Z) — the same commands either way, so behaviour
//! cannot differ between frontends.

use eframe::egui::{self, Key, Modifiers};
use vuwr_core::Command;

use crate::VuwrApp;

/// The command a key press means, or `None` if it is not bound.
///
/// `pending_g` is true when the previous key was a bare `g`, which is how
/// `gg` works; the caller owns that state, as in the TUI.
pub fn command_for(key: Key, mods: Modifiers, pending_g: bool) -> Option<Command> {
    if pending_g && key == Key::G && !mods.command {
        return Some(Command::GoTop);
    }

    // Platform shortcuts first: Cmd on macOS, Ctrl elsewhere.
    if mods.command {
        return match key {
            Key::S => Some(Command::Save),
            Key::Z if mods.shift => Some(Command::Redo),
            Key::Z => Some(Command::Undo),
            Key::F => Some(Command::Find),
            Key::G if mods.shift => Some(Command::FindPrev),
            Key::G => Some(Command::FindNext),
            Key::Q => Some(Command::Quit),
            Key::E => Some(Command::PrintMarks),
            Key::R => Some(Command::Redo),
            Key::D => Some(Command::HalfPageDown),
            Key::U => Some(Command::HalfPageUp),
            Key::B => Some(Command::PageUp),
            _ => None,
        };
    }

    Some(match key {
        Key::ArrowLeft | Key::H => Command::MoveLeft,
        Key::ArrowRight | Key::L => Command::MoveRight,
        Key::ArrowUp | Key::K => Command::MoveUp,
        Key::ArrowDown | Key::J => Command::MoveDown,
        Key::PageDown | Key::Space => Command::PageDown,
        Key::PageUp | Key::B => Command::PageUp,
        Key::D => Command::HalfPageDown,
        Key::Home => Command::GoRowStart,
        Key::End => Command::GoRowEnd,
        Key::G if mods.shift => Command::GoBottom,
        Key::Tab => Command::CycleView,
        Key::Num1 => Command::ViewTable,
        Key::Num2 => Command::ViewTree,
        Key::Num3 => Command::ViewText,
        Key::Enter => Command::DrillDown,
        Key::Escape => Command::DrillUp,
        Key::I => Command::EditCell,
        Key::C => Command::ReplaceCell,
        Key::U => Command::Undo,
        Key::Slash => Command::Find,
        Key::N if mods.shift => Command::FindPrev,
        Key::N => Command::FindNext,
        Key::R => Command::ClearFilter,
        Key::M if mods.shift => Command::ClearMarks,
        Key::M => Command::ToggleMark,
        Key::F => Command::FreezeColumns,
        Key::Q => Command::Quit,
        Key::Questionmark => Command::Help,
        _ => return None,
    })
}

/// The keys shown in the help window. Native shortcuts where they exist,
/// since that is what a windowed app's users will reach for.
pub fn keys_for(cmd: Command) -> &'static str {
    match cmd {
        Command::MoveLeft => "← / h",
        Command::MoveRight => "→ / l",
        Command::MoveUp => "↑ / k",
        Command::MoveDown => "↓ / j",
        Command::PageDown => "Space / PgDn",
        Command::PageUp => "b / PgUp",
        Command::HalfPageDown => "d",
        Command::HalfPageUp => "Ctrl-U",
        Command::GoTop => "gg",
        Command::GoBottom => "G",
        Command::GoRowStart => "Home",
        Command::GoRowEnd => "End",
        Command::CycleView => "Tab",
        Command::ViewTable => "1",
        Command::ViewTree => "2",
        Command::ViewText => "3",
        Command::DrillDown => "Enter",
        Command::DrillUp => "Esc",
        Command::EditCell => "i / Enter",
        Command::ReplaceCell => "c",
        Command::Undo => "Ctrl-Z / u",
        Command::Redo => "Ctrl-Shift-Z",
        Command::Find => "Ctrl-F / /",
        Command::FindNext => "n",
        Command::FindPrev => "N",
        Command::Filter => "&",
        Command::ClearFilter => "r",
        Command::ToggleMark => "m",
        Command::ClearMarks => "M",
        Command::PrintMarks => "Ctrl-E",
        Command::FreezeColumns => "f",
        Command::Save => "Ctrl-S",
        Command::Quit => "Ctrl-Q",
        Command::ForceQuit => "menu",
        Command::SaveAndQuit => "menu",
        Command::OpenPalette => ":",
        Command::Help => "?",
        Command::ToggleHints => "H",
    }
}

/// Feed this frame's input to the session.
pub fn handle(app: &mut VuwrApp, ctx: &egui::Context) {
    // Text being entered wins: a `q` typed into a search box is a letter,
    // not a quit. The TUI makes the same distinction.
    if app.session().is_entering_text() {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    for c in text.chars() {
                        app.session_mut().input_char(c);
                    }
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    Key::Enter => {
                        let effect = app.session_mut().input_submit();
                        app.apply_effect(effect, ctx);
                    }
                    Key::Escape => app.session_mut().input_cancel(),
                    Key::Backspace => app.session_mut().input_backspace(),
                    _ => {}
                },
                _ => {}
            }
        }
        return;
    }

    let mut pending_g = app.take_pending_g();
    let events = ctx.input(|i| i.events.clone());
    for event in events {
        let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        else {
            continue;
        };
        // A bare `g` waits to see whether another `g` follows.
        if key == Key::G && !modifiers.shift && !modifiers.command && !pending_g {
            pending_g = true;
            app.set_pending_g(true);
            continue;
        }
        if let Some(cmd) = command_for(key, modifiers, pending_g) {
            app.run(cmd, ctx);
        }
        pending_g = false;
        app.set_pending_g(false);
    }
}
