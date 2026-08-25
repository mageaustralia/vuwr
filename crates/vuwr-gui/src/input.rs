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
            Key::O => Some(Command::Open),
            Key::S if mods.shift => Some(Command::SaveAs),
            Key::S => Some(Command::Save),
            Key::C => Some(Command::Copy),
            Key::V => Some(Command::Paste),
            Key::Z if mods.shift => Some(Command::Redo),
            Key::Z => Some(Command::Undo),
            Key::F => Some(Command::Find),
            Key::G if mods.shift => Some(Command::FindPrev),
            Key::G => Some(Command::FindNext),
            Key::Q => Some(Command::Quit),
            Key::E => Some(Command::PrintMarks),
            Key::R => Some(Command::Redo),
            Key::Y => Some(Command::Redo),
            Key::D => Some(Command::HalfPageDown),
            // Formatting shortcuts follow JSON Editor Online's, which is
            // what people coming from a browser will already have in
            // their fingers.
            Key::I if mods.shift => Some(Command::FormatCompact),
            Key::I => Some(Command::FormatPretty),
            Key::J => Some(Command::FormatSmart),
            Key::U => Some(Command::HalfPageUp),
            Key::B => Some(Command::PageUp),
            _ => None,
        };
    }

    Some(match key {
        // Shift-guarded arms come first: a plain `Key::H` arm would match
        // Shift+H too and swallow it.
        Key::H if mods.shift => Command::ToggleHints,
        Key::V if mods.shift => Command::ToggleDetail,
        Key::E if mods.shift => Command::ToggleDecoded,
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
        Key::F2 => Command::EditLarge,
        Key::C => Command::ReplaceCell,
        Key::R if mods.shift => Command::RenameKey,
        Key::U => Command::Undo,
        Key::Slash => Command::Find,
        Key::Colon => Command::OpenPalette,
        Key::N if mods.shift => Command::FindPrev,
        Key::N => Command::FindNext,
        Key::R => Command::ClearFilter,
        Key::M if mods.shift => Command::ClearMarks,
        Key::M => Command::ToggleMark,
        Key::F => Command::FreezeColumns,
        Key::Equals => Command::AutoSizeColumns,
        Key::S if mods.shift => Command::SortNatural,
        Key::S => Command::Sort,
        Key::Q => Command::Quit,
        Key::Questionmark => Command::Help,
        _ => return None,
    })
}

/// The command a typed character means.
///
/// Punctuation that egui has no `Key` variant for arrives only as text.
/// Only characters with no key binding belong here: a character that is
/// both would fire its command twice, which silently cancels a toggle.
pub fn command_for_char(c: char) -> Option<Command> {
    match c {
        '&' => Some(Command::Filter),
        '>' => Some(Command::WidenColumn),
        '<' => Some(Command::NarrowColumn),
        _ => None,
    }
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
        Command::ExpandAll => "toolbar",
        Command::CollapseAll => "toolbar",
        Command::ViewTable => "1",
        Command::ViewTree => "2",
        Command::ViewText => "3",
        Command::DrillDown => "Enter",
        Command::DrillUp => "Esc",
        Command::EditCell => "i / Enter",
        Command::ReplaceCell => "c",
        Command::RenameKey => "double-click / R",
        Command::EditLarge => "F2",
        Command::Copy => "Ctrl-C",
        Command::CopyRow => "toolbar",
        Command::Paste => "Ctrl-V",
        Command::Undo => "Ctrl-Z / u",
        Command::Redo => "Ctrl-Shift-Z / Ctrl-Y",
        Command::Find => "Ctrl-F / /",
        Command::FindNext => "n",
        Command::FindPrev => "N",
        Command::Filter => "&",
        Command::ClearFilter => "r",
        Command::Sort => "s",
        Command::SortNumeric => "toolbar",
        Command::SortNatural => "S",
        Command::FormatPretty => "Ctrl-I",
        Command::FormatSmart => "Ctrl-J",
        Command::FormatCompact => "Ctrl-Shift-I",
        Command::ToggleMark => "m",
        Command::ClearMarks => "M",
        Command::PrintMarks => "Ctrl-E",
        Command::FreezeColumns => "f",
        Command::WidenColumn => "drag / >",
        Command::NarrowColumn => "drag / <",
        Command::AutoSizeColumns => "=",
        Command::Open => "Ctrl-O",
        Command::Save => "Ctrl-S",
        Command::SaveAs => "Ctrl-Shift-S",
        Command::Quit => "Ctrl-Q",
        Command::ForceQuit => "menu",
        Command::SaveAndQuit => "menu",
        Command::OpenPalette => ":",
        Command::Help => "?",
        Command::ToggleHints => "H",
        Command::ToggleDetail => "V",
        Command::ToggleDecoded => "E",
    }
}

/// Take clipboard text the platform sends us.
///
/// egui delivers a paste as an event rather than on demand, so a paste
/// asked for by a command is served on the frame the event arrives.
fn handle_clipboard(app: &mut VuwrApp, ctx: &egui::Context) {
    let pasted: Vec<String> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    });
    for text in pasted {
        if let Some(session) = app.try_session_mut() {
            session.paste(&text);
        }
    }
    app.want_paste = false;
}

/// Take any file dropped on the window.
///
/// On the web the bytes come with the drop event; natively only a path
/// does, so the two are read differently but land in the same place.
fn handle_dropped_files(app: &mut VuwrApp, ctx: &egui::Context) {
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    let Some(file) = dropped.into_iter().next() else {
        return;
    };

    let name = file.path.clone().or_else(|| Some(file.name.clone().into()));
    let bytes: Option<Vec<u8>> = match &file.bytes {
        Some(bytes) => Some(bytes.to_vec()),
        None => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                file.path.as_ref().and_then(|p| std::fs::read(p).ok())
            }
            #[cfg(target_arch = "wasm32")]
            {
                None
            }
        }
    };

    let Some(bytes) = bytes else {
        app.report_load_error("could not read the dropped file");
        return;
    };
    let label = name
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "file".to_string());
    match app.load(name, &bytes) {
        Ok(()) => {}
        // A file that does not parse is reported where it was dropped,
        // rather than leaving whatever was open looking like the new file.
        Err(e) => app.report_load_error(format!("{label}: {}", e.located(&bytes))),
    }
}

/// Feed this frame's input to the session.
pub fn handle(app: &mut VuwrApp, ctx: &egui::Context) {
    handle_dropped_files(app, ctx);
    handle_clipboard(app, ctx);

    if !app.has_document() {
        return;
    }

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
                    Key::Delete => app.session_mut().input_delete(),
                    // The caret moves, so an edit is an edit rather than
                    // a field you can only append to.
                    Key::ArrowLeft => app.session_mut().input_left(),
                    Key::ArrowRight => app.session_mut().input_right(),
                    Key::Home => app.session_mut().input_home(),
                    Key::End => app.session_mut().input_end(),
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
        // Punctuation egui has no Key variant for arrives as text.
        if let egui::Event::Text(text) = &event {
            for c in text.chars() {
                if let Some(cmd) = command_for_char(c) {
                    app.run(cmd, ctx);
                }
            }
            continue;
        }
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
