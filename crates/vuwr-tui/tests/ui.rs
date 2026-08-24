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
    // Wide enough for the whole bar; it truncates gracefully when narrower.
    let out = render(&mut app, 120, 8);
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

/// `:wq!` is muscle memory from vim. It writes and quits like `:wq` — the
/// `!` does not mean "quit even if the write failed", which would discard
/// the edits the command was asked to save.
#[test]
fn wq_bang_writes_and_quits() {
    let dir = std::env::temp_dir().join("vuwr-wq-bang");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.csv");
    std::fs::write(&path, "a\n1\n").unwrap();

    let doc = Document::parse(b"a\n1\n", FormatHint::Csv).unwrap();
    let mut app = App::new(path.clone(), doc);
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('9')));
    app.handle_key(key(KeyCode::Enter));

    for c in ":wq!".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.quit, "should quit: {}", app.status);
    assert!(!app.dirty);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\n9\n");
    std::fs::remove_dir_all(&dir).ok();
}

/// A write that cannot succeed must not quit, `!` or no `!` — otherwise
/// the bang silently throws away the edits.
#[test]
fn wq_bang_does_not_quit_when_the_write_fails() {
    let doc = Document::parse(b"a\n1\n", FormatHint::Csv).unwrap();
    // A directory that does not exist: the write fails, nothing is lost.
    let mut app = App::new(PathBuf::from("/nonexistent-dir-vuwr/t.csv"), doc);
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('9')));
    app.handle_key(key(KeyCode::Enter));

    for c in ":wq!".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(!app.quit, "a failed write must keep the editor open");
    assert!(app.dirty, "the edit is still unsaved");
    assert!(app.status.contains("save failed"), "{}", app.status);
}

// --- Editing in text (pager) view ---

#[test]
fn text_view_edits_the_source_line() {
    let mut app = app("name,age\nAlice,30\nBob,25\n");
    app.handle_key(key(KeyCode::Char('3'))); // text view
    app.grid.move_to(1, 0, 3, 1);

    app.handle_key(key(KeyCode::Char('c')));
    for c in "Alicia,31".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(app.dirty, "{}", app.status);
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "name,age\nAlicia,31\nBob,25\n"
    );
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("Alicia,31"));
}

/// A source edit that makes the document unparseable is refused, and the
/// document is left exactly as it was.
#[test]
fn text_view_refuses_an_edit_that_breaks_the_document() {
    let doc = Document::parse(b"{\n  \"a\": 1\n}", FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 3, 1);

    app.handle_key(key(KeyCode::Char('c')));
    for c in "  \"a\": ,,,".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert!(!app.dirty, "a rejected edit must not mark the file dirty");
    assert!(app.status.contains("not applied"), "{}", app.status);
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "{\n  \"a\": 1\n}"
    );
}

/// Splicing must not rewrite line endings: a CRLF file stays CRLF, and a
/// file with no final newline does not gain one.
#[test]
fn text_edits_preserve_line_endings() {
    let doc = Document::parse(b"a,b\r\n1,2\r\n", FormatHint::Csv).unwrap();
    let mut app = App::new(PathBuf::from("t.csv"), doc);
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    for c in "9,9".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "a,b\r\n9,9\r\n"
    );

    let doc = Document::parse(b"a,b\n1,2", FormatHint::Csv).unwrap();
    let mut app = App::new(PathBuf::from("t.csv"), doc);
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    for c in "8,8".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), "a,b\n8,8");
}

#[test]
fn text_edits_undo() {
    let mut app = app("a\n1\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('7')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), "a\n7\n");

    app.handle_key(key(KeyCode::Char('u')));
    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), "a\n1\n");
}

/// An edit made in text view is visible in table view, and vice versa —
/// they are two views of one document, not two documents.
#[test]
fn text_and_table_views_stay_in_step() {
    let mut app = app("a,b\n1,2\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('c')));
    for c in "5,6".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    app.handle_key(key(KeyCode::Char('1'))); // back to table
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("5"));
    assert_eq!(app.table_cell(1, 1).as_deref(), Some("6"));
}

// --- Search, filter, marks, frozen columns (phase 4.7) ---

fn typed(app: &mut App, s: &str) {
    for c in s.chars() {
        let code = if c == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(c)
        };
        app.handle_key(key(code));
    }
}

const SAMPLE: &str = "name,city,age\nAlice,Sydney,30\nBob,Perth,25\nCarol,Sydney,41\n";

#[test]
fn filter_shows_only_matching_rows_and_keeps_the_header() {
    let mut app = app(SAMPLE);
    typed(&mut app, "&Sydney\n");

    let (_, rows, _) = app.table_dims();
    assert_eq!(rows, 3, "header plus two matches");
    assert_eq!(app.table_cell(0, 0).as_deref(), Some("name"));
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("Alice"));
    assert_eq!(app.table_cell(2, 0).as_deref(), Some("Carol"));
}

/// The cursor addresses display rows and the sheet addresses source rows;
/// an edit under a filter must land on the row you can see.
#[test]
fn editing_under_a_filter_writes_to_the_right_row() {
    let mut app = app(SAMPLE);
    typed(&mut app, "&Sydney\n");
    app.grid.move_to(2, 0, 3, 3); // display row 2 == Carol == source row 3

    app.handle_key(key(KeyCode::Char('c')));
    typed(&mut app, "Caroline\n");

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "name,city,age\nAlice,Sydney,30\nBob,Perth,25\nCaroline,Sydney,41\n"
    );
}

#[test]
fn clearing_a_filter_keeps_you_on_the_same_row() {
    let mut app = app(SAMPLE);
    typed(&mut app, "&Sydney\n");
    app.grid.move_to(2, 0, 3, 3); // Carol
    app.handle_key(key(KeyCode::Char('r')));

    let (_, rows, _) = app.table_dims();
    assert_eq!(rows, 4, "all rows back");
    assert_eq!(
        app.table_cell(app.grid.cursor.0, 0).as_deref(),
        Some("Carol")
    );
}

#[test]
fn filter_with_no_matches_changes_nothing() {
    let mut app = app(SAMPLE);
    typed(&mut app, "&zzzz\n");
    let (_, rows, _) = app.table_dims();
    assert_eq!(rows, 4, "the view is left alone");
    assert!(app.status.contains("no rows match"), "{}", app.status);
}

#[test]
fn a_bad_pattern_reports_instead_of_panicking() {
    let mut app = app(SAMPLE);
    typed(&mut app, "&[unclosed\n");
    assert!(app.status.contains("bad pattern"), "{}", app.status);
}

#[test]
fn find_jumps_to_matches_and_n_cycles() {
    let mut app = app(SAMPLE);
    typed(&mut app, "/sydney\n"); // smart case: lower case matches Sydney
    assert_eq!(app.grid.cursor, (1, 1), "first match");

    app.handle_key(key(KeyCode::Char('n')));
    assert_eq!(app.grid.cursor, (3, 1), "next match");

    app.handle_key(key(KeyCode::Char('n')));
    assert_eq!(app.grid.cursor, (1, 1), "wraps around");

    app.handle_key(key(KeyCode::Char('N')));
    assert_eq!(app.grid.cursor, (3, 1), "backwards");
}

#[test]
fn find_reports_when_nothing_matches() {
    let mut app = app(SAMPLE);
    typed(&mut app, "/zzzz\n");
    assert!(app.status.contains("no match"), "{}", app.status);
}

#[test]
fn n_without_a_search_says_so() {
    let mut app = app(SAMPLE);
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.status.contains("no search yet"), "{}", app.status);
}

#[test]
fn marks_toggle_and_survive_filtering() {
    let mut app = app(SAMPLE);
    app.grid.move_to(2, 0, 4, 3); // Bob
    app.handle_key(key(KeyCode::Char('m')));
    assert!(app.grid.marks.contains(&2));

    typed(&mut app, "&Sydney\n"); // Bob is filtered out
    assert!(app.grid.marks.contains(&2), "the mark is kept");
    app.handle_key(key(KeyCode::Char('r')));

    app.grid.move_to(2, 0, 4, 3);
    app.handle_key(key(KeyCode::Char('m')));
    assert!(!app.grid.marks.contains(&2), "toggles off");
}

/// CSV's first row is column names, and it is emitted with the marks
/// anyway, so marking it would only duplicate it.
#[test]
fn the_csv_header_cannot_be_marked() {
    let mut app = app(SAMPLE);
    app.handle_key(key(KeyCode::Char('m')));
    assert!(app.grid.marks.is_empty());
    assert!(
        app.status.contains("header row cannot be marked"),
        "{}",
        app.status
    );
}

#[test]
fn ctrl_e_prints_marked_rows_with_the_header_and_exits() {
    let mut app = app(SAMPLE);
    app.grid.move_to(1, 0, 4, 3);
    app.handle_key(key(KeyCode::Char('m')));
    app.grid.move_to(3, 0, 4, 3);
    app.handle_key(key(KeyCode::Char('m')));

    app.handle_key(ctrl('e'));
    assert!(app.quit, "Ctrl-E exits");
    assert_eq!(
        app.take_output().unwrap(),
        "name,city,age\nAlice,Sydney,30\nCarol,Sydney,41\n"
    );
}

#[test]
fn ctrl_e_with_no_marks_reports_rather_than_quitting() {
    let mut app = app(SAMPLE);
    app.handle_key(ctrl('e'));
    assert!(!app.quit);
    assert!(app.status.contains("no rows marked"), "{}", app.status);
}

#[test]
fn freezing_pins_columns_left_of_the_cursor() {
    let mut app = app(SAMPLE);
    app.grid.move_to(0, 1, 4, 3);
    app.handle_key(key(KeyCode::Char('f')));
    assert_eq!(app.grid.frozen_cols, 1);
    assert!(app.status.contains("frozen"), "{}", app.status);

    app.handle_key(key(KeyCode::Char('f')));
    assert_eq!(app.grid.frozen_cols, 0, "same key unfreezes");
}

/// A frozen column stays on screen when the cursor scrolls far right.
#[test]
fn frozen_columns_stay_visible_when_scrolled() {
    let mut wide = String::from("id,");
    wide.push_str(
        &(0..20)
            .map(|i| format!("col{i}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    wide.push_str("\nKEY,");
    wide.push_str(
        &(0..20)
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    wide.push('\n');

    let mut app = app(&wide);
    app.grid.move_to(0, 0, 2, 21);
    app.handle_key(key(KeyCode::Char('f'))); // freeze nothing yet (col 0)
    app.grid.move_to(0, 1, 2, 21);
    app.handle_key(key(KeyCode::Char('f'))); // freeze the id column
    app.grid.move_to(1, 20, 2, 21);

    let out = render(&mut app, 40, 6);
    assert!(
        out.contains("KEY"),
        "the frozen column is still shown:\n{out}"
    );
    assert!(
        out.contains("v19"),
        "and so is the far-right column:\n{out}"
    );
}
