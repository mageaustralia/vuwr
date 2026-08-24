//! Use the platform's own fonts where we can.
//!
//! egui bundles Ubuntu Light and Hack, which look out of place on a Mac
//! and are the reason exotic glyphs render as empty boxes. Where a system
//! font is available we load it and put it first; the bundled fonts stay
//! behind it as fallbacks, so anything the system font lacks still draws.
//!
//! Nothing here is fatal: if a font is missing or unreadable, egui's own
//! stay in charge.

use eframe::egui::{self, FontData, FontFamily};

/// Candidate faces, best first. Variable fonts (`SFNS.ttf`) and
/// collections (`.ttc`) are skipped: the rasteriser reads neither, and a
/// failed load is worse than not trying.
#[cfg(target_os = "macos")]
const PROPORTIONAL: &[&str] = &[
    "/System/Library/Fonts/SFNSRounded.ttf",
    "/Library/Fonts/Arial.ttf",
];

#[cfg(target_os = "macos")]
const MONOSPACE: &[&str] = &[
    "/System/Library/Fonts/SFNSMono.ttf",
    "/System/Library/Fonts/Monaco.ttf",
];

#[cfg(all(unix, not(target_os = "macos")))]
const PROPORTIONAL: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

#[cfg(all(unix, not(target_os = "macos")))]
const MONOSPACE: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
];

#[cfg(not(unix))]
const PROPORTIONAL: &[&str] = &["C:\\Windows\\Fonts\\segoeui.ttf"];

#[cfg(not(unix))]
const MONOSPACE: &[&str] = &["C:\\Windows\\Fonts\\consola.ttf"];

/// Install system fonts ahead of the bundled ones.
///
/// Returns the names of the faces that were adopted, for reporting.
pub fn install(ctx: &egui::Context) -> Vec<String> {
    let mut definitions = egui::FontDefinitions::default();
    let mut adopted = Vec::new();

    for (family, candidates, key) in [
        (FontFamily::Proportional, PROPORTIONAL, "system-ui"),
        (FontFamily::Monospace, MONOSPACE, "system-mono"),
    ] {
        let Some((path, bytes)) = first_readable(candidates) else {
            continue;
        };
        definitions.font_data.insert(
            key.to_owned(),
            std::sync::Arc::new(FontData::from_owned(bytes)),
        );
        definitions
            .families
            .entry(family)
            .or_default()
            // First, so it wins; the bundled fonts remain behind it and
            // cover anything it is missing.
            .insert(0, key.to_owned());
        adopted.push(path);
    }

    if !adopted.is_empty() {
        ctx.set_fonts(definitions);
    }
    adopted
}

#[cfg(not(target_arch = "wasm32"))]
fn first_readable(candidates: &[&str]) -> Option<(String, Vec<u8>)> {
    candidates.iter().find_map(|path| {
        std::fs::read(path)
            .ok()
            .map(|bytes| ((*path).to_string(), bytes))
    })
}

/// The browser has no filesystem to read fonts from, and the bundled ones
/// are already the right answer there.
#[cfg(target_arch = "wasm32")]
fn first_readable(_candidates: &[&str]) -> Option<(String, Vec<u8>)> {
    None
}
