//! Colour schemes for the file's own text.
//!
//! The chrome is the design's and does not change; what a scheme decides
//! is how the *document* is coloured — tags, strings, numbers, comments —
//! plus whether it wants a light or a dark ground under them.
//!
//! The table lives in core rather than in a frontend because both of them
//! colour the same tokens, and two copies of a palette drift. Colours are
//! plain bytes here; each frontend turns them into its own type.
//!
//! The borrowed schemes are the usual suspects from vim, with their
//! published values rather than an approximation: someone who knows
//! Gruvbox will notice if the yellow is wrong.

use crate::Token;

/// Which ground a scheme is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    Light,
    Dark,
}

/// A colour, as sRGB bytes.
pub type Rgb = (u8, u8, u8);

const fn rgb(hex: u32) -> Rgb {
    ((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The schemes on offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// The design's own: one accent, amber for warnings, everything else
    /// on the grey ramp. Follows whichever ground is chosen.
    Vuwr,
    GruvboxDark,
    GruvboxLight,
    SolarizedDark,
    SolarizedLight,
    Nord,
    Monokai,
}

impl Scheme {
    pub const ALL: &'static [Self] = &[
        Self::Vuwr,
        Self::GruvboxDark,
        Self::GruvboxLight,
        Self::SolarizedDark,
        Self::SolarizedLight,
        Self::Nord,
        Self::Monokai,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Vuwr => "vuwr",
            Self::GruvboxDark => "Gruvbox dark",
            Self::GruvboxLight => "Gruvbox light",
            Self::SolarizedDark => "Solarized dark",
            Self::SolarizedLight => "Solarized light",
            Self::Nord => "Nord",
            Self::Monokai => "Monokai",
        }
    }

    /// Look this up by name, for `:scheme gruvbox-dark` and the like.
    pub fn from_name(name: &str) -> Option<Self> {
        let wanted = name.trim().to_ascii_lowercase().replace([' ', '_'], "-");
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.name().to_ascii_lowercase().replace([' ', '_'], "-") == wanted)
    }

    /// The ground this scheme expects, or `None` for one that works on
    /// either — which is only ours.
    pub fn ground(self) -> Option<Ground> {
        match self {
            Self::Vuwr => None,
            Self::GruvboxLight | Self::SolarizedLight => Some(Ground::Light),
            Self::GruvboxDark | Self::SolarizedDark | Self::Nord | Self::Monokai => {
                Some(Ground::Dark)
            }
        }
    }

    /// The colour for one token. `dark` says which ground is actually in
    /// use, which only matters for the schemes that work on either.
    pub fn token(self, token: Token, dark: bool) -> Rgb {
        use Token as T;
        match self {
            // Ours: structure in the accent, content in the body colour,
            // punctuation receding — the palette the rest of the app uses
            // rather than a second one for syntax.
            Self::Vuwr => {
                if dark {
                    match token {
                        T::Key | T::Tag => rgb(0x7AA2F7),
                        T::Keyword => rgb(0xC39BE8),
                        T::Str => rgb(0xA8CC8C),
                        T::Number => rgb(0xE5A76A),
                        T::Comment => rgb(0x6E7D91),
                        T::Escape => rgb(0xE0B341),
                        T::Punctuation => rgb(0x8892A0),
                        T::Plain => rgb(0xD6DCE5),
                    }
                } else {
                    match token {
                        T::Key | T::Tag => rgb(0x1C5FA8),
                        T::Keyword => rgb(0x6B3FA0),
                        T::Str => rgb(0x2F6B3A),
                        T::Number => rgb(0x9A5518),
                        T::Comment => rgb(0x7A828C),
                        T::Escape => rgb(0x8A5A1E),
                        T::Punctuation => rgb(0x6B7280),
                        T::Plain => rgb(0x2B3038),
                    }
                }
            }
            Self::GruvboxDark => match token {
                T::Key | T::Tag => rgb(0x83A598),
                T::Keyword => rgb(0xFB4934),
                T::Str => rgb(0xB8BB26),
                T::Number => rgb(0xD3869B),
                T::Comment => rgb(0x928374),
                T::Escape => rgb(0xFE8019),
                T::Punctuation => rgb(0xA89984),
                T::Plain => rgb(0xEBDBB2),
            },
            Self::GruvboxLight => match token {
                T::Key | T::Tag => rgb(0x076678),
                T::Keyword => rgb(0x9D0006),
                T::Str => rgb(0x79740E),
                T::Number => rgb(0x8F3F71),
                T::Comment => rgb(0x928374),
                T::Escape => rgb(0xAF3A03),
                T::Punctuation => rgb(0x7C6F64),
                T::Plain => rgb(0x3C3836),
            },
            Self::SolarizedDark => match token {
                T::Key | T::Tag => rgb(0x268BD2),
                T::Keyword => rgb(0x859900),
                T::Str => rgb(0x2AA198),
                T::Number => rgb(0xD33682),
                T::Comment => rgb(0x586E75),
                T::Escape => rgb(0xCB4B16),
                T::Punctuation => rgb(0x93A1A1),
                T::Plain => rgb(0x839496),
            },
            Self::SolarizedLight => match token {
                T::Key | T::Tag => rgb(0x268BD2),
                T::Keyword => rgb(0x859900),
                T::Str => rgb(0x2AA198),
                T::Number => rgb(0xD33682),
                T::Comment => rgb(0x93A1A1),
                T::Escape => rgb(0xCB4B16),
                T::Punctuation => rgb(0x657B83),
                T::Plain => rgb(0x586E75),
            },
            Self::Nord => match token {
                T::Key | T::Tag => rgb(0x88C0D0),
                T::Keyword => rgb(0x81A1C1),
                T::Str => rgb(0xA3BE8C),
                T::Number => rgb(0xB48EAD),
                T::Comment => rgb(0x616E88),
                T::Escape => rgb(0xEBCB8B),
                T::Punctuation => rgb(0x8FBCBB),
                T::Plain => rgb(0xD8DEE9),
            },
            Self::Monokai => match token {
                T::Key | T::Tag => rgb(0x66D9EF),
                T::Keyword => rgb(0xF92672),
                T::Str => rgb(0xE6DB74),
                T::Number => rgb(0xAE81FF),
                T::Comment => rgb(0x75715E),
                T::Escape => rgb(0xFD971F),
                T::Punctuation => rgb(0xF8F8F2),
                T::Plain => rgb(0xF8F8F2),
            },
        }
    }

    /// The ground this scheme wants behind the document, or `None` for
    /// one with no opinion — which is only ours.
    ///
    /// A scheme is a foreground *and* a background: Monokai's near-white
    /// text on a light terminal is invisible, and no amount of choosing
    /// the right grey fixes that. So a named scheme paints its own
    /// surface, exactly as it does in the editor it came from, and ours
    /// leaves the terminal's own background alone.
    pub fn background(self) -> Option<Rgb> {
        match self {
            Self::Vuwr => None,
            Self::GruvboxDark => Some(rgb(0x282828)),
            Self::GruvboxLight => Some(rgb(0xFBF1C7)),
            Self::SolarizedDark => Some(rgb(0x002B36)),
            Self::SolarizedLight => Some(rgb(0xFDF6E3)),
            Self::Nord => Some(rgb(0x2E3440)),
            Self::Monokai => Some(rgb(0x272822)),
        }
    }

    /// The row the cursor is on, against this scheme's ground.
    pub fn selection(self) -> Option<Rgb> {
        match self {
            Self::Vuwr => None,
            Self::GruvboxDark => Some(rgb(0x3C3836)),
            Self::GruvboxLight => Some(rgb(0xEBDBB2)),
            Self::SolarizedDark => Some(rgb(0x073642)),
            Self::SolarizedLight => Some(rgb(0xEEE8D5)),
            Self::Nord => Some(rgb(0x3B4252)),
            Self::Monokai => Some(rgb(0x3E3D32)),
        }
    }

    /// The colour for the value on a tree row, which is the same
    /// vocabulary read through the token table: a string is a string.
    ///
    /// For a leaf. A container's summary — `<item>`, `{…}` — is a
    /// placeholder rather than a value, and the row knows which it is;
    /// see [`Scheme::placeholder`]. An XML leaf is an `Element` and its
    /// value is its text, which is why that lands on content here.
    pub fn value(self, kind: crate::ValueKind, dark: bool) -> Rgb {
        use crate::ValueKind as V;
        let token = match kind {
            V::Null | V::Comment => Token::Comment,
            V::Bool => Token::Keyword,
            V::Number => Token::Number,
            V::String => Token::Str,
            V::Array | V::Object => Token::Tag,
            V::Element | V::Text | V::Other => Token::Plain,
        };
        self.token(token, dark)
    }

    /// A colour that reads on any ground, for when nobody knows what the
    /// ground is.
    ///
    /// A terminal that will not say what colour it is leaves the choice
    /// between the light and the dark palette a coin toss, and losing it
    /// means white text on white. These sit in the middle instead:
    /// against black and against white both, every one of them has enough
    /// contrast to read. Duller than either tuned palette, and that is the
    /// trade — legible everywhere beats handsome half the time.
    pub fn adaptive(token: Token) -> Rgb {
        use Token as T;
        match token {
            T::Key | T::Tag => rgb(0x4A90D9),
            T::Keyword => rgb(0x8B5CC7),
            T::Str => rgb(0x3F9E52),
            T::Number => rgb(0xC07830),
            T::Comment => rgb(0x8A8F98),
            T::Escape => rgb(0xC08A2E),
            T::Punctuation => rgb(0x8A8F98),
            T::Plain => rgb(0x9198A2),
        }
    }

    /// The colour for a container's summary: there is more inside, which
    /// is worth saying quietly rather than in the colour a value gets.
    pub fn placeholder(self, dark: bool) -> Rgb {
        self.token(Token::Punctuation, dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scheme_is_named_and_found_again() {
        for scheme in Scheme::ALL {
            let found = Scheme::from_name(scheme.name());
            assert_eq!(found, Some(*scheme), "{}", scheme.name());
        }
        // The spelling people will actually type.
        assert_eq!(Scheme::from_name("gruvbox-dark"), Some(Scheme::GruvboxDark));
        assert_eq!(Scheme::from_name("Gruvbox Dark"), Some(Scheme::GruvboxDark));
        assert_eq!(Scheme::from_name("nope"), None);
    }

    /// A scheme that names a ground and then draws text the same colour
    /// as that ground would be unreadable. Cheap to check, and the kind
    /// of thing a transcription error produces.
    #[test]
    fn no_scheme_is_invisible_on_its_own_ground() {
        for scheme in Scheme::ALL {
            let dark = !matches!(scheme.ground(), Some(Ground::Light));
            // Against the scheme's own surface where it has one, since
            // that is what it will actually be drawn on.
            let ground: i32 = match scheme.background() {
                Some((r, g, b)) => (r as i32 + g as i32 + b as i32) / 3,
                None if dark => 0x16,
                None => 0xFD,
            };
            for token in [
                Token::Key,
                Token::Tag,
                Token::Str,
                Token::Number,
                Token::Keyword,
                Token::Comment,
                Token::Escape,
                Token::Punctuation,
                Token::Plain,
            ] {
                let (r, g, b) = scheme.token(token, dark);
                let lightness = (r as i32 + g as i32 + b as i32) / 3;
                assert!(
                    (lightness - ground).abs() > 40,
                    "{} draws {token:?} at {lightness} against {ground}",
                    scheme.name()
                );
            }
        }
    }
}
