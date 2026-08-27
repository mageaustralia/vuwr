//! Driving the window: real key events through the real app.
//!
//! The GUI's own tests mostly called commands directly. These go in
//! through `command_for`, the same route a keypress takes, so a binding
//! that stops resolving is caught here rather than by a reader.

use eframe::egui::{self, Key, Modifiers};
use vuwr_core::{Command, Document, FormatHint, ViewMode};
use vuwr_gui::{VuwrApp, command_for, command_for_char};

const STOCK: &str = "sku,city\nA1,Sydney\nA2,Perth\nA3,Hobart\n";

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

/// Press a key the way the app receives one.
fn press(app: &mut VuwrApp, ctx: &egui::Context, key: Key, mods: Modifiers) {
    if let Some(cmd) = command_for(key, mods, false) {
        app.run(cmd, ctx);
    } else {
        panic!("{key:?} with {mods:?} is not bound to anything");
    }
}

fn source(app: &VuwrApp) -> String {
    String::from_utf8(app.session().doc.serialize()).unwrap()
}

/// The arrow keys and the vim letters reach the same place.
#[test]
fn arrows_and_letters_agree_in_the_window() {
    for keys in [[Key::ArrowDown, Key::ArrowRight], [Key::J, Key::L]] {
        let ctx = ctx();
        let mut app = VuwrApp::new(None, doc(STOCK));
        app.run(Command::ViewTable, &ctx);
        for key in keys {
            press(&mut app, &ctx, key, Modifiers::NONE);
        }
        assert_eq!(app.session().grid.cursor, (1, 1), "{keys:?}");
    }
}

/// Every key the help window advertises resolves to the command it names.
///
/// The help is read by people who cannot find a thing; a help page that
/// names a key which does nothing is worse than none.
#[test]
fn the_help_does_not_advertise_a_key_that_is_not_bound() {
    let mut wrong = Vec::new();
    for cmd in Command::ALL {
        let advertised = vuwr_gui::keys_for(*cmd);
        // Only the single-letter claims can be checked mechanically;
        // "Space / PgDn" and the like are prose.
        let single: Vec<char> = advertised.chars().collect();
        if single.len() != 1 {
            continue;
        }
        let c = single[0];
        let Some(key) = Key::from_name(&c.to_ascii_uppercase().to_string()) else {
            continue;
        };
        let mods = if c.is_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        // Either route: egui has no `Key` for some punctuation, so those
        // arrive as text and are resolved there instead.
        let resolved = command_for(key, mods, false).or_else(|| command_for_char(c));
        match resolved {
            Some(got) if got == *cmd => {}
            Some(got) => wrong.push(format!(
                "help says {advertised:?} runs {cmd:?}, but it runs {got:?}"
            )),
            None => wrong.push(format!(
                "help says {advertised:?} runs {cmd:?}; it does nothing"
            )),
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// A whole edit, through the app: open the field, type, commit, undo.
#[test]
fn a_cell_is_edited_through_the_window() {
    let ctx = ctx();
    let mut app = VuwrApp::new(None, doc(STOCK));
    app.run(Command::ViewTable, &ctx);
    app.session_mut().grid.cursor = (1, 1);

    app.run(Command::ReplaceCell, &ctx);
    for c in "Darwin".chars() {
        app.session_mut().input_char(c);
    }
    app.session_mut().input_submit();
    assert!(source(&app).contains("A1,Darwin"), "{}", source(&app));

    app.run(Command::Undo, &ctx);
    assert_eq!(source(&app), STOCK, "undo did not restore it");
    app.run(Command::Redo, &ctx);
    assert!(source(&app).contains("Darwin"), "redo did not put it back");
}

/// Switching views draws each one without panicking, for each format.
///
/// egui aborts the process on a text style it does not know, so "it drew"
/// is a real assertion here rather than a formality.
#[test]
fn every_view_of_every_format_draws_through_the_app() {
    let cases: [(&str, &str); 3] = [
        ("csv", STOCK),
        ("json", r#"[{"sku": "A1", "city": "Sydney"}]"#),
        ("xml", "<r>\n<item><sku>A1</sku></item>\n</r>\n"),
    ];
    for (name, src) in cases {
        let ctx = ctx();
        let mut app = VuwrApp::new(None, doc(src));
        let mut frame = eframe::Frame::_new_kittest();
        for view in [ViewMode::Table, ViewMode::Tree, ViewMode::Text] {
            if !app.session().available_views().contains(&view) {
                continue;
            }
            app.run(
                match view {
                    ViewMode::Table => Command::ViewTable,
                    ViewMode::Tree => Command::ViewTree,
                    ViewMode::Text => Command::ViewText,
                },
                &ctx,
            );
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| eframe::App::update(&mut app, ctx, &mut frame));
            assert!(
                !output.shapes.is_empty(),
                "{name}/{view:?}: drew nothing at all"
            );
        }
    }
}

/// A character must be bound on one route or the other, never both.
///
/// egui delivers a printable key as a `Key` event *and* as text. A
/// character resolved by both would run its command twice — which cancels
/// a toggle silently, and does a bulk replace twice. The rule is written
/// above `command_for_char`; this is it enforced.
#[test]
fn no_character_is_bound_twice() {
    let mut doubled = Vec::new();
    for c in ' '..='~' {
        let Some(by_text) = command_for_char(c) else {
            continue;
        };
        let name = c.to_ascii_uppercase().to_string();
        let Some(key) = Key::from_name(&name) else {
            continue;
        };
        let mods = if c.is_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        if let Some(by_key) = command_for(key, mods, false) {
            doubled.push(format!(
                "{c:?} runs {by_key:?} as a key and {by_text:?} as text"
            ));
        }
    }
    assert!(doubled.is_empty(), "\n{}", doubled.join("\n"));
}
