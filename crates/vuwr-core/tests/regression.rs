//! Regressions for defects found reviewing phases 0–4. Each test names the
//! behaviour that was wrong, so a reintroduction fails loudly.

use vuwr_core::{Document, EditOp, Error, FormatHint};

/// Editing a JSON document used to return `Ok(())` while changing nothing,
/// push a bogus inverse onto the undo stack, and let the frontend mark the
/// document dirty — so saving wrote the file back unchanged and the edit
/// silently vanished.
#[test]
fn json_edit_is_refused_not_silently_dropped() {
    let src = br#"[{"name":"Alice","age":30}]"#;
    let mut doc = Document::parse(src, FormatHint::Auto).unwrap();

    let err = doc
        .apply(EditOp::SetCell {
            row: 0,
            column: 0,
            value: "CHANGED".into(),
        })
        .expect_err("editing JSON must be refused");

    assert_eq!(err, Error::EditNotSupported { format: "JSON" });
    // Nothing was recorded, so there is nothing to undo.
    assert!(!doc.undo(), "a refused edit must not reach the undo stack");
    assert_eq!(doc.serialize(), src);
}

#[test]
fn xml_edit_is_refused_not_silently_dropped() {
    let src = b"<r><a>1</a></r>";
    let mut doc = Document::parse(src, FormatHint::Auto).unwrap();
    let err = doc
        .apply(EditOp::SetCell {
            row: 0,
            column: 0,
            value: "x".into(),
        })
        .expect_err("editing XML must be refused");
    assert_eq!(err, Error::EditNotSupported { format: "XML" });
    assert!(!doc.undo());
}

/// Content sniffing used to override an explicit hint, so a CSV whose first
/// header cell began with `{` was handed to the JSON parser and failed.
#[test]
fn explicit_hint_beats_content_sniffing() {
    let doc = Document::parse(b"{name},{age}\na,b\n", FormatHint::Csv).unwrap();
    assert!(doc.is_csv());

    let doc = Document::parse(b"<not>,<xml>\n1,2\n", FormatHint::Csv).unwrap();
    assert!(doc.is_csv());

    // Auto still sniffs.
    assert!(
        Document::parse(b"{\"a\":1}", FormatHint::Auto)
            .unwrap()
            .is_json()
    );
    assert!(Document::parse(b"<r/>", FormatHint::Auto).unwrap().is_xml());
}

/// A UTF-8 BOM is not ASCII whitespace, so it used to defeat detection: a
/// BOM'd JSON file parsed as a single garbage CSV row, silently.
#[test]
fn bom_does_not_defeat_detection_and_survives_roundtrip() {
    let mut src = vec![0xEF, 0xBB, 0xBF];
    src.extend_from_slice(br#"{"a":1}"#);

    let doc = Document::parse(&src, FormatHint::Auto).unwrap();
    assert!(doc.is_json(), "BOM'd JSON must still detect as JSON");
    assert_eq!(doc.serialize(), src, "the BOM must be preserved on save");
}

/// `parse_string` accepted only bytes 0x20..=0x7e, so *any* non-ASCII
/// character made the whole document unparseable; `serialize_string`
/// escaped per byte, which would have split multi-byte characters.
#[test]
fn non_ascii_json_parses_and_roundtrips() {
    for src in [
        r#"{"city":"café"}"#,
        r#"{"jp":"日本"}"#,
        r#"{"emoji":"🎉"}"#,
        r#"["naïve","Ω","👍🏽"]"#,
    ] {
        let doc = Document::parse(src.as_bytes(), FormatHint::Auto)
            .unwrap_or_else(|e| panic!("{src}: {e}"));
        assert_eq!(
            String::from_utf8(doc.serialize()).unwrap(),
            src,
            "round-trip of {src}"
        );
    }
}

/// `\uXXXX` escapes decode, including surrogate pairs for astral characters.
/// They re-serialize as the literal character: semantically identical, not
/// byte-identical, which is the one documented exception to byte fidelity.
#[test]
fn unicode_escapes_decode_including_surrogate_pairs() {
    // Source is pure ASCII: `\u00e9` is é, and the surrogate pair
    // `\ud83c\udf89` is 🎉 (U+1F389), which needs both halves to decode.
    let src = r#"{"a":"caf\u00e9","b":"\ud83c\udf89"}"#;
    let doc = Document::parse(src.as_bytes(), FormatHint::Auto).unwrap();
    let out = String::from_utf8(doc.serialize()).unwrap();
    assert_eq!(out, r#"{"a":"café","b":"🎉"}"#);
}

/// Truncated containers used to loop handing an empty slice to
/// `parse_string`, which indexed `bytes[0]` and panicked.
#[test]
fn truncated_input_errors_rather_than_panicking() {
    for src in [
        r#"{"a":1,"#,
        r#"{"a":"#,
        "[1,2,",
        "[",
        "{",
        r#"{"a""#,
        r#""unterminated"#,
        r#"{"a":"\u00"#,
    ] {
        let err = Document::parse(src.as_bytes(), FormatHint::Auto);
        assert!(err.is_err(), "{src:?} should be a parse error, got Ok");
    }
}

/// Every JSON syntax error used to report "input is not valid UTF-8".
/// Phase 5's lint and `--check` need the real kind and a real offset.
#[test]
fn syntax_errors_are_named_and_located() {
    let Err(err) = Document::parse(b"[1,2,x]", FormatHint::Auto) else {
        panic!("expected a parse error")
    };
    assert!(
        matches!(err, Error::UnexpectedToken { offset: 5 }),
        "expected UnexpectedToken at 5, got {err:?}"
    );

    let Err(err) = Document::parse(br#"{"a":"b\q"}"#, FormatHint::Auto) else {
        panic!("expected a parse error")
    };
    assert!(
        matches!(err, Error::InvalidEscape { .. }),
        "expected InvalidEscape, got {err:?}"
    );

    // Genuinely invalid UTF-8 still reports as such.
    let Err(err) = Document::parse(&[b'{', 0xFF, 0xFE], FormatHint::Auto) else {
        panic!("expected a parse error")
    };
    assert_eq!(err, Error::InvalidUtf8);
}
