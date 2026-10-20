//! YAML Validate and Beautify.
//!
//! Validation parses every document in the stream and also reports a key
//! repeated in one mapping, which YAML forbids but most parsers accept
//! silently. Beautify re-writes the documents from their parsed values, so it
//! cannot keep comments, and it writes aliases out in full; it refuses
//! documents with tags (`!Ref`), whose meaning it would lose.

use crate::format_tools::ToolError;
use std::collections::HashSet;
use yaml_rust2::{
    parser::{Event, MarkedEventReceiver, Parser},
    scanner::Marker,
    YamlEmitter, YamlLoader,
};

/// What Beautify would lose, found while validating.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct YamlFeatures {
    /// Lines that look like they hold a comment.
    pub comments: bool,
    /// `&anchor` / `*alias` pairs, which Beautify writes out in full.
    pub anchors: bool,
    /// Tags such as `!Ref` or `!!binary`.
    pub tags: bool,
}

enum Frame {
    Mapping {
        keys: HashSet<String>,
        expect_key: bool,
    },
    Sequence,
}

/// Follows the event stream to find repeated keys and note anchors and tags.
#[derive(Default)]
struct Checker {
    stack: Vec<Frame>,
    features: YamlFeatures,
    duplicate: Option<ToolError>,
}

impl Checker {
    /// A node (scalar, alias, or collection) took its place in the parent;
    /// `key` is the scalar text when the node is a plain key.
    fn node(&mut self, key: Option<&str>, text_offset: usize) {
        if let Some(Frame::Mapping { keys, expect_key }) = self.stack.last_mut() {
            if *expect_key {
                if let Some(key) = key {
                    if !keys.insert(key.to_owned()) && self.duplicate.is_none() {
                        self.duplicate = Some(ToolError {
                            message: format!("the key \"{key}\" appears twice in the same mapping"),
                            offset: text_offset,
                        });
                    }
                }
            }
            *expect_key = !*expect_key;
        }
    }
}

/// A receiver that also knows the text, to turn markers into byte offsets.
struct Receiver<'a> {
    checker: Checker,
    text: &'a str,
}

impl MarkedEventReceiver for Receiver<'_> {
    fn on_event(&mut self, event: Event, mark: Marker) {
        let offset = byte_offset(self.text, mark);
        let checker = &mut self.checker;
        match event {
            Event::Scalar(value, _, anchor, tag) => {
                checker.features.anchors |= anchor > 0;
                checker.features.tags |= tag.is_some();
                checker.node(Some(&value), offset);
            }
            Event::Alias(_) => {
                checker.features.anchors = true;
                checker.node(None, offset);
            }
            Event::MappingStart(anchor, ref tag) | Event::SequenceStart(anchor, ref tag) => {
                checker.features.anchors |= anchor > 0;
                checker.features.tags |= tag.is_some();
                // A collection used as a key is never a duplicate of a plain key.
                checker.node(None, offset);
                checker
                    .stack
                    .push(if matches!(event, Event::MappingStart(..)) {
                        Frame::Mapping {
                            keys: HashSet::new(),
                            expect_key: true,
                        }
                    } else {
                        Frame::Sequence
                    });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                checker.stack.pop();
            }
            Event::DocumentStart => checker.stack.clear(),
            _ => {}
        }
    }
}

/// The byte offset of a parser marker. The marker counts characters, so it
/// is converted rather than trusted as a byte index.
fn byte_offset(text: &str, mark: Marker) -> usize {
    text.char_indices()
        .nth(mark.index())
        .map_or(text.len(), |(offset, _)| offset)
}

/// Whether some line seems to hold a comment: a `#` at the start of a line or
/// after a space. A `#` inside a quoted or block string can look the same, so
/// this may say yes when there is none; it never says no when there is one.
fn has_comment_like(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('#') || line.contains(" #") || line.contains("\t#")
    })
}

/// Checks a YAML stream and reports what Beautify would lose.
pub(crate) fn validate_yaml(text: &str) -> Result<YamlFeatures, ToolError> {
    let mut receiver = Receiver {
        checker: Checker::default(),
        text,
    };
    let mut parser = Parser::new_from_str(text);
    parser
        .load(&mut receiver, true)
        .map_err(|error| ToolError {
            message: error.info().to_owned(),
            offset: byte_offset(text, *error.marker()),
        })?;
    if let Some(duplicate) = receiver.checker.duplicate {
        return Err(duplicate);
    }
    let mut features = receiver.checker.features;
    features.comments = has_comment_like(text);
    Ok(features)
}

/// Re-writes a valid YAML stream with two-space indentation. Refuses tags;
/// the caller asks before losing comments or anchors.
pub(crate) fn format_yaml(text: &str) -> Result<String, ToolError> {
    let features = validate_yaml(text)?;
    if features.tags {
        return Err(ToolError {
            message:
                "Beautify cannot keep YAML tags such as !Ref, so it left the document unchanged"
                    .into(),
            offset: 0,
        });
    }
    let documents = YamlLoader::load_from_str(text).map_err(|error| ToolError {
        message: error.info().to_owned(),
        offset: byte_offset(text, *error.marker()),
    })?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let explicit_start = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("---");
    let mut out = String::new();
    for (index, document) in documents.iter().enumerate() {
        let mut written = String::new();
        YamlEmitter::new(&mut written)
            .dump(document)
            .map_err(|error| ToolError {
                message: format!("{error:?}"),
                offset: 0,
            })?;
        // The emitter starts every document with "---"; keep it only where the
        // source had one or where it separates documents.
        let body = written
            .strip_prefix("---\n")
            .or_else(|| written.strip_prefix("---"))
            .unwrap_or(&written);
        if index > 0 || explicit_start {
            out.push_str("---\n");
        }
        out.push_str(body.trim_end_matches('\n'));
        out.push('\n');
    }
    if newline == "\r\n" {
        out = out.replace('\n', "\r\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{format_yaml, validate_yaml, YamlFeatures};

    #[test]
    fn yaml_streams_are_validated_with_a_position() {
        assert!(validate_yaml("name: build\nsteps:\n  - run: make\n").is_ok());
        assert!(validate_yaml("a: 1\n---\nb: 2\n").is_ok());
        let source = "jobs:\n  build:\n    runs-on: [ubuntu\n";
        let error = validate_yaml(source).expect_err("unclosed flow sequence");
        assert!(error.offset <= source.len());
        // A tab used for indentation is an error in YAML.
        assert!(validate_yaml("a:\n\tb: 1\n").is_err());
    }

    #[test]
    fn a_repeated_key_is_reported_where_it_repeats() {
        let source = "name: a\n# é\nname: b\n";
        let error = validate_yaml(source).expect_err("duplicate key");
        assert!(
            error.message.contains("\"name\" appears twice"),
            "{}",
            error.message
        );
        assert_eq!(&source[error.offset..error.offset + 4], "name");
        // The same key in different mappings, or in different documents, is fine.
        assert!(validate_yaml("a: {x: 1}\nb: {x: 2}\n---\na: 3\n").is_ok());
    }

    #[test]
    fn validation_says_what_beautify_would_lose() {
        assert_eq!(
            validate_yaml("a: 1\n").expect("plain"),
            YamlFeatures::default()
        );
        assert!(validate_yaml("# note\na: 1\n").expect("comment").comments);
        assert!(validate_yaml("a: &x 1\nb: *x\n").expect("anchor").anchors);
        assert!(validate_yaml("a: !Ref Bucket\n").expect("tag").tags);
    }

    #[test]
    fn beautify_reindents_and_keeps_values_order_and_line_endings() {
        let messy =
            "name:   build\nsteps:\n    -   run: make\n    -   run: test\nenv: {A: 1, B: two}\n";
        let tidy = format_yaml(messy).expect("beautify");
        assert_eq!(
            tidy,
            "name: build\nsteps:\n  - run: make\n  - run: test\nenv:\n  A: 1\n  B: two\n"
        );
        assert_eq!(format_yaml(&tidy).expect("stable"), tidy);
        assert_eq!(format_yaml("a:   1\r\n").expect("crlf"), "a: 1\r\n");
        assert_eq!(
            format_yaml("---\na: 1\n---\nb: 2\n").expect("two documents"),
            "---\na: 1\n---\nb: 2\n"
        );
    }

    #[test]
    fn beautify_refuses_tags() {
        let error = format_yaml("bucket: !Ref Bucket\n").expect_err("tag");
        assert!(error.message.contains("tags"));
    }
}
