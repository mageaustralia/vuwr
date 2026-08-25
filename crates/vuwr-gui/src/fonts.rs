//! The fonts the design asks for, carried with the binary.
//!
//! Bundled rather than borrowed from the system: the design names two
//! faces and five weights, and a window that renders in whatever the
//! platform happens to have is a different design on every machine. The
//! same five files go into the wasm, so the browser build matches too.
//!
//! egui's own faces stay behind them as fallbacks, so a glyph these two
//! lack still draws rather than becoming an empty box.

use eframe::egui::{self, FontData, FontFamily};

/// The faces, by the name each is referenced under.
const FACES: [(&str, &[u8]); 5] = [
    ("sans", include_bytes!("../fonts/IBMPlexSans-Regular.ttf")),
    (
        "sans_medium",
        include_bytes!("../fonts/IBMPlexSans-Medium.ttf"),
    ),
    (
        "sans_semi",
        include_bytes!("../fonts/IBMPlexSans-SemiBold.ttf"),
    ),
    ("mono", include_bytes!("../fonts/JetBrainsMono-Regular.ttf")),
    (
        "mono_medium",
        include_bytes!("../fonts/JetBrainsMono-Medium.ttf"),
    ),
];

/// Install them, returning the names adopted — which tests read to check
/// the bundle actually arrived.
pub fn install(ctx: &egui::Context) -> Vec<String> {
    let mut definitions = egui::FontDefinitions::default();
    let mut adopted = Vec::new();

    // Whatever egui ships with, to stand behind ours: a named family
    // holding one face has no fallback, so a glyph that face lacks — ⌘,
    // an arrow, anything outside Latin — draws as an empty box or a `?`.
    let fallbacks: Vec<String> = definitions.font_data.keys().cloned().collect();

    for (name, bytes) in FACES {
        definitions.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(FontData::from_static(bytes)),
        );
        // Each face is its own family, so a text style can ask for a
        // weight rather than asking egui to fake one — which it will not.
        let mut chain = vec![name.to_owned()];
        chain.extend(fallbacks.iter().cloned());
        definitions
            .families
            .insert(FontFamily::Name(name.into()), chain);
        adopted.push(name.to_string());
    }

    // The two defaults, so anything that does not name a face still gets
    // the right one. egui's own stay behind them as fallbacks.
    definitions
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "sans".to_owned());
    definitions
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "mono".to_owned());

    ctx.set_fonts(definitions);
    adopted
}
