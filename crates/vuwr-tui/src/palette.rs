//! The terminal's share of the design's palette.
//!
//! The window and the terminal are the same application, so they say the
//! same things the same way: one accent for *active* and *selected*, amber
//! for unsaved and warnings, and everything else on one grey ramp. What
//! differs is the means — a terminal has one cell size and one font, so
//! hierarchy comes from colour and inversion rather than weight and scale.
//!
//! Terminals that cannot do 24-bit colour get the indexed equivalents.
//! The layout is identical either way: only the colours change, so nothing
//! moves when a session is opened over a plain `TERM`.

use ratatui::style::Color;

/// Whether the terminal was launched with truecolor support.
///
/// Read once per process. `COLORTERM` is what every terminal that supports
/// it sets, and the fallback is not a downgrade worth detecting harder.
fn truecolor() -> bool {
    use std::sync::OnceLock;
    static TRUECOLOR: OnceLock<bool> = OnceLock::new();
    *TRUECOLOR.get_or_init(|| {
        std::env::var("COLORTERM")
            .map(|v| v.contains("truecolor") || v.contains("24bit"))
            .unwrap_or(false)
    })
}

/// Pick between the exact colour and its indexed stand-in.
fn pick(exact: Color, fallback: Color) -> Color {
    if truecolor() { exact } else { fallback }
}

/// Active values, the current view's name.
pub fn text() -> Color {
    pick(Color::Rgb(0xE6, 0xE8, 0xEB), Color::White)
}

/// Meta: counts, positions that are not the primary one.
pub fn dim() -> Color {
    pick(Color::Rgb(0xAB, 0xB1, 0xBA), Color::Gray)
}

/// Hint labels and anything else that should recede.
pub fn faint() -> Color {
    pick(Color::Rgb(0x8D, 0x93, 0x9C), Color::DarkGray)
}

/// Paths, identifiers, and the views you are not in.
pub fn accent() -> Color {
    pick(Color::Rgb(0x6E, 0xA2, 0xE0), Color::Blue)
}

/// Unsaved, outliers, anything the user should look at.
pub fn warn() -> Color {
    pick(Color::Rgb(0xD9, 0xA2, 0x3C), Color::Yellow)
}

/// A problem, as opposed to a caution.
pub fn bad() -> Color {
    pick(Color::Rgb(0xD1, 0x6B, 0x5A), Color::Red)
}

/// The background of the row the cursor is on.
pub fn row_selected() -> Color {
    pick(Color::Rgb(0x1C, 0x26, 0x36), Color::Blue)
}

/// The border of the cell being edited.
pub fn edit_ring() -> Color {
    pick(Color::Rgb(0xB4, 0x67, 0x1F), Color::Yellow)
}
