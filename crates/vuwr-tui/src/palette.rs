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

/// Whether the terminal's own background is dark.
///
/// `COLORFGBG` is what terminals that will say anything set: two or three
/// fields, the last being the background as an ANSI colour index. Under 8
/// is a dark background, 8 and over a light one. Terminals that say
/// nothing are assumed dark, which is what most of them are and what the
/// bundled colours were chosen against.
pub fn terminal_is_dark() -> bool {
    use std::sync::OnceLock;
    static DARK: OnceLock<bool> = OnceLock::new();
    *DARK.get_or_init(|| {
        let Ok(value) = std::env::var("COLORFGBG") else {
            return true;
        };
        match value
            .rsplit(';')
            .next()
            .and_then(|b| b.trim().parse::<u8>().ok())
        {
            Some(bg) => bg < 8,
            None => true,
        }
    })
}

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

/// Which ground the document is being drawn on: the scheme's own where it
/// brings one, the terminal's otherwise.
pub fn ground_is_dark() -> bool {
    match scheme().ground() {
        Some(vuwr_core::Ground::Light) => false,
        Some(vuwr_core::Ground::Dark) => true,
        None => terminal_is_dark(),
    }
}

/// The surface a named scheme wants behind the document, or `None` to
/// leave the terminal's own background showing.
///
/// A scheme is a foreground *and* a background. Monokai's near-white text
/// on a light terminal is invisible, and picking a different grey does
/// not fix it — so a named scheme paints its own surface, as it does in
/// the editor it came from.
pub fn background() -> Option<Color> {
    if !truecolor() {
        return None;
    }
    scheme().background().map(|(r, g, b)| Color::Rgb(r, g, b))
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
    from_rgb(scheme().token(t, ground_is_dark()), fallback)
}

/// The colour for a tree value of a given kind.
pub fn value(kind: vuwr_core::ValueKind) -> Color {
    use vuwr_core::ValueKind as V;
    let fallback = match kind {
        V::Array | V::Object | V::Element => placeholder(),
        V::Null | V::Comment => faint(),
        _ => text(),
    };
    from_rgb(scheme().value(kind, ground_is_dark()), fallback)
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
    if let Some((r, g, b)) = scheme().selection()
        && truecolor()
    {
        return Color::Rgb(r, g, b);
    }
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
