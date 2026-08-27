//! Documents handed to the page from outside the canvas.
//!
//! This is how the web build receives a file that was not dropped on it —
//! a userscript reads it and pushes the bytes in. The path had no test at
//! all: it was written, deployed, and exercised only by clicking a link.
//!
//! The delivery itself is wasm-only, but everything it leads to is not:
//! what arrives is loaded the way a dropped file is, and that is what can
//! go wrong. Checked here through the same door.

use eframe::egui;
use vuwr_core::{Document, FormatHint, ViewMode};
use vuwr_gui::VuwrApp;

fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

/// A file arriving after the app is running replaces what was open.
#[test]
fn a_delivered_document_replaces_the_one_on_screen() {
    let _ctx = ctx();
    let first = Document::parse(b"a,b\n1,2\n", FormatHint::Csv).unwrap();
    let mut app = VuwrApp::new(None, first);
    assert!(app.session().doc.is_csv());

    app.load(
        Some("feed.xml".into()),
        b"<r>\n<item><sku>A1</sku></item>\n</r>\n",
    )
    .expect("the delivered document should load");

    assert!(app.session().doc.is_xml(), "the new document did not take");
    assert_eq!(
        String::from_utf8(app.session().doc.serialize()).unwrap(),
        "<r>\n<item><sku>A1</sku></item>\n</r>\n",
        "it did not arrive byte for byte"
    );
    // And it opens the way a document opens: on its first record.
    assert_eq!(app.session().view_mode(), ViewMode::Tree);
    assert!(app.session().show_detail, "the panel did not open with it");
}

/// The format comes from the name, since bytes alone are ambiguous.
#[test]
fn the_name_decides_how_the_bytes_are_read() {
    let _ctx = ctx();
    let mut app = VuwrApp::new(None, Document::parse(b"a\n", FormatHint::Csv).unwrap());
    type Check = fn(&Document) -> bool;
    let cases: [(&str, Check); 3] = [
        ("f.json", |d| d.is_json()),
        ("f.xml", |d| d.is_xml()),
        ("f.csv", |d| d.is_csv()),
    ];
    for (name, is_right) in cases {
        let bytes: &[u8] = match name {
            "f.json" => b"{\"a\": 1}",
            "f.xml" => b"<r><a>1</a></r>",
            _ => b"a,b\n1,2\n",
        };
        app.load(Some(name.into()), bytes).expect(name);
        assert!(
            is_right(&app.session().doc),
            "{name} was read as something else"
        );
    }
}

/// Something that does not parse is reported, and leaves what was open.
///
/// The delivery path had no error handling worth the name until this
/// asked for it: a userscript can hand over a 404 page as easily as a
/// feed.
#[test]
fn rubbish_is_refused_and_the_open_document_survives() {
    let _ctx = ctx();
    let good = b"a,b\n1,2\n";
    let mut app = VuwrApp::new(None, Document::parse(good, FormatHint::Csv).unwrap());

    let result = app.load(Some("feed.xml".into()), b"<html><body>404 Not Found");
    assert!(result.is_err(), "a truncated document was accepted");
    assert_eq!(
        String::from_utf8(app.session().doc.serialize()).unwrap(),
        String::from_utf8(good.to_vec()).unwrap(),
        "the open document was lost to a bad delivery"
    );
}
