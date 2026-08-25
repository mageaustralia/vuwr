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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    ExpandAll,
    CollapseAll,
    ViewTable,
    ViewTree,
    ViewText,
    DrillDown,
    DrillUp,

    // --- Search, filter, marks ---
    Find,
    FindNext,
    FindPrev,
    Filter,
    ClearFilter,
    Sort,
    SortNumeric,
    SortNatural,
    FormatPretty,
    FormatSmart,
    FormatCompact,
    ToggleMark,
    ClearMarks,
    /// Print the marked rows to stdout and exit.
    PrintMarks,
    FreezeColumns,

    // --- Editing ---
    EditCell,
    ReplaceCell,
    RenameKey,
    EditLarge,
    Copy,
    CopyRow,
    Paste,
    Undo,
    Redo,

    // --- File ---
    Open,
    Save,
    SaveAs,
    Quit,
    ForceQuit,
    SaveAndQuit,

    // --- Interface ---
    OpenPalette,
    Help,
    /// Show or hide the hint bar.
    ToggleHints,
    ToggleDetail,
    ToggleDecoded,
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
        Command::ExpandAll,
        Command::CollapseAll,
        Command::ViewTable,
        Command::ViewTree,
        Command::ViewText,
        Command::DrillDown,
        Command::DrillUp,
        Command::Find,
        Command::FindNext,
        Command::FindPrev,
        Command::Filter,
        Command::ClearFilter,
        Command::Sort,
        Command::SortNumeric,
        Command::SortNatural,
        Command::FormatPretty,
        Command::FormatSmart,
        Command::FormatCompact,
        Command::ToggleMark,
        Command::ClearMarks,
        Command::PrintMarks,
        Command::FreezeColumns,
        Command::EditCell,
        Command::ReplaceCell,
        Command::RenameKey,
        Command::EditLarge,
        Command::Copy,
        Command::CopyRow,
        Command::Paste,
        Command::Undo,
        Command::Redo,
        Command::Open,
        Command::Save,
        Command::SaveAs,
        Command::Quit,
        Command::ForceQuit,
        Command::SaveAndQuit,
        Command::OpenPalette,
        Command::Help,
        Command::ToggleHints,
        Command::ToggleDetail,
        Command::ToggleDecoded,
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
            Command::ExpandAll => "expand-all",
            Command::CollapseAll => "collapse-all",
            Command::ViewTable => "view-table",
            Command::ViewTree => "view-tree",
            Command::ViewText => "view-text",
            Command::DrillDown => "drill-down",
            Command::DrillUp => "drill-up",
            Command::Find => "find",
            Command::FindNext => "find-next",
            Command::FindPrev => "find-prev",
            Command::Filter => "filter",
            Command::ClearFilter => "clear-filter",
            Command::Sort => "sort",
            Command::SortNumeric => "sort-numeric",
            Command::SortNatural => "sort-natural",
            Command::FormatPretty => "format",
            Command::FormatSmart => "format-smart",
            Command::FormatCompact => "compact",
            Command::ToggleMark => "mark",
            Command::ClearMarks => "clear-marks",
            Command::PrintMarks => "print-marks",
            Command::FreezeColumns => "freeze-columns",
            Command::EditCell => "edit-cell",
            Command::ReplaceCell => "replace-cell",
            Command::RenameKey => "rename-key",
            Command::EditLarge => "edit-large",
            Command::Copy => "copy",
            Command::CopyRow => "copy-row",
            Command::Paste => "paste",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Open => "open",
            Command::Save => "save",
            Command::SaveAs => "save-as",
            Command::Quit => "quit",
            Command::ForceQuit => "quit!",
            Command::SaveAndQuit => "save-quit",
            Command::OpenPalette => "palette",
            Command::Help => "help",
            Command::ToggleHints => "toggle-hints",
            Command::ToggleDetail => "detail",
            Command::ToggleDecoded => "decoded",
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
            Command::ExpandAll => "open every node in the tree",
            Command::CollapseAll => "close every node in the tree",
            Command::ViewTable => "switch to table view",
            Command::ViewTree => "switch to tree view",
            Command::ViewText => "switch to text view (pager)",
            Command::DrillDown => "descend into the selected value",
            Command::DrillUp => "return to the parent",
            Command::Find => "search for a pattern",
            Command::FindNext => "jump to the next match",
            Command::FindPrev => "jump to the previous match",
            Command::Filter => "show only rows matching a pattern",
            Command::ClearFilter => "clear the filter and sort",
            Command::Sort => "sort by this column (again to reverse)",
            Command::SortNumeric => "sort this column as numbers",
            Command::SortNatural => "sort naturally (file2 before file10)",
            Command::FormatPretty => "re-indent, one value per line",
            Command::FormatSmart => "re-indent, keeping short lists on one line",
            Command::FormatCompact => "remove all whitespace",
            Command::ToggleMark => "mark or unmark this row",
            Command::ClearMarks => "clear all marks",
            Command::PrintMarks => "print marked rows to stdout and exit",
            Command::FreezeColumns => "pin columns left of the cursor",
            Command::EditCell => "edit the selected cell",
            Command::ReplaceCell => "replace the selected cell",
            Command::RenameKey => "rename the selected key",
            Command::EditLarge => "edit the selected value in a larger window",
            Command::Copy => "copy the selected value",
            Command::CopyRow => "copy the whole row",
            Command::Paste => "paste over the selected value",
            Command::Undo => "undo the last edit",
            Command::Redo => "redo the last undone edit",
            Command::Open => "open a file",
            Command::Save => "save the file",
            Command::SaveAs => "save to a new file",
            Command::Quit => "quit, refusing to discard unsaved changes",
            Command::ForceQuit => "quit, discarding unsaved changes",
            Command::SaveAndQuit => "save the file and quit",
            Command::OpenPalette => "open the command line",
            Command::Help => "show this help",
            Command::ToggleHints => "show or hide the hint bar",
            Command::ToggleDetail => "show the selected value in full",
            Command::ToggleDecoded => "show text view decoded, or as the source",
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
            Command::ExpandAll => "expand",
            Command::CollapseAll => "collapse",
            Command::ViewTable => "table",
            Command::ViewTree => "tree",
            Command::ViewText => "text",
            Command::DrillDown => "open",
            Command::DrillUp => "back",
            Command::Find => "find",
            Command::FindNext => "next",
            Command::FindPrev => "prev",
            Command::Filter => "filter",
            Command::ClearFilter => "unfilter",
            Command::Sort => "sort",
            Command::SortNumeric => "sort 123",
            Command::SortNatural => "sort nat",
            Command::FormatPretty => "format",
            Command::FormatSmart => "smart",
            Command::FormatCompact => "compact",
            Command::ToggleMark => "mark",
            Command::ClearMarks => "unmark all",
            Command::PrintMarks => "print marked",
            Command::FreezeColumns => "freeze",
            Command::EditCell => "edit",
            Command::ReplaceCell => "replace",
            Command::RenameKey => "rename",
            Command::EditLarge => "edit big",
            Command::Copy => "copy",
            Command::CopyRow => "copy row",
            Command::Paste => "paste",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Open => "open",
            Command::Save => "save",
            Command::SaveAs => "save-as",
            Command::Quit => "quit",
            Command::ForceQuit => "discard",
            Command::SaveAndQuit => "save+quit",
            Command::OpenPalette => "command",
            Command::Help => "help",
            Command::ToggleHints => "hints",
            Command::ToggleDetail => "detail",
            Command::ToggleDecoded => "decoded",
        }
    }

    /// Look up a command typed at `:`. Accepts the stable name and the
    /// short vi-style aliases people reach for first.
    pub fn from_name(input: &str) -> Option<Command> {
        let name = input.trim();
        match name {
            "w" | "w!" | "write" => return Some(Command::Save),
            "write-as" => return Some(Command::SaveAs),
            "q" => return Some(Command::Quit),
            "q!" => return Some(Command::ForceQuit),
            // `!` is accepted because it is muscle memory, but it does not
            // mean "quit even if the write failed" -- that would discard
            // the very edits the command was asked to save. A failed write
            // keeps you in the editor, as vim does.
            "wq" | "wq!" | "x" | "x!" | "write-quit" => return Some(Command::SaveAndQuit),
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
        assert_eq!(Command::ALL.len(), 55, "update ALL when adding a command");
    }

    #[test]
    fn vi_aliases_resolve() {
        for alias in ["wq", "wq!", "x", "x!", "write-quit"] {
            assert_eq!(
                Command::from_name(alias),
                Some(Command::SaveAndQuit),
                ":{alias}"
            );
        }
        assert_eq!(Command::from_name("w!"), Some(Command::Save));
        // The vim spellings stay: `:w` is muscle memory in a terminal,
        // even though the GUI says Save.
        assert_eq!(Command::from_name("w"), Some(Command::Save));
        assert_eq!(Command::from_name("write"), Some(Command::Save));
        assert_eq!(Command::from_name("save"), Some(Command::Save));
        assert_eq!(Command::from_name("w"), Some(Command::Save));
        assert_eq!(Command::from_name("wq"), Some(Command::SaveAndQuit));
        assert_eq!(Command::from_name(" q! "), Some(Command::ForceQuit));
        assert_eq!(Command::from_name("nope"), None);
    }
}
