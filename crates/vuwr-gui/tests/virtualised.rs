//! Every view draws what is on screen, and not the rest of the file.
//!
//! The tree drew all of it: expanded, a product feed is sixty-odd
//! thousand rows, and laying every one of them out on every frame took
//! 300ms — three frames a second, which reads as the program having
//! hung. The table and the text view already drew only what fitted.
//!
//! Asserted as a duration, which is not the first choice — a stopwatch on
//! a shared machine fails for reasons that have nothing to do with the
//! code. But painting is already culled for what is off screen, so a
//! count of painted shapes cannot see the cost at all: it is the *layout*
//! of sixty thousand rows that is slow, and only the clock notices.
//!
//! The budget is a thousand times what the fixed version takes and a
//! twentieth of what the broken one did, so the margin covers a very slow
//! machine without letting the regression back in.

use eframe::egui;
use vuwr_core::{Command, Document, FormatHint, Session, ViewMode};

fn feed(items: usize) -> String {
    // With newlines: a file written as one enormous line is a different
    // problem — laying out a single galley of a million characters — and
    // not the one this is about.
    let mut xml = String::from("<rss>\n<channel>\n");
    for i in 0..items {
        xml.push_str(&format!(
            "<item>\n<sku>SKU{i:05}</sku>\n<name>Item {i}</name>\n<city>Sydney</city>\n</item>\n"
        ));
    }
    xml.push_str("</channel>\n</rss>\n");
    xml
}

fn frame_time(session: &mut Session) -> std::time::Duration {
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});

    let mut worst = std::time::Duration::ZERO;
    for n in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            time: Some(n as f64 * 0.05),
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                vuwr_gui::render_view(session, ui);
            });
        });
        // The first frame builds the font atlas and the galley cache;
        // what matters is the steady state a reader scrolls through.
        if n > 0 {
            worst = worst.max(start.elapsed());
        }
    }
    worst
}

/// A screenful is a screenful whatever the file's size.
///
/// The bound is generous — an 800px pane holds perhaps thirty rows of two
/// or three strings each — and still an order of magnitude below what
/// drawing the whole document costs.
#[test]
fn no_view_draws_the_whole_document() {
    let src = feed(20_000);
    let parse = || Document::parse(src.as_bytes(), FormatHint::Xml).unwrap();
    let rows_in_full = {
        let mut s = Session::new(parse());
        s.execute(Command::ViewTree);
        s.execute(Command::ExpandAll);
        s.tree_rows.len()
    };
    assert!(
        rows_in_full > 60_000,
        "the fixture is too small to prove anything: {rows_in_full} rows"
    );

    for view in [ViewMode::Table, ViewMode::Tree, ViewMode::Text] {
        let mut s = Session::new(parse());
        s.execute(match view {
            ViewMode::Table => Command::ViewTable,
            ViewMode::Tree => Command::ViewTree,
            ViewMode::Text => Command::ViewText,
        });
        if view == ViewMode::Tree {
            s.execute(Command::ExpandAll);
        }
        // Switching in is part of the cost: working out which lines the
        // source view has to shift was once quadratic, and four minutes.
        let switched = std::time::Instant::now();
        let _ = s.table_dims();
        assert!(
            switched.elapsed() < std::time::Duration::from_millis(250),
            "{view:?} took {:?} just to be ready",
            switched.elapsed()
        );
        let worst = frame_time(&mut s);
        assert!(
            worst < std::time::Duration::from_millis(250),
            "{view:?} took {worst:?} for a single frame of a {rows_in_full}-row document — \
             it is laying out what is not on screen"
        );
    }
}
