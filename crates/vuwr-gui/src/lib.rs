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
mod theme;
mod toolbar;

use std::path::PathBuf;

use eframe::egui;
use vuwr_core::{Command, Document, Effect, NewNode, Session, ViewMode};

pub use input::{command_for, command_for_char};
pub use table::{NodeAction, TreeAction, grip_id};

/// Adopt the platform's fonts, returning the faces used. Exposed so tests
/// can check the platform we build for actually has them.
pub fn install_fonts(ctx: &egui::Context) -> Vec<String> {
    fonts::install(ctx)
}

/// Install the palette, type scale and spacing.
///
/// Anything drawing a view has to call this first: the named text styles
/// it defines are what the views ask for, and egui panics on a style it
/// does not know rather than falling back.
pub fn install_theme(ctx: &egui::Context) {
    theme::install(ctx);
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
    Command::Lint,
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
    /// Light or dark ground. Starts from what the system asks for.
    dark: bool,
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
        theme::set_dark(matches!(
            ctx.system_theme(),
            Some(eframe::egui::Theme::Dark)
        ));
        theme::install(ctx);
        let mut app = match doc {
            Some(doc) => VuwrApp::new(path, doc),
            None => VuwrApp::empty(),
        };
        app.dark = theme::is_dark();
        app
    }

    pub fn new(path: Option<PathBuf>, doc: Document) -> VuwrApp {
        VuwrApp {
            session: Some(Session::new(doc)),
            path,
            last_output: None,
            pending_g: false,
            load_error: None,
            show_licenses: false,
            dark: false,
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
            dark: false,
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
    fn full_path(&self) -> String {
        match (&self.path, self.session.is_some()) {
            (Some(p), _) => p.display().to_string(),
            (None, true) => "read from standard input".to_string(),
            (None, false) => "no document loaded".to_string(),
        }
    }
}

/// The title row's height. Six pixels shorter where there is no window
/// to decorate, which is the web.
#[cfg(target_arch = "wasm32")]
const TITLE_HEIGHT: f32 = 34.0;
#[cfg(not(target_arch = "wasm32"))]
const TITLE_HEIGHT: f32 = 40.0;

/// A 1px rule along the bottom of the panel being drawn.
///
/// Panels draw their own separators as a shadow-ish line in the wrong
/// colour; these are the design's own dividers.
fn edge_bottom(ui: &egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().hline(
        ui.clip_rect().x_range(),
        rect.bottom().round() + 0.5,
        egui::Stroke::new(1.0_f32, theme::border()),
    );
}

/// What the document is, for the right-hand end of the status line.
fn format_label(session: &Session) -> String {
    if session.doc.is_json() {
        "JSON".into()
    } else if session.doc.is_xml() {
        "XML".into()
    } else {
        "CSV".into()
    }
}

/// A borderless action for the title row, where an outline would compete
/// with Save.
fn quiet_action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let colour = if enabled {
        theme::text_control()
    } else {
        theme::text_disabled()
    };
    let button = egui::Button::new(egui::RichText::new(label).color(colour))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE);
    ui.add_enabled(enabled, button)
}

impl eframe::App for VuwrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if theme::is_dark() != self.dark {
            theme::set_dark(self.dark);
            theme::install(ctx);
        }
        self.drain_file_dialogs(ctx);
        input::handle(self, ctx);

        // The title row. On a decorated window it sits under the OS bar
        // rather than replacing it, which is the design's own web variant:
        // filename, unsaved dot, menus and Save, no traffic lights.
        egui::TopBottomPanel::top("menu")
            .exact_height(TITLE_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::surface_chrome())
                    .inner_margin(egui::Margin::symmetric(14, 0)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                self.menu_bar(ui, ctx);
                edge_bottom(ui);
            });
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(theme::surface_sunk())
                    .inner_margin(egui::Margin::symmetric(14, 9)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                let cmd = toolbar::toolbar(self, ui);
                edge_bottom(ui);
                if let Some(cmd) = cmd {
                    self.run(cmd, ctx);
                }
            });
        if self.session.as_ref().is_some_and(|s| s.show_detail) {
            egui::TopBottomPanel::bottom("detail")
                .resizable(true)
                .default_height(140.0)
                .show(ctx, |ui| self.detail_pane(ui));
        }
        // Two rows, deliberately: position and document state on one,
        // the keys on another, so the hints read as a legend rather than
        // as more state.
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::new().fill(theme::surface_header()))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.diagnostics_bar(ui, ctx);
                egui::Frame::new()
                    .fill(theme::surface_header())
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        self.status_bar(ui);
                    });
                egui::Frame::new()
                    .fill(theme::surface_hint())
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        self.hint_bar(ui);
                    });
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
                ui.separator();
                ui.label(egui::RichText::new("Appearance").weak());
                for (label, dark) in [("Light", false), ("Dark", true)] {
                    if ui.selectable_label(self.dark == dark, label).clicked() {
                        self.dark = dark;
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

            // Right: the actions, laid out into the space that is left
            // rather than into a nested layout — nesting one inside the
            // other allocated a second, empty row under the title.
            let dirty = self.session.as_ref().is_some_and(|s| s.dirty);
            let name = self.file_name();
            let path = self.parent_path();
            let rest = ui.available_rect_before_wrap();
            ui.allocate_ui_with_layout(
                rest.size(),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.add_space(2.0);
                    if theme::primary(ui, "Save")
                        .on_hover_text("Save the file (Ctrl/Cmd+S)")
                        .clicked()
                    {
                        self.run(Command::Save, ctx);
                    }
                    theme::divider(ui);
                    let can_redo = self.session.as_ref().is_some_and(|s| s.doc.can_redo());
                    let can_undo = self.session.as_ref().is_some_and(|s| s.doc.can_undo());
                    if quiet_action(ui, "Redo", can_redo).clicked() {
                        self.run(Command::Redo, ctx);
                    }
                    if quiet_action(ui, "Undo", can_undo).clicked() {
                        self.run(Command::Undo, ctx);
                    }

                    // What is open sits in the middle of what is left,
                    // measured rather than guessed.
                    let centre = ui.available_rect_before_wrap();
                    ui.allocate_ui_with_layout(
                        centre.size(),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let text = ui
                                .painter()
                                .layout_no_wrap(
                                    name.clone(),
                                    egui::TextStyle::Monospace.resolve(ui.style()),
                                    theme::text_body(),
                                )
                                .size()
                                .x;
                            let extras = if dirty { 74.0 } else { 0.0 }
                                + if path.is_empty() { 0.0 } else { 90.0 };
                            let inset = ((centre.width() - text - extras) / 2.0).max(0.0);
                            ui.add_space(inset);
                            ui.label(
                                egui::RichText::new(&name)
                                    .monospace()
                                    .color(theme::text_body()),
                            )
                            .on_hover_text(self.full_path());
                            if dirty {
                                let (dot, _) = ui.allocate_exact_size(
                                    egui::vec2(9.0, 9.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(dot.center(), 2.5, theme::warn());
                                ui.label(
                                    egui::RichText::new("unsaved")
                                        .text_style(theme::meta())
                                        .color(theme::warn()),
                                );
                            }
                            if !path.is_empty() {
                                ui.label(
                                    egui::RichText::new(&path)
                                        .text_style(theme::meta())
                                        .color(theme::text_dim()),
                                );
                            }
                        },
                    );
                },
            );
        });
    }

    /// The file's own name, without the unsaved marker: the dot says that.
    fn file_name(&self) -> String {
        match (&self.path, self.session.is_some()) {
            (Some(p), _) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            (None, true) => "(piped)".to_string(),
            (None, false) => String::new(),
        }
    }

    /// The directory the file came from, shown beside the name. Empty on
    /// the web, where there is no path.
    fn parent_path(&self) -> String {
        let Some(path) = self.path.as_ref() else {
            return String::new();
        };
        let Some(parent) = path.parent() else {
            return String::new();
        };
        let shown = parent.display().to_string();
        if shown.is_empty() {
            return String::new();
        }
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && shown.starts_with(&home) => {
                shown.replacen(&home, "~", 1)
            }
            _ => shown,
        }
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
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.label(
                egui::RichText::new(session.position_label())
                    .text_style(theme::meta())
                    .color(theme::text_body()),
            );
            if let Some(n) = session.visible_count() {
                ui.label(
                    egui::RichText::new(format!("filtered {n}"))
                        .text_style(theme::meta())
                        .color(theme::accent_text()),
                );
            }
            // The right-hand end is what the document is, not where you
            // are in it: encoding, format, and whatever just happened.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for label in [format_label(session), "UTF-8".to_string()] {
                    ui.label(
                        egui::RichText::new(label)
                            .text_style(theme::meta())
                            .color(theme::text_muted()),
                    );
                }
                if !session.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&session.status)
                            .text_style(theme::meta())
                            .color(theme::text_muted()),
                    );
                }
            });
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
            ui.label(
                egui::RichText::new(&label)
                    .text_style(theme::heading())
                    .color(theme::text()),
            );
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
        let diagnostics: Vec<_> = match self.session.as_ref() {
            Some(s) => s.lint_results().unwrap_or_default().to_vec(),
            None => return,
        };
        if diagnostics.is_empty() {
            return;
        }

        let shown = self.diagnostic_index.min(diagnostics.len() - 1);
        let d = &diagnostics[shown];
        let fg = theme::warn_text();

        egui::Frame::new()
            .fill(theme::warn_tint())
            .stroke(egui::Stroke::new(1.0_f32, theme::warn_border()))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let count = diagnostics.len();
                    ui.label(
                        egui::RichText::new(if count == 1 {
                            "1 ISSUE".to_string()
                        } else {
                            format!("{count} ISSUES")
                        })
                        .text_style(theme::micro())
                        .strong()
                        .color(fg),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "line {} · col {} — {}",
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
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // The legend's tail is claimed first; what is left is what the
            // keys get. Laying both into the same space drew one over the
            // other once the row filled up.
            let full = ui.available_rect_before_wrap();
            let tail = 110.0_f32.min(full.width());
            let keys = egui::Rect::from_min_size(
                full.min,
                egui::vec2((full.width() - tail).max(0.0), full.height()),
            );
            ui.allocate_ui_with_layout(
                keys.size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.set_clip_rect(keys);
                    for cmd in hints {
                        theme::keycap(ui, input::keys_for(cmd));
                        ui.label(
                            egui::RichText::new(cmd.short_label())
                                .text_style(theme::meta())
                                .color(theme::text_muted()),
                        );
                        ui.add_space(12.0);
                    }
                },
            );
            // Whatever is left over, laid out from the right edge: sizing
            // the tail by hand left it wherever the keys happened to end.
            let rest = ui.available_rect_before_wrap();
            ui.allocate_ui_with_layout(
                rest.size(),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new("? all shortcuts")
                            .text_style(theme::meta())
                            .color(theme::text_dim()),
                    );
                },
            );
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
            // Bounded, because the content asks for the space it is given
            // and is then given the space it asked for: without a ceiling
            // the window crept wider and taller every frame until Save was
            // off the bottom of the screen.
            .max_size(ctx.content_rect().size() * 0.92)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&where_from).weak().small());
                ui.add_space(6.0);

                // The editor fills the window, leaving a strip for the
                // buttons, so resizing it gives you more text rather than
                // more grey.
                let footer = 34.0;
                let height = (ui.available_height() - footer).max(120.0);
                let width = ui.available_width();
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(width);
                        ui.add_sized(
                            [width, height],
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                // Wrapped, not scrolled sideways: prose is
                                // what lands here. An infinite desired
                                // width is what made the window grow.
                                .lock_focus(true)
                                .desired_width(width),
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Esc closes without saving")
                                .weak()
                                .small(),
                        );
                    });
                });
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
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
