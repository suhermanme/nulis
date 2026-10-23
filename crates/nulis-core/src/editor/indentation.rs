//! Automatic indentation: Enter (`editor.newline`), Tab (`editor.tab`),
//! closing characters that move their line left, and Backspace over
//! indentation made of spaces.
//!
//! Context is read from the piece table, not the viewport: the caret's line
//! (at most [`MAX_CONTEXT_BYTES`] each side) and up to [`MAX_CONTEXT_LINES`]
//! lines above it. Only whitespace and the inserted text are written, so
//! bytes that are not valid in the document's encoding are never rewritten.

use super::{
    decode_text, editable_ranges, encode_text, find_line_start, record_bookmark_edit,
    relocate_bookmarks, summary, validate_revision, DocumentId, DocumentManager, DocumentSummary,
    EditorError, EditorSession, Selection, TextEncoding,
};
use crate::editor::line_ops::LineFormatOptions;
use nulis_document::PieceTable;
use nulis_language::{
    indent::{
        detect_unit, indent_columns, leading_whitespace, IndentContext, IndentProvider, IndentUnit,
        RequiredUnit, MAX_CONTEXT_BYTES, MAX_CONTEXT_LINES,
    },
    CapabilityKind, Provider,
};
use nulis_syntax::LanguageId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// How much indentation Enter adds.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoIndent {
    /// A line break only, as before Nulis 0.5.
    None,
    /// The current line's indentation.
    Keep,
    /// The language's rules.
    #[default]
    Full,
}

/// The `editor.*` indentation settings for one language.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndentSettings {
    pub insert_spaces: bool,
    pub tab_size: u8,
    pub auto_indent: AutoIndent,
    pub detect: bool,
}

impl Default for IndentSettings {
    fn default() -> Self {
        Self {
            insert_spaces: false,
            tab_size: 4,
            auto_indent: AutoIndent::Full,
            detect: true,
        }
    }
}

impl IndentSettings {
    /// Reads `editor.insertSpaces`, `editor.tabSize` (1 to 16),
    /// `editor.autoIndent`, and `editor.detectIndentation` from effective
    /// settings; anything missing or malformed keeps its default.
    #[must_use]
    pub fn from_settings(values: &Value) -> Self {
        let editor = &values["editor"];
        let defaults = Self::default();
        Self {
            insert_spaces: editor["insertSpaces"]
                .as_bool()
                .unwrap_or(defaults.insert_spaces),
            tab_size: editor["tabSize"]
                .as_u64()
                .and_then(|size| u8::try_from(size.clamp(1, 16)).ok())
                .unwrap_or(defaults.tab_size),
            auto_indent: serde_json::from_value(editor["autoIndent"].clone())
                .unwrap_or(defaults.auto_indent),
            detect: editor["detectIndentation"]
                .as_bool()
                .unwrap_or(defaults.detect),
        }
    }
}

/// Indentation settings for every language: a default and per-language ones.
#[derive(Clone, Debug, Default)]
pub struct IndentSettingsTable {
    pub default: IndentSettings,
    pub languages: BTreeMap<String, IndentSettings>,
}

/// How a document is indented right now, for the interface (tab width, and
/// what Tab inserts).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndentationSummary {
    /// Tab inserts spaces rather than a tab character.
    pub spaces: bool,
    /// Columns per indentation level.
    pub width: u8,
    /// Columns a tab character takes.
    pub tab_size: u8,
}

impl Default for IndentationSummary {
    fn default() -> Self {
        Self {
            spaces: false,
            width: 4,
            tab_size: 4,
        }
    }
}

/// Bytes read from the start of a document to guess its indentation.
pub(super) const DETECTION_BYTES: u64 = 64 * 1024;

/// Guesses a document's indentation from its first bytes.
pub(super) fn detect_document_unit(
    table: &PieceTable,
    encoding: TextEncoding,
) -> Option<IndentUnit> {
    let bytes = table.read_vec(0, table.len().min(DETECTION_BYTES)).ok()?;
    detect_unit(&decode_text(encoding, &bytes).0)
}

/// What is known about the caret's line.
pub(super) struct LineContext {
    pub(super) line_start: u64,
    pub(super) before: String,
    pub(super) after: String,
    pub(super) previous: Vec<String>,
    /// The line ending Enter inserts: this line's, else the one above, else LF.
    pub(super) ending: &'static str,
    /// `before` holds the whole line up to the caret (it was not cut at the limit).
    pub(super) complete: bool,
}

fn ending_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.ends_with(b"\r\n") {
        Some("\r\n")
    } else if bytes.ends_with(b"\n") {
        Some("\n")
    } else if bytes.ends_with(b"\r") {
        Some("\r")
    } else {
        None
    }
}

pub(super) fn read_line_context(
    table: &PieceTable,
    encoding: TextEncoding,
    start: u64,
    end: u64,
) -> Result<LineContext, EditorError> {
    let cap = MAX_CONTEXT_BYTES as u64;
    let line_start = find_line_start(table, start)?;
    let complete = start - line_start <= cap;
    let before_end = if complete { start } else { line_start + cap };
    let before = decode_text(
        encoding,
        &table.read_vec(line_start, before_end - line_start)?,
    )
    .0;

    let after_bytes = table.read_vec(end, (table.len() - end).min(cap + 2))?;
    let line_end = after_bytes
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r'));
    let after = decode_text(
        encoding,
        &after_bytes[..line_end.unwrap_or(after_bytes.len()).min(MAX_CONTEXT_BYTES)],
    )
    .0;
    let own_ending = line_end.map(|position| {
        let rest = &after_bytes[position..];
        if rest.starts_with(b"\r\n") {
            "\r\n"
        } else if rest.starts_with(b"\n") {
            "\n"
        } else {
            "\r"
        }
    });

    let window_start = line_start.saturating_sub(cap);
    let above = table.read_vec(window_start, line_start - window_start)?;
    let ending = own_ending.or_else(|| ending_of(&above)).unwrap_or("\n");
    let text = decode_text(encoding, &above).0;
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines.pop(); // What follows the last line ending is the caret's line.
    if window_start > 0 && !lines.is_empty() {
        lines.remove(0); // Cut at the window start.
    }
    let previous = lines
        .iter()
        .rev()
        .take(MAX_CONTEXT_LINES)
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect();
    Ok(LineContext {
        line_start,
        before,
        after,
        previous,
        ending,
        complete,
    })
}

/// One replacement planned for a batch; `caret` is where the caret goes,
/// counted in bytes from the start of `text`.
pub(super) struct PlannedEdit {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) text: String,
    pub(super) caret: Option<usize>,
}

fn overlapping(edits: &[PlannedEdit]) -> bool {
    edits.windows(2).any(|pair| pair[0].end > pair[1].start)
}

/// Applies edits (ascending, non-overlapping) as one revision and one undo
/// step, moving bookmarks and putting a caret where each edit says.
pub(super) fn apply_planned(
    session: &mut EditorSession,
    edits: &[PlannedEdit],
) -> Result<(), EditorError> {
    let encoded = edits
        .iter()
        .map(|edit| encode_text(session.encoding, &edit.text))
        .collect::<Result<Vec<_>, _>>()?;
    let batch = edits
        .iter()
        .zip(&encoded)
        .rev()
        .map(|(edit, bytes)| (edit.start, edit.end - edit.start, bytes.clone()))
        .collect::<Vec<_>>();
    session.document.replace_many(&batch)?;
    let bookmark_edits = edits
        .iter()
        .zip(&encoded)
        .map(|(edit, bytes)| (edit.start, edit.end, bytes.len()))
        .collect::<Vec<_>>();
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);
    let mut shift: i128 = 0;
    let mut carets = Vec::new();
    for (edit, bytes) in edits.iter().zip(&encoded) {
        if let Some(caret) = edit.caret {
            // The caret offset counts characters of `text`; encode the prefix to count bytes.
            let prefix = encode_text(session.encoding, &edit.text[..caret])?.len();
            let position = i128::from(edit.start) + shift + prefix as i128;
            carets.push(Selection::caret(
                u64::try_from(position).map_err(|_| EditorError::OffsetOverflow)?,
            ));
        }
        shift += bytes.len() as i128 - i128::from(edit.end - edit.start);
    }
    session.selections = carets;
    session.rectangular_selection = None;
    session.preferred_scalar_columns = vec![None; session.selections.len()];
    session.navigation_index.reset();
    Ok(())
}

/// What Enter needs to know besides the line: the setting, the language's
/// rules, and the indentation unit.
struct Indenting<'a> {
    auto_indent: AutoIndent,
    provider: Option<&'a dyn IndentProvider>,
    unit: IndentUnit,
    tab_width: u8,
}

impl Indenting<'_> {
    /// The edits Enter makes at each caret. With `rewrite_lines` off, only
    /// text at the carets changes (used when several carets share a line).
    fn plan(
        &self,
        ranges: &[(u64, u64)],
        contexts: &[LineContext],
        rewrite_lines: bool,
    ) -> Vec<PlannedEdit> {
        let mut edits = Vec::new();
        for ((start, end), line) in ranges.iter().zip(contexts) {
            let ending = line.ending;
            if self.auto_indent == AutoIndent::None || !line.complete {
                let indent = if self.auto_indent == AutoIndent::None {
                    ""
                } else {
                    leading_whitespace(&line.before)
                };
                edits.push(PlannedEdit {
                    start: *start,
                    end: *end,
                    text: format!("{ending}{indent}"),
                    caret: Some(ending.len() + indent.len()),
                });
                continue;
            }
            let previous: Vec<&str> = line.previous.iter().map(String::as_str).collect();
            let context = IndentContext {
                before: &line.before,
                after: &line.after,
                previous: &previous,
                unit: self.unit,
                tab_width: self.tab_width,
            };
            let action = self.provider.map_or_else(
                || nulis_language::indent::NewlineAction {
                    indent: leading_whitespace(&line.before).to_owned(),
                    ..Default::default()
                },
                |provider| provider.on_enter(&context),
            );
            let old_indent = leading_whitespace(&line.before);
            let only_indent = line.before.len() == old_indent.len();
            let mut inserted = format!("{ending}{}", action.indent);
            let caret = inserted.len();
            if let Some(closing) = &action.closing_indent {
                inserted.push_str(ending);
                inserted.push_str(closing);
            }
            // A line that holds only indentation keeps none when the
            // caret leaves it, so Enter leaves no trailing spaces.
            let new_indent = if only_indent && line.after.is_empty() && !line.before.is_empty() {
                Some(String::new())
            } else {
                action.current_indent.clone()
            };
            match new_indent.filter(|indent| rewrite_lines && indent != old_indent) {
                Some(indent) if only_indent => edits.push(PlannedEdit {
                    start: line.line_start,
                    end: *end,
                    caret: Some(indent.len() + caret),
                    text: format!("{indent}{inserted}"),
                }),
                Some(indent) => {
                    edits.push(PlannedEdit {
                        start: line.line_start,
                        end: line.line_start + old_indent.len() as u64,
                        text: indent,
                        caret: None,
                    });
                    edits.push(PlannedEdit {
                        start: *start,
                        end: *end,
                        text: inserted,
                        caret: Some(caret),
                    });
                }
                None => edits.push(PlannedEdit {
                    start: *start,
                    end: *end,
                    text: inserted,
                    caret: Some(caret),
                }),
            }
        }
        edits
    }
}

impl DocumentManager {
    /// Replaces the indentation settings and refreshes open documents.
    pub fn set_indent_settings(&self, table: IndentSettingsTable) {
        *self
            .indent_settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = table;
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                self.refresh_language_state(session);
            }
        }
    }

    pub(super) fn indent_settings_for(&self, language: &LanguageId) -> IndentSettings {
        let table = self
            .indent_settings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        table
            .languages
            .get(language.as_str())
            .copied()
            .unwrap_or(table.default)
    }

    fn indent_provider(&self, language: &LanguageId) -> Option<Arc<dyn IndentProvider>> {
        match self
            .capability_registry()
            .resolve(language, CapabilityKind::Indentation)?
            .provider
        {
            Provider::Indent(provider) => Some(provider),
            _ => None,
        }
    }

    /// The indentation unit Enter and Tab use, and the tab width.
    pub(super) fn indent_unit(
        &self,
        language: &LanguageId,
        detected: Option<IndentUnit>,
    ) -> (IndentUnit, u8) {
        let settings = self.indent_settings_for(language);
        let tab_size = settings.tab_size;
        let configured = if settings.insert_spaces {
            IndentUnit::Spaces(tab_size)
        } else {
            IndentUnit::Tab
        };
        let unit = detected.filter(|_| settings.detect).unwrap_or(configured);
        let required = self
            .indent_provider(language)
            .and_then(|provider| provider.required_unit());
        let unit = match (required, unit) {
            (Some(RequiredUnit::Spaces), IndentUnit::Tab) => IndentUnit::Spaces(tab_size),
            (Some(RequiredUnit::Tabs), _) => IndentUnit::Tab,
            (_, unit) => unit,
        };
        (unit, tab_size)
    }

    pub(super) fn document_indentation(
        &self,
        language: &LanguageId,
        detected: Option<IndentUnit>,
    ) -> IndentationSummary {
        let (unit, tab_size) = self.indent_unit(language, detected);
        IndentationSummary {
            spaces: matches!(unit, IndentUnit::Spaces(_)),
            width: match unit {
                IndentUnit::Tab => tab_size,
                IndentUnit::Spaces(width) => width,
            },
            tab_size,
        }
    }

    /// Re-resolves what depends on the language and settings.
    pub(super) fn refresh_language_state(&self, session: &mut EditorSession) {
        session.capabilities = self.document_capabilities(&session.language_id);
        session.indentation =
            self.document_indentation(&session.language_id, session.detected_indent);
    }

    /// Indent and Outdent options for a document: its unit and tab width.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn line_format_defaults(
        &self,
        document_id: &DocumentId,
    ) -> Result<LineFormatOptions, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let (unit, tab_size) = self.indent_unit(&session.language_id, session.detected_indent);
        Ok(LineFormatOptions {
            tab_width: match unit {
                IndentUnit::Tab => tab_size,
                IndentUnit::Spaces(width) => width,
            },
            indent_with_spaces: matches!(unit, IndentUnit::Spaces(_)),
            ..LineFormatOptions::default()
        })
    }

    /// Enter: a line break plus the indentation the settings and the
    /// language's rules call for, at every caret, as one undo step.
    /// With `editor.autoIndent` set to `none` it inserts the line break only.
    ///
    /// # Errors
    /// Returns an error for a stale revision, a read-only document, or a
    /// storage failure.
    pub fn newline(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if session.rectangular_selection.is_some() {
            // A rectangle takes one line break per row, as typing it always did.
            let caret = session
                .selections
                .first()
                .map_or(0, |selection| selection.active_byte);
            let ending =
                read_line_context(session.document.table(), session.encoding, caret, caret)?.ending;
            drop(sessions);
            return self.insert_text(document_id, expected_revision, ending, false);
        }
        let settings = self.indent_settings_for(&session.language_id);
        let provider = (settings.auto_indent == AutoIndent::Full)
            .then(|| self.indent_provider(&session.language_id))
            .flatten();
        let (unit, tab_width) = self.indent_unit(&session.language_id, session.detected_indent);
        let ranges = editable_ranges(&session.selections);
        let mut contexts = Vec::with_capacity(ranges.len());
        for (start, end) in &ranges {
            contexts.push(read_line_context(
                session.document.table(),
                session.encoding,
                *start,
                *end,
            )?);
        }
        let indenting = Indenting {
            auto_indent: settings.auto_indent,
            provider: provider.as_deref(),
            unit,
            tab_width,
        };
        let mut edits = indenting.plan(&ranges, &contexts, true);
        if overlapping(&edits) {
            // Several carets on one line: leave existing indentation alone.
            edits = indenting.plan(&ranges, &contexts, false);
        }
        apply_planned(session, &edits)?;
        Ok(summary(document_id, session))
    }

    /// Tab: one level of indentation at every caret, as a tab character or
    /// as spaces up to the next tab stop.
    ///
    /// # Errors
    /// Returns an error for a stale revision, a read-only document, or a
    /// storage failure.
    pub fn insert_tab(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if Self::next_snippet_stop(session) {
            return Ok(summary(document_id, session));
        }
        let (unit, tab_width) = self.indent_unit(&session.language_id, session.detected_indent);
        if session.rectangular_selection.is_some() || unit == IndentUnit::Tab {
            drop(sessions);
            return self.insert_text(document_id, expected_revision, "\t", false);
        }
        let IndentUnit::Spaces(width) = unit else {
            return Err(EditorError::StateUnavailable);
        };
        let mut edits = Vec::new();
        for (start, end) in editable_ranges(&session.selections) {
            let line = read_line_context(session.document.table(), session.encoding, start, end)?;
            let column = indent_columns(&line.before, tab_width);
            let width = usize::from(width.max(1));
            let spaces = " ".repeat(width - column % width);
            edits.push(PlannedEdit {
                start,
                end,
                caret: Some(spaces.len()),
                text: spaces,
            });
        }
        apply_planned(session, &edits)?;
        Ok(summary(document_id, session))
    }

    /// Typing one character: when the language says it closes a block on a
    /// line of its own (`}`, `</`, Python `else:`), the line moves left in
    /// the same undo step. Anything else is ordinary typing.
    ///
    /// # Errors
    /// Returns the errors of [`DocumentManager::insert_text`].
    pub fn insert_typed_text(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        text: &str,
        overwrite: bool,
    ) -> Result<DocumentSummary, EditorError> {
        let mut characters = text.chars();
        let (Some(typed), None) = (characters.next(), characters.next()) else {
            return self.insert_text(document_id, expected_revision, text, overwrite);
        };
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let settings = self.indent_settings_for(&session.language_id);
        let provider = (settings.auto_indent == AutoIndent::Full && !overwrite)
            .then(|| self.indent_provider(&session.language_id))
            .flatten();
        let Some(provider) = provider.filter(|_| session.rectangular_selection.is_none()) else {
            drop(sessions);
            return self.insert_text(document_id, expected_revision, text, overwrite);
        };
        let (unit, tab_width) = self.indent_unit(&session.language_id, session.detected_indent);
        let mut edits = Vec::new();
        let mut moved = false;
        for (start, end) in editable_ranges(&session.selections) {
            let line = read_line_context(session.document.table(), session.encoding, start, end)?;
            let previous: Vec<&str> = line.previous.iter().map(String::as_str).collect();
            let context = IndentContext {
                before: &line.before,
                after: &line.after,
                previous: &previous,
                unit,
                tab_width,
            };
            let old_indent = leading_whitespace(&line.before);
            if let Some(indent) = (line.complete && start == end)
                .then(|| provider.on_type(&context, typed))
                .flatten()
            {
                moved = true;
                edits.push(PlannedEdit {
                    start: line.line_start,
                    end: line.line_start + old_indent.len() as u64,
                    text: indent,
                    caret: None,
                });
            }
            edits.push(PlannedEdit {
                start,
                end,
                text: text.to_owned(),
                caret: Some(text.len()),
            });
        }
        if !moved || overlapping(&edits) {
            drop(sessions);
            return self.insert_text(document_id, expected_revision, text, overwrite);
        }
        apply_planned(session, &edits)?;
        Ok(summary(document_id, session))
    }

    /// How many bytes Backspace removes before `caret` when the line up to
    /// it is all spaces and the document indents with spaces: back to the
    /// previous indentation stop. `None` means one character, as usual.
    pub(super) fn indentation_backspace(
        &self,
        session: &EditorSession,
        caret: u64,
    ) -> Result<Option<u64>, EditorError> {
        let (unit, _) = self.indent_unit(&session.language_id, session.detected_indent);
        let IndentUnit::Spaces(width) = unit else {
            return Ok(None);
        };
        let table = session.document.table();
        let line_start = find_line_start(table, caret)?;
        let length = caret - line_start;
        if length == 0 || length > MAX_CONTEXT_BYTES as u64 {
            return Ok(None);
        }
        let bytes = table.read_vec(line_start, length)?;
        if !bytes.iter().all(|byte| *byte == b' ') {
            return Ok(None);
        }
        let width = u64::from(width.max(1));
        let remove = match length % width {
            0 => width,
            rest => rest,
        };
        Ok(Some(remove.min(length)))
    }
}

#[cfg(test)]
mod tests {
    use super::{AutoIndent, IndentSettings, IndentSettingsTable};
    use crate::editor::{DocumentId, DocumentManager, EditorError, Selection, ViewportRequest};

    fn settings(auto_indent: AutoIndent, insert_spaces: bool) -> IndentSettingsTable {
        IndentSettingsTable {
            default: IndentSettings {
                insert_spaces,
                tab_size: 4,
                auto_indent,
                detect: true,
            },
            languages: std::collections::BTreeMap::new(),
        }
    }

    /// Opens `name` with `text`, where each `|` marks a caret.
    fn open(manager: &DocumentManager, name: &str, text: &str) -> (DocumentId, u64) {
        let plain = text.replace('|', "");
        let document = manager
            .create_untitled(name.into(), plain)
            .expect("document");
        let mut carets = Vec::new();
        let mut offset = 0_u64;
        for part in text
            .split('|')
            .collect::<Vec<_>>()
            .split_last()
            .expect("text")
            .1
        {
            offset += part.len() as u64;
            carets.push(Selection::caret(offset));
        }
        manager
            .set_selections(&document.document_id, carets)
            .expect("carets");
        (document.document_id, document.revision)
    }

    /// The text with `|` at every caret.
    fn shown(manager: &DocumentManager, id: &DocumentId) -> String {
        let view = manager
            .viewport(&ViewportRequest {
                document_id: id.clone(),
                start_byte: 0,
                start_line: 1,
                line_continuation: false,
                max_bytes: 64 * 1024,
            })
            .expect("viewport");
        let mut text = manager.markdown_preview_source(id).expect("text").text;
        let mut carets: Vec<usize> = view
            .selections
            .iter()
            .map(|selection| usize::try_from(selection.active_byte).expect("small"))
            .collect();
        carets.sort_unstable();
        for caret in carets.into_iter().rev() {
            text.insert(caret, '|');
        }
        text
    }

    fn enter(manager: &DocumentManager, name: &str, text: &str) -> String {
        let (id, revision) = open(manager, name, text);
        manager.newline(&id, revision).expect("newline");
        shown(manager, &id)
    }

    #[test]
    fn auto_indent_none_inserts_exactly_the_line_ending_typing_used_to() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::None, false));
        assert_eq!(enter(&manager, "a.rs", "    fn a() {|"), "    fn a() {\n|");
        assert_eq!(
            enter(&manager, "a.rs", "a\r\n  b|\r\n"),
            "a\r\n  b\r\n|\r\n"
        );
        assert_eq!(enter(&manager, "a.txt", "a\rb|"), "a\rb\r|");
        assert_eq!(enter(&manager, "a.txt", "|"), "\n|");
        assert_eq!(enter(&manager, "a.txt", "x\r\n\ty|"), "x\r\n\ty\r\n|");
    }

    #[test]
    fn keep_carries_the_indentation_and_full_follows_the_language() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Keep, false));
        assert_eq!(enter(&manager, "a.rs", "\t  x {|"), "\t  x {\n\t  |");
        manager.set_indent_settings(settings(AutoIndent::Full, false));
        assert_eq!(enter(&manager, "a.rs", "fn a() {|}"), "fn a() {\n\t|\n}");
        assert_eq!(enter(&manager, "a.py", "def f():|"), "def f():\n\t|");
        assert_eq!(enter(&manager, "notes.txt", "  x {|"), "  x {\n  |");
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        assert_eq!(enter(&manager, "a.rs", "fn a() {|}"), "fn a() {\n    |\n}");
        assert_eq!(
            enter(&manager, "a.rs", "fn a() {|}\r\n"),
            "fn a() {\r\n    |\r\n}\r\n"
        );
        // A line holding only indentation leaves no spaces behind.
        assert_eq!(enter(&manager, "a.rs", "{\n    |"), "{\n\n    |");
        // YAML never gets tabs, even when tabs are the setting.
        manager.set_indent_settings(settings(AutoIndent::Full, false));
        assert_eq!(enter(&manager, "a.yaml", "jobs:|"), "jobs:\n    |");
        // A Makefile recipe gets a tab, even when spaces are the setting.
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        assert_eq!(enter(&manager, "Makefile", "all: a.o|"), "all: a.o\n\t|");
        // Ruby `end` moves left on Enter.
        assert_eq!(
            enter(&manager, "a.rb", "def a\n    b\n    end|"),
            "def a\n    b\nend\n|"
        );
    }

    #[test]
    fn every_caret_gets_its_own_indentation_in_one_undo_step() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        let (id, revision) = open(&manager, "a.js", "if (a) {|\n  b();|");
        let before = manager.markdown_preview_source(&id).expect("text").text;
        let after = manager.newline(&id, revision).expect("newline");
        assert_eq!(shown(&manager, &id), "if (a) {\n    |\n  b();\n  |");
        assert_eq!(after.revision, revision + 1);
        manager.undo(&id).expect("undo");
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            before
        );
        // Two carets on one indentation-only line still each get a line.
        let (id, revision) = open(&manager, "a.js", "  |  |");
        manager.newline(&id, revision).expect("newline");
        assert_eq!(shown(&manager, &id), "  \n  |  \n    |");
    }

    #[test]
    fn a_closing_brace_moves_its_line_left_in_the_same_undo_step() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        let (id, revision) = open(&manager, "a.ts", "function a() {\n    x();\n    |");
        let typed = manager
            .insert_typed_text(&id, revision, "}", false)
            .expect("type");
        assert_eq!(shown(&manager, &id), "function a() {\n    x();\n}|");
        assert_eq!(typed.revision, revision + 1);
        manager.undo(&id).expect("undo");
        // One step restores the exact bytes (Undo places the caret its usual way).
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            "function a() {\n    x();\n    "
        );
        // Ordinary characters, and autoIndent other than full, change nothing else.
        let (id, revision) = open(&manager, "a.ts", "    |");
        manager
            .insert_typed_text(&id, revision, "x", false)
            .expect("type");
        assert_eq!(shown(&manager, &id), "    x|");
        manager.set_indent_settings(settings(AutoIndent::Keep, true));
        let (id, revision) = open(&manager, "a.ts", "{\n    x();\n    |");
        manager
            .insert_typed_text(&id, revision, "}", false)
            .expect("type");
        assert_eq!(shown(&manager, &id), "{\n    x();\n    }|");
    }

    #[test]
    fn tab_and_backspace_follow_the_indentation_unit() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        let (id, revision) = open(&manager, "a.rs", "ab|");
        manager.insert_tab(&id, revision).expect("tab");
        assert_eq!(shown(&manager, &id), "ab  |");
        let (id, revision) = open(&manager, "a.rs", "\t|");
        manager.insert_tab(&id, revision).expect("tab");
        assert_eq!(shown(&manager, &id), "\t    |");
        let (id, revision) = open(&manager, "a.rs", "      |");
        manager.delete_backward(&id, revision).expect("backspace");
        assert_eq!(shown(&manager, &id), "    |");
        let (id, revision) = open(&manager, "a.rs", "  x  |");
        manager.delete_backward(&id, revision).expect("backspace");
        assert_eq!(shown(&manager, &id), "  x |");
        manager.set_indent_settings(settings(AutoIndent::Full, false));
        let (id, revision) = open(&manager, "a.rs", "ab|");
        manager.insert_tab(&id, revision).expect("tab");
        assert_eq!(shown(&manager, &id), "ab\t|");
        let (id, revision) = open(&manager, "a.rs", "        |");
        manager.delete_backward(&id, revision).expect("backspace");
        assert_eq!(shown(&manager, &id), "       |");
    }

    #[test]
    fn a_document_keeps_its_own_indentation() {
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Full, false));
        let text = "{\n  \"a\": {\n    \"b\": 1\n  },\n  \"c\": [|]\n}";
        assert_eq!(
            enter(&manager, "a.json", text),
            "{\n  \"a\": {\n    \"b\": 1\n  },\n  \"c\": [\n    |\n  ]\n}"
        );
        let (id, _) = open(&manager, "a.json", text);
        let options = manager.line_format_defaults(&id).expect("options");
        assert!(options.indent_with_spaces);
        assert_eq!(options.tab_width, 2);
        // With detection off, the setting wins.
        let mut no_detection = settings(AutoIndent::Full, false);
        no_detection.default.detect = false;
        manager.set_indent_settings(no_detection);
        assert!(
            !manager
                .line_format_defaults(&id)
                .expect("options")
                .indent_with_spaces
        );
    }

    #[test]
    fn line_breaks_inserted_as_text_are_never_indented() {
        // Macros recorded before automatic indentation replay Enter as this.
        let manager = DocumentManager::default();
        manager.set_indent_settings(settings(AutoIndent::Full, true));
        let (id, revision) = open(&manager, "a.rs", "    fn a() {|");
        manager
            .insert_text(&id, revision, "\n", false)
            .expect("insert");
        assert_eq!(shown(&manager, &id), "    fn a() {\n|");
    }

    #[test]
    fn read_only_documents_refuse_enter_and_tab() {
        let manager = DocumentManager::default();
        let (id, revision) = open(&manager, "a.rs", "x|");
        let summary = manager.toggle_read_only(&id).expect("read only");
        assert!(summary.read_only);
        assert!(matches!(
            manager.newline(&id, revision),
            Err(EditorError::Document(_) | EditorError::ReadOnly)
        ));
        assert!(manager.insert_tab(&id, revision).is_err());
        assert_eq!(shown(&manager, &id), "x|");
    }
}
