//! Built-in JSON and XML tools: validate, beautify, and minify. YAML and
//! property lists have their own modules (`yaml_tools`, `plist_tools`).
//!
//! Both work on text only and never reorder, rename, or re-encode content:
//! JSON keeps its key order and number spelling, and XML keeps every tag,
//! attribute, comment, and text run byte for byte, changing only the
//! whitespace between elements.

const INDENT: &str = "  ";
/// Deeper levels share this indent, so output stays linear in the input size.
const MAX_INDENT_DEPTH: usize = 256;

/// Where and why a document is not valid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ToolError {
    pub message: String,
    /// Byte offset into the text the tool was given.
    pub offset: usize,
}

fn fail<T>(message: impl Into<String>, offset: usize) -> Result<T, ToolError> {
    Err(ToolError {
        message: message.into(),
        offset,
    })
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn skip_bom(text: &str) -> usize {
    if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    }
}

/// Checks that the text is one JSON value.
pub(crate) fn validate_json(text: &str) -> Result<(), ToolError> {
    let start = skip_bom(text);
    let body = &text[start..];
    if body.trim().is_empty() {
        return fail("the document is empty", start);
    }
    serde_json::from_str::<serde::de::IgnoredAny>(body)
        .map(|_| ())
        .map_err(|error| {
            // serde reports a one-based line and a byte column within it.
            let line_start: usize = body
                .split_inclusive('\n')
                .take(error.line().saturating_sub(1))
                .map(str::len)
                .sum();
            let offset = (line_start + error.column().saturating_sub(1)).min(body.len());
            let mut message = error.to_string();
            if let Some(position) = message.rfind(" at line ") {
                message.truncate(position);
            }
            ToolError {
                message,
                offset: start + offset,
            }
        })
}

/// Re-indents valid JSON, or removes all insignificant whitespace when
/// `minify` is set. Strings, numbers, and key order are copied unchanged.
pub(crate) fn format_json(text: &str, minify: bool) -> Result<String, ToolError> {
    validate_json(text)?;
    let newline = line_ending(text);
    let start = skip_bom(text);
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    out.push_str(&text[..start]);
    let mut depth = 0_usize;
    let mut index = start;
    let break_line = |out: &mut String, depth: usize| {
        if !minify {
            out.push_str(newline);
            for _ in 0..depth {
                out.push_str(INDENT);
            }
        }
    };
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' => {
                let mut end = index + 1;
                while end < bytes.len() && bytes[end] != b'"' {
                    end += 1 + usize::from(bytes[end] == b'\\');
                }
                let end = (end + 1).min(bytes.len());
                out.push_str(&text[index..end]);
                index = end;
                continue;
            }
            b'{' | b'[' => {
                let close = if byte == b'{' { b'}' } else { b']' };
                let next = bytes[index + 1..]
                    .iter()
                    .position(|next| !next.is_ascii_whitespace())
                    .map(|offset| index + 1 + offset);
                out.push(char::from(byte));
                if let Some(next) = next.filter(|next| bytes[*next] == close) {
                    // An empty container stays on one line.
                    out.push(char::from(close));
                    index = next + 1;
                    continue;
                }
                depth += 1;
                break_line(&mut out, depth);
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                break_line(&mut out, depth);
                out.push(char::from(byte));
            }
            b',' => {
                out.push(',');
                break_line(&mut out, depth);
            }
            b':' => out.push_str(if minify { ":" } else { ": " }),
            _ if byte.is_ascii_whitespace() => {}
            _ => {
                // A number or literal: copy it as written.
                let end = bytes[index..]
                    .iter()
                    .position(|next| {
                        next.is_ascii_whitespace() || matches!(next, b',' | b'}' | b']' | b':')
                    })
                    .map_or(bytes.len(), |offset| index + offset);
                out.push_str(&text[index..end]);
                index = end;
                continue;
            }
        }
        index += 1;
    }
    if text.ends_with('\n') {
        out.push_str(newline);
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum XmlKind {
    /// `<?target …?>`, including the XML declaration.
    Instruction,
    Comment,
    Doctype,
    CData,
    Text,
    Open,
    SelfClosing,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct XmlToken<'a> {
    pub(crate) kind: XmlKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Element name for tags.
    pub(crate) name: &'a str,
    /// `xml:space="preserve"` was set on this start tag.
    pub(crate) preserve: bool,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':') || byte >= 0x80
}

fn is_name_byte(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
}

fn name_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| !is_name_byte(*byte))
        .map_or(bytes.len(), |offset| from + offset)
}

fn find(text: &str, from: usize, needle: &str) -> Option<usize> {
    text.get(from..)?.find(needle).map(|offset| from + offset)
}

/// Every `&` must begin a named or numeric reference ending in `;`.
fn check_references(text: &str, start: usize, end: usize) -> Result<(), ToolError> {
    let bytes = text.as_bytes();
    let mut index = start;
    while let Some(offset) = bytes[index..end].iter().position(|byte| *byte == b'&') {
        let at = index + offset;
        let close = bytes[at..end]
            .iter()
            .position(|byte| *byte == b';')
            .map(|n| at + n);
        let body = close.map(|close| &text[at + 1..close]);
        let valid = body.is_some_and(|body| {
            if let Some(number) = body.strip_prefix("#x") {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_hexdigit())
            } else if let Some(number) = body.strip_prefix('#') {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            } else {
                body.as_bytes()
                    .first()
                    .is_some_and(|byte| is_name_start(*byte))
                    && body.bytes().all(is_name_byte)
            }
        });
        if !valid {
            return fail("'&' must start an entity such as &amp; or &#38;", at);
        }
        index = close.unwrap_or(at) + 1;
    }
    Ok(())
}

/// Reads one start, empty, or end tag beginning at `start`.
#[allow(clippy::too_many_lines)] // One pass over a tag; splitting it would hide the grammar.
fn read_tag(text: &str, start: usize) -> Result<XmlToken<'_>, ToolError> {
    let bytes = text.as_bytes();
    let closing = bytes.get(start + 1) == Some(&b'/');
    let name_start = start + 1 + usize::from(closing);
    if !bytes.get(name_start).copied().is_some_and(is_name_start) {
        return fail(
            "'<' must be followed by an element name; write &lt; for a literal '<'",
            start,
        );
    }
    let name_stop = name_end(bytes, name_start);
    let name = &text[name_start..name_stop];
    let mut index = name_stop;
    let mut seen: Vec<&str> = Vec::new();
    let mut preserve = false;
    loop {
        let before = index;
        while bytes
            .get(index)
            .copied()
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        match bytes.get(index).copied() {
            None => return fail(format!("the <{name}> tag is never closed with '>'"), start),
            Some(b'>') => {
                let kind = if closing {
                    XmlKind::Close
                } else {
                    XmlKind::Open
                };
                return Ok(XmlToken {
                    kind,
                    start,
                    end: index + 1,
                    name,
                    preserve,
                });
            }
            Some(b'/') if !closing && bytes.get(index + 1) == Some(&b'>') => {
                return Ok(XmlToken {
                    kind: XmlKind::SelfClosing,
                    start,
                    end: index + 2,
                    name,
                    preserve,
                });
            }
            Some(byte) if is_name_start(byte) && !closing => {
                if before == index {
                    return fail("attributes must be separated by whitespace", index);
                }
                let attribute_stop = name_end(bytes, index);
                let attribute = &text[index..attribute_stop];
                if seen.contains(&attribute) {
                    return fail(
                        format!("attribute '{attribute}' appears twice in <{name}>"),
                        index,
                    );
                }
                seen.push(attribute);
                let mut cursor = attribute_stop;
                while bytes
                    .get(cursor)
                    .copied()
                    .is_some_and(|byte| byte.is_ascii_whitespace())
                {
                    cursor += 1;
                }
                if bytes.get(cursor) != Some(&b'=') {
                    return fail(
                        format!("attribute '{attribute}' needs a quoted value"),
                        attribute_stop,
                    );
                }
                cursor += 1;
                while bytes
                    .get(cursor)
                    .copied()
                    .is_some_and(|byte| byte.is_ascii_whitespace())
                {
                    cursor += 1;
                }
                let Some(quote) = bytes
                    .get(cursor)
                    .copied()
                    .filter(|byte| matches!(byte, b'"' | b'\''))
                else {
                    return fail(
                        format!("the value of '{attribute}' must be in quotes"),
                        cursor.min(bytes.len()),
                    );
                };
                let Some(close) = bytes[cursor + 1..].iter().position(|byte| *byte == quote) else {
                    return fail(
                        format!("the value of '{attribute}' is missing its closing quote"),
                        cursor,
                    );
                };
                let value_end = cursor + 1 + close;
                if let Some(bad) = bytes[cursor + 1..value_end]
                    .iter()
                    .position(|byte| *byte == b'<')
                {
                    return fail(
                        "'<' is not allowed inside an attribute value; write &lt;",
                        cursor + 1 + bad,
                    );
                }
                check_references(text, cursor + 1, value_end)?;
                preserve |= attribute == "xml:space" && &text[cursor + 1..value_end] == "preserve";
                index = value_end + 1;
            }
            Some(_) => {
                let what = if closing {
                    "an end tag takes no attributes"
                } else {
                    "unexpected character in a tag"
                };
                return fail(what, index);
            }
        }
    }
}

/// Splits XML into tokens and checks that it is well formed: one root
/// element, matching and properly nested tags, quoted and unique attributes,
/// and complete references. DTDs and namespaces are not validated.
#[allow(clippy::too_many_lines)] // One pass over the document, one arm per construct.
pub(crate) fn tokenize_xml(text: &str) -> Result<Vec<XmlToken<'_>>, ToolError> {
    let bytes = text.as_bytes();
    let first = skip_bom(text);
    let token = |kind, start, end| XmlToken {
        kind,
        start,
        end,
        name: "",
        preserve: false,
    };
    let mut tokens = Vec::new();
    let mut open: Vec<XmlToken<'_>> = Vec::new();
    let mut roots = 0_usize;
    let mut index = first;
    while index < bytes.len() {
        if bytes[index] != b'<' {
            let end = bytes[index..]
                .iter()
                .position(|byte| *byte == b'<')
                .map_or(bytes.len(), |n| index + n);
            let blank = text[index..end].trim().is_empty();
            if open.is_empty() && !blank {
                let at = index + text[index..end].len() - text[index..end].trim_start().len();
                let message = if roots == 0 {
                    "text is not allowed before the root element"
                } else {
                    "text is not allowed after the root element"
                };
                return fail(message, at);
            }
            if let Some(bad) = find(text, index, "]]>").filter(|bad| *bad < end) {
                return fail("']]>' is not allowed in text", bad);
            }
            check_references(text, index, end)?;
            tokens.push(token(XmlKind::Text, index, end));
            index = end;
        } else if text[index..].starts_with("<!--") {
            let Some(close) = find(text, index + 4, "-->") else {
                return fail("this comment is never closed with '-->'", index);
            };
            if let Some(bad) = find(text, index + 4, "--").filter(|bad| *bad < close) {
                return fail("'--' is not allowed inside a comment", bad);
            }
            tokens.push(token(XmlKind::Comment, index, close + 3));
            index = close + 3;
        } else if text[index..].starts_with("<![CDATA[") {
            if open.is_empty() {
                return fail("a CDATA section is only allowed inside an element", index);
            }
            let Some(close) = find(text, index + 9, "]]>") else {
                return fail("this CDATA section is never closed with ']]>'", index);
            };
            tokens.push(token(XmlKind::CData, index, close + 3));
            index = close + 3;
        } else if text[index..].starts_with("<?") {
            let Some(close) = find(text, index + 2, "?>") else {
                return fail(
                    "this processing instruction is never closed with '?>'",
                    index,
                );
            };
            let target = &text[index + 2..name_end(bytes, index + 2)];
            if target.eq_ignore_ascii_case("xml") && index != first {
                return fail(
                    "the XML declaration must be the very first thing in the document",
                    index,
                );
            }
            if target.is_empty() {
                return fail("a processing instruction needs a target name", index);
            }
            tokens.push(token(XmlKind::Instruction, index, close + 2));
            index = close + 2;
        } else if text[index..].starts_with("<!") {
            if !text[index..].starts_with("<!DOCTYPE") {
                return fail("unknown markup declaration", index);
            }
            if roots > 0 || !open.is_empty() {
                return fail("the DOCTYPE must come before the root element", index);
            }
            // An internal subset may contain '>' inside its brackets.
            let mut depth = 0_i32;
            let end = bytes[index..].iter().position(|byte| {
                match byte {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                *byte == b'>' && depth <= 0
            });
            let Some(end) = end else {
                return fail("the DOCTYPE is never closed with '>'", index);
            };
            tokens.push(token(XmlKind::Doctype, index, index + end + 1));
            index += end + 1;
        } else {
            let tag = read_tag(text, index)?;
            if tag.kind == XmlKind::Close {
                match open.pop() {
                    Some(start) if start.name == tag.name => {}
                    Some(start) => {
                        return fail(
                            format!(
                                "expected </{}> to close the element opened earlier, but found </{}>",
                                start.name, tag.name
                            ),
                            tag.start,
                        );
                    }
                    None => {
                        return fail(
                            format!("</{}> has no matching start tag", tag.name),
                            tag.start,
                        )
                    }
                }
            } else {
                if open.is_empty() {
                    roots += 1;
                    if roots > 1 {
                        return fail("a document can have only one root element", tag.start);
                    }
                }
                if tag.kind == XmlKind::Open {
                    open.push(tag);
                }
            }
            index = tag.end;
            tokens.push(tag);
        }
    }
    if let Some(unclosed) = open.last() {
        return fail(
            format!("<{}> is never closed", unclosed.name),
            unclosed.start,
        );
    }
    if roots == 0 {
        return fail("the document has no root element", bytes.len().min(first));
    }
    Ok(tokens)
}

/// Checks that the text is well-formed XML.
pub(crate) fn validate_xml(text: &str) -> Result<(), ToolError> {
    tokenize_xml(text).map(|_| ())
}

/// Re-indents well-formed XML. Only whitespace between elements changes: an
/// element holding text, or marked `xml:space="preserve"`, is kept verbatim.
pub(crate) fn format_xml(text: &str) -> Result<String, ToolError> {
    let tokens = tokenize_xml(text)?;
    let newline = line_ending(text);
    // Index of the end tag matching each start tag.
    let mut partner = vec![0_usize; tokens.len()];
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            XmlKind::Open => stack.push(index),
            XmlKind::Close => partner[stack.pop().unwrap_or(index)] = index,
            _ => {}
        }
    }
    let blank = |token: &XmlToken<'_>| {
        token.kind == XmlKind::Text && text[token.start..token.end].trim().is_empty()
    };
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    out.push_str(&text[..skip_bom(text)]);
    let mut depth = 0_usize;
    let line = |out: &mut String, depth: usize, content: &str| {
        if out.len() > skip_bom(text) {
            out.push_str(newline);
        }
        for _ in 0..depth.min(MAX_INDENT_DEPTH) {
            out.push_str(INDENT);
        }
        out.push_str(content);
    };
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        match token.kind {
            XmlKind::Open => {
                let close = partner[index];
                // Text directly inside this element, skipping over child elements.
                let mut has_text = false;
                let mut has_children = false;
                let mut child = index + 1;
                while child < close {
                    match tokens[child].kind {
                        XmlKind::Text | XmlKind::CData => has_text |= !blank(&tokens[child]),
                        XmlKind::Open => {
                            has_children = true;
                            child = partner[child];
                        }
                        _ => has_children = true,
                    }
                    child += 1;
                }
                if token.preserve || has_text {
                    // Whitespace here may be meaningful: keep the element as written.
                    line(&mut out, depth, &text[token.start..tokens[close].end]);
                    index = close;
                } else if !has_children {
                    let mut joined = text[token.start..token.end].to_owned();
                    joined.push_str(&text[tokens[close].start..tokens[close].end]);
                    line(&mut out, depth, &joined);
                    index = close;
                } else {
                    line(&mut out, depth, &text[token.start..token.end]);
                    depth += 1;
                }
            }
            XmlKind::Close => {
                depth = depth.saturating_sub(1);
                line(&mut out, depth, &text[token.start..token.end]);
            }
            XmlKind::Text if blank(token) => {}
            _ => line(&mut out, depth, text[token.start..token.end].trim()),
        }
        index += 1;
    }
    if text.ends_with('\n') {
        out.push_str(newline);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{format_json, format_xml, validate_json, validate_xml};

    #[test]
    fn json_is_validated_with_a_position_and_formatted_without_changing_values() {
        assert!(validate_json("{\"a\": [1, 2, {\"b\": null}]}").is_ok());
        assert!(validate_json("\u{feff}[]\n").is_ok());
        let error = validate_json("{\n  \"a\": 1,\n  \"b\": }\n").expect_err("invalid");
        assert_eq!(error.message, "expected value");
        assert_eq!(
            &"{\n  \"a\": 1,\n  \"b\": }\n"[error.offset..=error.offset],
            "}"
        );
        assert_eq!(
            validate_json("  \n").expect_err("empty").message,
            "the document is empty"
        );
        assert!(validate_json("{\"a\": 1,}").is_err());
        assert!(validate_json("{} extra").is_err());

        // Key order, number spelling, escapes, and strings holding braces survive.
        let source =
            "{\"z\":1.50,\"a\":[true,null,{\"s\":\"x{,}:\\\"y\"}],\"e\":{},\"l\":[ ],\"n\":-1e9}";
        let pretty = format_json(source, false).expect("pretty");
        assert_eq!(
            pretty,
            "{\n  \"z\": 1.50,\n  \"a\": [\n    true,\n    null,\n    {\n      \"s\": \"x{,}:\\\"y\"\n    }\n  ],\n  \"e\": {},\n  \"l\": [],\n  \"n\": -1e9\n}"
        );
        assert_eq!(
            format_json(&pretty, true).expect("minify"),
            source.replace("[ ]", "[]")
        );
        // Formatting is stable, keeps CRLF and a final newline, and refuses invalid input.
        assert_eq!(format_json(&pretty, false).expect("again"), pretty);
        assert_eq!(
            format_json("{\"a\":1}\r\n", false).expect("crlf"),
            "{\r\n  \"a\": 1\r\n}\r\n"
        );
        assert!(format_json("{\"a\":", false).is_err());
        assert_eq!(
            format_json("\"just a string\"", false).expect("scalar"),
            "\"just a string\""
        );
    }

    #[test]
    fn xml_well_formedness_errors_name_the_problem_and_its_position() {
        assert!(validate_xml("<?xml version=\"1.0\"?>\n<!DOCTYPE a [<!ENTITY x \"y>\">]>\n<a b=\"1\" c='2'><!-- n --><b/>t &amp; &#38; &#x26;<![CDATA[<raw>]]></a>\n").is_ok());
        let cases: [(&str, &str, &str); 14] = [
            ("<a><b></a>", "expected </b>", "</a>"),
            ("<a>", "<a> is never closed", "<a>"),
            ("</a>", "no matching start tag", "</a>"),
            ("<a/><b/>", "only one root element", "<b/>"),
            ("text<a/>", "before the root element", "text"),
            ("<a/> tail", "after the root element", "tail"),
            ("<a b=1/>", "must be in quotes", "1/>"),
            ("<a b=\"1\" b=\"2\"/>", "appears twice", "b=\"2\""),
            ("<a b=\"1\"c=\"2\"/>", "separated by whitespace", "c="),
            ("<a>fish & chips</a>", "must start an entity", "& chips"),
            ("<a>1 < 2</a>", "must be followed by an element name", "< 2"),
            ("<a><!-- x -- y --></a>", "'--' is not allowed", "-- y"),
            ("<a/><?xml version=\"1.0\"?>", "very first thing", "<?xml"),
            ("", "no root element", ""),
        ];
        for (source, message, at) in cases {
            let error = validate_xml(source).expect_err(source);
            assert!(
                error.message.contains(message),
                "{source}: {}",
                error.message
            );
            assert!(
                source[error.offset..].starts_with(at),
                "{source}: offset {}",
                error.offset
            );
        }
        assert!(validate_xml("<a b=\"x<y\"/>").is_err());
        assert!(validate_xml("<a><![CDATA[open</a>").is_err());
    }

    #[test]
    fn xml_is_indented_without_touching_text_or_preserved_content() {
        let source = "<?xml version=\"1.0\"?><root a=\"1\"><!-- note --><item id=\"1\"><name>Ada  Lovelace</name><empty></empty><self/></item><p>mixed <b>bold</b> text</p><pre xml:space=\"preserve\"> <x/> </pre></root>";
        let pretty = format_xml(source).expect("pretty");
        assert_eq!(
            pretty,
            "<?xml version=\"1.0\"?>\n<root a=\"1\">\n  <!-- note -->\n  <item id=\"1\">\n    <name>Ada  Lovelace</name>\n    <empty></empty>\n    <self/>\n  </item>\n  <p>mixed <b>bold</b> text</p>\n  <pre xml:space=\"preserve\"> <x/> </pre>\n</root>"
        );
        // Stable on its own output, and deep nesting does not recurse.
        assert_eq!(format_xml(&pretty).expect("again"), pretty);
        let deep = format!("{}{}", "<a>".repeat(20_000), "</a>".repeat(20_000));
        let formatted = format_xml(&deep).expect("deep");
        assert!(formatted.lines().count() >= 20_000);
        // Indentation is capped, so the result stays proportional to the input.
        assert!(formatted.len() < deep.len() * 200);
        assert_eq!(
            format_xml("<a>\r\n<b/>\r\n</a>\r\n").expect("crlf"),
            "<a>\r\n  <b/>\r\n</a>\r\n"
        );
        assert!(format_xml("<a><b></a>").is_err());
    }
}
