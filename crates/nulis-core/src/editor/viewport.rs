//! What the interface draws: bounded viewports of text, the document map,
//! and the Markdown preview's source and images.

use super::{
    decode_text, find_line_start, line_number_at_offset, line_offset_for_number, read_byte,
    ChangeTracker, DocumentId, DocumentManager, DocumentMapChange, DocumentMapSample,
    DocumentMapSnapshot, EditorError, EditorSession, LineChange, LineEnding, MarkdownPreviewSource,
    SparseNavigationIndex, SyntaxBracketPair, SyntaxFold, SyntaxHighlight, SyntaxSnapshot,
    SyntaxSymbol, TextEncoding, ViewportAtRequest, ViewportCursor, ViewportLine,
    ViewportLineRequest, ViewportRequest, ViewportSnapshot, BASE64, DOCUMENT_MAP_FULL_BYTES,
    DOCUMENT_MAP_SAMPLE_BYTES, MAX_DOCUMENT_MAP_SAMPLES, MAX_MARKDOWN_IMAGE_BYTES,
    MAX_MARKDOWN_PREVIEW_BYTES, MAX_VIEWPORT_BYTES,
};
use base64::Engine;
use nulis_document::PieceTable;
use nulis_syntax::{LanguageId, LanguageRegistry, SyntaxAnalysis};
use std::fs;
use std::path::Path;

impl DocumentManager {
    /// Returns a bounded text snapshot. The cursor makes continuation of a
    /// very long line explicit without ever expanding the byte limit.
    ///
    /// # Errors
    /// Returns an error for unknown documents, invalid offsets, or oversized requests.
    pub fn viewport(&self, request: &ViewportRequest) -> Result<ViewportSnapshot, EditorError> {
        validate_viewport_limit(request.max_bytes)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(&request.document_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.document_id.clone()))?;
        let snapshot = session_viewport(
            session,
            &self.languages(),
            &request.document_id,
            request.start_byte,
            request.start_line,
            request.line_continuation,
            request.max_bytes,
        )?;
        record_snapshot_lines(&mut session.navigation_index, &snapshot);
        Ok(snapshot)
    }

    /// Returns a bounded viewport that reveals an arbitrary byte offset. The
    /// line number is recovered from the nearest sparse checkpoint with fixed-size reads.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid offset, or oversized request.
    pub fn viewport_at(
        &self,
        request: &ViewportAtRequest,
    ) -> Result<ViewportSnapshot, EditorError> {
        validate_viewport_limit(request.max_bytes)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(&request.document_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.document_id.clone()))?;
        let document_len = session.document.table().len();
        if request.byte_offset > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: request.byte_offset,
                document_len,
            });
        }
        let line_start = find_line_start(session.document.table(), request.byte_offset)?;
        let line = line_number_at_offset(
            session.document.table(),
            &session.navigation_index,
            line_start,
        )?;
        let reveal_margin = request.max_bytes / 2;
        let candidate = request.byte_offset.saturating_sub(reveal_margin);
        let start_byte = if candidate > line_start && session.encoding == TextEncoding::Utf8 {
            align_utf8_start(session.document.table(), candidate, request.byte_offset)?
        } else {
            line_start
        };
        let snapshot = session_viewport(
            session,
            &self.languages(),
            &request.document_id,
            start_byte,
            line,
            start_byte > line_start,
            request.max_bytes,
        )?;
        record_snapshot_lines(&mut session.navigation_index, &snapshot);
        Ok(snapshot)
    }

    /// Returns a bounded overview sampled directly from the document's backing store.
    ///
    /// # Errors
    /// Returns an error for an unknown document or backing-store read failure.
    pub fn document_map(
        &self,
        document_id: &DocumentId,
    ) -> Result<DocumentMapSnapshot, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let length = table.len();
        let mut samples = Vec::new();
        let mut changes = Vec::new();
        let mut total_lines = None;
        if length <= DOCUMENT_MAP_FULL_BYTES {
            let bytes = table.read_vec(0, length)?;
            changes = full_map_changes(&session.changes, &bytes);
            let line_count = bytes.split(|byte| *byte == b'\n').count();
            total_lines = Some(line_count);
            let step = line_count.div_ceil(MAX_DOCUMENT_MAP_SAMPLES).max(1);
            let mut start = 0_usize;
            for (index, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
                if index % step == 0 {
                    samples.push(DocumentMapSample {
                        byte_offset: u64::try_from(start)
                            .map_err(|_| EditorError::OffsetOverflow)?,
                        line: u64::try_from(index + 1).map_err(|_| EditorError::OffsetOverflow)?,
                        text: document_map_text(session.encoding, line),
                    });
                }
                start += line.len();
            }
            if bytes.is_empty() {
                samples.push(DocumentMapSample {
                    byte_offset: 0,
                    line: 1,
                    text: String::new(),
                });
            } else if bytes.last() == Some(&b'\n') && (line_count - 1) % step == 0 {
                samples.push(DocumentMapSample {
                    byte_offset: length,
                    line: u64::try_from(line_count).map_err(|_| EditorError::OffsetOverflow)?,
                    text: String::new(),
                });
            }
        } else {
            for index in 0..MAX_DOCUMENT_MAP_SAMPLES {
                let offset = u64::try_from(
                    u128::from(length) * index as u128 / MAX_DOCUMENT_MAP_SAMPLES as u128,
                )
                .map_err(|_| EditorError::OffsetOverflow)?;
                let sample = large_map_sample(table, session.encoding, offset)?;
                // Long lines can make neighbors land on the same line.
                if samples
                    .last()
                    .is_none_or(|last: &DocumentMapSample| last.byte_offset < sample.byte_offset)
                {
                    samples.push(sample);
                }
            }
            changes.extend(
                session
                    .changes
                    .ranges()
                    .iter()
                    .map(|range| DocumentMapChange {
                        byte_offset: range.start,
                        line: 0,
                        change: match (range.saved, range.start == range.end) {
                            (false, false) => LineChange::Unsaved,
                            (false, true) => LineChange::UnsavedDeletion,
                            (true, false) => LineChange::Saved,
                            (true, true) => LineChange::SavedDeletion,
                        },
                    }),
            );
        }
        if changes.len() > MAX_DOCUMENT_MAP_SAMPLES {
            let step = changes.len().div_ceil(MAX_DOCUMENT_MAP_SAMPLES);
            changes = changes.into_iter().step_by(step).collect();
        }
        Ok(DocumentMapSnapshot {
            document_id: document_id.clone(),
            revision: table.revision(),
            document_byte_len: length,
            total_lines,
            samples,
            changes,
            line_changes: session.changes.status(),
        })
    }

    /// Reads the current text, including unsaved changes, for a bounded Markdown preview.
    ///
    /// # Errors
    /// Returns an error for an unknown document, a file above the preview limit, or a backing-store failure.
    pub fn markdown_preview_source(
        &self,
        document_id: &DocumentId,
    ) -> Result<MarkdownPreviewSource, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let length = table.len();
        if length > MAX_MARKDOWN_PREVIEW_BYTES {
            return Err(EditorError::MarkdownPreviewTooLarge {
                size: length,
                maximum: MAX_MARKDOWN_PREVIEW_BYTES,
            });
        }
        let bytes = table.read_vec(0, length)?;
        Ok(MarkdownPreviewSource {
            document_id: document_id.clone(),
            revision: table.revision(),
            text: decode_text(session.encoding, &bytes).0,
        })
    }

    /// Reads a relative raster image alongside a saved Markdown document.
    ///
    /// # Errors
    /// Returns an error for an unsaved document, a path outside the document directory,
    /// an unsupported image type, or an oversized image.
    pub fn markdown_preview_image(
        &self,
        document_id: &DocumentId,
        relative_path: &str,
    ) -> Result<String, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let source = session
            .source_path
            .as_ref()
            .ok_or(EditorError::SavePathRequired)?;
        let directory = source
            .parent()
            .ok_or(EditorError::SavePathRequired)?
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        let relative = Path::new(relative_path);
        // `has_root` also rejects `/name`, which Windows does not count as absolute.
        if relative.is_absolute()
            || relative.has_root()
            || relative_path.contains(':')
            || relative_path.contains('\\')
        {
            return Err(EditorError::InvalidPreviewImage(relative_path.to_owned()));
        }
        let path = directory
            .join(relative)
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        if !path.starts_with(&directory) {
            return Err(EditorError::InvalidPreviewImage(relative_path.to_owned()));
        }
        let mime = match path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => return Err(EditorError::InvalidPreviewImage(relative_path.to_owned())),
        };
        let metadata = fs::metadata(&path).map_err(|error| EditorError::io(&error))?;
        if !metadata.is_file() || metadata.len() > MAX_MARKDOWN_IMAGE_BYTES {
            return Err(EditorError::InvalidPreviewImage(relative_path.to_owned()));
        }
        let bytes = fs::read(&path).map_err(|error| EditorError::io(&error))?;
        if bytes.len() as u64 > MAX_MARKDOWN_IMAGE_BYTES {
            return Err(EditorError::InvalidPreviewImage(relative_path.to_owned()));
        }
        Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
    }

    /// Locates a one-based logical line from the nearest sparse checkpoint and
    /// returns a bounded viewport beginning there.
    ///
    /// # Errors
    /// Returns an error for an unknown document, line zero, an out-of-range line,
    /// or a backing-store read failure.
    pub fn viewport_line(
        &self,
        request: &ViewportLineRequest,
    ) -> Result<ViewportSnapshot, EditorError> {
        validate_viewport_limit(request.max_bytes)?;
        if request.line == 0 {
            return Err(EditorError::LineOutOfRange(0));
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(&request.document_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.document_id.clone()))?;
        let start_byte = line_offset_for_number(
            session.document.table(),
            &session.navigation_index,
            request.line,
        )?
        .ok_or(EditorError::LineOutOfRange(request.line))?;
        let snapshot = session_viewport(
            session,
            &self.languages(),
            &request.document_id,
            start_byte,
            request.line,
            false,
            request.max_bytes,
        )?;
        record_snapshot_lines(&mut session.navigation_index, &snapshot);
        Ok(snapshot)
    }
}

fn syntax_snapshot(analysis: SyntaxAnalysis) -> SyntaxSnapshot {
    SyntaxSnapshot {
        language_id: analysis.language_id.as_str().to_owned(),
        parsed_bytes: analysis.parsed_bytes,
        highlights: analysis
            .highlights
            .into_iter()
            .map(|span| SyntaxHighlight {
                start_byte: span.start_byte,
                end_byte: span.end_byte,
                token: span.token,
            })
            .collect(),
        folds: analysis
            .folds
            .into_iter()
            .map(|fold| SyntaxFold {
                start_byte: fold.start_byte,
                end_byte: fold.end_byte,
                start_line: fold.start_line,
                end_line: fold.end_line,
            })
            .collect(),
        symbols: analysis
            .symbols
            .into_iter()
            .map(|symbol| SyntaxSymbol {
                name: symbol.name,
                kind: symbol.kind,
                start_byte: symbol.start_byte,
                line: symbol.line,
            })
            .collect(),
        bracket_pairs: analysis
            .bracket_pairs
            .into_iter()
            .map(|pair| SyntaxBracketPair {
                open_byte: pair.open_byte,
                close_byte: pair.close_byte,
            })
            .collect(),
        truncated: analysis.truncated,
        timed_out: analysis.timed_out,
    }
}

/// The first whole line at or after `offset` in a large document, read within
/// [`DOCUMENT_MAP_SAMPLE_BYTES`]; a line longer than that is shown from `offset`.
fn large_map_sample(
    table: &PieceTable,
    encoding: TextEncoding,
    offset: u64,
) -> Result<DocumentMapSample, EditorError> {
    let length = table.len();
    let from = offset.saturating_sub(1);
    let window = table.read_vec(from, DOCUMENT_MAP_SAMPLE_BYTES.min(length - from))?;
    // A line ends at LF, or at a CR that is not the first half of a CRLF.
    let ends_line = |index: usize| match window.get(index) {
        Some(b'\n') => true,
        Some(b'\r') => window.get(index + 1) != Some(&b'\n'),
        _ => false,
    };
    let skip = if offset == 0 {
        0
    } else if ends_line(0) {
        1
    } else {
        (1..window.len())
            .find(|index| ends_line(*index))
            .map_or(1, |index| index + 1)
    };
    let rest = &window[skip.min(window.len())..];
    let line = rest
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(rest, |end| &rest[..end]);
    Ok(DocumentMapSample {
        byte_offset: from + skip as u64,
        line: 0,
        text: document_map_text(encoding, line),
    })
}

fn document_map_text(encoding: TextEncoding, bytes: &[u8]) -> String {
    decode_text(encoding, bytes)
        .0
        .chars()
        .take(90)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn validate_viewport_limit(max_bytes: u64) -> Result<(), EditorError> {
    if (2..=MAX_VIEWPORT_BYTES).contains(&max_bytes) {
        Ok(())
    } else {
        Err(EditorError::ViewportLimit(max_bytes))
    }
}

fn session_viewport(
    session: &EditorSession,
    languages: &LanguageRegistry,
    document_id: &DocumentId,
    start_byte: u64,
    start_line: u64,
    line_continuation: bool,
    max_bytes: u64,
) -> Result<ViewportSnapshot, EditorError> {
    validate_viewport_limit(max_bytes)?;
    let document_len = session.document.table().len();
    if start_byte > document_len {
        return Err(EditorError::OffsetOutOfRange {
            offset: start_byte,
            document_len,
        });
    }
    let read_len = max_bytes.min(document_len - start_byte);
    let bytes = session.document.table().read_vec(start_byte, read_len)?;
    let (mut lines, next) = viewport_lines(
        &bytes,
        start_byte,
        start_line,
        line_continuation,
        document_len,
        session.encoding,
    );
    let table = session.document.table();
    let is_line_start = |offset: u64| {
        offset == 0
            || table
                .read_vec(offset - 1, 1)
                .is_ok_and(|byte| matches!(byte.first(), Some(b'\n' | b'\r')))
    };
    for line in &mut lines {
        let text_len: u64 = line.source_byte_widths.iter().map(|w| u64::from(*w)).sum();
        let last_line =
            line.end_byte == document_len && line.start_byte + text_len == line.end_byte;
        line.change =
            session
                .changes
                .line_change(line.start_byte, line.end_byte, last_line, &is_line_start);
    }
    // A language that is no longer registered (a removed plugin) shows as
    // Plain Text; the document keeps its language ID and stays readable.
    let analysis = match languages.analyze(&session.language_id, &bytes, start_byte, start_line) {
        Err(nulis_syntax::SyntaxError::UnknownLanguage(_)) => languages.analyze(
            &LanguageId::new("plaintext")?,
            &bytes,
            start_byte,
            start_line,
        ),
        other => other,
    };
    let syntax = analysis.map(syntax_snapshot)?;
    Ok(ViewportSnapshot {
        document_id: document_id.clone(),
        revision: session.document.table().revision(),
        document_byte_len: document_len,
        encoding: session.encoding,
        lines,
        selections: session.selections.clone(),
        rectangular_selection: session.rectangular_selection,
        syntax,
        bookmarks: session.bookmarks.iter().copied().collect(),
        line_changes: session.changes.status(),
        next,
    })
}

/// Line change ticks for a document small enough to be read whole.
fn full_map_changes(tracker: &ChangeTracker, bytes: &[u8]) -> Vec<DocumentMapChange> {
    let mut changes = Vec::new();
    if tracker.ranges().is_empty() {
        return changes;
    }
    let length = bytes.len() as u64;
    let is_line_start = |offset: u64| {
        offset == 0
            || usize::try_from(offset - 1)
                .ok()
                .and_then(|index| bytes.get(index))
                .is_some_and(|byte| *byte == b'\n')
    };
    let mut start = 0_u64;
    let mut lines = bytes.split_inclusive(|byte| *byte == b'\n').peekable();
    let mut number = 0_u64;
    while let Some(line) = lines.next() {
        number += 1;
        let end = start + line.len() as u64;
        let last_line = lines.peek().is_none() && line.last() != Some(&b'\n');
        if let Some(change) = tracker.line_change(start, end, last_line, &is_line_start) {
            changes.push(DocumentMapChange {
                byte_offset: start,
                line: number,
                change,
            });
        }
        start = end;
    }
    if bytes.is_empty() || bytes.last() == Some(&b'\n') {
        if let Some(change) = tracker.line_change(length, length, true, &is_line_start) {
            changes.push(DocumentMapChange {
                byte_offset: length,
                line: number + 1,
                change,
            });
        }
    }
    changes
}

fn record_snapshot_lines(index: &mut SparseNavigationIndex, snapshot: &ViewportSnapshot) {
    for line in &snapshot.lines {
        if !line.continuation {
            index.record(line.start_byte, line.line);
        }
    }
    if let Some(next) = &snapshot.next {
        if !next.line_continuation {
            index.record(next.byte_offset, next.line);
        }
    }
}

fn align_utf8_start(
    table: &PieceTable,
    mut candidate: u64,
    maximum: u64,
) -> Result<u64, EditorError> {
    while candidate < maximum {
        let byte = read_byte(table, candidate)?;
        if byte & 0b1100_0000 != 0b1000_0000 {
            break;
        }
        candidate = candidate
            .checked_add(1)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(candidate)
}

pub(super) fn viewport_lines(
    bytes: &[u8],
    source_start: u64,
    start_line: u64,
    starts_with_continuation: bool,
    document_len: u64,
    encoding: TextEncoding,
) -> (Vec<ViewportLine>, Option<ViewportCursor>) {
    let mut lines = Vec::new();
    let mut segment_start = 0_usize;
    let mut line_number = start_line;
    let mut continuation = starts_with_continuation;
    let mut cursor = 0_usize;
    let mut consumed_len = bytes.len();
    while cursor < bytes.len() {
        let ending_len = match bytes[cursor] {
            b'\r'
                if cursor + 1 == bytes.len()
                    && source_start + u64::try_from(bytes.len()).expect("usize fits into u64")
                        < document_len =>
            {
                consumed_len = cursor;
                break;
            }
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => 2,
            b'\n' | b'\r' => 1,
            _ => {
                cursor += 1;
                continue;
            }
        };
        push_viewport_line(
            &mut lines,
            bytes,
            source_start,
            segment_start,
            cursor,
            line_number,
            continuation,
            false,
            ending_len,
            encoding,
        );
        cursor += ending_len;
        segment_start = cursor;
        line_number = line_number.saturating_add(1);
        continuation = false;
    }

    let consumed_end = source_start + u64::try_from(consumed_len).expect("usize fits into u64");
    if segment_start < consumed_len {
        push_viewport_line(
            &mut lines,
            bytes,
            source_start,
            segment_start,
            consumed_len,
            line_number,
            continuation,
            consumed_end < document_len,
            0,
            encoding,
        );
    } else if consumed_end == document_len
        && (bytes.is_empty() || (consumed_len > 0 && segment_start == consumed_len))
    {
        // A document ending in a line break has one final empty logical line.
        // Keep that row in an ordinary viewport so a caret created by Enter
        // does not require replacing the surrounding viewport with an EOF-only
        // reveal.
        lines.push(ViewportLine {
            line: line_number,
            start_byte: consumed_end,
            end_byte: consumed_end,
            text: String::new(),
            source_byte_widths: Vec::new(),
            continuation: bytes.is_empty() && starts_with_continuation,
            truncated: false,
            change: None,
            ending: None,
        });
    }

    let next = (consumed_end < document_len).then_some(ViewportCursor {
        byte_offset: consumed_end,
        line: line_number,
        line_continuation: segment_start < consumed_len,
    });
    (lines, next)
}

#[allow(clippy::too_many_arguments)]
fn push_viewport_line(
    output: &mut Vec<ViewportLine>,
    bytes: &[u8],
    source_start: u64,
    segment_start: usize,
    text_end: usize,
    line: u64,
    continuation: bool,
    truncated: bool,
    ending_len: usize,
    encoding: TextEncoding,
) {
    let absolute_start = source_start + u64::try_from(segment_start).expect("usize fits into u64");
    let absolute_end = source_start
        + u64::try_from(text_end + ending_len).expect("bounded viewport fits into u64");
    let (text, source_byte_widths) = decode_text(encoding, &bytes[segment_start..text_end]);
    let ending = match ending_len {
        0 => None,
        2 => Some(LineEnding::Crlf),
        _ if bytes[text_end] == b'\r' => Some(LineEnding::Cr),
        _ => Some(LineEnding::Lf),
    };
    output.push(ViewportLine {
        line,
        start_byte: absolute_start,
        end_byte: absolute_end,
        text,
        source_byte_widths,
        continuation,
        truncated,
        change: None,
        ending,
    });
}
