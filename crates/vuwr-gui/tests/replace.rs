//! Find and replace, through the window.
//!
//! The core has its own tests for what replacing does; these are about
//! whether the window can get there — the keys, the toolbar, the two
//! prompts and the warning that a filter is narrowing the job. Claiming a
//! warning is drawn is not the same as its being drawn.

use eframe::egui;
use vuwr_core::{Command, Document, FormatHint};
use vuwr_gui::VuwrApp;

const STOCK: &str = "sku,size,city\nA1,120mm,Sydney\nA2,130mm,Perth\nA3,125mm,Sydney\n";

fn doc(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Auto).unwrap()
}

fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

/// Every string the whole window paints in one frame.
fn painted(app: &mut VuwrApp, ctx: &egui::Context) -> Vec<String> {
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = Vec::new();
    for n in 0..2 {
        out.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 700.0),
            )),
            time: Some(n as f64 * 0.05),
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut frame));
        for c in &full.shapes {
            collect(&c.shape, &mut out);
        }
    }
    out
}

fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

fn typed(app: &mut VuwrApp, text: &str) {
    for c in text.chars() {
        app.session_mut().input_char(c);
    }
}

fn source(app: &VuwrApp) -> String {
    String::from_utf8(app.session().doc.serialize()).unwrap()
}

/// The whole flow, from the key that starts it to the undo that reverses
/// it.
#[test]
fn replacing_works_from_the_keyboard() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(STOCK));
    app.run(Command::ViewTable, &ctx);

    app.run(Command::Substitute, &ctx);
    let shown = painted(&mut app, &ctx);
    assert!(
        shown.iter().any(|t| t.contains("find what")),
        "the first prompt is not labelled: {shown:?}"
    );

    app.session_mut().select_all();
    typed(&mut app, "Sydney");
    app.session_mut().input_submit();
    let shown = painted(&mut app, &ctx);
    assert!(
        shown.iter().any(|t| t.contains("Replace — with")),
        "the second prompt is not labelled: {shown:?}"
    );

    typed(&mut app, "Hobart");
    app.session_mut().input_submit();

    // This one, then leave the next alone.
    app.run(Command::SubstituteOne, &ctx);
    assert!(source(&app).contains("A1,120mm,Hobart"), "{}", source(&app));
    assert!(
        source(&app).contains("A3,125mm,Sydney"),
        "the skipped row changed: {}",
        source(&app)
    );

    // The rest, and one undo for all of it.
    app.run(Command::SubstituteAll, &ctx);
    assert!(!source(&app).contains("Sydney"), "{}", source(&app));
    app.run(Command::Undo, &ctx);
    assert!(
        source(&app).contains("A3,125mm,Sydney"),
        "one undo did not undo the batch: {}",
        source(&app)
    );
}

/// The buttons appear once there is something to step through, and not
/// before — an "All" that replaces nothing is a trap.
#[test]
fn the_stepping_buttons_appear_only_once_a_replacement_is_set_up() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(STOCK));
    app.run(Command::ViewTable, &ctx);

    let before = painted(&mut app, &ctx);
    assert!(
        before.iter().any(|t| t == "Replace"),
        "no way in: {before:?}"
    );
    assert!(
        !before.iter().any(|t| t == "This one"),
        "stepping offered with nothing to step through: {before:?}"
    );

    app.run(Command::Substitute, &ctx);
    app.session_mut().select_all();
    typed(&mut app, "mm");
    app.session_mut().input_submit();
    typed(&mut app, " mm");
    app.session_mut().input_submit();

    let after = painted(&mut app, &ctx);
    for label in ["This one", "Skip", "All"] {
        assert!(
            after.iter().any(|t| t == label),
            "{label:?} is missing: {after:?}"
        );
    }
}

/// A filter narrows what is replaced, and the window says so while you
/// are still typing — not afterwards.
#[test]
fn the_filter_warning_is_drawn_beside_both_prompts() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(STOCK));
    app.run(Command::ViewTable, &ctx);

    app.run(Command::Filter, &ctx);
    typed(&mut app, "Sydney");
    app.session_mut().input_submit();

    app.run(Command::Substitute, &ctx);
    let shown = painted(&mut app, &ctx);
    assert!(
        shown.iter().any(|t| t.contains("the filter shows")),
        "no warning while choosing what to find: {shown:?}"
    );

    app.session_mut().select_all();
    typed(&mut app, "mm");
    app.session_mut().input_submit();
    let shown = painted(&mut app, &ctx);
    assert!(
        shown.iter().any(|t| t.contains("the filter shows")),
        "no warning while choosing the replacement: {shown:?}"
    );

    typed(&mut app, "millimetres");
    app.session_mut().input_submit();
    app.run(Command::SubstituteAll, &ctx);

    // Perth is filtered out, so it keeps its value.
    assert!(
        source(&app).contains("130mm,Perth"),
        "a hidden row was changed: {}",
        source(&app)
    );
}
