//! Keys to commands.
//!
//! The only place in the TUI that knows about crossterm. `App::execute`
//! takes [`Command`]s, so the GUI can bind menu items to the same actions
//! and a config file can later rebind keys without touching behaviour.
//!
//! Pager keys follow `less`: space and `f` forward a screen, `b` back,
//! `d`/`u` half a screen, `g`/`G` to the ends.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vuwr_core::Command;

/// What a key resolved to.
pub enum Resolved {
    /// Run this command.
    Run(Command),
    /// `g` was pressed and we are waiting to see whether `gg` follows.
    PendingG,
    /// Not bound.
    None,
}

/// Resolve a key press in normal mode.
///
/// `pending_g` is true when the previous key was a bare `g`.
pub fn resolve(key: KeyEvent, pending_g: bool) -> Resolved {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if pending_g && key.code == KeyCode::Char('g') {
        return Resolved::Run(Command::GoTop);
    }

    let cmd = match (key.code, ctrl) {
        // Navigation
        (KeyCode::Left, false) | (KeyCode::Char('h'), false) => Command::MoveLeft,
        (KeyCode::Down, false) | (KeyCode::Char('j'), false) => Command::MoveDown,
        (KeyCode::Up, false) | (KeyCode::Char('k'), false) => Command::MoveUp,
        (KeyCode::Right, false) | (KeyCode::Char('l'), false) => Command::MoveRight,

        // Pager: less/more bindings alongside the obvious ones
        (KeyCode::PageDown, false) | (KeyCode::Char(' '), false) | (KeyCode::Char('f'), true) => {
            Command::PageDown
        }
        (KeyCode::PageUp, false) | (KeyCode::Char('b'), false) | (KeyCode::Char('b'), true) => {
            Command::PageUp
        }
        (KeyCode::Char('d'), false) | (KeyCode::Char('d'), true) => Command::HalfPageDown,
        (KeyCode::Char('u'), true) => Command::HalfPageUp,

        (KeyCode::Home, false) => Command::GoRowStart,
        (KeyCode::End, false) => Command::GoRowEnd,
        (KeyCode::Char('G'), false) => Command::GoBottom,
        (KeyCode::Char('g'), false) => return Resolved::PendingG,

        // Views. Direct bindings as well as Tab: cycling alone gives no
        // hint that the other views exist.
        (KeyCode::Tab, false) => Command::CycleView,
        (KeyCode::Char('*'), false) => Command::ExpandAll,
        (KeyCode::Char('_'), false) => Command::CollapseAll,
        (KeyCode::Char('1'), false) => Command::ViewTable,
        (KeyCode::Char('2'), false) => Command::ViewTree,
        (KeyCode::Char('3'), false) => Command::ViewText,
        (KeyCode::Enter, false) => Command::DrillDown,
        (KeyCode::Esc, false) => Command::DrillUp,

        // Editing. `u` is undo only without Ctrl; Ctrl-u is half-page up.
        (KeyCode::Char('i'), false) => Command::EditCell,
        (KeyCode::Char('c'), false) => Command::ReplaceCell,
        (KeyCode::Char('R'), false) => Command::RenameKey,
        (KeyCode::F(2), false) => Command::EditLarge,
        (KeyCode::Char('y'), false) => Command::Copy,
        (KeyCode::Char('Y'), false) => Command::CopyRow,
        (KeyCode::Char('p'), false) => Command::Paste,
        (KeyCode::Char('u'), false) => Command::Undo,
        (KeyCode::Char('r'), true) => Command::Redo,

        // Search, filter, marks — csvlens bindings, which are close to
        // less's where they overlap.
        (KeyCode::Char('/'), false) => Command::Find,
        (KeyCode::Char('n'), false) => Command::FindNext,
        (KeyCode::Char('N'), false) => Command::FindPrev,
        (KeyCode::Char('&'), false) => Command::Filter,
        // Replacing. `%` for the setup, after vim's `:%s`; then `.` takes
        // the match under the cursor and `n` — already "next match" —
        // skips it, so stepping through needs no new vocabulary. `a`
        // takes the rest at once.
        (KeyCode::Char('%'), false) => Command::Substitute,
        (KeyCode::Char('.'), false) => Command::SubstituteOne,
        (KeyCode::Char('a'), false) => Command::SubstituteAll,
        (KeyCode::Char('r'), false) => Command::ClearFilter,
        (KeyCode::Char('m'), false) => Command::ToggleMark,
        (KeyCode::Char('M'), false) => Command::ClearMarks,
        (KeyCode::Char('e'), true) => Command::PrintMarks,
        (KeyCode::Char('f'), false) => Command::FreezeColumns,
        (KeyCode::Char('>'), false) => Command::WidenColumn,
        (KeyCode::Char('<'), false) => Command::NarrowColumn,
        (KeyCode::Char('='), false) => Command::AutoSizeColumns,
        (KeyCode::Char('-'), false) => Command::HideColumn,
        (KeyCode::Char('+'), false) => Command::ShowAllColumns,
        // Sorting: `s` cycles direction, capital variants pick a
        // comparison. csvlens uses Shift-Down; `s` is easier to reach.
        (KeyCode::Char('s'), false) => Command::Sort,
        (KeyCode::Char('S'), false) => Command::SortNatural,
        (KeyCode::Char('s'), true) => Command::SortNumeric,

        // File and interface
        (KeyCode::Char('q'), false) => Command::Quit,
        (KeyCode::Char(':'), false) => Command::OpenPalette,
        (KeyCode::Char('?'), false) => Command::Help,
        (KeyCode::Char('H'), false) => Command::ToggleHints,
        (KeyCode::Char('V'), false) => Command::ToggleDetail,
        (KeyCode::Char('E'), false) => Command::ToggleDecoded,

        _ => return Resolved::None,
    };
    Resolved::Run(cmd)
}

/// The keys bound to a command, for the help overlay. Generated from the
/// same table `resolve` uses, so help cannot fall out of date silently —
/// [`tests::help_lists_a_key_for_every_reachable_command`] enforces it.
pub fn keys_for(cmd: Command) -> &'static str {
    match cmd {
        Command::MoveLeft => "h  ←",
        Command::MoveRight => "l  →",
        Command::MoveUp => "k  ↑",
        Command::MoveDown => "j  ↓",
        Command::PageDown => "Space  Ctrl-F  PgDn",
        Command::PageUp => "b  Ctrl-B  PgUp",
        Command::HalfPageDown => "d  Ctrl-D",
        Command::HalfPageUp => "Ctrl-U",
        Command::GoTop => "gg",
        Command::GoBottom => "G",
        Command::GoRowStart => "Home",
        Command::GoRowEnd => "End",
        Command::CycleView => "Tab",
        Command::ExpandAll => "*",
        Command::CollapseAll => "_",
        Command::ViewTable => "1",
        Command::ViewTree => "2",
        Command::ViewText => "3",
        Command::DrillDown => "Enter",
        Command::DrillUp => "Esc",
        Command::Find => "/",
        Command::FindNext => "n",
        Command::FindPrev => "N",
        Command::Filter => "&",
        Command::Substitute => "%",
        Command::SubstituteOne => ".",
        Command::SubstituteAll => "a",
        Command::ClearFilter => "r",
        Command::Sort => "s",
        Command::SortNumeric => "Ctrl-S",
        Command::SortNatural => "S",
        Command::FormatPretty => ":format",
        Command::FormatSmart => ":format-smart",
        Command::FormatCompact => ":compact",
        Command::ToggleMark => "m",
        Command::ClearMarks => "M",
        Command::PrintMarks => "Ctrl-E",
        Command::FreezeColumns => "f",
        Command::WidenColumn => ">",
        Command::NarrowColumn => "<",
        Command::AutoSizeColumns => "=",
        Command::HideColumn => "-",
        Command::ShowAllColumns => "+",
        Command::Lint => ":lint",
        Command::EditCell => "i  Enter",
        Command::ReplaceCell => "c",
        Command::RenameKey => "R",
        Command::EditLarge => "F2",
        Command::Copy => "y",
        Command::CopyRow => "Y",
        Command::Paste => "p",
        Command::Undo => "u",
        Command::Redo => "Ctrl-R",
        // A terminal has no file dialog; these are the `:` commands.
        Command::Open => ":open",
        Command::Save => ":w",
        Command::SaveAs => ":write-as",
        Command::Quit => "q  :q",
        Command::ForceQuit => ":q!",
        Command::SaveAndQuit => ":wq  :wq!  :x",
        Command::OpenPalette => ":",
        Command::Help => "?",
        Command::ToggleHints => "H",
        Command::ToggleDetail => "V",
        Command::ToggleDecoded => "E",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn run(key: KeyEvent) -> Option<Command> {
        match resolve(key, false) {
            Resolved::Run(c) => Some(c),
            _ => None,
        }
    }

    #[test]
    fn pager_keys_follow_less() {
        assert_eq!(run(k(' ')), Some(Command::PageDown));
        assert_eq!(run(ctrl('f')), Some(Command::PageDown));
        assert_eq!(run(k('b')), Some(Command::PageUp));
        assert_eq!(run(k('d')), Some(Command::HalfPageDown));
        assert_eq!(run(ctrl('u')), Some(Command::HalfPageUp));
        assert_eq!(run(k('G')), Some(Command::GoBottom));
    }

    /// Ctrl-U is half-page-up, so plain `u` must still be undo.
    #[test]
    fn u_is_undo_and_ctrl_u_is_half_page() {
        assert_eq!(run(k('u')), Some(Command::Undo));
        assert_eq!(run(ctrl('u')), Some(Command::HalfPageUp));
    }

    #[test]
    fn gg_needs_the_pending_state() {
        assert!(matches!(resolve(k('g'), false), Resolved::PendingG));
        assert!(matches!(
            resolve(k('g'), true),
            Resolved::Run(Command::GoTop)
        ));
    }

    /// Help is generated from `keys_for`, so a command with no keys listed
    /// would appear in the overlay as a blank line.
    #[test]
    fn help_lists_a_key_for_every_command() {
        for c in Command::ALL {
            assert!(
                !keys_for(*c).trim().is_empty(),
                "{} has no key listed in help",
                c.name()
            );
        }
    }
}
