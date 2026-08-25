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

use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::Color;
use vuwr_core::Scheme;

static SCHEME: AtomicUsize = AtomicUsize::new(0);

/// The scheme the document's own text is coloured by. The chrome keeps
/// the terminal's own five colours: a status line in Gruvbox red would be
/// a scheme deciding something it was not asked about.
pub fn scheme() -> Scheme {
    Scheme::ALL
        .get(SCHEME.load(Ordering::Relaxed))
        .copied()
        .unwrap_or(Scheme::Vuwr)
}

pub fn set_scheme(chosen: Scheme) {
    let i = Scheme::ALL.iter().position(|s| *s == chosen).unwrap_or(0);
    SCHEME.store(i, Ordering::Relaxed);
}

/// A colour from core's table. Truecolor only: an indexed terminal cannot
/// show a scheme's palette, so it keeps the five it can.
fn from_rgb(c: vuwr_core::Rgb, fallback: Color) -> Color {
    pick(Color::Rgb(c.0, c.1, c.2), fallback)
}

/// The colour for one syntax token under the chosen scheme.
pub fn token(t: vuwr_core::Token) -> Color {
    use vuwr_core::Token as T;
    let fallback = match t {
        T::Key | T::Tag | T::Keyword => accent(),
        T::Comment => faint(),
        T::Escape => warn(),
        T::Punctuation => dim(),
        T::Str | T::Number | T::Plain => text(),
    };
    // The terminal is dark far more often than not, and a scheme that
    // names a ground is asking for that one.
    let dark = !matches!(scheme().ground(), Some(vuwr_core::Ground::Light));
    from_rgb(scheme().token(t, dark), fallback)
}

/// The colour for a tree value of a given kind.
pub fn value(kind: vuwr_core::ValueKind) -> Color {
    use vuwr_core::ValueKind as V;
    let fallback = match kind {
        V::Array | V::Object | V::Element => placeholder(),
        V::Null | V::Comment => faint(),
        _ => text(),
    };
    let dark = !matches!(scheme().ground(), Some(vuwr_core::Ground::Light));
    from_rgb(scheme().value(kind, dark), fallback)
}

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
///
/// The indexed fallback is *light* blue: plain `Blue` is near-black on a
/// dark terminal, which is where a terminal application spends its life.
pub fn accent() -> Color {
    pick(Color::Rgb(0x6E, 0xA2, 0xE0), Color::LightBlue)
}

/// Unsaved, outliers, anything the user should look at.
pub fn warn() -> Color {
    pick(Color::Rgb(0xD9, 0xA2, 0x3C), Color::LightYellow)
}

/// A problem, as opposed to a caution.
pub fn bad() -> Color {
    pick(Color::Rgb(0xD1, 0x6B, 0x5A), Color::LightRed)
}

/// The background of the row the cursor is on.
pub fn row_selected() -> Color {
    pick(Color::Rgb(0x1C, 0x26, 0x36), Color::Blue)
}

/// A value that is really a placeholder — `<item>`, `{…}` — rather than
/// content. Meta, so it recedes behind the values around it.
pub fn placeholder() -> Color {
    dim()
}

/// The border of the cell being edited.
pub fn edit_ring() -> Color {
    pick(Color::Rgb(0xB4, 0x67, 0x1F), Color::Yellow)
}
