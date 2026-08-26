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

/// Choose the ground, and the surface it paints. Exposed for the test
/// that checks every string can be read against the ground behind it —
/// contrast is arithmetic, so it is checked rather than eyeballed.
pub fn set_dark(on: bool) {
    theme::set_dark(on);
}

/// The surface the views are painted on, in the current mode.
pub fn surface() -> egui::Color32 {
    theme::surface()
}

/// The colour a control that cannot be used is labelled in. Faint on
/// purpose, so the contrast test knows to leave it alone.
pub fn text_disabled() -> egui::Color32 {
    theme::text_disabled()
}

/// Whether this context carries our style. Exposed for the test that
/// guards against a restored one taking the app down.
pub fn theme_is_installed(ctx: &egui::Context) -> bool {
    theme::is_installed(ctx)
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
    Command::ShowAllColumns,
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
    /// The ground the reader asked for. `update` installs the style to
    /// match; nothing else should set the ground directly.
    pub dark: bool,
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

/// A document handed to the page from outside the canvas.
///
/// The page can be *given* bytes rather than fetching them, which is the
/// point: something with cross-origin rights of its own — a userscript, an
/// extension, a host page — reads the file and pushes it here. The bytes
/// go from the origin server to this tab and nowhere else, vuwr makes no
/// network request, and "nothing is uploaded" stays literally true.
#[cfg(target_arch = "wasm32")]
mod handoff {
    use std::sync::Mutex;

    static INBOX: Mutex<Option<(String, Vec<u8>)>> = Mutex::new(None);
    /// Kept so a delivery can ask for a frame. Without it the bytes sit
    /// in the inbox until something else — a keypress, a mouse move —
    /// happens to wake the canvas, and handing a file in looks like it
    /// did nothing.
    static WAKE: Mutex<Option<eframe::egui::Context>> = Mutex::new(None);

    /// Hand a document in. Replaces anything not yet collected.
    pub fn deliver(name: String, bytes: Vec<u8>) {
        if let Ok(mut inbox) = INBOX.lock() {
            *inbox = Some((name, bytes));
        }
        if let Ok(wake) = WAKE.lock()
            && let Some(ctx) = wake.as_ref()
        {
            ctx.request_repaint();
        }
    }

    /// Collect whatever was handed in, if anything.
    pub fn take() -> Option<(String, Vec<u8>)> {
        INBOX.lock().ok()?.take()
    }

    pub fn wake_with(ctx: &eframe::egui::Context) {
        if let Ok(mut wake) = WAKE.lock() {
            *wake = Some(ctx.clone());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use handoff::deliver;
#[cfg(target_arch = "wasm32")]
pub(crate) use handoff::take as handoff_take;

impl VuwrApp {
    /// Build the app and adopt the platform's fonts.
    ///
    /// Separate from `new` because it needs a context; callers that have
    /// one (both entry points do) should use it.
    pub fn with_context(ctx: &egui::Context, path: Option<PathBuf>, doc: Option<Document>) -> Self {
        fonts::install(ctx);
        theme::set_dark(matches!(
            ctx.system_theme(),
            Some(eframe::egui::Theme::Dark)
        ));
        theme::install(ctx);
        // So a document handed in later can ask for a frame.
        #[cfg(target_arch = "wasm32")]
        handoff::wake_with(ctx);
        let mut app = match doc {
            Some(doc) => Self::new(path, doc),
            None => Self::empty(),
        };
        app.dark = theme::is_dark();
        app
    }

    pub fn new(path: Option<PathBuf>, doc: Document) -> Self {
        let mut session = Session::new(doc);
        // Open with the panel showing, where there is a record for it to
        // show. A window has the room, and a record read downwards is
        // what a feed twenty-three columns wide is hard to read without.
        // The terminal decides for itself: eighty columns is a different
        // question.
        session.show_detail = session.can_inspect();
        Self {
            session: Some(session),
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
    pub fn empty() -> Self {
        Self {
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
            .map_or(vuwr_core::FormatHint::Auto, |ext| match ext {
                "csv" => vuwr_core::FormatHint::Csv,
                "tsv" => vuwr_core::FormatHint::Tsv,
                "json" => vuwr_core::FormatHint::Json,
                "xml" => vuwr_core::FormatHint::Xml,
                _ => vuwr_core::FormatHint::Auto,
            });
        let doc = Document::parse(bytes, hint)?;
        let mut session = Session::new(doc);
        // As on the way in: a file dropped on the window opens the same
        // way one named on the command line does.
        session.show_detail = session.can_inspect();
        self.session = Some(session);
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
                let name = self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
                    || "untitled.json".to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
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
        self.apply(effect, ctx);
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
            // The window has a design; the terminal takes schemes.
            Effect::SchemeChanged(_) => {
                self.report_status("colour schemes apply to the terminal, not the window");
            }
            Effect::EditLarge => {
                self.large_edit = self
                    .session
                    .as_ref()
                    .and_then(vuwr_core::Session::large_edit_text);
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

/// A command's shortcut, spelled the way this platform spells it.
///
/// The keymap says `Ctrl-S`, which is right everywhere except a Mac,
/// where it is the wrong key and the wrong symbol. Entries with no key —
/// "menu", "toolbar" — get nothing rather than a label that is not a
/// shortcut.
fn shortcut_label(ctx: &egui::Context, cmd: Command) -> String {
    let keys = input::keys_for(cmd);
    if matches!(keys, "menu" | "toolbar") || keys.contains("double-click") {
        return String::new();
    }
    // One binding in the menu: the list is a reminder, not a reference.
    let first = keys.split(" / ").next().unwrap_or(keys);
    if ctx.os() != egui::os::OperatingSystem::Mac {
        return first.to_string();
    }
    // ⌘ is in every Mac font; ⇧ is not in all of them, and a missing
    // glyph draws as an empty box — which is worse than the word.
    // Both symbols are in the faces we bundle now, so the shortcut is
    // spelled the way a Mac spells it.
    // Mac order puts the modifier first: Shift ⌘S, not ⌘Shift S.
    first
        .replace("Ctrl-Shift-", "⇧⌘")
        .replace("Ctrl-", "⌘")
        .replace("Shift-", "⇧")
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

/// The order the views are listed in: the order their keys are in.
/// Listing them in cycle order put `1` in the middle of the list, which
/// reads as an error even though both were right.
pub(crate) const VIEW_ORDER: [ViewMode; 3] = [ViewMode::Table, ViewMode::Tree, ViewMode::Text];

/// How wide a menu is. Wide enough that the shortcut column and the
/// labels are not pressed against each other: they were, and a shortcut
/// touching its label reads as one run of characters.
const MENU_WIDTH: f32 = 230.0;

/// The inspector's width, and the width of its key column.
const INSPECTOR_WIDTH: f32 = 356.0;
const KEY_COLUMN: f32 = 132.0;
/// Height of one field row, and the inset its text sits at — the same
/// 14px the panel's header and footer use.
const FIELD_ROW: f32 = 22.0;
const FIELD_PAD: f32 = 14.0;

/// A rule along the top of the frame being drawn.
fn edge_top(ui: &egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().hline(
        ui.clip_rect().x_range(),
        rect.top().round() - 0.5,
        egui::Stroke::new(1.0_f32, theme::border()),
    );
}

/// A rule down the left edge of the panel being drawn.
fn edge_left(ui: &egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().vline(
        rect.left().round() - 0.5,
        ui.clip_rect().y_range(),
        egui::Stroke::new(1.0_f32, theme::border()),
    );
}

/// A field's colour: what the value is, for reading, never for meaning.
fn field_colour(kind: vuwr_core::FieldKind) -> egui::Color32 {
    match kind {
        vuwr_core::FieldKind::Url => theme::accent_text(),
        vuwr_core::FieldKind::Number => theme::text_body(),
        vuwr_core::FieldKind::Text => theme::text_body(),
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
    /// Do not restore egui's saved memory.
    ///
    /// eframe otherwise writes the whole of `egui::Memory` to disk on
    /// exit and reads it back on start — the style included. A style
    /// saved by an older build has none of the named text styles this one
    /// asks for, and egui aborts on a style it does not know rather than
    /// falling back: the app crashed on load, for anybody who had ever
    /// run an earlier version. The style is ours to decide, every time.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::ensure(ctx);
        // Against the ground the *style* was built for, not the one that
        // happens to be set. Those are different questions, and asking
        // the second one meant a control that set the ground itself made
        // this comparison agree with itself: the surfaces turned dark and
        // every widget kept the light mode's text colour on top of them.
        if !theme::installed_for(self.dark) {
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
                // Centred in the row rather than sitting at the top of
                // it: a 40px bar with 20px of menu at the top reads as a
                // mistake, because it is one.
                ui.horizontal_centered(|ui| self.menu_bar(ui, ctx));
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
                let asked = toolbar::toolbar(self, ui);
                edge_bottom(ui);
                if let Some(col) = asked.toggle_column
                    && let Some(session) = self.session.as_mut()
                {
                    session.toggle_column(col);
                }
                if let Some(cmd) = asked.command {
                    self.run(cmd, ctx);
                }
            });
        if self.session.as_ref().is_some_and(|s| s.show_detail) {
            egui::SidePanel::right("inspector")
                .resizable(true)
                .default_width(INSPECTOR_WIDTH)
                .min_width(240.0)
                .frame(egui::Frame::new().fill(theme::surface_sunk()))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    edge_left(ui);
                    self.inspector_panel(ui, ctx);
                });
        }
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.entry().is_some() && !s.is_editing_inline())
        {
            egui::TopBottomPanel::top("prompt")
                .frame(
                    egui::Frame::new()
                        .fill(theme::surface_sunk())
                        .inner_margin(egui::Margin::symmetric(14, 8)),
                )
                .show_separator_line(false)
                .show(ctx, |ui| {
                    self.prompt_bar(ui);
                    edge_bottom(ui);
                });
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
    /// A menu entry, with the key that does the same thing beside it.
    ///
    /// Taken from the keymap rather than written out here, so a binding
    /// and the menu cannot disagree — the same rule the help window and
    /// the hint bar follow.
    fn menu_item(ui: &mut egui::Ui, label: &str, cmd: Command) -> bool {
        let keys = shortcut_label(ui.ctx(), cmd);
        ui.add(egui::Button::new(label).shortcut_text(keys))
            .clicked()
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                ui.set_min_width(MENU_WIDTH);
                if Self::menu_item(ui, "Open…", Command::Open) {
                    self.run(Command::Open, ctx);
                    ui.close();
                }
                ui.separator();
                if Self::menu_item(ui, "Save", Command::Save) {
                    self.run(Command::Save, ctx);
                    ui.close();
                }
                if Self::menu_item(ui, "Save As…", Command::SaveAs) {
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
                    if Self::menu_item(ui, label, *cmd) {
                        self.run(*cmd, ctx);
                        ui.close();
                    }
                }
                if Self::menu_item(ui, "Quit", Command::Quit) {
                    self.run(Command::Quit, ctx);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.set_min_width(MENU_WIDTH);
                if Self::menu_item(ui, "Undo", Command::Undo) {
                    self.run(Command::Undo, ctx);
                    ui.close();
                }
                if Self::menu_item(ui, "Redo", Command::Redo) {
                    self.run(Command::Redo, ctx);
                    ui.close();
                }
            });
            let mut toggle_detail = false;
            ui.menu_button("View", |ui| {
                ui.set_min_width(MENU_WIDTH);
                // Only the views this document actually has, exactly as
                // the TUI's indicator does it.
                let Some(session) = self.session.as_ref() else {
                    ui.label("no document");
                    return;
                };
                let current = session.view_mode();
                let available = session.available_views();
                let can_inspect = session.can_inspect();
                let showing_detail = session.show_detail;
                for view in VIEW_ORDER.iter().copied().filter(|v| available.contains(v)) {
                    let (label, cmd) = match view {
                        ViewMode::Table => ("Table", Command::ViewTable),
                        ViewMode::Tree => ("Tree", Command::ViewTree),
                        ViewMode::Text => ("Text", Command::ViewText),
                    };
                    let keys = shortcut_label(ui.ctx(), cmd);
                    if ui
                        .add(egui::Button::selectable(current == view, label).shortcut_text(keys))
                        .clicked()
                    {
                        self.run(cmd, ctx);
                        ui.close();
                    }
                }
                ui.separator();
                // The panel belongs in the menu that lists the views: it
                // is one, and a shortcut nobody can find is not one.
                let (can, showing) = (can_inspect, showing_detail);
                ui.add_enabled_ui(can, |ui| {
                    let keys = shortcut_label(ui.ctx(), Command::ToggleDetail);
                    if ui
                        .add(egui::Button::selectable(showing, "Detail panel").shortcut_text(keys))
                        .on_disabled_hover_text(
                            "Nothing to show here — put the cursor inside a value",
                        )
                        .clicked()
                    {
                        toggle_detail = true;
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new("Appearance").weak());
                for (label, dark) in [("Light", false), ("Dark", true)] {
                    if ui.selectable_label(self.dark == dark, label).clicked() {
                        // Only the preference. `update` compares it to the
                        // ground in force and reinstalls the style, which
                        // is the part that matters: the palette is also an
                        // egui `Style`, and half of what is on screen is
                        // drawn from that rather than from a call site.
                        //
                        // Setting the ground here made that comparison
                        // agree with itself, so nothing was reinstalled —
                        // the surfaces flipped to dark and every widget
                        // kept the light mode's text colour on top of them.
                        self.dark = dark;
                        ui.close();
                    }
                }
            });
            if toggle_detail {
                self.run(Command::ToggleDetail, ctx);
            }
            ui.menu_button("Help", |ui| {
                ui.set_min_width(MENU_WIDTH);
                if Self::menu_item(ui, "Keys", Command::Help) {
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
            (Some(p), _) => p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
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
            // An inline edit is drawn where the value is; a prompt has
            // its own bar under the toolbar. Either way the status line
            // stays what it is — where you are, and what the document is.
            if session.is_editing_inline() {
                ui.label(
                    egui::RichText::new("editing — Enter to commit, Esc to cancel")
                        .text_style(theme::meta())
                        .color(theme::edit_ring()),
                );
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
    /// Find, filter, or a `:` command — in a bar under the toolbar.
    ///
    /// The terminal puts this on the bottom line because that is where a
    /// terminal's prompt lives. A window has somewhere better: next to
    /// the thing being searched, where a browser's find bar is, and where
    /// the eye already is after clicking Find.
    fn prompt_bar(&mut self, ui: &mut egui::Ui) {
        let mut place: Option<(egui::Response, f32)> = None;
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some((sigil, _)) = session.entry() else {
            return;
        };
        let what = match session.prompt_kind() {
            Some(vuwr_core::PromptKind::Find) => "Find",
            Some(vuwr_core::PromptKind::Filter) => "Filter rows",
            Some(vuwr_core::PromptKind::SubstituteFind) => "Replace — find what",
            Some(vuwr_core::PromptKind::SubstituteWith) => "Replace — with",
            None => "Command",
        };
        // A filter narrows what a replacement touches, which is usually
        // what you want and never something to leave unsaid.
        let note = matches!(
            session.prompt_kind(),
            Some(vuwr_core::PromptKind::SubstituteFind | vuwr_core::PromptKind::SubstituteWith)
        )
        .then(|| session.substitution_note())
        .flatten();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(
                egui::RichText::new(what)
                    .text_style(theme::micro())
                    .color(theme::text_muted()),
            );
            if let Some(note) = &note {
                ui.label(
                    egui::RichText::new(note)
                        .text_style(theme::micro())
                        .color(theme::warn_text()),
                );
            }
            egui::Frame::new()
                .fill(theme::surface())
                .stroke(egui::Stroke::new(1.0_f32, theme::accent_border()))
                .corner_radius(egui::CornerRadius::same(7))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.set_min_width(320.0);
                    ui.label(
                        egui::RichText::new(sigil.to_string())
                            .monospace()
                            .color(theme::accent_text()),
                    );
                    let response = ui.add(
                        egui::Label::new(table::caret_text(session))
                            .sense(egui::Sense::click_and_drag()),
                    );
                    let left = response.rect.left();
                    if session.entry_selection().is_none() {
                        table::draw_caret(
                            ui,
                            left + table::caret_offset(ui, session),
                            response.rect.top(),
                            response.rect.height(),
                        );
                    }
                    place = Some((response, left));
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Esc cancels")
                        .text_style(theme::meta())
                        .color(theme::text_dim()),
                );
                ui.label(
                    egui::RichText::new("Enter applies")
                        .text_style(theme::meta())
                        .color(theme::text_muted()),
                );
            });
        });

        // Clicking, dragging and double-clicking the prompt do what they
        // do in a cell: the search bar is a text field like any other.
        if let Some((response, left)) = place
            && let Some(session) = self.session.as_mut()
        {
            table::place_caret(session, &response, ui, left);
        }
    }

    /// The whole record under the cursor, read downwards.
    ///
    /// A feed row is twenty-three columns wide and the window shows five,
    /// so the fields worth checking are usually the ones off the right
    /// edge. Clicking a field takes the cursor to that column, so the
    /// table and the panel are two views of one position rather than two
    /// places to be.
    fn inspector_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        // An open panel with nothing in it is worse than a closed one: it
        // looks like the answer. This says so instead.
        let empty = !session.can_inspect();
        let inspector = session.inspector();
        let table = session.view_mode() == ViewMode::Table;
        let cursor_col = session.grid.cursor.1;
        let mut go_to = None;
        let mut edit_field = None;

        egui::Frame::new()
            .fill(theme::surface_sunk())
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 11,
                bottom: 10,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(
                            egui::RichText::new(&inspector.meta)
                                .text_style(theme::meta())
                                .color(theme::text_dim()),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&inspector.title)
                                    .text_style(theme::heading())
                                    .color(theme::text()),
                            )
                            .truncate(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.label(
                            egui::RichText::new("esc")
                                .text_style(theme::micro())
                                .color(theme::text_dim()),
                        )
                        .on_hover_text("Close the inspector (V). Double-click a field to edit it.");
                    });
                });
            });
        edge_bottom(ui);

        // The fields, keys in one column and values in another, so the
        // eye runs down the names rather than hunting across.
        let footer = 46.0;
        let height = (ui.available_height() - footer).max(60.0);
        egui::ScrollArea::vertical()
            .id_salt("inspector")
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if empty {
                    ui.add_space(FIELD_PAD);
                    ui.horizontal(|ui| {
                        ui.add_space(FIELD_PAD);
                        ui.label(
                            egui::RichText::new(
                                "Nothing to show here.\nPut the cursor inside a value.",
                            )
                            .text_style(theme::meta())
                            .color(theme::text_dim()),
                        );
                    });
                    return;
                }
                for (i, field) in inspector.fields.iter().enumerate() {
                    let selected = table && i == cursor_col;
                    // Painted into an exact row: the key column has to
                    // start at the same x on every line, or the names stop
                    // being a column and the eye has to hunt again.
                    let width = ui.available_width();
                    let key_font = ui
                        .style()
                        .text_styles
                        .get(&theme::meta())
                        .cloned()
                        .unwrap_or_default();
                    let value_font = egui::TextStyle::Monospace.resolve(ui.style());

                    // Laid out before the row is allocated, because a feed
                    // has names like `g:shopping_ads_excluded_country`
                    // that do not fit the key column on one line. The row
                    // takes its height from the name: clipping a wrapped
                    // name to a fixed row cut it in half and left the
                    // remains sitting over the field below.
                    let key = ui.painter().layout(
                        field.key.clone(),
                        key_font,
                        theme::text_muted(),
                        KEY_COLUMN - 8.0,
                    );
                    let row_height = (key.size().y + 4.0).max(FIELD_ROW);
                    let (row, response) =
                        ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::click());
                    if selected {
                        ui.painter().rect_filled(row, 0.0, theme::row_selected());
                    }
                    let clip = row.intersect(ui.clip_rect());

                    let y = row.center().y - key.size().y / 2.0;
                    // Inset like the header above it: the names were
                    // flush against the panel's edge, which reads as the
                    // list having fallen off it.
                    ui.painter().with_clip_rect(clip).galley(
                        egui::pos2(row.left() + FIELD_PAD, y),
                        key,
                        theme::text_muted(),
                    );

                    let colour = field_colour(field.kind);
                    let value = ui.painter().layout_no_wrap(
                        field.value.replace('\n', " "),
                        value_font,
                        colour,
                    );
                    let y = row.center().y - value.size().y / 2.0;
                    ui.painter().with_clip_rect(clip).galley(
                        egui::pos2(row.left() + FIELD_PAD + KEY_COLUMN, y),
                        value,
                        colour,
                    );

                    // A single click goes there — in the tree as well as
                    // the table, where the record's fields are nodes.
                    if response.clicked() {
                        go_to = Some(i);
                    }
                    // Double-click edits, as it does in the table, the
                    // tree and the text: one gesture, one meaning.
                    if response.double_clicked() {
                        edit_field = Some(i);
                    }
                    response.on_hover_text(&field.value);
                }
            });

        // The two things you do with a record you are looking at.
        egui::Frame::new()
            .fill(theme::surface_sunk())
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                edge_top(ui);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 6.0) / 2.0;
                    if ui
                        .add_sized([width, 24.0], egui::Button::new("Edit field"))
                        .clicked()
                    {
                        self.run(Command::EditCell, ctx);
                    }
                    if ui
                        .add_sized([width, 24.0], egui::Button::new("Copy row"))
                        .clicked()
                    {
                        self.run(Command::CopyRow, ctx);
                    }
                });
            });

        if let Some(field) = go_to
            && let Some(session) = self.session.as_mut()
        {
            session.focus_record_field(field);
        }
        if let Some(field) = edit_field {
            // The cursor goes to the field that was clicked first. The
            // panel shows the record wherever in it the cursor is, so
            // editing "the cursor's value" would edit whichever field it
            // had been left on.
            if let Some(session) = self.session.as_mut() {
                session.focus_record_field(field);
            }
            // In the source view the panel shows a whole value, which is
            // what the large editor is for; a line edit would take the
            // one line under the cursor instead.
            let cmd = if self.session().view_mode() == ViewMode::Text {
                Command::EditLarge
            } else {
                Command::EditCell
            };
            self.run(cmd, ctx);
        }
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
                        egui::RichText::new(format!("{} — {}", located(d), d.message)).color(fg),
                    );

                    // Right-aligned controls, so the message can be long.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Show me").clicked() {
                            let d = d.clone();
                            if let Some(s) = self.session.as_mut() {
                                s.reveal_diagnostic(&d);
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
            .map(vuwr_core::Session::position_label)
            .unwrap_or_default();

        // A definite size, decided from the screen rather than from the
        // content. An auto-sizing window measures its content, and the
        // content — a text area told to fill the space it is given —
        // measures the window; each frame the margins made the answer a
        // little larger, and the window walked off the bottom of the
        // screen. Nothing here asks how much room there is.
        let screen = ctx.content_rect().size();
        let size = egui::vec2(
            880.0_f32.min(screen.x * 0.9),
            600.0_f32.min(screen.y * 0.85),
        );
        // The body is what is left after the title row, the position
        // line and the row of buttons.
        let body = size.y - 78.0;

        egui::Window::new("Edit value")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(size)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&where_from).weak().small());
                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .max_height(body)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_sized(
                            [size.x - 8.0, body],
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                // Wrapped, not scrolled sideways: prose is
                                // what lands here.
                                .lock_focus(true)
                                .desired_width(size.x - 8.0),
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
        "IBM Plex Sans — SIL Open Font License 1.1",
        include_str!("../licenses/IBMPlexSans-OFL.txt"),
    ),
    (
        "JetBrains Mono — SIL Open Font License 1.1",
        include_str!("../licenses/JetBrainsMono-OFL.txt"),
    ),
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
    // The views ask for text styles by name and egui aborts on a name it
    // does not know, so this is checked where the drawing happens rather
    // than trusted to have been done once at startup.
    theme::ensure(ui.ctx());
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

/// Where a diagnostic is, in the terms it can honestly be stated in.
///
/// A problem in the source has a line; a value that disagrees with its
/// column has a row and a column, which are not the same thing. Printing
/// the row as a line sent people to an unrelated part of the file.
fn located(d: &vuwr_core::Diagnostic) -> String {
    match d.at {
        Some((line, column)) => format!("line {line} · col {column}"),
        None => match d.place {
            vuwr_core::Place::Cell { row, column } => {
                format!("row {} · col {}", row + 1, column + 1)
            }
            vuwr_core::Place::Text(_) => String::new(),
        },
    }
}
