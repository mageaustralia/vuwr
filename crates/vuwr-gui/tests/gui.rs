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
    // Fonts, then the theme that names them — the order the app uses.
    // The views ask for named text styles, and egui panics on a style or
    // a family it does not know rather than falling back.
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
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

/// The help window must not claim a binding this frontend does not have.
///
/// The GUI advertised `&` for filter and `:` for the palette while binding
/// neither, and said "menu" for two commands the File menu did not offer.
/// This is the GUI's version of the TUI's test, and it is what caught it.
#[test]
fn help_never_claims_a_binding_the_gui_does_not_have() {
    use eframe::egui::{Key, Modifiers};

    const KEYS: &[Key] = &[
        Key::F2,
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::Enter,
        Key::Escape,
        Key::Tab,
        Key::Space,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::PageDown,
        Key::Slash,
        Key::Colon,
        Key::Questionmark,
        Key::Equals,
        Key::Minus,
    ];
    let mods = [
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::COMMAND,
        Modifiers::COMMAND.plus(Modifiers::SHIFT),
    ];

    let mut reachable = std::collections::HashSet::new();
    for k in KEYS {
        for m in mods {
            for pending in [false, true] {
                if let Some(c) = vuwr_gui::command_for(*k, m, pending) {
                    reachable.insert(c);
                }
            }
        }
    }
    // Punctuation with no Key variant arrives as text.
    for c in ['&', '/', ':', '?', '<', '>'] {
        if let Some(cmd) = vuwr_gui::command_for_char(c) {
            reachable.insert(cmd);
        }
    }

    let unreachable: Vec<&str> = Command::ALL
        .iter()
        .filter(|c| {
            !reachable.contains(c)
                && !vuwr_gui::MENU_ONLY.contains(c)
                && !vuwr_gui::TOOLBAR_ONLY.contains(c)
        })
        .map(|c| c.name())
        .collect();
    assert!(
        unreachable.is_empty(),
        "help lists these but no key runs them: {unreachable:?}"
    );
}

/// Commands help calls menu-only must actually be in the menu — the menu
/// is built from this same list, so this asserts they are labelled.
#[test]
fn toolbar_only_commands_are_labelled_as_such() {
    for cmd in vuwr_gui::TOOLBAR_ONLY {
        assert_eq!(vuwr_gui::keys_for_test(*cmd), "toolbar", "{}", cmd.name());
    }
}

#[test]
fn menu_only_commands_are_labelled_as_such() {
    for cmd in vuwr_gui::MENU_ONLY {
        assert_eq!(
            vuwr_gui::keys_for_test(*cmd),
            "menu",
            "{} is menu-only, so help should say so",
            cmd.name()
        );
    }
}

#[test]
fn the_hint_bar_draws() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::ToggleHints, &ctx);
    assert!(!app.session().show_hints, "H toggles the bar off");
    app.run(Command::ToggleHints, &ctx);
    assert!(app.session().show_hints);
}

/// The bundled fonts are under licences that require their notices to be
/// distributed with the software, so they are embedded and reachable.
#[test]
fn license_notices_are_embedded_and_complete() {
    let notices = vuwr_gui::LICENSE_NOTICES;
    assert!(notices.len() >= 4, "one notice per bundled font");

    let all: String = notices.iter().map(|(_, text)| *text).collect();
    // The two that egui's crate metadata names as non-MIT obligations.
    assert!(all.contains("SIL OPEN FONT LICENSE"), "OFL notice present");
    assert!(
        all.contains("UBUNTU FONT LICENCE"),
        "Ubuntu font notice present"
    );

    for (title, text) in notices {
        assert!(!title.trim().is_empty());
        assert!(
            text.len() > 200,
            "{title} looks truncated ({} bytes)",
            text.len()
        );
    }
}

#[test]
fn the_acknowledgements_window_draws() {
    let ctx = ctx();
    let mut open = true;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        vuwr_gui::render_license_window(&mut open, ctx);
    });
    assert!(open, "the window stays open until closed");
}

/// The platform's fonts are preferred over the bundled ones, which look
/// out of place and lack most symbols. A missing font must not be fatal.
#[test]
fn the_bundled_faces_are_installed() {
    let ctx = egui::Context::default();
    let adopted = vuwr_gui::install_fonts(&ctx);
    // Five files, five families: the design names two faces and three
    // weights of one of them, and egui will not fake a weight.
    assert_eq!(
        adopted,
        vec!["sans", "sans_medium", "sans_semi", "mono", "mono_medium"],
        "the bundle did not arrive"
    );
    vuwr_gui::install_theme(&ctx);

    // And every named face actually draws.
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for name in &adopted {
                ui.label(
                    egui::RichText::new("still renders")
                        .family(egui::FontFamily::Name(name.as_str().into())),
                );
            }
        });
    });
}

/// Open and Save As go through a file dialog, which core cannot reach, so
/// they must be reachable and must not fall through to a no-op.
#[test]
fn open_and_save_as_are_bound() {
    use eframe::egui::{Key, Modifiers};
    assert_eq!(
        vuwr_gui::command_for(Key::O, Modifiers::COMMAND, false),
        Some(Command::Open)
    );
    assert_eq!(
        vuwr_gui::command_for(Key::S, Modifiers::COMMAND.plus(Modifiers::SHIFT), false),
        Some(Command::SaveAs)
    );
    assert_eq!(
        vuwr_gui::command_for(Key::S, Modifiers::COMMAND, false),
        Some(Command::Save)
    );
}

/// Undo and redo answer to the shortcuts people already have in their
/// fingers, both halves of the world's conventions.
#[test]
fn undo_and_redo_have_the_usual_shortcuts() {
    use eframe::egui::{Key, Modifiers};
    let cmd = Modifiers::COMMAND;
    assert_eq!(
        vuwr_gui::command_for(Key::Z, cmd, false),
        Some(Command::Undo)
    );
    assert_eq!(
        vuwr_gui::command_for(Key::Z, cmd.plus(Modifiers::SHIFT), false),
        Some(Command::Redo)
    );
    assert_eq!(
        vuwr_gui::command_for(Key::Y, cmd, false),
        Some(Command::Redo)
    );
    // And the vim keys still work, since the TUI shares this vocabulary.
    assert_eq!(
        vuwr_gui::command_for(Key::U, Modifiers::NONE, false),
        Some(Command::Undo)
    );
}

/// Copy must reach the clipboard, not silently copy nothing.
#[test]
fn copying_from_the_gui_puts_text_on_the_clipboard() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.run(Command::MoveDown, &ctx);
    app.run(Command::Copy, &ctx);
    assert!(
        app.session().status.contains("copied"),
        "{}",
        app.session().status
    );
}

// --- The tree's context menu ---
//
// Every item in that menu arrives at apply_tree_action. "Copy value does
// nothing" was a bug in this layer as far as anyone clicking it was
// concerned, and nothing tested it.

use vuwr_gui::{NodeAction, TreeAction};

fn context(app: &mut VuwrApp, ctx: &egui::Context, row: usize, action: NodeAction) {
    app.apply_tree_action(TreeAction::Context { row, action }, ctx);
}

fn tree_app(src: &str) -> (egui::Context, VuwrApp) {
    (ctx(), VuwrApp::new(None, doc(src)))
}

#[test]
fn context_menu_copy_value_reaches_the_clipboard() {
    let (ctx, mut app) = tree_app(r#"{"a":"hello","b":2}"#);
    context(&mut app, &ctx, 0, NodeAction::CopyValue);
    assert!(
        app.session().status.contains("copied"),
        "{}",
        app.session().status
    );
}

#[test]
fn context_menu_remove_deletes_the_node() {
    let (ctx, mut app) = tree_app(r#"{"a":1,"b":2}"#);
    context(&mut app, &ctx, 0, NodeAction::Remove);
    assert_eq!(app.document_text(), r#"{"b":2}"#);
}

#[test]
fn context_menu_duplicate_adds_a_uniquely_named_copy() {
    let (ctx, mut app) = tree_app(r#"{"a":1}"#);
    context(&mut app, &ctx, 0, NodeAction::Duplicate);
    let out = app.document_text();
    assert!(out.contains(r#""a":1"#), "the original survives: {out}");
    assert!(
        out.contains("a copy"),
        "and the copy is named apart, rather than making a duplicate key: {out}"
    );
}

#[test]
fn context_menu_inserts_after_the_selected_node() {
    for (action, expect) in [
        (NodeAction::InsertValueAfter, r#""""#),
        (NodeAction::InsertObjectAfter, "{}"),
        (NodeAction::InsertArrayAfter, "[]"),
    ] {
        let (ctx, mut app) = tree_app(r#"{"a":1}"#);
        context(&mut app, &ctx, 0, action);
        let out = app.document_text();
        assert!(
            out.contains(expect),
            "{action:?} should insert {expect}: {out}"
        );
        assert!(out.starts_with(r#"{"a":1,"#), "after, not before: {out}");
    }
}

#[test]
fn context_menu_edit_value_opens_the_inline_editor() {
    let (ctx, mut app) = tree_app(r#"{"a":"short"}"#);
    context(&mut app, &ctx, 0, NodeAction::EditValue);
    assert!(app.session().is_editing_inline());
    assert_eq!(
        app.session().entry().map(|(_, b)| b.to_string()),
        Some("short".into())
    );
}

/// The window is the path for values the inline editor refuses.
#[test]
fn context_menu_edit_in_a_window_opens_on_the_value() {
    let long = "x".repeat(400);
    let (ctx, mut app) = tree_app(&format!(r#"{{"a":"{long}"}}"#));
    context(&mut app, &ctx, 0, NodeAction::EditLarge);
    assert!(
        !app.session().is_editing_inline(),
        "the inline editor must not open on this"
    );
}

/// Selecting and toggling arrive the same way.
#[test]
fn tree_select_and_toggle_actions() {
    let (ctx, mut app) = tree_app(r#"{"a":1,"o":{"x":1}}"#);
    app.apply_tree_action(TreeAction::Select(1), &ctx);
    assert_eq!(app.session().grid.cursor.0, 1);

    let path = app.session().tree_rows[1].path.clone();
    app.apply_tree_action(TreeAction::Toggle(path), &ctx);
    assert_eq!(app.session().tree_rows.len(), 3, "the object opened");
}

/// Dragging a column boundary resizes that column.
///
/// The handles were once drawn under an allocation made to reserve the
/// header's space, which is hit-tested after them and swallowed every
/// hover and drag they existed for. Nothing about the drawing looked
/// wrong, so only driving the pointer catches it.
#[test]
fn dragging_a_column_boundary_resizes_it() {
    let mut session = Session::new(doc(r#"[{"aaa":1,"bbb":2},{"aaa":3,"bbb":4}]"#));
    session.execute(Command::ViewTable);
    let ctx = ctx();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));

    // One frame to lay the header out, so the boundary can be found.
    fn frame(
        ctx: &egui::Context,
        session: &mut Session,
        screen: egui::Rect,
        events: Vec<egui::Event>,
        pointer: Option<egui::Pos2>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        if let Some(p) = pointer {
            input.events.insert(0, egui::Event::PointerMoved(p));
        }
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| vuwr_gui::render_view(session, ui));
        });
    }
    frame(&ctx, &mut session, screen, vec![], None);

    let before = session.widths()[0];
    let handle = ctx
        .read_response(vuwr_gui::grip_id(0))
        .expect("the first column has a resize handle")
        .rect;
    let (x, y) = (handle.center().x, handle.center().y);
    let at = egui::pos2(x, y);
    let down = egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut session, screen, vec![down], Some(at));
    for step in 1..=6 {
        frame(
            &ctx,
            &mut session,
            screen,
            vec![],
            Some(egui::pos2(x + step as f32 * 10.0, y)),
        );
    }
    let end = egui::pos2(x + 60.0, y);
    frame(
        &ctx,
        &mut session,
        screen,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        Some(end),
    );

    assert!(
        session.widths()[0] > before,
        "the drag did not widen the column: {before} -> {:?}",
        session.widths()
    );
    assert!(session.column_is_manual(0));
}

/// Both of egui's styles must be ours.
///
/// egui keeps one `Style` for light and one for dark, and `set_style`
/// fills only whichever is active. We installed into one, the active
/// theme flipped when the system theme arrived a moment after startup,
/// and the other slot was egui's default — with none of the text styles
/// the views ask for by name. egui aborts the process on a name it does
/// not know, so the app died on load. An abort cannot be caught, so this
/// checks the condition that caused it.
#[test]
fn either_theme_can_be_active_without_taking_the_app_down() {
    let ctx = ctx();
    // Whichever ground egui decides on, the style behind it is ours.
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.set_theme(theme);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        assert!(
            vuwr_gui::theme_is_installed(&ctx),
            "{theme:?} has a style we did not install"
        );
    }

    // And a style that is not ours at all is put back before drawing.
    ctx.set_style(egui::Style::default());
    assert!(
        !vuwr_gui::theme_is_installed(&ctx),
        "the foreign style did not take"
    );

    // Every view, drawn on that context. This is the path that aborted.
    let mut session = Session::new(doc(SAMPLE));
    for view in session.available_views() {
        session.execute(match view {
            ViewMode::Table => Command::ViewTable,
            ViewMode::Tree => Command::ViewTree,
            ViewMode::Text => Command::ViewText,
        });
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| vuwr_gui::render_view(&mut session, ui));
        });
    }

    assert!(
        vuwr_gui::theme_is_installed(&ctx),
        "drawing did not put our own style back"
    );
}

/// egui's saved memory carries the style, and a style saved by an older
/// build would arrive without the names this one uses.
#[test]
fn egui_memory_is_not_persisted() {
    let app = VuwrApp::empty();
    assert!(
        !eframe::App::persist_egui_memory(&app),
        "a saved style would be restored over ours"
    );
}
