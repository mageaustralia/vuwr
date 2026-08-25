//! The TUI's thin layer over [`Session`]: turn key presses into commands,
//! and carry out the effects core cannot (writing the file, exiting).
//!
//! Everything about *what* an action does lives in `vuwr-core`, so the GUI
//! behaves identically without sharing a line of this file.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vuwr_core::{Command, Document, Effect, Session};

use crate::keymap::Resolved;

pub struct App {
    pub session: Session,
    /// The larger editor: the value being edited, and the caret in it.
    /// A terminal has no window to open, so it is drawn as an overlay.
    pub large_edit: Option<(String, usize)>,
    /// Where the document came from. `-` means it was piped in.
    path: PathBuf,
    pub quit: bool,
    /// Text to hand back to the shell on exit, from `Ctrl-E`.
    pending_output: Option<String>,
    pending_g: bool,
}

impl std::ops::Deref for App {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.session
    }
}

impl std::ops::DerefMut for App {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

impl App {
    pub fn new(path: PathBuf, doc: Document) -> App {
        App {
            session: Session::new(doc),
            large_edit: None,
            path,
            quit: false,
            pending_output: None,
            pending_g: false,
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Take whatever should be written to stdout after the UI exits.
    pub fn take_output(&mut self) -> Option<String> {
        self.pending_output.take()
    }

    /// True while the larger editor is open.
    pub fn editing_large(&self) -> bool {
        self.large_edit.is_some()
    }

    /// Keys for the larger editor. It takes newlines, which is the whole
    /// point of it; Ctrl-S commits and Esc abandons.
    fn large_edit_key(&mut self, key: KeyEvent) {
        let Some((buf, caret)) = self.large_edit.as_mut() else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.large_edit = None,
            KeyCode::Char('s') if ctrl => {
                let (text, _) = self.large_edit.take().expect("open");
                self.session.commit_large_edit(&text);
            }
            KeyCode::Enter => {
                buf.insert(*caret, '\n');
                *caret += 1;
            }
            KeyCode::Backspace => {
                if let Some(prev) = buf[..*caret].chars().next_back() {
                    let start = *caret - prev.len_utf8();
                    buf.remove(start);
                    *caret = start;
                }
            }
            KeyCode::Delete => {
                if *caret < buf.len() {
                    buf.remove(*caret);
                }
            }
            KeyCode::Left => {
                if let Some(prev) = buf[..*caret].chars().next_back() {
                    *caret -= prev.len_utf8();
                }
            }
            KeyCode::Right => {
                if let Some(next) = buf[*caret..].chars().next() {
                    *caret += next.len_utf8();
                }
            }
            KeyCode::Home => *caret = 0,
            KeyCode::End => *caret = buf.len(),
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                buf.insert(*caret, c);
                *caret += c.len_utf8();
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.editing_large() {
            self.large_edit_key(key);
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            let effect = self.session.execute(Command::Quit);
            self.apply(effect);
            return;
        }

        // While text is being entered, keys are text — not shortcuts.
        if self.session.is_entering_text() {
            match key.code {
                KeyCode::Esc => self.session.input_cancel(),
                KeyCode::Enter => {
                    let effect = self.session.input_submit();
                    self.apply(effect);
                }
                KeyCode::Backspace => self.session.input_backspace(),
                KeyCode::Delete => self.session.input_delete(),
                // Caret movement, so an edit is an edit rather than a
                // field you can only append to.
                KeyCode::Left => self.session.input_left(),
                KeyCode::Right => self.session.input_right(),
                KeyCode::Home => self.session.input_home(),
                KeyCode::End => self.session.input_end(),
                KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => {
                    self.session.input_home()
                }
                KeyCode::Char('e') if key.modifiers == KeyModifiers::CONTROL => {
                    self.session.input_end()
                }
                KeyCode::Char(c)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    self.session.input_char(c)
                }
                _ => {}
            }
            return;
        }

        let was_pending_g = self.pending_g;
        self.pending_g = false;
        match crate::keymap::resolve(key, was_pending_g) {
            Resolved::Run(Command::EditLarge) => match self.session.large_edit_text() {
                Some(text) => {
                    let caret = text.len();
                    self.large_edit = Some((text, caret));
                }
                None => self.session.report("nothing here to edit"),
            },
            Resolved::Run(cmd) => {
                let effect = self.session.execute(cmd);
                self.apply(effect);
            }
            Resolved::PendingG => self.pending_g = true,
            Resolved::None => {}
        }
    }

    /// Carry out what the session cannot: core does no I/O.
    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Save => {
                self.save();
            }
            Effect::SaveAndQuit => {
                // Only quit if the write actually succeeded, or the edits
                // it was asked to save are gone.
                if self.save() {
                    self.quit = true;
                }
            }
            Effect::Quit => self.quit = true,
            Effect::Output(text) => {
                self.pending_output = Some(text);
                self.quit = true;
            }
            Effect::Copy(text) => match copy_to_clipboard(&text) {
                Ok(()) => {
                    let n = text.chars().count();
                    self.session.report(format!("copied {n} characters"));
                }
                Err(e) => self.session.report(format!("copy failed: {e}")),
            },
            Effect::Paste => match paste_from_clipboard() {
                Ok(text) => self.session.paste(&text),
                Err(e) => self.session.report(format!("paste failed: {e}")),
            },
        }
    }

    /// Returns true if the file was written.
    fn save(&mut self) -> bool {
        match fs::write(&self.path, self.session.doc.serialize()) {
            Ok(()) => {
                let what = self.path.display().to_string();
                self.session.mark_saved(&what);
                true
            }
            Err(e) => {
                self.session.report(format!("save failed: {e}"));
                false
            }
        }
    }
}

/// The system clipboard.
///
/// A terminal cannot reach it on its own, so this goes through the OS
/// rather than the terminal — which also means it works when vuwr is not
/// the frontmost window.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_owned()))
        .map_err(|e| e.to_string())
}

fn paste_from_clipboard() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|e| e.to_string())
}
