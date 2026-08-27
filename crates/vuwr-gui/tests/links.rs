//! Values that are links.
//!
//! A feed is mostly URLs, and following one was the thing you could not
//! do: they were coloured like links and did nothing, in every view and
//! in the panel alike.

use eframe::egui;
use vuwr_core::{Command, Document, FormatHint, ViewMode};
use vuwr_gui::VuwrApp;

const FEED: &str = "<r>\n<item><sku>A1</sku>\
                    <link>https://example.com/products/a1</link></item>\n</r>\n";

fn ctx() -> egui::Context {
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    ctx
}

/// The colour each string was drawn in, so "is it offered as a link" can
/// be asked of the frame rather than of the code.
fn painted(app: &mut VuwrApp, ctx: &egui::Context) -> Vec<(String, egui::Color32)> {
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = Vec::new();
    for _ in 0..2 {
        out.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 700.0),
            )),
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut frame));
        for c in &full.shapes {
            collect(&c.shape, &mut out);
        }
    }
    out
}

fn collect(shape: &egui::Shape, out: &mut Vec<(String, egui::Color32)>) {
    match shape {
        egui::Shape::Text(t) => {
            let colour = t.override_text_color.unwrap_or_else(|| {
                t.galley
                    .rows
                    .iter()
                    .find_map(|r| r.visuals.mesh.vertices.first().map(|v| v.color))
                    .unwrap_or(egui::Color32::PLACEHOLDER)
            });
            out.push((t.galley.text().to_string(), colour));
        }
        egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

fn link_colour_of(painted: &[(String, egui::Color32)], needle: &str) -> Option<egui::Color32> {
    painted
        .iter()
        .find(|(text, _)| text.contains(needle))
        .map(|(_, colour)| *colour)
}

/// A URL is drawn as a link in the table, the tree and the panel.
#[test]
fn a_url_is_offered_as_a_link_in_every_view() {
    let ctx = ctx();
    let mut app = VuwrApp::new(
        None,
        Document::parse(FEED.as_bytes(), FormatHint::Xml).unwrap(),
    );
    let accent = {
        vuwr_gui::set_dark(false);
        vuwr_gui::accent_text()
    };

    for (view, cmd) in [
        (ViewMode::Table, Command::ViewTable),
        (ViewMode::Tree, Command::ViewTree),
    ] {
        app.run(cmd, &ctx);
        let shown = painted(&mut app, &ctx);
        assert_eq!(
            link_colour_of(&shown, "https://example.com/products/a1"),
            Some(accent),
            "{view:?}: the URL is not drawn as a link"
        );
    }
}

/// And the setting turns it off, which is the point of its being one.
#[test]
fn the_setting_turns_links_back_into_text() {
    let ctx = ctx();
    let mut app = VuwrApp::new(
        None,
        Document::parse(FEED.as_bytes(), FormatHint::Xml).unwrap(),
    );
    app.run(Command::ViewTable, &ctx);
    let accent = {
        vuwr_gui::set_dark(false);
        vuwr_gui::accent_text()
    };

    app.run(Command::ToggleLinks, &ctx);
    assert!(!app.session().links_clickable);
    let shown = painted(&mut app, &ctx);
    let colour = link_colour_of(&shown, "https://example.com/products/a1");
    assert!(
        colour.is_some() && colour != Some(accent),
        "the URL is still drawn as a link: {colour:?}"
    );
}

/// A value that merely contains a URL is not itself a link.
///
/// Following a click on a description because there is an address
/// somewhere in it would be a surprise; the whole value has to be one.
#[test]
fn only_a_whole_value_is_a_link() {
    assert!(vuwr_core::as_link("https://example.com").is_some());
    assert!(vuwr_core::as_link("see https://example.com for more").is_none());
    assert!(vuwr_core::as_link("129.00 AUD").is_none());
}
