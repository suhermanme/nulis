//! Colors for printed pages.

use crate::PrintError;
use nulis_syntax::SyntaxToken;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Most theme colors accepted in one request.
const MAX_THEME_COLORS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Self = Self(0, 0, 0);

    /// Parses `#RRGGBB` (an alpha suffix `#RRGGBBAA` is ignored).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
        Some(Self(channel(0)?, channel(2)?, channel(4)?))
    }

    /// Channels from 0 to 1, as PDF color operators take them.
    #[must_use]
    pub fn unit(self) -> [f32; 3] {
        [self.0, self.1, self.2].map(|channel| f32::from(channel) / 255.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenStyle {
    pub color: Rgb,
    pub bold: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    /// Page background; `None` leaves the paper white.
    pub background: Option<Rgb>,
    pub text: Rgb,
    pub line_number: Rgb,
    /// Header and footer text.
    pub band: Rgb,
    /// Control-character pictures and the cut marker.
    pub symbol: Rgb,
    tokens: [TokenStyle; TOKEN_COUNT],
}

const TOKEN_COUNT: usize = 9;

pub(crate) fn token_index(token: SyntaxToken) -> usize {
    match token {
        SyntaxToken::Comment => 0,
        SyntaxToken::Keyword => 1,
        SyntaxToken::String => 2,
        SyntaxToken::Number => 3,
        SyntaxToken::Function => 4,
        SyntaxToken::Type => 5,
        SyntaxToken::Property => 6,
        SyntaxToken::Escape => 7,
        SyntaxToken::Invalid => 8,
    }
}

/// Theme color keys for each token, in [`token_index`] order.
const TOKEN_KEYS: [&str; TOKEN_COUNT] = [
    "syntax.comment",
    "syntax.keyword",
    "syntax.string",
    "syntax.number",
    "syntax.function",
    "syntax.type",
    "syntax.property",
    "syntax.escape",
    "syntax.invalid",
];

const fn plain(color: Rgb) -> TokenStyle {
    TokenStyle { color, bold: false }
}

impl Palette {
    #[must_use]
    pub fn token(&self, token: SyntaxToken) -> TokenStyle {
        self.tokens[token_index(token)]
    }

    /// Every token's style, in `SyntaxToken` order.
    #[must_use]
    pub fn token_styles(&self) -> &[TokenStyle] {
        &self.tokens
    }

    /// Dark text on white, with token colors that stay apart in grayscale.
    /// Based on the Nulis · Paper Current theme, darkened for paper.
    #[must_use]
    pub fn print() -> Self {
        Self {
            background: None,
            text: Rgb(0x1F, 0x23, 0x28),
            line_number: Rgb(0x7B, 0x85, 0x8C),
            band: Rgb(0x57, 0x60, 0x6A),
            symbol: Rgb(0x8C, 0x95, 0x9F),
            tokens: [
                plain(Rgb(0x5C, 0x73, 0x69)),
                TokenStyle {
                    color: Rgb(0x9A, 0x3B, 0x2B),
                    bold: true,
                },
                plain(Rgb(0x2F, 0x6B, 0x48)),
                plain(Rgb(0x6A, 0x47, 0x9A)),
                plain(Rgb(0x0E, 0x5F, 0x7D)),
                plain(Rgb(0x75, 0x56, 0x10)),
                plain(Rgb(0x0E, 0x5F, 0x7D)),
                plain(Rgb(0x9A, 0x3B, 0x2B)),
                TokenStyle {
                    color: Rgb(0xB4, 0x23, 0x18),
                    bold: true,
                },
            ],
        }
    }

    /// Black text only; keywords bold and comments gray.
    #[must_use]
    pub fn black_and_white() -> Self {
        let gray = Rgb(0x59, 0x59, 0x59);
        let mut tokens = [plain(Rgb::BLACK); TOKEN_COUNT];
        tokens[token_index(SyntaxToken::Comment)] = plain(gray);
        tokens[token_index(SyntaxToken::Keyword)].bold = true;
        tokens[token_index(SyntaxToken::Invalid)].bold = true;
        Self {
            background: None,
            text: Rgb::BLACK,
            line_number: Rgb(0x80, 0x80, 0x80),
            band: gray,
            symbol: Rgb(0x80, 0x80, 0x80),
            tokens,
        }
    }

    /// The editor theme's own colors, background included. Missing or
    /// unreadable keys fall back to the print palette.
    #[must_use]
    pub fn from_theme(colors: &BTreeMap<String, String>) -> Self {
        let fallback = Self::print();
        let color = |key: &str, default: Rgb| {
            colors
                .get(key)
                .and_then(|value| Rgb::parse(value))
                .unwrap_or(default)
        };
        let mut tokens = fallback.tokens;
        for (style, key) in tokens.iter_mut().zip(TOKEN_KEYS) {
            *style = plain(color(key, style.color));
        }
        let text = color("editor.foreground", fallback.text);
        Self {
            background: colors
                .get("editor.background")
                .and_then(|value| Rgb::parse(value)),
            text,
            line_number: color("editor.lineNumber", fallback.line_number),
            band: color("editor.lineNumber", fallback.band),
            symbol: color("editor.lineNumber", fallback.symbol),
            tokens,
        }
    }
}

/// Which colors to print with, as the desktop sends it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ColorChoice {
    #[default]
    Print,
    BlackAndWhite,
    /// The current editor theme's colors, keyed like a theme file.
    Theme {
        colors: BTreeMap<String, String>,
    },
}

impl ColorChoice {
    /// # Errors
    /// Returns [`PrintError::Setup`] for too many theme colors.
    pub fn palette(&self) -> Result<Palette, PrintError> {
        Ok(match self {
            Self::Print => Palette::print(),
            Self::BlackAndWhite => Palette::black_and_white(),
            Self::Theme { colors } => {
                if colors.len() > MAX_THEME_COLORS {
                    return Err(PrintError::Setup(format!(
                        "at most {MAX_THEME_COLORS} theme colors"
                    )));
                }
                Palette::from_theme(colors)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorChoice, Palette, Rgb};
    use nulis_syntax::SyntaxToken;
    use std::collections::BTreeMap;

    #[test]
    fn colors_parse_from_theme_hex() {
        assert_eq!(Rgb::parse("#0C708C"), Some(Rgb(0x0C, 0x70, 0x8C)));
        assert_eq!(Rgb::parse("#0C708CFF"), Some(Rgb(0x0C, 0x70, 0x8C)));
        for bad in ["0C708C", "#0C708", "#GGGGGG", "#ÄÄÄÄÄÄ"] {
            assert_eq!(Rgb::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn black_and_white_has_no_color_and_marks_keywords_bold() {
        let palette = Palette::black_and_white();
        for token in [
            SyntaxToken::Keyword,
            SyntaxToken::String,
            SyntaxToken::Number,
        ] {
            let style = palette.token(token);
            assert!(style.color.0 == style.color.1 && style.color.1 == style.color.2);
        }
        assert!(palette.token(SyntaxToken::Keyword).bold);
    }

    #[test]
    fn theme_colors_replace_the_print_palette_where_given() {
        let colors = BTreeMap::from([
            ("editor.background".to_owned(), "#24292E".to_owned()),
            ("editor.foreground".to_owned(), "#E1E4E8".to_owned()),
            ("syntax.keyword".to_owned(), "#F97583".to_owned()),
            ("syntax.string".to_owned(), "not a color".to_owned()),
        ]);
        let palette = ColorChoice::Theme { colors }.palette().expect("palette");
        assert_eq!(palette.background, Some(Rgb(0x24, 0x29, 0x2E)));
        assert_eq!(
            palette.token(SyntaxToken::Keyword).color,
            Rgb(0xF9, 0x75, 0x83)
        );
        assert_eq!(
            palette.token(SyntaxToken::String),
            Palette::print().token(SyntaxToken::String)
        );
        assert_eq!(Palette::print().background, None);
    }
}
