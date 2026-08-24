//! The command vocabulary.
//!
//! Every action a user can take is one of these, whatever triggers it: a
//! keypress in the TUI, a menu item in the GUI, a line typed at `:`, or a
//! script calling in. Frontends map their own inputs onto this enum and
//! nothing else, which is what stops the TUI and GUI drifting apart in
//! what they permit — the same reason all mutation funnels through
//! [`crate::EditOp`].
//!
//! Names are stable: they appear in the `:` palette and in help, and a
//! future config file will bind keys to them.

/// A user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    // --- Navigation ---
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    PageDown,
    PageUp,
    HalfPageDown,
    HalfPageUp,
    GoTop,
    GoBottom,
    GoRowStart,
    GoRowEnd,

    // --- Views ---
    CycleView,
    ViewTable,
    ViewTree,
    ViewText,
    DrillDown,
    DrillUp,

    // --- Editing ---
    EditCell,
    ReplaceCell,
    Undo,
    Redo,

    // --- File ---
    Save,
    Quit,
    ForceQuit,
    SaveAndQuit,

    // --- Interface ---
    OpenPalette,
    Help,
    /// Show or hide the hint bar.
    ToggleHints,
}

impl Command {
    /// Every command, in the order help should list them.
    pub const ALL: &'static [Command] = &[
        Command::MoveLeft,
        Command::MoveRight,
        Command::MoveUp,
        Command::MoveDown,
        Command::PageDown,
        Command::PageUp,
        Command::HalfPageDown,
        Command::HalfPageUp,
        Command::GoTop,
        Command::GoBottom,
        Command::GoRowStart,
        Command::GoRowEnd,
        Command::CycleView,
        Command::ViewTable,
        Command::ViewTree,
        Command::ViewText,
        Command::DrillDown,
        Command::DrillUp,
        Command::EditCell,
        Command::ReplaceCell,
        Command::Undo,
        Command::Redo,
        Command::Save,
        Command::Quit,
        Command::ForceQuit,
        Command::SaveAndQuit,
        Command::OpenPalette,
        Command::Help,
        Command::ToggleHints,
    ];

    /// The stable name used by the `:` palette and by help.
    pub fn name(self) -> &'static str {
        match self {
            Command::MoveLeft => "move-left",
            Command::MoveRight => "move-right",
            Command::MoveUp => "move-up",
            Command::MoveDown => "move-down",
            Command::PageDown => "page-down",
            Command::PageUp => "page-up",
            Command::HalfPageDown => "half-page-down",
            Command::HalfPageUp => "half-page-up",
            Command::GoTop => "go-top",
            Command::GoBottom => "go-bottom",
            Command::GoRowStart => "go-row-start",
            Command::GoRowEnd => "go-row-end",
            Command::CycleView => "cycle-view",
            Command::ViewTable => "view-table",
            Command::ViewTree => "view-tree",
            Command::ViewText => "view-text",
            Command::DrillDown => "drill-down",
            Command::DrillUp => "drill-up",
            Command::EditCell => "edit-cell",
            Command::ReplaceCell => "replace-cell",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Save => "write",
            Command::Quit => "quit",
            Command::ForceQuit => "quit!",
            Command::SaveAndQuit => "write-quit",
            Command::OpenPalette => "palette",
            Command::Help => "help",
            Command::ToggleHints => "toggle-hints",
        }
    }

    /// One line for the help overlay and the palette.
    pub fn description(self) -> &'static str {
        match self {
            Command::MoveLeft => "move one column left",
            Command::MoveRight => "move one column right",
            Command::MoveUp => "move one row up",
            Command::MoveDown => "move one row down",
            Command::PageDown => "scroll one screen down",
            Command::PageUp => "scroll one screen up",
            Command::HalfPageDown => "scroll half a screen down",
            Command::HalfPageUp => "scroll half a screen up",
            Command::GoTop => "jump to the first row",
            Command::GoBottom => "jump to the last row",
            Command::GoRowStart => "jump to the first column",
            Command::GoRowEnd => "jump to the last column",
            Command::CycleView => "cycle table / tree / text",
            Command::ViewTable => "switch to table view",
            Command::ViewTree => "switch to tree view",
            Command::ViewText => "switch to text view (pager)",
            Command::DrillDown => "descend into the selected value",
            Command::DrillUp => "return to the parent",
            Command::EditCell => "edit the selected cell",
            Command::ReplaceCell => "replace the selected cell",
            Command::Undo => "undo the last edit",
            Command::Redo => "redo the last undone edit",
            Command::Save => "write the file",
            Command::Quit => "quit, refusing to discard unsaved changes",
            Command::ForceQuit => "quit, discarding unsaved changes",
            Command::SaveAndQuit => "write the file and quit",
            Command::OpenPalette => "open the command line",
            Command::Help => "show this help",
            Command::ToggleHints => "show or hide the hint bar",
        }
    }

    /// A one- or two-word label for the hint bar, where the full
    /// description would not fit.
    pub fn short_label(self) -> &'static str {
        match self {
            Command::MoveLeft => "left",
            Command::MoveRight => "right",
            Command::MoveUp => "up",
            Command::MoveDown => "down",
            Command::PageDown => "page",
            Command::PageUp => "back",
            Command::HalfPageDown => "half down",
            Command::HalfPageUp => "half up",
            Command::GoTop => "top",
            Command::GoBottom => "end",
            Command::GoRowStart => "row start",
            Command::GoRowEnd => "row end",
            Command::CycleView => "cycle view",
            Command::ViewTable => "table",
            Command::ViewTree => "tree",
            Command::ViewText => "text",
            Command::DrillDown => "open",
            Command::DrillUp => "back",
            Command::EditCell => "edit",
            Command::ReplaceCell => "replace",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Save => "write",
            Command::Quit => "quit",
            Command::ForceQuit => "discard",
            Command::SaveAndQuit => "write+quit",
            Command::OpenPalette => "command",
            Command::Help => "help",
            Command::ToggleHints => "hints",
        }
    }

    /// Look up a command typed at `:`. Accepts the stable name and the
    /// short vi-style aliases people reach for first.
    pub fn from_name(input: &str) -> Option<Command> {
        let name = input.trim();
        match name {
            "w" => return Some(Command::Save),
            "q" => return Some(Command::Quit),
            "q!" => return Some(Command::ForceQuit),
            "wq" | "x" => return Some(Command::SaveAndQuit),
            "h" | "?" => return Some(Command::Help),
            _ => {}
        }
        Command::ALL.iter().copied().find(|c| c.name() == name)
    }

    /// True when the command changes the document, so a frontend can
    /// refuse it on a read-only view without knowing what it does.
    pub fn mutates(self) -> bool {
        matches!(
            self,
            Command::EditCell | Command::ReplaceCell | Command::Undo | Command::Redo
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = Command::ALL.iter().map(|c| c.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two commands share a name");
    }

    #[test]
    fn every_command_is_listed_and_round_trips_by_name() {
        // A command missing from ALL is invisible to help and the palette.
        for c in Command::ALL {
            assert_eq!(Command::from_name(c.name()), Some(*c), "{}", c.name());
        }
        assert_eq!(Command::ALL.len(), 29, "update ALL when adding a command");
    }

    #[test]
    fn vi_aliases_resolve() {
        assert_eq!(Command::from_name("w"), Some(Command::Save));
        assert_eq!(Command::from_name("wq"), Some(Command::SaveAndQuit));
        assert_eq!(Command::from_name(" q! "), Some(Command::ForceQuit));
        assert_eq!(Command::from_name("nope"), None);
    }
}
