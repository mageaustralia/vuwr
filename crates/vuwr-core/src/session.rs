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
use crate::view::GridState;
use crate::{Command, Document};

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
}

pub enum Mode {
    Normal,
    /// Inline edit of the cursor cell; `buf` is committed as a `SetCell`.
    Edit {
        buf: String,
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
    /// For tree view: the keys visible in the current node (or column
    /// headers for table view).
    pub tree_keys: Vec<String>,
    /// For tree view: summary strings for each visible row.
    pub tree_summaries: Vec<String>,
    /// Help overlay visibility, toggled by `?`.
    pub show_help: bool,
    /// Hint bar visibility, toggled by `H`. On by default: the bindings
    /// are not guessable, and a viewer people reach for occasionally
    /// should not require remembering them.
    pub show_hints: bool,
    /// The last search, reused by `n` and `N` and for highlighting.
    pub search: Option<Search>,
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
        let (view, widths, tree_keys, tree_summaries) = if doc.is_json() || doc.is_xml() {
            (ViewMode::Tree, vec![], vec![], vec![])
        } else {
            (
                ViewMode::Table,
                compute_widths(doc.as_csv().expect("CSV only")),
                vec![],
                vec![],
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
            tree_keys,
            tree_summaries,
            show_help: false,
            show_hints: true,
            search: None,
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
            ViewMode::Tree => (self.tree_keys.clone(), self.tree_summaries.len(), 1),
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
            ViewMode::Tree => self.tree_summaries.get(row).cloned(),
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
                ViewMode::Tree => self.tree_drill(),
                // Enter doubles as "edit" in a table, where there is
                // nothing to descend into.
                ViewMode::Table => return self.execute(Command::EditCell),
                ViewMode::Text => {}
            },
            Command::DrillUp => {
                if self.view == ViewMode::Tree {
                    self.grid.drill_up();
                    self.rebuild_tree();
                }
            }

            Command::EditCell | Command::ReplaceCell => {
                let editable = match self.view {
                    ViewMode::Table => self.doc.sheet().is_some(),
                    // Text view edits the source line itself.
                    ViewMode::Text => true,
                    ViewMode::Tree => false,
                };
                if !editable {
                    self.status = "this view is not editable".into();
                } else if cmd == Command::ReplaceCell {
                    self.mode = Mode::Edit { buf: String::new() };
                } else {
                    self.start_edit();
                }
            }
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
                if self.grid.visible.is_some() {
                    self.grid.clear_filter();
                    self.status = "filter cleared".into();
                } else {
                    self.status = "no filter".into();
                }
            }
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
            Mode::Edit { buf } => Some(('>', buf.as_str())),
            Mode::Command { buf } => Some((':', buf.as_str())),
            Mode::Prompt { kind, buf } => Some((kind.sigil(), buf.as_str())),
        }
    }

    /// Append to whatever is being entered.
    pub fn input_char(&mut self, c: char) {
        match &mut self.mode {
            Mode::Normal => {}
            Mode::Edit { buf } | Mode::Command { buf } | Mode::Prompt { buf, .. } => buf.push(c),
        }
    }

    pub fn input_backspace(&mut self) {
        match &mut self.mode {
            Mode::Normal => {}
            Mode::Edit { buf } | Mode::Command { buf } | Mode::Prompt { buf, .. } => {
                buf.pop();
            }
        }
    }

    /// Abandon what is being entered.
    pub fn input_cancel(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Accept what is being entered.
    pub fn input_submit(&mut self) -> Effect {
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
            Mode::Normal => Effect::None,
            Mode::Edit { buf } => {
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

    /// Commit an edit to the cell or source line under the cursor.
    fn commit_edit(&mut self, value: String) {
        let (row, column) = self.grid.cursor;
        if self.view == ViewMode::Text {
            self.commit_text_line(row, &value);
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
                self.search = Some(search);
                self.grid.visible = Some(rows);
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
            ViewMode::Tree => (self.tree_summaries.len(), self.tree_keys.len().max(1)),
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
        self.tree_keys.clear();
        self.tree_summaries.clear();
        if self.doc.is_json() {
            let node = self.current_json_node().clone();
            build_tree_view(&node, &mut self.tree_keys, &mut self.tree_summaries);
        } else if self.doc.is_xml() {
            let node = self.current_xml_node().clone();
            build_xml_tree_view(&node, &mut self.tree_keys, &mut self.tree_summaries);
        }
    }

    /// The JSON node we're currently viewing (respects drill-down stack).
    fn current_json_node(&self) -> &Node {
        let json = self.doc.as_json().expect("JSON doc");
        let mut node = json.root();
        for entry in &self.grid.drill_stack {
            node = match node {
                Node::Map(m) => m
                    .entries
                    .get(entry.parent_row)
                    .map(|(_, v)| v)
                    .unwrap_or(node),
                Node::Array(a) => a.items.get(entry.parent_row).unwrap_or(node),
                _ => node,
            };
        }
        node
    }

    /// The XML node we're currently viewing (respects drill-down stack).
    fn current_xml_node(&self) -> &Node {
        let xml = self.doc.as_xml().expect("XML doc");
        let mut node = xml.root();
        for entry in &self.grid.drill_stack {
            node = match node {
                Node::Element(e) => e.children.get(entry.parent_row).unwrap_or(node),
                _ => node,
            };
        }
        node
    }

    fn tree_drill(&mut self) {
        let node = if self.doc.is_json() {
            self.current_json_node()
        } else {
            self.current_xml_node()
        };
        let (row, _col) = self.grid.cursor;
        let child = match node {
            Node::Map(m) => m.entries.get(row).map(|(_, v)| v),
            Node::Array(a) => a.items.get(row),
            Node::Element(e) => e.children.get(row),
            _ => None,
        };
        match child {
            Some(Node::Map(_)) | Some(Node::Array(_)) | Some(Node::Element(_)) => {
                self.grid.drill_down();
                self.rebuild_tree();
            }
            _ => {
                self.start_edit();
            }
        }
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
            ViewMode::Tree => {
                let node = if self.doc.is_json() {
                    self.current_json_node()
                } else {
                    self.current_xml_node()
                };
                match node {
                    Node::Map(m) => m
                        .entries
                        .get(r)
                        .map(|(_, v)| node_to_edit_string(v))
                        .unwrap_or_default(),
                    Node::Array(a) => a.items.get(r).map(node_to_edit_string).unwrap_or_default(),
                    Node::Element(e) => e
                        .children
                        .get(r)
                        .map(node_to_edit_string)
                        .unwrap_or_default(),
                    _ => String::new(),
                }
            }
        };
        self.mode = Mode::Edit { buf };
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

/// Build tree view keys and summaries for a JSON node.
fn build_tree_view(node: &Node, keys: &mut Vec<String>, summaries: &mut Vec<String>) {
    keys.clear();
    summaries.clear();
    match node {
        Node::Map(m) => {
            for (key, val) in &m.entries {
                keys.push(key.clone());
                summaries.push(summarize_node(val));
            }
        }
        Node::Array(arr) => {
            for (i, item) in arr.items.iter().enumerate() {
                keys.push(i.to_string());
                summaries.push(summarize_node(item));
            }
        }
        _ => {
            keys.push("value".to_string());
            summaries.push(summarize_node(node));
        }
    }
}

/// Summarize a node for display: `{key1, key2}` for objects, `[n]` for
/// arrays, the value for scalars.
fn summarize_node(node: &Node) -> String {
    match node {
        Node::Null => "null".to_string(),
        Node::Bool(b) => b.to_string(),
        Node::Number(s) => s.clone(),
        Node::Str(s) => format!("\"{}\"", s),
        Node::Map(m) => {
            if m.entries.is_empty() {
                "{}".to_string()
            } else {
                let keys: Vec<&str> = m.entries.iter().take(3).map(|(k, _)| k.as_str()).collect();
                // The separator belongs between the keys and the ellipsis,
                // not after the last key: `{a, b, c}`, not `{a, b, c,}`.
                let more = if m.entries.len() > 3 { ", …" } else { "" };
                format!("{{{}{}}}", keys.join(", "), more)
            }
        }
        Node::Array(arr) => {
            if arr.items.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", arr.items.len())
            }
        }
        Node::Element(e) => format!("<{}>", e.tag),
        Node::Comment(text) => format!("<!--{}-->", text.chars().take(20).collect::<String>()),
        Node::Text(s) => s.chars().take(30).collect(),
        Node::XmlDecl(_) => "<?xml?>".to_string(),
        Node::ProcessingInstruction { target, .. } => format!("<?{target}?>"),
    }
}

/// Convert a JSON node to an editable string (for inline editing).
fn node_to_edit_string(node: &Node) -> String {
    match node {
        Node::Null => String::new(),
        Node::Bool(b) => b.to_string(),
        Node::Number(s) => s.clone(),
        Node::Str(s) => s.clone(),
        Node::Text(s) => s.clone(),
        _ => String::new(),
    }
}

/// Build tree view for an XML node.
fn build_xml_tree_view(node: &Node, keys: &mut Vec<String>, summaries: &mut Vec<String>) {
    keys.clear();
    summaries.clear();
    match node {
        Node::Element(e) => {
            for (i, child) in e.children.iter().enumerate() {
                match child {
                    Node::Element(c) => {
                        keys.push(format!("<{}>", c.tag));
                        summaries.push(summarize_xml_element(c));
                    }
                    Node::Comment(text) => {
                        keys.push(format!("<!--{i}-->"));
                        summaries.push(text.trim().to_string());
                    }
                    Node::Text(text) => {
                        keys.push(format!("text{i}"));
                        summaries.push(text.trim().to_string());
                    }
                    Node::XmlDecl(_) => {
                        keys.push("<?xml?>".to_string());
                        summaries.push("declaration".to_string());
                    }
                    Node::ProcessingInstruction { target, .. } => {
                        keys.push(format!("<?{target}?>"));
                        summaries.push(target.clone());
                    }
                    _ => {
                        keys.push(i.to_string());
                        summaries.push(summarize_node(child));
                    }
                }
            }
        }
        _ => {
            keys.push("value".to_string());
            summaries.push(summarize_node(node));
        }
    }
}

/// Summarize an XML element for display.
fn summarize_xml_element(elem: &crate::Element) -> String {
    if elem.children.is_empty() {
        if elem.attributes.is_empty() {
            "/>".to_string()
        } else {
            let attrs: Vec<&str> = elem
                .attributes
                .iter()
                .take(3)
                .map(|(k, _, _)| k.as_str())
                .collect();
            format!("{}…", attrs.join(", "))
        }
    } else if elem.children.len() == 1 {
        match &elem.children[0] {
            Node::Text(t) => t.chars().take(30).collect(),
            _ => format!("[{}]", elem.children.len()),
        }
    } else {
        format!("[{}]", elem.children.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FormatHint;

    fn session(src: &str) -> Session {
        Session::new(Document::parse(src.as_bytes(), FormatHint::Auto).unwrap())
    }

    /// The summary used to end with a stray separator: `{a, b, c,}`.
    #[test]
    fn map_summaries_have_no_trailing_separator() {
        assert_eq!(
            summarize_node(&Node::Map(crate::Map {
                open: '{',
                close: '}',
                entries: vec![
                    ("a".into(), Node::Null),
                    ("b".into(), Node::Null),
                    ("c".into(), Node::Null),
                ],
                trailing_comma: false,
                inline: true,
                spaced: false,
            })),
            "{a, b, c}"
        );
    }

    #[test]
    fn map_summaries_mark_omitted_keys() {
        let s = session(r#"{"a":1,"b":2,"c":3,"d":4}"#);
        // The root's own summary is what a parent would show for it.
        let summary = summarize_node(s.doc.as_json().unwrap().root());
        assert_eq!(summary, "{a, b, c, …}");
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
