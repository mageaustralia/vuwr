//! Rendering snapshots and key-handling tests for the table UI.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vuwr_core::{Document, FormatHint};
use vuwr_tui::{App, ViewMode};

fn app(input: &str) -> App {
    let doc = Document::parse(input.as_bytes(), FormatHint::Csv).unwrap();
    App::new(PathBuf::from("test.csv"), doc)
}

fn json_app(input: &str) -> App {
    let doc = Document::parse(input.as_bytes(), FormatHint::Auto).unwrap();
    App::new(PathBuf::from("test.json"), doc)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| vuwr_tui::ui::render(f, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn renders_grid() {
    let mut app = app("name,age\nAlice,30\nBob,25\n");
    insta::assert_snapshot!(render(&mut app, 30, 8));
}

#[test]
fn cursor_moves_with_arrows_and_vim_keys() {
    let mut a = app("a,b\n1,2\n3,4\n");
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Right));
    assert_eq!(a.grid.cursor, (1, 1));
    a.handle_key(key(KeyCode::Char('G')));
    assert_eq!(a.grid.cursor, (2, 1));
    a.handle_key(key(KeyCode::Char('g')));
    a.handle_key(key(KeyCode::Char('g')));
    assert_eq!(a.grid.cursor, (0, 1));
}

#[test]
fn edit_commits_set_cell() {
    let mut a = app("a,b\n1,2\n");
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Enter)); // start edit on "1"
    a.handle_key(key(KeyCode::Backspace));
    a.handle_key(key(KeyCode::Char('9')));
    a.handle_key(key(KeyCode::Enter)); // commit
    assert_eq!(a.doc.serialize(), b"a,b\n9,2\n");
    assert!(a.dirty);
}

#[test]
fn esc_cancels_edit() {
    let mut a = app("a\n1\n");
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Char('x')));
    a.handle_key(key(KeyCode::Esc));
    assert_eq!(a.doc.serialize(), b"a\n1\n");
    assert!(!a.dirty);
}

#[test]
fn undo_and_redo_keys() {
    let mut a = app("a\n1\n");
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Backspace));
    a.handle_key(key(KeyCode::Char('9')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Char('u')));
    assert_eq!(a.doc.serialize(), b"a\n1\n");
    a.handle_key(ctrl('r'));
    assert_eq!(a.doc.serialize(), b"a\n9\n");
}

#[test]
fn quit_refuses_to_discard_unsaved_changes() {
    let mut a = app("a\n1\n");
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Char('9')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Char('q')));
    assert!(!a.quit);
    assert!(a.status.contains("unsaved"));
    // :q! forces it.
    a.handle_key(key(KeyCode::Char(':')));
    for c in "q!".chars() {
        a.handle_key(key(KeyCode::Char(c)));
    }
    a.handle_key(key(KeyCode::Enter));
    assert!(a.quit);
}

#[test]
fn write_saves_to_disk() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("vuwr-test-{}.csv", std::process::id()));
    std::fs::write(&path, b"a\n1\n").unwrap();

    let doc = Document::parse(b"a\n1\n", FormatHint::Csv).unwrap();
    let mut a = App::new(path.clone(), doc);
    a.handle_key(key(KeyCode::Char('j')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Backspace));
    a.handle_key(key(KeyCode::Char('9')));
    a.handle_key(key(KeyCode::Enter));
    a.handle_key(key(KeyCode::Char(':')));
    a.handle_key(key(KeyCode::Char('w')));
    a.handle_key(key(KeyCode::Enter));

    assert_eq!(std::fs::read(&path).unwrap(), b"a\n9\n");
    assert!(!a.dirty);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn scrolling_follows_the_cursor() {
    let rows: String = (0..50).map(|i| format!("r{i}\n")).collect();
    let mut a = app(&rows);
    for _ in 0..20 {
        a.handle_key(key(KeyCode::Char('j')));
    }
    let out = render(&mut a, 20, 6);
    assert!(out.contains("r20"), "cursor row should be visible:\n{out}");
}

// --- JSON tree view tests ---

#[test]
fn json_opens_in_tree_mode() {
    let a = json_app("{\"a\":1,\"b\":2}");
    assert_eq!(a.view_mode(), ViewMode::Tree);
}

#[test]
fn json_tree_shows_keys_and_values() {
    let mut a = json_app("{\"name\":\"Alice\",\"age\":30}");
    let out = render(&mut a, 40, 10);
    assert!(out.contains("name"), "should show key: {out}");
    assert!(out.contains("Alice"), "should show value: {out}");
    assert!(out.contains("age"), "should show key: {out}");
}

#[test]
fn json_tree_cursor_moves() {
    let mut a = json_app("{\"a\":1,\"b\":2,\"c\":3}");
    a.handle_key(key(KeyCode::Char('j')));
    assert_eq!(a.grid.cursor, (1, 0));
    a.handle_key(key(KeyCode::Char('k')));
    assert_eq!(a.grid.cursor, (0, 0));
}

#[test]
fn json_tree_drill_down_and_up() {
    let mut a = json_app("{\"nested\":{\"x\":1},\"other\":2}");
    // Cursor is at row 0 (nested object)
    a.handle_key(key(KeyCode::Enter)); // drill into nested
    assert_eq!(a.grid.depth(), 1);
    let out = render(&mut a, 40, 10);
    assert!(out.contains("x"), "should show inner key: {out}");
    a.handle_key(key(KeyCode::Esc)); // drill back up
    assert_eq!(a.grid.depth(), 0);
}

#[test]
fn json_tree_drill_array() {
    let mut a = json_app("{\"items\":[1,2,3],\"name\":\"test\"}");
    a.handle_key(key(KeyCode::Enter)); // drill into items array
    assert_eq!(a.grid.depth(), 1);
    let out = render(&mut a, 40, 10);
    assert!(out.contains("1"), "should show array element: {out}");
}

#[test]
fn json_table_view_for_array_of_objects() {
    let mut a = json_app("[{\"a\":1,\"b\":2},{\"a\":3,\"b\":4}]");
    assert_eq!(a.view_mode(), ViewMode::Tree); // starts in tree
    a.handle_key(key(KeyCode::Tab)); // cycle to table
    assert_eq!(a.view_mode(), ViewMode::Table);
    let out = render(&mut a, 40, 10);
    assert!(out.contains("a"), "should show header: {out}");
}

/// A document with no row shape has no table view, so Tab skips straight
/// to text. It used to stay in tree with a status message, which left the
/// user with nowhere to go.
#[test]
fn json_table_skipped_for_non_array() {
    let mut a = json_app("{\"a\":1}");
    a.handle_key(key(KeyCode::Tab));
    assert_eq!(a.view_mode(), ViewMode::Text);
    a.handle_key(key(KeyCode::Tab));
    assert_eq!(a.view_mode(), ViewMode::Tree);
}

#[test]
fn json_tree_snapshot() {
    let mut a = json_app("{\n  \"name\": \"Alice\",\n  \"age\": 30,\n  \"tags\": [\"admin\"]\n}");
    insta::assert_snapshot!(render(&mut a, 40, 8));
}

fn xml_app(input: &str) -> App {
    let doc = Document::parse(input.as_bytes(), FormatHint::Auto).unwrap();
    App::new(PathBuf::from("test.xml"), doc)
}

#[test]
fn xml_opens_in_tree_mode() {
    let a = xml_app("<root><child/></root>");
    assert_eq!(a.view_mode(), ViewMode::Tree);
}

#[test]
fn xml_tree_shows_elements() {
    let mut a = xml_app("<root><item name=\"a\"/><item name=\"b\"/></root>");
    let out = render(&mut a, 40, 10);
    assert!(out.contains("<item>"), "should show element tag: {out}");
}

#[test]
fn xml_tree_drill_into_element() {
    let mut a = xml_app("<root><child>hello</child></root>");
    // Cursor at row 0 (the <child> element), drill into it
    a.handle_key(key(KeyCode::Char('i')));
    // Should now show child's content
    let out = render(&mut a, 40, 10);
    assert!(out.contains("hello"), "should show text content: {out}");
}

#[test]
fn xml_drill_up_from_root_is_noop() {
    let mut a = xml_app("<root><child/></root>");
    a.handle_key(ctrl('u'));
    assert_eq!(a.view_mode(), ViewMode::Tree);
    let out = render(&mut a, 40, 10);
    assert!(out.contains("<child"), "should still show child: {out}");
}

/// XML table mode used to render an empty grid: `table_dims` had no XML
/// branch, so an eligible document switched to Table and showed nothing.
#[test]
fn xml_table_mode_renders_rows() {
    let src = "<?xml version=\"1.0\"?>\n<items>\n  <item name=\"a\" qty=\"1\"/>\n  <item name=\"b\" qty=\"2\"/>\n</items>";
    let doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    assert_eq!(app.view_mode(), ViewMode::Tree, "XML opens as a tree");

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Table, "Tab reaches table view");

    let (headers, rows, cols) = app.table_dims();
    assert_eq!(headers, vec!["name", "qty"]);
    assert_eq!((rows, cols), (2, 2));

    let out = render(&mut app, 40, 8);
    assert!(out.contains("name"), "headers must render:\n{out}");
    assert!(
        out.contains('a') && out.contains('b'),
        "rows must render:\n{out}"
    );
}

/// Editing used to be CSV-only; a JSON edit was accepted by the UI and
/// then silently discarded by the core. It now writes through.
#[test]
fn json_cell_edits_write_through_from_the_ui() {
    let doc = Document::parse(br#"[{"name":"Alice","age":30}]"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);

    app.handle_key(key(KeyCode::Tab)); // tree -> table
    assert_eq!(app.view_mode(), ViewMode::Table);

    app.handle_key(key(KeyCode::Char('c'))); // replace, not append
    for c in "Alicia".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(
        app.dirty,
        "the edit must mark the document dirty: {}",
        app.status
    );
    assert_eq!(app.table_cell(0, 0).as_deref(), Some("Alicia"));

    app.handle_key(key(KeyCode::Char('u')));
    assert_eq!(app.table_cell(0, 0).as_deref(), Some("Alice"), "undo works");
}

/// A JSON number edited to another number stays a number in the file.
#[test]
fn json_edit_from_the_ui_preserves_type() {
    let doc = Document::parse(br#"[{"n":30}]"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Tab));
    app.handle_key(key(KeyCode::Char('c')));
    for c in "31".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        r#"[{"n":31}]"#
    );
}

#[test]
fn i_appends_to_the_existing_value_and_c_replaces_it() {
    let mut app = app("name\nAlice\n");
    app.grid.move_to(1, 0, 2, 1);

    app.handle_key(key(KeyCode::Char('i')));
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("Alice!"));

    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('Z')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("Z"));
}

// --- Command layer, text pager, help ---

#[test]
fn colon_commands_go_through_the_same_vocabulary_as_keys() {
    let mut by_name_app = app("a\n1\n");
    // `:go-bottom` and `G` must do the same thing.
    for c in ":go-bottom".chars() {
        by_name_app.handle_key(key(KeyCode::Char(c)));
    }
    by_name_app.handle_key(key(KeyCode::Enter));
    let by_name = by_name_app.grid.cursor;

    let mut app2 = app("a\n1\n");
    app2.handle_key(key(KeyCode::Char('G')));
    assert_eq!(by_name, app2.grid.cursor);
}

#[test]
fn unknown_colon_command_reports_rather_than_silently_doing_nothing() {
    let mut app = app("a\n1\n");
    for c in ":nonsense".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert!(app.status.contains("unknown command"), "{}", app.status);
}

#[test]
fn pager_keys_scroll_a_screen() {
    let mut rows = String::from("n\n");
    for i in 0..100 {
        rows.push_str(&format!("{i}\n"));
    }
    let mut app = app(&rows);
    app.set_viewport_rows(10);

    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.grid.cursor.0, 10, "space pages down");

    app.handle_key(key(KeyCode::Char('d')));
    assert_eq!(app.grid.cursor.0, 15, "d is half a page");

    app.handle_key(key(KeyCode::Char('b')));
    assert_eq!(app.grid.cursor.0, 5, "b pages back");
}

#[test]
fn text_view_pages_the_source_and_is_read_only() {
    let mut app = app("name,age\nAlice,30\nBob,25\n");
    app.handle_key(key(KeyCode::Tab)); // CSV: table -> text
    assert_eq!(app.view_mode(), ViewMode::Text);

    let (_, lines, _) = app.table_dims();
    assert_eq!(lines, 3, "three source lines");
    assert_eq!(app.table_cell(0, 0).as_deref(), Some("name,age"));

    // Editing keys must not open an editor here.
    app.handle_key(key(KeyCode::Char('i')));
    assert!(!app.dirty);
    assert!(app.status.contains("not editable"), "{}", app.status);

    let out = render(&mut app, 40, 6);
    assert!(out.contains("Alice,30"), "raw source renders:\n{out}");
    assert!(
        out.contains('1') && out.contains('2'),
        "line numbers:\n{out}"
    );
}

#[test]
fn text_view_shows_what_would_be_written() {
    let mut app = app("a,b\n1,2\n");
    app.grid.move_to(1, 0, 2, 2);
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('9')));
    app.handle_key(key(KeyCode::Enter));

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Text);
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("9,2"));
}

#[test]
fn json_cycles_tree_table_text() {
    let doc = Document::parse(br#"[{"a":1}]"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    assert_eq!(app.view_mode(), ViewMode::Tree);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Table);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Text);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Tree);
}

/// A document with no row shape skips table view rather than showing a
/// blank grid.
#[test]
fn non_table_shaped_json_cycles_tree_text_only() {
    let doc = Document::parse(br#"{"a":{"b":1}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    assert_eq!(app.view_mode(), ViewMode::Tree);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Text);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.view_mode(), ViewMode::Tree);
}

#[test]
fn help_overlay_toggles_and_lists_every_command() {
    let mut app = app("a\n1\n");
    assert!(!app.show_help);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(app.show_help);

    let out = render(&mut app, 70, 30);
    assert!(out.contains("keys"), "help renders:\n{out}");
    assert!(out.contains("undo"), "help lists commands:\n{out}");

    app.handle_key(key(KeyCode::Char('?')));
    assert!(!app.show_help);
}

// --- View discoverability ---

/// Cycling with Tab alone gave no clue the other views existed. The status
/// bar now lists every view this document supports.
#[test]
fn status_bar_lists_available_views() {
    let mut app = app("a,b\n1,2\n");
    let out = render(&mut app, 60, 6);
    assert!(
        out.contains("[table] text"),
        "CSV has table and text:\n{out}"
    );

    let doc = Document::parse(br#"[{"a":1}]"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    let out = render(&mut app, 60, 6);
    assert!(
        out.contains("[tree] table text"),
        "table-shaped JSON offers all three:\n{out}"
    );

    let doc = Document::parse(br#"{"a":{"b":1}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    let out = render(&mut app, 60, 6);
    assert!(
        out.contains("[tree] text") && !out.contains("table"),
        "nested JSON offers no table view:\n{out}"
    );
}

/// Number keys jump straight to a view, so reaching text does not require
/// knowing how many times to press Tab.
#[test]
fn number_keys_select_views_directly() {
    let doc = Document::parse(br#"[{"a":1}]"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);

    app.handle_key(key(KeyCode::Char('3')));
    assert_eq!(app.view_mode(), ViewMode::Text);
    app.handle_key(key(KeyCode::Char('1')));
    assert_eq!(app.view_mode(), ViewMode::Table);
    app.handle_key(key(KeyCode::Char('2')));
    assert_eq!(app.view_mode(), ViewMode::Tree);
}

#[test]
fn unavailable_views_report_instead_of_switching() {
    let mut app = app("a\n1\n");
    app.handle_key(key(KeyCode::Char('2'))); // no tree for CSV
    assert_eq!(app.view_mode(), ViewMode::Table);
    assert!(app.status.contains("no tree view"), "{}", app.status);

    let doc = Document::parse(br#"{"a":{"b":1}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Char('1'))); // not row-shaped
    assert_eq!(app.view_mode(), ViewMode::Tree);
    assert!(app.status.contains("not row-shaped"), "{}", app.status);
}

/// Text view must be reachable for every format — it is the one view that
/// always applies, since it is just the source.
#[test]
fn text_view_is_available_for_every_format() {
    let cases: [(&str, &[u8]); 4] = [
        ("csv", b"a,b\n1,2\n"),
        ("json array", br#"[{"a":1}]"#),
        ("json nested", br#"{"a":{"b":1}}"#),
        (
            "xml",
            b"<?xml version=\"1.0\"?><items><item name=\"a\"/></items>",
        ),
    ];
    for (label, src) in cases {
        let doc = Document::parse(src, FormatHint::Auto).unwrap();
        let mut app = App::new(PathBuf::from("t"), doc);
        app.handle_key(key(KeyCode::Char('3')));
        assert_eq!(app.view_mode(), ViewMode::Text, "{label}");
        let (_, lines, _) = app.table_dims();
        assert!(lines > 0, "{label}: text view has no lines");
    }
}

// --- Hint bar ---

#[test]
fn hint_bar_shows_keys_along_the_bottom() {
    let mut app = app("name,age\nAlice,30\n");
    let out = render(&mut app, 76, 8);
    let last = out.lines().last().unwrap();
    assert!(last.contains("help"), "hint bar on the last line: {last}");
    assert!(last.contains("edit") && last.contains("quit"), "{last}");
}

/// Nano's bar is fixed; ours reflects what is possible right now, so it
/// cannot advertise an action the current view does not support.
#[test]
fn hints_follow_the_view() {
    let doc = Document::parse(b"{\"x\":{\"y\":1}}", FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);

    let tree = render(&mut app, 76, 8);
    let tree_last = tree.lines().last().unwrap().to_string();
    assert!(
        tree_last.contains("open"),
        "tree offers drill-down: {tree_last}"
    );
    assert!(
        !tree_last.contains("edit"),
        "tree is not a cell editor: {tree_last}"
    );

    app.handle_key(key(KeyCode::Char('3')));
    let text = render(&mut app, 76, 8);
    let text_last = text.lines().last().unwrap().to_string();
    assert!(
        text_last.contains("page"),
        "pager offers paging: {text_last}"
    );
    assert!(
        !text_last.contains("open"),
        "pager has nothing to open: {text_last}"
    );
}

/// The bar must not offer a view the document does not have.
#[test]
fn hints_only_offer_available_views() {
    let doc = Document::parse(b"{\"x\":{\"y\":1}}", FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    let last = render(&mut app, 76, 8).lines().last().unwrap().to_string();
    assert!(last.contains("text"), "{last}");
    assert!(
        !last.contains("table"),
        "nested JSON has no table view: {last}"
    );
}

#[test]
fn hint_bar_hides_while_editing_and_toggles_with_shift_h() {
    let mut app = app("a\n1\n");
    app.grid.move_to(1, 0, 2, 1);

    app.handle_key(key(KeyCode::Char('i')));
    let editing = render(&mut app, 76, 8);
    assert!(
        !editing.lines().last().unwrap().contains("help"),
        "the bar gives way to the edit prompt"
    );
    app.handle_key(key(KeyCode::Esc));

    assert!(app.show_hints);
    app.handle_key(key(KeyCode::Char('H')));
    assert!(!app.show_hints);
    let hidden = render(&mut app, 76, 8);
    assert!(!hidden.lines().last().unwrap().contains("help"));
}

/// Every key the bar advertises must resolve to the command it names.
#[test]
fn hint_bar_never_advertises_a_binding_that_does_not_exist() {
    use vuwr_tui::keymap::{Resolved, keys_for, resolve};
    let doc = Document::parse(br#"[{"a":1}]"#, FormatHint::Auto).unwrap();
    let app = App::new(PathBuf::from("t.json"), doc);

    for cmd in app.hints() {
        let keys = keys_for(cmd);
        assert!(!keys.trim().is_empty(), "{} has no key", cmd.name());
        let first = keys.split_whitespace().next().unwrap();
        // Single-character bindings must round-trip through the keymap.
        if first.chars().count() == 1 && !first.starts_with(':') {
            let c = first.chars().next().unwrap();
            let got = resolve(key(KeyCode::Char(c)), false);
            assert!(
                matches!(got, Resolved::Run(r) if r == cmd),
                "hint says {first} runs {}, but the keymap disagrees",
                cmd.name()
            );
        }
    }
}
