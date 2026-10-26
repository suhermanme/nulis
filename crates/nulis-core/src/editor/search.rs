//! Find and replace in a document, and Find in Files across a folder.

use super::{
    encode_text, find_all, record_bookmark_edit, relocate_bookmarks, replace_range, summary,
    validate_revision, DocumentId, DocumentManager, DocumentSummary, EditorError, FileSearchScope,
    FindDirection, FindResult, ReplaceAllResult, Selection, TextEncoding, WorkspaceSearchMatch,
    WorkspaceSearchResult, MAX_WORKSPACE_SEARCH_FILES, MAX_WORKSPACE_SEARCH_RESULTS,
};
use nulis_large_file::{FileSource, ReadAt};
use nulis_search::{
    expand_replacements, plan_replacements_each, search, validate_pattern, CancellationToken,
    SearchMatch, SearchMode, SearchOptions, MAX_PAGE_MATCHES,
};
use std::fs;
use std::path::Path;

impl DocumentManager {
    /// Replaces one search match, the way Replace All would: with regular
    /// expressions, group references in `replacement` are filled in from
    /// this match. The range must still be exactly a match of `query`.
    ///
    /// # Errors
    /// Returns an error for a stale revision, an invalid pattern or
    /// replacement, a range that is no longer a match, or an edit too large.
    #[allow(clippy::too_many_arguments)] // One search request, as the command sends it.
    pub fn replace_match(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        query: &str,
        replacement: &str,
        options: SearchOptions,
        start_byte: u64,
        end_byte: u64,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let pattern = search_pattern(session.encoding, query, options)?;
        let replacement = encode_text(session.encoding, replacement)?;
        let page = search(
            session.document.table(),
            &pattern,
            options,
            start_byte.min(session.document.table().len()),
            1,
            &CancellationToken::default(),
        )?;
        let found = page
            .matches
            .into_iter()
            .next()
            .filter(|found| found.range.start.0 == start_byte && found.range.end.0 == end_byte)
            .ok_or(nulis_search::SearchError::MatchChanged)?;
        let mut texts = expand_replacements(
            session.document.table(),
            &pattern,
            options,
            std::slice::from_ref(&found),
            &replacement,
        )?;
        let text = texts.pop().unwrap_or_default();
        replace_range(session, start_byte, end_byte, &text)?;
        Ok(summary(document_id, session))
    }

    /// Finds one match relative to a byte position and makes it the primary
    /// selection. Scanning uses fixed-size source windows.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid pattern, or source read.
    pub fn find(
        &self,
        document_id: &DocumentId,
        query: &str,
        options: SearchOptions,
        start_byte: u64,
        direction: FindDirection,
        wrap: bool,
    ) -> Result<FindResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let document_len = session.document.table().len();
        if start_byte > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: start_byte,
                document_len,
            });
        }
        let pattern = search_pattern(session.encoding, query, options)?;
        let cancellation = CancellationToken::default();
        let (found, wrapped) = match direction {
            FindDirection::Next => {
                let direct = first_search_match(
                    session.document.table(),
                    &pattern,
                    options,
                    start_byte,
                    &cancellation,
                )?;
                if direct.is_some() || !wrap || start_byte == 0 {
                    (direct, false)
                } else {
                    (
                        first_search_match(
                            session.document.table(),
                            &pattern,
                            options,
                            0,
                            &cancellation,
                        )?,
                        true,
                    )
                }
            }
            FindDirection::Previous => {
                let direct = last_search_match_before(
                    session.document.table(),
                    &pattern,
                    options,
                    start_byte,
                    &cancellation,
                )?;
                if direct.is_some() || !wrap || start_byte == document_len {
                    (direct, false)
                } else {
                    (
                        last_search_match_before(
                            session.document.table(),
                            &pattern,
                            options,
                            document_len.saturating_add(1),
                            &cancellation,
                        )?,
                        true,
                    )
                }
            }
        };
        let selection = found.map(|found| Selection {
            anchor_byte: found.range.start.0,
            active_byte: found.range.end.0,
        });
        if let Some(selection) = selection {
            session.selections = vec![selection];
            session.rectangular_selection = None;
            session.preferred_scalar_columns = vec![None];
        }
        Ok(FindResult {
            revision: session.document.table().revision(),
            selection,
            wrapped,
        })
    }

    /// Replaces every match as one revision and one undo action. Replacement
    /// count and inserted bytes are hard-capped by the search planner.
    ///
    /// # Errors
    /// Returns an error for stale revisions, invalid patterns, excessive match
    /// counts, unrepresentable replacement text, or backing-store failure.
    pub fn replace_all(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        query: &str,
        replacement: &str,
        options: SearchOptions,
        within: Option<(u64, u64)>,
    ) -> Result<ReplaceAllResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let pattern = search_pattern(session.encoding, query, options)?;
        let replacement = encode_text(session.encoding, replacement)?;
        let len = session.document.table().len();
        let (start, end) = within.map_or((0, len), |(start, end)| {
            (start.min(len), end.min(len).max(start))
        });
        let mut page = search(
            session.document.table(),
            &pattern,
            options,
            start,
            MAX_PAGE_MATCHES,
            &CancellationToken::default(),
        )?;
        let inside = page
            .matches
            .iter()
            .take_while(|found| found.range.end.0 <= end)
            .count();
        // An incomplete page is fine when it already reached past the range.
        if !page.complete && inside == page.matches.len() {
            return Err(EditorError::TooManySearchMatches(MAX_PAGE_MATCHES));
        }
        page.matches.truncate(inside);
        if page.matches.is_empty() {
            return Ok(ReplaceAllResult {
                document: summary(document_id, session),
                replacements: 0,
            });
        }
        let texts = expand_replacements(
            session.document.table(),
            &pattern,
            options,
            &page.matches,
            &replacement,
        )?;
        let batch = plan_replacements_each(&page.matches, &texts)?;
        session.document.replace_many(&batch)?;
        let bookmark_edits = page
            .matches
            .iter()
            .zip(&texts)
            .map(|(found, text)| (found.range.start.0, found.range.end.0, text.len()))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(session, bookmark_edits);
        let first = &page.matches[0];
        let caret = first
            .range
            .start
            .0
            .checked_add(u64::try_from(texts[0].len()).map_err(|_| EditorError::OffsetOverflow)?)
            .ok_or(EditorError::OffsetOverflow)?;
        session.selections = vec![Selection::caret(caret)];
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None];
        session.navigation_index.reset();
        Ok(ReplaceAllResult {
            document: summary(document_id, session),
            replacements: page.matches.len(),
        })
    }

    /// Recursively searches regular files below a selected root with bounded
    /// file and result counts. Symbolic links are never followed.
    ///
    /// # Errors
    /// Returns an error for invalid limits, roots, patterns, or directory reads.
    pub fn find_in_files(
        &self,
        root: &Path,
        query: &str,
        options: SearchOptions,
        scope: &FileSearchScope,
        max_results: usize,
    ) -> Result<WorkspaceSearchResult, EditorError> {
        if max_results == 0 || max_results > MAX_WORKSPACE_SEARCH_RESULTS {
            return Err(EditorError::WorkspaceSearchLimit(max_results));
        }
        let metadata = fs::symlink_metadata(root).map_err(|error| EditorError::io(&error))?;
        if !metadata.is_dir() && !metadata.is_file() {
            return Err(EditorError::InvalidSearchRoot(root.display().to_string()));
        }
        let pattern = query.as_bytes();
        validate_pattern(pattern, options)?;
        let mut stack = vec![(root.to_path_buf(), 0_usize)];
        let mut result = WorkspaceSearchResult {
            matches: Vec::new(),
            files_scanned: 0,
            skipped_files: 0,
            truncated: false,
        };
        while let Some((path, depth)) = stack.pop() {
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                result.skipped_files += 1;
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if depth > 0 && !scope.recursive {
                    continue;
                }
                let mut entries = fs::read_dir(&path)
                    .map_err(|error| EditorError::io(&error))?
                    .filter_map(Result::ok)
                    .map(|entry| (entry.path(), depth + 1))
                    .collect::<Vec<_>>();
                entries.sort_by(|left, right| right.0.cmp(&left.0));
                if stack.len().saturating_add(entries.len()) > MAX_WORKSPACE_SEARCH_FILES {
                    result.truncated = true;
                    entries.truncate(MAX_WORKSPACE_SEARCH_FILES.saturating_sub(stack.len()));
                }
                stack.extend(entries);
                continue;
            }
            if !metadata.is_file() || !scope.includes(&path) {
                continue;
            }
            if result.files_scanned == MAX_WORKSPACE_SEARCH_FILES {
                result.truncated = true;
                break;
            }
            let Ok(source) = FileSource::open(&path) else {
                result.skipped_files += 1;
                continue;
            };
            result.files_scanned += 1;
            let remaining = max_results - result.matches.len();
            let page = search(
                &source,
                pattern,
                options,
                0,
                remaining,
                &CancellationToken::default(),
            )?;
            let mut lines = find_all::LineTracker::at_start();
            let mut previous_line_start = None;
            let mut line_text = String::new();
            for found in page.matches {
                let (line, line_start) = lines.advance(&source, found.range.start.0)?;
                if previous_line_start != Some(line_start) {
                    line_text = find_all::line_preview(&source, TextEncoding::Utf8, line_start)?.0;
                    previous_line_start = Some(line_start);
                }
                result.matches.push(WorkspaceSearchMatch {
                    path: path.to_string_lossy().into_owned(),
                    start_byte: found.range.start.0,
                    end_byte: found.range.end.0,
                    line,
                    line_text: line_text.clone(),
                });
            }
            if result.matches.len() == max_results {
                result.truncated = !page.complete || !stack.is_empty();
                break;
            }
        }
        Ok(result)
    }
}

pub(super) fn search_pattern(
    encoding: TextEncoding,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<u8>, EditorError> {
    if options.mode == SearchMode::Regex {
        if encoding != TextEncoding::Utf8 {
            return Err(EditorError::RegexRequiresUtf8);
        }
        Ok(query.as_bytes().to_vec())
    } else {
        encode_text(encoding, query)
    }
}

fn first_search_match(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    start: u64,
    cancellation: &CancellationToken,
) -> Result<Option<SearchMatch>, EditorError> {
    Ok(search(source, pattern, options, start, 1, cancellation)?
        .matches
        .into_iter()
        .next())
}

fn last_search_match_before(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    before: u64,
    cancellation: &CancellationToken,
) -> Result<Option<SearchMatch>, EditorError> {
    let mut offset = 0_u64;
    let mut last = None;
    loop {
        let page = search(
            source,
            pattern,
            options,
            offset,
            MAX_PAGE_MATCHES,
            cancellation,
        )?;
        for found in page.matches {
            if found.range.start.0 >= before {
                return Ok(last);
            }
            last = Some(found);
        }
        let Some(next) = page.next_offset else {
            return Ok(last);
        };
        if page.complete || next <= offset || next >= before {
            return Ok(last);
        }
        offset = next;
    }
}
