//! The editing session: everything a frontend needs to show and change a
//! document, with nothing about how it is drawn or how input arrives.
//!
//! This lives in core so the TUI, the GUI and any future frontend share
//! one implementation rather than three. A frontend supplies input (its
//! own keys, menus or buttons), turns them into [`Command`]s, and draws
//! the result; everything between is here.
//!
//! Core performs no I/O, so anything that must touch the outside world is
//! returned as an [`Effect`] for the frontend to carry out.

use crate::csv::CsvDoc;
use crate::node::Node;
use crate::search::Search;
use crate::sort::{SortDirection, SortKind, sort_rows};
use crate::tree::{Expansion, TreeRow, rows as tree_rows_of};
use crate::view::GridState;
use crate::{Command, Document};

/// A path as a person would write it: `$.users[0].name`.
pub fn path_label(path: &[crate::PathSeg]) -> String {
    let mut out = String::from("$");
    for seg in path {
        match seg {
            crate::PathSeg::Key(k) => {
                out.push('.');
                out.push_str(k);
            }
            crate::PathSeg::Index(i) => out.push_str(&format!("[{i}]")),
            crate::PathSeg::Attr(a) => out.push_str(&format!("@{a}")),
            crate::PathSeg::Text => out.push_str(".#text"),
        }
    }
    out
}

/// What a fresh node should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewNode {
    Value,
    Object,
    Array,
}

impl NewNode {
    pub fn node(self) -> crate::Node {
        match self {
            NewNode::Value => crate::Node::Str(String::new()),
            NewNode::Object => crate::Node::Map(crate::Map {
                open: '{',
                close: '}',
                entries: Vec::new(),
                trailing_comma: false,
                inline: true,
                spaced: false,
            }),
            NewNode::Array => crate::Node::Array(crate::Array {
                open: '[',
                close: ']',
                items: Vec::new(),
                trailing_comma: false,
                inline: true,
                spaced: false,
            }),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NewNode::Value => "value",
            NewNode::Object => "object",
            NewNode::Array => "array",
        }
    }
}

/// An active sort: which column, how to compare, which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortSpec {
    pub column: usize,
    pub kind: SortKind,
    pub direction: SortDirection,
}

/// Something the session cannot do itself, because core does no I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Nothing for the frontend to do.
    None,
    /// Write the document out. The frontend calls [`Session::mark_saved`]
    /// on success, or [`Session::report`] with the error.
    Save,
    /// Write the document out and, only if that succeeded, close. A failed
    /// write must not quit, or the edits it was asked to save are lost.
    SaveAndQuit,
    /// Close the session.
    Quit,
    /// Hand this text back to the shell (or the clipboard, in a GUI).
    Output(String),
    /// Put this on the clipboard.
    Copy(String),
    /// Read the clipboard and hand it back via [`Session::paste`].
    Paste,
}

pub enum Mode {
    Normal,
    /// Inline edit of whatever the cursor is on. `caret` is a byte index
    /// into `buf`: an edit without a movable caret is a text field you can
    /// only append to, which is not editing.
    Edit {
        buf: String,
        caret: usize,
    },
    /// The `:` command line.
    Command {
        buf: String,
    },
    /// A `/` or `&` prompt.
    Prompt {
        kind: PromptKind,
        buf: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Find,
    Filter,
}

impl PromptKind {
    pub fn sigil(self) -> char {
        match self {
            PromptKind::Find => '/',
            PromptKind::Filter => '&',
        }
    }
}

/// Which view we're showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    Tree,
    /// Raw source, paged like `less`. Read-only.
    Text,
}

pub struct Session {
    pub doc: Document,
    pub grid: GridState,
    pub mode: Mode,
    pub view: ViewMode,
    pub status: String,
    pub dirty: bool,
    widths: Vec<usize>,
    viewport_rows: usize,
    /// For table view: the column headers.
    pub tree_keys: Vec<String>,
    /// The flattened tree rows currently visible.
    pub tree_rows: Vec<TreeRow>,
    /// Which tree nodes are open.
    pub expansion: Expansion,
    /// Help overlay visibility, toggled by `?`.
    pub show_help: bool,
    /// Hint bar visibility, toggled by `H`. On by default: the bindings
    /// are not guessable, and a viewer people reach for occasionally
    /// should not require remembering them.
    pub show_hints: bool,
    /// The last search, reused by `n` and `N` and for highlighting.
    pub search: Option<Search>,
    /// The active row filter, if any.
    filter: Option<Search>,
    /// The active sort, if any.
    sort: Option<SortSpec>,
    /// True while the open edit is renaming a key rather than changing a
    /// value. Both use the same prompt; only the commit differs.
    renaming: bool,
    /// Vertical scroll offset of text view, so a frontend can keep a fixed
    /// gutter in step with the content beside it.
    pub text_scroll: f32,
    /// Rendered lines for text view, rebuilt when the document changes.
    text_lines: Vec<String>,
    /// The exact bytes those lines came from, and each line's byte span
    /// within them. An edit splices into these, so CRLF endings and a
    /// missing final newline survive untouched.
    text_bytes: Vec<u8>,
    text_spans: Vec<(usize, usize)>,
}

impl Session {
    pub fn new(doc: Document) -> Session {
        let (view, widths): (ViewMode, Vec<usize>) = if doc.is_json() || doc.is_xml() {
            (ViewMode::Tree, Vec::new())
        } else {
            (
                ViewMode::Table,
                compute_widths(doc.as_csv().expect("CSV only")),
            )
        };
        let mut app = Session {
            doc,
            grid: GridState::new(),
            mode: Mode::Normal,
            view,
            status: String::new(),
            dirty: false,
            widths,
            viewport_rows: 10,
            tree_keys: Vec::new(),
            tree_rows: Vec::new(),
            expansion: Expansion::new(),
            show_help: false,
            show_hints: true,
            search: None,
            filter: None,
            sort: None,
            renaming: false,
            text_scroll: 0.0,
            text_lines: Vec::new(),
            text_bytes: Vec::new(),
            text_spans: Vec::new(),
        };
        if app.view == ViewMode::Tree {
            app.rebuild_tree();
        }
        app
    }

    /// For table view: returns (headers, row_count, column_count).
    pub fn table_dims(&self) -> (Vec<String>, usize, usize) {
        match self.view {
            ViewMode::Table => match self.doc.sheet() {
                Some(sheet) => {
                    let (rows, cols) = sheet.dims();
                    // A filter changes how many rows are on display, not
                    // how many the document has.
                    (sheet.headers(), self.grid.visible_rows(rows), cols)
                }
                None => (Vec::new(), 0, 0),
            },
            ViewMode::Tree => (
                self.tree_rows.iter().map(|r| r.label.clone()).collect(),
                self.tree_rows.len(),
                1,
            ),
            ViewMode::Text => (Vec::new(), self.text_lines.len(), 1),
        }
    }

    /// The display text of one cell.
    pub fn table_cell(&self, row: usize, col: usize) -> Option<String> {
        match self.view {
            // `row` is a display row; the sheet speaks source rows.
            ViewMode::Table => self
                .doc
                .sheet()?
                .cell(self.grid.source_row(row), col)
                .map(|v| escape(&v)),
            ViewMode::Tree => self.tree_rows.get(row).map(|r| r.summary.clone()),
            ViewMode::Text => self.text_lines.get(row).cloned(),
        }
    }

    /// True when column names are carried separately from the rows, so the
    /// renderer must draw a header row. CSV's header is its own first row.
    pub fn has_separate_header(&self) -> bool {
        self.doc
            .sheet()
            .map(|s| !s.header_is_first_row())
            .unwrap_or(false)
    }

    /// The views this document supports, in cycle order. Drives the
    /// indicator in the status bar so the other views are discoverable
    /// rather than something you have to know to press Tab for.
    pub fn available_views(&self) -> Vec<ViewMode> {
        let mut views = Vec::new();
        if self.doc.is_csv() {
            views.push(ViewMode::Table);
        } else {
            views.push(ViewMode::Tree);
            if self.table_eligible() {
                views.push(ViewMode::Table);
            }
        }
        views.push(ViewMode::Text);
        views
    }

    /// The commands worth showing in the hint bar right now.
    ///
    /// Context-sensitive rather than nano's fixed list: what you can do
    /// differs sharply between a table, a tree, a pager and an open edit.
    pub fn hints(&self) -> Vec<Command> {
        match self.mode {
            Mode::Edit { .. } | Mode::Command { .. } | Mode::Prompt { .. } => Vec::new(),
            Mode::Normal => {
                let mut v = vec![Command::Help];
                match self.view {
                    ViewMode::Table => {
                        if self.doc.sheet().is_some() {
                            v.push(Command::EditCell);
                            v.push(Command::Find);
                            if self.grid.visible.is_some() {
                                v.push(Command::ClearFilter);
                            } else {
                                v.push(Command::Filter);
                            }
                            v.push(Command::ToggleMark);
                            if !self.grid.marks.is_empty() {
                                v.push(Command::PrintMarks);
                            }
                        }
                        v.push(Command::Undo);
                    }
                    ViewMode::Tree => {
                        v.push(Command::DrillDown);
                        v.push(Command::DrillUp);
                    }
                    ViewMode::Text => {
                        v.push(Command::EditCell);
                        v.push(Command::PageDown);
                        v.push(Command::PageUp);
                    }
                }
                // Only offer views this document actually has.
                for view in self.available_views() {
                    if view == self.view {
                        continue;
                    }
                    v.push(match view {
                        ViewMode::Table => Command::ViewTable,
                        ViewMode::Tree => Command::ViewTree,
                        ViewMode::Text => Command::ViewText,
                    });
                }
                v.push(Command::Save);
                v.push(Command::Quit);
                v
            }
        }
    }

    pub fn view_mode(&self) -> ViewMode {
        self.view
    }

    /// True if Tab can cycle to another view mode.
    pub fn can_cycle_view(&self) -> bool {
        self.doc.is_json() || self.doc.is_xml()
    }

    /// Rendered width of each column (table mode only).
    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    pub fn set_viewport_rows(&mut self, rows: usize) {
        self.viewport_rows = rows.max(1);
    }

    /// Run one command. The single entry point for every action, whatever
    /// triggered it — a key, the `:` line, or (later) a GUI menu item.
    pub fn execute(&mut self, cmd: Command) -> Effect {
        let (rows, cols) = self.grid_dims();
        let page = self.viewport_rows as isize;
        match cmd {
            Command::MoveLeft => self.grid.move_by(0, -1, rows, cols),
            Command::MoveRight => self.grid.move_by(0, 1, rows, cols),
            Command::MoveUp => self.grid.move_by(-1, 0, rows, cols),
            Command::MoveDown => self.grid.move_by(1, 0, rows, cols),
            Command::PageDown => self.grid.move_by(page, 0, rows, cols),
            Command::PageUp => self.grid.move_by(-page, 0, rows, cols),
            Command::HalfPageDown => self.grid.move_by(page / 2, 0, rows, cols),
            Command::HalfPageUp => self.grid.move_by(-(page / 2), 0, rows, cols),
            Command::GoTop => self.grid.move_to(0, self.grid.cursor.1, rows, cols),
            Command::GoBottom => self
                .grid
                .move_to(usize::MAX, self.grid.cursor.1, rows, cols),
            Command::GoRowStart => self.grid.move_to(self.grid.cursor.0, 0, rows, cols),
            Command::GoRowEnd => self
                .grid
                .move_to(self.grid.cursor.0, usize::MAX, rows, cols),

            Command::CycleView => self.cycle_view(),
            Command::ViewTable => {
                if self.table_eligible() {
                    self.set_view(ViewMode::Table);
                } else {
                    self.status = "no table view: this document is not row-shaped".into();
                }
            }
            Command::ViewTree => {
                if self.doc.is_csv() {
                    self.status = "no tree view for CSV".into();
                } else {
                    self.set_view(ViewMode::Tree);
                }
            }
            Command::ViewText => self.set_view(ViewMode::Text),
            Command::DrillDown => match self.view {
                ViewMode::Tree => self.toggle_row(),
                // Enter doubles as "edit" in a table, where there is
                // nothing to descend into.
                ViewMode::Table => return self.execute(Command::EditCell),
                ViewMode::Text => {}
            },
            // Esc closes the row under the cursor, or its parent when it
            // is already closed — the way collapsing works elsewhere.
            Command::DrillUp => {
                if self.view == ViewMode::Tree
                    && let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned()
                {
                    {
                        if row.is_expanded() {
                            self.expansion.close(&row.path);
                        } else if row.path.len() > 1 {
                            let parent = &row.path[..row.path.len() - 1];
                            self.expansion.close(parent);
                            if let Some(i) = self.tree_rows.iter().position(|r| r.path == parent) {
                                self.grid.cursor.0 = i;
                            }
                        }
                        self.rebuild_tree();
                        self.clamp_cursor();
                    }
                }
            }
            Command::ExpandAll => {
                if let Some(root) = self.tree_root() {
                    self.expansion.expand_all(&root);
                    self.rebuild_tree();
                    self.status = format!("{} rows", self.tree_rows.len());
                }
            }
            Command::CollapseAll => {
                self.expansion.collapse_all();
                self.rebuild_tree();
                self.grid.cursor = (0, 0);
                self.grid.offset = (0, 0);
            }

            Command::EditCell | Command::ReplaceCell => {
                let editable = match self.view {
                    ViewMode::Table => self.doc.sheet().is_some(),
                    // Text view edits the source line itself.
                    ViewMode::Text => true,
                    // A tree scalar is editable; a container is not — it
                    // has no single value to type over.
                    ViewMode::Tree => self
                        .tree_rows
                        .get(self.grid.cursor.0)
                        .is_some_and(|r| !r.is_container()),
                };
                if !editable {
                    self.status = "this view is not editable".into();
                } else if cmd == Command::ReplaceCell {
                    self.mode = Mode::Edit {
                        buf: String::new(),
                        caret: 0,
                    };
                } else {
                    self.start_edit();
                }
            }
            Command::RenameKey => self.start_rename(),
            // The large editor needs somewhere to put a window, so the
            // frontend handles it; a terminal falls back to inline.
            Command::EditLarge => return self.execute(Command::EditCell),
            Command::Copy => match self.value_text_at_cursor() {
                Some(text) if !text.is_empty() => return Effect::Copy(text),
                _ => self.status = "nothing to copy here".into(),
            },
            Command::CopyRow => match self.row_text_at_cursor() {
                Some(text) => return Effect::Copy(text),
                None => self.status = "nothing to copy here".into(),
            },
            Command::Paste => return Effect::Paste,
            Command::Undo => {
                if self.doc.undo() {
                    self.dirty = true;
                    self.after_edit();
                }
            }
            Command::Redo => {
                if self.doc.redo() {
                    self.dirty = true;
                    self.after_edit();
                }
            }

            Command::Find => {
                self.mode = Mode::Prompt {
                    kind: PromptKind::Find,
                    buf: String::new(),
                }
            }
            Command::Filter => {
                self.mode = Mode::Prompt {
                    kind: PromptKind::Filter,
                    buf: String::new(),
                }
            }
            Command::FindNext => self.find_step(true),
            Command::FindPrev => self.find_step(false),
            Command::ClearFilter => {
                if self.filter.is_some() || self.sort.is_some() {
                    self.filter = None;
                    self.sort = None;
                    self.recompute_view();
                    self.status = "filter and sort cleared".into();
                } else {
                    self.status = "nothing to clear".into();
                }
            }
            Command::Sort => self.sort_by_cursor_column(SortKind::Lexical),
            Command::SortNumeric => self.sort_by_cursor_column(SortKind::Numeric),
            Command::SortNatural => self.sort_by_cursor_column(SortKind::Natural),
            Command::FormatPretty => self.relayout(crate::Layout::Pretty),
            Command::FormatSmart => self.relayout(crate::Layout::Smart),
            Command::FormatCompact => self.relayout(crate::Layout::Compact),
            Command::ToggleMark => {
                if self.view == ViewMode::Table {
                    let source = self.grid.source_row(self.grid.cursor.0);
                    // CSV's first row is the column names, not data. It is
                    // emitted with the marked rows regardless, so marking
                    // it would only duplicate it.
                    if source == 0 && !self.has_separate_header() {
                        self.status = "the header row cannot be marked".into();
                        return Effect::None;
                    }
                    let now = self.grid.toggle_mark(source);
                    self.status = format!(
                        "row {} {}  ({} marked)",
                        source + 1,
                        if now { "marked" } else { "unmarked" },
                        self.grid.marks.len()
                    );
                }
            }
            Command::ClearMarks => {
                let n = self.grid.marks.len();
                self.grid.marks.clear();
                self.status = format!("cleared {n} marks");
            }
            Command::PrintMarks => match self.marked_rows_text() {
                Some(text) => return Effect::Output(text),
                None => self.status = "no rows marked — press m to mark one".into(),
            },
            Command::FreezeColumns => {
                // Pin everything left of the cursor, or unpin if already
                // pinned there — one key both ways.
                let want = self.grid.cursor.1;
                self.grid.frozen_cols = if self.grid.frozen_cols == want {
                    0
                } else {
                    want
                };
                self.status = match self.grid.frozen_cols {
                    0 => "columns unfrozen".into(),
                    n => format!("{n} column(s) frozen"),
                };
            }

            // Opening and Save As need a file dialog, which core cannot
            // reach; the frontend takes them.
            Command::Open | Command::SaveAs => return Effect::None,
            Command::Save => return Effect::Save,
            Command::Quit => {
                return if self.dirty {
                    self.status = "unsaved changes — :q! to discard, :wq to save and quit".into();
                    Effect::None
                } else {
                    Effect::Quit
                };
            }
            Command::ForceQuit => return Effect::Quit,
            // The frontend saves, then asks again: a failed write must not
            // quit, or the edits it was asked to save are lost.
            Command::SaveAndQuit => return Effect::SaveAndQuit,

            Command::OpenPalette => self.mode = Mode::Command { buf: String::new() },
            Command::Help => self.show_help = !self.show_help,
            Command::ToggleHints => self.show_hints = !self.show_hints,
        }
        Effect::None
    }

    /// Re-lay-out the document and refresh what depends on its text.
    fn relayout(&mut self, style: crate::Layout) {
        match self.doc.reformat(style) {
            Ok(()) => {
                self.dirty = true;
                self.after_edit();
                if self.view == ViewMode::Text {
                    self.rebuild_text();
                }
                self.status = format!("reformatted ({style:?})").to_lowercase();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// The whole row under the cursor as delimited text.
    pub fn row_text_at_cursor(&self) -> Option<String> {
        let sheet = self.doc.sheet()?;
        let (_, cols) = sheet.dims();
        let row = self.grid.source_row(self.grid.cursor.0);
        Some(
            (0..cols)
                .map(|c| sheet.cell(row, c).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    /// Take clipboard text.
    ///
    /// While something is being typed it goes in at the caret; otherwise
    /// it replaces the value under the cursor. Multi-line text is refused
    /// for a cell, which holds one value — pasting a block would silently
    /// flatten it.
    pub fn paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.is_entering_text() {
            for c in text.chars().filter(|c| *c != '\n' && *c != '\r') {
                self.input_char(c);
            }
            return;
        }
        if text.contains('\n') {
            self.status = "clipboard holds several lines; open an edit first".into();
            return;
        }
        match self.view {
            ViewMode::Table => {
                let (row, column) = self.grid.cursor;
                let row = self.grid.source_row(row);
                match self.doc.set_cell(row, column, text) {
                    Ok(()) => {
                        self.dirty = true;
                        self.after_edit();
                        self.status = "pasted".into();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            ViewMode::Tree => {
                let row = self.grid.cursor.0;
                self.commit_tree_edit(row, text);
                self.status = "pasted".into();
            }
            ViewMode::Text => {
                let row = self.grid.cursor.0;
                self.commit_text_line(row, text);
            }
        }
    }

    /// The value under the cursor, for editing somewhere with room.
    ///
    /// A cell holding a paragraph of escaped HTML cannot be edited on one
    /// line, and the inline caret is the wrong tool for it. The frontend
    /// takes this, edits it however it likes, and hands it back to
    /// [`Session::commit_large_edit`].
    pub fn large_edit_text(&self) -> Option<String> {
        match self.view {
            ViewMode::Tree => {
                let row = self.tree_rows.get(self.grid.cursor.0)?;
                if row.is_container() {
                    return None;
                }
                let root = self.tree_root()?;
                Some(node_to_edit_string(root.get_at(&row.path)?))
            }
            ViewMode::Table => {
                let sheet = self.doc.sheet()?;
                sheet.cell(self.grid.source_row(self.grid.cursor.0), self.grid.cursor.1)
            }
            // Text view edits a line, which is one line by definition.
            ViewMode::Text => None,
        }
    }

    /// Write back a value edited elsewhere. Newlines are allowed: this is
    /// the path for values the inline editor cannot hold.
    pub fn commit_large_edit(&mut self, text: &str) {
        match self.view {
            ViewMode::Tree => {
                let row = self.grid.cursor.0;
                self.commit_tree_edit(row, text);
            }
            ViewMode::Table => {
                let (row, column) = self.grid.cursor;
                let row = self.grid.source_row(row);
                match self.doc.set_cell(row, column, text) {
                    Ok(()) => {
                        self.dirty = true;
                        self.after_edit();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            ViewMode::Text => {}
        }
    }

    /// Begin renaming the key of the tree row under the cursor.
    pub fn start_rename(&mut self) {
        if self.view != ViewMode::Tree {
            self.status = "renaming needs the tree view".into();
            return;
        }
        let Some(row) = self.tree_rows.get(self.grid.cursor.0) else {
            return;
        };
        if !matches!(row.path.last(), Some(crate::PathSeg::Key(_))) {
            self.status = "only object keys have names".into();
            return;
        }
        self.renaming = true;
        let buf = row.label.clone();
        let caret = buf.len();
        self.mode = Mode::Edit { buf, caret };
    }

    /// True while a key is being renamed.
    pub fn is_renaming(&self) -> bool {
        self.renaming
    }

    /// Which grammar text view should colour by.
    pub fn grammar(&self) -> crate::Grammar {
        if self.doc.is_json() {
            crate::Grammar::Json
        } else if self.doc.is_xml() {
            crate::Grammar::Xml
        } else {
            crate::Grammar::None
        }
    }

    /// Problems in the current document, recomputed on demand.
    ///
    /// Cheap enough to call per frame for ordinary files; a frontend that
    /// opens something enormous should cache it.
    pub fn diagnostics(&self) -> Vec<crate::Diagnostic> {
        self.doc.diagnostics()
    }

    /// Jump to a byte offset in the document's text.
    ///
    /// Switches to text view, because that is the only view where an
    /// offset means anything — "show me" has to actually show you.
    pub fn reveal(&mut self, offset: usize) {
        if self.view != ViewMode::Text {
            self.set_view(ViewMode::Text);
        }
        let bytes = self.doc.serialize();
        let (line, column) = crate::line_col(&bytes, offset);
        let (rows, cols) = self.grid_dims();
        self.grid.move_to(line.saturating_sub(1), 0, rows, cols);
        self.status = format!("line {line}, column {column}");
    }

    /// Where the cursor is, in the terms each view uses.
    pub fn position_label(&self) -> String {
        match self.view {
            ViewMode::Text => {
                let line = self.grid.cursor.0 + 1;
                let column = 1;
                let total = self.text_lines.len();
                format!("Line: {line}  Column: {column}  of {total}")
            }
            ViewMode::Tree => {
                let row = self.grid.cursor.0 + 1;
                let total = self.tree_rows.len();
                let path = self
                    .tree_rows
                    .get(self.grid.cursor.0)
                    .map(|r| path_label(&r.path))
                    .unwrap_or_default();
                if path.is_empty() {
                    format!("Row: {row} of {total}")
                } else {
                    format!("Row: {row} of {total}   {path}")
                }
            }
            ViewMode::Table => {
                let (_, rows, cols) = self.table_dims();
                let (r, c) = self.grid.cursor;
                format!("Row: {} of {}   Column: {} of {}", r + 1, rows, c + 1, cols)
            }
        }
    }

    /// Note a successful write.
    pub fn mark_saved(&mut self, what: &str) {
        self.dirty = false;
        self.status = format!("wrote {what}");
    }

    /// Show a message, typically a failure the frontend hit.
    pub fn report(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    /// True while a prompt, command line or cell edit is open, so a
    /// frontend knows to route typing rather than treat it as a shortcut.
    pub fn is_entering_text(&self) -> bool {
        !matches!(self.mode, Mode::Normal)
    }

    /// The text being entered, if any, with the sigil that introduces it.
    pub fn entry(&self) -> Option<(char, &str)> {
        match &self.mode {
            Mode::Normal => None,
            Mode::Edit { buf, .. } => Some(('>', buf.as_str())),
            Mode::Command { buf } => Some((':', buf.as_str())),
            Mode::Prompt { kind, buf } => Some((kind.sigil(), buf.as_str())),
        }
    }

    /// Insert a character at the caret.
    pub fn input_char(&mut self, c: char) {
        match &mut self.mode {
            Mode::Normal => {}
            Mode::Edit { buf, caret } => {
                let at = (*caret).min(buf.len());
                buf.insert(at, c);
                *caret = at + c.len_utf8();
            }
            Mode::Command { buf } | Mode::Prompt { buf, .. } => buf.push(c),
        }
    }

    /// Delete the character before the caret.
    pub fn input_backspace(&mut self) {
        match &mut self.mode {
            Mode::Normal => {}
            Mode::Edit { buf, caret } => {
                let at = (*caret).min(buf.len());
                if let Some(prev) = buf[..at].chars().next_back() {
                    let start = at - prev.len_utf8();
                    buf.remove(start);
                    *caret = start;
                }
            }
            Mode::Command { buf } | Mode::Prompt { buf, .. } => {
                buf.pop();
            }
        }
    }

    /// Delete the character at the caret.
    pub fn input_delete(&mut self) {
        if let Mode::Edit { buf, caret } = &mut self.mode {
            let at = (*caret).min(buf.len());
            if at < buf.len() {
                buf.remove(at);
            }
        }
    }

    /// Move the caret one character left.
    pub fn input_left(&mut self) {
        if let Mode::Edit { buf, caret } = &mut self.mode
            && let Some(prev) = buf[..(*caret).min(buf.len())].chars().next_back()
        {
            *caret -= prev.len_utf8();
        }
    }

    /// Move the caret one character right.
    pub fn input_right(&mut self) {
        if let Mode::Edit { buf, caret } = &mut self.mode
            && let Some(next) = buf[(*caret).min(buf.len())..].chars().next()
        {
            *caret += next.len_utf8();
        }
    }

    pub fn input_home(&mut self) {
        if let Mode::Edit { caret, .. } = &mut self.mode {
            *caret = 0;
        }
    }

    pub fn input_end(&mut self) {
        if let Mode::Edit { buf, caret } = &mut self.mode {
            *caret = buf.len();
        }
    }

    /// Where the caret sits in the text being entered, as a byte index.
    pub fn entry_caret(&self) -> usize {
        match &self.mode {
            Mode::Edit { caret, .. } => *caret,
            Mode::Command { buf } | Mode::Prompt { buf, .. } => buf.len(),
            Mode::Normal => 0,
        }
    }

    /// True while a cell, line or key is being edited in place, as opposed
    /// to a `:` command or a search prompt.
    pub fn is_editing_inline(&self) -> bool {
        matches!(self.mode, Mode::Edit { .. })
    }

    /// Abandon what is being entered.
    pub fn input_cancel(&mut self) {
        self.renaming = false;
        self.mode = Mode::Normal;
    }

    /// Accept what is being entered.
    pub fn input_submit(&mut self) -> Effect {
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
            Mode::Normal => Effect::None,
            Mode::Edit { buf, .. } => {
                self.commit_edit(buf);
                Effect::None
            }
            Mode::Prompt { kind, buf } => {
                self.commit_prompt(kind, buf);
                Effect::None
            }
            Mode::Command { buf } => match Command::from_name(&buf) {
                Some(c) => self.execute(c),
                None => {
                    self.status = format!("unknown command :{}", buf.trim());
                    Effect::None
                }
            },
        }
    }

    /// Commit an edit to the cell, source line or tree node under the
    /// cursor.
    fn commit_edit(&mut self, value: String) {
        let (row, column) = self.grid.cursor;
        if self.view == ViewMode::Text {
            self.commit_text_line(row, &value);
            return;
        }
        if self.view == ViewMode::Tree {
            if std::mem::take(&mut self.renaming) {
                self.commit_rename(row, value);
            } else {
                self.commit_tree_edit(row, &value);
            }
            return;
        }
        let row = self.grid.source_row(row);
        match self.doc.set_cell(row, column, &value) {
            Ok(()) => {
                self.dirty = true;
                // JSON/XML edits can change a cell's rendered width.
                if self.doc.sheet().is_some() && !self.doc.is_csv() {
                    self.rebuild_table_widths();
                }
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Give the key under the cursor a new name.
    fn commit_rename(&mut self, row: usize, name: String) {
        if name.is_empty() {
            self.status = "a key cannot be empty".into();
            return;
        }
        let Some((parent, index)) = self.slot_of(row) else {
            return;
        };
        match self.doc.rename_node(&parent, index, name) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_tree();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Write a value into the tree node under the cursor.
    fn commit_tree_edit(&mut self, row: usize, value: &str) {
        let Some(mut path) = self.tree_rows.get(row).map(|r| r.path.clone()) else {
            return;
        };
        let old = self
            .tree_root()
            .and_then(|root| root.get_at(&path).cloned());

        // Writing to an element must change its *text*, not replace the
        // element: setting the node itself would turn `<description>…`
        // into a bare string and lose the tag entirely.
        let replacement = match &old {
            Some(Node::Element(_)) => {
                path.push(crate::PathSeg::Text);
                crate::Node::Text(value.to_string())
            }
            // XML attributes and text are strings; JSON keeps its own
            // type where the new text still fits it.
            _ if self.doc.is_xml() => crate::Node::Str(value.to_string()),
            _ => crate::sheet::typed_replacement(old.as_ref(), value),
        };
        match self.doc.set_node(&path, replacement) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_tree();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Open or close a specific path, for a frontend that can point at
    /// one directly rather than moving a cursor to it.
    pub fn toggle_path(&mut self, path: &[crate::PathSeg]) {
        self.expansion.toggle(path);
        self.rebuild_tree();
        self.clamp_cursor();
    }

    /// The value under the cursor, as text — for copying out.
    pub fn value_text_at_cursor(&self) -> Option<String> {
        match self.view {
            ViewMode::Tree => {
                let row = self.tree_rows.get(self.grid.cursor.0)?;
                let root = self.tree_root()?;
                let node = root.get_at(&row.path)?;
                Some(match node {
                    crate::Node::Array(_) | crate::Node::Map(_) => {
                        // A container copies as JSON, which is what you
                        // would want to paste somewhere else.
                        let mut doc = crate::JsonDoc::parse(b"null").ok()?;
                        *doc.root_mut() = node.clone();
                        String::from_utf8_lossy(&doc.serialize()).into_owned()
                    }
                    other => other.scalar_text(),
                })
            }
            _ => self.table_cell(self.grid.cursor.0, self.grid.cursor.1),
        }
    }

    /// The cursor row's parent path and its position among its siblings.
    fn cursor_slot(&self) -> Option<(Vec<crate::PathSeg>, usize)> {
        self.slot_of(self.grid.cursor.0)
    }

    /// A row's parent path and its position among its siblings.
    fn slot_of(&self, row: usize) -> Option<(Vec<crate::PathSeg>, usize)> {
        let row = self.tree_rows.get(row)?;
        let (last, parent) = row.path.split_last()?;
        let index = match last {
            crate::PathSeg::Index(i) => *i,
            crate::PathSeg::Key(k) => match self.tree_root()?.get_at(parent)? {
                crate::Node::Map(m) => m.entries.iter().position(|(key, _)| key == k)?,
                _ => return None,
            },
            _ => return None,
        };
        Some((parent.to_vec(), index))
    }

    /// Remove the node under the cursor.
    pub fn remove_at_cursor(&mut self) {
        let Some((parent, index)) = self.cursor_slot() else {
            self.status = "nothing to remove here".into();
            return;
        };
        match self.doc.remove_node(&parent, index) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_tree();
                self.clamp_cursor();
                self.status = "removed".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Copy the node under the cursor in beside itself.
    pub fn duplicate_at_cursor(&mut self) {
        let Some((parent, index)) = self.cursor_slot() else {
            self.status = "nothing to duplicate here".into();
            return;
        };
        let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned() else {
            return;
        };
        let Some(value) = self
            .tree_root()
            .and_then(|root| root.get_at(&row.path).cloned())
        else {
            return;
        };
        // A map needs a name, and reusing the old one would create the
        // duplicate key the tree flags as a bug.
        let key = match row.path.last() {
            Some(crate::PathSeg::Key(k)) => Some(self.unique_key(&parent, k)),
            _ => None,
        };
        match self.doc.insert_node(&parent, index + 1, key, value) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_tree();
                self.status = "duplicated".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Insert a new node after the cursor's.
    pub fn insert_after_cursor(&mut self, what: NewNode) {
        let Some((parent, index)) = self.cursor_slot() else {
            self.status = "nothing to insert beside here".into();
            return;
        };
        let value = what.node();
        let key = match self.tree_root().and_then(|r| r.get_at(&parent).cloned()) {
            Some(crate::Node::Map(_)) => Some(self.unique_key(&parent, "new")),
            _ => None,
        };
        match self.doc.insert_node(&parent, index + 1, key, value) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_tree();
                let (rows, cols) = self.grid_dims();
                self.grid.move_by(1, 0, rows, cols);
                self.status = format!("inserted {}", what.label());
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// A key not already used in this map, so an insert never creates the
    /// duplicate the tree would flag.
    fn unique_key(&self, parent: &[crate::PathSeg], base: &str) -> String {
        let existing: Vec<String> = match self.tree_root().and_then(|r| r.get_at(parent).cloned()) {
            Some(crate::Node::Map(m)) => m.entries.iter().map(|(k, _)| k.clone()).collect(),
            _ => Vec::new(),
        };
        let mut candidate = format!("{base} copy");
        let mut n = 2;
        while existing.iter().any(|k| k == &candidate) {
            candidate = format!("{base} copy {n}");
            n += 1;
        }
        candidate
    }

    /// The marked rows as delimited text, with the header first so the
    /// output stands on its own in a pipeline.
    fn marked_rows_text(&self) -> Option<String> {
        if self.grid.marks.is_empty() {
            return None;
        }
        let sheet = self.doc.sheet()?;
        let (_, cols) = sheet.dims();
        let row_text = |r: usize| {
            (0..cols)
                .map(|c| sheet.cell(r, c).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut out = String::new();
        if self.has_separate_header() {
            out.push_str(&sheet.headers().join(","));
        } else {
            out.push_str(&row_text(0));
        }
        out.push('\n');
        for &r in &self.grid.marks {
            out.push_str(&row_text(r));
            out.push('\n');
        }
        Some(out)
    }

    /// Rebuild the visible row order from the filter and the sort.
    ///
    /// Both write the same row order, so they have to be applied together
    /// rather than each overwriting the other: filter first to choose the
    /// rows, then sort to order them. The cursor follows its own row
    /// rather than its index, so nothing jumps under you.
    fn recompute_view(&mut self) {
        let Some(sheet) = self.doc.sheet() else {
            self.grid.visible = None;
            return;
        };
        let (total, _) = sheet.dims();
        let anchor = self.grid.source_row(self.grid.cursor.0);

        let mut rows: Vec<usize> = match &self.filter {
            Some(search) => search.filter_rows(sheet, true),
            None => (0..total).collect(),
        };
        if let Some(spec) = self.sort {
            rows = sort_rows(sheet, &rows, spec.column, spec.kind, spec.direction);
        }

        let unchanged = rows.len() == total && rows.iter().enumerate().all(|(i, &r)| i == r);
        self.grid.visible = if unchanged { None } else { Some(rows) };

        let (rows_now, cols) = self.grid_dims();
        let row = self.grid.display_row(anchor).unwrap_or(0);
        self.grid.move_to(row, self.grid.cursor.1, rows_now, cols);
    }

    /// The active sort, if any.
    pub fn sort_spec(&self) -> Option<SortSpec> {
        self.sort
    }

    /// True when a filter is hiding rows.
    pub fn is_filtered(&self) -> bool {
        self.filter.is_some()
    }

    /// Sort by the cursor's column, flipping direction if it is already
    /// the sort column — one action for both directions, as a column
    /// header click behaves everywhere else.
    pub fn sort_by_cursor_column(&mut self, kind: SortKind) {
        if self.doc.sheet().is_none() {
            self.status = "sorting needs a table view".into();
            return;
        }
        let column = self.grid.cursor.1;
        let direction = match self.sort {
            Some(spec) if spec.column == column && spec.kind == kind => spec.direction.flipped(),
            _ => SortDirection::Ascending,
        };
        self.sort = Some(SortSpec {
            column,
            kind,
            direction,
        });
        self.recompute_view();

        let name = self
            .doc
            .sheet()
            .and_then(|s| s.headers().get(column).cloned())
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| format!("column {}", column + 1));
        self.status = format!(
            "sorted by {name} {}{}",
            match direction {
                SortDirection::Ascending => "ascending",
                SortDirection::Descending => "descending",
            },
            match kind {
                SortKind::Natural => " (natural)",
                SortKind::Numeric => " (numeric)",
                SortKind::Lexical => "",
            }
        );
    }

    /// Jump to the next or previous match of the current search.
    fn find_step(&mut self, forward: bool) {
        let Some(search) = self.search.clone() else {
            self.status = "no search yet — press / to search".into();
            return;
        };
        let Some(sheet) = self.doc.sheet() else {
            self.status = "search needs a table view".into();
            return;
        };
        let from = (self.grid.source_row(self.grid.cursor.0), self.grid.cursor.1);
        match search.find_from(sheet, from, forward) {
            Some((row, col)) => {
                // A match in a filtered-out row cannot be shown, so drop
                // the filter rather than moving the cursor somewhere the
                // user cannot see.
                if self.grid.display_row(row).is_none() {
                    self.grid.clear_filter();
                }
                let display = self.grid.display_row(row).unwrap_or(row);
                let (rows, cols) = self.grid_dims();
                self.grid.move_to(display, col, rows, cols);
                self.status = format!("/{}", search.pattern());
            }
            None => self.status = format!("no match for /{}", search.pattern()),
        }
    }

    fn commit_prompt(&mut self, kind: PromptKind, pattern: String) {
        if pattern.is_empty() {
            return;
        }
        let search = match Search::new(&pattern) {
            Ok(s) => s,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        match kind {
            PromptKind::Find => {
                self.search = Some(search);
                self.find_step(true);
            }
            PromptKind::Filter => {
                let Some(sheet) = self.doc.sheet() else {
                    self.status = "filter needs a table view".into();
                    return;
                };
                let rows = search.filter_rows(sheet, true);
                let data_rows = rows
                    .len()
                    .saturating_sub(usize::from(sheet.header_is_first_row()));
                if data_rows == 0 {
                    self.status = format!("no rows match &{pattern}");
                    return;
                }
                // Record the filter rather than writing the row order
                // directly: sorting writes the same order, and the two
                // have to be applied together or each undoes the other.
                self.search = Some(search.clone());
                self.filter = Some(search);
                self.recompute_view();
                self.grid.cursor = (0, self.grid.cursor.1);
                self.grid.offset = (0, self.grid.offset.1);
                self.status = format!("&{pattern}  ({data_rows} rows)");
            }
        }
    }

    /// Refresh derived state after the document changed.
    fn after_edit(&mut self) {
        self.clamp_cursor();
        if self.view == ViewMode::Table && !self.doc.is_csv() {
            self.rebuild_table_widths();
        } else if self.view == ViewMode::Tree {
            self.rebuild_tree();
        }
    }

    /// (rows, cols) of the current grid, adapting to view mode.
    fn grid_dims(&self) -> (usize, usize) {
        match self.view {
            ViewMode::Table => match self.doc.sheet() {
                Some(sheet) => {
                    let (rows, cols) = sheet.dims();
                    (self.grid.visible_rows(rows), cols)
                }
                None => (0, 0),
            },
            ViewMode::Tree => (self.tree_rows.len(), 1),
            ViewMode::Text => (self.text_lines.len(), 1),
        }
    }

    /// Cycle to the next view this document supports.
    ///
    /// CSV alternates table and text. JSON and XML go tree → table → text,
    /// skipping table when the document is not row-shaped.
    fn cycle_view(&mut self) {
        let next = match self.view {
            ViewMode::Tree if self.table_eligible() => ViewMode::Table,
            ViewMode::Tree => ViewMode::Text,
            ViewMode::Table => ViewMode::Text,
            ViewMode::Text if self.doc.is_csv() => ViewMode::Table,
            ViewMode::Text => ViewMode::Tree,
        };
        self.set_view(next);
    }

    fn table_eligible(&self) -> bool {
        self.doc.sheet().is_some()
    }

    fn set_view(&mut self, view: ViewMode) {
        if view != ViewMode::Table {
            // The row mapping belongs to the table; carrying it into a
            // tree or a pager would index rows that mean something else.
            self.grid.clear_filter();
        }
        self.view = view;
        match view {
            ViewMode::Table => self.rebuild_table_widths(),
            ViewMode::Tree => self.rebuild_tree(),
            ViewMode::Text => self.rebuild_text(),
        }
        self.grid.cursor = (0, 0);
        self.grid.offset = (0, 0);
    }

    /// Text view shows the document exactly as it would be written out, so
    /// what you page through is what `:w` produces.
    fn rebuild_text(&mut self) {
        self.text_bytes = self.doc.serialize();
        self.text_spans.clear();
        self.text_lines.clear();

        let mut start = 0usize;
        let mut i = 0usize;
        while i < self.text_bytes.len() {
            if self.text_bytes[i] == b'\n' {
                // Exclude a CR so the line reads correctly, but leave it in
                // the source: splicing must not convert CRLF to LF.
                let mut end = i;
                if end > start && self.text_bytes[end - 1] == b'\r' {
                    end -= 1;
                }
                self.text_spans.push((start, end));
                start = i + 1;
            }
            i += 1;
        }
        if start < self.text_bytes.len() {
            self.text_spans.push((start, self.text_bytes.len()));
        }

        self.text_lines = self
            .text_spans
            .iter()
            .map(|&(a, b)| String::from_utf8_lossy(&self.text_bytes[a..b]).into_owned())
            .collect();
    }

    /// Commit an edited source line: splice it into the original bytes and
    /// re-parse. A line that makes the document invalid is refused with the
    /// parse error, leaving the file as it was.
    fn commit_text_line(&mut self, line: usize, value: &str) {
        let Some(&(start, end)) = self.text_spans.get(line) else {
            return;
        };
        let mut bytes = Vec::with_capacity(self.text_bytes.len() + value.len());
        bytes.extend_from_slice(&self.text_bytes[..start]);
        bytes.extend_from_slice(value.as_bytes());
        bytes.extend_from_slice(&self.text_bytes[end..]);

        match self.doc.replace_source(&bytes) {
            Ok(()) => {
                self.dirty = true;
                self.rebuild_text();
                self.clamp_cursor();
            }
            // Locate against the bytes that failed, so the position
            // refers to what the user just typed.
            Err(e) => self.status = format!("not applied — {}", e.located(&bytes)),
        }
    }

    /// Size JSON/XML table columns to their contents, the way CSV columns
    /// already are. These used to be pinned at 3 characters, so anything
    /// wider than `abc` — every header longer than three letters included —
    /// was clipped away entirely.
    fn rebuild_table_widths(&mut self) {
        let (headers, row_count, col_count) = self.table_dims();
        if col_count == 0 {
            self.widths = Vec::new();
            return;
        }
        let mut widths: Vec<usize> = headers
            .iter()
            .map(|h| h.chars().count().clamp(3, 40))
            .collect();
        for r in 0..row_count.min(1000) {
            for (c, w) in widths.iter_mut().enumerate().take(col_count) {
                if let Some(text) = self.table_cell(r, c) {
                    *w = (*w).max(text.chars().count()).min(40);
                }
            }
        }
        self.widths = widths;
        self.tree_keys = headers;
    }

    fn rebuild_tree(&mut self) {
        self.tree_rows = match self.tree_root() {
            Some(root) => tree_rows_of(&root, &self.expansion),
            None => Vec::new(),
        };
    }

    /// The document's root node, if it has a tree at all.
    fn tree_root(&self) -> Option<crate::Node> {
        self.doc
            .as_json()
            .map(|j| j.root().clone())
            .or_else(|| self.doc.as_xml().map(|x| x.root().clone()))
    }

    /// Open or close the row under the cursor. Scalars have nothing to
    /// open, so Enter edits them instead.
    fn toggle_row(&mut self) {
        let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned() else {
            return;
        };
        if !row.is_container() {
            self.start_edit();
            return;
        }
        self.expansion.toggle(&row.path);
        self.rebuild_tree();
        self.clamp_cursor();
    }

    fn start_edit(&mut self) {
        let (r, c) = self.grid.cursor;
        let buf = match self.view {
            ViewMode::Table => self
                .doc
                .sheet()
                .and_then(|s| s.cell(self.grid.source_row(r), c))
                .unwrap_or_default(),
            ViewMode::Text => self.text_lines.get(r).cloned().unwrap_or_default(),
            // The row carries a path, so the value comes from the
            // document rather than from whatever the row happens to show.
            ViewMode::Tree => self
                .tree_rows
                .get(r)
                .and_then(|row| {
                    self.tree_root()
                        .and_then(|root| root.get_at(&row.path).map(node_to_edit_string))
                })
                .unwrap_or_default(),
        };
        // Start at the end, the way a rename or a tweak usually wants.
        let caret = buf.len();
        self.mode = Mode::Edit { buf, caret };
    }

    fn clamp_cursor(&mut self) {
        let (rows, cols) = self.grid_dims();
        let (r, c) = self.grid.cursor;
        self.grid.move_to(r, c, rows, cols);
    }
}

/// Rendered width per column: the widest value seen in the first 1000 rows
/// (after escaping), clamped to 3..=40.
fn compute_widths(doc: &CsvDoc) -> Vec<usize> {
    let mut widths = vec![3usize; doc.width()];
    for row in doc.rows().iter().take(1000) {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(escape(&cell.value).chars().count()).min(40);
        }
    }
    widths
}

/// Make control characters visible so a cell always renders on one line.
pub fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Convert a JSON node to an editable string (for inline editing).
fn node_to_edit_string(node: &Node) -> String {
    match node {
        Node::Null => String::new(),
        Node::Bool(b) => b.to_string(),
        Node::Number(s) => s.clone(),
        Node::Str(s) => s.clone(),
        Node::Text(s) | Node::CData(s) => s.clone(),
        // An element that holds only text *is* its text as far as editing
        // goes: `<description>` shows its content in the tree, so the
        // editor has to open on the same thing.
        Node::Element(e) => e
            .children
            .iter()
            .filter_map(|c| match c {
                Node::Text(t) | Node::CData(t) => Some(t.as_str()),
                _ => None,
            })
            .collect(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FormatHint, PathSeg};

    fn session(src: &str) -> Session {
        Session::new(Document::parse(src.as_bytes(), FormatHint::Auto).unwrap())
    }

    /// Summaries are now counts rather than key lists, which sidesteps
    /// the stray-separator bug the old `{a, b, c,}` form had entirely.
    #[test]
    fn container_summaries_are_counts() {
        let s = session(r#"{"a":1,"b":[1,2,3]}"#);
        let root = s.doc.as_json().unwrap().root();
        assert_eq!(crate::tree::summarize(root), "{2}");
        assert_eq!(
            crate::tree::summarize(root.get_at(&[PathSeg::Key("b".into())]).unwrap()),
            "[3]"
        );
    }

    /// Effects are how a frontend learns it must touch the outside world.
    #[test]
    fn save_and_quit_are_distinct_effects() {
        let mut s = session("a\n1\n");
        assert_eq!(s.execute(Command::Save), Effect::Save);
        assert_eq!(s.execute(Command::SaveAndQuit), Effect::SaveAndQuit);
        assert_eq!(s.execute(Command::ForceQuit), Effect::Quit);
    }

    /// Quitting with unsaved changes must not produce a Quit effect, or the
    /// frontend closes over the top of them.
    #[test]
    fn quit_is_refused_while_dirty() {
        let mut s = session("a\n1\n");
        s.grid.cursor = (1, 0);
        s.execute(Command::ReplaceCell);
        s.input_char('9');
        s.input_submit();
        assert!(s.dirty);

        assert_eq!(s.execute(Command::Quit), Effect::None);
        assert!(s.status.contains("unsaved changes"), "{}", s.status);
        assert_eq!(s.execute(Command::ForceQuit), Effect::Quit);
    }

    #[test]
    fn text_entry_routes_through_the_session() {
        let mut s = session("a\n1\n");
        assert!(!s.is_entering_text());
        s.execute(Command::Find);
        assert!(s.is_entering_text());
        assert_eq!(s.entry(), Some(('/', "")));
        s.input_char('x');
        s.input_char('y');
        s.input_backspace();
        assert_eq!(s.entry(), Some(('/', "x")));
        s.input_cancel();
        assert!(!s.is_entering_text());
    }

    #[test]
    fn mark_saved_clears_dirty() {
        let mut s = session("a\n1\n");
        s.grid.cursor = (1, 0);
        s.execute(Command::ReplaceCell);
        s.input_char('2');
        s.input_submit();
        assert!(s.dirty);
        s.mark_saved("t.csv");
        assert!(!s.dirty);
        assert!(s.status.contains("wrote t.csv"), "{}", s.status);
    }
}
