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

#[test]
fn json_table_not_available_for_non_array() {
    let mut a = json_app("{\"a\":1}");
    a.handle_key(key(KeyCode::Tab));
    assert_eq!(a.view_mode(), ViewMode::Tree); // stays tree
    assert!(a.status.contains("not available"));
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
