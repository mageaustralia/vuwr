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

    pub fn handle_key(&mut self, key: KeyEvent) {
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
