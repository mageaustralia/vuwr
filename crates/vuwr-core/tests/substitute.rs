//! Find and replace.

use vuwr_core::{Command, Document, FormatHint, Session};

fn csv(rows: &str) -> Session {
    Session::new(Document::parse(rows.as_bytes(), FormatHint::Csv).unwrap())
}

fn text(s: &Session) -> String {
    String::from_utf8(s.doc.serialize()).unwrap()
}

/// Set up a replacement: pattern, then what to put there.
fn substitute(s: &mut Session, pattern: &str, with: &str) {
    s.execute(Command::Substitute);
    s.select_all();
    s.input_text(pattern);
    s.input_submit();
    s.input_text(with);
    s.input_submit();
}

const STOCK: &str = "sku,size,city\nA1,120mm,Sydney\nA2,130mm,Perth\nA3,125mm,Sydney\n";

#[test]
fn replace_all_changes_every_match_and_undoes_as_one() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    substitute(&mut s, "Sydney", "Melbourne");
    s.execute(Command::SubstituteAll);

    assert!(text(&s).contains("Melbourne"));
    assert!(!text(&s).contains("Sydney"));

    // One press, not one per row: a bulk edit that takes four hundred
    // undos to reverse is not reversible in any useful sense.
    assert!(s.doc.undo());
    assert_eq!(text(&s), STOCK);
}

/// `$1` stands for what the pattern captured.
#[test]
fn a_captured_group_can_be_used_in_the_replacement() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    substitute(&mut s, r"(\d+)mm", "$1 mm");
    s.execute(Command::SubstituteAll);

    let out = text(&s);
    assert!(out.contains("120 mm"), "{out}");
    assert!(out.contains("130 mm"), "{out}");
}

/// A filter narrows what is replaced, and the status says so.
#[test]
fn a_filter_narrows_the_replacement_and_says_it_does() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    s.execute(Command::Filter);
    s.input_text("Sydney");
    s.input_submit();

    substitute(&mut s, "mm", "millimetres");
    assert!(
        s.status.contains("the filter shows"),
        "the scope was not stated: {}",
        s.status
    );

    s.execute(Command::SubstituteAll);
    let out = text(&s);
    assert!(out.contains("120millimetres"), "{out}");
    assert!(out.contains("125millimetres"), "{out}");
    // Perth is filtered out, so it is not touched.
    assert!(
        out.contains("130mm,Perth"),
        "the hidden row was changed: {out}"
    );
    assert!(s.status.contains("the filter shows"), "{}", s.status);
}

/// Stepping: replace this one, skip that one.
#[test]
fn matches_can_be_stepped_through_and_skipped() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    substitute(&mut s, "Sydney", "Hobart");

    // The first match is under the cursor already.
    s.execute(Command::SubstituteOne);
    let out = text(&s);
    assert!(out.contains("A1,120mm,Hobart"), "{out}");
    assert!(
        out.contains("A3,125mm,Sydney"),
        "the second one was taken too: {out}"
    );

    // Skip it, and it stays as it was.
    s.execute(Command::FindNext);
    let out = text(&s);
    assert!(out.contains("A3,125mm,Sydney"), "{out}");
}

/// An empty replacement deletes what matched, which is a real request.
#[test]
fn an_empty_replacement_removes_the_match() {
    let mut s = csv(STOCK);
    s.execute(Command::ViewTable);
    substitute(&mut s, "mm", "");
    s.execute(Command::SubstituteAll);
    assert!(text(&s).contains("A1,120,Sydney"), "{}", text(&s));
}
