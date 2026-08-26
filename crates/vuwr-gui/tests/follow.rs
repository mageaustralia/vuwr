use eframe::egui;
use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};

fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

fn texts(session: &mut Session) -> Vec<String> {
    let ctx = ctx();
    let mut out = Vec::new();
    for frame in 0..30 {
        out.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 500.0),
            )),
            // Time has to move, or egui's smooth scrolling never arrives.
            time: Some(frame as f64 * 0.05),
            predicted_dt: 0.05,
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                vuwr_gui::render_view(session, ui);
            });
        });
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

/// Every view scrolls to the match `n` found.
///
/// The cursor moved and the screen did not, in the tree and in the text:
/// the match was found, selected, and left off-screen. Only the table
/// followed, and only because it had been fixed for a different reason.
#[test]
fn every_view_scrolls_to_the_match() {
    let mut xml = String::from("<rss>\n<channel>\n");
    for i in 0..300 {
        xml.push_str(&format!(
            "<item>\n<sku>SKU{i:04}</sku>\n<name>Item {i}</name>\n</item>\n"
        ));
    }
    xml.push_str("</channel>\n</rss>\n");

    let mut missing = Vec::new();
    for view in [ViewMode::Table, ViewMode::Tree, ViewMode::Text] {
        let doc = Document::parse(xml.as_bytes(), FormatHint::Xml).unwrap();
        let mut s = Session::new(doc);
        s.execute(match view {
            ViewMode::Table => Command::ViewTable,
            ViewMode::Tree => Command::ViewTree,
            ViewMode::Text => Command::ViewText,
        });
        s.execute(Command::Find);
        s.input_text("SKU0250");
        s.input_submit();
        let painted = texts(&mut s);
        if !painted.iter().any(|t| t.contains("SKU0250")) {
            let near: Vec<&String> = painted
                .iter()
                .filter(|t| t.contains("SKU"))
                .take(3)
                .collect();
            missing.push(format!(
                "{view:?}: cursor is on row {} but the screen shows {near:?}",
                s.grid.cursor.0
            ));
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}
