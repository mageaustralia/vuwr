//! GUI tests. egui runs without a window, so these drive the real drawing
//! and input code rather than merely proving it compiles.

use eframe::egui::{self, Key, Modifiers};
use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};
use vuwr_gui::{VuwrApp, command_for};

fn doc(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Auto).unwrap()
}

fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    // One warm-up pass so fonts and style exist before anything is drawn.
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

/// Draw one frame of a session's current view. Panics if drawing does.
fn draw(session: &mut Session) {
    let ctx = ctx();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| vuwr_gui::render_view(session, ui));
    });
}

const SAMPLE: &str = "name,city,age\nAlice,Sydney,30\nBob,Perth,25\n";

#[test]
fn every_view_draws_for_every_format() {
    let cases: [(&str, &str); 4] = [
        ("csv", SAMPLE),
        ("json array", r#"[{"a":1,"b":2}]"#),
        ("json nested", r#"{"a":{"b":1}}"#),
        (
            "xml",
            "<?xml version=\"1.0\"?><items><item n=\"1\"/></items>",
        ),
    ];
    for (label, src) in cases {
        let mut session = Session::new(doc(src));
        for view in session.available_views() {
            let cmd = match view {
                ViewMode::Table => Command::ViewTable,
                ViewMode::Tree => Command::ViewTree,
                ViewMode::Text => Command::ViewText,
            };
            session.execute(cmd);
            assert_eq!(session.view_mode(), view, "{label}: {view:?}");
            draw(&mut session); // panics on failure
        }
    }
}

/// An empty document must not panic the grid.
#[test]
fn empty_documents_draw() {
    for src in ["", "[]", "{}"] {
        let mut session = Session::new(doc(src));
        draw(&mut session);
        session.execute(Command::ViewText);
        draw(&mut session);
    }
}

// --- Input mapping ---

#[test]
fn platform_shortcuts_map_to_the_shared_commands() {
    let cmd = Modifiers::COMMAND;
    assert_eq!(command_for(Key::S, cmd, false), Some(Command::Save));
    assert_eq!(command_for(Key::Z, cmd, false), Some(Command::Undo));
    assert_eq!(
        command_for(Key::Z, cmd.plus(Modifiers::SHIFT), false),
        Some(Command::Redo)
    );
    assert_eq!(command_for(Key::F, cmd, false), Some(Command::Find));
}

/// The GUI reuses the TUI's vocabulary, so the vim-style keys work too.
#[test]
fn vim_keys_still_work_in_the_gui() {
    let none = Modifiers::NONE;
    assert_eq!(command_for(Key::J, none, false), Some(Command::MoveDown));
    assert_eq!(command_for(Key::Slash, none, false), Some(Command::Find));
    assert_eq!(command_for(Key::M, none, false), Some(Command::ToggleMark));
    assert_eq!(command_for(Key::Num3, none, false), Some(Command::ViewText));
}

#[test]
fn gg_needs_the_pending_state() {
    let none = Modifiers::NONE;
    assert_eq!(command_for(Key::G, none, true), Some(Command::GoTop));
    assert_eq!(
        command_for(Key::G, Modifiers::SHIFT, false),
        Some(Command::GoBottom)
    );
}

#[test]
fn unbound_keys_are_ignored() {
    assert_eq!(command_for(Key::F9, Modifiers::NONE, false), None);
}

// --- Behaviour, shared with the TUI by construction ---

#[test]
fn commands_drive_the_session_the_same_way() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));

    app.run(Command::ViewText, &ctx);
    assert_eq!(app.session().view_mode(), ViewMode::Text);

    app.run(Command::ViewTable, &ctx);
    app.run(Command::MoveDown, &ctx);
    assert_eq!(app.session().grid.cursor.0, 1);
}

/// Editing works in the GUI because it works in the session; the GUI adds
/// nothing but the keystroke.
#[test]
fn editing_writes_through_the_session() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::MoveDown, &ctx);
    app.run(Command::ReplaceCell, &ctx);
    for c in "Alicia".chars() {
        app.session_mut().input_char(c);
    }
    app.session_mut().input_submit();

    assert!(app.document_text().contains("Alicia,Sydney,30"));
    assert!(app.session().dirty);

    app.run(Command::Undo, &ctx);
    assert!(app.document_text().contains("Alice,Sydney,30"));
}

/// With nothing to write back to — piped input, or the browser — saving
/// must say so rather than failing silently.
#[test]
fn saving_without_a_path_reports_why() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::Save, &ctx);
    assert!(
        app.session().status.contains("no file to write to"),
        "{}",
        app.session().status
    );
}

#[test]
fn saving_writes_the_file_and_clears_dirty() {
    let dir = std::env::temp_dir().join("vuwr-gui-save");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.csv");
    std::fs::write(&path, SAMPLE).unwrap();

    let ctx = ctx();
    let mut app = VuwrApp::new(Some(path.clone()), doc(SAMPLE));
    app.run(Command::MoveDown, &ctx);
    app.run(Command::ReplaceCell, &ctx);
    for c in "Zed".chars() {
        app.session_mut().input_char(c);
    }
    app.session_mut().input_submit();
    app.run(Command::Save, &ctx);

    assert!(!app.session().dirty, "{}", app.session().status);
    assert!(std::fs::read_to_string(&path).unwrap().contains("Zed"));
    std::fs::remove_dir_all(&dir).ok();
}

/// A GUI has no stdout, so marked rows go to the clipboard instead — the
/// same idea, handing them to whatever comes next.
#[test]
fn marked_rows_are_handed_out() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::MoveDown, &ctx);
    app.run(Command::ToggleMark, &ctx);
    app.run(Command::PrintMarks, &ctx);

    assert_eq!(
        app.last_output(),
        Some("name,city,age\nAlice,Sydney,30\n"),
        "{}",
        app.session().status
    );
}

/// Search and filter come from core, so they behave identically here.
#[test]
fn filtering_applies_in_the_gui() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::Filter, &ctx);
    for c in "Sydney".chars() {
        app.session_mut().input_char(c);
    }
    app.session_mut().input_submit();

    let (_, rows, _) = app.session().table_dims();
    assert_eq!(rows, 2, "header plus one match: {}", app.session().status);
    draw(app.session_mut());
}

/// Every command the help window lists must have a key shown, or the
/// window renders blank rows.
#[test]
fn help_lists_a_key_for_every_command() {
    for cmd in Command::ALL {
        assert!(
            !vuwr_gui::keys_for_test(*cmd).trim().is_empty(),
            "{} has no key",
            cmd.name()
        );
    }
}
