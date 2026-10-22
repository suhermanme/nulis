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
use nulis_syntax::SyntaxToken;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Bytes read per window without highlighting.
pub const PRINT_WINDOW_BYTES: u64 = 256 * 1024;
/// Bytes read per window with highlighting: small enough that dense code
/// stays under the analyzer's span limit (`nulis_syntax::MAX_HIGHLIGHT_SPANS`).
pub const HIGHLIGHT_WINDOW_BYTES: u64 = 64 * 1024;
/// Text before a window that is analyzed with it, so a comment or string
/// that began shortly before the window edge keeps its color.
pub const HIGHLIGHT_LOOKBACK_BYTES: u64 = 16 * 1024;
/// Bytes scanned per step when counting lines.
const LINE_COUNT_WINDOW_BYTES: u64 = 4 * 1024 * 1024;

/// Documents up to this size have their lines counted for the Page Setup
/// preview's gutter; larger ones are previewed without counting.
pub const PREVIEW_COUNT_BYTES: u64 = 8 * 1024 * 1024;

/// Asks before exporting more pages than this, unless confirmed.
pub const LARGE_EXPORT_PAGES: u64 = 1_000;

/// Which part of the export is running.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportPhase {
    #[default]
    CountingLines,
    CountingPages,
    Writing,
}

/// How far an export has got, for a progress bar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub phase: ExportPhase,
    /// Work done and total, in bytes read over all passes.
    #[serde(with = "decimal_u64")]
    pub done: u64,
    #[serde(with = "decimal_u64")]
    pub total: u64,
    #[serde(with = "decimal_u64")]
    pub pages_written: u64,
    /// The fewest pages the export can take (see [`nulis_print::estimate_pages`]).
    #[serde(with = "decimal_u64")]
    pub estimated_pages: u64,
}

/// Follows an export: progress reports and a way to stop it.
pub trait ExportObserver: Sync {
    fn cancelled(&self) -> bool;
    fn report(&self, progress: ExportProgress);
}

/// Follows nothing: an export that runs to the end.
#[derive(Debug, Default)]
pub struct Unobserved;

impl ExportObserver for Unobserved {
    fn cancelled(&self) -> bool {
        false
    }

    fn report(&self, _progress: ExportProgress) {}
}

/// Which text an export covers.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportScope {
    #[default]
    Document,
    /// Every selected range in order, with a rule between them. A column
    /// selection covers the whole lines it spans.
    Selection,
    /// The bytes as a hexadecimal dump, as the hexadecimal editor shows them.
    HexDump,
}

/// A byte range to print and the line it starts on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PrintRange {
    start: u64,
    end: u64,
    first_line: u64,
    lines: u64,
    /// The range ends just after a line break: the empty line after it is not printed.
    drop_final_empty: bool,
}

/// What to export and how.
#[derive(Clone, Debug)]
pub struct PdfExportRequest {
    pub path: PathBuf,
    pub expected_revision: u64,
    pub setup: PageSetup,
    pub colors: ColorChoice,
    /// The date for `{date}`, already formatted by the desktop.
    pub date: String,
    /// Ask first ([`EditorError::ExportNeedsConfirmation`]) above this many
    /// pages; `None` exports whatever the size.
    pub confirm_above: Option<u64>,
    pub scope: ExportScope,
}

impl PdfExportRequest {
    fn layout(&self) -> PrintRequest {
        PrintRequest {
            expected_revision: self.expected_revision,
            setup: self.setup.clone(),
            colors: self.colors.clone(),
            date: self.date.clone(),
            confirm_above: self.confirm_above,
            max_pages: None,
            scope: self.scope,
        }
    }
}

/// What to lay out for Print.
#[derive(Clone, Debug)]
pub struct PrintRequest {
    pub expected_revision: u64,
    pub setup: PageSetup,
    pub colors: ColorChoice,
    /// The date for `{date}`, already formatted by the desktop.
    pub date: String,
    /// Ask first ([`EditorError::ExportNeedsConfirmation`]) above this many pages.
    pub confirm_above: Option<u64>,
    /// Refuse more pages than this.
    pub max_pages: Option<u64>,
    pub scope: ExportScope,
}

/// Most pages Print lays out: the Print Preview window holds every page.
pub const MAX_PRINT_PAGES: u64 = 5_000;

/// A document laid out for the Print Preview window.
#[derive(Clone, Debug)]
pub struct PrintedDocument {
    pub title: String,
    pub frame: nulis_print::PageFrame,
    /// Each page as JSON (a [`nulis_print::PrintedPage`]), so a finished
    /// layout is held compactly until the window reads it.
    pub pages: Vec<String>,
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
        observer: &dyn ExportObserver,
    ) -> Result<PdfExportResult, EditorError> {
        let (_, source_path) = self.print_identity(document_id, request.expected_revision)?;
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
        let (mut source, options) = self.print_source(document_id, &request.layout(), observer)?;
        write_pdf(directory, &request.path, &mut source, &options)
    }

    /// Lays out the document as it is at `expected_revision`, unsaved edits
    /// included, as pages for the Print Preview window.
    ///
    /// # Errors
    /// Returns an error for a stale revision, an edit made during the
    /// layout, an unusable page setup, or more pages than
    /// `request.max_pages`.
    pub fn print_pages(
        &self,
        document_id: &DocumentId,
        request: &PrintRequest,
        observer: &dyn ExportObserver,
    ) -> Result<PrintedDocument, EditorError> {
        let (mut source, options) = self.print_source(document_id, request, observer)?;
        let limit = request.max_pages.unwrap_or(u64::MAX);
        let mut pages = Vec::new();
        let frame = nulis_print::print_pages(&mut source, &options, |page| {
            if page.number > limit {
                return Err(PrintError::Setup(too_many_pages(limit, page.number)));
            }
            pages.push(
                serde_json::to_string(&page)
                    .map_err(|error| PrintError::Source(error.to_string()))?,
            );
            Ok(())
        })
        .map_err(export_error)?;
        Ok(PrintedDocument {
            title: options.title,
            frame,
            pages,
        })
    }

    /// The first page of the document for the Page Setup preview. It is
    /// quick whatever the document's size: only the first page is laid out,
    /// and `{pages}` shows an estimate. In a document over
    /// [`PREVIEW_COUNT_BYTES`] the line numbers are not counted, so the
    /// gutter is as wide as for ten million lines.
    ///
    /// # Errors
    /// Returns an error for a stale revision or an unusable page setup.
    pub fn print_preview(
        &self,
        document_id: &DocumentId,
        request: &PrintRequest,
    ) -> Result<(nulis_print::PageFrame, nulis_print::PrintedPage), EditorError> {
        let request = PrintRequest {
            confirm_above: None,
            max_pages: None,
            ..request.clone()
        };
        let (len, _) = self.length_and_revision(document_id)?;
        let (mut source, mut options) =
            if request.scope == ExportScope::HexDump || len <= PREVIEW_COUNT_BYTES {
                self.print_source(document_id, &request, &Unobserved)?
            } else {
                self.uncounted_source(document_id, &request, len)?
            };
        let rows = match &source {
            PrintSource::Text(text) => text.ranges.iter().map(|range| range.lines).sum(),
            PrintSource::Hex(_) => options.last_line,
        };
        options.fields.pages = Some(
            nulis_print::estimate_pages(&options.setup, rows, options.last_line)
                .map_err(export_error)?,
        );
        let frame = nulis_print::page_frame(&options).map_err(export_error)?;
        let mut first = None;
        let outcome = nulis_print::print_pages(&mut source, &options, |page| {
            first = Some(page);
            // Stop after the first page.
            Err(PrintError::Cancelled)
        });
        match (outcome, first) {
            (_, Some(page)) => Ok((frame, page)),
            (Err(error), None) => Err(export_error(error)),
            (Ok(_), None) => Err(EditorError::Export("there is nothing to preview".into())),
        }
    }

    /// The whole document as one range, without counting its lines first.
    fn uncounted_source<'a>(
        &'a self,
        document_id: &DocumentId,
        request: &PrintRequest,
        len: u64,
    ) -> Result<(PrintSource<'a>, ExportOptions), EditorError> {
        let (name, source_path) = self.print_identity(document_id, request.expected_revision)?;
        let last_line = 9_999_999;
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
            last_line,
            fallback_fonts: false,
        };
        let source = DocumentLines {
            manager: self,
            document_id: document_id.clone(),
            revision: request.expected_revision,
            ranges: vec![PrintRange {
                start: 0,
                end: len,
                first_line: 1,
                lines: last_line,
                drop_final_empty: false,
            }],
            index: 0,
            cursor: None,
            separator_pending: false,
            observer: &Unobserved,
            progress: ExportProgress::default(),
            passes: 0,
            counting_pages: false,
            scanned: 0,
            range_bytes: len,
            finished_bytes: 0,
        };
        Ok((PrintSource::Text(source), options))
    }

    /// The lines `request` covers and how to lay them out, after checking
    /// the revision and asking about (or refusing) very many pages.
    fn print_source<'a>(
        &'a self,
        document_id: &DocumentId,
        request: &PrintRequest,
        observer: &'a dyn ExportObserver,
    ) -> Result<(PrintSource<'a>, ExportOptions), EditorError> {
        let (name, source_path) = self.print_identity(document_id, request.expected_revision)?;
        let fields = BandFields {
            path: source_path
                .as_ref()
                .map_or_else(|| name.clone(), |path| path.display().to_string()),
            file: name.clone(),
            date: request.date.clone(),
            pages: None,
        };
        let base = ExportBase {
            fields,
            palette: request.colors.palette().map_err(export_error)?,
            title: name,
            counting_pages: request.setup.header.uses_page_count()
                || request.setup.footer.uses_page_count(),
        };
        if request.scope == ExportScope::HexDump {
            self.hex_source(document_id, request, observer, base)
        } else {
            self.text_source(document_id, request, observer, base)
        }
    }

    /// The hexadecimal dump: 16 bytes a row, without line numbers.
    fn hex_source<'a>(
        &'a self,
        document_id: &DocumentId,
        request: &PrintRequest,
        observer: &'a dyn ExportObserver,
        base: ExportBase,
    ) -> Result<(PrintSource<'a>, ExportOptions), EditorError> {
        let (len, _) = self.length_and_revision(document_id)?;
        let rows = len.div_ceil(HEX_ROW_BYTES).max(1);
        let setup = PageSetup {
            line_numbers: false,
            long_lines: nulis_print::LongLines::Cut,
            syntax_highlighting: true,
            ..request.setup.clone()
        };
        let estimated_pages =
            nulis_print::estimate_pages(&setup, rows, rows).map_err(export_error)?;
        check_size(request, estimated_pages)?;
        let progress = ExportProgress {
            total: len * (1 + u64::from(base.counting_pages)),
            estimated_pages,
            ..ExportProgress::default()
        };
        observer.report(progress);
        let options = ExportOptions {
            setup,
            palette: base.palette,
            fields: base.fields,
            title: base.title,
            last_line: rows,
            fallback_fonts: true,
        };
        let source = HexLines {
            manager: self,
            document_id: document_id.clone(),
            revision: request.expected_revision,
            len,
            offset: None,
            observer,
            progress,
            passes: 0,
            counting_pages: base.counting_pages,
        };
        Ok((PrintSource::Hex(source), options))
    }

    /// The text of the whole document or of the selections.
    fn text_source<'a>(
        &'a self,
        document_id: &DocumentId,
        request: &PrintRequest,
        observer: &'a dyn ExportObserver,
        base: ExportBase,
    ) -> Result<(PrintSource<'a>, ExportOptions), EditorError> {
        let counting_pages = base.counting_pages;
        let spans = self.print_spans(document_id, request.expected_revision, request.scope)?;
        let scanned = spans.last().map_or(0, |span| span.1);
        let range_bytes: u64 = spans.iter().map(|(start, end)| end - start).sum();
        let mut progress = ExportProgress {
            total: scanned.max(1),
            ..ExportProgress::default()
        };
        observer.report(progress);
        let ranges = self.print_ranges(document_id, request.expected_revision, &spans, observer)?;
        let last_line = ranges
            .iter()
            .map(|range| range.first_line + range.lines.saturating_sub(1))
            .max()
            .unwrap_or(1);
        let rows = ranges.iter().map(|range| range.lines).sum::<u64>()
            + u64::try_from(ranges.len().saturating_sub(1)).unwrap_or(0);
        progress.estimated_pages =
            nulis_print::estimate_pages(&request.setup, rows, last_line).map_err(export_error)?;
        check_size(request, progress.estimated_pages)?;
        progress.done = scanned;
        progress.total = scanned + range_bytes * (1 + u64::from(counting_pages));
        observer.report(progress);
        let options = ExportOptions {
            setup: request.setup.clone(),
            palette: base.palette,
            fields: base.fields,
            title: base.title,
            last_line,
            fallback_fonts: true,
        };
        let source = DocumentLines {
            manager: self,
            document_id: document_id.clone(),
            revision: request.expected_revision,
            ranges,
            index: 0,
            cursor: None,
            separator_pending: false,
            observer,
            progress,
            passes: 0,
            counting_pages,
            scanned,
            range_bytes,
            finished_bytes: 0,
        };
        Ok((PrintSource::Text(source), options))
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

    /// The byte ranges `scope` covers: the whole document, or the
    /// selections (sorted, merged, empty ones left out).
    fn print_spans(
        &self,
        document_id: &DocumentId,
        revision: u64,
        scope: ExportScope,
    ) -> Result<Vec<(u64, u64)>, EditorError> {
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
        if scope == ExportScope::Document {
            return Ok(vec![(0, len)]);
        }
        let mut spans: Vec<(u64, u64)> = if let Some(block) = session.rectangular_selection {
            let start = block.anchor_byte.min(block.active_byte);
            let end = block.anchor_byte.max(block.active_byte);
            vec![(line_start(table, start)?, line_end(table, end, len)?)]
        } else {
            session
                .selections
                .iter()
                .map(|selection| {
                    let start = selection.anchor_byte.min(selection.active_byte);
                    let end = selection.anchor_byte.max(selection.active_byte);
                    (start.min(len), end.min(len))
                })
                .filter(|(start, end)| start < end)
                .collect()
        };
        spans.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(spans.len());
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        if merged.is_empty() {
            return Err(EditorError::Export(
                "select the text to export first".into(),
            ));
        }
        Ok(merged)
    }

    /// Line numbers for each span, counted in one pass from the start of the
    /// document to the end of the last span. A line is ended by CRLF, LF,
    /// or CR, as the editor counts them.
    fn print_ranges(
        &self,
        document_id: &DocumentId,
        revision: u64,
        spans: &[(u64, u64)],
        observer: &dyn ExportObserver,
    ) -> Result<Vec<PrintRange>, EditorError> {
        let scan_end = spans.last().map_or(0, |span| span.1);
        let (len, _) = self.length_and_revision(document_id)?;
        // Line breaks completed before each span's start and end.
        let mut marks: Vec<u64> = spans
            .iter()
            .flat_map(|(start, end)| [*start, *end])
            .collect();
        marks.sort_unstable();
        marks.dedup();
        let mut breaks_at = std::collections::HashMap::with_capacity(marks.len());
        let mut next_mark = 0;
        let mut offset = 0_u64;
        let mut breaks = 0_u64;
        let mut after_cr = false;
        let mut ends_with_break = std::collections::HashMap::new();
        let mut previous_was_break = false;
        loop {
            while marks.get(next_mark).is_some_and(|mark| *mark <= offset) {
                breaks_at.insert(marks[next_mark], breaks);
                ends_with_break.insert(marks[next_mark], previous_was_break);
                next_mark += 1;
            }
            if offset >= scan_end {
                break;
            }
            let window = (scan_end - offset).min(LINE_COUNT_WINDOW_BYTES);
            let bytes = self.read_at_revision(document_id, revision, offset, window)?;
            if bytes.is_empty() {
                break;
            }
            for byte in &bytes {
                while marks.get(next_mark).is_some_and(|mark| *mark <= offset) {
                    breaks_at.insert(marks[next_mark], breaks);
                    ends_with_break.insert(marks[next_mark], previous_was_break);
                    next_mark += 1;
                }
                match byte {
                    b'\n' if after_cr => after_cr = false,
                    b'\n' => breaks += 1,
                    b'\r' => {
                        breaks += 1;
                        after_cr = true;
                    }
                    _ => after_cr = false,
                }
                previous_was_break = matches!(byte, b'\n' | b'\r');
                offset += 1;
            }
            if observer.cancelled() {
                return Err(EditorError::ExportCancelled);
            }
            observer.report(ExportProgress {
                done: offset,
                total: scan_end.max(1),
                ..ExportProgress::default()
            });
        }
        Ok(spans
            .iter()
            .map(|(start, end)| {
                let before = breaks_at.get(start).copied().unwrap_or(0);
                let through = breaks_at.get(end).copied().unwrap_or(before);
                // A selection ending just after a line break does not print
                // the empty line after it; the document's own last line does.
                let drop_final_empty = *end < len
                    && *end > *start
                    && ends_with_break.get(end).copied().unwrap_or(false);
                PrintRange {
                    start: *start,
                    end: *end,
                    first_line: before + 1,
                    lines: through - before + 1 - u64::from(drop_final_empty),
                    drop_final_empty,
                }
            })
            .collect())
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
        self.range_window(document_id, revision, cursor, highlight, None, false)
    }

    /// [`Self::print_window`] that stops at `end` (the document's end when
    /// `None`), optionally without the empty line after a final line break.
    fn range_window(
        &self,
        document_id: &DocumentId,
        revision: u64,
        cursor: &ViewportCursor,
        highlight: bool,
        end: Option<u64>,
        drop_final_empty: bool,
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
        let len = end.unwrap_or(u64::MAX).min(table.len());
        let offset = cursor.byte_offset.min(len);
        let limit = if highlight {
            HIGHLIGHT_WINDOW_BYTES
        } else {
            PRINT_WINDOW_BYTES
        };
        let mut bytes = table.read_vec(offset, limit.min(len - offset))?;
        let window_end = offset + u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if window_end < len {
            cut_at_boundary(&mut bytes, session.encoding);
        }
        let (mut lines, next) = viewport_lines(
            &bytes,
            offset,
            cursor.line,
            cursor.line_continuation,
            len,
            session.encoding,
        );
        if next.is_none()
            && drop_final_empty
            && lines.len() > 1
            && lines
                .last()
                .is_some_and(|line| line.text.is_empty() && !line.continuation)
        {
            lines.pop();
        }
        let mut highlights = if highlight {
            // Start the analysis at a line start up to the lookback earlier.
            let back = offset.saturating_sub(HIGHLIGHT_LOOKBACK_BYTES);
            let before = table.read_vec(back, offset - back)?;
            let skip = if back == 0 {
                0
            } else {
                before
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(before.len(), |index| index + 1)
            };
            let context = &before[skip..];
            #[allow(clippy::naive_bytecount)] // At most the 16 KiB lookback.
            let context_lines =
                u64::try_from(context.iter().filter(|byte| **byte == b'\n').count()).unwrap_or(0);
            let mut source = Vec::with_capacity(context.len() + bytes.len());
            source.extend_from_slice(context);
            source.extend_from_slice(&bytes);
            let base = offset - u64::try_from(context.len()).unwrap_or(0);
            self.languages
                .analyze(
                    &session.language_id,
                    &source,
                    base,
                    cursor.line.saturating_sub(context_lines).max(1),
                )
                .map(|analysis| analysis.highlights)
                .unwrap_or_default()
                .into_iter()
                .filter(|span| span.end_byte > offset)
                .collect()
        } else {
            Vec::new()
        };
        highlights.sort_by_key(|span| span.start_byte);
        Ok((source_lines(&lines, &highlights), next))
    }
}

/// What every export path needs besides its lines.
#[derive(Debug)]
struct ExportBase {
    fields: BandFields,
    palette: nulis_print::Palette,
    title: String,
    counting_pages: bool,
}

/// Bytes per row of a hexadecimal dump, as in the hexadecimal editor.
const HEX_ROW_BYTES: u64 = 16;

/// Refuses above `request.max_pages`, and asks first
/// ([`EditorError::ExportNeedsConfirmation`]) above `request.confirm_above`.
fn check_size(request: &PrintRequest, estimated_pages: u64) -> Result<(), EditorError> {
    if let Some(limit) = request.max_pages.filter(|limit| estimated_pages > *limit) {
        return Err(EditorError::Export(too_many_pages(limit, estimated_pages)));
    }
    match request.confirm_above {
        Some(limit) if estimated_pages > limit => {
            Err(EditorError::ExportNeedsConfirmation { estimated_pages })
        }
        _ => Ok(()),
    }
}

/// Why Print refuses a document that needs at least `pages` pages.
fn too_many_pages(limit: u64, pages: u64) -> String {
    format!(
        "Print takes at most {} pages, and this needs at least {}; use Export to PDF instead",
        grouped(limit),
        grouped(pages)
    )
}

/// `1234567` as `1,234,567`.
fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Either kind of printed lines.
enum PrintSource<'a> {
    Text(DocumentLines<'a>),
    Hex(HexLines<'a>),
}

impl LineSource for PrintSource<'_> {
    fn rewind(&mut self) -> Result<(), PrintError> {
        match self {
            Self::Text(source) => source.rewind(),
            Self::Hex(source) => source.rewind(),
        }
    }

    fn next_lines(&mut self, highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError> {
        match self {
            Self::Text(source) => source.next_lines(highlight),
            Self::Hex(source) => source.next_lines(highlight),
        }
    }

    fn page_written(&mut self, number: u64) {
        match self {
            Self::Text(source) => source.page_written(number),
            Self::Hex(source) => source.page_written(number),
        }
    }
}

/// Lays out `source` and writes the PDF to `path` through a temporary file.
fn write_pdf<S: LineSource>(
    directory: &Path,
    path: &Path,
    source: &mut S,
    options: &ExportOptions,
) -> Result<PdfExportResult, EditorError> {
    let summary = write_atomically(directory, path, |out| {
        export_pdf(source, out, options)
            .map(|(_, summary)| summary)
            .map_err(export_error)
    })?;
    Ok(PdfExportResult {
        path: path.display().to_string(),
        pages: summary.pages,
        missing_glyphs: summary.missing_glyphs,
        bytes: summary.bytes,
    })
}

/// Writes a file through `write` into a temporary file in `directory`, then
/// syncs it and moves it to `path`. On any error the temporary file is removed.
fn write_atomically<T>(
    directory: &Path,
    path: &Path,
    write: impl FnOnce(&mut BufWriter<&mut std::fs::File>) -> Result<T, EditorError>,
) -> Result<T, EditorError> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".nulis-export-").suffix(".pdf");
    // A temporary file is private (0600) and keeps that mode when it is moved
    // into place. Ask for 0666 so the umask gives the PDF the same mode as any
    // other new file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut file = builder
        .tempfile_in(directory)
        .map_err(|error| EditorError::io(&error))?;
    let mut out = BufWriter::new(file.as_file_mut());
    let value = write(&mut out)?;
    out.into_inner()
        .map_err(|error| EditorError::io(error.error()))?
        .flush()
        .map_err(|error| EditorError::io(&error))?;
    file.as_file()
        .sync_all()
        .map_err(|error| EditorError::io(&error))?;
    file.persist(path)
        .map_err(|error| EditorError::io(&error.error))?;
    Ok(value)
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
    match error {
        PrintError::Cancelled => EditorError::ExportCancelled,
        other => EditorError::Export(other.to_string()),
    }
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
                separator_before: false,
                text: line.text.clone(),
                spans,
            }
        })
        .collect()
}

/// Bytes read per step when looking for the start or end of a line.
const LINE_EDGE_STEP_BYTES: u64 = 64 * 1024;

/// The start of the line holding `offset`.
fn line_start(table: &nulis_document::PieceTable, offset: u64) -> Result<u64, EditorError> {
    let mut end = offset;
    while end > 0 {
        let start = end.saturating_sub(LINE_EDGE_STEP_BYTES);
        let bytes = table.read_vec(start, end - start)?;
        if let Some(index) = bytes.iter().rposition(|byte| matches!(byte, b'\n' | b'\r')) {
            return Ok(start + u64::try_from(index).unwrap_or(0) + 1);
        }
        end = start;
    }
    Ok(0)
}

/// The end of the line holding `offset`, after its line break.
fn line_end(table: &nulis_document::PieceTable, offset: u64, len: u64) -> Result<u64, EditorError> {
    let mut start = offset;
    while start < len {
        let bytes = table.read_vec(start, LINE_EDGE_STEP_BYTES.min(len - start))?;
        if let Some(index) = bytes.iter().position(|byte| matches!(byte, b'\n' | b'\r')) {
            let at = start + u64::try_from(index).unwrap_or(0);
            let crlf = bytes.get(index) == Some(&b'\r')
                && (bytes.get(index + 1) == Some(&b'\n')
                    || (index + 1 == bytes.len()
                        && table.read_vec(at + 1, 1)?.first() == Some(&b'\n')));
            return Ok((at + 1 + u64::from(crlf)).min(len));
        }
        start += u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    }
    Ok(len)
}

/// The ranges to print as a [`LineSource`], read window by window,
/// reporting progress and stopping when the observer cancels.
struct DocumentLines<'a> {
    manager: &'a DocumentManager,
    document_id: DocumentId,
    revision: u64,
    ranges: Vec<PrintRange>,
    /// The range being read.
    index: usize,
    /// `None` before the first rewind and after the last range.
    cursor: Option<ViewportCursor>,
    /// The next line returned starts a new range.
    separator_pending: bool,
    observer: &'a dyn ExportObserver,
    progress: ExportProgress,
    /// Rewinds so far: 1 is the first pass over the ranges.
    passes: u64,
    counting_pages: bool,
    /// Bytes the line count scanned.
    scanned: u64,
    /// Bytes in all ranges: one pass.
    range_bytes: u64,
    /// Bytes of the ranges already finished in this pass.
    finished_bytes: u64,
}

impl DocumentLines<'_> {
    fn start_of(range: &PrintRange) -> ViewportCursor {
        ViewportCursor {
            byte_offset: range.start,
            line: range.first_line,
            line_continuation: false,
        }
    }
}

impl LineSource for DocumentLines<'_> {
    fn rewind(&mut self) -> Result<(), PrintError> {
        self.index = 0;
        self.cursor = self.ranges.first().map(Self::start_of);
        self.separator_pending = false;
        self.finished_bytes = 0;
        self.passes += 1;
        self.progress.phase = if self.counting_pages && self.passes == 1 {
            ExportPhase::CountingPages
        } else {
            ExportPhase::Writing
        };
        Ok(())
    }

    fn next_lines(&mut self, highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError> {
        if self.observer.cancelled() {
            return Err(PrintError::Cancelled);
        }
        let Some(cursor) = self.cursor.take() else {
            return Ok(None);
        };
        let range = self.ranges[self.index];
        let (mut lines, next) = self
            .manager
            .range_window(
                &self.document_id,
                self.revision,
                &cursor,
                highlight,
                Some(range.end),
                range.drop_final_empty,
            )
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
        if std::mem::take(&mut self.separator_pending) {
            if let Some(first) = lines.first_mut() {
                first.separator_before = true;
            }
        }
        let read_in_range = next.as_ref().map_or(range.end, |next| next.byte_offset) - range.start;
        self.progress.done = self.scanned
            + self.range_bytes * (self.passes - 1)
            + self.finished_bytes
            + read_in_range;
        self.observer.report(self.progress);
        if next.is_some() {
            self.cursor = next;
        } else {
            self.finished_bytes += range.end - range.start;
            self.index += 1;
            self.separator_pending = true;
            self.cursor = self.ranges.get(self.index).map(Self::start_of);
        }
        Ok(Some(lines))
    }

    fn page_written(&mut self, number: u64) {
        self.progress.pages_written = number;
    }
}

/// Bytes read per step of a hexadecimal dump (a whole number of rows).
const HEX_WINDOW_BYTES: u64 = 64 * 1024;

/// The document's bytes as hexadecimal dump rows:
/// `00000000  48 65 6C 6C 6F 0A …  Hello.`. The offset is shown like a
/// comment and the characters like a string.
struct HexLines<'a> {
    manager: &'a DocumentManager,
    document_id: DocumentId,
    revision: u64,
    len: u64,
    /// `None` before the first rewind and after the last row.
    offset: Option<u64>,
    observer: &'a dyn ExportObserver,
    progress: ExportProgress,
    passes: u64,
    counting_pages: bool,
}

/// One dump row for `bytes` (at most 16) at `offset`, with offset digits wide.
fn hex_row(offset: u64, bytes: &[u8], digits: usize, number: u64) -> SourceLine {
    use std::fmt::Write as _;
    let mut text = format!("{offset:0digits$X}  ");
    for (index, byte) in bytes.iter().enumerate() {
        let _ = write!(text, "{byte:02X} ");
        if index == 7 {
            text.push(' ');
        }
    }
    // Pad short last rows so the characters stay in their column.
    let full_width = digits + 2 + 16 * 3 + 1;
    while text.chars().count() < full_width {
        text.push(' ');
    }
    let characters_start = text.chars().count() + 1;
    text.push(' ');
    text.extend(bytes.iter().map(|byte| {
        if (0x20..0x7F).contains(byte) {
            char::from(*byte)
        } else {
            '.'
        }
    }));
    let end = text.chars().count();
    SourceLine {
        number,
        continuation: false,
        separator_before: false,
        spans: vec![
            Span {
                start: 0,
                end: digits,
                token: SyntaxToken::Comment,
            },
            Span {
                start: characters_start,
                end,
                token: SyntaxToken::String,
            },
        ],
        text,
    }
}

impl LineSource for HexLines<'_> {
    fn rewind(&mut self) -> Result<(), PrintError> {
        self.offset = Some(0);
        self.passes += 1;
        self.progress.phase = if self.counting_pages && self.passes == 1 {
            ExportPhase::CountingPages
        } else {
            ExportPhase::Writing
        };
        Ok(())
    }

    fn next_lines(&mut self, _highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError> {
        if self.observer.cancelled() {
            return Err(PrintError::Cancelled);
        }
        let Some(offset) = self.offset.take() else {
            return Ok(None);
        };
        let bytes = self
            .manager
            .read_at_revision(&self.document_id, self.revision, offset, HEX_WINDOW_BYTES)
            .map_err(|error| match error {
                EditorError::StaleRevision { .. } => PrintError::Source(
                    "the document changed during the export; export it again".into(),
                ),
                other => PrintError::Source(other.to_string()),
            })?;
        let digits = usize::try_from(self.len.max(1).ilog(16) + 1)
            .unwrap_or(8)
            .max(8);
        let lines = bytes
            .chunks(16)
            .zip((offset..).step_by(16))
            .map(|(row, at)| hex_row(at, row, digits, at / HEX_ROW_BYTES + 1))
            .collect();
        let next = offset + u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        // An empty read ends the dump, so a wrong length can never loop.
        self.offset = (next < self.len && !bytes.is_empty()).then_some(next);
        self.progress.done = self.len * (self.passes - 1) + next;
        self.observer.report(self.progress);
        Ok(Some(lines))
    }

    fn page_written(&mut self, number: u64) {
        self.progress.pages_written = number;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cut_at_boundary, DocumentLines, DocumentManager, ExportProgress, ExportScope, TextEncoding,
        Unobserved,
    };
    use crate::editor::{DocumentId, Selection};
    use nulis_print::LineSource;

    /// Every line an export of `scope` prints: (number, text, separator before).
    fn printed(
        manager: &DocumentManager,
        id: &DocumentId,
        scope: ExportScope,
    ) -> Vec<(u64, String, bool)> {
        let spans = manager.print_spans(id, 0, scope).expect("spans");
        let ranges = manager
            .print_ranges(id, 0, &spans, &Unobserved)
            .expect("ranges");
        let mut source = DocumentLines {
            manager,
            document_id: id.clone(),
            revision: 0,
            ranges,
            index: 0,
            cursor: None,
            separator_pending: false,
            observer: &Unobserved,
            progress: ExportProgress::default(),
            passes: 0,
            counting_pages: false,
            scanned: 0,
            range_bytes: 0,
            finished_bytes: 0,
        };
        source.rewind().expect("rewind");
        let mut lines = Vec::new();
        while let Some(batch) = source.next_lines(false).expect("lines") {
            lines.extend(
                batch
                    .into_iter()
                    .map(|line| (line.number, line.text, line.separator_before)),
            );
        }
        lines
    }

    fn document(text: &str) -> (tempfile::TempDir, DocumentManager, DocumentId) {
        let root = tempfile::tempdir().expect("session root");
        let manager = DocumentManager::new(root.path().into());
        let summary = manager
            .create_untitled("lines.txt".into(), text.into())
            .expect("document");
        (root, manager, summary.document_id)
    }

    #[test]
    fn a_selection_prints_its_ranges_with_their_own_line_numbers() {
        let text = (1..=10).fold(String::new(), |mut text, line| {
            text.push_str("line ");
            text.push_str(&line.to_string());
            text.push('\n');
            text
        });
        let (_root, manager, id) = document(&text);
        let offset = |line: usize| {
            u64::try_from(text.match_indices("line ").nth(line - 1).expect("line").0)
                .expect("small")
        };
        manager
            .set_selections(
                &id,
                vec![
                    // Line 7 from "ne 7" (backwards), then all of line 3 with its line break.
                    Selection {
                        anchor_byte: offset(7) + 6,
                        active_byte: offset(7) + 2,
                    },
                    Selection {
                        anchor_byte: offset(3),
                        active_byte: offset(4),
                    },
                ],
            )
            .expect("select");
        assert_eq!(
            printed(&manager, &id, ExportScope::Selection),
            [
                (3, "line 3".to_owned(), false),
                (7, "ne 7".to_owned(), true)
            ]
        );
        // The whole document keeps the empty line after its last line break.
        let whole = printed(&manager, &id, ExportScope::Document);
        assert_eq!(whole.len(), 11);
        assert_eq!(whole[10], (11, String::new(), false));
    }

    #[test]
    fn a_column_selection_prints_the_whole_lines_it_spans() {
        let (_root, manager, id) = document("alpha\r\nbeta\r\ngamma\r\ndelta\r\n");
        // Columns 1–3 of lines 2–3.
        manager
            .set_rectangular_selection(&id, 8, 16, 1, 3)
            .expect("block");
        assert_eq!(
            printed(&manager, &id, ExportScope::Selection),
            [
                (2, "beta".to_owned(), false),
                (3, "gamma".to_owned(), false)
            ]
        );
    }

    #[test]
    fn a_hex_dump_shows_offsets_bytes_and_characters() {
        let (_root, manager, id) = document("Hello, hex dump!\nTwo\u{1}");
        let mut source = super::HexLines {
            manager: &manager,
            document_id: id,
            revision: 0,
            len: 21,
            offset: None,
            observer: &Unobserved,
            progress: ExportProgress::default(),
            passes: 0,
            counting_pages: false,
        };
        source.rewind().expect("rewind");
        let rows = source.next_lines(true).expect("rows").expect("some rows");
        assert!(source.next_lines(true).expect("end").is_none());
        let texts: Vec<_> = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "00000000  48 65 6C 6C 6F 2C 20 68  65 78 20 64 75 6D 70 21  Hello, hex dump!",
                "00000010  0A 54 77 6F 01                                    .Two.",
            ]
        );
        assert_eq!(rows[0].spans[0].end, 8);
        assert_eq!(
            rows[1].spans[1].start,
            texts[1].find('.').expect("characters")
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_exported_file_gets_the_mode_of_a_new_file() {
        use crate::EditorError;
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("out.pdf");
        super::write_atomically(directory.path(), &path, |out| {
            out.write_all(b"%PDF")
                .map_err(|error| EditorError::io(&error))
        })
        .expect("write");
        // `File::create` asks for 0666 and the umask trims it.
        let plain = directory.path().join("plain");
        std::fs::File::create(&plain).expect("plain file");
        let mode = |path: &std::path::Path| {
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&path), mode(&plain));
    }

    #[test]
    fn print_stops_when_wrapped_lines_need_more_pages_than_allowed() {
        // 50 lines fit on one page, but each wraps onto three rows.
        let line = "word ".repeat(40);
        let text = format!("{line}\n").repeat(50);
        let (_root, manager, id) = document(&text);
        let request = super::PrintRequest {
            expected_revision: 0,
            setup: nulis_print::PageSetup {
                syntax_highlighting: false,
                ..nulis_print::PageSetup::default()
            },
            colors: nulis_print::ColorChoice::Print,
            date: String::new(),
            confirm_above: None,
            max_pages: Some(2),
            scope: ExportScope::Document,
        };
        let error = manager
            .print_pages(&id, &request, &Unobserved)
            .expect_err("too many pages");
        assert!(error.to_string().contains("at least 3"), "{error}");
        let allowed = super::PrintRequest {
            max_pages: Some(5),
            ..request
        };
        let printed = manager
            .print_pages(&id, &allowed, &Unobserved)
            .expect("pages");
        assert_eq!(printed.pages.len(), 3);
        assert_eq!(printed.title, "lines.txt");
    }

    #[test]
    fn exporting_an_empty_selection_asks_for_one() {
        let (_root, manager, id) = document("text");
        assert!(manager.print_spans(&id, 0, ExportScope::Selection).is_err());
    }

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
