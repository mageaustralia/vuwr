//! Rendering snapshots and key-handling tests for the table UI.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vuwr_core::{Document, FormatHint};
use vuwr_tui::App;

fn app(input: &str) -> App {
    let doc = Document::parse(input.as_bytes(), FormatHint::Csv).unwrap();
    App::new(PathBuf::from("test.csv"), doc)
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
