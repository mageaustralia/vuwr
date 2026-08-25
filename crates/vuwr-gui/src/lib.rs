//! eframe/egui frontend for vuwr, native and `wasm32-unknown-unknown`.
//!
//! Like the TUI, this is a thin layer over [`Session`]: map input to
//! [`Command`]s, draw the result, and carry out the [`Effect`]s core
//! cannot. No behaviour is decided here — that is why the two frontends
//! cannot drift apart.

mod files;
mod fonts;
mod input;
mod table;
mod toolbar;

use std::path::PathBuf;

use eframe::egui;
use vuwr_core::{Command, Document, Effect, NewNode, Session, ViewMode};

pub use input::{command_for, command_for_char};
pub use table::{NodeAction, TreeAction};

/// Adopt the platform's fonts, returning the faces used. Exposed so tests
/// can check the platform we build for actually has them.
pub fn install_fonts(ctx: &egui::Context) -> Vec<String> {
    fonts::install(ctx)
}

/// Commands the GUI offers only through the menu bar. Help says "menu" for
/// these, and the File menu builds from the same list, so the two cannot
/// disagree.
pub const MENU_ONLY: &[Command] = &[Command::SaveAndQuit, Command::ForceQuit];

/// Commands the GUI offers only on the toolbar. Help says "toolbar" for
/// these, and the toolbar builds from the same list.
pub const TOOLBAR_ONLY: &[Command] = &[
    Command::SortNumeric,
    Command::ExpandAll,
    Command::CollapseAll,
    Command::CopyRow,
];

/// The keys help shows for a command. Exposed for tests, which assert the
/// window can never render a blank row.
pub fn keys_for_test(cmd: Command) -> &'static str {
    input::keys_for(cmd)
}

/// The GUI application.
pub struct VuwrApp {
    /// `None` before anything is loaded. The browser starts here, with
    /// nothing to show until a file is dropped in.
    session: Option<Session>,
    /// Where the document came from. `None` when it was piped in or
    /// dropped into the browser, where there is nowhere to write back to.
    path: Option<PathBuf>,
    /// The last text handed out (marked rows), kept so a test or a caller
    /// can see what a copy-out produced.
    last_output: Option<String>,
    /// True when a bare `g` is waiting for a second one.
    pending_g: bool,
    /// Why the last load failed, shown in the drop zone.
    load_error: Option<String>,
    /// The Acknowledgements window.
    show_licenses: bool,
    /// Which diagnostic the bar is showing.
    diagnostic_index: usize,
    /// A paste was asked for and the clipboard has not arrived yet.
    pub(crate) want_paste: bool,
    /// The large-value editor: what is being edited, if anything.
    large_edit: Option<String>,
    /// A file dialog's answer, when it arrives.
    pending_open: files::Pending,
    pending_save: std::sync::Arc<std::sync::Mutex<Option<files::SaveResult>>>,
}

impl VuwrApp {
    /// Build the app and adopt the platform's fonts.
    ///
    /// Separate from `new` because it needs a context; callers that have
    /// one (both entry points do) should use it.
    pub fn with_context(
        ctx: &egui::Context,
        path: Option<PathBuf>,
        doc: Option<Document>,
    ) -> VuwrApp {
        fonts::install(ctx);
        match doc {
            Some(doc) => VuwrApp::new(path, doc),
            None => VuwrApp::empty(),
        }
    }

    pub fn new(path: Option<PathBuf>, doc: Document) -> VuwrApp {
        VuwrApp {
            session: Some(Session::new(doc)),
            path,
            last_output: None,
            pending_g: false,
            load_error: None,
            show_licenses: false,
            diagnostic_index: 0,
            want_paste: false,
            large_edit: None,
            pending_open: files::pending(),
            pending_save: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// An app with nothing loaded yet, waiting for a file.
    pub fn empty() -> VuwrApp {
        VuwrApp {
            session: None,
            path: None,
            last_output: None,
            pending_g: false,
            load_error: None,
            show_licenses: false,
            diagnostic_index: 0,
            want_paste: false,
            large_edit: None,
            pending_open: files::pending(),
            pending_save: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Load bytes as a document, replacing whatever is open.
    ///
    /// Returns the parse error rather than showing it, so the caller can
    /// decide: the browser reports it in the drop zone, a file dialog in
    /// the status bar.
    pub fn load(&mut self, name: Option<PathBuf>, bytes: &[u8]) -> Result<(), vuwr_core::Error> {
        let hint = name
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|ext| match ext {
                "csv" => vuwr_core::FormatHint::Csv,
                "tsv" => vuwr_core::FormatHint::Tsv,
                "json" => vuwr_core::FormatHint::Json,
                "xml" => vuwr_core::FormatHint::Xml,
                _ => vuwr_core::FormatHint::Auto,
            })
            .unwrap_or(vuwr_core::FormatHint::Auto);
        let doc = Document::parse(bytes, hint)?;
        self.session = Some(Session::new(doc));
        self.path = name;
        self.load_error = None;
        Ok(())
    }

    /// Show why a file could not be loaded. Kept separate from the
    /// session's status line because it must be visible with nothing
    /// loaded, which is exactly when there is no status line.
    pub fn report_load_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        match self.session.as_mut() {
            Some(s) => s.report(message),
            None => self.load_error = Some(message),
        }
    }

    /// Open the Acknowledgements window.
    pub fn show_acknowledgements(&mut self) {
        self.show_licenses = true;
    }

    /// True when a document is open.
    pub fn has_document(&self) -> bool {
        self.session.is_some()
    }

    /// Take whatever a file dialog has finished with.
    fn drain_file_dialogs(&mut self, ctx: &egui::Context) {
        let picked = self.pending_open.lock().ok().and_then(|mut s| s.take());
        if let Some(files::Picked { path, name, bytes }) = picked {
            let label = path.clone().unwrap_or_else(|| PathBuf::from(&name));
            match self.load(Some(label), &bytes) {
                Ok(()) => {
                    // A file chosen in the browser has no path to write
                    // back to; keep the name for the title regardless.
                    self.path = path.or_else(|| Some(PathBuf::from(&name)));
                    self.report_status(format!("opened {name}"));
                }
                Err(e) => self.report_load_error(format!("{name}: {}", e.located(&bytes))),
            }
            ctx.request_repaint();
        }

        let saved = self.pending_save.lock().ok().and_then(|mut s| s.take());
        match saved {
            Some(files::SaveResult::Written { path, name }) => {
                if path.is_some() {
                    self.path = path;
                }
                if let Some(s) = self.session.as_mut() {
                    s.mark_saved(&name);
                }
                ctx.request_repaint();
            }
            Some(files::SaveResult::Failed(e)) => self.report_status(format!("save failed: {e}")),
            None => {}
        }
    }

    /// Carry out something the tree asked for.
    ///
    /// Public so the context menu's wiring is testable: every item in it
    /// arrives here, and "Copy value does nothing" was a bug in exactly
    /// this layer as far as anyone clicking it was concerned.
    pub fn apply_tree_action(&mut self, action: table::TreeAction, ctx: &egui::Context) {
        use table::{NodeAction, TreeAction};
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match action {
            TreeAction::Select(row) => session.grid.cursor.0 = row,
            TreeAction::Toggle(path) => session.toggle_path(&path),
            TreeAction::Edit(row) => {
                session.grid.cursor.0 = row;
                self.run(Command::EditCell, ctx);
            }
            // A double-click in the table edits the cell already under the
            // cursor, which the click itself just moved.
            TreeAction::EditCurrent => self.run(Command::EditCell, ctx),
            TreeAction::RenameKey(row) => {
                session.grid.cursor.0 = row;
                session.start_rename();
            }
            TreeAction::Context { row, action } => {
                session.grid.cursor.0 = row;
                match action {
                    NodeAction::EditValue => self.run(Command::EditCell, ctx),
                    NodeAction::EditLarge => self.run(Command::EditLarge, ctx),
                    NodeAction::CopyValue => {
                        let text = session.value_text_at_cursor().unwrap_or_default();
                        ctx.copy_text(text);
                        session.report("copied to the clipboard");
                    }
                    NodeAction::Duplicate => session.duplicate_at_cursor(),
                    NodeAction::Remove => session.remove_at_cursor(),
                    NodeAction::InsertValueAfter => session.insert_after_cursor(NewNode::Value),
                    NodeAction::InsertObjectAfter => session.insert_after_cursor(NewNode::Object),
                    NodeAction::InsertArrayAfter => session.insert_after_cursor(NewNode::Array),
                }
            }
        }
    }

    /// Run a command and carry out whatever it asks for.
    pub fn run(&mut self, cmd: Command, ctx: &egui::Context) {
        match cmd {
            Command::Open => {
                files::open(self.pending_open.clone());
                self.report_status("choose a file…");
                return;
            }
            Command::SaveAs => {
                let name = self
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "untitled.json".to_string());
                let bytes = self
                    .session
                    .as_ref()
                    .map(|s| s.doc.serialize())
                    .unwrap_or_default();
                files::save_as(&name, bytes, self.pending_save.clone());
                return;
            }
            _ => {}
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let effect = session.execute(cmd);
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
            Effect::Copy(text) => {
                let n = text.chars().count();
                ctx.copy_text(text);
                self.report_status(format!("copied {n} characters"));
            }
            // egui delivers the clipboard as an event rather than on
            // demand, so ask for it and take it next frame.
            Effect::Paste => self.want_paste = true,
            Effect::EditLarge => {
                self.large_edit = self.session.as_ref().and_then(|s| s.large_edit_text());
            }
            Effect::Output(text) => {
                // A GUI has no stdout worth writing to, so the marked rows
                // go to the clipboard, which is the same idea in this
                // context: hand them to whatever comes next.
                ctx.copy_text(text.clone());
                self.last_output = Some(text);
                self.report_status("marked rows copied to the clipboard");
            }
        }
    }

    /// Returns true if the document was written.
    fn report_status(&mut self, message: impl Into<String>) {
        if let Some(s) = self.session.as_mut() {
            s.report(message);
        }
    }

    fn save(&mut self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            return match std::fs::write(path, self.session().doc.serialize()) {
                Ok(()) => {
                    let what = path.display().to_string();
                    self.session_mut().mark_saved(&what);
                    true
                }
                Err(e) => {
                    self.report_status(format!("save failed: {e}"));
                    false
                }
            };
        }
        // In the browser, and for piped input, there is no path to write
        // back to. Offer the document rather than failing silently.
        self.report_status("no file to write to — use Copy to take the document");
        false
    }

    /// The document as text, for the copy-out path where saving cannot
    /// work.
    pub fn document_text(&self) -> String {
        match &self.session {
            Some(s) => String::from_utf8_lossy(&s.doc.serialize()).into_owned(),
            None => String::new(),
        }
    }

    /// The open session. Panics if nothing is loaded — callers that can
    /// be in the empty state use [`VuwrApp::try_session`].
    pub fn session(&self) -> &Session {
        self.session.as_ref().expect("no document loaded")
    }

    pub fn session_mut(&mut self) -> &mut Session {
        self.session.as_mut().expect("no document loaded")
    }

    pub fn try_session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub fn try_session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// The file's name, not its path: a long temp path crowded the menu
    /// bar out. The full path is still available as a tooltip.
    fn title(&self) -> String {
        let name = match (&self.path, self.session.is_some()) {
            (Some(p), _) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            // Loaded but nameless: piped in, or dropped without a path.
            (None, true) => "(piped)".to_string(),
            // Nothing loaded at all, which is where the browser starts.
            (None, false) => String::new(),
        };
        if self.session.as_ref().is_some_and(|s| s.dirty) {
            format!("{name} *")
        } else {
            name
        }
    }

    fn full_path(&self) -> String {
        match (&self.path, self.session.is_some()) {
            (Some(p), _) => p.display().to_string(),
            (None, true) => "read from standard input".to_string(),
            (None, false) => "no document loaded".to_string(),
        }
    }
}

impl eframe::App for VuwrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_file_dialogs(ctx);
        input::handle(self, ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui, ctx));
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if let Some(cmd) = toolbar::toolbar(self, ui) {
                self.run(cmd, ctx);
            }
        });
        if self.session.as_ref().is_some_and(|s| s.show_detail) {
            egui::TopBottomPanel::bottom("detail")
                .resizable(true)
                .default_height(140.0)
                .show(ctx, |ui| self.detail_pane(ui));
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            self.diagnostics_bar(ui, ctx);
            self.status_bar(ui);
            self.hint_bar(ui);
        });
        let tree_action = egui::CentralPanel::default()
            .show(ctx, |ui| match self.session.as_mut() {
                Some(session) => render_view(session, ui),
                None => {
                    drop_zone(ui, self.load_error.as_deref());
                    None
                }
            })
            .inner;
        if let Some(action) = tree_action {
            self.apply_tree_action(action, ctx);
        }

        if self.session.as_ref().is_some_and(|s| s.show_help) {
            self.help_window(ctx);
        }
        if self.show_licenses {
            self.licenses_window(ctx);
        }
        self.large_edit_window(ctx);
    }
}

impl VuwrApp {
    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open…").clicked() {
                    self.run(Command::Open, ctx);
                    ui.close();
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.run(Command::Save, ctx);
                    ui.close();
                }
                if ui.button("Save As…").clicked() {
                    self.run(Command::SaveAs, ctx);
                    ui.close();
                }
                if ui.button("Copy document").clicked() {
                    ctx.copy_text(self.document_text());
                    self.report_status("document copied to the clipboard");
                    ui.close();
                }
                ui.separator();
                for cmd in MENU_ONLY {
                    let label = match cmd {
                        Command::SaveAndQuit => "Save and quit",
                        Command::ForceQuit => "Quit without saving",
                        other => other.description(),
                    };
                    if ui.button(label).clicked() {
                        self.run(*cmd, ctx);
                        ui.close();
                    }
                }
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
                let Some(session) = self.session.as_ref() else {
                    ui.label("no document");
                    return;
                };
                let current = session.view_mode();
                for view in session.available_views() {
                    let (label, cmd) = match view {
                        ViewMode::Table => ("Table", Command::ViewTable),
                        ViewMode::Tree => ("Tree", Command::ViewTree),
                        ViewMode::Text => ("Text", Command::ViewText),
                    };
                    if ui.selectable_label(current == view, label).clicked() {
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
                if ui.button("Acknowledgements").clicked() {
                    self.show_licenses = true;
                    ui.close();
                }
            });

            ui.separator();
            ui.label(self.title()).on_hover_text(self.full_path());
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_ref() else {
            ui.monospace("no document — drop a CSV, JSON or XML file here");
            return;
        };
        ui.horizontal(|ui| {
            // An open prompt takes the line, as in the TUI.
            if let Some((sigil, buf)) = session.entry() {
                // An inline edit is drawn where the value is, so repeating
                // it here is noise — and a paragraph grew the panel until
                // it swallowed the window. A `:` or `/` prompt has nowhere
                // else to live, so that one is still shown.
                if session.is_editing_inline() {
                    ui.label("editing — Enter to commit, Esc to cancel");
                } else {
                    let single: String = buf.chars().take(120).filter(|c| *c != '\n').collect();
                    let ellipsis = if buf.chars().count() > 120 { "…" } else { "" };
                    ui.monospace(format!("{sigil}{single}{ellipsis}▏"));
                }
                return;
            }
            ui.monospace(session.position_label());
            if !session.status.is_empty() {
                ui.separator();
                ui.label(&session.status);
            }
        });
    }

    /// The selected value in full, wrapped and selectable — a
    /// spreadsheet's formula bar. A table column is far narrower than a
    /// description, so most of the file is otherwise truncated away.
    fn detail_pane(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let label = session.detail_label();
        let text = session.detail_text();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&label).strong().monospace());
            ui.label(
                egui::RichText::new(format!(
                    "{} characters",
                    text.as_deref().map(|t| t.chars().count()).unwrap_or(0)
                ))
                .weak()
                .small(),
            );
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match text {
                // Selectable, so it can be copied out with the mouse the
                // way any other text can.
                Some(text) => {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).monospace())
                            .wrap()
                            .selectable(true),
                    );
                }
                None => {
                    ui.label(egui::RichText::new("nothing selected").weak());
                }
            });
    }

    /// Problems that are legal but probably wrong, with somewhere to go.
    ///
    /// A warning without a position leaves you hunting, so each one names
    /// its line and offers to take you there.
    fn diagnostics_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let diagnostics = match self.session.as_ref() {
            Some(s) => s.diagnostics(),
            None => return,
        };
        if diagnostics.is_empty() {
            return;
        }

        let shown = self.diagnostic_index.min(diagnostics.len() - 1);
        let d = &diagnostics[shown];
        let (bg, fg) = (egui::Color32::from_rgb(200, 60, 55), egui::Color32::WHITE);

        egui::Frame::new()
            .fill(bg)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("!").color(fg).strong().monospace());
                    ui.label(
                        egui::RichText::new(format!(
                            "line {}, column {}: {}",
                            d.line, d.column, d.message
                        ))
                        .color(fg),
                    );

                    // Right-aligned controls, so the message can be long.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Show me").clicked() {
                            let offset = d.offset;
                            if let Some(s) = self.session.as_mut() {
                                s.reveal(offset);
                            }
                        }
                        if diagnostics.len() > 1 {
                            if ui.small_button("›").clicked() {
                                self.diagnostic_index = (shown + 1) % diagnostics.len();
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} of {}",
                                    shown + 1,
                                    diagnostics.len()
                                ))
                                .color(fg)
                                .small(),
                            );
                            if ui.small_button("‹").clicked() {
                                self.diagnostic_index =
                                    (shown + diagnostics.len() - 1) % diagnostics.len();
                            }
                        }
                    });
                });
            });
        let _ = ctx;
    }

    fn hint_bar(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        if !session.show_hints {
            return;
        }
        let hints = session.hints();
        if hints.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for cmd in hints {
                ui.label(
                    egui::RichText::new(input::keys_for(cmd))
                        .monospace()
                        .strong(),
                );
                ui.label(egui::RichText::new(cmd.short_label()).weak());
                ui.add_space(10.0);
            }
        });
    }

    fn licenses_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_licenses;
        render_license_window(&mut open, ctx);
        self.show_licenses = open;
    }

    /// A window with room to edit a value that does not fit on a line —
    /// a description holding a paragraph of escaped HTML, say.
    fn large_edit_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.large_edit.take() else {
            return;
        };
        let mut open = true;
        let mut commit = false;
        let mut cancel = false;
        let where_from = self
            .session
            .as_ref()
            .map(|s| s.position_label())
            .unwrap_or_default();

        egui::Window::new("Edit value")
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            // Room to read a paragraph without scrolling: the point of
            // this window is that the value did not fit anywhere smaller.
            .default_size([860.0, 560.0])
            .min_width(420.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&where_from).weak().small());
                ui.add_space(6.0);

                // The editor fills the window, leaving a strip for the
                // buttons, so resizing it gives you more text rather than
                // more grey.
                let footer = 34.0;
                let height = (ui.available_height() - footer).max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), height],
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                // Wrapped, not scrolled sideways: prose is
                                // what lands here.
                                .lock_focus(true)
                                .desired_width(f32::INFINITY),
                        );
                    });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} characters, {} lines",
                            text.chars().count(),
                            text.lines().count().max(1)
                        ))
                        .weak()
                        .small(),
                    );
                });
            });

        if commit {
            if let Some(s) = self.session.as_mut() {
                s.commit_large_edit(&text);
            }
        } else if !cancel && open {
            // Still open: keep what has been typed for the next frame.
            self.large_edit = Some(text);
        }
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
        if !open && let Some(s) = self.session.as_mut() {
            s.show_help = false;
        }
    }
}

/// Acknowledgements.
///
/// The bundled fonts are distributed under licences that require their
/// notices to travel with the software, so the notices are embedded in the
/// binary rather than merely referenced. Everything else is
/// MIT/Apache-2.0-style and needs attribution, which the summary provides.
///
/// A free function so it can be drawn headlessly in tests, like
/// [`render_view`].
pub fn render_license_window(open: &mut bool, ctx: &egui::Context) {
    egui::Window::new("Acknowledgements")
        .open(open)
        .resizable(true)
        .default_size([620.0, 460.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    "vuwr is MIT OR Apache-2.0. It is built on egui and eframe \
                     (MIT OR Apache-2.0), ratatui (MIT), regex, serde and clap \
                     (MIT OR Apache-2.0), and others — all permissive.",
                );
                ui.add_space(8.0);
                ui.label(
                    "The fonts below are bundled by egui and carry licences that \
                     require these notices to be distributed with the software.",
                );
                for (title, text) in LICENSE_NOTICES {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.heading(*title);
                    ui.add_space(4.0);
                    ui.monospace(*text);
                }
            });
        });
}

/// Licence notices that must be distributed with the binary, embedded so
/// they cannot be separated from it.
pub const LICENSE_NOTICES: &[(&str, &str)] = &[
    (
        "Ubuntu Light — Ubuntu Font Licence 1.0",
        include_str!("../licenses/UFL.txt"),
    ),
    (
        "Noto Emoji — SIL Open Font License 1.1",
        include_str!("../licenses/OFL.txt"),
    ),
    (
        "Hack — MIT (bitmap fonts: Bitstream Vera / Arev)",
        include_str!("../licenses/Hack-Regular.txt"),
    ),
    (
        "emoji-icon-font — MIT",
        include_str!("../licenses/emoji-icon-font-mit-license.txt"),
    ),
];

/// What the browser shows before a file arrives.
fn drop_zone(ui: &mut egui::Ui, error: Option<&str>) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("vuwr");
        ui.add_space(12.0);
        ui.label("Drop a CSV, TSV, JSON or XML file here.");
        ui.add_space(4.0);
        ui.small("Nothing is uploaded — the file is read in this tab.");
        if let Some(error) = error {
            ui.add_space(16.0);
            ui.colored_label(egui::Color32::from_rgb(240, 120, 120), error);
        }
    });
}

/// Draw whichever view the session is in.
///
/// Public so it can be exercised headlessly: egui runs without a window,
/// so the drawing code is testable like anything else.
pub fn render_view(session: &mut Session, ui: &mut egui::Ui) -> Option<table::TreeAction> {
    match session.view_mode() {
        ViewMode::Table => table::table(session, ui).then_some(table::TreeAction::EditCurrent),
        ViewMode::Tree => table::tree(session, ui),
        ViewMode::Text => table::text(session, ui).then_some(table::TreeAction::EditCurrent),
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
        Box::new(|cc| {
            Ok(Box::new(VuwrApp::with_context(
                &cc.egui_ctx,
                path,
                Some(doc),
            )))
        }),
    )
}
