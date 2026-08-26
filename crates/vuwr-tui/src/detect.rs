//! Ask the terminal what colour it is.
//!
//! Guessing is not good enough. A dark scheme drawn on a white terminal
//! is not merely wrong, it is unreadable — near-white text on white — and
//! the guess was wrong for anybody whose terminal does not set
//! `COLORFGBG`, which is most of them.
//!
//! So: ask. `OSC 11` is the question ("what is your background?") and
//! every terminal worth the name answers it — iTerm2, Terminal.app,
//! kitty, WezTerm, Alacritty, foot, and xterm itself, which invented it.
//! One that does not answer costs a tenth of a second and falls back.

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::time::{Duration, Instant};

/// How long to wait for an answer. Long enough for a terminal over ssh,
/// short enough that nobody notices one that stays silent.
#[cfg(unix)]
const PATIENCE: Duration = Duration::from_millis(120);

/// The terminal's background colour, if it will say.
///
/// Must be called with the terminal in raw mode, or the reply is line
/// buffered and never arrives. Anything unexpected on the way back is
/// treated as no answer rather than parsed hopefully.
#[cfg(unix)]
pub fn background() -> Option<(u8, u8, u8)> {
    let mut out = std::io::stdout();
    // `ESC ] 11 ; ? BEL` — "what is your background colour?"
    out.write_all(b"\x1b]11;?\x07").ok()?;
    out.flush().ok()?;

    let deadline = Instant::now() + PATIENCE;
    let mut reply = Vec::new();
    let mut stdin = std::io::stdin();
    let mut byte = [0u8; 1];

    while Instant::now() < deadline {
        // Waiting on the descriptor itself, not through crossterm's event
        // reader: that one buffers whatever it polls, so the bytes of the
        // answer never reached the read below — and crossterm handed them
        // to the app afterwards as though somebody had typed them.
        //
        // A blocking read is not an option either: a terminal that never
        // answers would hang the program on startup, which is worse than
        // the bug being fixed.
        if !readable(stdin.as_raw_fd(), deadline) {
            break;
        }
        if stdin.read(&mut byte).ok()? == 0 {
            break;
        }
        reply.push(byte[0]);
        // The answer ends with BEL or ST, and is short. A long one is not
        // an answer to this question.
        if byte[0] == 0x07 || reply.ends_with(b"\x1b\\") || reply.len() > 64 {
            break;
        }
    }
    parse(&reply)
}

/// Not asked on Windows.
///
/// Reading the reply means waiting on the console handle without letting
/// the event reader buffer it first, which is a different problem there
/// and not one worth solving blind. Saying "unknown" costs nothing: the
/// palette already has a set of colours for a ground it cannot determine,
/// chosen to read on either — see `palette::token`.
#[cfg(not(unix))]
pub fn background() -> Option<(u8, u8, u8)> {
    None
}

/// Whether the descriptor has something to read before the deadline.
#[cfg(unix)]
fn readable(fd: std::os::fd::RawFd, deadline: Instant) -> bool {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return false;
    }
    let mut waiting = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one initialised `pollfd`, a matching count, and a timeout.
    // `poll` reads and writes only that struct.
    let ready = unsafe { libc::poll(&mut waiting, 1, left.as_millis() as libc::c_int) };
    ready > 0 && waiting.revents & libc::POLLIN != 0
}

/// Pull `rgb:RRRR/GGGG/BBBB` out of a reply, in whatever width the
/// terminal chose to give it — one to four hex digits per channel.
///
/// Only a Unix build asks the question, but the parsing is worth keeping
/// compiled and tested everywhere: it is where a transcription error
/// would hide, and a test that runs on one platform only is a test that
/// rots on the others.
#[cfg_attr(not(unix), allow(dead_code))]
fn parse(reply: &[u8]) -> Option<(u8, u8, u8)> {
    let text = std::str::from_utf8(reply).ok()?;
    let rest = text.split("rgb:").nth(1)?;
    let mut channels = rest.split('/');
    let r = channel(channels.next()?)?;
    let g = channel(channels.next()?)?;
    let b = channel(channels.next()?)?;
    Some((r, g, b))
}

/// One channel, scaled to a byte. Terminals answer in 16 bits more often
/// than not, so `ffff` and `ff` both have to mean full.
#[cfg_attr(not(unix), allow(dead_code))]
fn channel(text: &str) -> Option<u8> {
    let hex: String = text.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() {
        return None;
    }
    let value = u32::from_str_radix(&hex, 16).ok()?;
    let full = 16u32.pow(hex.len() as u32) - 1;
    Some((value * 255 / full.max(1)) as u8)
}

/// Whether a colour is dark enough to want light text on it.
///
/// Perceived brightness rather than the plain average: the eye takes far
/// more of it from green than from blue, and a saturated blue background
/// averages dark while looking mid.
pub fn is_dark(colour: (u8, u8, u8)) -> bool {
    let (r, g, b) = colour;
    let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    luma < 128.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_read_in_any_width() {
        // 16 bits per channel, which is what most terminals answer.
        assert_eq!(
            parse(b"\x1b]11;rgb:ffff/ffff/ffff\x07"),
            Some((255, 255, 255))
        );
        assert_eq!(parse(b"\x1b]11;rgb:0000/0000/0000\x07"), Some((0, 0, 0)));
        // And 8, which some do.
        assert_eq!(parse(b"\x1b]11;rgb:1e/1e/2e\x07"), Some((0x1e, 0x1e, 0x2e)));
        // Terminated with ST rather than BEL.
        assert_eq!(
            parse(b"\x1b]11;rgb:2828/2828/2828\x1b\\"),
            Some((0x28, 0x28, 0x28))
        );
    }

    #[test]
    fn nonsense_is_not_an_answer() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"\x1b]11;?\x07"), None);
        assert_eq!(parse(b"rgb:zz/zz/zz"), None);
        assert_eq!(parse(b"rgb:ff/ff"), None, "a colour has three channels");
    }

    #[test]
    fn darkness_is_perceived_rather_than_averaged() {
        assert!(is_dark((0x1E, 0x1E, 0x2E)));
        assert!(is_dark((0x28, 0x28, 0x28)));
        assert!(!is_dark((0xFF, 0xFF, 0xFF)));
        assert!(!is_dark((0xFD, 0xF6, 0xE3)), "Solarized light");
        // Averages to 85 and reads as mid: green carries the brightness.
        assert!(!is_dark((0x00, 0xFF, 0x00)));
    }
}
