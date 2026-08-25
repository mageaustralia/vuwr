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

use ratatui::style::{Color, Style};
use vuwr_core::Scheme;

static SCHEME: AtomicUsize = AtomicUsize::new(0);

static GROUND: AtomicUsize = AtomicUsize::new(GROUND_UNKNOWN);
const GROUND_UNKNOWN: usize = 0;
const GROUND_DARK: usize = 1;
const GROUND_LIGHT: usize = 2;

/// Ask the terminal what colour it is, once, at startup.
///
/// Called from `run` with the terminal in raw mode. Everything after this
/// reads the answer; before it, nothing is assumed.
pub fn detect_ground() {
    let known = match crate::detect::background() {
        Some(colour) if crate::detect::is_dark(colour) => GROUND_DARK,
        Some(_) => GROUND_LIGHT,
        // `COLORFGBG`, for a terminal that will not answer but did set it:
        // two or three fields, the last being the background as an ANSI
        // colour index. Under 8 is dark.
        None => match std::env::var("COLORFGBG")
            .ok()
            .and_then(|v| v.rsplit(';').next()?.trim().parse::<u8>().ok())
        {
            Some(bg) if bg < 8 => GROUND_DARK,
            Some(_) => GROUND_LIGHT,
            None => GROUND_UNKNOWN,
        },
    };
    GROUND.store(known, Ordering::Relaxed);
}

/// Whether the terminal's own background is dark.
///
/// Only meaningful once [`detect_ground`] has run, and a terminal that
/// will not say leaves it unknown. Nothing that has to stay legible
/// depends on this being right — see [`token`].
pub fn terminal_is_dark() -> bool {
    GROUND.load(Ordering::Relaxed) != GROUND_LIGHT
}

/// Whether the terminal told us, rather than us assuming.
pub fn ground_is_known() -> bool {
    GROUND.load(Ordering::Relaxed) != GROUND_UNKNOWN
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
///
/// Two things keep this legible whatever the terminal turns out to be.
///
/// The file's own prose — a value, a line of text — is drawn in the
/// terminal's *own* foreground colour rather than one of ours. Whatever
/// that is, it reads: it is what the terminal draws everything else in.
/// Choosing a colour for the bulk of the text is how it ended up white on
/// white.
///
/// And when the terminal will not say what colour it is, the accents come
/// from a set that reads on either ground rather than from a guess.
pub fn token(t: vuwr_core::Token) -> Color {
    use vuwr_core::Token as T;
    let scheme = scheme();
    let content = matches!(t, T::Plain | T::Str);

    if scheme == Scheme::Vuwr {
        if content {
            return Color::Reset;
        }
        if !ground_is_known() {
            let (r, g, b) = Scheme::adaptive(t);
            return pick(Color::Rgb(r, g, b), accent_fallback(t));
        }
    }
    from_rgb(scheme.token(t, ground_is_dark()), accent_fallback(t))
}

/// What an indexed terminal gets instead, which is the same five colours
/// the rest of the chrome uses.
fn accent_fallback(t: vuwr_core::Token) -> Color {
    use vuwr_core::Token as T;
    match t {
        T::Key | T::Tag | T::Keyword => accent(),
        T::Comment => faint(),
        T::Escape => warn(),
        T::Punctuation => dim(),
        T::Str | T::Number | T::Plain => Color::Reset,
    }
}

/// The colour for a container's summary on a tree row.
pub fn placeholder_value() -> Color {
    token(vuwr_core::Token::Punctuation)
}

/// The colour for a leaf's value on a tree row.
///
/// Through [`token`], so a value is legible under the same rules the rest
/// of the file's text is.
pub fn value(kind: vuwr_core::ValueKind) -> Color {
    use vuwr_core::{Token as T, ValueKind as V};
    token(match kind {
        V::Null | V::Comment => T::Comment,
        V::Bool => T::Keyword,
        V::Number => T::Number,
        V::String => T::Str,
        V::Array | V::Object => T::Tag,
        V::Element | V::Text | V::Other => T::Plain,
    })
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

// The chrome's colours.
//
// Foregrounds are mid-tones, chosen to read on a white terminal and a
// black one alike, and the brightest of them is the terminal's own
// foreground — whatever that is, it reads. Nothing here is picked from a
// guess about the ground, because a guess that goes wrong here is a
// status line nobody can see.
//
// A *background* cannot be a mid-tone — it has to match the ground — so
// the one of those there is asks which ground we are on.

/// Active values, the current view's name: the terminal's own foreground.
pub fn text() -> Color {
    Color::Reset
}

/// Meta: counts, positions that are not the primary one.
pub fn dim() -> Color {
    pick(Color::Rgb(0x8A, 0x90, 0x99), Color::Gray)
}

/// Hint labels and anything else that should recede.
pub fn faint() -> Color {
    pick(Color::Rgb(0x7A, 0x80, 0x89), Color::DarkGray)
}

/// Paths, identifiers, and the views you are not in.
pub fn accent() -> Color {
    pick(Color::Rgb(0x4A, 0x90, 0xD9), Color::LightBlue)
}

/// Unsaved, outliers, anything the user should look at.
pub fn warn() -> Color {
    pick(Color::Rgb(0xC0, 0x8A, 0x2E), Color::LightYellow)
}

/// A problem, as opposed to a caution.
pub fn bad() -> Color {
    pick(Color::Rgb(0xCC, 0x5F, 0x52), Color::LightRed)
}

/// How to mark the row the cursor is on.
///
/// A background has to match the ground — a dark band under a light
/// terminal's dark text hides the row it is meant to point at — so this
/// is the one colour that cannot be a mid-tone.
///
/// When the terminal will not say what colour it is, this inverts
/// instead. Inversion is right on any ground by construction, which is
/// why terminals have always used it, and it is better than a band that
/// is wrong half the time.
pub fn selection() -> Style {
    if let Some((r, g, b)) = scheme().selection()
        && truecolor()
    {
        return Style::default().bg(Color::Rgb(r, g, b));
    }
    if !ground_is_known() {
        return Style::default().add_modifier(ratatui::style::Modifier::REVERSED);
    }
    let bg = if terminal_is_dark() {
        pick(Color::Rgb(0x1C, 0x26, 0x36), Color::Blue)
    } else {
        pick(Color::Rgb(0xDC, 0xE6, 0xF5), Color::Cyan)
    };
    Style::default().bg(bg)
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
