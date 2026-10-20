//! Find All: every match in a document or a byte range, with line numbers
//! and a short preview of each matching line, for Count All and the
//! "lines containing" list. One forward pass counts lines, so the cost is
//! proportional to the searched range, and results are capped.

use super::{
    decode_text, search_pattern, DocumentId, DocumentManager, EditorError, TextEncoding,
    MAX_BOOKMARKS,
};
use nulis_large_file::ReadAt;
use nulis_search::{
    count_slice, search, search_until, CancellationToken, CountProgress, SearchOptions,
    MAX_PAGE_MATCHES,
};
use serde::Serialize;

pub const MAX_FIND_ALL_MATCHES: usize = 10_000;
pub const MAX_FIND_ALL_LINES: usize = 5_000;
/// Bytes of a matching line shown in the list, from the start of the line.
pub const MAX_LINE_PREVIEW_BYTES: u64 = 512;
const LINE_SCAN_CHUNK: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundMatch {
    #[serde(with = "super::decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "super::decimal_u64")]
    pub end_byte: u64,
    #[serde(with = "super::decimal_u64")]
    pub line: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundLine {
    #[serde(with = "super::decimal_u64")]
    pub line: u64,
    #[serde(with = "super::decimal_u64")]
    pub start_byte: u64,
    /// The start of the line, at most [`MAX_LINE_PREVIEW_BYTES`], without its ending.
    pub text: String,
    /// The line continues past `text`.
    pub truncated: bool,
    pub matches: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindAllResult {
    #[serde(with = "super::decimal_u64")]
    pub revision: u64,
    pub matches: Vec<FoundMatch>,
    pub lines: Vec<FoundLine>,
    /// More matches or lines exist than were returned.
    pub truncated: bool,
}

/// Bytes one counting step reads while it holds the document; between
/// steps the editor stays free for scrolling and typing.
pub const COUNT_SLICE_BYTES: u64 = 64 * 1024 * 1024;

/// Most matches painted for one visible range.
pub const MAX_RANGE_MATCHES: usize = 2_000;

/// Where matches are in a range: enough to paint "highlight all".
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeMatches {
    #[serde(with = "super::decimal_u64")]
    pub revision: u64,
    pub matches: Vec<RangeMatch>,
    /// More matches start in the range than were returned.
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeMatch {
    #[serde(with = "super::decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "super::decimal_u64")]
    pub end_byte: u64,
}

/// Counts lines while moving forward through a document.
pub(super) struct LineTracker {
    position: u64,
    line: u64,
    line_start: u64,
    pending_cr: bool,
}

impl LineTracker {
    pub(super) fn at_start() -> Self {
        Self {
            position: 0,
            line: 1,
            line_start: 0,
            pending_cr: false,
        }
    }

    /// Moves to `offset` (never backward) and returns its line number and line start.
    pub(super) fn advance(
        &mut self,
        source: &dyn ReadAt,
        offset: u64,
    ) -> Result<(u64, u64), EditorError> {
        while self.position < offset {
            let len = (offset - self.position).min(LINE_SCAN_CHUNK);
            let bytes = read_source(source, self.position, len)?;
            for (index, byte) in bytes.iter().enumerate() {
                let next = self.position + index as u64 + 1;
                if std::mem::take(&mut self.pending_cr) && *byte == b'\n' {
                    self.line_start = next;
                    continue;
                }
                match byte {
                    b'\n' => {
                        self.line += 1;
                        self.line_start = next;
                    }
                    b'\r' => {
                        self.line += 1;
                        self.line_start = next;
                        self.pending_cr = true;
                    }
                    _ => {}
                }
            }
            self.position += len;
        }
        Ok((self.line, self.line_start))
    }
}

impl DocumentManager {
    /// Counts the next slice (at most [`COUNT_SLICE_BYTES`]) of a count over
    /// `start..end` of a document, adding to `progress`. The document must
    /// still be at `revision`, so a count never mixes two versions.
    ///
    /// # Errors
    /// Returns an error for an unknown or changed document, an invalid
    /// pattern, or a read failure.
    #[allow(clippy::too_many_arguments)] // One counting step of one request.
    pub fn count_step(
        &self,
        document_id: &DocumentId,
        revision: u64,
        query: &str,
        options: SearchOptions,
        end: u64,
        progress: &mut CountProgress,
        cancellation: &CancellationToken,
    ) -> Result<(), EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        if table.revision() != revision {
            return Err(EditorError::Tool(format!(
                "{} changed while counting; count again",
                session.name
            )));
        }
        let pattern = search_pattern(session.encoding, query, options)?;
        let end = end.min(table.len());
        let stop = progress.next.saturating_add(COUNT_SLICE_BYTES).min(end);
        count_slice(table, &pattern, options, progress, stop, cancellation)?;
        if progress.next >= end {
            progress.done = true;
        }
        Ok(())
    }

    /// The length and revision of a document, for starting a count.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn length_and_revision(&self, document_id: &DocumentId) -> Result<(u64, u64), EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        Ok((
            session.document.table().len(),
            session.document.table().revision(),
        ))
    }

    /// The matches that start in `start..end` (the visible lines), at most
    /// [`MAX_RANGE_MATCHES`]. Uses the same search as Find, so what is
    /// painted is what Find Next would select. The cost is proportional to
    /// the range, not the document.
    ///
    /// # Errors
    /// Returns an error for an unknown document or an invalid pattern.
    pub fn matches_in_range(
        &self,
        document_id: &DocumentId,
        query: &str,
        options: SearchOptions,
        start: u64,
        end: u64,
    ) -> Result<RangeMatches, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let pattern = search_pattern(session.encoding, query, options)?;
        let start = start.min(table.len());
        let page = search_until(
            table,
            &pattern,
            options,
            start,
            end.min(table.len()).max(start),
            MAX_RANGE_MATCHES,
            &CancellationToken::default(),
        )?;
        Ok(RangeMatches {
            revision: table.revision(),
            truncated: page.matches.len() == MAX_RANGE_MATCHES && !page.complete,
            matches: page
                .matches
                .iter()
                .map(|found| RangeMatch {
                    start_byte: found.range.start.0,
                    end_byte: found.range.end.0,
                })
                .collect(),
        })
    }

    /// Finds every match in the document, or in `within` (a byte range), up
    /// to `max_matches` (at most [`MAX_FIND_ALL_MATCHES`]).
    ///
    /// # Errors
    /// Returns an error for an unknown document, an invalid pattern, or a read failure.
    pub fn find_all(
        &self,
        document_id: &DocumentId,
        query: &str,
        options: SearchOptions,
        within: Option<(u64, u64)>,
        max_matches: usize,
    ) -> Result<FindAllResult, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let pattern = search_pattern(session.encoding, query, options)?;
        let max_matches = max_matches.clamp(1, MAX_FIND_ALL_MATCHES);
        let (start, end) = within.map_or((0, table.len()), |(start, end)| {
            (start.min(table.len()), end.min(table.len()).max(start))
        });

        let mut result = FindAllResult {
            revision: table.revision(),
            matches: Vec::new(),
            lines: Vec::new(),
            truncated: false,
        };
        let mut tracker = LineTracker::at_start();
        let mut cursor = start;
        'pages: loop {
            let page = search(
                table,
                &pattern,
                options,
                cursor,
                MAX_PAGE_MATCHES,
                &CancellationToken::default(),
            )?;
            for found in &page.matches {
                let (found_start, found_end) = (found.range.start.0, found.range.end.0);
                if found_end > end {
                    break 'pages;
                }
                if result.matches.len() == max_matches {
                    result.truncated = true;
                    break 'pages;
                }
                let (line, line_start) = tracker.advance(table, found_start)?;
                result.matches.push(FoundMatch {
                    start_byte: found_start,
                    end_byte: found_end,
                    line,
                });
                if let Some(last) = result
                    .lines
                    .last_mut()
                    .filter(|last| last.start_byte == line_start)
                {
                    last.matches += 1;
                } else if result.lines.len() == MAX_FIND_ALL_LINES {
                    result.truncated = true;
                    break 'pages;
                } else {
                    let (text, truncated) = line_preview(table, session.encoding, line_start)?;
                    result.lines.push(FoundLine {
                        line,
                        start_byte: line_start,
                        text,
                        truncated,
                        matches: 1,
                    });
                }
            }
            match page.next_offset {
                Some(next) if !page.complete && next > cursor => cursor = next,
                _ => break,
            }
        }
        Ok(result)
    }

    /// Bookmarks each given offset (Bookmark All); existing bookmarks stay.
    ///
    /// # Errors
    /// Returns an error for an unknown document, an offset past the end, or
    /// when the result would exceed [`MAX_BOOKMARKS`].
    pub fn add_bookmarks(
        &self,
        document_id: &DocumentId,
        offsets: &[u64],
    ) -> Result<Vec<u64>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let len = session.document.table().len();
        if let Some(offset) = offsets.iter().copied().find(|offset| *offset > len) {
            return Err(EditorError::OffsetOutOfRange {
                offset,
                document_len: len,
            });
        }
        let mut bookmarks = session.bookmarks.clone();
        bookmarks.extend(offsets.iter().copied());
        if bookmarks.len() > MAX_BOOKMARKS {
            return Err(EditorError::TooManyBookmarks(MAX_BOOKMARKS));
        }
        session.bookmarks = bookmarks;
        Ok(session.bookmarks.iter().copied().collect())
    }
}

fn read_source(source: &dyn ReadAt, offset: u64, len: u64) -> Result<Vec<u8>, EditorError> {
    let size = usize::try_from(len).map_err(|_| EditorError::OffsetOverflow)?;
    let mut bytes = vec![0; size];
    source
        .read_exact_at(offset, &mut bytes)
        .map_err(|error| EditorError::io(&error))?;
    Ok(bytes)
}

/// The start of a line, without its ending, and whether the line goes on.
pub(super) fn line_preview(
    source: &dyn ReadAt,
    encoding: TextEncoding,
    line_start: u64,
) -> Result<(String, bool), EditorError> {
    let available = source.len() - line_start;
    let bytes = read_source(source, line_start, available.min(MAX_LINE_PREVIEW_BYTES))?;
    let ending = bytes.iter().position(|byte| matches!(byte, b'\n' | b'\r'));
    let shown = ending.unwrap_or(bytes.len());
    let truncated = ending.is_none() && available > MAX_LINE_PREVIEW_BYTES;
    Ok((decode_text(encoding, &bytes[..shown]).0, truncated))
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentManager, SearchMode, Selection};
    use nulis_search::{CancellationToken, SearchOptions};

    fn options(mode: SearchMode, whole_word: bool) -> SearchOptions {
        SearchOptions {
            mode,
            case_sensitive: true,
            whole_word,
            columns: None,
        }
    }

    #[test]
    fn counting_has_no_match_limit_and_stops_if_the_document_changes() {
        let manager = DocumentManager::default();
        // 12,000 lines, two matches each: well past the Find All limit of 10,000.
        let document = manager
            .create_untitled("big".into(), "cat dog cat\n".repeat(12_000))
            .expect("document");
        let id = &document.document_id;
        let (len, revision) = manager.length_and_revision(id).expect("length");
        let mut progress = nulis_search::CountProgress::starting_at(0);
        while !progress.done {
            manager
                .count_step(
                    id,
                    revision,
                    "cat",
                    options(SearchMode::Literal, false),
                    len,
                    &mut progress,
                    &CancellationToken::default(),
                )
                .expect("step");
        }
        assert_eq!((progress.matches, progress.lines), (24_000, 12_000));

        manager.replace(id, revision, 0, 3, "cow").expect("edit");
        let mut progress = nulis_search::CountProgress::starting_at(0);
        let error = manager
            .count_step(
                id,
                revision,
                "cat",
                options(SearchMode::Literal, false),
                len,
                &mut progress,
                &CancellationToken::default(),
            )
            .expect_err("changed");
        assert!(
            error.to_string().contains("changed while counting"),
            "{error}"
        );
    }

    #[test]
    fn matches_in_range_returns_what_starts_in_the_visible_range() {
        let manager = DocumentManager::default();
        let document = manager
            .create_untitled("test".into(), "cat cat\ncat catalog\ncat".into())
            .expect("document");
        // Bytes 4..15 cover "cat\ncat ca": matches at 4 and 8, and the
        // "cat" of "catalog" at 12, which runs past the end of the range.
        let found = manager
            .matches_in_range(
                &document.document_id,
                "cat",
                options(SearchMode::Literal, false),
                4,
                15,
            )
            .expect("range");
        let starts: Vec<_> = found.matches.iter().map(|found| found.start_byte).collect();
        assert_eq!(starts, vec![4, 8, 12]);
        assert!(!found.truncated);
        let words = manager
            .matches_in_range(
                &document.document_id,
                "cat",
                options(SearchMode::Regex, true),
                0,
                30,
            )
            .expect("whole words");
        assert_eq!(words.matches.len(), 4);
    }

    #[test]
    fn find_all_reports_lines_previews_and_counts() {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), "let a = 1;\r\nx\rlet let\nlettuce".into())
            .expect("document")
            .document_id;
        let all = manager
            .find_all(&id, "let", options(SearchMode::Literal, false), None, 100)
            .expect("find all");
        assert_eq!(all.matches.len(), 4);
        let lines = all
            .lines
            .iter()
            .map(|line| (line.line, line.text.as_str(), line.matches))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![(1, "let a = 1;", 1), (3, "let let", 2), (4, "lettuce", 1)]
        );
        assert!(!all.truncated);

        let words = manager
            .find_all(&id, "let", options(SearchMode::Literal, true), None, 100)
            .expect("whole words");
        assert_eq!(words.matches.len(), 3, "lettuce is not the word let");
        let regex = manager
            .find_all(&id, "l[e]t", options(SearchMode::Regex, true), None, 100)
            .expect("whole-word regex");
        assert_eq!(regex.matches.len(), 3);

        let limited = manager
            .find_all(&id, "let", options(SearchMode::Literal, false), None, 2)
            .expect("capped");
        assert_eq!(limited.matches.len(), 2);
        assert!(limited.truncated);

        let inside = manager
            .find_all(
                &id,
                "let",
                options(SearchMode::Literal, false),
                Some((12, 22)),
                100,
            )
            .expect("within");
        assert_eq!(inside.matches.len(), 2);
        assert_eq!(inside.matches[0].line, 3);
    }

    #[test]
    fn replace_all_within_a_range_touches_only_that_range() {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), "a a a a".into())
            .expect("document")
            .document_id;
        let revision = manager.summary(&id).expect("summary").revision;
        let result = manager
            .replace_all(
                &id,
                revision,
                "a",
                "b",
                SearchOptions::default(),
                Some((2, 5)),
            )
            .expect("replace");
        assert_eq!(result.replacements, 2);
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            "a b b a"
        );
        manager.undo(&id).expect("undo");
        assert_eq!(
            manager.markdown_preview_source(&id).expect("text").text,
            "a a a a"
        );
    }

    #[test]
    fn bookmark_all_adds_line_starts_and_checks_bounds() {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), "one\ntwo".into())
            .expect("document")
            .document_id;
        manager
            .set_selections(&id, vec![Selection::caret(1)])
            .expect("caret");
        manager.toggle_bookmark(&id).expect("bookmark");
        assert_eq!(
            manager.add_bookmarks(&id, &[0, 4]).expect("add"),
            vec![0, 1, 4]
        );
        assert!(manager.add_bookmarks(&id, &[99]).is_err());
    }
}
