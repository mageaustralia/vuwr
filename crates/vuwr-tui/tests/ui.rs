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
fn json_tree_expands_in_place() {
    let mut a = json_app("{\"nested\":{\"x\":1},\"other\":2}");
    assert_eq!(a.tree_rows.len(), 2, "top level only");

    a.handle_key(key(KeyCode::Enter)); // open `nested`
    assert_eq!(a.tree_rows.len(), 3, "its child appears");
    let out = render(&mut a, 40, 10);
    assert!(out.contains("x"), "the inner key shows: {out}");
    assert!(
        out.contains("other"),
        "and the neighbour is still there — the point of expanding in \
         place rather than replacing the view: {out}"
    );

    a.handle_key(key(KeyCode::Esc)); // close it again
    assert_eq!(a.tree_rows.len(), 2);
}

#[test]
fn json_tree_expands_an_array() {
    let mut a = json_app("{\"items\":[1,2,3],\"name\":\"test\"}");
    a.handle_key(key(KeyCode::Enter));
    assert_eq!(a.tree_rows.len(), 5, "two keys plus three items");
    let out = render(&mut a, 40, 10);
    assert!(out.contains("1"), "array elements show: {out}");
}

/// Expand-all opens every level; collapse-all closes back to the top.
#[test]
fn expand_all_and_collapse_all() {
    let mut a = json_app("{\"a\":{\"b\":{\"c\":1}}}");
    a.handle_key(key(KeyCode::Char('*')));
    assert_eq!(a.tree_rows.len(), 3, "every level open");
    a.handle_key(key(KeyCode::Char('_')));
    assert_eq!(a.tree_rows.len(), 1, "back to the top level");
}

/// A key used twice is legal JSON and nearly always a bug, so it is
/// marked rather than left to be spotted.
#[test]
fn duplicate_keys_are_marked_in_the_tree() {
    let mut a = json_app("{\"color\":true,\"color\":\"gold\"}");
    assert!(a.tree_rows[0].duplicate);
    assert!(a.tree_rows[1].duplicate);
    let out = render(&mut a, 40, 10);
    assert!(out.contains('!'), "flagged in the render: {out}");
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
    assert!(out.contains("item"), "should show element tag: {out}");
    assert_eq!(a.tree_rows.len(), 2, "both items are rows");
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
/// Collapsing at the top level has nothing to close, and must not empty
/// the view.
fn xml_collapse_at_top_level_is_harmless() {
    let mut a = xml_app("<root><child/></root>");
    a.handle_key(key(KeyCode::Esc));
    assert_eq!(a.view_mode(), ViewMode::Tree);
    let out = render(&mut a, 40, 10);
    assert!(out.contains("child"), "should still show child: {out}");
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
    // The message must say where, not just that something is wrong.
    assert!(
        app.status.contains("2:"),
        "reports the line the edit broke: {}",
        app.status
    );
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

/// Every command must be reachable somehow, and help must describe the way
/// it actually is: a key that resolves, or a `:` name the palette accepts.
/// The GUI shipped with five commands whose advertised bindings did not
/// exist; this is the TUI's guard against the same thing.
#[test]
fn every_command_is_reachable_by_the_route_help_advertises() {
    use vuwr_core::Command;
    use vuwr_tui::keymap::{Resolved, keys_for, resolve};

    let mut codes: Vec<KeyCode> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("/&:?*_<>=".chars())
        .map(KeyCode::Char)
        .collect();
    codes.extend([
        KeyCode::F(2),
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
    ]);
    let mods = [
        KeyModifiers::NONE,
        KeyModifiers::SHIFT,
        KeyModifiers::CONTROL,
    ];

    let mut by_key = std::collections::HashSet::new();
    for code in &codes {
        for m in mods {
            for pending in [false, true] {
                if let Resolved::Run(c) = resolve(KeyEvent::new(*code, m), pending) {
                    by_key.insert(c);
                }
            }
        }
    }

    for cmd in Command::ALL {
        if by_key.contains(cmd) {
            continue;
        }
        // Not on a key, so help must point at a `:` command that resolves.
        let advertised = keys_for(*cmd);
        let name = advertised
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_start_matches(':');
        assert_eq!(
            Command::from_name(name),
            Some(*cmd),
            "{} is not on a key, and help's {advertised:?} does not resolve to it",
            cmd.name()
        );
    }
}

// --- Inline editing with a caret ---

/// Editing used to be a field at the bottom you could only append to.
/// The text is now typed where it lives, and the caret moves.
#[test]
fn text_edits_render_on_the_line_being_edited() {
    let mut app = app("name,age\nAlice,30\n");
    app.handle_key(key(KeyCode::Char('3'))); // text view
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('i')));

    let out = render(&mut app, 60, 8);
    let line = out.lines().nth(1).unwrap();
    assert!(
        line.contains("Alice,30"),
        "the buffer is shown on its own line: {line}"
    );
    assert!(
        !out.lines().last().unwrap().contains("Alice,30"),
        "and not duplicated in the status line"
    );
}

#[test]
fn the_caret_moves_and_inserts_where_it_is() {
    let mut app = app("a\nabc\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('i'))); // caret at the end

    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Char('X'))); // between a and b
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "a\naXbc\n",
        "inserted at the caret, not appended"
    );
}

#[test]
fn home_end_and_delete_work() {
    let mut app = app("a\nabc\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('i')));

    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Delete)); // removes 'a'
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), "a\nbc!\n");
}

#[test]
fn backspace_removes_before_the_caret_not_the_end() {
    let mut app = app("a\nabc\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('i')));

    app.handle_key(key(KeyCode::Left)); // between b and c
    app.handle_key(key(KeyCode::Backspace)); // removes b
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), "a\nac\n");
}

/// A multi-byte character must not be split by caret movement.
#[test]
fn the_caret_steps_over_whole_characters() {
    let mut app = app("a\ncafé\n");
    app.handle_key(key(KeyCode::Char('3')));
    app.grid.move_to(1, 0, 2, 1);
    app.handle_key(key(KeyCode::Char('i')));

    app.handle_key(key(KeyCode::Left)); // over é in one step
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "a\ncaf!é\n"
    );
}

#[test]
fn table_edits_also_render_in_the_cell() {
    let mut app = app("name,age\nAlice,30\n");
    app.grid.move_to(1, 0, 2, 2);
    app.handle_key(key(KeyCode::Char('c')));
    for c in "Zed".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    let out = render(&mut app, 60, 8);
    assert!(out.contains("Zed"), "typed text shows in the cell:\n{out}");
}

/// The tree edits in place too. It was the one view still echoing the
/// buffer at the bottom while showing the old value in the row.
#[test]
fn tree_edits_render_on_the_row_being_edited() {
    let doc = Document::parse(br#"{"name":"Alice","age":30}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Char('c'))); // replace the value
    for c in "Zed".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }

    let out = render(&mut app, 60, 8);
    let first = out.lines().next().unwrap();
    assert!(
        first.contains("Zed"),
        "typed text shows on the row: {first}"
    );
    assert!(
        !first.contains("Alice"),
        "and replaces the old value rather than sitting beside it: {first}"
    );
    assert!(
        !out.lines().last().unwrap().contains("Zed"),
        "not duplicated in the hint bar"
    );
}

#[test]
fn tree_edits_commit_to_the_right_node() {
    let doc = Document::parse(br#"{"name":"Alice","age":30}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.grid.move_to(1, 0, 2, 1); // the age row
    app.handle_key(key(KeyCode::Char('c')));
    app.handle_key(key(KeyCode::Char('4')));
    app.handle_key(key(KeyCode::Char('1')));
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        r#"{"name":"Alice","age":41}"#,
        "the number stays a number"
    );
}

// --- Tree navigation with arrows, and the larger editor ---

/// Down, right, down, right — the way every file browser works. Right in
/// a tree used to move a column, which a tree does not have.
#[test]
fn right_opens_a_node_and_left_closes_it() {
    let doc = Document::parse(br#"{"a":1,"o":{"x":1,"y":2}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    assert_eq!(app.tree_rows.len(), 2);

    app.handle_key(key(KeyCode::Down)); // the object
    app.handle_key(key(KeyCode::Right)); // open it
    assert_eq!(app.tree_rows.len(), 4, "its children appear");

    app.handle_key(key(KeyCode::Left)); // close it again
    assert_eq!(app.tree_rows.len(), 2);
}

/// Right on an already-open node steps into it rather than doing nothing.
#[test]
fn right_on_an_open_node_steps_into_it() {
    let doc = Document::parse(br#"{"o":{"x":1}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Right)); // open
    app.handle_key(key(KeyCode::Right)); // into
    assert_eq!(app.grid.cursor.0, 1);
    assert_eq!(app.tree_rows[1].label, "x");
}

/// Left on a closed child goes out to its parent.
#[test]
fn left_on_a_child_returns_to_the_parent() {
    let doc = Document::parse(br#"{"o":{"x":1}}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right)); // on `x`
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.grid.cursor.0, 0, "back on the object");
}

#[test]
fn right_on_a_scalar_does_nothing() {
    let doc = Document::parse(br#"{"a":1}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.tree_rows.len(), 1);
    assert_eq!(app.grid.cursor.0, 0);
}

/// The terminal gets the same larger editor as the window, since a long
/// value cannot be edited inline in either.
#[test]
fn f2_opens_the_larger_editor_in_the_terminal() {
    let long = "line one\nline two";
    let src = format!("<r><d>{long}</d></r>");
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);

    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    assert!(app.editing_large());

    let out = render(&mut app, 60, 12);
    assert!(out.contains("line one"), "the value is shown:\n{out}");
    assert!(out.contains("Ctrl-S"), "and how to commit it:\n{out}");
}

#[test]
fn the_larger_editor_takes_newlines_and_commits_with_ctrl_s() {
    let doc = Document::parse(b"<r><d>old</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    for _ in 0.."old".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "one".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter)); // a newline, not a commit
    for c in "two".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    assert!(
        app.editing_large(),
        "Enter inserts a line rather than committing"
    );

    app.handle_key(ctrl('s'));
    assert!(!app.editing_large());
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>one\ntwo</d></r>"
    );
}

#[test]
fn escape_abandons_the_larger_editor() {
    let src = "<r><d>old</d></r>";
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Esc));
    assert!(!app.editing_large());
    assert_eq!(String::from_utf8(app.doc.serialize()).unwrap(), src);
}

/// The terminal decodes and encodes exactly as the window does.
#[test]
fn the_larger_editor_decodes_and_encodes() {
    let doc = Document::parse(b"<r><d>&lt;p&gt;hi&lt;/p&gt;</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    let out = render(&mut app, 60, 12);
    assert!(out.contains("<p>hi</p>"), "decoded for reading:\n{out}");

    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(ctrl('s'));
    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>&lt;p&gt;hi&lt;/p&gt;!</d></r>",
        "and encoded on the way back"
    );
}

/// Editing a long value opens the overlay by itself, in the terminal too.
#[test]
fn a_long_value_opens_the_overlay_without_f2() {
    let long = "x".repeat(120);
    let src = format!("<r><d>{long}</d></r>");
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);

    app.handle_key(key(KeyCode::Char('i')));
    assert!(app.editing_large(), "the overlay opened on its own");
    assert!(!app.is_entering_text(), "and not the inline editor");
}

// --- The detail pane ---

/// A table column is far narrower than a description, so most of a feed
/// is behind a truncation. The pane shows the selected value in full.
#[test]
fn the_detail_pane_shows_the_selected_value_whole() {
    let long = "a very long description that a narrow column cannot possibly show in full";
    let src = format!("<rows><row><d>{long}</d></row></rows>");
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(key(KeyCode::Char('1'))); // table view

    let before = render(&mut app, 40, 14);
    assert!(
        !before.contains("cannot possibly show"),
        "truncated in the column"
    );

    app.handle_key(key(KeyCode::Char('V')));
    let after = render(&mut app, 40, 14);
    assert!(
        after.contains("cannot possibly"),
        "the pane wraps it into view:\n{after}"
    );
}

#[test]
fn the_detail_pane_toggles_off_again() {
    let mut app = app("a,b\nlong value here,2\n");
    app.handle_key(key(KeyCode::Char('V')));
    assert!(app.show_detail);
    app.handle_key(key(KeyCode::Char('V')));
    assert!(!app.show_detail);
}

/// It names what it is showing, so a wide table stays legible.
#[test]
fn the_detail_pane_names_the_column() {
    let mut app = app("name,age\nAlice,30\n");
    app.grid.move_to(1, 1, 2, 2);
    app.handle_key(key(KeyCode::Char('V')));
    let out = render(&mut app, 50, 14);
    assert!(
        out.contains("age"),
        "the column name is on the pane:\n{out}"
    );
}

/// It follows the cursor, in the tree as well.
#[test]
fn the_detail_pane_follows_the_selection() {
    let doc = Document::parse(br#"{"a":"first","b":"second"}"#, FormatHint::Auto).unwrap();
    let mut app = App::new(PathBuf::from("t.json"), doc);
    app.handle_key(key(KeyCode::Char('V')));
    assert_eq!(app.detail_text().as_deref(), Some("first"));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.detail_text().as_deref(), Some("second"));
}

/// A value that grows by an edit must not then be judged against the old
/// column width: it sent a six-character cell to the large editor.
#[test]
fn column_widths_keep_up_with_edits() {
    let mut app = app("name\nAlice\n");
    app.grid.move_to(1, 0, 2, 1);

    app.handle_key(key(KeyCode::Char('i')));
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.table_cell(1, 0).as_deref(), Some("Alice!"));

    // The next edit still happens in place.
    app.handle_key(key(KeyCode::Char('c')));
    assert!(!app.editing_large(), "still inline after the value grew");
    assert!(app.is_entering_text());
}

// --- Moving about inside the larger editor ---

#[test]
fn arrows_move_the_caret_inside_the_editor() {
    let doc = Document::parse(b"<r><d>abc</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Char('X'))); // between a and b
    app.handle_key(ctrl('s'));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>aXbc</d></r>",
        "inserted at the caret, not appended"
    );
}

/// Up and down move between lines, which is the whole point of a value
/// that needed a box.
#[test]
fn up_and_down_move_between_lines() {
    let doc = Document::parse(b"<r><d>one\ntwo\nthree</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    // Caret starts at the end; go up two lines and to the line start.
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Up));
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(ctrl('s'));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>!one\ntwo\nthree</d></r>"
    );
}

/// Home and End work on the current line, not the whole value.
#[test]
fn home_and_end_are_per_line() {
    let doc = Document::parse(b"<r><d>one\ntwo</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    app.handle_key(key(KeyCode::Up)); // onto line one
    app.handle_key(key(KeyCode::End)); // its end, not the value's
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(ctrl('s'));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>one!\ntwo</d></r>"
    );
}

/// Moving up onto a shorter line lands at its end rather than past it.
#[test]
fn moving_onto_a_shorter_line_lands_at_its_end() {
    let doc = Document::parse(b"<r><d>ab\nlonger line</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    app.handle_key(key(KeyCode::Up)); // from the end of the long line
    app.handle_key(key(KeyCode::Char('!')));
    app.handle_key(ctrl('s'));

    assert_eq!(
        String::from_utf8(app.doc.serialize()).unwrap(),
        "<r><d>ab!\nlonger line</d></r>"
    );
}

/// The editor says how to leave it, at both edges.
#[test]
fn the_editor_shows_how_to_leave() {
    let doc = Document::parse(b"<r><d>x</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    let out = render(&mut app, 70, 14);
    assert!(out.contains("Ctrl-S"), "how to save:\n{out}");
    assert!(out.contains("Esc"), "how to leave:\n{out}");
}

/// The tree opens the box on the same values a table would, rather than
/// editing a cut-off value in place.
#[test]
fn the_tree_opens_the_box_for_a_value_that_does_not_fit() {
    let url = "https://www.example.com/media/catalog/product/b/a/babolat_ert_300-5.png";
    let src = format!("<r><g:image_link>{url}</g:image_link></r>");
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);

    // A narrow view: the value cannot fit beside its key.
    let _ = render(&mut app, 50, 10);
    app.handle_key(key(KeyCode::Char('i')));
    assert!(app.editing_large(), "the box opened in the tree too");
}

/// Moving the caret must not change the text. With the caret *on* a
/// newline, that newline was drawn as a reversed span holding "\n", which
/// draws nothing and does not break the line — so the following line
/// silently joined it and the value appeared to rewrite itself as the
/// cursor moved.
#[test]
fn moving_the_caret_never_changes_what_is_drawn() {
    // Escaped, so it is the element's *text* rather than child elements —
    // which is how a feed carries markup, and what the editor opens on.
    let encoded = "&lt;ul&gt;\n&lt;li&gt;one&lt;/li&gt;\n&lt;li&gt;two&lt;/li&gt;\n&lt;/ul&gt;";
    let src = format!("<r><d>{encoded}</d></r>");
    let value = "<ul>\n<li>one</li>\n<li>two</li>\n</ul>";
    let doc = Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));

    /// The drawn text, ignoring where the caret happens to be.
    fn drawn(app: &mut App) -> Vec<String> {
        render(app, 60, 16)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.contains("<li>") || l.contains("<ul>") || l.contains("</ul>"))
            .collect()
    }

    let start = drawn(&mut app);
    assert!(
        start.len() >= 4,
        "four structural lines to begin with: {start:?}"
    );

    // Walk the caret through the whole value; the text must not move.
    for _ in 0..value.chars().count() {
        app.handle_key(key(KeyCode::Left));
        assert_eq!(drawn(&mut app), start, "the text moved as the caret did");
    }
}

/// The caret has to be findable: a reversed cell on a busy screen was not.
#[test]
fn the_editor_is_visibly_a_separate_thing() {
    let doc = Document::parse(b"<r><d>hello there</d></r>", FormatHint::Xml).unwrap();
    let mut app = App::new(PathBuf::from("t.xml"), doc);
    app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    let out = render(&mut app, 60, 12);
    assert!(out.contains("edit value"), "titled:\n{out}");
    assert!(out.contains("Ctrl-S"), "and says how to leave:\n{out}");
    assert!(
        out.contains('━'),
        "with a heavy border to separate it:\n{out}"
    );
}
