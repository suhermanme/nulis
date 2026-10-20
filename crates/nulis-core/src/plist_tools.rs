//! Property list (XML plist) Validate. Beautify is the XML formatter, which
//! changes only the whitespace between elements.
//!
//! Well-formed XML is checked first; then the plist structure on the same
//! tokens, so every problem has a position: one value inside `<plist>`, a
//! `<key>` before each value in a `<dict>` with no key twice, only plist
//! elements, and values that read as what their element says.

use crate::format_tools::{tokenize_xml, ToolError, XmlKind, XmlToken};
use base64::Engine;
use std::collections::HashSet;

const VALUE_ELEMENTS: &[&str] = &[
    "dict", "array", "string", "integer", "real", "date", "data", "true", "false",
];

fn fail<T>(message: impl Into<String>, offset: usize) -> Result<T, ToolError> {
    Err(ToolError {
        message: message.into(),
        offset,
    })
}

struct Walker<'a> {
    text: &'a str,
    /// Elements and text that matter; comments, instructions, and blank text are left out.
    tokens: Vec<XmlToken<'a>>,
    index: usize,
}

impl<'a> Walker<'a> {
    fn new(text: &'a str) -> Result<Self, ToolError> {
        let tokens = tokenize_xml(text)?
            .into_iter()
            .filter(|token| match token.kind {
                XmlKind::Comment | XmlKind::Instruction | XmlKind::Doctype => false,
                XmlKind::Text => !text[token.start..token.end].trim().is_empty(),
                _ => true,
            })
            .collect();
        Ok(Self {
            text,
            tokens,
            index: 0,
        })
    }

    fn peek(&self) -> Option<XmlToken<'a>> {
        self.tokens.get(self.index).copied()
    }

    fn next(&mut self) -> Option<XmlToken<'a>> {
        let token = self.peek();
        self.index += 1;
        token
    }

    fn end(&self) -> usize {
        self.text.len()
    }

    /// One value, starting at the next token.
    fn value(&mut self) -> Result<(), ToolError> {
        let Some(token) = self.next() else {
            return fail("a value is missing here", self.end());
        };
        match token.kind {
            XmlKind::Open | XmlKind::SelfClosing => {}
            XmlKind::Text | XmlKind::CData => {
                return fail(
                    "text must be inside a value element such as <string>",
                    token.start,
                )
            }
            _ => {
                return fail(
                    format!("a value is expected, not </{}>", token.name),
                    token.start,
                )
            }
        }
        if token.name == "key" {
            return fail(
                "a <key> belongs inside a <dict>, before a value",
                token.start,
            );
        }
        if !VALUE_ELEMENTS.contains(&token.name) {
            return fail(
                format!("<{}> is not a property list element", token.name),
                token.start,
            );
        }
        if token.kind == XmlKind::SelfClosing {
            return match token.name {
                "integer" | "real" | "date" => fail(
                    format!("<{0}/> is empty; it needs a value", token.name),
                    token.start,
                ),
                _ => Ok(()),
            };
        }
        match token.name {
            "dict" => self.dict(),
            "array" => {
                while self.peek().is_some_and(|next| next.kind != XmlKind::Close) {
                    self.value()?;
                }
                self.close("array")
            }
            "true" | "false" => {
                if let Some(next) = self.peek().filter(|next| next.kind != XmlKind::Close) {
                    return fail(format!("<{}/> must be empty", token.name), next.start);
                }
                self.close(token.name)
            }
            name => {
                let (content, at) = self.content(name)?;
                check_scalar(name, &content, at)
            }
        }
    }

    /// The end tag of `name`, next.
    fn close(&mut self, name: &str) -> Result<(), ToolError> {
        match self.next() {
            Some(token) if token.kind == XmlKind::Close && token.name == name => Ok(()),
            Some(token) => fail(format!("</{name}> is expected here"), token.start),
            None => fail(format!("</{name}> is missing"), self.end()),
        }
    }

    /// The text of a leaf element up to its end tag, and where it starts.
    fn content(&mut self, name: &str) -> Result<(String, usize), ToolError> {
        let mut content = String::new();
        let mut at = self.peek().map_or(self.end(), |token| token.start);
        let mut first = true;
        while let Some(token) = self.peek() {
            match token.kind {
                XmlKind::Text => content.push_str(&self.text[token.start..token.end]),
                XmlKind::CData => content.push_str(
                    self.text[token.start..token.end]
                        .trim_start_matches("<![CDATA[")
                        .trim_end_matches("]]>"),
                ),
                XmlKind::Close => break,
                _ => {
                    return fail(
                        format!("<{name}> holds text, not other elements"),
                        token.start,
                    )
                }
            }
            if first {
                at = token.start;
                first = false;
            }
            self.index += 1;
        }
        self.close(name)?;
        Ok((content, at))
    }

    fn dict(&mut self) -> Result<(), ToolError> {
        let mut keys = HashSet::new();
        while let Some(token) = self.peek() {
            if token.kind == XmlKind::Close {
                break;
            }
            if token.name != "key" || !matches!(token.kind, XmlKind::Open | XmlKind::SelfClosing) {
                return fail(
                    "each value in a <dict> needs a <key> before it",
                    token.start,
                );
            }
            self.index += 1;
            let key = if token.kind == XmlKind::SelfClosing {
                String::new()
            } else {
                self.content("key")?.0
            };
            if !keys.insert(key.clone()) {
                return fail(
                    format!("the key \"{key}\" appears twice in this <dict>"),
                    token.start,
                );
            }
            match self.peek() {
                Some(next) if next.kind == XmlKind::Close => {
                    return fail(format!("the key \"{key}\" has no value"), token.start)
                }
                None => return fail(format!("the key \"{key}\" has no value"), token.start),
                _ => self.value()?,
            }
        }
        self.close("dict")
    }
}

/// Whether a leaf element's text reads as its type.
fn check_scalar(name: &str, content: &str, at: usize) -> Result<(), ToolError> {
    let value = content.trim();
    let ok = match name {
        "integer" => {
            let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
            let negative = value.starts_with('-');
            let parsed = match unsigned
                .strip_prefix("0x")
                .or_else(|| unsigned.strip_prefix("0X"))
            {
                Some(hex) => u64::from_str_radix(hex, 16).ok(),
                None => unsigned
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
                    .then(|| unsigned.parse::<u64>().ok())
                    .flatten(),
            };
            // From -2^63 up to 2^64 - 1, as property lists store integers.
            parsed.is_some_and(|number| !negative || number <= 1 << 63)
        }
        "real" => {
            let lower = value.to_ascii_lowercase();
            matches!(
                lower.trim_start_matches(['-', '+']),
                "nan" | "inf" | "infinity"
            ) || (!value.is_empty() && value.parse::<f64>().is_ok())
        }
        "date" => is_plist_date(value),
        "data" => {
            let compact: String = value
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            base64::engine::general_purpose::STANDARD
                .decode(compact)
                .is_ok()
        }
        _ => true,
    };
    if ok {
        return Ok(());
    }
    let expected = match name {
        "integer" => "a whole number, such as 42, -7, or 0x1F, that fits in 64 bits",
        "real" => "a number, such as 3.14 or 1e-5",
        "date" => "a date such as 2026-10-20T09:14:02Z",
        _ => "Base64 text",
    };
    fail(
        format!("<{name}> must hold {expected}, not \"{value}\""),
        at,
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ`, the form property lists store.
fn is_plist_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return false;
    }
    let number = |range: std::ops::Range<usize>| {
        let digits = &value[range];
        digits
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| digits.parse::<u32>().ok())
            .flatten()
    };
    matches!(
        (
            number(5..7),
            number(8..10),
            number(11..13),
            number(14..16),
            number(17..19),
            number(0..4)
        ),
        (
            Some(1..=12),
            Some(1..=31),
            Some(0..=23),
            Some(0..=59),
            Some(0..=60),
            Some(_)
        )
    )
}

/// Checks that the text is a well-formed XML property list.
pub(crate) fn validate_plist(text: &str) -> Result<(), ToolError> {
    let mut walker = Walker::new(text)?;
    let Some(root) = walker.next() else {
        return fail("the document is empty", 0);
    };
    if root.name != "plist" || !matches!(root.kind, XmlKind::Open | XmlKind::SelfClosing) {
        return fail("a property list starts with <plist>", root.start);
    }
    let tag = &text[root.start..root.end];
    if let Some(version) = tag.split("version=").nth(1) {
        let version = version.trim_start_matches(['"', '\'']);
        if !version.starts_with("1.0") {
            return fail("<plist> must be version 1.0", root.start);
        }
    }
    if root.kind == XmlKind::SelfClosing {
        return fail("<plist> must hold one value", root.start);
    }
    walker.value()?;
    match walker.peek() {
        Some(token) if token.kind != XmlKind::Close => fail(
            "<plist> holds exactly one value; wrap several in <dict> or <array>",
            token.start,
        ),
        _ => walker.close("plist"),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_plist;

    const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>com.nulis.editor</string>
	<!-- shown in Finder -->
	<key>CFBundleShortVersionString</key>
	<string>0.2.1</string>
	<key>LSMinimumSystemVersion</key>
	<string>10.15</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>Count</key>
	<integer>-42</integer>
	<key>Ratio</key>
	<real>0.75</real>
	<key>Built</key>
	<date>2026-10-20T09:14:02Z</date>
	<key>Icon</key>
	<data>
	SGVsbG8s
	IHdvcmxk
	</data>
	<key>Types</key>
	<array>
		<string>public.plain-text</string>
		<dict/>
		<string/>
	</array>
</dict>
</plist>
"#;

    fn problem(text: &str) -> (String, String) {
        let error = validate_plist(text).expect_err("invalid");
        let at = text[error.offset..].chars().take(12).collect();
        (error.message, at)
    }

    #[test]
    fn a_real_info_plist_is_valid() {
        validate_plist(INFO_PLIST).expect("valid");
    }

    #[test]
    fn each_rule_names_the_problem_where_it_is() {
        let wrap = |body: &str| format!("<plist version=\"1.0\"><dict>{body}</dict></plist>");
        let cases = [
            (wrap("<string>x</string>"), "needs a <key>", "<string>x</s"),
            (
                wrap("<key>a</key><string>1</string><key>a</key><string>2</string>"),
                "\"a\" appears twice",
                "<key>a</key>",
            ),
            (wrap("<key>a</key>"), "\"a\" has no value", "<key>a</key>"),
            (
                wrap("<key>n</key><integer>twelve</integer>"),
                "<integer> must hold",
                "twelve</inte",
            ),
            (
                wrap("<key>n</key><integer>99999999999999999999</integer>"),
                "fits in 64 bits",
                "999999999999",
            ),
            (
                wrap("<key>r</key><real>1,5</real>"),
                "<real> must hold",
                "1,5</real></",
            ),
            (
                wrap("<key>d</key><date>2026-13-01T00:00:00Z</date>"),
                "<date> must hold",
                "2026-13-01T0",
            ),
            (
                wrap("<key>b</key><data>@@@</data>"),
                "Base64",
                "@@@</data></",
            ),
            (
                wrap("<key>t</key><true>yes</true>"),
                "<true/> must be empty",
                "yes</true></",
            ),
            (
                wrap("<key>x</key><color>red</color>"),
                "not a property list element",
                "<color>red</",
            ),
            (
                wrap("<key>x</key><string><b>bold</b></string>"),
                "holds text, not other elements",
                "<b>bold</b><",
            ),
        ];
        for (text, message, at) in cases {
            let (found, place) = problem(&text);
            assert!(found.contains(message), "{text}: {found}");
            assert_eq!(place, at, "{text}");
        }
        assert!(problem("<dict></dict>").0.contains("starts with <plist>"));
        assert!(problem("<plist version=\"2.0\"><dict/></plist>")
            .0
            .contains("version 1.0"));
        assert!(problem("<plist><dict/><dict/></plist>")
            .0
            .contains("exactly one value"));
        // Not well-formed XML is reported by the XML check.
        assert!(validate_plist("<plist><dict></plist>").is_err());
    }

    #[test]
    fn numbers_at_the_edges_of_their_range() {
        let value = |element: &str, text: &str| {
            validate_plist(&format!("<plist><{element}>{text}</{element}></plist>")).is_ok()
        };
        assert!(value("integer", "-9223372036854775808"));
        assert!(!value("integer", "-9223372036854775809"));
        assert!(value("integer", "18446744073709551615"));
        assert!(value("integer", "0x1F"));
        assert!(value("real", "-inf") && value("real", "nan") && value("real", "1e-5"));
    }
}
