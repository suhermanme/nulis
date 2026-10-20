//! Contrast floors for the colors people must be able to see: the text
//! selection, secondary text, and toolbar icons. A theme that falls below a
//! floor is strengthened when it is loaded; its own colors are kept where
//! they already work, and a stronger color keeps the theme's hue.

use crate::Theme;

/// Selection against the editor background.
pub const MIN_SELECTION_CONTRAST: f64 = 1.6;
/// Selection against the current-line highlight, so the two stay apart.
pub const MIN_SELECTION_LINE_CONTRAST: f64 = 1.25;
/// Text drawn on the selection stays readable (WCAG AA for text). A theme
/// whose text has less than 1.6 × 4.5 against its background cannot have
/// both; there the selection floor wins and text keeps all it can.
pub const MIN_SELECTED_TEXT_CONTRAST: f64 = 4.5;
/// Secondary text against the toolbar, panels, and window (WCAG AA for text).
pub const MIN_MUTED_TEXT_CONTRAST: f64 = 4.5;
/// Icons against the toolbar and panels (WCAG non-text contrast).
pub const MIN_ICON_CONTRAST: f64 = 3.0;
/// A disabled icon is still visible, and clearly weaker than an enabled one.
pub const MIN_DISABLED_ICON_CONTRAST: f64 = 1.8;

/// A color as red, green, and blue from 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub f64, pub f64, pub f64);

impl Rgb {
    /// Reads `#RRGGBB`, or `#RRGGBBAA` drawn over `under`.
    #[must_use]
    pub fn parse(color: &str, under: Option<Rgb>) -> Option<Self> {
        let hex = color.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let channel =
            |start: usize| f64::from(u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0));
        let color = Self(channel(0), channel(2), channel(4));
        Some(match (hex.len(), under) {
            (8, Some(under)) => under.mix(color, channel(6) / 255.0),
            _ => color,
        })
    }

    /// `self` moved toward `other` by `amount` (0 keeps `self`, 1 is `other`).
    #[must_use]
    pub fn mix(self, other: Self, amount: f64) -> Self {
        let blend = |from: f64, to: f64| from + (to - from) * amount.clamp(0.0, 1.0);
        Self(
            blend(self.0, other.0),
            blend(self.1, other.1),
            blend(self.2, other.2),
        )
    }

    /// WCAG relative luminance.
    #[must_use]
    pub fn luminance(self) -> f64 {
        let linear = |channel: f64| {
            let value = channel / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }

    /// WCAG contrast ratio, from 1 to 21.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (light, dark) = {
            let (left, right) = (self.luminance(), other.luminance());
            if left > right {
                (left, right)
            } else {
                (right, left)
            }
        };
        (light + 0.05) / (dark + 0.05)
    }

    #[must_use]
    pub fn hex(self) -> String {
        // Rounded and clamped to 0..=255 first, so the conversion is exact.
        let byte = |channel: f64| {
            (0_u8..=255)
                .min_by(|left, right| {
                    (f64::from(*left) - channel)
                        .abs()
                        .total_cmp(&(f64::from(*right) - channel).abs())
                })
                .unwrap_or(0)
        };
        format!(
            "#{:02X}{:02X}{:02X}",
            byte(self.0),
            byte(self.1),
            byte(self.2)
        )
    }
}

/// The first of `start` moved toward `toward` in small steps that reaches
/// `min` against every color in `against`. When even `toward` is not enough,
/// it goes on toward black or white, whichever stands out from `against`.
fn strengthen(start: Rgb, toward: Rgb, against: &[Rgb], min: f64) -> Rgb {
    let fits = |color: &Rgb| against.iter().all(|other| color.contrast(*other) >= min);
    let steps = |from: Rgb, to: Rgb| (0..=20).map(move |step| from.mix(to, f64::from(step) / 20.0));
    let count = f64::from(u32::try_from(against.len().max(1)).unwrap_or(u32::MAX));
    let average = against.iter().map(|color| color.luminance()).sum::<f64>() / count;
    let dark_ground = average < 0.18;
    let extreme = if dark_ground {
        Rgb(255.0, 255.0, 255.0)
    } else {
        Rgb(0.0, 0.0, 0.0)
    };
    steps(start, toward)
        .chain(steps(toward, extreme))
        .find(fits)
        .unwrap_or(extreme)
}

/// A selection that stands out from the background and the current line
/// and keeps text on it readable. Tries the theme's own selection first,
/// then its accent blended into the background, weakest first.
fn readable_selection(selection: Rgb, background: Rgb, line: Rgb, text: Rgb, accent: Rgb) -> Rgb {
    let text_floor =
        MIN_SELECTED_TEXT_CONTRAST.min(text.contrast(background) / MIN_SELECTION_CONTRAST);
    let fits = |color: Rgb| {
        color.contrast(background) >= MIN_SELECTION_CONTRAST
            && color.contrast(line) >= MIN_SELECTION_LINE_CONTRAST
            && color.contrast(text) >= text_floor - 1e-9
    };
    if fits(selection) {
        return selection;
    }
    let blends = (1..=200).map(|step| background.mix(accent, f64::from(step) / 200.0));
    // Toward the text color as well, for an accent too close to the background.
    let toward_text = (1..=200).map(|step| background.mix(text, f64::from(step) / 200.0));
    let candidates: Vec<Rgb> = blends.chain(toward_text).collect();
    if let Some(color) = candidates.iter().copied().find(|color| fits(*color)) {
        return color;
    }
    // Nothing meets every floor (a current line close to the selection's
    // only fitting shades): stand out from the background as far as text allows.
    candidates
        .into_iter()
        .filter(|color| color.contrast(text) >= text_floor - 1e-9)
        .max_by(|left, right| {
            left.contrast(background)
                .total_cmp(&right.contrast(background))
        })
        .unwrap_or(selection)
}

impl Theme {
    /// Strengthens the selection, secondary text, and icon colors where they
    /// fall below the contrast floors in this module, and fills in
    /// `ui.icon` and `ui.iconDisabled` when the theme has none. Colors that
    /// already meet the floors are left exactly as they are.
    pub fn ensure_readable(&mut self) {
        let get = |colors: &std::collections::BTreeMap<String, String>,
                   token: &str,
                   under: Option<Rgb>| {
            colors.get(token).and_then(|color| Rgb::parse(color, under))
        };
        let colors = &self.colors;
        let Some(editor) = get(colors, "editor.background", None) else {
            return;
        };
        let window = get(colors, "ui.background", None).unwrap_or(editor);
        let parse = |token: &str| get(colors, token, Some(window));
        let (Some(text), Some(editor_text), Some(accent)) = (
            parse("ui.text"),
            get(colors, "editor.foreground", Some(editor)),
            parse("ui.accent"),
        ) else {
            return;
        };
        let line = get(colors, "editor.currentLine", Some(editor)).unwrap_or(editor);
        let selection = get(colors, "editor.selection", Some(editor)).unwrap_or(line);
        let surfaces: Vec<Rgb> = ["ui.toolbar", "ui.panel", "ui.background"]
            .iter()
            .filter_map(|token| parse(token))
            .collect();
        let muted = parse("ui.mutedText").unwrap_or(text);

        let mut changes = Vec::new();
        let readable = readable_selection(selection, editor, line, editor_text, accent);
        if readable != selection {
            changes.push(("editor.selection", readable.hex()));
        }
        let strong_muted = strengthen(muted, text, &surfaces, MIN_MUTED_TEXT_CONTRAST);
        if strong_muted != muted {
            changes.push(("ui.mutedText", strong_muted.hex()));
        }
        // Icons sit a little above secondary text, and always meet their own floor.
        let icon = match parse("ui.icon") {
            Some(icon) => strengthen(icon, text, &surfaces, MIN_ICON_CONTRAST),
            None => strengthen(
                strong_muted.mix(text, 0.3),
                text,
                &surfaces,
                MIN_ICON_CONTRAST,
            ),
        };
        if parse("ui.icon") != Some(icon) {
            changes.push(("ui.icon", icon.hex()));
        }
        let toolbar = parse("ui.toolbar").unwrap_or(window);
        let disabled = match parse("ui.iconDisabled") {
            Some(disabled) => strengthen(disabled, icon, &surfaces, MIN_DISABLED_ICON_CONTRAST),
            None => strengthen(
                toolbar.mix(icon, 0.35),
                icon,
                &surfaces,
                MIN_DISABLED_ICON_CONTRAST,
            ),
        };
        if parse("ui.iconDisabled") != Some(disabled) {
            changes.push(("ui.iconDisabled", disabled.hex()));
        }
        for (token, color) in changes {
            self.colors.insert(token.to_owned(), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Rgb, MIN_DISABLED_ICON_CONTRAST, MIN_ICON_CONTRAST, MIN_MUTED_TEXT_CONTRAST,
        MIN_SELECTED_TEXT_CONTRAST, MIN_SELECTION_CONTRAST, MIN_SELECTION_LINE_CONTRAST,
    };
    use crate::{Theme, ThemeCatalog, ThemeKind, THEME_TOKENS};

    /// Hex colors round each channel, so a ratio may land a hair under its target.
    const ROUNDING: f64 = 0.03;

    fn color(theme: &Theme, token: &str) -> Rgb {
        Rgb::parse(&theme.colors[token], None).unwrap_or_else(|| panic!("{} {token}", theme.name))
    }

    #[test]
    fn every_bundled_theme_meets_the_contrast_floors() {
        let catalog = ThemeCatalog::new(std::env::temp_dir().join("nulis-no-user-themes"));
        let themes = catalog.list().expect("themes");
        assert_eq!(themes.len(), 99);
        for record in themes {
            let theme = &record.theme;
            let name = &theme.name;
            let editor = color(theme, "editor.background");
            let selection = color(theme, "editor.selection");
            let text = color(theme, "editor.foreground");
            assert!(
                selection.contrast(editor) >= MIN_SELECTION_CONTRAST - ROUNDING,
                "{name}: selection"
            );
            // Text on the selection keeps 4.5:1, or all its own contrast allows next to a 1.6:1 selection.
            let text_floor =
                MIN_SELECTED_TEXT_CONTRAST.min(text.contrast(editor) / MIN_SELECTION_CONTRAST);
            assert!(
                selection.contrast(text) >= text_floor - ROUNDING,
                "{name}: text on selection"
            );
            // Kept apart from the current line as far as the two floors above allow.
            assert!(
                selection.contrast(color(theme, "editor.currentLine"))
                    >= MIN_SELECTION_LINE_CONTRAST - 0.1,
                "{name}: selection and current line"
            );
            for surface in ["ui.toolbar", "ui.panel", "ui.background"] {
                let surface = color(theme, surface);
                assert!(
                    color(theme, "ui.mutedText").contrast(surface)
                        >= MIN_MUTED_TEXT_CONTRAST - ROUNDING,
                    "{name}: muted"
                );
                assert!(
                    color(theme, "ui.icon").contrast(surface) >= MIN_ICON_CONTRAST - ROUNDING,
                    "{name}: icon"
                );
                assert!(
                    color(theme, "ui.iconDisabled").contrast(surface)
                        >= MIN_DISABLED_ICON_CONTRAST - ROUNDING,
                    "{name}: disabled icon"
                );
            }
        }
    }

    #[test]
    fn a_faint_installed_theme_is_strengthened_but_its_file_is_kept() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let mut theme = Theme {
            schema: None,
            id: "example.faint".into(),
            name: "Faint".into(),
            version: 1,
            kind: ThemeKind::Dark,
            colors: THEME_TOKENS
                .iter()
                .map(|token| ((*token).into(), "#202020".into()))
                .collect(),
        };
        for (token, value) in [
            ("ui.text", "#E0E0E0"),
            ("editor.foreground", "#E0E0E0"),
            ("ui.accent", "#4A90D9"),
            ("editor.selection", "#252525"),
            ("ui.mutedText", "#303030"),
            ("ui.overlay", "#202020AA"),
        ] {
            theme.colors.insert(token.into(), value.into());
        }
        let source = directory.path().join("faint.nulis-theme.json");
        std::fs::write(&source, serde_json::to_vec(&theme).expect("serialize")).expect("write");
        let catalog = ThemeCatalog::new(directory.path().join("themes"));
        let installed = catalog.install(&source).expect("install").theme;
        let editor = color(&installed, "editor.background");
        assert!(
            color(&installed, "editor.selection").contrast(editor)
                >= MIN_SELECTION_CONTRAST - ROUNDING
        );
        assert!(
            color(&installed, "ui.mutedText").contrast(editor)
                >= MIN_MUTED_TEXT_CONTRAST - ROUNDING
        );
        assert!(installed.colors.contains_key("ui.icon"));
        // What is stored is what the author wrote.
        let stored = std::fs::read_to_string(
            directory
                .path()
                .join("themes/example.faint.nulis-theme.json"),
        )
        .expect("stored");
        assert!(stored.contains("#252525") && !stored.contains("ui.icon"));
        // A theme that already reads well keeps its colors.
        let ember = catalog
            .list()
            .expect("list")
            .into_iter()
            .find(|record| record.theme.id == "nulis.ember-dark")
            .expect("ember");
        assert_eq!(ember.theme.colors["editor.selection"], "#39566B");
        assert_eq!(ember.theme.colors["ui.icon"], "#969DA7");
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        let black = Rgb::parse("#000000", None).expect("black");
        let white = Rgb::parse("#FFFFFF", None).expect("white");
        assert!((black.contrast(white) - 21.0).abs() < 1e-9);
        let grey = Rgb::parse("#767676", None).expect("grey");
        assert!((grey.contrast(white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn translucent_colors_are_measured_over_what_is_under_them() {
        let white = Rgb::parse("#FFFFFF", None).expect("white");
        let half = Rgb::parse("#00000080", Some(white)).expect("half black");
        assert_eq!(half.hex(), "#7F7F7F");
    }
}
