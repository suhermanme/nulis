//! Line endings: which style a document uses, and converting every ending to
//! one style as a single undo step.
//!
//! The document gets one small edit per changed ending, which keeps undo
//! cheap. Bookmarks and line change markers get at most a few thousand merged
//! ranges instead, because their bookkeeping grows with the number of edits.
//! Merged ranges never contain a bookmark, so bookmarks stay exact.

use super::{
    record_bookmark_edit, relocate_bookmarks, summary, validate_revision, DocumentId,
    DocumentManager, DocumentSummary, EditorError, Selection, MAX_CHARACTER_COUNT_BYTES,
};
use nulis_document::PieceTable;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// How much of a document too large to count is read to guess its style.
pub const LINE_ENDING_SAMPLE_BYTES: u64 = 1024 * 1024;
/// Largest document whose line endings can be converted.
pub const MAX_LINE_ENDING_CONVERSION_BYTES: u64 = MAX_CHARACTER_COUNT_BYTES;
/// Most line endings one conversion changes; this bounds its memory.
pub const MAX_LINE_ENDING_CHANGES: usize = 1_000_000;
/// Most ranges one conversion hands to bookmarks and line change markers.
const MAX_TRACKED_RANGES: usize = 2_048;
const SCAN_CHUNK: u64 = 1024 * 1024;

/// One kind of line break.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    #[must_use]
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::Crlf => b"\r\n",
            Self::Cr => b"\r",
        }
    }

    const fn len(self) -> u64 {
        self.bytes().len() as u64
    }
}

/// The line endings a document uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEndingStyle {
    Lf,
    Crlf,
    Cr,
    /// More than one kind.
    Mixed,
    /// No line endings at all.
    None,
}

/// Counts line endings across chunks; a CR at the end of one chunk is
/// resolved by the first byte of the next.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LineEndingScanner {
    lf: u64,
    crlf: u64,
    cr: u64,
    after_cr: bool,
}

impl LineEndingScanner {
    pub(super) fn push(&mut self, byte: u8) {
        if self.after_cr {
            if byte == b'\n' {
                self.crlf += 1;
                self.after_cr = false;
                return;
            }
            self.cr += 1;
        } else if byte == b'\n' {
            self.lf += 1;
        }
        self.after_cr = byte == b'\r';
    }

    /// Number of line endings seen so far, including a trailing CR.
    pub(super) fn endings(&self) -> u64 {
        self.lf + self.crlf + self.cr + u64::from(self.after_cr)
    }

    pub(super) fn style(&self) -> LineEndingStyle {
        let cr = self.cr + u64::from(self.after_cr);
        match (self.lf > 0, self.crlf > 0, cr > 0) {
            (false, false, false) => LineEndingStyle::None,
            (true, false, false) => LineEndingStyle::Lf,
            (false, true, false) => LineEndingStyle::Crlf,
            (false, false, true) => LineEndingStyle::Cr,
            _ => LineEndingStyle::Mixed,
        }
    }
}

/// Guesses the style of a document from its first [`LINE_ENDING_SAMPLE_BYTES`].
pub(super) fn sample_style(table: &PieceTable) -> Result<LineEndingStyle, EditorError> {
    let length = table.len().min(LINE_ENDING_SAMPLE_BYTES);
    let mut scanner = LineEndingScanner::default();
    for &byte in &table.read_vec(0, length)? {
        scanner.push(byte);
    }
    // A CR that ends the sample may be the first half of a CRLF.
    if scanner.after_cr && length < table.len() && table.read_vec(length, 1)? == b"\n" {
        scanner.after_cr = false;
        scanner.crlf += 1;
    }
    Ok(scanner.style())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEndingConversion {
    pub document: DocumentSummary,
    /// Number of line endings that were changed.
    pub converted: usize,
    pub selections: Vec<Selection>,
}

/// A line ending to replace: where it starts and how many bytes it has.
#[derive(Clone, Copy, Debug)]
struct Ending {
    start: u64,
    len: u64,
}

impl DocumentManager {
    /// Changes every line ending to `target` as one revision and one undo
    /// step. Documents above [`MAX_LINE_ENDING_CONVERSION_BYTES`] or with more
    /// than [`MAX_LINE_ENDING_CHANGES`] endings to change are refused.
    ///
    /// # Errors
    /// Returns an error for an unknown, stale, read-only, or too large
    /// document, or when it cannot be read.
    pub fn convert_line_endings(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        target: LineEnding,
    ) -> Result<LineEndingConversion, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if session.document.is_read_only() {
            return Err(EditorError::ReadOnly);
        }
        let length = session.document.table().len();
        if length > MAX_LINE_ENDING_CONVERSION_BYTES {
            return Err(EditorError::LineEndingConversionTooLarge {
                size: length,
                maximum: MAX_LINE_ENDING_CONVERSION_BYTES,
            });
        }
        let endings = endings_to_change(session.document.table(), target)?;
        if endings.is_empty() {
            return Ok(LineEndingConversion {
                document: summary(document_id, session),
                converted: 0,
                selections: session.selections.clone(),
            });
        }

        let batch = endings
            .iter()
            .rev()
            .map(|ending| (ending.start, ending.len, target.bytes().to_vec()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        drop(batch);

        let tracked = tracked_ranges(&endings, target, &session.bookmarks);
        relocate_bookmarks(&mut session.bookmarks, &tracked)?;
        record_bookmark_edit(session, tracked);

        let mut points = session
            .selections
            .iter()
            .flat_map(|selection| [selection.anchor_byte, selection.active_byte])
            .collect::<Vec<_>>();
        points.sort_unstable();
        points.dedup();
        let mapped = map_offsets(&endings, target, &points);
        let moved = |offset: u64| mapped[points.binary_search(&offset).unwrap_or(0)];
        let mut selections = session
            .selections
            .iter()
            .map(|selection| Selection {
                anchor_byte: moved(selection.anchor_byte),
                active_byte: moved(selection.active_byte),
            })
            .collect::<Vec<_>>();
        selections = super::deduplicate_selections(selections);
        session.preferred_scalar_columns = vec![None; selections.len()];
        session.selections.clone_from(&selections);
        session.rectangular_selection = None;
        session.navigation_index.reset();
        Ok(LineEndingConversion {
            document: summary(document_id, session),
            converted: endings.len(),
            selections,
        })
    }
}

/// Every line ending that differs from `target`, in document order.
fn endings_to_change(table: &PieceTable, target: LineEnding) -> Result<Vec<Ending>, EditorError> {
    let length = table.len();
    let mut changed = Vec::new();
    let mut pending_cr: Option<u64> = None;
    let mut offset = 0_u64;
    while offset < length {
        let chunk = table.read_vec(offset, SCAN_CHUNK.min(length - offset))?;
        for (index, &byte) in chunk.iter().enumerate() {
            let position = offset + index as u64;
            if let Some(cr) = pending_cr.take() {
                if byte == b'\n' {
                    note(&mut changed, target, LineEnding::Crlf, cr)?;
                    continue;
                }
                note(&mut changed, target, LineEnding::Cr, cr)?;
            }
            match byte {
                b'\r' => pending_cr = Some(position),
                b'\n' => note(&mut changed, target, LineEnding::Lf, position)?,
                _ => {}
            }
        }
        offset += chunk.len() as u64;
    }
    if let Some(cr) = pending_cr {
        note(&mut changed, target, LineEnding::Cr, cr)?;
    }
    Ok(changed)
}

fn note(
    changed: &mut Vec<Ending>,
    target: LineEnding,
    kind: LineEnding,
    start: u64,
) -> Result<(), EditorError> {
    if kind == target {
        return Ok(());
    }
    if changed.len() == MAX_LINE_ENDING_CHANGES {
        return Err(EditorError::TooManyLineEndingChanges(
            MAX_LINE_ENDING_CHANGES,
        ));
    }
    changed.push(Ending {
        start,
        len: kind.len(),
    });
    Ok(())
}

/// Ascending `(start, end, inserted_len)` ranges describing the conversion
/// for bookmarks and line change markers: the endings themselves when there
/// are few, otherwise neighbors merged into at most about
/// [`MAX_TRACKED_RANGES`] spans, never across a bookmark.
fn tracked_ranges(
    endings: &[Ending],
    target: LineEnding,
    bookmarks: &BTreeSet<u64>,
) -> Vec<(u64, u64, usize)> {
    let new_len = target.bytes().len();
    if endings.len() <= MAX_TRACKED_RANGES {
        return endings
            .iter()
            .map(|ending| (ending.start, ending.start + ending.len, new_len))
            .collect();
    }
    let first = endings[0].start;
    let last = endings[endings.len() - 1];
    let span = (last.start + last.len - first).div_ceil(MAX_TRACKED_RANGES as u64);
    let mut ranges = Vec::new();
    // (start, end, bytes after the edit)
    let mut current = (first, first + endings[0].len, new_len as u64);
    for ending in &endings[1..] {
        let end = ending.start + ending.len;
        let crosses_bookmark = bookmarks.range(current.0 + 1..end).next().is_some();
        if end - current.0 <= span && !crosses_bookmark {
            current.2 += (ending.start - current.1) + new_len as u64;
            current.1 = end;
        } else {
            ranges.push(current);
            current = (ending.start, end, new_len as u64);
        }
    }
    ranges.push(current);
    ranges
        .into_iter()
        .map(|(start, end, inserted)| {
            (
                start,
                end,
                usize::try_from(inserted).expect("bounded by the document size"),
            )
        })
        .collect()
}

/// Where each of the ascending `offsets` lands after the conversion, in one
/// pass. An offset inside a CRLF moves to the start of its replacement.
fn map_offsets(endings: &[Ending], target: LineEnding, offsets: &[u64]) -> Vec<u64> {
    let mut mapped = Vec::with_capacity(offsets.len());
    let (mut index, mut delta) = (0_usize, 0_i128);
    for &offset in offsets {
        while let Some(ending) = endings.get(index) {
            if ending.start + ending.len > offset {
                break;
            }
            delta += i128::from(target.len()) - i128::from(ending.len);
            index += 1;
        }
        let point = match endings.get(index) {
            Some(ending) if ending.start < offset => ending.start,
            _ => offset,
        };
        mapped.push(u64::try_from(i128::from(point) + delta).unwrap_or(0));
    }
    mapped
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentId, DocumentManager, EditorError, Selection, ViewportRequest};
    use super::{
        endings_to_change, sample_style, LineEnding, LineEndingStyle, LINE_ENDING_SAMPLE_BYTES,
        SCAN_CHUNK,
    };
    use nulis_document::PieceTable;
    use std::fmt::Write as _;

    fn open(text: &str) -> (DocumentManager, DocumentId) {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("test".into(), text.into())
            .expect("document")
            .document_id;
        (manager, id)
    }

    fn text(manager: &DocumentManager, id: &DocumentId) -> String {
        manager.markdown_preview_source(id).expect("text").text
    }

    fn revision(manager: &DocumentManager, id: &DocumentId) -> u64 {
        manager.summary(id).expect("summary").revision
    }

    fn style(text: &str) -> LineEndingStyle {
        let (manager, id) = open(text);
        manager.character_count(&id).expect("count").line_ending
    }

    fn convert(manager: &DocumentManager, id: &DocumentId, target: LineEnding) -> usize {
        manager
            .convert_line_endings(id, revision(manager, id), target)
            .expect("convert")
            .converted
    }

    fn caret_at(manager: &DocumentManager, id: &DocumentId, offset: u64) {
        manager
            .set_selections(id, vec![Selection::caret(offset)])
            .expect("caret");
    }

    fn bookmarks(manager: &DocumentManager, id: &DocumentId) -> Vec<u64> {
        manager
            .viewport(&ViewportRequest {
                document_id: id.clone(),
                start_byte: 0,
                start_line: 0,
                line_continuation: false,
                max_bytes: 4096,
            })
            .expect("viewport")
            .bookmarks
    }

    #[test]
    fn detects_each_style() {
        assert_eq!(style("a\nb\n"), LineEndingStyle::Lf);
        assert_eq!(style("a\r\nb\r\n"), LineEndingStyle::Crlf);
        assert_eq!(style("a\rb\r"), LineEndingStyle::Cr);
        assert_eq!(style("a\nb\r\n"), LineEndingStyle::Mixed);
        assert_eq!(style("abc"), LineEndingStyle::None);
        assert_eq!(style(""), LineEndingStyle::None);
    }

    #[test]
    fn a_sample_that_ends_inside_a_crlf_is_still_crlf() {
        let mut bytes = vec![b'a'; usize::try_from(LINE_ENDING_SAMPLE_BYTES).unwrap() - 1];
        bytes.extend_from_slice(b"\r\nb\r\n");
        assert_eq!(
            sample_style(&PieceTable::from_bytes(bytes)).unwrap(),
            LineEndingStyle::Crlf
        );
    }

    #[test]
    fn converts_mixed_endings_in_one_undo_step() {
        let (manager, id) = open("one\ntwo\r\nthree\rfour");
        assert_eq!(convert(&manager, &id, LineEnding::Crlf), 2);
        assert_eq!(text(&manager, &id), "one\r\ntwo\r\nthree\r\nfour");
        assert_eq!(convert(&manager, &id, LineEnding::Crlf), 0);
        assert_eq!(convert(&manager, &id, LineEnding::Lf), 3);
        assert_eq!(text(&manager, &id), "one\ntwo\nthree\nfour");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "one\r\ntwo\r\nthree\r\nfour");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "one\ntwo\r\nthree\rfour");
        manager.redo(&id).expect("redo");
        assert_eq!(text(&manager, &id), "one\r\ntwo\r\nthree\r\nfour");
        assert_eq!(convert(&manager, &id, LineEnding::Cr), 3);
        assert_eq!(text(&manager, &id), "one\rtwo\rthree\rfour");
    }

    #[test]
    fn a_crlf_split_across_scan_chunks_counts_once() {
        let mut bytes = vec![b'a'; usize::try_from(SCAN_CHUNK).unwrap() - 1];
        bytes.extend_from_slice(b"\r\nb\r");
        let table = PieceTable::from_bytes(bytes);
        let crlf = endings_to_change(&table, LineEnding::Lf).unwrap();
        let starts = crlf.iter().map(|e| (e.start, e.len)).collect::<Vec<_>>();
        assert_eq!(starts, vec![(SCAN_CHUNK - 1, 2), (SCAN_CHUNK + 2, 1)]);
    }

    #[test]
    fn caret_and_bookmarks_follow_their_lines() {
        let (manager, id) = open("one\r\ntwo\r\nthree\r\n");
        caret_at(&manager, &id, 5);
        manager.toggle_bookmark(&id).expect("bookmark");
        caret_at(&manager, &id, 12);
        manager.toggle_bookmark(&id).expect("bookmark");
        assert_eq!(bookmarks(&manager, &id), vec![5, 12]);
        let result = manager
            .convert_line_endings(&id, revision(&manager, &id), LineEnding::Lf)
            .expect("convert");
        // The caret was two characters into "three".
        assert_eq!(result.selections, vec![Selection::caret(10)]);
        assert_eq!(bookmarks(&manager, &id), vec![4, 10]);
        manager.undo(&id).expect("undo");
        assert_eq!(bookmarks(&manager, &id), vec![5, 12]);
    }

    #[test]
    fn many_lines_keep_bookmarks_exact_and_markers_available() {
        let lines = 10_000_u64;
        let mut source = String::new();
        for line in 0..lines {
            write!(source, "{line:05}\r\n").expect("write to a string");
        }
        let (manager, id) = open(&source);
        // Bookmark the start of line 1234 and the middle of 7777 (7 bytes a line).
        for offset in [1234 * 7, 7777 * 7 + 3] {
            caret_at(&manager, &id, offset);
            manager.toggle_bookmark(&id).expect("bookmark");
        }
        assert_eq!(convert(&manager, &id, LineEnding::Lf), 10_000);
        assert_eq!(text(&manager, &id).len(), 60_000);
        let after = manager.bookmark_lines(&id).expect("bookmarks");
        let texts = after
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, vec!["01234", "07777"]);
        let snapshot = manager
            .viewport(&ViewportRequest {
                document_id: id.clone(),
                start_byte: 0,
                start_line: 0,
                line_continuation: false,
                max_bytes: 4096,
            })
            .expect("viewport");
        assert!(snapshot.line_changes.available);
        assert!(snapshot.lines[0].change.is_some());
        manager.undo(&id).expect("undo");
        let undone = manager.bookmark_lines(&id).expect("bookmarks");
        let texts = undone
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, vec!["01234", "07777"]);
        assert_eq!(text(&manager, &id).len(), 70_000);
    }

    #[test]
    fn refuses_stale_and_read_only_documents() {
        let (manager, id) = open("a\nb");
        let stale = revision(&manager, &id) + 1;
        assert!(matches!(
            manager.convert_line_endings(&id, stale, LineEnding::Crlf),
            Err(EditorError::StaleRevision { .. })
        ));
        manager.toggle_read_only(&id).expect("read only");
        assert_eq!(
            manager.convert_line_endings(&id, revision(&manager, &id), LineEnding::Crlf),
            Err(EditorError::ReadOnly)
        );
    }

    #[test]
    fn viewport_lines_report_their_endings() {
        let (manager, id) = open("a\nb\r\nc\rd");
        let snapshot = manager
            .viewport(&ViewportRequest {
                document_id: id.clone(),
                start_byte: 0,
                start_line: 0,
                line_continuation: false,
                max_bytes: 4096,
            })
            .expect("viewport");
        let endings = snapshot
            .lines
            .iter()
            .map(|line| line.ending)
            .collect::<Vec<_>>();
        assert_eq!(
            endings,
            vec![
                Some(LineEnding::Lf),
                Some(LineEnding::Crlf),
                Some(LineEnding::Cr),
                None
            ]
        );
    }
}
