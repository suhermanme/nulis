//! Reading a document for printing, and Export to PDF (`IMPROVEMENT_PLAN_3.md`).
//!
//! The document is read in windows of at most [`PRINT_WINDOW_BYTES`], each
//! under the session lock for only as long as one window takes, and each
//! checked against the revision the export started from. Editing during an
//! export therefore never blocks, and an edit stops the export instead of
//! mixing two versions of the text.

use super::{
    decimal_u64, viewport_lines, DocumentId, DocumentManager, EditorError, TextEncoding,
    ViewportCursor, ViewportLine,
};
use nulis_print::{
    export_pdf, BandFields, ColorChoice, ExportOptions, LineSource, PageSetup, PrintError,
    SourceLine, Span,
};
use nulis_syntax::HighlightSpan;
use serde::Serialize;
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Bytes read per window: the most one syntax analysis covers.
pub const PRINT_WINDOW_BYTES: u64 = 256 * 1024;
/// Bytes scanned per step when counting lines.
const LINE_COUNT_WINDOW_BYTES: u64 = 4 * 1024 * 1024;

/// What to export and how.
#[derive(Clone, Debug)]
pub struct PdfExportRequest {
    pub path: PathBuf,
    pub expected_revision: u64,
    pub setup: PageSetup,
    pub colors: ColorChoice,
    /// The date for `{date}`, already formatted by the desktop.
    pub date: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfExportResult {
    pub path: String,
    #[serde(with = "decimal_u64")]
    pub pages: u64,
    /// Characters left blank because the font lacks them.
    #[serde(with = "decimal_u64")]
    pub missing_glyphs: u64,
    #[serde(with = "decimal_u64")]
    pub bytes: u64,
}

impl DocumentManager {
    /// Exports the document as it is at `expected_revision`, unsaved edits
    /// included, to a PDF file at `request.path`. The file is written next to
    /// its destination and moved into place only when complete.
    ///
    /// # Errors
    /// Returns an error for a stale revision, an edit made during the export,
    /// a destination that is the document's own file, an unusable page setup,
    /// or a failed write.
    pub fn export_pdf(
        &self,
        document_id: &DocumentId,
        request: &PdfExportRequest,
    ) -> Result<PdfExportResult, EditorError> {
        let (name, source_path) = self.print_identity(document_id, request.expected_revision)?;
        if !request.path.is_absolute() {
            return Err(EditorError::Export("choose where to save the PDF".into()));
        }
        if source_path
            .as_deref()
            .is_some_and(|source| same_file(source, &request.path))
        {
            return Err(EditorError::Export(
                "the PDF cannot replace the document it is made from".into(),
            ));
        }
        let directory = request
            .path
            .parent()
            .filter(|directory| directory.is_dir())
            .ok_or_else(|| EditorError::Export("the folder for the PDF does not exist".into()))?;
        let options = ExportOptions {
            setup: request.setup.clone(),
            palette: request.colors.palette().map_err(export_error)?,
            fields: BandFields {
                path: source_path
                    .as_ref()
                    .map_or_else(|| name.clone(), |path| path.display().to_string()),
                file: name.clone(),
                date: request.date.clone(),
                pages: None,
            },
            title: name,
            last_line: self.print_line_count(document_id, request.expected_revision)?,
        };

        let mut file = tempfile::Builder::new()
            .prefix(".nulis-export-")
            .suffix(".pdf")
            .tempfile_in(directory)
            .map_err(|error| EditorError::io(&error))?;
        let mut source = DocumentLines::new(self, document_id.clone(), request.expected_revision);
        let (writer, summary) =
            export_pdf(&mut source, BufWriter::new(file.as_file_mut()), &options)
                .map_err(export_error)?;
        writer
            .into_inner()
            .map_err(|error| EditorError::io(error.error()))?
            .flush()
            .map_err(|error| EditorError::io(&error))?;
        file.as_file()
            .sync_all()
            .map_err(|error| EditorError::io(&error))?;
        file.persist(&request.path)
            .map_err(|error| EditorError::io(&error.error))?;
        Ok(PdfExportResult {
            path: request.path.display().to_string(),
            pages: summary.pages,
            missing_glyphs: summary.missing_glyphs,
            bytes: summary.bytes,
        })
    }

    /// The document's name and file, checking it is still at `revision`.
    fn print_identity(
        &self,
        document_id: &DocumentId,
        revision: u64,
    ) -> Result<(String, Option<PathBuf>), EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        check_revision(session.document.table().revision(), revision)?;
        Ok((session.name.clone(), session.source_path.clone()))
    }

    /// Lines in the document at `revision`: one more than its line breaks
    /// (CRLF, LF, or CR), as the editor counts them.
    ///
    /// # Errors
    /// Returns an error for an unknown document or a changed revision.
    pub fn print_line_count(
        &self,
        document_id: &DocumentId,
        revision: u64,
    ) -> Result<u64, EditorError> {
        let mut offset = 0_u64;
        let mut breaks = 0_u64;
        let mut after_cr = false;
        loop {
            let bytes =
                self.read_at_revision(document_id, revision, offset, LINE_COUNT_WINDOW_BYTES)?;
            if bytes.is_empty() {
                return Ok(breaks + 1);
            }
            for byte in &bytes {
                match byte {
                    b'\n' if after_cr => after_cr = false,
                    b'\n' => breaks += 1,
                    b'\r' => {
                        breaks += 1;
                        after_cr = true;
                    }
                    _ => after_cr = false,
                }
            }
            offset += u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        }
    }

    fn read_at_revision(
        &self,
        document_id: &DocumentId,
        revision: u64,
        offset: u64,
        max_bytes: u64,
    ) -> Result<Vec<u8>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        check_revision(table.revision(), revision)?;
        let len = table.len();
        if offset >= len {
            return Ok(Vec::new());
        }
        Ok(table.read_vec(offset, max_bytes.min(len - offset))?)
    }

    /// One window of lines from `cursor`, with highlights when asked, and
    /// where the next window starts (`None` after the last line).
    ///
    /// # Errors
    /// Returns an error for an unknown document or a changed revision.
    pub fn print_window(
        &self,
        document_id: &DocumentId,
        revision: u64,
        cursor: &ViewportCursor,
        highlight: bool,
    ) -> Result<(Vec<SourceLine>, Option<ViewportCursor>), EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        check_revision(table.revision(), revision)?;
        let len = table.len();
        let offset = cursor.byte_offset.min(len);
        let mut bytes = table.read_vec(offset, PRINT_WINDOW_BYTES.min(len - offset))?;
        let window_end = offset + u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if window_end < len {
            cut_at_boundary(&mut bytes, session.encoding);
        }
        let (lines, next) = viewport_lines(
            &bytes,
            offset,
            cursor.line,
            cursor.line_continuation,
            len,
            session.encoding,
        );
        let mut highlights = if highlight {
            self.languages
                .analyze(&session.language_id, &bytes, offset, cursor.line)
                .map(|analysis| analysis.highlights)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        highlights.sort_by_key(|span| span.start_byte);
        Ok((source_lines(&lines, &highlights), next))
    }
}

fn check_revision(actual: u64, expected: u64) -> Result<(), EditorError> {
    if actual == expected {
        Ok(())
    } else {
        Err(EditorError::StaleRevision { expected, actual })
    }
}

#[allow(clippy::needless_pass_by_value)] // `Result::map_err` transfers ownership.
fn export_error(error: PrintError) -> EditorError {
    EditorError::Export(error.to_string())
}

/// Whether two paths name the same file (also when one does not exist yet).
fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Ends a window that stops inside the document after its last line break,
/// or else at a whole character, so no character is split between windows.
fn cut_at_boundary(bytes: &mut Vec<u8>, encoding: TextEncoding) {
    if let Some(last_break) = bytes.iter().rposition(|byte| *byte == b'\n') {
        bytes.truncate(last_break + 1);
        return;
    }
    if encoding != TextEncoding::Utf8 {
        return;
    }
    let len = bytes.len();
    for back in 1..=len.min(3) {
        let byte = bytes[len - back];
        if byte & 0xC0 == 0x80 {
            continue;
        }
        let needed = match byte {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        };
        if needed > back && len > back {
            bytes.truncate(len - back);
        }
        return;
    }
}

/// Viewport lines with their highlights as character ranges.
fn source_lines(lines: &[ViewportLine], highlights: &[HighlightSpan]) -> Vec<SourceLine> {
    let mut first = 0;
    lines
        .iter()
        .map(|line| {
            let mut starts = Vec::with_capacity(line.source_byte_widths.len());
            let mut position = line.start_byte;
            for width in &line.source_byte_widths {
                starts.push(position);
                position += u64::from(*width);
            }
            let text_end = position;
            while highlights
                .get(first)
                .is_some_and(|span| span.end_byte <= line.start_byte)
            {
                first += 1;
            }
            let character_at = |byte: u64| starts.partition_point(|start| *start < byte);
            let spans = highlights[first.min(highlights.len())..]
                .iter()
                .take_while(|span| span.start_byte < text_end)
                .filter(|span| span.end_byte > line.start_byte)
                .filter_map(|span| {
                    let start = character_at(span.start_byte.max(line.start_byte));
                    let end = character_at(span.end_byte.min(text_end));
                    (start < end).then_some(Span {
                        start,
                        end,
                        token: span.token,
                    })
                })
                .collect();
            SourceLine {
                number: line.line,
                continuation: line.continuation,
                text: line.text.clone(),
                spans,
            }
        })
        .collect()
}

/// The document as a [`LineSource`], read window by window.
struct DocumentLines<'a> {
    manager: &'a DocumentManager,
    document_id: DocumentId,
    revision: u64,
    cursor: Option<ViewportCursor>,
}

impl<'a> DocumentLines<'a> {
    fn new(manager: &'a DocumentManager, document_id: DocumentId, revision: u64) -> Self {
        Self {
            manager,
            document_id,
            revision,
            cursor: Some(start_cursor()),
        }
    }
}

fn start_cursor() -> ViewportCursor {
    ViewportCursor {
        byte_offset: 0,
        line: 1,
        line_continuation: false,
    }
}

impl LineSource for DocumentLines<'_> {
    fn rewind(&mut self) -> Result<(), PrintError> {
        self.cursor = Some(start_cursor());
        Ok(())
    }

    fn next_lines(&mut self, highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError> {
        let Some(cursor) = self.cursor.take() else {
            return Ok(None);
        };
        let (lines, next) = self
            .manager
            .print_window(&self.document_id, self.revision, &cursor, highlight)
            .map_err(|error| match error {
                EditorError::StaleRevision { .. } => PrintError::Source(
                    "the document changed during the export; export it again".into(),
                ),
                other => PrintError::Source(other.to_string()),
            })?;
        if let Some(next) = &next {
            if next.byte_offset <= cursor.byte_offset {
                return Err(PrintError::Source("reading stopped advancing".into()));
            }
        }
        self.cursor = next;
        Ok(Some(lines))
    }
}

#[cfg(test)]
mod tests {
    use super::{cut_at_boundary, TextEncoding};

    #[test]
    fn windows_end_after_a_line_break_or_at_a_whole_character() {
        let mut bytes = b"one\ntwo\nthr".to_vec();
        cut_at_boundary(&mut bytes, TextEncoding::Utf8);
        assert_eq!(bytes, b"one\ntwo\n");
        // "aé" with the second byte of é missing.
        let mut bytes = vec![b'a', 0xC3];
        cut_at_boundary(&mut bytes, TextEncoding::Utf8);
        assert_eq!(bytes, b"a");
        let mut bytes = "a€".as_bytes().to_vec();
        bytes.pop();
        cut_at_boundary(&mut bytes, TextEncoding::Utf8);
        assert_eq!(bytes, b"a");
        let mut whole = "a€".as_bytes().to_vec();
        cut_at_boundary(&mut whole, TextEncoding::Utf8);
        assert_eq!(whole, "a€".as_bytes());
        let mut single_byte = vec![b'a', 0xC3];
        cut_at_boundary(&mut single_byte, TextEncoding::Windows1252);
        assert_eq!(single_byte, [b'a', 0xC3]);
    }
}
