//! Contrast: nothing may be drawn in a colour you cannot read against
//! the ground behind it — in either mode, and across a switch between
//! them. Switching to dark left the table's own text nearly invisible.
//!
//! Contrast is the one property of a palette that can be checked
//! arithmetically, so it is checked rather than looked at: every string
//! the app paints, against whatever was filled underneath it.
//!
//! Its own binary because the ground is a process-global: a test that
//! switches it would otherwise reach every other test rendering beside
//! it. Cargo gives each integration test file its own process.

use eframe::egui;
use vuwr_core::{Command, Document, FormatHint};
use vuwr_gui::VuwrApp;

const SAMPLE: &str = "name,city,age\nAlice,Sydney,30\nBob,Perth,25\n";

fn doc(src: &str) -> Document {
    Document::parse(src.as_bytes(), FormatHint::Auto).unwrap()
}

/// WCAG relative luminance.
fn luminance(c: egui::Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

fn ratio(a: egui::Color32, b: egui::Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// One painted thing, in paint order.
enum Item {
    Fill(egui::Rect, egui::Color32),
    Text(egui::Pos2, egui::Color32, String),
}

/// `src` over `dst`. egui's colours are premultiplied, so the source
/// channels are already scaled and only the destination is faded.
fn over(src: egui::Color32, dst: egui::Color32) -> egui::Color32 {
    let keep = 1.0 - src.a() as f32 / 255.0;
    let mix = |s: u8, d: u8| (s as f32 + d as f32 * keep).round().min(255.0) as u8;
    egui::Color32::from_rgb(
        mix(src.r(), dst.r()),
        mix(src.g(), dst.g()),
        mix(src.b(), dst.b()),
    )
}

/// `Start` opens in the mode; `Switch` opens in the other one and
/// changes to it the way the View menu does.
#[derive(Clone, Copy)]
enum How {
    Start,
    Switch,
}

fn painted(dark: bool, how: How) -> Vec<Item> {
    vuwr_gui::set_dark(match how {
        How::Start => dark,
        How::Switch => !dark,
    });
    let ctx = egui::Context::default();
    vuwr_gui::install_fonts(&ctx);
    vuwr_gui::install_theme(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});

    let mut app = VuwrApp::new(None, doc(SAMPLE));
    app.dark = !dark;
    app.run(Command::ViewTable, &ctx);
    let mut frame = eframe::Frame::_new_kittest();
    let mut out = Vec::new();
    // One frame in the mode it started in, then the switch, then the
    // frames that are actually inspected.
    for pass in 0..3 {
        if pass == 1 {
            // As a control that sets both would: the preference and
            // the ground together.
            app.dark = dark;
            vuwr_gui::set_dark(dark);
        }
        out.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| eframe::App::update(&mut app, ctx, &mut frame));
        for clipped in &full.shapes {
            walk(&clipped.shape, &mut out);
        }
    }
    out
}

fn walk(shape: &egui::Shape, out: &mut Vec<Item>) {
    match shape {
        egui::Shape::Rect(r) if r.fill.a() > 0 => out.push(Item::Fill(r.rect, r.fill)),
        egui::Shape::Text(t) => {
            let text = t.galley.text().to_string();
            if text.trim().is_empty() {
                return;
            }
            // The colour actually in the mesh, which is what reaches
            // the screen — not what a call site claimed to pass.
            let colour = t.override_text_color.unwrap_or_else(|| {
                t.galley
                    .rows
                    .iter()
                    .find_map(|r| r.visuals.mesh.vertices.first().map(|v| v.color))
                    .unwrap_or(egui::Color32::PLACEHOLDER)
            });
            out.push(Item::Text(t.pos, colour, text));
        }
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                walk(s, out);
            }
        }
        _ => {}
    }
}

#[test]
fn every_string_reads_against_its_ground() {
    for (dark, how, name) in [
        (false, How::Start, "light"),
        (true, How::Start, "dark"),
        (false, How::Switch, "switched to light"),
        (true, How::Switch, "switched to dark"),
    ] {
        let items = painted(dark, how);
        let ground = {
            vuwr_gui::set_dark(dark);
            vuwr_gui::surface()
        };
        assert!(!items.is_empty(), "nothing was drawn");
        let mut worst: Option<(f32, egui::Color32, egui::Color32, String)> = None;
        for (i, item) in items.iter().enumerate() {
            let Item::Text(pos, colour, text) = item else {
                continue;
            };
            if *colour == egui::Color32::PLACEHOLDER || colour.a() == 0 {
                continue;
            }
            // Controls that cannot be used are faint on purpose, and
            // egui fades them further, so their drawn colour is not
            // any one value to test against. With a document just
            // opened there is nothing to undo, redo or clear.
            if *colour == vuwr_gui::text_disabled()
                || matches!(text.as_str(), "Undo" | "Redo" | "Clear")
            {
                continue;
            }
            // Whatever was filled underneath it, last one wins — a
            // label on a filled button is read against the button.
            let mut behind = ground;
            for earlier in &items[..i] {
                if let Item::Fill(rect, fill) = earlier
                    && rect.contains(*pos)
                {
                    behind = over(*fill, behind);
                }
            }
            let front = over(*colour, behind);
            let r = ratio(front, behind);
            if worst.as_ref().is_none_or(|(w, ..)| r < *w) {
                worst = Some((r, front, behind, text.clone()));
            }
        }
        let (r, front, behind, text) = worst.expect("something was drawn in a colour");
        assert!(
            r >= 3.0,
            "{name}: {text:?} is {front:?} on {behind:?} — contrast {r:.2}:1"
        );
    }
    vuwr_gui::set_dark(false);
}
