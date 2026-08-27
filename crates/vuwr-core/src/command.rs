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
    ToggleLinks,
    Substitute,
    SubstituteOne,
    SubstituteAll,
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
    WidenColumn,
    NarrowColumn,
    AutoSizeColumns,
    Lint,
    HideColumn,
    ShowAllColumns,

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
    pub const ALL: &'static [Self] = &[
        Self::MoveLeft,
        Self::MoveRight,
        Self::MoveUp,
        Self::MoveDown,
        Self::PageDown,
        Self::PageUp,
        Self::HalfPageDown,
        Self::HalfPageUp,
        Self::GoTop,
        Self::GoBottom,
        Self::GoRowStart,
        Self::GoRowEnd,
        Self::CycleView,
        Self::ExpandAll,
        Self::CollapseAll,
        Self::ViewTable,
        Self::ViewTree,
        Self::ViewText,
        Self::DrillDown,
        Self::DrillUp,
        Self::Find,
        Self::FindNext,
        Self::FindPrev,
        Self::Filter,
        Self::ToggleLinks,
        Self::Substitute,
        Self::SubstituteOne,
        Self::SubstituteAll,
        Self::ClearFilter,
        Self::Sort,
        Self::SortNumeric,
        Self::SortNatural,
        Self::FormatPretty,
        Self::FormatSmart,
        Self::FormatCompact,
        Self::ToggleMark,
        Self::ClearMarks,
        Self::PrintMarks,
        Self::FreezeColumns,
        Self::WidenColumn,
        Self::NarrowColumn,
        Self::AutoSizeColumns,
        Self::Lint,
        Self::HideColumn,
        Self::ShowAllColumns,
        Self::EditCell,
        Self::ReplaceCell,
        Self::RenameKey,
        Self::EditLarge,
        Self::Copy,
        Self::CopyRow,
        Self::Paste,
        Self::Undo,
        Self::Redo,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::Quit,
        Self::ForceQuit,
        Self::SaveAndQuit,
        Self::OpenPalette,
        Self::Help,
        Self::ToggleHints,
        Self::ToggleDetail,
        Self::ToggleDecoded,
    ];

    /// The stable name used by the `:` palette and by help.
    pub fn name(self) -> &'static str {
        match self {
            Self::MoveLeft => "move-left",
            Self::MoveRight => "move-right",
            Self::MoveUp => "move-up",
            Self::MoveDown => "move-down",
            Self::PageDown => "page-down",
            Self::PageUp => "page-up",
            Self::HalfPageDown => "half-page-down",
            Self::HalfPageUp => "half-page-up",
            Self::GoTop => "go-top",
            Self::GoBottom => "go-bottom",
            Self::GoRowStart => "go-row-start",
            Self::GoRowEnd => "go-row-end",
            Self::CycleView => "cycle-view",
            Self::ExpandAll => "expand-all",
            Self::CollapseAll => "collapse-all",
            Self::ViewTable => "view-table",
            Self::ViewTree => "view-tree",
            Self::ViewText => "view-text",
            Self::DrillDown => "drill-down",
            Self::DrillUp => "drill-up",
            Self::Find => "find",
            Self::FindNext => "find-next",
            Self::FindPrev => "find-prev",
            Self::Filter => "filter",
            Self::ClearFilter => "clear-filter",
            Self::ToggleLinks => "links",
            Self::Substitute => "replace",
            Self::SubstituteOne => "replace-one",
            Self::SubstituteAll => "replace-all",
            Self::Sort => "sort",
            Self::SortNumeric => "sort-numeric",
            Self::SortNatural => "sort-natural",
            Self::FormatPretty => "format",
            Self::FormatSmart => "format-smart",
            Self::FormatCompact => "compact",
            Self::ToggleMark => "mark",
            Self::ClearMarks => "clear-marks",
            Self::PrintMarks => "print-marks",
            Self::FreezeColumns => "freeze-columns",
            Self::WidenColumn => "widen-column",
            Self::NarrowColumn => "narrow-column",
            Self::AutoSizeColumns => "auto-size-columns",
            Self::Lint => "lint",
            Self::HideColumn => "hide-column",
            Self::ShowAllColumns => "show-all-columns",
            Self::EditCell => "edit-cell",
            Self::ReplaceCell => "replace-cell",
            Self::RenameKey => "rename-key",
            Self::EditLarge => "edit-large",
            Self::Copy => "copy",
            Self::CopyRow => "copy-row",
            Self::Paste => "paste",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveAs => "save-as",
            Self::Quit => "quit",
            Self::ForceQuit => "quit!",
            Self::SaveAndQuit => "save-quit",
            Self::OpenPalette => "palette",
            Self::Help => "help",
            Self::ToggleHints => "toggle-hints",
            Self::ToggleDetail => "detail",
            Self::ToggleDecoded => "decoded",
        }
    }

    /// One line for the help overlay and the palette.
    pub fn description(self) -> &'static str {
        match self {
            Self::MoveLeft => "move one column left",
            Self::MoveRight => "move one column right",
            Self::MoveUp => "move one row up",
            Self::MoveDown => "move one row down",
            Self::PageDown => "scroll one screen down",
            Self::PageUp => "scroll one screen up",
            Self::HalfPageDown => "scroll half a screen down",
            Self::HalfPageUp => "scroll half a screen up",
            Self::GoTop => "jump to the first row",
            Self::GoBottom => "jump to the last row",
            Self::GoRowStart => "jump to the first column",
            Self::GoRowEnd => "jump to the last column",
            Self::CycleView => "cycle table / tree / text",
            Self::ExpandAll => "open every node in the tree",
            Self::CollapseAll => "close every node in the tree",
            Self::ViewTable => "switch to table view",
            Self::ViewTree => "switch to tree view",
            Self::ViewText => "switch to text view (pager)",
            Self::DrillDown => "descend into the selected value",
            Self::DrillUp => "return to the parent",
            Self::Find => "search for a pattern",
            Self::FindNext => "jump to the next match",
            Self::FindPrev => "jump to the previous match",
            Self::Filter => "show only rows matching a pattern",
            Self::ClearFilter => "clear the filter and sort",
            Self::ToggleLinks => "follow a URL in a value with Cmd-click, or stop offering to",
            Self::Substitute => "find and replace, with $1 for a captured group",
            Self::SubstituteOne => "replace the match under the cursor, then move to the next",
            Self::SubstituteAll => "replace every remaining match, as one undo step",
            Self::Sort => "sort by this column (again to reverse)",
            Self::SortNumeric => "sort this column as numbers",
            Self::SortNatural => "sort naturally (file2 before file10)",
            Self::FormatPretty => "re-indent, one value per line",
            Self::FormatSmart => "re-indent, keeping short lists on one line",
            Self::FormatCompact => "remove all whitespace",
            Self::ToggleMark => "mark or unmark this row",
            Self::ClearMarks => "clear all marks",
            Self::PrintMarks => "print marked rows to stdout and exit",
            Self::FreezeColumns => "pin columns left of the cursor",
            Self::WidenColumn => "widen this column",
            Self::NarrowColumn => "narrow this column",
            Self::AutoSizeColumns => "size every column to its contents again",
            Self::Lint => "check the document for problems a parser lets through",
            Self::HideColumn => "put this column away",
            Self::ShowAllColumns => "bring every column back",
            Self::EditCell => "edit the selected cell",
            Self::ReplaceCell => "replace the selected cell",
            Self::RenameKey => "rename the selected key",
            Self::EditLarge => "edit the selected value in a larger window",
            Self::Copy => "copy the selected value",
            Self::CopyRow => "copy the whole row",
            Self::Paste => "paste over the selected value",
            Self::Undo => "undo the last edit",
            Self::Redo => "redo the last undone edit",
            Self::Open => "open a file",
            Self::Save => "save the file",
            Self::SaveAs => "save to a new file",
            Self::Quit => "quit, refusing to discard unsaved changes",
            Self::ForceQuit => "quit, discarding unsaved changes",
            Self::SaveAndQuit => "save the file and quit",
            Self::OpenPalette => "open the command line",
            Self::Help => "show this help",
            Self::ToggleHints => "show or hide the hint bar",
            Self::ToggleDetail => "show the selected value in full",
            Self::ToggleDecoded => "show text view decoded, or as the source",
        }
    }

    /// A one- or two-word label for the hint bar, where the full
    /// description would not fit.
    pub fn short_label(self) -> &'static str {
        match self {
            Self::MoveLeft => "left",
            Self::MoveRight => "right",
            Self::MoveUp => "up",
            Self::MoveDown => "down",
            Self::PageDown => "page",
            Self::PageUp => "back",
            Self::HalfPageDown => "half down",
            Self::HalfPageUp => "half up",
            Self::GoTop => "top",
            Self::GoBottom => "end",
            Self::GoRowStart => "row start",
            Self::GoRowEnd => "row end",
            Self::CycleView => "cycle view",
            Self::ExpandAll => "expand",
            Self::CollapseAll => "collapse",
            Self::ViewTable => "table",
            Self::ViewTree => "tree",
            Self::ViewText => "text",
            Self::DrillDown => "open",
            Self::DrillUp => "back",
            Self::Find => "find",
            Self::FindNext => "next",
            Self::FindPrev => "prev",
            Self::Filter => "filter",
            Self::ClearFilter => "unfilter",
            Self::ToggleLinks => "links",
            Self::Substitute => "replace…",
            Self::SubstituteOne => "this one",
            Self::SubstituteAll => "all",
            Self::Sort => "sort",
            Self::SortNumeric => "sort 123",
            Self::SortNatural => "sort nat",
            Self::FormatPretty => "format",
            Self::FormatSmart => "smart",
            Self::FormatCompact => "compact",
            Self::ToggleMark => "mark",
            Self::ClearMarks => "unmark all",
            Self::PrintMarks => "print marked",
            Self::FreezeColumns => "freeze",
            Self::WidenColumn => "wider",
            Self::NarrowColumn => "narrower",
            Self::AutoSizeColumns => "auto width",
            Self::Lint => "lint",
            Self::HideColumn => "hide column",
            Self::ShowAllColumns => "all columns",
            Self::EditCell => "edit",
            Self::ReplaceCell => "replace",
            Self::RenameKey => "rename",
            Self::EditLarge => "edit big",
            Self::Copy => "copy",
            Self::CopyRow => "copy row",
            Self::Paste => "paste",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveAs => "save-as",
            Self::Quit => "quit",
            Self::ForceQuit => "discard",
            Self::SaveAndQuit => "save+quit",
            Self::OpenPalette => "command",
            Self::Help => "help",
            Self::ToggleHints => "hints",
            Self::ToggleDetail => "detail",
            Self::ToggleDecoded => "decoded",
        }
    }

    /// Look up a command typed at `:`. Accepts the stable name and the
    /// short vi-style aliases people reach for first.
    pub fn from_name(input: &str) -> Option<Self> {
        let name = input.trim();
        match name {
            "w" | "w!" | "write" => return Some(Self::Save),
            "write-as" => return Some(Self::SaveAs),
            "q" => return Some(Self::Quit),
            "q!" => return Some(Self::ForceQuit),
            // `!` is accepted because it is muscle memory, but it does not
            // mean "quit even if the write failed" -- that would discard
            // the very edits the command was asked to save. A failed write
            // keeps you in the editor, as vim does.
            "wq" | "wq!" | "x" | "x!" | "write-quit" => return Some(Self::SaveAndQuit),
            "h" | "?" => return Some(Self::Help),
            _ => {}
        }
        Self::ALL.iter().copied().find(|c| c.name() == name)
    }

    /// True when the command changes the document, so a frontend can
    /// refuse it on a read-only view without knowing what it does.
    pub fn mutates(self) -> bool {
        matches!(
            self,
            Self::EditCell
                | Self::ReplaceCell
                | Self::SubstituteOne
                | Self::SubstituteAll
                | Self::Undo
                | Self::Redo
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
        assert_eq!(Command::ALL.len(), 65, "update ALL when adding a command");
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
