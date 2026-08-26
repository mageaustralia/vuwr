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
            Self::Value => crate::Node::Str(String::new()),
            Self::Object => crate::Node::Map(crate::Map {
                open: '{',
                close: '}',
                entries: Vec::new(),
                trailing_comma: false,
                inline: true,
                spaced: false,
                // A new object is written the way most JSON is.
                colon_spaced: true,
            }),
            Self::Array => crate::Node::Array(crate::Array {
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
            Self::Value => "value",
            Self::Object => "object",
            Self::Array => "array",
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
    /// Open the larger editor on [`Session::large_edit_text`]. Core has no
    /// window and no overlay, so the frontend puts it somewhere.
    EditLarge,
    /// The colour scheme changed. Core keeps the choice; a frontend that
    /// caches colours or installs a style has to hear about it.
    SchemeChanged(crate::Scheme),
}

pub enum Mode {
    Normal,
    /// Inline edit of whatever the cursor is on.
    Edit(Entry),
    /// The `:` command line.
    Command(Entry),
    /// A `/` or `&` prompt.
    Prompt {
        kind: PromptKind,
        entry: Entry,
    },
}

/// Text being typed, wherever it is being typed.
///
/// One shape for all three, because a text field is a text field. The
/// search bar used to be its own thing — append a character, backspace
/// the last one — so the caret could not move, and a selection could not
/// be deleted because there was nothing to select with. Anything that
/// takes typing gets the same behaviour by construction now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub buf: String,
    /// A byte index into `buf`.
    pub caret: usize,
    /// Where a selection started, so `anchor..caret` — in either order —
    /// is what is selected. Equal to `caret` means nothing is selected,
    /// which is the usual state.
    pub anchor: usize,
}

impl Entry {
    /// An entry holding `buf`, with the caret at the end — where a
    /// rename or a tweak usually wants it.
    pub fn at_end(buf: String) -> Self {
        let caret = buf.len();
        Self {
            buf,
            caret,
            anchor: caret,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Find,
    Filter,
    /// What to look for, on the way to replacing it.
    SubstituteFind,
    /// What to put in its place.
    SubstituteWith,
}

impl PromptKind {
    pub fn sigil(self) -> char {
        match self {
            Self::Find | Self::SubstituteFind => '/',
            Self::Filter => '&',
            Self::SubstituteWith => '=',
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
    /// The value the cursor sits inside, worked out once per cursor line.
    block: Option<(usize, Option<Block>)>,
    /// The colour scheme the document's own text is drawn in.
    scheme: crate::Scheme,
    /// The layout last applied, so a frontend can show which one the
    /// document is in. `None` until one is, which is honest: a file
    /// arrives with whatever shape it was written with, and guessing at
    /// which of the three that resembles would sometimes be wrong.
    layout: Option<crate::Layout>,
    /// Columns the user has put away, by their index in the document.
    ///
    /// Hiding is a view: the column is still there, still saved, still
    /// edited by anything that addresses it directly. A feed has
    /// twenty-three of them and you are usually reading four.
    hidden_columns: std::collections::BTreeSet<usize>,
    viewport_rows: usize,
    viewport_cols: usize,
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
    /// A replacement in progress: what to find and what to put there.
    ///
    /// Held between the prompt and the edits so the same pattern serves
    /// stepping through the matches and replacing the rest at once.
    substitution: Option<(Search, String)>,
    /// The pattern half, while the second prompt is open.
    pending_pattern: Option<String>,
    /// The active sort, if any.
    sort: Option<SortSpec>,
    /// True while the open edit is renaming a key rather than changing a
    /// value. Both use the same prompt; only the commit differs.
    renaming: bool,
    /// Vertical scroll offset of text view, so a frontend can keep a fixed
    /// gutter in step with the content beside it.
    pub text_scroll: f32,
    /// Show the whole value of the selected cell, wrapped, the way a
    /// spreadsheet's formula bar does. A table column is far narrower
    /// than a description, and truncation hides most of the file.
    pub show_detail: bool,
    /// Column widths the user chose, in characters.
    ///
    /// Kept apart from the measured ones so that re-measuring after an
    /// edit does not undo a column somebody widened on purpose.
    manual_widths: std::collections::BTreeMap<usize, usize>,

    /// Diagnostics, worked out once per change.
    ///
    /// Finding them means serialising the whole document — seven
    /// megabytes for a feed — and the bar that shows them asks every
    /// frame, which is not a thing to do sixty times a second.
    /// What the last lint found, or `None` if the document has not been
    /// linted since it last changed.
    lint: Option<Vec<crate::Diagnostic>>,

    /// Show text view with entity references decoded.
    ///
    /// Off by default: text view is the source, and the source is what it
    /// should show. On, it reads as the markup it represents, which is
    /// the point of asking.
    pub decoded_text: bool,
    /// Rendered lines for text view, rebuilt when the document changes.
    text_lines: Vec<String>,
    /// How far each line is displayed in from the left.
    ///
    /// A CDATA section keeps its own newlines, so the later lines of a
    /// description start at column zero however deeply the element is
    /// nested. Showing them under the tag that owns them is the point —
    /// but it used to be done only for the block under the cursor, so the
    /// text moved sideways as the cursor passed over it. Worked out once,
    /// for every line, and always applied.
    text_indents: Vec<usize>,
    /// The longest line, in characters.
    ///
    /// Worked out once here rather than by the frontend on every frame:
    /// measuring it meant cloning two thousand strings sixty times a
    /// second, which cost more than drawing the file did.
    text_widest: usize,
    /// The exact bytes those lines came from, and each line's byte span
    /// within them. An edit splices into these, so CRLF endings and a
    /// missing final newline survive untouched.
    text_bytes: Vec<u8>,
    text_spans: Vec<(usize, usize)>,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        let (view, widths): (ViewMode, Vec<usize>) = if doc.is_json() || doc.is_xml() {
            (ViewMode::Tree, Vec::new())
        } else {
            (
                ViewMode::Table,
                compute_widths(doc.as_csv().expect("CSV only")),
            )
        };
        let mut app = Self {
            doc,
            grid: GridState::new(),
            mode: Mode::Normal,
            view,
            status: String::new(),
            dirty: false,
            widths,
            viewport_rows: 10,
            viewport_cols: 80,
            tree_keys: Vec::new(),
            tree_rows: Vec::new(),
            expansion: Expansion::new(),
            show_help: false,
            show_hints: true,
            search: None,
            filter: None,
            substitution: None,
            pending_pattern: None,
            sort: None,
            renaming: false,
            text_scroll: 0.0,
            show_detail: false,
            // On by default: escaped markup is unreadable, and every
            // other view already showed it decoded.
            decoded_text: true,
            block: None,
            scheme: crate::Scheme::Vuwr,
            layout: None,
            hidden_columns: std::collections::BTreeSet::new(),
            manual_widths: std::collections::BTreeMap::new(),
            lint: None,
            text_lines: Vec::new(),
            text_indents: Vec::new(),
            text_widest: 0,
            text_bytes: Vec::new(),
            text_spans: Vec::new(),
        };
        if app.view == ViewMode::Tree {
            // Open on something rather than on one closed line reading
            // `channel : <channel>`, and put the cursor on that first
            // record so the panel beside it has a record to show.
            if let Some(root) = app.tree_root() {
                let record = app.expansion.expand_to_first_record(&root);
                app.rebuild_tree();
                if let Some(row) = app.tree_rows.iter().position(|r| r.path == record) {
                    app.grid.cursor = (row, 0);
                }
            } else {
                app.rebuild_tree();
            }
        }
        app
    }

    /// For table view: returns (headers, row_count, column_count).
    pub fn table_dims(&self) -> (Vec<String>, usize, usize) {
        match self.view {
            ViewMode::Table => match self.doc.sheet() {
                Some(sheet) => {
                    let (rows, _) = sheet.dims();
                    // A filter changes how many rows are on display, and
                    // hiding changes how many columns are; neither changes
                    // what the document holds.
                    let headers = sheet.headers();
                    let shown: Vec<String> = self
                        .visible_columns()
                        .into_iter()
                        .map(|c| headers.get(c).cloned().unwrap_or_default())
                        .collect();
                    let cols = shown.len();
                    (shown, self.grid.visible_rows(rows), cols)
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

    /// The columns on display, as indices into the document.
    pub fn visible_columns(&self) -> Vec<usize> {
        let cols = self.doc.sheet().map_or(0, |s| s.dims().1);
        (0..cols)
            .filter(|c| !self.hidden_columns.contains(c))
            .collect()
    }

    /// Every column, with whether it is on display — for a frontend that
    /// offers to put one away.
    pub fn column_visibility(&self) -> Vec<(String, bool)> {
        let Some(sheet) = self.doc.sheet() else {
            return Vec::new();
        };
        sheet
            .headers()
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, !self.hidden_columns.contains(&i)))
            .collect()
    }

    /// The document column a display column refers to.
    ///
    /// Every write and every sort must go through this: with a column
    /// hidden, display column 3 is not document column 3, and mixing them
    /// edits the wrong field.
    pub fn source_col(&self, display: usize) -> usize {
        self.visible_columns()
            .get(display)
            .copied()
            .unwrap_or(display)
    }

    /// Put a column away, or bring it back.
    pub fn toggle_column(&mut self, source: usize) {
        if !self.hidden_columns.remove(&source) {
            // Never hide the last one: an empty table is not a view of
            // anything, and there would be nothing left to click.
            if self.visible_columns().len() <= 1 {
                self.status = "the last column stays".into();
                return;
            }
            self.hidden_columns.insert(source);
        }
        self.rebuild_table_widths();
        self.clamp_cursor();
    }

    /// Hide the column the cursor is on.
    pub fn hide_cursor_column(&mut self) {
        if self.view != ViewMode::Table {
            self.status = "columns are a table thing".into();
            return;
        }
        let source = self.source_col(self.grid.cursor.1);
        let name = self
            .doc
            .sheet()
            .and_then(|s| s.headers().get(source).cloned())
            .unwrap_or_default();
        self.toggle_column(source);
        if self.hidden_columns.contains(&source) {
            self.status = format!("{name} hidden — 'show all' brings it back");
        }
    }

    /// Bring every column back.
    pub fn show_all_columns(&mut self) {
        let n = self.hidden_columns.len();
        self.hidden_columns.clear();
        self.rebuild_table_widths();
        self.status = match n {
            0 => "every column was already showing".into(),
            1 => "1 column brought back".into(),
            n => format!("{n} columns brought back"),
        };
    }

    /// How many columns are put away.
    pub fn hidden_column_count(&self) -> usize {
        self.hidden_columns.len()
    }

    /// The display text of one cell.
    pub fn table_cell(&self, row: usize, col: usize) -> Option<String> {
        match self.view {
            // `row` is a display row; the sheet speaks source rows.
            ViewMode::Table => self
                .doc
                .sheet()?
                .cell(self.grid.source_row(row), self.source_col(col))
                .map(|v| escape(&self.for_display(&v))),
            ViewMode::Tree => self.tree_rows.get(row).map(|r| r.summary.clone()),
            ViewMode::Text => self.text_lines.get(row).cloned(),
        }
    }

    /// True when column names are carried separately from the rows, so the
    /// renderer must draw a header row. CSV's header is its own first row.
    pub fn has_separate_header(&self) -> bool {
        self.doc.sheet().is_some_and(|s| !s.header_is_first_row())
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
            Mode::Edit(_) | Mode::Command(_) | Mode::Prompt { .. } => Vec::new(),
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
                        // Only where there is a layout to change: CSV's
                        // shape is its content.
                        if self.doc.is_json() || self.doc.is_xml() {
                            v.push(Command::FormatPretty);
                        }
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

    /// Widest a column may be, in characters. Wide enough for a URL, and
    /// short of a description, which belongs in the detail pane.
    pub const MAX_COLUMN: usize = 200;

    /// Set a column's width, in characters. Survives re-measuring.
    pub fn set_column_width(&mut self, col: usize, chars: usize) {
        self.manual_widths
            .insert(col, chars.clamp(1, Self::MAX_COLUMN));
        self.rebuild_table_widths();
    }

    /// Give a column back to the measurer.
    pub fn auto_size_column(&mut self, col: usize) {
        self.manual_widths.remove(&col);
        self.rebuild_table_widths();
    }

    /// Give every column back to the measurer.
    pub fn auto_size_all_columns(&mut self) {
        self.manual_widths.clear();
        self.rebuild_table_widths();
    }

    /// True when this column's width was chosen rather than measured.
    pub fn column_is_manual(&self, col: usize) -> bool {
        self.manual_widths.contains_key(&col)
    }

    /// Widen or narrow the cursor's column by `delta` characters.
    pub fn resize_cursor_column(&mut self, delta: isize) {
        if self.view != ViewMode::Table {
            return;
        }
        let col = self.grid.cursor.1;
        let current = self.widths.get(col).copied().unwrap_or(12);
        let next = (current as isize + delta).clamp(1, Self::MAX_COLUMN as isize) as usize;
        self.set_column_width(col, next);
        self.status = format!("column {} is {next} wide", col + 1);
    }

    /// True when a column reads as numbers.
    ///
    /// Sampled rather than proven: a frontend uses it to right-align the
    /// column and nothing else, so a stray non-numeric row costs nothing.
    /// No value is coerced — the text is still the text.
    pub fn column_is_numeric(&self, col: usize) -> bool {
        let Some(sheet) = self.doc.sheet() else {
            return false;
        };
        let col = self.source_col(col);
        // CSV keeps its headings in row 0, and a heading is never a
        // number: sampling it would say every column is text.
        let first = usize::from(sheet.header_is_first_row());
        let rows = sheet.dims().0.min(40 + first);
        let mut seen = 0usize;
        for r in first..rows {
            let Some(value) = sheet.cell(r, col) else {
                continue;
            };
            let text = value.trim();
            if text.is_empty() {
                continue;
            }
            if !crate::diagnostics::reads_as_number(text) {
                return false;
            }
            seen += 1;
        }
        seen > 0
    }

    /// Rendered width of each column (table mode only).
    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    pub fn set_viewport_rows(&mut self, rows: usize) {
        self.viewport_rows = rows.max(1);
    }

    /// How wide the view is, in characters. Core cannot know, and it is
    /// what decides whether a value is cut off.
    pub fn set_viewport_cols(&mut self, cols: usize) {
        self.viewport_cols = cols.max(1);
    }

    /// Run one command. The single entry point for every action, whatever
    /// triggered it — a key, the `:` line, or (later) a GUI menu item.
    pub fn execute(&mut self, cmd: Command) -> Effect {
        let (rows, cols) = self.grid_dims();
        let page = self.viewport_rows as isize;
        match cmd {
            // In a tree, left and right open and close, the way every
            // file browser behaves: down, right, down, right.
            Command::MoveLeft if self.view == ViewMode::Tree => self.collapse_or_parent(),
            Command::MoveRight if self.view == ViewMode::Tree => self.expand_or_child(),
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
                ViewMode::Tree => return self.toggle_row(),
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
                } else if self.value_needs_more_room() {
                    // Too long for one line, so open the editor that can
                    // hold it rather than making the user find F2.
                    return Effect::EditLarge;
                } else if cmd == Command::ReplaceCell {
                    self.mode = Mode::Edit(Entry::default());
                } else {
                    self.start_edit();
                }
            }
            Command::RenameKey => self.start_rename(),
            Command::EditLarge => {
                return if self.large_edit_text().is_some() {
                    Effect::EditLarge
                } else {
                    self.status = "nothing here to edit".into();
                    Effect::None
                };
            }
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
                    self.mark_changed();
                    self.after_edit();
                }
            }
            Command::Redo => {
                if self.doc.redo() {
                    self.mark_changed();
                    self.after_edit();
                }
            }

            Command::Find => {
                self.mode = Mode::Prompt {
                    kind: PromptKind::Find,
                    entry: Entry::default(),
                }
            }
            Command::Filter => {
                // Opening the prompt on an active filter shows what that
                // filter is, selected, so the field behaves like every
                // other one: type to replace it, or clear it and press
                // Enter to take the filter off. Coming to it blind and
                // having to hunt for a separate Clear button was the whole
                // complaint.
                let entry = match &self.filter {
                    Some(search) => Entry {
                        buf: search.pattern().to_string(),
                        caret: search.pattern().len(),
                        anchor: 0,
                    },
                    None => Entry::default(),
                };
                self.mode = Mode::Prompt {
                    kind: PromptKind::Filter,
                    entry,
                }
            }
            Command::FindNext => self.find_step(true),
            Command::FindPrev => self.find_step(false),
            Command::Substitute => {
                if !self.can_substitute() {
                    self.status =
                        "replacing works in the table, where the cursor is a cell — press 1".into();
                    return Effect::None;
                }
                // The pattern first, prefilled with whatever was last
                // searched for: replacing what you just found is the
                // common case.
                let entry = match &self.search {
                    Some(search) => Entry {
                        buf: search.pattern().to_string(),
                        caret: search.pattern().len(),
                        anchor: 0,
                    },
                    None => Entry::default(),
                };
                self.pending_pattern = None;
                self.mode = Mode::Prompt {
                    kind: PromptKind::SubstituteFind,
                    entry,
                };
            }
            Command::SubstituteOne => return self.substitute_one(),
            Command::SubstituteAll => return self.substitute_all(),
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
            Command::Lint => self.run_lint(),
            Command::HideColumn => self.hide_cursor_column(),
            Command::ShowAllColumns => self.show_all_columns(),
            Command::WidenColumn => self.resize_cursor_column(4),
            Command::NarrowColumn => self.resize_cursor_column(-4),
            Command::AutoSizeColumns => {
                self.auto_size_all_columns();
                self.status = "columns sized to their contents".into();
            }
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

            Command::OpenPalette => self.mode = Mode::Command(Entry::default()),
            Command::Help => self.show_help = !self.show_help,
            Command::ToggleDecoded => {
                if !self.doc.is_xml() {
                    self.status = "only XML carries entity references".into();
                    return Effect::None;
                }
                self.decoded_text = !self.decoded_text;
                self.rebuild_text();
                self.rebuild_table_widths();
                self.status = if self.decoded_text {
                    "showing decoded text".into()
                } else {
                    "showing the source".into()
                };
            }
            Command::ToggleDetail => {
                self.show_detail = !self.show_detail;
                // Against what the panel will actually show. It used to
                // ask whether there was a line under the cursor, which is
                // a different question and answered "nothing selected"
                // over a panel full of fields.
                if self.show_detail && !self.can_inspect() {
                    self.status = "nothing to show here — put the cursor inside a value".into();
                }
            }
            Command::ToggleHints => self.show_hints = !self.show_hints,
        }
        Effect::None
    }

    /// Re-lay-out the document and refresh what depends on its text.
    fn relayout(&mut self, style: crate::Layout) {
        match self.doc.reformat(style) {
            Ok(()) => {
                self.mark_changed();
                // After `mark_changed`, which forgets the old one.
                self.layout = Some(style);
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
                let (row, column) = (self.grid.source_row(row), self.source_col(column));
                match self.doc.set_cell(row, column, text) {
                    Ok(()) => {
                        self.mark_changed();
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

    /// Longest value still worth editing on one line, where nothing
    /// better is known. A tree row has no column to measure against.
    pub const INLINE_LIMIT: usize = 80;

    /// True when the value under the cursor will not fit where it is
    /// shown, so editing it inline would hide most of it.
    ///
    /// In a table the test is the column's own rendered width: a URL cut
    /// off at 40 characters needs the larger editor as much as a
    /// paragraph does, and a fixed limit missed exactly that case.
    pub fn value_needs_more_room(&self) -> bool {
        if self.view == ViewMode::Text {
            // A line that is only part of a value is edited as that whole
            // value: editing one line of a description in isolation is
            // rarely what anybody means.
            return self.block_span_read().is_some();
        }
        let Some(text) = self.large_edit_text() else {
            return false;
        };
        if text.contains('\n') {
            return true;
        }
        let visible = match self.view {
            ViewMode::Table => self
                .widths
                .get(self.grid.cursor.1)
                .copied()
                .unwrap_or(Self::INLINE_LIMIT),
            // A tree row runs to the edge of the view, less its indent and
            // its key. Measuring the same way as a table keeps the two
            // views behaving alike, which is the point.
            ViewMode::Tree => {
                let row = self.tree_rows.get(self.grid.cursor.0);
                let used = row.map_or(0, |r| r.depth * 2 + r.label.chars().count() + 4);
                self.viewport_cols.saturating_sub(used).max(8)
            }
            ViewMode::Text => usize::MAX,
        };
        text.chars().count() > visible
    }

    /// A stored value as it should be read.
    ///
    /// XML text carries entity references, and `&lt;p&gt;` is not what
    /// anybody is trying to read. The tree has always shown these decoded
    /// and the editor has always opened on decoded text; this is the same
    /// rule for the table. JSON strings have no entities, and decoding one
    /// would eat a literal `&amp;`.
    fn for_display(&self, value: &str) -> String {
        if self.doc.is_xml() && self.decoded_text {
            crate::decode(value)
        } else {
            value.to_string()
        }
    }

    /// Prepare a value typed in a table cell for writing.
    ///
    /// The editor works in decoded text, so XML has to be encoded again —
    /// otherwise a typed `<p>` lands in the file as markup and breaks the
    /// document. A CDATA section holds its content literally, so that one
    /// is left alone.
    fn encode_for_cell(&self, text: &str) -> String {
        if !self.doc.is_xml() {
            return text.to_string();
        }
        let (row, col) = (
            self.grid.source_row(self.grid.cursor.0),
            self.source_col(self.grid.cursor.1),
        );
        let literal = self.doc.as_xml().is_some_and(|x| x.cell_is_cdata(row, col));
        if literal {
            text.to_string()
        } else {
            crate::encode(text)
        }
    }

    /// The selected value in full, decoded, for the detail pane.
    ///
    /// The same text the editor would open on, so what you read is what
    /// you would change.
    pub fn detail_text(&self) -> Option<String> {
        match self.view {
            ViewMode::Text => self.text_lines.get(self.grid.cursor.0).cloned(),
            _ => self.large_edit_text(),
        }
    }

    /// The whole record under the cursor, for the inspector.
    ///
    /// A feed row is twenty-three columns wide and a window shows five, so
    /// the fields you actually want are usually the ones off the right
    /// edge. This is the same row read downwards, where all of it fits.
    ///
    /// Outside table view there is no record to read, so it degrades to
    /// the one value the cursor is on — which is what the detail pane
    /// showed before it.
    pub fn inspector(&self) -> Inspector {
        // In the tree, the record is the node the cursor is inside — the
        // `<item>`, not the one row of it you happen to be on. Showing
        // the row alone made the panel read `item : item`, which is the
        // row's own label and summary and tells you nothing.
        if self.view == ViewMode::Tree
            && let Some(record) = self.tree_record()
        {
            return record;
        }
        if self.view != ViewMode::Table {
            // In the source, the useful unit is the value the cursor is
            // inside, not the line it is on: a description runs over
            // twenty lines, and showing one of them beside the twenty
            // already on screen tells the reader nothing they cannot see.
            if self.view == ViewMode::Text
                && let Some(field) = self.text_value_field()
            {
                return Inspector {
                    meta: format!(
                        "Line {} of {}",
                        self.grid.cursor.0 + 1,
                        self.text_lines.len()
                    ),
                    title: field.key.clone(),
                    fields: vec![field],
                };
            }
            let label = self.detail_label();
            let value = self.detail_text().unwrap_or_default();
            return Inspector {
                meta: match self.view {
                    ViewMode::Text => format!(
                        "Line {} of {}",
                        self.grid.cursor.0 + 1,
                        self.text_lines.len()
                    ),
                    _ => format!("Row {} of {}", self.grid.cursor.0 + 1, self.tree_rows.len()),
                },
                title: label.clone(),
                fields: vec![Field {
                    key: label,
                    value,
                    kind: FieldKind::Text,
                }],
            };
        }

        let (headers, rows, cols) = self.table_dims();
        let row = self.grid.cursor.0;
        let fields: Vec<Field> = (0..cols)
            .map(|c| {
                let value = self.table_cell(row, c).unwrap_or_default();
                let kind = if value.starts_with("http://") || value.starts_with("https://") {
                    FieldKind::Url
                } else if self.column_is_numeric(c) {
                    FieldKind::Number
                } else {
                    FieldKind::Text
                };
                Field {
                    key: headers.get(c).cloned().unwrap_or_default(),
                    value,
                    kind,
                }
            })
            .collect();

        // Whatever reads as this record's name: the first field with room
        // for one, which in a feed is the title rather than the id.
        let title = fields
            .iter()
            .find(|f| f.value.chars().count() > 8 && !matches!(f.kind, FieldKind::Url))
            .or_else(|| fields.first())
            .map(|f| f.value.clone())
            .unwrap_or_default();

        Inspector {
            meta: format!("Row {} of {}", row + 1, rows),
            title,
            fields,
        }
    }

    /// What the detail pane should call the thing it is showing.
    pub fn detail_label(&self) -> String {
        match self.view {
            ViewMode::Tree => self
                .tree_rows
                .get(self.grid.cursor.0)
                .map(|r| r.label.clone())
                .unwrap_or_default(),
            ViewMode::Table => {
                let (headers, _, _) = self.table_dims();
                headers.get(self.grid.cursor.1).cloned().unwrap_or_default()
            }
            ViewMode::Text => format!("line {}", self.grid.cursor.0 + 1),
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
                // Escaped markup is unreadable as written, so it is
                // decoded for editing and re-encoded on the way back.
                Some(crate::decode(&node_to_edit_string(root.get_at(&row.path)?)))
            }
            ViewMode::Table => {
                let sheet = self.doc.sheet()?;
                let raw = sheet.cell(
                    self.grid.source_row(self.grid.cursor.0),
                    self.source_col(self.grid.cursor.1),
                )?;
                // XML text carries entity references; JSON strings do not,
                // and decoding one would eat a literal `&amp;`.
                Some(if self.doc.is_xml() {
                    crate::decode(&raw)
                } else {
                    raw
                })
            }
            // A line is one line by definition, but the value it belongs
            // to need not be: `<description>` runs for twenty of them, and
            // that is what somebody asking to edit it means. The source is
            // handed over as it is written, since a block can hold a CDATA
            // section whose markup is literal — decoding and re-encoding
            // that would rewrite the file's own content.
            ViewMode::Text => {
                let b = self.block_span_read()?;
                let raw =
                    String::from_utf8_lossy(&self.text_bytes[b.inner.0..b.inner.1]).into_owned();
                // Decoded to read and to edit, as everywhere else. A value
                // holding no entity references is already what it says.
                Some(self.for_display(&raw))
            }
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
                let (row, column) = (self.grid.source_row(row), self.source_col(column));
                let written = self.encode_for_cell(text);
                match self.doc.set_cell(row, column, &written) {
                    Ok(()) => {
                        self.mark_changed();
                        self.after_edit();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            ViewMode::Text => {
                let Some(b) = self.block_span_read() else {
                    return;
                };
                // Encoded again only if it arrived encoded: markup a CDATA
                // section holds literally must stay literal.
                let raw = String::from_utf8_lossy(&self.text_bytes[b.inner.0..b.inner.1]);
                let encoded;
                let value = if self.doc.is_xml() && crate::decode(&raw) != raw {
                    encoded = crate::encode(text);
                    encoded.as_str()
                } else {
                    text
                };
                self.splice_source(b.inner.0, b.inner.1, value);
            }
        }
    }

    /// The block under the cursor, from the cache when it is warm.
    fn block_span_read(&self) -> Option<Block> {
        if self.view != ViewMode::Text || !self.doc.is_xml() {
            return None;
        }
        match self.block {
            Some((line, found)) if line == self.grid.cursor.0 => found,
            _ => self.compute_block(self.grid.cursor.0),
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
        self.mode = Mode::Edit(Entry::at_end(row.label.clone()));
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
    /// What the last lint found, if the document has been linted since it
    /// last changed.
    ///
    /// Asked for rather than computed as you type: the scan re-serialises
    /// the whole document, which is 150 ms on a 15 MB file — a hitch after
    /// every edit, paid whether or not anybody was looking.
    pub fn lint_results(&self) -> Option<&[crate::Diagnostic]> {
        self.lint.as_deref()
    }

    /// Scan the document now, and keep the result.
    pub fn run_lint(&mut self) {
        let found = self.doc.diagnostics();
        self.status = match found.len() {
            0 if !self.doc.is_json() => "nothing to check in this format yet".into(),
            0 => "no problems found".into(),
            1 => "1 problem found".into(),
            n => format!("{n} problems found"),
        };
        self.lint = Some(found);
    }

    /// Record that the document changed.
    ///
    /// Everything derived from it is dropped here rather than at each
    /// call site: a path that forgot left the diagnostics bar reporting a
    /// problem that had been fixed.
    fn mark_changed(&mut self) {
        self.dirty = true;
        // The findings were about the bytes as they were.
        self.lint = None;
        self.block = None;
        // An edit leaves the layout alone, but an undo of a reformat does
        // not — and from here the two look the same. Saying nothing beats
        // lighting a button for a layout the document may no longer have.
        self.layout = None;
    }

    /// The lines the value under the cursor spans.
    ///
    /// A description is one element and reads as one thing, but in the
    /// source it is twenty lines. Highlighting only the line the cursor
    /// happens to be on says nothing about where the value starts or
    /// ends, so the whole element is marked instead. `None` when the
    /// value is a single line, which is its own block.
    pub fn value_block(&mut self) -> Option<(usize, usize)> {
        let b = self.block_span()?;
        Some((b.first, b.last))
    }

    /// The block under the cursor, as lines and as bytes.
    ///
    /// Worked out once per cursor line and kept: the scan reads the whole
    /// source, and the frame that draws the highlight asks every time.
    fn block_span(&mut self) -> Option<Block> {
        if self.view != ViewMode::Text || !self.doc.is_xml() {
            return None;
        }
        let line = self.grid.cursor.0;
        if self.block.map(|(l, _)| l) != Some(line) {
            let found = self.compute_block(line);
            self.block = Some((line, found));
        }
        self.block.and_then(|(_, found)| found)
    }

    fn compute_block(&self, line: usize) -> Option<Block> {
        let &(from, to) = self.text_spans.get(line)?;
        let (span, inside_text) = value_element(&self.text_bytes, from, to);
        // A line that stands on its own is its own block, and marking it
        // would swallow it into its parent — the whole `<item>` lighting
        // up because the cursor is on one of its fields. Markup inside a
        // CDATA section is content, so a balanced-looking line there is
        // still part of the value around it.
        if !inside_text && line_is_self_contained(&self.text_bytes[from..to]) {
            return None;
        }
        let (start, end) = span?;
        let first = self.line_of(start)?;
        let last = self.line_of(end.saturating_sub(1))?;
        if first == last {
            // A one-line value is already what the cursor highlights.
            return None;
        }
        Some(Block {
            first,
            last,
            start,
            end,
            inner: inner_span(&self.text_bytes, start, end),
        })
    }

    /// The name of the element a block opens with: `<g:description ...>` is
    /// `g:description`.
    fn opening_tag(bytes: &[u8], start: usize) -> Option<String> {
        let rest = bytes.get(start..)?;
        if rest.first() != Some(&b'<') {
            return None;
        }
        let name: Vec<u8> = rest[1..]
            .iter()
            .copied()
            .take_while(|b| !b.is_ascii_whitespace() && *b != b'>' && *b != b'/')
            .collect();
        (!name.is_empty()).then(|| String::from_utf8_lossy(&name).into_owned())
    }

    /// The line a byte offset falls on.
    fn line_of(&self, byte: usize) -> Option<usize> {
        match self.text_spans.binary_search_by(|&(a, b)| {
            if byte < a {
                std::cmp::Ordering::Greater
            } else if byte >= b {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(i) => Some(i),
            // Between two spans (on the newline itself): the line before.
            Err(i) => Some(i.saturating_sub(1)),
        }
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

    /// Take the cursor to where a diagnostic is, in whichever view can
    /// show it: the text for a syntax problem, the table for a value.
    pub fn reveal_diagnostic(&mut self, d: &crate::Diagnostic) {
        match d.place {
            crate::Place::Text(offset) => self.reveal(offset),
            crate::Place::Cell { row, column } => self.reveal_cell(row, column),
        }
    }

    /// Put the cursor on one cell of the table.
    ///
    /// The row is the document's; the table shows display rows, and with
    /// a filter on they are not the same. A row a filter is hiding cannot
    /// be shown without dropping the filter, so it says so rather than
    /// landing somewhere else and looking like it worked.
    pub fn reveal_cell(&mut self, row: usize, column: usize) {
        if self.doc.sheet().is_none() {
            self.status = "no table view for this document".into();
            return;
        }
        if self.view != ViewMode::Table {
            self.set_view(ViewMode::Table);
        }
        let Some(display_row) = self.grid.display_row(row) else {
            self.status = "that row is hidden by the filter — clear it to see".into();
            return;
        };
        let display_col = self
            .visible_columns()
            .iter()
            .position(|c| *c == column)
            .unwrap_or(0);
        if !self.visible_columns().contains(&column) {
            self.status = "that column is hidden — bring it back to see".into();
            return;
        }
        let (rows, cols) = self.grid_dims();
        self.grid.move_to(display_row, display_col, rows, cols);
        self.grid.ensure_visible(self.viewport_rows);
        let name = self
            .doc
            .sheet()
            .and_then(|s| s.headers().get(column).cloned())
            .unwrap_or_default();
        self.status = format!("row {}, {name}", row + 1);
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
        let sigil = match &self.mode {
            Mode::Normal => return None,
            Mode::Edit(_) => '>',
            Mode::Command(_) => ':',
            Mode::Prompt { kind, .. } => kind.sigil(),
        };
        Some((sigil, self.entry_ref()?.buf.as_str()))
    }

    /// The text being typed, wherever it is being typed.
    fn entry_ref(&self) -> Option<&Entry> {
        match &self.mode {
            Mode::Normal => None,
            Mode::Edit(e) | Mode::Command(e) => Some(e),
            Mode::Prompt { entry, .. } => Some(entry),
        }
    }

    fn entry_mut(&mut self) -> Option<&mut Entry> {
        match &mut self.mode {
            Mode::Normal => None,
            Mode::Edit(e) | Mode::Command(e) => Some(e),
            Mode::Prompt { entry, .. } => Some(entry),
        }
    }

    /// Insert a character at the caret, replacing the selection if there
    /// is one — which is what typing over selected text means everywhere.
    pub fn input_char(&mut self, c: char) {
        self.delete_selection();
        if let Some(e) = self.entry_mut() {
            let at = e.caret.min(e.buf.len());
            e.buf.insert(at, c);
            e.caret = at + c.len_utf8();
            e.anchor = e.caret;
        }
    }

    /// Insert text at the caret: a paste, or anything else arriving whole.
    pub fn input_text(&mut self, text: &str) {
        for c in text.chars() {
            self.input_char(c);
        }
    }

    /// The selected range, or `None` when nothing is selected.
    fn selection(&self) -> Option<(usize, usize)> {
        let e = self.entry_ref()?;
        (e.caret != e.anchor).then(|| (e.caret.min(e.anchor), e.caret.max(e.anchor)))
    }

    /// The selected text, for a copy.
    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.entry_ref()?.buf[a..b].to_string())
    }

    /// What a copy or cut should take: the selection, or the whole value
    /// when there is none — the same rule a browser address bar follows.
    pub fn entry_text(&self) -> Option<String> {
        let e = self.entry_ref()?;
        Some(self.selected_text().unwrap_or_else(|| e.buf.clone()))
    }

    /// Remove the selection, leaving the caret where it was.
    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.selection() else {
            return false;
        };
        if let Some(e) = self.entry_mut() {
            e.buf.replace_range(a..b, "");
            e.caret = a;
            e.anchor = a;
            return true;
        }
        false
    }

    /// Cut the selection to the caller, which puts it on the clipboard.
    pub fn input_cut(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.delete_selection();
        Some(text)
    }

    /// Select everything being typed.
    pub fn select_all(&mut self) {
        if let Some(e) = self.entry_mut() {
            e.anchor = 0;
            e.caret = e.buf.len();
        }
    }

    /// Which prompt is open, so a frontend can label it.
    ///
    /// The sigil alone is not enough: finding and finding-in-order-to-
    /// replace both carry `/`, and a reader in the middle of a replacement
    /// should be told so.
    pub fn prompt_kind(&self) -> Option<PromptKind> {
        match self.mode {
            Mode::Prompt { kind, .. } => Some(kind),
            _ => None,
        }
    }

    /// The whole value the text cursor is inside, named by its tag.
    ///
    /// `None` on a line that is not inside one — a bare `<channel>`, a
    /// blank line — where there is nothing to show that the screen is not
    /// already showing. See [`Session::can_inspect`].
    fn text_value_field(&self) -> Option<Field> {
        let block = self.block_span_read()?;
        let (from, to) = block.inner;
        let raw = String::from_utf8_lossy(self.text_bytes.get(from..to)?).into_owned();
        // Only a value made of text. The root element is a "block" whose
        // inner span is the entire document, and offering the whole file
        // as one field is not a detail of anything.
        if raw.contains('<') {
            return None;
        }
        let value = if self.doc.is_xml() {
            crate::decode(&raw)
        } else {
            raw
        };
        Some(Field {
            key: Self::opening_tag(&self.text_bytes, block.start).unwrap_or_else(|| "value".into()),
            value,
            kind: FieldKind::Text,
        })
    }

    /// The path of the record the tree cursor is inside.
    ///
    /// A leaf's record is its parent; a container is its own.
    fn tree_record_path(&self) -> Option<Vec<crate::PathSeg>> {
        let here = self.tree_rows.get(self.grid.cursor.0)?;
        if here.is_container() {
            return Some(here.path.clone());
        }
        let mut path = here.path.clone();
        path.pop()?;
        Some(path)
    }

    /// Put the cursor on the record's `index`-th field.
    ///
    /// So that clicking a field in the panel acts on *that* field. The
    /// panel shows the record wherever in it the cursor happens to be, so
    /// without this a double-click edited whatever row the cursor was
    /// left on — `g:id` when you had clicked `g:price`.
    pub fn focus_record_field(&mut self, index: usize) {
        match self.view {
            ViewMode::Table => {
                if let Some(&column) = self.visible_columns().get(index) {
                    self.grid.cursor.1 = column;
                }
            }
            ViewMode::Tree => self.focus_tree_field(index),
            ViewMode::Text => {}
        }
    }

    fn focus_tree_field(&mut self, index: usize) {
        let Some(path) = self.tree_record_path() else {
            return;
        };
        let Some(root) = self.tree_root() else {
            return;
        };
        let Some(node) = crate::tree::at(&root, &path) else {
            return;
        };
        let Some(step) = crate::tree::child_steps(node).into_iter().nth(index) else {
            return;
        };
        // The record has to be open for its field to be a row at all.
        for depth in 0..=path.len() {
            self.expansion.open(&path[..depth]);
        }
        self.rebuild_tree();
        let mut child = path;
        child.push(step);
        if let Some(row) = self.tree_rows.iter().position(|r| r.path == child) {
            let (rows, cols) = self.grid_dims();
            self.grid.move_to(row, 0, rows, cols);
        }
    }

    /// Whether the panel has anything to show here.
    ///
    /// The table always does — a row is a record. The tree does wherever
    /// the cursor is inside a container. The source view only does when
    /// the cursor is inside a value: otherwise the panel would repeat the
    /// line already on screen next to it, which is worse than an empty
    /// panel because it looks like information.
    pub fn can_inspect(&self) -> bool {
        match self.view {
            ViewMode::Table => self.doc.sheet().is_some(),
            ViewMode::Tree => self.tree_record().is_some(),
            ViewMode::Text => self.text_value_field().is_some(),
        }
    }

    /// The record the tree cursor is inside, read downwards.
    ///
    /// The nearest container with children: standing on a field shows the
    /// item that holds it, and standing on the item itself shows the same
    /// thing — which is what "the record" means either way.
    fn tree_record(&self) -> Option<Inspector> {
        let path = self.tree_record_path()?;
        let root = self.tree_root()?;
        let node = crate::tree::at(&root, &path)?;
        let children = crate::tree::child_fields(node);
        if children.is_empty() {
            return None;
        }
        let title = self
            .tree_rows
            .iter()
            .find(|r| r.path == path)
            .map_or_else(|| "record".to_string(), |r| r.label.clone());
        let fields = children
            .into_iter()
            .map(|(key, value, kind)| {
                let kind = if value.starts_with("http://") || value.starts_with("https://") {
                    FieldKind::Url
                } else {
                    match kind {
                        crate::ValueKind::Number => FieldKind::Number,
                        _ => FieldKind::Text,
                    }
                };
                Field { key, value, kind }
            })
            .collect();
        Some(Inspector {
            meta: format!("Row {} of {}", self.grid.cursor.0 + 1, self.tree_rows.len()),
            title,
            fields,
        })
    }

    /// Whether replacing can address anything here.
    ///
    /// The table only. A replacement works on cells, and outside the
    /// table the cursor is a tree node or a line of source — so a row and
    /// a column taken from it name some other cell entirely. It used to
    /// arm itself anywhere and then act on whatever those numbers
    /// happened to point at.
    pub fn can_substitute(&self) -> bool {
        self.view == ViewMode::Table && self.doc.sheet().is_some()
    }

    /// Whether a replacement is set up and waiting to be applied.
    pub fn substitution_active(&self) -> bool {
        self.substitution.is_some()
    }

    /// A sentence naming what a replacement will touch, when a filter is
    /// narrowing it. `None` when nothing is filtered.
    pub fn substitution_note(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(shown) = self.visible_count() {
            let total = self.doc.sheet().map_or(0, |s| s.dims().0);
            parts.push(format!(
                "only the {shown} rows the filter shows, not all {total}"
            ));
        }
        match self.hidden_column_count() {
            0 => {}
            1 => parts.push("and not the column you have hidden".into()),
            n => parts.push(format!("and not the {n} columns you have hidden")),
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    /// The selection, as byte offsets, for a frontend to draw.
    pub fn entry_selection(&self) -> Option<(usize, usize)> {
        self.selection()
    }

    /// Delete the character before the caret, or the selection.
    pub fn input_backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        if let Some(e) = self.entry_mut() {
            let at = e.caret.min(e.buf.len());
            if let Some(prev) = e.buf[..at].chars().next_back() {
                let start = at - prev.len_utf8();
                e.buf.remove(start);
                e.caret = start;
                e.anchor = start;
            }
        }
    }

    /// Delete the character at the caret, or the selection.
    pub fn input_delete(&mut self) {
        if self.delete_selection() {
            return;
        }
        if let Some(e) = self.entry_mut() {
            let at = e.caret.min(e.buf.len());
            if at < e.buf.len() {
                e.buf.remove(at);
            }
        }
    }

    /// Move the caret one character left, dropping any selection.
    pub fn input_left(&mut self) {
        self.move_caret(Move::Left, false);
    }

    /// Move the caret one character right, dropping any selection.
    pub fn input_right(&mut self) {
        self.move_caret(Move::Right, false);
    }

    pub fn input_home(&mut self) {
        self.move_caret(Move::Start, false);
    }

    pub fn input_end(&mut self) {
        self.move_caret(Move::End, false);
    }

    /// The same four moves, extending the selection rather than dropping
    /// it — shift-arrow, as everywhere else.
    pub fn input_select_left(&mut self) {
        self.move_caret(Move::Left, true);
    }

    pub fn input_select_right(&mut self) {
        self.move_caret(Move::Right, true);
    }

    pub fn input_select_home(&mut self) {
        self.move_caret(Move::Start, true);
    }

    pub fn input_select_end(&mut self) {
        self.move_caret(Move::End, true);
    }

    fn move_caret(&mut self, how: Move, extend: bool) {
        // An arrow with something selected collapses to that end rather
        // than stepping from the caret: pressing Left after selecting a
        // word puts you before the word, not one character into it.
        if !extend
            && let Some((a, b)) = self.selection()
            && let Some(e) = self.entry_mut()
        {
            match how {
                Move::Left => {
                    e.caret = a;
                    e.anchor = a;
                    return;
                }
                Move::Right => {
                    e.caret = b;
                    e.anchor = b;
                    return;
                }
                Move::Start | Move::End => {}
            }
        }
        if let Some(e) = self.entry_mut() {
            match how {
                Move::Left => {
                    if let Some(prev) = e.buf[..e.caret.min(e.buf.len())].chars().next_back() {
                        e.caret -= prev.len_utf8();
                    }
                }
                Move::Right => {
                    if let Some(next) = e.buf[e.caret.min(e.buf.len())..].chars().next() {
                        e.caret += next.len_utf8();
                    }
                }
                Move::Start => e.caret = 0,
                Move::End => e.caret = e.buf.len(),
            }
            if !extend {
                e.anchor = e.caret;
            }
        }
    }

    /// Complete what is being typed at the `:` prompt.
    ///
    /// Tab again to take the next candidate, and again to come back
    /// round. What is offered depends on where the caret is: the command
    /// while that is still being typed, then its argument — so `:scheme`
    /// then Tab walks the schemes rather than the commands.
    ///
    /// Nothing happens anywhere but the command line: Tab in a search
    /// prompt is a character somebody wants, and Tab in a cell is the
    /// next field.
    pub fn complete(&mut self) {
        let Mode::Command(entry) = &self.mode else {
            return;
        };
        let typed = entry.buf.clone();
        // An argument if there is a space, otherwise the command itself.
        let (prefix, stem) = match typed.split_once(' ') {
            Some((cmd, arg)) => (format!("{cmd} "), arg.to_string()),
            None => (String::new(), typed.clone()),
        };
        let candidates = self.candidates(prefix.trim_end());
        if candidates.is_empty() {
            return;
        }
        // Which one follows what is there: if the buffer already holds a
        // candidate, the next; otherwise the first that starts with what
        // has been typed so far.
        let next = match candidates.iter().position(|c| *c == stem) {
            Some(at) => candidates[(at + 1) % candidates.len()].clone(),
            None => candidates
                .iter()
                .find(|c| c.starts_with(&stem))
                // Nothing matches what was typed, so offer everything
                // rather than nothing: a typo should not leave Tab
                // looking broken.
                .unwrap_or(&candidates[0])
                .clone(),
        };
        self.mode = Mode::Command(Entry::at_end(format!("{prefix}{next}")));
    }

    /// What Tab can offer, given the command already typed.
    fn candidates(&self, command: &str) -> Vec<String> {
        match command {
            // `:scheme <name>` is the only command that takes one.
            "scheme" | "theme" => crate::Scheme::ALL
                .iter()
                .map(|s| s.name().to_ascii_lowercase().replace(' ', "-"))
                .collect(),
            "" => {
                let mut names: Vec<String> =
                    Command::ALL.iter().map(|c| c.name().to_string()).collect();
                names.push("scheme".to_string());
                names.push("theme".to_string());
                names.sort();
                names
            }
            // A command that takes no argument has nothing to offer.
            _ => Vec::new(),
        }
    }

    /// Put the caret at a byte offset in the text being typed.
    ///
    /// Clicking where you want to type is how every other editor works;
    /// without it a typo halfway along means retyping the rest.
    pub fn set_entry_caret(&mut self, byte: usize) {
        if let Some(e) = self.entry_mut() {
            e.caret = floor_boundary(&e.buf, byte);
            e.anchor = e.caret;
        }
    }

    /// Move the caret to a byte offset, keeping the anchor — a drag
    /// across text, or a shift-click.
    pub fn extend_entry_selection(&mut self, byte: usize) {
        if let Some(e) = self.entry_mut() {
            e.caret = floor_boundary(&e.buf, byte);
        }
    }

    /// Select the word around a byte offset — what a double-click means
    /// inside a field.
    ///
    /// A word is a run of letters, digits and the punctuation that holds
    /// identifiers together, so `SKU-1001` and `g:price` come out whole
    /// rather than in pieces. A click in whitespace takes the whitespace.
    pub fn select_word_at(&mut self, byte: usize) {
        let Some(e) = self.entry_mut() else {
            return;
        };
        if e.buf.is_empty() {
            return;
        }
        let at = floor_boundary(&e.buf, byte.min(e.buf.len().saturating_sub(1)));
        let wordish = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':');
        let here = e.buf[at..].chars().next().unwrap_or(' ');
        let want_word = wordish(here);

        let mut start = at;
        for (i, c) in e.buf[..at].char_indices().rev() {
            if wordish(c) != want_word {
                break;
            }
            start = i;
        }
        let mut end = at;
        for (i, c) in e.buf[at..].char_indices() {
            if wordish(c) != want_word {
                break;
            }
            end = at + i + c.len_utf8();
        }
        e.anchor = start;
        e.caret = end;
    }

    /// Where the caret sits in the text being entered, as a byte index.
    pub fn entry_caret(&self) -> usize {
        self.entry_ref().map_or(0, |e| e.caret)
    }

    pub fn is_editing_inline(&self) -> bool {
        matches!(self.mode, Mode::Edit(_))
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
            Mode::Edit(e) => {
                self.commit_edit(e.buf);
                Effect::None
            }
            Mode::Prompt { kind, entry } => {
                self.commit_prompt(kind, entry.buf);
                Effect::None
            }
            Mode::Command(e) => {
                let buf = e.buf;
                // `:scheme <name>` takes an argument, which no other
                // command does — so it is read here rather than becoming
                // a Command variant per scheme.
                let typed = buf.trim();
                // `:theme` as well as `:scheme`: both are the word people
                // reach for, and refusing one of them teaches nothing.
                if let Some(rest) = typed
                    .strip_prefix("scheme")
                    .or_else(|| typed.strip_prefix("theme"))
                {
                    return self.choose_scheme(rest.trim());
                }
                match Command::from_name(&buf) {
                    Some(c) => self.execute(c),
                    None => {
                        self.status = format!("unknown command :{}", buf.trim());
                        Effect::None
                    }
                }
            }
        }
    }

    /// Pick a colour scheme by name, or say which there are.
    ///
    /// Core keeps the choice because both frontends colour the same
    /// tokens from the same table; what each does with it is its own
    /// business.
    fn choose_scheme(&mut self, name: &str) -> Effect {
        if name.is_empty() {
            let names: Vec<&str> = crate::Scheme::ALL.iter().map(|s| s.name()).collect();
            self.status = format!("schemes: {}", names.join(", "));
            return Effect::None;
        }
        match crate::Scheme::from_name(name) {
            Some(scheme) => {
                self.scheme = scheme;
                self.status = format!("{} colours", scheme.name());
                Effect::SchemeChanged(scheme)
            }
            None => {
                self.status = format!("no scheme called {name:?}");
                Effect::None
            }
        }
    }

    /// The layout last applied, or `None` if none has been.
    pub fn layout(&self) -> Option<crate::Layout> {
        self.layout
    }

    /// The scheme in use, for a frontend drawing the document.
    pub fn scheme(&self) -> crate::Scheme {
        self.scheme
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
        let (row, column) = (self.grid.source_row(row), self.source_col(column));
        let value = self.encode_for_cell(&value);
        match self.doc.set_cell(row, column, &value) {
            Ok(()) => {
                self.mark_changed();
                self.after_edit();
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
                self.mark_changed();
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
                // A CDATA section is already literal, so encoding it would
                // double the escaping; plain text needs it.
                let holds_cdata = matches!(&old, Some(Node::Element(e))
                    if e.children.iter().any(|c| matches!(c, Node::CData(_))));
                let text = if holds_cdata {
                    value.to_string()
                } else {
                    crate::encode(value)
                };
                crate::Node::Text(text)
            }
            // XML attributes and text are strings; JSON keeps its own
            // type where the new text still fits it.
            _ if self.doc.is_xml() => crate::Node::Str(value.to_string()),
            _ => crate::sheet::typed_replacement(old.as_ref(), value),
        };
        match self.doc.set_node(&path, replacement) {
            Ok(()) => {
                self.mark_changed();
                self.rebuild_tree();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Right: open this node, or step into it if it is already open.
    fn expand_or_child(&mut self) {
        let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned() else {
            return;
        };
        if !row.is_container() {
            return;
        }
        if row.is_expanded() {
            // Already open, so move to the first child, which is the next
            // row by construction.
            let (rows, cols) = self.grid_dims();
            self.grid.move_by(1, 0, rows, cols);
        } else {
            self.expansion.open(&row.path);
            self.rebuild_tree();
        }
    }

    /// Left: close this node, or step out to its parent.
    fn collapse_or_parent(&mut self) {
        let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned() else {
            return;
        };
        if row.is_expanded() {
            self.expansion.close(&row.path);
            self.rebuild_tree();
            self.clamp_cursor();
            return;
        }
        if row.path.len() > 1 {
            let parent = &row.path[..row.path.len() - 1];
            if let Some(i) = self.tree_rows.iter().position(|r| r.path == parent) {
                self.grid.cursor.0 = i;
            }
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
                    // An element is its text, decoded — the same thing the
                    // editor opens on, so copy and edit agree.
                    crate::Node::Element(_) => crate::decode(&node_to_edit_string(node)),
                    crate::Node::Array(_) | crate::Node::Map(_) => {
                        // A container copies as JSON, which is what you
                        // would want to paste somewhere else.
                        let mut doc = crate::JsonDoc::parse(b"null").ok()?;
                        *doc.root_mut() = node.clone();
                        String::from_utf8_lossy(&doc.serialize()).into_owned()
                    }
                    other => crate::decode(&other.scalar_text()),
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
                self.mark_changed();
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
                self.mark_changed();
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
                self.mark_changed();
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

    /// How many rows a filter is letting through, or `None` when nothing
    /// is filtered. A control that says it is on should say what it did.
    pub fn visible_count(&self) -> Option<usize> {
        self.grid.visible.as_ref().map(std::vec::Vec::len)
    }

    /// Sort by the cursor's column, flipping direction if it is already
    /// the sort column — one action for both directions, as a column
    /// header click behaves everywhere else.
    pub fn sort_by_cursor_column(&mut self, kind: SortKind) {
        if self.doc.sheet().is_none() {
            self.status = "sorting needs a table view".into();
            return;
        }
        // Sorting reorders the document's rows by a document column.
        let column = self.source_col(self.grid.cursor.1);
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
        // The tree is not the table. Searching used to go through the
        // sheet wherever you were, and in the tree that landed the cursor
        // on whatever row happened to share the sheet row's number — a
        // different item, closed, with nothing to show for it.
        if self.view == ViewMode::Tree {
            self.tree_find_step(&search, forward);
            return;
        }
        // Nor is the text view the table. Its cursor is a line of the
        // source, and a sheet row is not one: in a CSV the two nearly
        // agree, which is why this went unnoticed, and in anything with
        // structure they do not agree at all.
        if self.view == ViewMode::Text {
            self.text_find_step(&search, forward);
            return;
        }
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

    /// The same jump, through the source text.
    ///
    /// Matches against the bytes on screen, so what the eye can see the
    /// search can find — including markup, which no cell contains.
    fn text_find_step(&mut self, search: &Search, forward: bool) {
        let text = String::from_utf8_lossy(&self.text_bytes).into_owned();
        // From the end of the current line going forward and its start
        // going back, so a second `n` leaves the line it is on.
        let line = self.grid.cursor.0;
        let from = match self.text_spans.get(line) {
            Some(&(_, b)) if forward => b.saturating_sub(1),
            Some(&(a, _)) => a,
            None => 0,
        };
        let Some(at) = search.find_in_text(&text, from, forward) else {
            self.status = format!("no match for /{}", search.pattern());
            return;
        };
        let Some(row) = self.line_of(at) else {
            self.status = format!("no match for /{}", search.pattern());
            return;
        };
        let (rows, cols) = self.grid_dims();
        self.grid.move_to(row, 0, rows, cols);
        self.status = format!("/{}", search.pattern());
    }

    /// The same jump, through the tree instead of the sheet.
    ///
    /// Walks every node whether or not its ancestors are open, matches on
    /// the key and the value alike, then opens the way down to the hit so
    /// the cursor lands on the row that actually matched rather than on
    /// the closed container above it.
    fn tree_find_step(&mut self, search: &Search, forward: bool) {
        let Some(root) = self.tree_root() else {
            self.status = "nothing to search".into();
            return;
        };
        let all = crate::tree::flatten(&root);
        if all.is_empty() {
            self.status = format!("no match for /{}", search.pattern());
            return;
        }
        let here = self.tree_rows.get(self.grid.cursor.0).map(|r| &r.path);
        let at = here.and_then(|p| all.iter().position(|e| e.path == *p));
        // From where we are, wrapping once: `n` and `N` walk the whole
        // document rather than stopping at the end of it.
        let n = all.len();
        let start = match at {
            Some(i) => i,
            None if forward => n - 1,
            None => 0,
        };
        let hit = (1..=n).map(|k| {
            if forward {
                (start + k) % n
            } else {
                (start + 2 * n - k) % n
            }
        });
        let Some(i) = hit
            .into_iter()
            .find(|&i| search.matches(&all[i].label) || search.matches(&all[i].summary))
        else {
            self.status = format!("no match for /{}", search.pattern());
            return;
        };

        let path = all[i].path.clone();
        // Every ancestor, or the row is not drawn and the cursor has
        // nowhere to land.
        for depth in 0..path.len() {
            self.expansion.open(&path[..depth]);
        }
        self.rebuild_tree();
        let Some(row) = self.tree_rows.iter().position(|r| r.path == path) else {
            self.status = format!("no match for /{}", search.pattern());
            return;
        };
        let (rows, cols) = self.grid_dims();
        self.grid.move_to(row, 0, rows, cols);
        self.status = format!("/{}", search.pattern());
    }

    /// Set up a replacement and go to the first match.
    ///
    /// The status says the scope out loud. A filter narrows what is
    /// replaced — which is usually the point, and is also the sort of
    /// thing that quietly rewrites the wrong four hundred rows if nobody
    /// says it.
    fn begin_substitution(&mut self, with: String) {
        let Some(pattern) = self.pending_pattern.take() else {
            return;
        };
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
        if !self.can_substitute() {
            self.status = "replacing works in the table, where the cursor is a cell".into();
            return;
        }
        let scope = self.substitution_scope();
        self.search = Some(search.clone());
        self.substitution = Some((search, with.clone()));
        self.find_step(true);
        self.status =
            format!("replace {pattern} with {with}{scope} — . replaces, n skips, a does the rest");
    }

    /// How the scope reads in the status line.
    fn substitution_scope(&self) -> String {
        match self.substitution_note() {
            Some(note) => format!(" — {note}"),
            None => String::new(),
        }
    }

    /// The rows a replacement may touch, as source rows.
    ///
    /// With a filter on, only what the filter shows. A row hidden from you
    /// is not a row you asked to change.
    fn substitution_rows(&self) -> Vec<usize> {
        let Some(sheet) = self.doc.sheet() else {
            return Vec::new();
        };
        let (rows, _) = sheet.dims();
        let first = usize::from(sheet.header_is_first_row());
        match &self.grid.visible {
            Some(visible) => visible.iter().copied().filter(|r| *r >= first).collect(),
            None => (first..rows).collect(),
        }
    }

    /// Replace what the cursor is on, then move to the next match.
    fn substitute_one(&mut self) -> Effect {
        let Some((search, with)) = self.substitution.clone() else {
            self.status = "nothing to replace — % sets one up".into();
            return Effect::None;
        };
        let row = self.grid.source_row(self.grid.cursor.0);
        let column = self.source_col(self.grid.cursor.1);
        let Some(before) = self.doc.sheet().and_then(|s| s.cell(row, column)) else {
            return Effect::None;
        };
        if !search.matches(&before) {
            self.status = "no match here — n moves to the next".into();
            return Effect::None;
        }
        let after = search.replace_in(&before, &with);
        match self.doc.set_cell(row, column, &after) {
            Ok(()) => {
                self.mark_changed();
                self.after_edit();
                self.status = format!("replaced — {after}");
                self.find_step(true);
            }
            Err(e) => self.status = e.to_string(),
        }
        Effect::None
    }

    /// Replace every match in scope, as one edit.
    fn substitute_all(&mut self) -> Effect {
        let Some((search, with)) = self.substitution.clone() else {
            self.status = "nothing to replace — % sets one up".into();
            return Effect::None;
        };
        let Some(sheet) = self.doc.sheet() else {
            self.status = "replacing needs a table view".into();
            return Effect::None;
        };
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        let mut cells = 0usize;
        // Only the columns on display, for the reason the filter gets the
        // same treatment: a column you have put away is not one you asked
        // to change.
        let columns = self.visible_columns();
        for row in self.substitution_rows() {
            for &column in &columns {
                let Some(before) = sheet.cell(row, column) else {
                    continue;
                };
                if !search.matches(&before) {
                    continue;
                }
                let after = search.replace_in(&before, &with);
                if after == before {
                    continue;
                }
                cells += 1;
                edits.push((row, column, after));
            }
        }
        if edits.is_empty() {
            self.status = format!("nothing matched /{}", search.pattern());
            return Effect::None;
        }
        // One undo step, through the path each format actually supports.
        match self.doc.set_cells(&edits) {
            Ok(_) => {
                self.mark_changed();
                self.after_edit();
                let scope = self.substitution_scope();
                self.status = format!("replaced {cells} cells{scope} — u undoes all of it");
            }
            Err(e) => self.status = e.to_string(),
        }
        Effect::None
    }

    fn commit_prompt(&mut self, kind: PromptKind, pattern: String) {
        // An empty replacement means "delete what matched", which is a
        // real thing to ask for — so it does not count as an empty
        // prompt.
        if kind == PromptKind::SubstituteWith {
            self.begin_substitution(pattern);
            return;
        }
        if pattern.is_empty() {
            // An empty filter is not a filter. Emptying the field and
            // pressing Enter is how every search box is turned off, so it
            // turns this one off too rather than silently doing nothing.
            if kind == PromptKind::Filter && self.filter.is_some() {
                self.execute(Command::ClearFilter);
            }
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
            PromptKind::SubstituteFind => {
                // Held, not applied: the replacement is asked for next.
                self.pending_pattern = Some(pattern);
                self.mode = Mode::Prompt {
                    kind: PromptKind::SubstituteWith,
                    entry: Entry::default(),
                };
            }
            PromptKind::SubstituteWith => self.begin_substitution(pattern),
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
        match self.view {
            // Widths decide whether a value still fits its column, so a
            // stale one sends a barely-longer value to the large editor.
            ViewMode::Table => self.rebuild_table_widths(),
            ViewMode::Tree => self.rebuild_tree(),
            ViewMode::Text => {}
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

        self.block = None;
        self.text_lines = self
            .text_spans
            .iter()
            .map(|&(a, b)| {
                let line = String::from_utf8_lossy(&self.text_bytes[a..b]).into_owned();
                // Decoding is a view: the bytes and the spans behind it are
                // untouched, so an untouched line is written back exactly.
                if self.decoded_text {
                    crate::decode(&line)
                } else {
                    line
                }
            })
            .collect();
        self.text_widest = self
            .text_lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        self.rebuild_line_indents();
    }

    /// Work out every line's display indent, block by block.
    ///
    /// Each block is found once and then skipped over, so this is one
    /// pass over the source rather than one scan per line.
    fn rebuild_line_indents(&mut self) {
        let mut indents = vec![0usize; self.text_lines.len()];
        if !self.doc.is_xml() {
            self.text_indents = indents;
            return;
        }
        // One pass over the bytes, not one search per line. Asking the
        // block machinery for every line was quadratic: on a feed of
        // seventy thousand lines, switching to the source view took four
        // and a half minutes.
        //
        // A line needs shifting when it *starts* inside a value — inside
        // a CDATA section, or between a start tag and the next `<`. Its
        // own indentation is then whatever the value happens to contain,
        // which is usually nothing at all.
        let bytes = std::mem::take(&mut self.text_bytes);
        let leading = |line: usize, lines: &[String]| -> usize {
            lines
                .get(line)
                .map_or(0, |l| l.len() - l.trim_start_matches([' ', '\t']).len())
        };
        let (mut line, mut i) = (0usize, 0usize);
        let (mut in_cdata, mut in_text, mut starting) = (false, false, false);
        let mut owner = 0usize;
        while i < bytes.len() {
            if in_cdata {
                if bytes[i..].starts_with(b"]]>") {
                    in_cdata = false;
                    i += 3;
                    continue;
                }
            } else if bytes[i..].starts_with(b"<![CDATA[") {
                in_cdata = true;
                owner = leading(line, &self.text_lines);
                i += 9;
                continue;
            } else if bytes[i] == b'<' {
                in_text = false;
                // A start tag opens a value; an end tag closes one, and
                // what follows it is the whitespace between elements —
                // which is the file's own indentation, not a value's.
                starting = bytes.get(i + 1) != Some(&b'/');
            } else if bytes[i] == b'>' {
                in_text = starting && bytes.get(i.wrapping_sub(1)) != Some(&b'/');
                owner = leading(line, &self.text_lines);
            }
            if bytes[i] == b'\n' {
                line += 1;
                // Only where the line has no indentation of its own. A
                // pretty-printed file puts `\n  ` between its tags, and
                // that whitespace *is* the indentation; the lines worth
                // shifting are the ones a value left at column zero.
                if (in_cdata || in_text)
                    && line < indents.len()
                    && leading(line, &self.text_lines) == 0
                {
                    indents[line] = owner;
                }
            }
            i += 1;
        }
        self.text_bytes = bytes;
        self.text_indents = indents;
    }

    /// How far in one line of the source is drawn.
    pub fn line_indent(&self, line: usize) -> usize {
        self.text_indents.get(line).copied().unwrap_or(0)
    }

    /// How many characters the longest line holds, for a frontend sizing
    /// its scroll area.
    pub fn widest_line(&self) -> usize {
        self.text_widest
    }

    /// Commit an edited source line: splice it into the original bytes and
    /// re-parse. A line that makes the document invalid is refused with the
    /// parse error, leaving the file as it was.
    fn commit_text_line(&mut self, line: usize, value: &str) {
        let Some(&(start, end)) = self.text_spans.get(line) else {
            return;
        };
        // Typed against decoded text, so it has to be encoded again — but
        // only if this line was encoded to begin with. A line inside a
        // CDATA section holds its markup literally, and encoding that
        // would turn `<li>` in the file into `&lt;li&gt;`.
        let raw = String::from_utf8_lossy(&self.text_bytes[start..end]).into_owned();
        let encoded;
        let value = if self.decoded_text && crate::decode(&raw) != raw {
            encoded = crate::encode(value);
            encoded.as_str()
        } else {
            value
        };
        self.splice_source(start, end, value);
    }

    /// Put `value` in place of the source between two byte offsets, and
    /// re-parse. Anything that would make the document invalid is refused
    /// with the parse error, leaving the file as it was.
    fn splice_source(&mut self, start: usize, end: usize, value: &str) {
        let mut bytes = Vec::with_capacity(self.text_bytes.len() + value.len());
        bytes.extend_from_slice(&self.text_bytes[..start]);
        bytes.extend_from_slice(value.as_bytes());
        bytes.extend_from_slice(&self.text_bytes[end..]);

        match self.doc.replace_source(&bytes) {
            Ok(()) => {
                self.mark_changed();
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
        // A width the user chose wins over the measured one, so an edit
        // that re-measures does not undo a column they widened.
        for (col, chars) in &self.manual_widths {
            if let Some(w) = widths.get_mut(*col) {
                *w = *chars;
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
    fn toggle_row(&mut self) -> Effect {
        let Some(row) = self.tree_rows.get(self.grid.cursor.0).cloned() else {
            return Effect::None;
        };
        if !row.is_container() {
            // Through the same door `i` uses, so Enter on a description
            // opens the editor that can hold it rather than putting a
            // thousand characters on one line at the bottom of the screen.
            return self.execute(Command::EditCell);
        }
        self.expansion.toggle(&row.path);
        self.rebuild_tree();
        self.clamp_cursor();
        Effect::None
    }

    fn start_edit(&mut self) {
        let (r, c) = self.grid.cursor;
        let buf = match self.view {
            // Decoded, because `encode_for_cell` encodes what comes back:
            // starting from the raw text would turn `&lt;` into `&amp;lt;`
            // on the way out.
            ViewMode::Table => self
                .doc
                .sheet()
                .and_then(|s| s.cell(self.grid.source_row(r), self.source_col(c)))
                .map(|v| self.for_display(&v))
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
        // Starting at the end, the way a tweak usually wants — see
        // `Entry::at_end`.
        self.mode = Mode::Edit(Entry::at_end(buf));
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

/// The record under the cursor, read downwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspector {
    /// Where this record sits — `Row 4 of 2,298`.
    pub meta: String,
    /// What the record is called, for the heading.
    pub title: String,
    pub fields: Vec<Field>,
}

/// One field of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
    pub kind: FieldKind,
}

/// What a field's value is, for colour only. Nothing is coerced: this
/// says how to draw the text, never what the text means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    Url,
}

/// The nearest character boundary at or before `byte`, so an offset
/// worked out from a click never lands inside a character.
fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut at = byte.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// How the caret moves, so extending and not extending share one path.
#[derive(Debug, Clone, Copy)]
enum Move {
    Left,
    Right,
    Start,
    End,
}

/// Where the value under the cursor begins and ends, as lines and as
/// bytes into the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Block {
    first: usize,
    last: usize,
    start: usize,
    end: usize,
    /// The value itself: what sits between the tags, with any CDATA
    /// wrapper taken off. Editing this rather than the whole block means
    /// the tags cannot be broken by an edit, and lets the text be shown
    /// decoded — which is only safe because no markup of ours is in it.
    inner: (usize, usize),
}

/// What an element actually holds: the offsets between its opening and
/// closing tags, less a CDATA wrapper when the content is one section.
///
/// Feeds put their prose in CDATA, often carrying entity references
/// inside it. Editing the wrapper is never what anybody means, and it is
/// what stopped the value from being shown decoded.
fn inner_span(src: &[u8], start: usize, end: usize) -> (usize, usize) {
    let Some((open_end, self_closing, _)) = tag_end(src, start) else {
        return (start, end);
    };
    if self_closing {
        return (open_end, open_end);
    }
    let Some(close_start) = src[..end].iter().rposition(|&b| b == b'<') else {
        return (open_end, end);
    };
    if close_start < open_end {
        return (open_end, end);
    }
    let (mut a, mut b) = (open_end, close_start);
    // Trim whitespace so `<d>\n  <![CDATA[…]]>\n</d>` still counts.
    while a < b && src[a].is_ascii_whitespace() {
        a += 1;
    }
    while b > a && src[b - 1].is_ascii_whitespace() {
        b -= 1;
    }
    let body = &src[a..b];
    if body.starts_with(b"<![CDATA[") && body.ends_with(b"]]>") && body.len() >= 12 {
        // One section and nothing else: the value is what is inside it.
        let inside = &body[9..body.len() - 3];
        if !inside.windows(3).any(|w| w == b"]]>") {
            return (a + 9, b - 3);
        }
    }
    (open_end, close_start)
}

/// The element the source line `from..to` is part of, as the offsets of
/// its opening `<` and of the byte after its closing `>`.
///
/// Read straight from the source rather than from the parsed tree: the
/// tree does not record where each node came from, and the text view is
/// about the file as written.
///
/// Two cases count. A line that opens an element and does not close it —
/// `<description><![CDATA[…` — belongs to the element it opened. A line
/// that is text inside one, which is every line of a CDATA section,
/// belongs to the element around it. A line that opens and closes
/// everything it starts is a value in its own right, and gets `None`
/// rather than being swallowed by its parent.
fn value_element(src: &[u8], from: usize, to: usize) -> (Option<(usize, usize)>, bool) {
    let mut open: Vec<usize> = Vec::new();
    let mut i = 0usize;
    // The outermost element that starts on this line and outlives it.
    let mut starts_here: Option<(usize, usize)> = None;
    let mut inside_opaque = false;
    while i < src.len() {
        if src[i] != b'<' {
            i += 1;
            continue;
        }
        // Comments, CDATA, declarations and processing instructions hold
        // whatever they like, `<li>` included — so they are stepped over
        // rather than read as markup.
        if let Some(skip) = opaque_end(src, i) {
            if i < to && skip > from {
                inside_opaque = true;
            }
            i = skip;
            continue;
        }
        let Some((end, self_closing, closing)) = tag_end(src, i) else {
            break;
        };
        if closing {
            // Popped either way: the stack has to stay balanced.
            if let Some(start) = open.pop()
                && end >= to
            {
                if start >= from && start < to {
                    // Opened on this line. Pops run outwards, so the last
                    // one seen is the outermost of them.
                    starts_here = Some((start, end));
                } else if start < from {
                    // Nothing on the line opened it, so the line is text
                    // inside this element.
                    return (starts_here.or(Some((start, end))), inside_opaque);
                }
            }
        } else if !self_closing {
            open.push(i);
        }
        i = end;
    }
    (starts_here, inside_opaque)
}

/// True when the line closes everything it opens, so it stands on its own.
fn line_is_self_contained(line: &[u8]) -> bool {
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < line.len() {
        if line[i] != b'<' {
            i += 1;
            continue;
        }
        if let Some(skip) = opaque_end(line, i) {
            // An unterminated CDATA section runs past this line.
            if skip >= line.len() && !line[i..].ends_with(b">") {
                return false;
            }
            i = skip;
            continue;
        }
        let Some((end, self_closing, closing)) = tag_end(line, i) else {
            return false;
        };
        if closing {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        } else if !self_closing {
            depth += 1;
        }
        i = end;
    }
    depth == 0
}

/// Where a comment, CDATA section, declaration or processing instruction
/// starting at `i` ends, or `None` if this is an ordinary tag.
fn opaque_end(src: &[u8], i: usize) -> Option<usize> {
    let rest = &src[i..];
    let after = |needle: &[u8], from: usize| -> usize {
        find(src, from, needle).map_or(src.len(), |p| p + needle.len())
    };
    if rest.starts_with(b"<!--") {
        Some(after(b"-->", i + 4))
    } else if rest.starts_with(b"<![CDATA[") {
        Some(after(b"]]>", i + 9))
    } else if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
        Some(after(b">", i + 2))
    } else {
        None
    }
}

fn find(src: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= src.len() {
        return None;
    }
    src[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The end of the tag starting at `i`, and what kind it is: `(offset after
/// `>`, self-closing, closing)`. Quoted attribute values are skipped, so a
/// `>` inside one does not end the tag early.
fn tag_end(src: &[u8], i: usize) -> Option<(usize, bool, bool)> {
    let closing = src.get(i + 1) == Some(&b'/');
    let mut j = i + 1;
    let mut quote = 0u8;
    while j < src.len() {
        let c = src[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some((j + 1, src[j - 1] == b'/', closing));
        }
        j += 1;
    }
    None
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
        Node::Element(e) => e.text_content(),
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

    /// A width somebody chose has to survive the re-measure that follows
    /// an edit, or every keystroke undoes their resize.
    #[test]
    fn a_chosen_column_width_survives_editing() {
        let mut s = session("a,b\n1,2\n");
        s.set_column_width(0, 30);
        assert_eq!(s.widths()[0], 30);
        assert!(s.column_is_manual(0));

        s.grid.cursor = (0, 0);
        let _ = s.execute(Command::EditCell);
        s.input_char('x');
        let _ = s.input_submit();
        assert_eq!(s.widths()[0], 30, "the resize was undone by the edit");

        s.auto_size_column(0);
        assert!(!s.column_is_manual(0));
        assert!(s.widths()[0] < 30);
    }

    #[test]
    fn resizing_the_cursor_column_is_bounded() {
        let mut s = session("a,b\n1,2\n");
        s.grid.cursor = (0, 1);
        s.resize_cursor_column(-100);
        assert!(s.widths()[1] >= 1);
        s.resize_cursor_column(100_000);
        assert_eq!(s.widths()[1], Session::MAX_COLUMN);
        s.auto_size_all_columns();
        assert!(!s.column_is_manual(1));
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
