//! Application state and key handling for the table view.
//!
//! Key scheme: vim-flavoured, arrows always work. `i`/`Enter` edit a cell,
//! `:` opens the command line (`w`, `q`, `q!`, `wq`), `u`/`Ctrl-R` are
//! undo/redo, `gg`/`G` jump, PageUp/PageDown scroll a viewport.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vuwr_core::{CsvDoc, Document, EditOp, GridState};

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
}

pub struct App {
    pub doc: Document,
    pub grid: GridState,
    pub mode: Mode,
    pub status: String,
    pub dirty: bool,
    pub quit: bool,
    path: PathBuf,
    widths: Vec<usize>,
    pending_g: bool,
    viewport_rows: usize,
}

impl App {
    pub fn new(path: PathBuf, doc: Document) -> App {
        let widths = compute_widths(doc.as_csv().expect("phase 2: CSV only"));
        App {
            doc,
            grid: GridState::new(),
            mode: Mode::Normal,
            status: String::new(),
            dirty: false,
            quit: false,
            path,
            widths,
            pending_g: false,
            viewport_rows: 10,
        }
    }

    pub fn csv(&self) -> &CsvDoc {
        self.doc.as_csv().expect("phase 2: CSV only")
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Rendered width of each column. Computed once at load.
    pub fn widths(&self) -> &[usize] {
        &self.widths
    }

    /// Called by the renderer each frame so PageUp/PageDown know the
    /// viewport size.
    pub fn set_viewport_rows(&mut self, rows: usize) {
        self.viewport_rows = rows.max(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-C behaves like `q`: it refuses to discard unsaved changes.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.try_quit();
            return;
        }
        if matches!(self.mode, Mode::Normal) {
            self.normal_key(key);
        } else if matches!(self.mode, Mode::Edit { .. }) {
            self.edit_key(key);
        } else {
            self.command_key(key);
        }
    }

    fn normal_key(&mut self, key: KeyEvent) {
        let (rows, cols) = (self.csv().height(), self.csv().width());
        let was_pending_g = self.pending_g;
        self.pending_g = false;
        match key.code {
            KeyCode::Char('q') => self.try_quit(),
            KeyCode::Char(':') => self.mode = Mode::Command { buf: String::new() },
            KeyCode::Char('i') | KeyCode::Enter => self.start_edit(),
            KeyCode::Char('u') => {
                if self.doc.undo() {
                    self.dirty = true;
                    self.clamp_cursor();
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.doc.redo() {
                    self.dirty = true;
                    self.clamp_cursor();
                }
            }
            KeyCode::Char('g') if was_pending_g => {
                self.grid.move_to(0, self.grid.cursor.1, rows, cols)
            }
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('G') => self
                .grid
                .move_to(usize::MAX, self.grid.cursor.1, rows, cols),
            KeyCode::Left | KeyCode::Char('h') => self.grid.move_by(0, -1, rows, cols),
            KeyCode::Down | KeyCode::Char('j') => self.grid.move_by(1, 0, rows, cols),
            KeyCode::Up | KeyCode::Char('k') => self.grid.move_by(-1, 0, rows, cols),
            KeyCode::Right | KeyCode::Char('l') => self.grid.move_by(0, 1, rows, cols),
            KeyCode::PageDown => self
                .grid
                .move_by(self.viewport_rows as isize, 0, rows, cols),
            KeyCode::PageUp => self
                .grid
                .move_by(-(self.viewport_rows as isize), 0, rows, cols),
            KeyCode::Home => self.grid.move_to(self.grid.cursor.0, 0, rows, cols),
            KeyCode::End => self
                .grid
                .move_to(self.grid.cursor.0, usize::MAX, rows, cols),
            _ => {}
        }
    }

    fn start_edit(&mut self) {
        let (r, c) = self.grid.cursor;
        let buf = self
            .csv()
            .cell(r, c)
            .map(|cell| cell.value.clone())
            .unwrap_or_default();
        self.mode = Mode::Edit { buf };
    }

    fn edit_key(&mut self, key: KeyEvent) {
        let Mode::Edit { buf } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let (row, column) = self.grid.cursor;
                let value = std::mem::take(buf);
                self.mode = Mode::Normal;
                match self.doc.apply(EditOp::SetCell { row, column, value }) {
                    Ok(()) => self.dirty = true,
                    Err(e) => self.status = e.to_string(),
                }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                buf.push(c);
            }
            _ => {}
        }
    }

    fn command_key(&mut self, key: KeyEvent) {
        let Mode::Command { buf } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let cmd = std::mem::take(buf);
                self.mode = Mode::Normal;
                self.run_command(&cmd);
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                buf.push(c);
            }
            _ => {}
        }
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "w" => self.save(),
            "q" => self.try_quit(),
            "q!" => self.quit = true,
            "wq" => {
                self.save();
                if !self.dirty {
                    self.quit = true;
                }
            }
            other => self.status = format!("unknown command :{other}"),
        }
    }

    fn save(&mut self) {
        match fs::write(&self.path, self.doc.serialize()) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("wrote {}", self.path.display());
            }
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    fn try_quit(&mut self) {
        if self.dirty {
            self.status = "unsaved changes — :q! to discard, :wq to save and quit".into();
        } else {
            self.quit = true;
        }
    }

    /// After undo/redo the cursor can sit outside the restored sheet.
    fn clamp_cursor(&mut self) {
        let (rows, cols) = (self.csv().height(), self.csv().width());
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
pub(crate) fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
