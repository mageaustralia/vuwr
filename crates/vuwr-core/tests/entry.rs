//! Typing and selecting, in every field.
//!
//! A prompt, the command line and a cell edit are one kind of text field,
//! so each of these runs in all three. That is the point of their being
//! one thing — and the only way to find out whether they really are.

use vuwr_core::{Command, Document, FormatHint, Session};

fn session() -> Session {
    let mut s =
        Session::new(Document::parse(b"sku,city\nA1,Sydney\nA2,Perth\n", FormatHint::Csv).unwrap());
    s.execute(Command::ViewTable);
    s.grid.cursor = (1, 1);
    s
}

/// The three ways into a text field.
fn fields() -> Vec<(&'static str, Command)> {
    vec![
        ("find prompt", Command::Find),
        ("filter prompt", Command::Filter),
        ("command line", Command::OpenPalette),
        ("cell edit", Command::ReplaceCell),
    ]
}

fn open(which: Command) -> Session {
    let mut s = session();
    s.execute(which);
    s
}

fn buf(s: &Session) -> String {
    s.entry()
        .map(|(_, text)| text.to_string())
        .unwrap_or_default()
}

#[test]
fn typing_and_backspace_work_in_every_field() {
    let mut wrong = Vec::new();
    for (name, cmd) in fields() {
        let mut s = open(cmd);
        s.select_all();
        s.input_text("hello");
        if buf(&s) != "hello" {
            wrong.push(format!("{name}: typed {:?}", buf(&s)));
            continue;
        }
        s.input_backspace();
        if buf(&s) != "hell" {
            wrong.push(format!("{name}: backspace gave {:?}", buf(&s)));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// The caret goes where it is sent, and typing lands there.
#[test]
fn the_caret_moves_and_typing_follows_it() {
    let mut wrong = Vec::new();
    for (name, cmd) in fields() {
        let mut s = open(cmd);
        s.select_all();
        s.input_text("abcd");

        s.input_home();
        if s.entry_caret() != 0 {
            wrong.push(format!(
                "{name}: Home left the caret at {}",
                s.entry_caret()
            ));
            continue;
        }
        s.input_char('X');
        if buf(&s) != "Xabcd" {
            wrong.push(format!("{name}: typed at the wrong end: {:?}", buf(&s)));
            continue;
        }

        s.input_end();
        s.input_char('Z');
        if buf(&s) != "XabcdZ" {
            wrong.push(format!("{name}: End then type gave {:?}", buf(&s)));
            continue;
        }

        // One step left, then type between the last two.
        s.input_left();
        s.input_char('Y');
        if buf(&s) != "XabcdYZ" {
            wrong.push(format!("{name}: left-then-type gave {:?}", buf(&s)));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// A selection is replaced by what is typed, and removed by Delete.
#[test]
fn a_selection_is_what_the_next_keystroke_replaces() {
    let mut wrong = Vec::new();
    for (name, cmd) in fields() {
        // Typing over it.
        let mut s = open(cmd);
        s.select_all();
        s.input_text("DLTA90431");
        s.select_all();
        s.input_text("WRZ990200");
        if buf(&s) != "WRZ990200" {
            wrong.push(format!(
                "{name}: typing over a selection gave {:?}",
                buf(&s)
            ));
        }

        // Deleting it — from the front, where the caret is not.
        let mut s = open(cmd);
        s.select_all();
        s.input_text("DLTA90431");
        s.set_entry_caret(0);
        for _ in 0..4 {
            s.input_select_right();
        }
        if s.selected_text().as_deref() != Some("DLTA") {
            wrong.push(format!("{name}: selected {:?}", s.selected_text()));
            continue;
        }
        s.input_delete();
        if buf(&s) != "90431" {
            wrong.push(format!("{name}: delete gave {:?}", buf(&s)));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// Escape abandons what was typed; Enter keeps it.
#[test]
fn escape_abandons_and_enter_commits() {
    let before = {
        let s = session();
        String::from_utf8(s.doc.serialize()).unwrap()
    };

    let mut s = session();
    s.execute(Command::ReplaceCell);
    s.input_text("Hobart");
    s.input_cancel();
    assert_eq!(
        String::from_utf8(s.doc.serialize()).unwrap(),
        before,
        "Esc kept the edit"
    );
    assert!(!s.is_entering_text(), "Esc left the field open");

    let mut s = session();
    s.execute(Command::ReplaceCell);
    s.input_text("Hobart");
    s.input_submit();
    assert!(
        String::from_utf8(s.doc.serialize())
            .unwrap()
            .contains("Hobart"),
        "Enter dropped the edit"
    );
    assert!(!s.is_entering_text(), "Enter left the field open");
}

/// Editing appends to what is there; replacing starts empty.
///
/// The two commands the README names as "edit / replace the cell".
#[test]
fn edit_keeps_the_value_and_replace_clears_it() {
    let mut s = session();
    s.execute(Command::EditCell);
    assert_eq!(buf(&s), "Sydney", "edit did not start from the value");

    let mut s = session();
    s.execute(Command::ReplaceCell);
    assert_eq!(buf(&s), "", "replace started from the old value");
}

/// While a field is open, letters are letters — not commands.
#[test]
fn a_field_swallows_the_keys_that_would_otherwise_be_commands() {
    let mut s = session();
    s.execute(Command::Find);
    for c in "qu1&".chars() {
        s.input_char(c);
    }
    assert_eq!(buf(&s), "qu1&");
    assert!(s.is_entering_text(), "a keystroke closed the field");
}
