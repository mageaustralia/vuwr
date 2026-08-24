//! eframe/egui frontend for vuwr, native and `wasm32-unknown-unknown`.
//!
//! Like the TUI, this is a thin layer over [`Session`]: map input to
//! [`Command`]s, draw the result, and carry out the [`Effect`]s core
//! cannot. No behaviour is decided here — that is why the two frontends
//! cannot drift apart.

mod input;
mod table;

use std::path::PathBuf;

use eframe::egui;
use vuwr_core::{Command, Document, Effect, Session, ViewMode};

pub use input::command_for;

/// The keys help shows for a command. Exposed for tests, which assert the
/// window can never render a blank row.
pub fn keys_for_test(cmd: Command) -> &'static str {
    input::keys_for(cmd)
}

/// The GUI application.
pub struct VuwrApp {
    session: Session,
    /// Where the document came from. `None` when it was piped in or
    /// dropped into the browser, where there is nowhere to write back to.
    path: Option<PathBuf>,
    /// The last text handed out (marked rows), kept so a test or a caller
    /// can see what a copy-out produced.
    last_output: Option<String>,
    /// True when a bare `g` is waiting for a second one.
    pending_g: bool,
}

impl VuwrApp {
    pub fn new(path: Option<PathBuf>, doc: Document) -> VuwrApp {
        VuwrApp {
            session: Session::new(doc),
            path,
            last_output: None,
            pending_g: false,
        }
    }

    /// Run a command and carry out whatever it asks for.
    pub fn run(&mut self, cmd: Command, ctx: &egui::Context) {
        let effect = self.session.execute(cmd);
        self.apply(effect, ctx);
    }

    pub(crate) fn take_pending_g(&mut self) -> bool {
        std::mem::take(&mut self.pending_g)
    }

    pub(crate) fn set_pending_g(&mut self, value: bool) {
        self.pending_g = value;
    }

    /// What the last copy-out produced, if anything.
    pub fn last_output(&self) -> Option<&str> {
        self.last_output.as_deref()
    }

    pub(crate) fn apply_effect(&mut self, effect: Effect, ctx: &egui::Context) {
        self.apply(effect, ctx)
    }

    fn apply(&mut self, effect: Effect, ctx: &egui::Context) {
        match effect {
            Effect::None => {}
            Effect::Save => {
                self.save();
            }
            Effect::SaveAndQuit => {
                if self.save() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Effect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Effect::Output(text) => {
                // A GUI has no stdout worth writing to, so the marked rows
                // go to the clipboard, which is the same idea in this
                // context: hand them to whatever comes next.
                ctx.copy_text(text.clone());
                self.last_output = Some(text);
                self.session.report("marked rows copied to the clipboard");
            }
        }
    }

    /// Returns true if the document was written.
    fn save(&mut self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            return match std::fs::write(path, self.session.doc.serialize()) {
                Ok(()) => {
                    let what = path.display().to_string();
                    self.session.mark_saved(&what);
                    true
                }
                Err(e) => {
                    self.session.report(format!("save failed: {e}"));
                    false
                }
            };
        }
        // In the browser, and for piped input, there is no path to write
        // back to. Offer the document rather than failing silently.
        self.session
            .report("no file to write to — use Copy to take the document");
        false
    }

    /// The document as text, for the copy-out path where saving cannot
    /// work.
    pub fn document_text(&self) -> String {
        String::from_utf8_lossy(&self.session.doc.serialize()).into_owned()
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// The file's name, not its path: a long temp path crowded the menu
    /// bar out. The full path is still available as a tooltip.
    fn title(&self) -> String {
        let name = match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            None => "(piped)".to_string(),
        };
        if self.session.dirty {
            format!("{name} *")
        } else {
            name
        }
    }

    fn full_path(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => "read from standard input".to_string(),
        }
    }
}

impl eframe::App for VuwrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        input::handle(self, ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui, ctx));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ctx, |ui| render_view(&mut self.session, ui));

        if self.session.show_help {
            self.help_window(ctx);
        }
    }
}

impl VuwrApp {
    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Write").clicked() {
                    self.run(Command::Save, ctx);
                    ui.close();
                }
                if ui.button("Copy document").clicked() {
                    ctx.copy_text(self.document_text());
                    self.session.report("document copied to the clipboard");
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.run(Command::Quit, ctx);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Undo").clicked() {
                    self.run(Command::Undo, ctx);
                    ui.close();
                }
                if ui.button("Redo").clicked() {
                    self.run(Command::Redo, ctx);
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                // Only the views this document actually has, exactly as
                // the TUI's indicator does it.
                for view in self.session.available_views() {
                    let (label, cmd) = match view {
                        ViewMode::Table => ("Table", Command::ViewTable),
                        ViewMode::Tree => ("Tree", Command::ViewTree),
                        ViewMode::Text => ("Text", Command::ViewText),
                    };
                    if ui
                        .selectable_label(self.session.view_mode() == view, label)
                        .clicked()
                    {
                        self.run(cmd, ctx);
                        ui.close();
                    }
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keys").clicked() {
                    self.run(Command::Help, ctx);
                    ui.close();
                }
            });

            ui.separator();
            ui.label(self.title()).on_hover_text(self.full_path());
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // An open prompt takes the status line, as in the TUI.
            if let Some((sigil, buf)) = self.session.entry() {
                ui.monospace(format!("{sigil}{buf}▏"));
                return;
            }
            let (_, rows, cols) = self.session.table_dims();
            let (r, c) = self.session.grid.cursor;
            ui.monospace(match self.session.view_mode() {
                ViewMode::Text => format!("line {}/{}", r + 1, rows),
                _ => format!("row {}/{}  col {}/{}", r + 1, rows, c + 1, cols),
            });
            if !self.session.status.is_empty() {
                ui.separator();
                ui.label(&self.session.status);
            }
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Keys")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                egui::Grid::new("help").striped(true).show(ui, |ui| {
                    for cmd in Command::ALL {
                        ui.monospace(input::keys_for(*cmd));
                        ui.label(cmd.description());
                        ui.end_row();
                    }
                });
            });
        if !open {
            self.session.show_help = false;
        }
    }
}

/// Draw whichever view the session is in.
///
/// Public so it can be exercised headlessly: egui runs without a window,
/// so the drawing code is testable like anything else.
pub fn render_view(session: &mut Session, ui: &mut egui::Ui) {
    match session.view_mode() {
        ViewMode::Table => table::table(session, ui),
        ViewMode::Tree => table::tree(session, ui),
        ViewMode::Text => table::text(session, ui),
    }
}

/// Launch the native window.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(path: Option<PathBuf>, doc: Document) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "vuwr",
        options,
        Box::new(|_cc| Ok(Box::new(VuwrApp::new(path, doc)))),
    )
}
