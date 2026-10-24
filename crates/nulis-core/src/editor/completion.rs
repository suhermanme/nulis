//! Completion requests, accepting a suggestion, and moving through a
//! snippet's placeholders with Tab.
//!
//! A request reads at most [`COMPLETION_WINDOW_BYTES`] on each side of the
//! primary caret and answers with the byte range the typed word occupies,
//! so the interface replaces exactly that range. Every answer names the
//! revision it was made for; accepting checks it again.

use super::{
    decode_text, editable_ranges, encode_text, summary, validate_revision, DocumentId,
    DocumentManager, DocumentSummary, EditorError, EditorSession, Selection,
};
use crate::editor::indentation::{apply_planned, read_line_context, PlannedEdit};
use nulis_language::{
    completion::{
        expand_snippet, rank, word_prefix, CompletionItem, CompletionProvider, CompletionRequest,
    },
    indent::{leading_whitespace, IndentUnit},
    CapabilityKind, Provider,
};
use serde::Serialize;
use std::sync::Arc;

/// Bytes read on each side of the caret for words.
pub const COMPLETION_WINDOW_BYTES: u64 = 128 * 1024;

/// The answer to `completion.request`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    pub request_id: u64,
    #[serde(with = "super::decimal_u64")]
    pub revision: u64,
    /// The typed word, which an accepted item replaces.
    #[serde(with = "super::decimal_u64")]
    pub replace_start: u64,
    #[serde(with = "super::decimal_u64")]
    pub replace_end: u64,
    pub prefix: String,
    pub items: Vec<CompletionItem>,
}

/// Placeholders of an inserted snippet still to visit with Tab.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SnippetSession {
    /// Every placeholder as a document byte range, in visiting order.
    stops: Vec<(u64, u64)>,
    /// The placeholder selected now.
    current: usize,
}

impl SnippetSession {
    /// Moves placeholders with an edit (ascending `(start, end, inserted
    /// length)` in old coordinates). Typing inside a placeholder grows it;
    /// an edit across a placeholder's edge ends the session.
    pub(super) fn relocate(&mut self, edits: &[(u64, u64, usize)]) -> bool {
        for (start, end, inserted) in edits.iter().rev() {
            let delta = *inserted as i128 - i128::from(end - start);
            for stop in &mut self.stops {
                let (stop_start, stop_end) = *stop;
                let shift = |value: u64| u64::try_from(i128::from(value) + delta).ok();
                let inside = *start >= stop_start && *end <= stop_end;
                if *end <= stop_start && !(*start == stop_start && *end == stop_start) {
                    match (shift(stop_start), shift(stop_end)) {
                        (Some(a), Some(b)) => *stop = (a, b),
                        _ => return false,
                    }
                } else if inside {
                    match shift(stop_end) {
                        Some(b) => stop.1 = b,
                        None => return false,
                    }
                } else if *start > stop_end {
                    // After the placeholder: unaffected.
                } else {
                    return false;
                }
            }
        }
        true
    }
}

fn completion_providers(
    manager: &DocumentManager,
    session: &EditorSession,
) -> Vec<(nulis_language::ProviderInfo, Arc<dyn CompletionProvider>)> {
    let mut providers: Vec<_> = manager
        .capability_registry()
        .resolve_all(&session.language_id, CapabilityKind::Completion)
        .into_iter()
        .filter_map(|resolved| match resolved.provider {
            Provider::Completion(provider) => Some((resolved.info, provider)),
            _ => None,
        })
        .collect();
    providers.sort_by_key(|(_, provider)| std::cmp::Reverse(provider.priority()));
    providers
}

impl DocumentManager {
    /// Whether completion opens by itself while typing ("auto"), only when
    /// asked ("manual"), or not at all (`None`).
    pub(super) fn completion_mode(&self, language: &nulis_syntax::LanguageId) -> Option<String> {
        let providers: Vec<_> = self
            .capability_registry()
            .resolve_all(language, CapabilityKind::Completion)
            .into_iter()
            .filter_map(|resolved| match resolved.provider {
                Provider::Completion(provider) => Some(provider),
                _ => None,
            })
            .collect();
        (!providers.is_empty()).then(|| {
            if providers
                .iter()
                .any(|provider| provider.opens_automatically())
            {
                "auto".to_owned()
            } else {
                "manual".to_owned()
            }
        })
    }

    /// Suggestions for the word before the primary caret. Without
    /// `explicit`, a word shorter than `min_prefix` characters gets none.
    ///
    /// # Errors
    /// Returns an error for an unknown document, a stale revision, or a
    /// storage failure.
    pub fn completions(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        request_id: u64,
        explicit: bool,
        min_prefix: usize,
    ) -> Result<CompletionList, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let revision = session.document.table().revision();
        let table = session.document.table();
        let selection = session
            .selections
            .first()
            .copied()
            .unwrap_or(Selection::caret(0));
        let caret = selection.active_byte.min(table.len());
        let empty = |prefix: String| CompletionList {
            request_id,
            revision,
            replace_start: caret,
            replace_end: caret,
            prefix,
            items: Vec::new(),
        };
        if selection.anchor_byte != selection.active_byte || session.rectangular_selection.is_some()
        {
            return Ok(empty(String::new()));
        }
        let window_start = caret.saturating_sub(COMPLETION_WINDOW_BYTES);
        let window_end = (caret + COMPLETION_WINDOW_BYTES).min(table.len());
        let before = decode_text(
            session.encoding,
            &table.read_vec(window_start, caret - window_start)?,
        )
        .0;
        let after = decode_text(
            session.encoding,
            &table.read_vec(caret, window_end - caret)?,
        )
        .0;
        let word_bytes = self.languages().word_bytes(&session.language_id).to_vec();
        let prefix = word_prefix(&before, &word_bytes).to_owned();
        if !explicit && prefix.chars().count() < min_prefix.max(1) {
            return Ok(empty(prefix));
        }
        let request = CompletionRequest {
            prefix: &prefix,
            before: &before,
            after: &after,
            word_bytes: &word_bytes,
        };
        let registry = self.capability_registry();
        let sources = completion_providers(self, session)
            .into_iter()
            .map(|(info, provider)| {
                registry
                    .call(&info, || Ok(provider.complete(&request)))
                    .unwrap_or_default()
            })
            .collect();
        let prefix_bytes = encode_text(session.encoding, &prefix)?.len() as u64;
        Ok(CompletionList {
            request_id,
            revision,
            replace_start: caret - prefix_bytes.min(caret),
            replace_end: caret,
            items: rank(&prefix, sources),
            prefix,
        })
    }

    /// Inserts a chosen suggestion in place of the typed word at every
    /// caret where that word was typed, as one undo step. A snippet's first
    /// placeholder is selected; Tab visits the next ones.
    ///
    /// # Errors
    /// Returns an error for a stale revision or range, a read-only document,
    /// or a storage failure.
    pub fn accept_completion(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        replace_start: u64,
        replace_end: u64,
        text: &str,
        snippet: bool,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let table = session.document.table();
        let primary = session
            .selections
            .first()
            .map(|selection| selection.active_byte);
        if primary != Some(replace_end) || replace_start > replace_end || replace_end > table.len()
        {
            return Err(EditorError::Tool(
                "The suggestion no longer fits the text".into(),
            ));
        }
        let typed = table.read_vec(replace_start, replace_end - replace_start)?;
        let (unit, _) = self.indent_unit(&session.language_id, session.detected_indent);
        let unit_text = match unit {
            IndentUnit::Tab => "\t".to_owned(),
            IndentUnit::Spaces(width) => " ".repeat(usize::from(width)),
        };
        let mut edits = Vec::new();
        let mut first_stops = Vec::new();
        for (start, end) in editable_ranges(&session.selections) {
            // Other carets complete the same word when they follow the same typed text.
            let typed_here = start == end
                && start >= typed.len() as u64
                && table.read_vec(start - typed.len() as u64, typed.len() as u64)? == typed;
            let from = if typed_here {
                start - typed.len() as u64
            } else {
                start
            };
            let line = read_line_context(table, session.encoding, from, end)?;
            let (inserted, stops) = if snippet {
                let expanded = expand_snippet(
                    text,
                    line.ending,
                    leading_whitespace(&line.before),
                    &unit_text,
                );
                (expanded.text, expanded.stops)
            } else {
                let length = text.len();
                (text.to_owned(), vec![(length, length)])
            };
            first_stops.push(stops);
            edits.push(PlannedEdit {
                start: from,
                end,
                caret: Some(
                    first_stops
                        .last()
                        .and_then(|stops| stops.last())
                        .map_or(inserted.len(), |stop| stop.0),
                ),
                text: inserted,
            });
        }
        apply_planned(session, &edits)?;
        session.snippet = None;
        if snippet && edits.len() == 1 {
            // One caret: select the first placeholder and remember the rest.
            let base = edits[0].start;
            let encoding = session.encoding;
            let absolute = |offset: usize| -> Result<u64, EditorError> {
                Ok(base + encode_text(encoding, &edits[0].text[..offset])?.len() as u64)
            };
            let stops = first_stops[0]
                .iter()
                .map(|(start, end)| Ok((absolute(*start)?, absolute(*end)?)))
                .collect::<Result<Vec<_>, EditorError>>()?;
            if let Some((start, end)) = stops.first().copied() {
                session.selections = vec![Selection {
                    anchor_byte: start,
                    active_byte: end,
                }];
                session.preferred_scalar_columns = vec![None];
                if stops.len() > 1 {
                    session.snippet = Some(SnippetSession { stops, current: 0 });
                }
            }
        }
        Ok(summary(document_id, session))
    }

    /// Tab inside a snippet: selects the next placeholder and returns true.
    /// False when no snippet is active or the caret left its placeholder.
    pub(super) fn next_snippet_stop(session: &mut EditorSession) -> bool {
        let Some(snippet) = session.snippet.as_mut() else {
            return false;
        };
        let (start, end) = snippet.stops[snippet.current];
        let inside = session.selections.len() == 1
            && session.selections.iter().all(|selection| {
                let low = selection.anchor_byte.min(selection.active_byte);
                let high = selection.anchor_byte.max(selection.active_byte);
                low >= start && high <= end
            });
        if !inside || snippet.current + 1 >= snippet.stops.len() {
            session.snippet = None;
            return false;
        }
        snippet.current += 1;
        let (start, end) = snippet.stops[snippet.current];
        session.selections = vec![Selection {
            anchor_byte: start,
            active_byte: end,
        }];
        session.preferred_scalar_columns = vec![None];
        if snippet.current + 1 == snippet.stops.len() {
            session.snippet = None;
        }
        true
    }
}

#[cfg(test)]
mod tests;
