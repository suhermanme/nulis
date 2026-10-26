use crate::format_tools;
use crate::line_changes::{ChangeHistory, ChangeTracker, LineChange, LineChangeStatus};
use crate::plugin_languages::PluginLanguageSync;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nulis_diff::{
    compare_binary, compare_text, BinaryDiffRequest, CompareMode, DiffPage, TextDiffRequest,
};
use nulis_document::{ByteOffset, Document, PieceTable, PieceTableSnapshot};
use nulis_filesystem::{fingerprint, save_atomic, FileFingerprint};
use nulis_hex::{parse_hex, replace_nibble, HexLayout};
use nulis_language::{
    CapabilityKind, CapabilityRegistry, DocumentText, FormatAssessment, FormatMode, FormatOutcome,
    Provider, ProviderError,
};
use nulis_large_file::{DiskAppendStore, FileSource, ReadAt};
use nulis_search::{
    expand_replacements, plan_replacements_each, search, validate_pattern, CancellationToken,
    SearchMatch, MAX_PAGE_MATCHES,
};
pub use nulis_search::{SearchMode, SearchOptions};
use nulis_syntax::{LanguageId, LanguageRegistry, SymbolKind, SyntaxAnalysis, SyntaxToken};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

mod column_tools;
pub use column_tools::{Justify, NumberPadding, NumberSequence, NumberSum};
mod completion;
mod external_changes;
mod server_support;
pub use server_support::ServerDocument;
mod find_all;
pub use completion::{CompletionList, COMPLETION_WINDOW_BYTES};
mod indentation;
pub use external_changes::{ExternalChange, ExternalChangeKind};
pub use indentation::{AutoIndent, IndentSettings, IndentSettingsTable, IndentationSummary};
mod line_endings;
mod line_ops;
mod navigation;
pub use line_endings::{
    LineEnding, LineEndingConversion, LineEndingStyle, LINE_ENDING_SAMPLE_BYTES,
    MAX_LINE_ENDING_CHANGES, MAX_LINE_ENDING_CONVERSION_BYTES,
};
pub use navigation::ChangeDirection;
mod print_export;
mod rename;
pub use print_export::{
    ExportObserver, ExportPhase, ExportProgress, ExportScope, PdfExportRequest, PdfExportResult,
    PrintRequest, PrintedDocument, Unobserved, HIGHLIGHT_LOOKBACK_BYTES, HIGHLIGHT_WINDOW_BYTES,
    LARGE_EXPORT_PAGES, MAX_PRINT_PAGES, PRINT_WINDOW_BYTES,
};
pub use rename::MAX_FILE_NAME_BYTES;
mod languages;
mod replace_in_files;
pub use find_all::{
    FindAllResult, FoundLine, FoundMatch, MAX_FIND_ALL_LINES, MAX_FIND_ALL_MATCHES,
    MAX_LINE_PREVIEW_BYTES,
};
pub use line_ops::{LineEditResult, LineFormatOptions, LineOperation};
pub use replace_in_files::{
    FileStamp, ReplaceFileOutcome, ReplacePreview, ReplacePreviewFile, ReplaceTarget,
    MAX_REPLACE_FILES,
};

pub const MAX_VIEWPORT_BYTES: u64 = 256 * 1024;
pub const MAX_DOCUMENT_MAP_SAMPLES: usize = 1024;
pub const MAX_MARKDOWN_PREVIEW_BYTES: u64 = 1024 * 1024;
pub const MAX_MARKDOWN_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Up to this size the document map reads the whole text and knows line
/// numbers; larger documents are sampled.
const DOCUMENT_MAP_FULL_BYTES: u64 = 8 * 1024 * 1024;
/// How far a sample of a large document reads to find a whole line.
const DOCUMENT_MAP_SAMPLE_BYTES: u64 = 512;
pub const MAX_HEX_VIEWPORT_BYTES: u64 = 64 * 1024;
pub const MAX_HEX_EDIT_BYTES: usize = 256 * 1024;
pub const MAX_HEX_CLIPBOARD_BYTES: u64 = 256 * 1024;
pub const MAX_EDIT_BYTES: usize = 1024 * 1024;
const MAX_WORD_SCAN_BYTES: u64 = 1024 * 1024;
const LANGUAGE_SNIFF_BYTES: u64 = 256;
/// Largest document the built-in JSON and XML tools read as a whole.
pub const MAX_TOOL_BYTES: u64 = 8 * 1024 * 1024;
/// Largest document whose characters are counted for the status bar.
pub const MAX_CHARACTER_COUNT_BYTES: u64 = 64 * 1024 * 1024;
const CHARACTER_COUNT_CHUNK: u64 = 256 * 1024;
pub const MAX_CLIPBOARD_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_SELECTIONS: usize = 1_024;
pub const MAX_RECTANGULAR_ROWS: usize = 100_000;
pub const MAX_BOOKMARKS: usize = 1_024;
pub const MAX_WORKSPACE_SEARCH_RESULTS: usize = 2_000;
pub const MAX_WORKSPACE_SEARCH_FILES: usize = 10_000;
pub const MAX_MERGE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_HISTORY_BUDGET: usize = 64 * 1024 * 1024;
const NAVIGATION_CHUNK_BYTES: u64 = 64 * 1024;
const MAX_NAVIGATION_CHECKPOINTS: usize = 8 * 1024;
const MAX_BOOKMARK_HISTORY: usize = 8 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DocumentId(pub String);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    #[serde(with = "decimal_u64")]
    pub anchor_byte: u64,
    #[serde(with = "decimal_u64")]
    pub active_byte: u64,
}

impl Selection {
    #[must_use]
    pub fn caret(offset: u64) -> Self {
        Self {
            anchor_byte: offset,
            active_byte: offset,
        }
    }
}

/// A constant-size column selection. Lines are one-based and scalar columns
/// are zero-based; byte endpoints preserve exact source positions for the two
/// corners without materializing one `Selection` per covered line.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RectangularSelection {
    #[serde(with = "decimal_u64")]
    pub anchor_byte: u64,
    #[serde(with = "decimal_u64")]
    pub active_byte: u64,
    #[serde(with = "decimal_u64")]
    pub anchor_line: u64,
    #[serde(with = "decimal_u64")]
    pub active_line: u64,
    #[serde(with = "decimal_u64")]
    pub anchor_column: u64,
    #[serde(with = "decimal_u64")]
    pub active_column: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub document_id: DocumentId,
    pub name: String,
    #[serde(with = "decimal_u64")]
    pub byte_len: u64,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    pub encoding: TextEncoding,
    pub dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Stable checkpoint identity, present once the document can be recovered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_id: Option<String>,
    /// The language the document is highlighted as.
    pub language_id: String,
    /// The user chose the language with View As; detection would pick another.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub language_override: bool,
    /// Whether the document currently refuses every content change.
    pub read_only: bool,
    /// What the document's language offers right now (tools, preview).
    #[serde(skip_serializing_if = "DocumentCapabilities::is_empty")]
    pub capabilities: DocumentCapabilities,
    /// What Tab inserts and how wide a tab is drawn.
    pub indentation: IndentationSummary,
    /// A binary format the document is in, such as "binary property list":
    /// it is not text, so the editor shows it in the hexadecimal view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_format: Option<String>,
}

/// The number of characters in a document, when it is small enough to count.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterCount {
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    /// Unicode scalar values in the document's encoding; absent above
    /// [`MAX_CHARACTER_COUNT_BYTES`], where counting would read too much.
    /// A decimal string, like every other 64-bit value crossing IPC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characters: Option<String>,
    /// Runs of non-space characters, counted in the same pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
    /// Lines (line endings plus one), counted in the same pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<String>,
    /// The line endings in use; above [`MAX_CHARACTER_COUNT_BYTES`] it is
    /// read from the first [`LINE_ENDING_SAMPLE_BYTES`] only.
    pub line_ending: LineEndingStyle,
}

/// The language features a document has right now, each named by the label
/// its provider shows ("JSON"), so the interface needs no list of languages.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCapabilities {
    /// Validate, by label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
    /// Beautify, by label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Minify, by label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact: Option<String>,
    /// Beautify may drop something; ask (Validate reports what) before running it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub format_may_lose_data: bool,
    /// A rendered view, by kind ("markdown").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Completion: "auto" (may open while typing) or "manual" (only when asked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    /// The approved language server for the document's language, by ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_server: Option<String>,
}

impl DocumentCapabilities {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// The name shown with the tools ("JSON"), if the document has any.
    fn tool_label(&self) -> Option<&str> {
        self.validate
            .as_deref()
            .or(self.format.as_deref())
            .or(self.compact.as_deref())
    }
}

/// One capability of a language, as Settings lists it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySwitch {
    pub kind: CapabilityKind,
    /// The provider's label ("JSON").
    pub label: String,
    pub enabled: bool,
}

/// A language and the capabilities that can be switched off for it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageCapabilities {
    pub id: String,
    pub display_name: String,
    pub capabilities: Vec<CapabilitySwitch>,
}

/// One language's claim on a file pattern, for the conflict list.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssociationClaim {
    pub language: String,
    pub display_name: String,
    /// `builtin` or the contributing plugin ID.
    pub source: String,
}

/// Several languages claim one file pattern. The first claim is used until
/// the user records a `files.associations` choice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageConflict {
    pub pattern: String,
    pub claims: Vec<AssociationClaim>,
}

/// The outcome of checking a document with a built-in tool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub valid: bool,
    pub message: String,
    /// Where the first problem is, as a decimal byte offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
    /// What Beautify would lose on this document (YAML comments, anchors),
    /// or why it cannot run (YAML tags); Beautify asks before losing anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beautify_note: Option<String>,
    /// Beautify would refuse this document.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub beautify_unavailable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCandidate {
    pub recovery_id: String,
    pub name: String,
    pub source_path: Option<String>,
    pub source_changed: bool,
    #[serde(with = "decimal_u64")]
    pub byte_len: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditorRecoveryRecord {
    format_version: u32,
    recovery_id: String,
    name: String,
    source_path: PathBuf,
    document_path: Option<PathBuf>,
    untitled: bool,
    source_fingerprint: FileFingerprint,
    additions_path: PathBuf,
    clean_revision: Option<u64>,
    encoding: TextEncoding,
    language_id: String,
    table: PieceTableSnapshot,
    /// Unsaved line change ranges; absent in older checkpoints and whenever
    /// the markers could not be computed.
    #[serde(default)]
    unsaved_changes: Option<Vec<(u64, u64)>>,
}

const RECOVERY_FORMAT_VERSION: u32 = 1;
const MAX_RECOVERY_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RECOVERY_CANDIDATES: usize = 128;
const MAX_RECOVERY_DIRECTORY_ENTRIES: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPayload {
    pub text: String,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    pub had_invalid_utf8: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewportRequest {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub start_line: u64,
    #[serde(default)]
    pub line_continuation: bool,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewportAtRequest {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewportLineRequest {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub line: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportCursor {
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
    #[serde(with = "decimal_u64")]
    pub line: u64,
    pub line_continuation: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportLine {
    #[serde(with = "decimal_u64")]
    pub line: u64,
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub end_byte: u64,
    pub text: String,
    pub source_byte_widths: Vec<u8>,
    pub continuation: bool,
    pub truncated: bool,
    /// Editing-session line change marker; absent for an untouched line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<LineChange>,
    /// The line's ending; absent for the last line and a truncated segment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ending: Option<LineEnding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportSnapshot {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    #[serde(with = "decimal_u64")]
    pub document_byte_len: u64,
    pub encoding: TextEncoding,
    pub lines: Vec<ViewportLine>,
    pub selections: Vec<Selection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rectangular_selection: Option<RectangularSelection>,
    pub syntax: SyntaxSnapshot,
    #[serde(with = "decimal_u64_vec")]
    pub bookmarks: Vec<u64>,
    pub line_changes: LineChangeStatus,
    pub next: Option<ViewportCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapSample {
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
    #[serde(with = "decimal_u64")]
    pub line: u64,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapSnapshot {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    #[serde(with = "decimal_u64")]
    pub document_byte_len: u64,
    pub total_lines: Option<usize>,
    pub samples: Vec<DocumentMapSample>,
    pub changes: Vec<DocumentMapChange>,
    pub line_changes: LineChangeStatus,
}

/// A bounded positional line change tick for the document map.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapChange {
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
    /// Zero when the document is too large for exact line numbers.
    #[serde(with = "decimal_u64")]
    pub line: u64,
    pub change: LineChange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownPreviewSource {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HexViewportRequest {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    pub max_bytes: u64,
    pub bytes_per_row: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexViewportRow {
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexCursor {
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexViewportSnapshot {
    pub document_id: DocumentId,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    #[serde(with = "decimal_u64")]
    pub document_byte_len: u64,
    pub bytes_per_row: u8,
    pub rows: Vec<HexViewportRow>,
    pub selections: Vec<Selection>,
    pub next: Option<HexCursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexCaret {
    #[serde(with = "decimal_u64")]
    pub byte_offset: u64,
    pub high_nibble: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexEditResult {
    pub document: DocumentSummary,
    pub caret: HexCaret,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexClipboardPayload {
    pub hex: String,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDescriptor {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxHighlight {
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub end_byte: u64,
    pub token: SyntaxToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxFold {
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub end_byte: u64,
    #[serde(with = "decimal_u64")]
    pub start_line: u64,
    #[serde(with = "decimal_u64")]
    pub end_line: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxSymbol {
    pub name: String,
    pub kind: SymbolKind,
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub line: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxBracketPair {
    #[serde(with = "decimal_u64")]
    pub open_byte: u64,
    #[serde(with = "decimal_u64")]
    pub close_byte: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxSnapshot {
    pub language_id: String,
    pub parsed_bytes: usize,
    pub highlights: Vec<SyntaxHighlight>,
    pub folds: Vec<SyntaxFold>,
    pub symbols: Vec<SyntaxSymbol>,
    pub bracket_pairs: Vec<SyntaxBracketPair>,
    pub truncated: bool,
    pub timed_out: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BookmarkDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TextTransform {
    Uppercase,
    Lowercase,
    TrimWhitespace,
    /// Swaps upper and lower case letter by letter.
    InvertCase,
    /// Upper-cases the first letter of each word and lower-cases the rest.
    Capitalize,
    /// Replaces the selected bytes with their Base64 text.
    Base64Encode,
    /// Replaces Base64 text with the bytes it encodes.
    Base64Decode,
}

fn invert_case(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            let swapped: Vec<char> = if character.is_uppercase() {
                character.to_lowercase().collect()
            } else if character.is_lowercase() {
                character.to_uppercase().collect()
            } else {
                vec![character]
            };
            swapped
        })
        .collect()
}

fn capitalize_words(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut at_word_start = true;
    for character in text.chars() {
        if character.is_alphanumeric() {
            if at_word_start {
                output.extend(character.to_uppercase());
            } else {
                output.extend(character.to_lowercase());
            }
            at_word_start = false;
        } else {
            // An apostrophe inside a word ("don't") does not start a new word.
            at_word_start = character != '\'' && character != '’';
            output.push(character);
        }
    }
    output
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextEncoding {
    Utf8,
    Windows1252,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FindDirection {
    Previous,
    Next,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    pub selection: Option<Selection>,
    pub wrapped: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAllResult {
    pub document: DocumentSummary,
    pub replacements: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSearchMatch {
    pub path: String,
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub end_byte: u64,
    #[serde(with = "decimal_u64")]
    pub line: u64,
    /// The start of the matching line (see [`MAX_LINE_PREVIEW_BYTES`]).
    pub line_text: String,
}

/// Which files Find in Files reads below its root.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileSearchScope {
    /// File-name patterns such as `*.rs`; `*` and `?` are wildcards,
    /// letters match either case, and an empty list means every file.
    #[serde(default)]
    pub include: Vec<String>,
    /// Whether folders below the root are searched too.
    #[serde(default = "default_true_scope")]
    pub recursive: bool,
}

fn default_true_scope() -> bool {
    true
}

impl FileSearchScope {
    fn includes(&self, path: &Path) -> bool {
        let patterns = self
            .include
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .collect::<Vec<_>>();
        if patterns.is_empty() {
            return true;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        patterns
            .iter()
            .any(|pattern| wildcard_match(&pattern.to_lowercase(), &name))
    }
}

/// Matches `*` (any run) and `?` (one character) against a whole name.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let name = name.chars().collect::<Vec<_>>();
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, n));
            p += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|character| *character == '*')
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSearchResult {
    pub matches: Vec<WorkspaceSearchMatch>,
    pub files_scanned: usize,
    pub skipped_files: usize,
    pub truncated: bool,
}

#[derive(Debug)]
struct HistoryEntry {
    edits: Vec<(u64, u64, usize)>,
    changes: ChangeHistory,
}

#[derive(Debug)]
struct EditorSession {
    name: String,
    source_path: Option<PathBuf>,
    source_fingerprint: Option<FileFingerprint>,
    original_path: Option<PathBuf>,
    additions_path: Option<PathBuf>,
    recovery_id: Option<String>,
    document: Document,
    encoding: TextEncoding,
    language_id: LanguageId,
    /// The user chose the language (View As) instead of detection.
    language_override: bool,
    /// Resolved when the language or the registry changes.
    capabilities: DocumentCapabilities,
    /// How the text was indented when it was opened, if that could be told.
    detected_indent: Option<nulis_language::indent::IndentUnit>,
    /// Resolved with the capabilities.
    indentation: IndentationSummary,
    /// A snippet whose placeholders Tab still visits.
    snippet: Option<completion::SnippetSession>,
    selections: Vec<Selection>,
    rectangular_selection: Option<RectangularSelection>,
    preferred_scalar_columns: Vec<Option<u64>>,
    bookmarks: BTreeSet<u64>,
    bookmark_undo: VecDeque<HistoryEntry>,
    bookmark_redo: VecDeque<HistoryEntry>,
    changes: ChangeTracker,
    navigation_index: SparseNavigationIndex,
}

#[derive(Debug)]
struct SparseNavigationIndex {
    lines_by_offset: BTreeMap<u64, u64>,
}

impl Default for SparseNavigationIndex {
    fn default() -> Self {
        Self {
            lines_by_offset: BTreeMap::from([(0, 1)]),
        }
    }
}

impl SparseNavigationIndex {
    fn reset(&mut self) {
        self.lines_by_offset.clear();
        self.lines_by_offset.insert(0, 1);
    }

    fn record(&mut self, byte_offset: u64, line: u64) {
        self.lines_by_offset.insert(byte_offset, line);
        while self.lines_by_offset.len() > MAX_NAVIGATION_CHECKPOINTS {
            let Some(oldest_non_origin) = self.lines_by_offset.keys().nth(1).copied() else {
                break;
            };
            self.lines_by_offset.remove(&oldest_non_origin);
        }
    }

    fn checkpoint_for_offset(&self, byte_offset: u64) -> (u64, u64) {
        self.lines_by_offset
            .range(..=byte_offset)
            .next_back()
            .map_or((0, 1), |(offset, line)| (*offset, *line))
    }

    fn checkpoint_for_line(&self, line: u64) -> (u64, u64) {
        self.lines_by_offset
            .iter()
            .filter(|(_, candidate)| **candidate <= line)
            .max_by_key(|(_, candidate)| **candidate)
            .map_or((0, 1), |(offset, candidate)| (*offset, *candidate))
    }
}

#[derive(Debug)]
pub struct DocumentManager {
    next_id: AtomicU64,
    session_root: PathBuf,
    languages: std::sync::RwLock<LanguageRegistry>,
    capabilities: std::sync::RwLock<CapabilityRegistry>,
    indent_settings: std::sync::RwLock<IndentSettingsTable>,
    /// Language ID to the approved language server for it.
    language_servers: std::sync::RwLock<BTreeMap<String, String>>,
    /// Configured language servers and the ones running.
    servers: crate::language_servers::LanguageServers,
    sessions: Mutex<BTreeMap<DocumentId, EditorSession>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DocumentCompareRequest {
    pub left_id: DocumentId,
    pub right_id: DocumentId,
    pub mode: CompareMode,
    pub left_start_byte: u64,
    pub right_start_byte: u64,
    pub left_start_line: u64,
    pub right_start_line: u64,
    pub max_bytes: usize,
    pub max_changes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ApplyCompareRequest {
    pub source_id: DocumentId,
    pub target_id: DocumentId,
    pub expected_target_revision: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub target_start: u64,
    pub target_end: u64,
}

impl Default for DocumentManager {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("nulis-sessions"))
    }
}

impl Drop for DocumentManager {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.get_mut() {
            for session in sessions.values() {
                if session.recovery_id.is_some() {
                    continue;
                }
                if let Some(path) = &session.additions_path {
                    let _ = fs::remove_file(path);
                }
                if let Some(path) = &session.original_path {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

impl DocumentManager {
    /// Reads the whole document for a tool, named by its provider's label.
    fn tool_source(
        &self,
        session: &EditorSession,
    ) -> Result<(String, Result<String, ToolProblem>), EditorError> {
        tool_source(session, || {
            self.provider_labels(CapabilityKind::Diagnostics)
        })
    }

    /// The labels of every registered provider of one kind, in registration
    /// order without repeats ("JSON, XML, YAML, and plist").
    fn provider_labels(&self, kind: CapabilityKind) -> String {
        let mut labels: Vec<String> = Vec::new();
        for (info, _) in self.capability_registry().providers() {
            if info.kind == kind && !labels.contains(&info.label) {
                labels.push(info.label);
            }
        }
        match labels.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
        }
    }

    #[must_use]
    pub fn new(session_root: PathBuf) -> Self {
        let approvals = session_root.join("lsp-approvals.json");
        Self {
            next_id: AtomicU64::new(0),
            session_root,
            languages: std::sync::RwLock::new(LanguageRegistry::default()),
            capabilities: std::sync::RwLock::new(crate::builtin_providers::builtin_registry()),
            indent_settings: std::sync::RwLock::new(IndentSettingsTable::default()),
            language_servers: std::sync::RwLock::new(BTreeMap::new()),
            servers: crate::language_servers::LanguageServers::new(approvals),
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    /// Creates an untitled in-memory document. File-backed opening is a
    /// separate lifecycle operation and never routes large files through IPC.
    ///
    /// # Errors
    /// Returns an error when initial content exceeds the bounded command limit.
    pub fn create_untitled(
        &self,
        name: String,
        initial_text: String,
    ) -> Result<DocumentSummary, EditorError> {
        if initial_text.len() > MAX_EDIT_BYTES {
            return Err(EditorError::EditTooLarge(initial_text.len()));
        }
        let starts_clean = initial_text.is_empty();
        let initial_bytes = initial_text.into_bytes();
        fs::create_dir_all(&self.session_root).map_err(|error| EditorError::io(&error))?;
        let base = tempfile::Builder::new()
            .prefix("document-")
            .suffix(".base")
            .tempfile_in(&self.session_root)
            .map_err(|error| EditorError::io(&error))?;
        let (mut base_file, original_path) =
            base.keep().map_err(|error| EditorError::io(&error.error))?;
        base_file
            .write_all(&initial_bytes)
            .and_then(|()| base_file.sync_all())
            .map_err(|error| EditorError::io(&error))?;
        drop(base_file);
        let source_fingerprint =
            fingerprint(&original_path).map_err(|error| EditorError::io(&error))?;
        let additions = tempfile::Builder::new()
            .prefix("document-")
            .suffix(".add")
            .tempfile_in(&self.session_root)
            .map_err(|error| EditorError::io(&error))?;
        let (additions_file, additions_path) = additions
            .keep()
            .map_err(|error| EditorError::io(&error.error))?;
        drop(additions_file);
        let additions =
            DiskAppendStore::open(&additions_path).map_err(|error| EditorError::io(&error))?;
        let source = FileSource::open(&original_path).map_err(|error| EditorError::io(&error))?;
        let number = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = DocumentId(format!("document-{number}"));
        let language_id = self.languages().detect_path(Path::new(&name));
        let mut document = Document::new(
            PieceTable::new(std::sync::Arc::new(source), Box::new(additions)),
            DEFAULT_HISTORY_BUDGET,
        );
        if !starts_clean {
            document.mark_unsaved();
        }
        let recovery_id = additions_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| EditorError::Recovery("invalid session filename".into()))?
            .to_owned();
        let detected_indent =
            indentation::detect_document_unit(document.table(), TextEncoding::Utf8);
        let summary = DocumentSummary {
            document_id: id.clone(),
            name: name.clone(),
            byte_len: document.table().len(),
            revision: document.table().revision(),
            encoding: TextEncoding::Utf8,
            dirty: !starts_clean,
            source_path: None,
            recovery_id: Some(recovery_id.clone()),
            language_id: language_id.as_str().to_owned(),
            language_override: false,
            read_only: false,
            capabilities: self.document_capabilities(&language_id),
            indentation: self.document_indentation(&language_id, detected_indent),
            binary_format: None,
        };
        let language_id_for_indent = language_id.clone();
        let capabilities = self.document_capabilities(&language_id);
        self.sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?
            .insert(
                id,
                EditorSession {
                    name,
                    source_path: None,
                    source_fingerprint,
                    original_path: Some(original_path),
                    additions_path: Some(additions_path),
                    recovery_id: Some(recovery_id),
                    document,
                    encoding: TextEncoding::Utf8,
                    language_id,
                    language_override: false,
                    capabilities,
                    detected_indent,
                    indentation: self
                        .document_indentation(&language_id_for_indent, detected_indent),
                    snippet: None,
                    selections: vec![Selection::caret(0)],
                    rectangular_selection: None,
                    preferred_scalar_columns: vec![None],
                    bookmarks: BTreeSet::new(),
                    bookmark_undo: VecDeque::new(),
                    bookmark_redo: VecDeque::new(),
                    changes: ChangeTracker::default(),
                    navigation_index: SparseNavigationIndex::default(),
                },
            );
        Ok(summary)
    }

    /// Returns current metadata for one open document.
    ///
    /// # Errors
    /// Returns an error when the document is unknown or state is unavailable.
    pub fn summary(&self, document_id: &DocumentId) -> Result<DocumentSummary, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        Ok(summary(document_id, session))
    }

    /// Every open document's summary, in document-ID order; the editor
    /// refreshes its copies after settings change their capabilities.
    ///
    /// # Errors
    /// Returns an error when the document state is unavailable.
    pub fn summaries(&self) -> Result<Vec<DocumentSummary>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        Ok(sessions
            .iter()
            .map(|(id, session)| summary(id, session))
            .collect())
    }

    /// Switches a document between editing and read-only and returns its metadata.
    /// A read-only document can still be read, searched, saved, and closed.
    ///
    /// # Errors
    /// Returns an error if the document is unknown.
    pub fn toggle_read_only(
        &self,
        document_id: &DocumentId,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let read_only = !session.document.is_read_only();
        session.document.set_read_only(read_only);
        Ok(summary(document_id, session))
    }

    /// Counts the characters of a document by streaming it in bounded chunks.
    /// UTF-8 counts every byte that starts a sequence, so each invalid byte
    /// that is not a continuation byte counts as one character.
    ///
    /// # Errors
    /// Returns an error if the document is unknown or cannot be read.
    pub fn character_count(&self, document_id: &DocumentId) -> Result<CharacterCount, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let length = table.len();
        if length > MAX_CHARACTER_COUNT_BYTES {
            return Ok(CharacterCount {
                revision: table.revision(),
                characters: None,
                words: None,
                lines: None,
                line_ending: line_endings::sample_style(table)?,
            });
        }
        let utf8 = session.encoding == TextEncoding::Utf8;
        let (mut characters, mut words) = (0_u64, 0_u64);
        let mut in_word = false;
        let mut endings = line_endings::LineEndingScanner::default();
        let mut offset = 0_u64;
        while offset < length {
            let chunk = table.read_vec(offset, CHARACTER_COUNT_CHUNK.min(length - offset))?;
            for &byte in &chunk {
                // Windows-1252 maps every byte to one character; UTF-8 counts sequence starts.
                if !utf8 || byte & 0xC0 != 0x80 {
                    characters += 1;
                }
                let space = matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
                if !space && !in_word {
                    words += 1;
                }
                in_word = !space;
                endings.push(byte);
            }
            offset += chunk.len() as u64;
        }
        let characters = Some(characters.to_string());
        Ok(CharacterCount {
            revision: table.revision(),
            characters,
            words: Some(words.to_string()),
            lines: Some((endings.endings() + 1).to_string()),
            line_ending: endings.style(),
        })
    }

    /// Reports whether any open document still has unsaved edits.
    ///
    /// # Errors
    /// Returns an error if document state is unavailable.
    pub fn has_dirty_documents(&self) -> Result<bool, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        Ok(sessions
            .values()
            .any(|session| !session.document.is_clean()))
    }

    /// Opens a regular file without reading its content and assigns a securely
    /// created disk append store under the configured session root.
    ///
    /// # Errors
    /// Returns an error if the source or persistent session storage cannot be opened.
    pub fn open_file(&self, path: &Path) -> Result<DocumentSummary, EditorError> {
        let external_changes::OpenedSource {
            mut document,
            fingerprint: source_fingerprint,
            additions_path,
        } = self.open_source(path)?;
        let number = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = DocumentId(format!("document-{number}"));
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        // A binary file is not text: it opens read-only, for viewing as bytes.
        let head = document
            .table()
            .read_vec(0, document.table().len().min(8))?;
        if binary_kind(&head).is_some() {
            document.set_read_only(true);
        }
        let mut language_id = self.languages().detect_path(path);
        if language_id.as_str() == "plaintext" {
            // A name that says nothing: look for a `#!` line or a markup prolog.
            let table = document.table();
            let head = table.read_vec(0, table.len().min(LANGUAGE_SNIFF_BYTES))?;
            if let Some(detected) = self.languages().detect_content(&head) {
                language_id = detected;
            }
        }
        let detected_indent =
            indentation::detect_document_unit(document.table(), TextEncoding::Utf8);
        let language_id_for_indent = language_id.clone();
        let capabilities = self.document_capabilities(&language_id);
        let session = EditorSession {
            name,
            source_path: Some(path.to_path_buf()),
            source_fingerprint,
            original_path: None,
            additions_path: Some(additions_path),
            recovery_id: None,
            document,
            encoding: TextEncoding::Utf8,
            language_id,
            language_override: false,
            capabilities,
            detected_indent,
            indentation: self.document_indentation(&language_id_for_indent, detected_indent),
            snippet: None,
            selections: vec![Selection::caret(0)],
            rectangular_selection: None,
            preferred_scalar_columns: vec![None],
            bookmarks: BTreeSet::new(),
            bookmark_undo: VecDeque::new(),
            bookmark_redo: VecDeque::new(),
            changes: ChangeTracker::default(),
            navigation_index: SparseNavigationIndex::default(),
        };
        let summary = summary(&id, &session);
        self.sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?
            .insert(id, session);
        Ok(summary)
    }

    /// Saves a local file-backed document to its current path using atomic
    /// replacement and the fingerprint captured by the previous open/save.
    ///
    /// # Errors
    /// Returns an error for an untitled document, stale revision,
    /// concurrent destination change, or storage failure.
    pub fn save(
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
        let destination = session
            .source_path
            .clone()
            .ok_or(EditorError::SavePathRequired)?;
        let saved = save_atomic(&destination, session.source_fingerprint.as_ref(), |file| {
            session.document.table().write_to(file)
        })
        .map_err(|error| EditorError::Save(error.to_string()))?;
        session.source_fingerprint = Some(saved);
        rebase_saved_session(session, &self.session_root, &destination)?;
        Ok(summary(document_id, session))
    }

    /// Atomically saves any document to a local destination and makes that
    /// path authoritative for subsequent Save commands.
    ///
    /// # Errors
    /// Returns an error for a stale revision, invalid path, concurrent
    /// destination change, or storage failure.
    pub fn save_as(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        destination: &Path,
    ) -> Result<DocumentSummary, EditorError> {
        if destination.as_os_str().is_empty() || destination.file_name().is_none() {
            return Err(EditorError::InvalidSavePath(
                destination.to_string_lossy().into_owned(),
            ));
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if session.additions_path.as_deref() == Some(destination)
            || session.original_path.as_deref() == Some(destination)
        {
            return Err(EditorError::InvalidSavePath(
                destination.display().to_string(),
            ));
        }
        let expected = if session.source_path.as_deref() == Some(destination) {
            session.source_fingerprint.clone()
        } else {
            fingerprint(destination).map_err(|error| EditorError::io(&error))?
        };
        let saved = save_atomic(destination, expected.as_ref(), |file| {
            session.document.table().write_to(file)
        })
        .map_err(|error| EditorError::Save(error.to_string()))?;
        session.name = destination.file_name().map_or_else(
            || destination.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        session.source_path = Some(destination.to_path_buf());
        session.source_fingerprint = Some(saved);
        session.language_id = self.languages().detect_path(destination);
        session.language_override = false;
        self.refresh_language_state(session);
        rebase_saved_session(session, &self.session_root, destination)?;
        Ok(summary(document_id, session))
    }

    /// Durably checkpoints edits without copying an entire source file.
    ///
    /// # Errors
    /// Returns an error for an unknown document or an invalid recovery source.
    pub fn checkpoint_recovery(&self, document_id: &DocumentId) -> Result<(), EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        if session.document.is_clean() {
            return Ok(());
        }
        let original = session
            .original_path
            .as_ref()
            .or(session.source_path.as_ref())
            .ok_or_else(|| {
                EditorError::Recovery("the document has no file to recover from".into())
            })?;
        let additions = session
            .additions_path
            .as_ref()
            .ok_or_else(|| EditorError::Recovery("no persistent edit store".into()))?;
        let recovery_id = match &session.recovery_id {
            Some(id) => id.clone(),
            None => additions
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| EditorError::Recovery("invalid session filename".into()))?
                .to_owned(),
        };
        if !valid_recovery_id(&recovery_id) {
            return Err(EditorError::Recovery("invalid recovery identifier".into()));
        }
        session
            .document
            .table_mut()
            .sync_additions()
            .map_err(|error| EditorError::io(&error))?;
        let source_fingerprint = fingerprint(original)
            .map_err(|error| EditorError::io(&error))?
            .ok_or_else(|| EditorError::Recovery("recovery source is missing".into()))?;
        if session.source_fingerprint.as_ref() != Some(&source_fingerprint) {
            return Err(EditorError::Recovery("the original file changed".into()));
        }
        let record = EditorRecoveryRecord {
            format_version: RECOVERY_FORMAT_VERSION,
            recovery_id: recovery_id.clone(),
            name: session.name.clone(),
            source_path: original.clone(),
            document_path: session.source_path.clone(),
            untitled: session.original_path.is_some(),
            source_fingerprint,
            additions_path: additions.clone(),
            clean_revision: session
                .document
                .is_clean()
                .then(|| session.document.table().revision()),
            encoding: session.encoding,
            language_id: session.language_id.as_str().to_owned(),
            table: session.document.table().snapshot(),
            unsaved_changes: session
                .changes
                .status()
                .available
                .then(|| session.changes.unsaved_ranges()),
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|error| EditorError::Recovery(error.to_string()))?;
        if bytes.len() as u64 > MAX_RECOVERY_MANIFEST_BYTES {
            return Err(EditorError::Recovery(
                "recovery manifest is too large".into(),
            ));
        }
        let path = recovery_manifest_path(&self.session_root, &recovery_id);
        fs::create_dir_all(self.session_root.join("recovery"))
            .map_err(|error| EditorError::io(&error))?;
        let expected = fingerprint(&path).map_err(|error| EditorError::io(&error))?;
        save_atomic(&path, expected.as_ref(), |file| {
            file.write_all(&bytes)?;
            u64::try_from(bytes.len()).map_err(std::io::Error::other)
        })
        .map_err(|error| EditorError::Recovery(error.to_string()))?;
        session.recovery_id = Some(recovery_id);
        Ok(())
    }

    /// Lists valid recovery checkpoints from previous sessions, including
    /// checkpoints whose original source changed.
    ///
    /// # Errors
    /// Returns an error if the recovery directory cannot be read.
    pub fn recovery_candidates(&self) -> Result<Vec<RecoveryCandidate>, EditorError> {
        let directory = self.session_root.join("recovery");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(EditorError::io(&error)),
        };
        let mut candidates = Vec::new();
        for entry in entries.take(MAX_RECOVERY_DIRECTORY_ENTRIES) {
            let entry = entry.map_err(|error| EditorError::io(&error))?;
            let Some(id) = entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            if let Ok(record) = self.load_recovery_record(&id) {
                let source_changed = fingerprint(&record.source_path).ok().flatten()
                    != Some(record.source_fingerprint.clone());
                candidates.push(RecoveryCandidate {
                    recovery_id: id,
                    name: record.name,
                    source_path: record
                        .document_path
                        .map(|path| path.to_string_lossy().into_owned()),
                    source_changed,
                    byte_len: record.table.document_len,
                });
                if candidates.len() == MAX_RECOVERY_CANDIDATES {
                    break;
                }
            }
        }
        candidates.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(candidates)
    }

    /// Restores one validated checkpoint into an editable document.
    ///
    /// # Errors
    /// Returns an error for an invalid checkpoint, changed source, or missing append store.
    pub fn restore_recovery(&self, recovery_id: &str) -> Result<DocumentSummary, EditorError> {
        let record = self.load_recovery_record(recovery_id)?;
        if fingerprint(&record.source_path).map_err(|error| EditorError::io(&error))?
            != Some(record.source_fingerprint.clone())
        {
            return Err(EditorError::Recovery("the original file changed".into()));
        }
        let source =
            FileSource::open(&record.source_path).map_err(|error| EditorError::io(&error))?;
        let additions = DiskAppendStore::open(&record.additions_path)
            .map_err(|error| EditorError::io(&error))?;
        let table = PieceTable::from_snapshot(
            std::sync::Arc::new(source),
            Box::new(additions),
            &record.table,
        )?;
        let language_id = LanguageId::new(record.language_id)?;
        // Checkpoints do not record whether the language was chosen by hand;
        // one that detection would not pick was.
        let language_override = language_id
            != self.languages().detect_path(
                record
                    .document_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new(&record.name)),
            );
        let changes = record
            .unsaved_changes
            .as_deref()
            .map_or_else(ChangeTracker::unavailable, |ranges| {
                ChangeTracker::from_unsaved(ranges, table.len())
            });
        // Undo history is not recovered, so a recovered edit stays unsaved.
        let recovered_clean = record.clean_revision == Some(table.revision());
        let mut document = Document::new(table, DEFAULT_HISTORY_BUDGET);
        if !recovered_clean {
            document.mark_unsaved();
        }
        let number = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = DocumentId(format!("document-{number}"));
        let detected_indent = indentation::detect_document_unit(document.table(), record.encoding);
        let language_id_for_indent = language_id.clone();
        let capabilities = self.document_capabilities(&language_id);
        let session = EditorSession {
            name: record.name,
            source_path: record.document_path,
            source_fingerprint: if record.untitled {
                None
            } else {
                Some(record.source_fingerprint)
            },
            original_path: record.untitled.then_some(record.source_path),
            additions_path: Some(record.additions_path),
            recovery_id: Some(recovery_id.to_owned()),
            document,
            encoding: record.encoding,
            language_id,
            language_override,
            capabilities,
            detected_indent,
            indentation: self.document_indentation(&language_id_for_indent, detected_indent),
            snippet: None,
            selections: vec![Selection::caret(0)],
            rectangular_selection: None,
            preferred_scalar_columns: vec![None],
            bookmarks: BTreeSet::new(),
            bookmark_undo: VecDeque::new(),
            bookmark_redo: VecDeque::new(),
            changes,
            navigation_index: SparseNavigationIndex::default(),
        };
        let result = summary(&id, &session);
        self.sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?
            .insert(id, session);
        Ok(result)
    }

    /// Closes a document, optionally discarding unsaved edits and its checkpoint.
    ///
    /// # Errors
    /// Returns an error if the document has unsaved edits and discard was not requested.
    pub fn close_document(
        &self,
        document_id: &DocumentId,
        discard: bool,
    ) -> Result<(), EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        if !discard && !session.document.is_clean() {
            return Err(EditorError::Recovery(
                "unsaved edits require save or discard".into(),
            ));
        }
        if let Some(id) = &session.recovery_id {
            remove_if_present(&recovery_manifest_path(&self.session_root, id))?;
        }
        let session = sessions
            .remove(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        drop(sessions);
        if let Some(path) = session.additions_path {
            let _ = remove_if_present(&path);
        }
        if let Some(path) = session.original_path {
            let _ = remove_if_present(&path);
        }
        Ok(())
    }

    /// Removes a recovery checkpoint after explicit user dismissal.
    ///
    /// # Errors
    /// Returns an error for an invalid identifier or filesystem failure.
    pub fn discard_recovery(&self, recovery_id: &str) -> Result<(), EditorError> {
        let record = self.load_recovery_record(recovery_id)?;
        remove_if_present(&recovery_manifest_path(&self.session_root, recovery_id))?;
        remove_if_present(&record.additions_path)?;
        if record.untitled {
            remove_if_present(&record.source_path)?;
        }
        Ok(())
    }

    fn load_recovery_record(&self, recovery_id: &str) -> Result<EditorRecoveryRecord, EditorError> {
        if !valid_recovery_id(recovery_id) {
            return Err(EditorError::Recovery("invalid recovery identifier".into()));
        }
        let path = recovery_manifest_path(&self.session_root, recovery_id);
        let metadata = fs::symlink_metadata(&path).map_err(|error| EditorError::io(&error))?;
        if !metadata.is_file() || metadata.len() > MAX_RECOVERY_MANIFEST_BYTES {
            return Err(EditorError::Recovery("invalid recovery manifest".into()));
        }
        let record: EditorRecoveryRecord =
            serde_json::from_slice(&fs::read(&path).map_err(|error| EditorError::io(&error))?)
                .map_err(|error| EditorError::Recovery(error.to_string()))?;
        if record.format_version != RECOVERY_FORMAT_VERSION || record.recovery_id != recovery_id {
            return Err(EditorError::Recovery(
                "unsupported recovery manifest".into(),
            ));
        }
        if record.untitled && record.document_path.is_some() {
            return Err(EditorError::Recovery(
                "invalid untitled recovery source".into(),
            ));
        }
        if !record.untitled && record.document_path.as_ref() != Some(&record.source_path) {
            return Err(EditorError::Recovery(
                "invalid document recovery source".into(),
            ));
        }
        let root = self
            .session_root
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        let additions = record
            .additions_path
            .canonicalize()
            .map_err(|error| EditorError::io(&error))?;
        if !additions.starts_with(&root) || !additions.is_file() {
            return Err(EditorError::Recovery("invalid recovery edit store".into()));
        }
        if record.untitled {
            let parent = record
                .source_path
                .parent()
                .ok_or_else(|| EditorError::Recovery("invalid untitled source".into()))?
                .canonicalize()
                .map_err(|error| EditorError::io(&error))?;
            if parent != root
                || record
                    .source_path
                    .extension()
                    .is_none_or(|extension| extension != "base")
            {
                return Err(EditorError::Recovery("invalid untitled source".into()));
            }
            match fs::symlink_metadata(&record.source_path) {
                Ok(metadata) if metadata.is_file() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(EditorError::Recovery("invalid untitled source".into())),
            }
        }
        Ok(record)
    }

    /// Compares bounded windows from two open documents without materializing
    /// either complete source.
    ///
    /// # Errors
    /// Returns an error for unknown documents, invalid bounds, or source reads.
    pub(super) fn compare_documents(
        &self,
        request: &DocumentCompareRequest,
    ) -> Result<DiffPage, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let left = sessions
            .get(&request.left_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.left_id.clone()))?;
        let right = sessions
            .get(&request.right_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.right_id.clone()))?;
        match request.mode {
            CompareMode::Text => compare_text(
                left.document.table(),
                right.document.table(),
                TextDiffRequest {
                    left_start_byte: request.left_start_byte,
                    right_start_byte: request.right_start_byte,
                    left_start_line: request.left_start_line,
                    right_start_line: request.right_start_line,
                    max_bytes: request.max_bytes,
                    max_changes: request.max_changes,
                },
            ),
            CompareMode::Binary => {
                if request.left_start_byte != request.right_start_byte {
                    return Err(EditorError::Diff(
                        "binary comparison requires aligned source offsets".into(),
                    ));
                }
                compare_binary(
                    left.document.table(),
                    right.document.table(),
                    BinaryDiffRequest {
                        start_byte: request.left_start_byte,
                        max_bytes: request.max_bytes,
                        max_changes: request.max_changes,
                    },
                )
            }
        }
        .map_err(|error| EditorError::Diff(error.to_string()))
    }

    /// Copies one exact byte range from a source document over a range in a
    /// target document as one revision-checked, undoable edit.
    ///
    /// # Errors
    /// Returns an error for stale revisions, invalid/oversized ranges, unknown
    /// documents, or backing-store failures.
    pub(super) fn apply_compare_change(
        &self,
        request: &ApplyCompareRequest,
    ) -> Result<DocumentSummary, EditorError> {
        if request.source_end < request.source_start {
            return Err(EditorError::InvalidRange {
                start: request.source_start,
                end: request.source_end,
            });
        }
        if request.target_end < request.target_start {
            return Err(EditorError::InvalidRange {
                start: request.target_start,
                end: request.target_end,
            });
        }
        let source_len = request.source_end - request.source_start;
        if source_len > MAX_MERGE_BYTES {
            return Err(EditorError::MergeTooLarge {
                selected: source_len,
                maximum: MAX_MERGE_BYTES,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let copied = {
            let source = sessions
                .get(&request.source_id)
                .ok_or_else(|| EditorError::UnknownDocument(request.source_id.clone()))?;
            source
                .document
                .table()
                .read_vec(request.source_start, source_len)?
        };
        let target = sessions
            .get_mut(&request.target_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.target_id.clone()))?;
        validate_revision(target, request.expected_target_revision)?;
        if request.target_end > target.document.table().len() {
            return Err(EditorError::OffsetOutOfRange {
                offset: request.target_end,
                document_len: target.document.table().len(),
            });
        }
        target.document.replace(
            request.target_start,
            request.target_end - request.target_start,
            &copied,
        )?;
        let inserted_len = u64::try_from(copied.len()).map_err(|_| EditorError::OffsetOverflow)?;
        let caret = request
            .target_start
            .checked_add(inserted_len)
            .ok_or(EditorError::OffsetOverflow)?;
        target.selections = vec![Selection::caret(caret)];
        target.rectangular_selection = None;
        let bookmark_edits = vec![(request.target_start, request.target_end, copied.len())];
        relocate_bookmarks(&mut target.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(target, bookmark_edits);
        target.preferred_scalar_columns = vec![None];
        target.navigation_index.reset();
        Ok(summary(&request.target_id, target))
    }

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

    /// Returns a bounded raw-byte snapshot aligned to a hexadecimal row.
    /// The returned data is independent of the document's text encoding.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid layout, out-of-range
    /// offset, or a request beyond the hexadecimal viewport budget.
    pub fn hex_viewport(
        &self,
        request: &HexViewportRequest,
    ) -> Result<HexViewportSnapshot, EditorError> {
        let layout = HexLayout::new(request.bytes_per_row)?;
        let row_width = u64::from(layout.bytes_per_row());
        if request.max_bytes < row_width || request.max_bytes > MAX_HEX_VIEWPORT_BYTES {
            return Err(EditorError::HexViewportLimit {
                requested: request.max_bytes,
                minimum: row_width,
                maximum: MAX_HEX_VIEWPORT_BYTES,
            });
        }
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(&request.document_id)
            .ok_or_else(|| EditorError::UnknownDocument(request.document_id.clone()))?;
        let document_len = session.document.table().len();
        if request.start_byte > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: request.start_byte,
                document_len,
            });
        }
        let start = layout.align_offset(ByteOffset(request.start_byte));
        let budget = request.max_bytes - request.max_bytes % row_width;
        let read_len = document_len.saturating_sub(start.0).min(budget);
        let bytes = session.document.table().read_vec(start.0, read_len)?;
        let rows = layout
            .rows(start, &bytes)
            .into_iter()
            .map(|row| HexViewportRow {
                start_byte: row.offset.0,
                bytes: row.bytes.to_vec(),
            })
            .collect();
        let consumed_end = start
            .0
            .checked_add(read_len)
            .ok_or(EditorError::OffsetOverflow)?;
        Ok(HexViewportSnapshot {
            document_id: request.document_id.clone(),
            revision: session.document.table().revision(),
            document_byte_len: document_len,
            bytes_per_row: layout.bytes_per_row(),
            rows,
            selections: session.selections.clone(),
            next: (consumed_end < document_len).then_some(HexCursor {
                byte_offset: consumed_end,
            }),
        })
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

    /// Applies one hexadecimal digit to the primary byte selection. Typing a
    /// high nibble retains the byte position for the low nibble; typing a low
    /// nibble advances to the next byte.
    ///
    /// # Errors
    /// Returns an error for a stale revision, invalid nibble, invalid selection,
    /// or backing-store failure.
    pub fn type_hex_nibble(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        high_nibble: bool,
        nibble: u8,
        overwrite: bool,
    ) -> Result<HexEditResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (start, end) = primary_selection_range(session);
        let document_len = session.document.table().len();
        if end > document_len {
            return Err(EditorError::OffsetOutOfRange {
                offset: end,
                document_len,
            });
        }
        let existing = if start == end && start < document_len {
            session.document.table().read_vec(start, 1)?[0]
        } else {
            0
        };
        let byte = replace_nibble(existing, high_nibble, nibble)?;
        let delete_end = if start == end && (overwrite || !high_nibble) {
            start.saturating_add(1).min(document_len)
        } else {
            end
        };
        let caret = HexCaret {
            byte_offset: if high_nibble {
                start
            } else {
                start.checked_add(1).ok_or(EditorError::OffsetOverflow)?
            },
            high_nibble: !high_nibble,
        };
        apply_hex_replacement(session, document_id, start, delete_end, &[byte], caret)
    }

    /// Replaces the primary byte selection with parsed hexadecimal bytes. A
    /// collapsed selection either inserts or overwrites raw bytes.
    ///
    /// # Errors
    /// Returns an error for stale state, malformed/oversized hexadecimal input,
    /// or backing-store failure.
    pub fn replace_hex_bytes(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        hex: &str,
        overwrite: bool,
    ) -> Result<HexEditResult, EditorError> {
        let bytes = parse_hex(hex, MAX_HEX_EDIT_BYTES)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (start, selected_end) = primary_selection_range(session);
        let document_len = session.document.table().len();
        let byte_len = u64::try_from(bytes.len()).map_err(|_| EditorError::OffsetOverflow)?;
        let end = if start != selected_end {
            selected_end
        } else if overwrite {
            start.saturating_add(byte_len).min(document_len)
        } else {
            start
        };
        let byte_offset = start
            .checked_add(byte_len)
            .ok_or(EditorError::OffsetOverflow)?;
        let caret = HexCaret {
            byte_offset,
            high_nibble: true,
        };
        if bytes.is_empty() {
            session.selections = vec![Selection::caret(start)];
            session.rectangular_selection = None;
            return Ok(HexEditResult {
                document: summary(document_id, session),
                caret,
            });
        }
        apply_hex_replacement(session, document_id, start, end, &bytes, caret)
    }

    /// Deletes the selected raw-byte range or exactly one preceding byte.
    ///
    /// # Errors
    /// Returns an error for stale state or backing-store failure.
    pub fn delete_hex_backward(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
    ) -> Result<HexEditResult, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (mut start, end) = primary_selection_range(session);
        if start == end {
            if start == 0 {
                return Ok(HexEditResult {
                    document: summary(document_id, session),
                    caret: HexCaret {
                        byte_offset: 0,
                        high_nibble: true,
                    },
                });
            }
            start -= 1;
        }
        let caret = HexCaret {
            byte_offset: start,
            high_nibble: true,
        };
        apply_hex_replacement(session, document_id, start, end, &[], caret)
    }

    /// Reads selected bytes as uppercase, space-separated hexadecimal text.
    ///
    /// # Errors
    /// Returns an error for an unknown document, oversized selection, or
    /// backing-store failure.
    pub fn hex_selection(
        &self,
        document_id: &DocumentId,
    ) -> Result<HexClipboardPayload, EditorError> {
        const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let ranges = editable_ranges(&session.selections)
            .into_iter()
            .filter(|(start, end)| start != end)
            .collect::<Vec<_>>();
        let selected = ranges.iter().try_fold(0_u64, |total, (start, end)| {
            total
                .checked_add(end - start)
                .ok_or(EditorError::OffsetOverflow)
        })?;
        if selected > MAX_HEX_CLIPBOARD_BYTES {
            return Err(EditorError::HexClipboardTooLarge {
                selected,
                maximum: MAX_HEX_CLIPBOARD_BYTES,
            });
        }
        let capacity = usize::try_from(selected)
            .map_err(|_| EditorError::OffsetOverflow)?
            .saturating_mul(3);
        let mut hex = String::with_capacity(capacity);
        for (start, end) in ranges {
            for byte in session.document.table().read_vec(start, end - start)? {
                if !hex.is_empty() {
                    hex.push(' ');
                }
                hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
                hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
            }
        }
        Ok(HexClipboardPayload {
            hex,
            revision: session.document.table().revision(),
        })
    }

    /// Applies a replacement only against the revision observed by the UI.
    ///
    /// # Errors
    /// Returns an error for stale revisions, invalid ranges, or oversized edits.
    pub fn replace(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        start_byte: u64,
        end_byte: u64,
        text: &str,
    ) -> Result<DocumentSummary, EditorError> {
        if end_byte < start_byte {
            return Err(EditorError::InvalidRange {
                start: start_byte,
                end: end_byte,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let actual_revision = session.document.table().revision();
        if actual_revision != expected_revision {
            return Err(EditorError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        let encoded = encode_text(session.encoding, text)?;
        replace_range(session, start_byte, end_byte, &encoded)?;
        Ok(summary(document_id, session))
    }

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

    /// Selects a Unicode word, whitespace run, or punctuation run at a byte position.
    /// The scan is bounded independently of the source file size.
    ///
    /// # Errors
    /// Returns an error for an invalid offset, an overlong token, or a source read failure.
    pub fn select_word_at(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        byte_offset: u64,
    ) -> Result<Selection, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let table = session.document.table();
        let len = table.len();
        if byte_offset > len {
            return Err(EditorError::OffsetOutOfRange {
                offset: byte_offset,
                document_len: len,
            });
        }
        let selection = word_selection(table, session.encoding, byte_offset)?;
        session.selections = vec![selection];
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None];
        Ok(selection)
    }

    /// Moves or copies one ordinary text selection within its document as one
    /// revision-checked undo action, preserving the exact source bytes.
    ///
    /// # Errors
    /// Returns an error for stale revision, invalid range, oversized text, or storage failure.
    pub fn move_selected_text(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        start_byte: u64,
        end_byte: u64,
        destination_byte: u64,
        copy: bool,
    ) -> Result<DocumentSummary, EditorError> {
        if start_byte >= end_byte {
            return Err(EditorError::InvalidRange {
                start: start_byte,
                end: end_byte,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let len = session.document.table().len();
        if end_byte > len || destination_byte > len {
            return Err(EditorError::OffsetOutOfRange {
                offset: end_byte.max(destination_byte),
                document_len: len,
            });
        }
        let selected = end_byte - start_byte;
        if selected > MAX_EDIT_BYTES as u64 {
            return Err(EditorError::EditTooLarge(
                usize::try_from(selected).unwrap_or(usize::MAX),
            ));
        }
        let selection_matches = session.rectangular_selection.is_none()
            && session.selections.len() == 1
            && session.selections[0]
                .anchor_byte
                .min(session.selections[0].active_byte)
                == start_byte
            && session.selections[0]
                .anchor_byte
                .max(session.selections[0].active_byte)
                == end_byte;
        if !selection_matches {
            return Err(EditorError::InvalidRange {
                start: start_byte,
                end: end_byte,
            });
        }
        if (start_byte..=end_byte).contains(&destination_byte) {
            return Ok(summary(document_id, session));
        }
        let bytes = session.document.table().read_vec(start_byte, selected)?;
        let inserted_len = bytes.len();
        let edits = if copy {
            session.document.replace(destination_byte, 0, &bytes)?;
            vec![(destination_byte, destination_byte, inserted_len)]
        } else {
            let batch = if destination_byte > end_byte {
                vec![
                    (destination_byte, 0, bytes),
                    (start_byte, selected, Vec::new()),
                ]
            } else {
                vec![
                    (start_byte, selected, Vec::new()),
                    (destination_byte, 0, bytes),
                ]
            };
            session.document.replace_many(&batch)?;
            if destination_byte > end_byte {
                vec![
                    (start_byte, end_byte, 0),
                    (destination_byte, destination_byte, inserted_len),
                ]
            } else {
                vec![
                    (destination_byte, destination_byte, inserted_len),
                    (start_byte, end_byte, 0),
                ]
            }
        };
        relocate_bookmarks(&mut session.bookmarks, &edits)?;
        record_bookmark_edit(session, edits);
        let caret = if copy || destination_byte < start_byte {
            destination_byte
                .checked_add(selected)
                .ok_or(EditorError::OffsetOverflow)?
        } else {
            destination_byte
        };
        session.selections = vec![Selection::caret(caret)];
        session.preferred_scalar_columns = vec![None];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }

    /// Inserts text at every selection. In overwrite mode, each collapsed
    /// caret replaces the same number of UTF-8 scalars without crossing a line ending.
    ///
    /// # Errors
    /// Returns an error for stale revisions, oversized text, or storage failure.
    pub fn insert_text(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        text: &str,
        overwrite: bool,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let inserted = encode_text(session.encoding, text)?;
        if inserted.len() > MAX_EDIT_BYTES {
            return Err(EditorError::EditTooLarge(inserted.len()));
        }
        if session.rectangular_selection.is_some() {
            insert_rectangular_text(session, text, overwrite)?;
            return Ok(summary(document_id, session));
        }
        let mut replacements = editable_ranges(&session.selections);
        for (start, end) in &mut replacements {
            if overwrite && start == end && !text.contains(['\r', '\n']) {
                *end = overwrite_end(session, *start, text.chars().count())?;
            }
        }
        let replacements = merge_ranges(replacements);
        let batch = replacements
            .iter()
            .rev()
            .map(|(start, end)| (*start, end - start, inserted.clone()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let bookmark_edits = replacements
            .iter()
            .map(|(start, end)| (*start, *end, inserted.len()))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(session, bookmark_edits);
        session.selections = carets_after_replacements(&replacements, inserted.len())?;
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }

    /// Reads selected ranges for clipboard transfer under a fixed byte cap.
    /// Ranges are joined with newlines and invalid UTF-8 is made visible with
    /// replacement characters.
    ///
    /// # Errors
    /// Returns an error for an unknown document, an oversized selection, or storage failure.
    pub fn selection_text(
        &self,
        document_id: &DocumentId,
    ) -> Result<ClipboardPayload, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let ranges = if let Some(rectangle) = session.rectangular_selection {
            if rectangle.anchor_column == rectangle.active_column {
                Vec::new()
            } else {
                rectangular_ranges(session)?
                    .into_iter()
                    .map(|range| (range.start, range.end))
                    .collect::<Vec<_>>()
            }
        } else {
            editable_ranges(&session.selections)
                .into_iter()
                .filter(|(start, end)| start != end)
                .collect::<Vec<_>>()
        };
        let selected_len = ranges.iter().try_fold(0_u64, |total, (start, end)| {
            total
                .checked_add(end - start)
                .ok_or(EditorError::OffsetOverflow)
        })?;
        let separator_len = u64::try_from(ranges.len().saturating_sub(1))
            .map_err(|_| EditorError::OffsetOverflow)?;
        if selected_len.saturating_add(separator_len) > MAX_CLIPBOARD_BYTES {
            return Err(EditorError::ClipboardTooLarge {
                selected: selected_len.saturating_add(separator_len),
                maximum: MAX_CLIPBOARD_BYTES,
            });
        }
        let mut text = String::new();
        let mut had_invalid_utf8 = false;
        for (index, (start, end)) in ranges.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            let bytes = session.document.table().read_vec(*start, end - start)?;
            had_invalid_utf8 |=
                session.encoding == TextEncoding::Utf8 && std::str::from_utf8(&bytes).is_err();
            text.push_str(&decode_text(session.encoding, &bytes).0);
        }
        Ok(ClipboardPayload {
            text,
            revision: session.document.table().revision(),
            had_invalid_utf8,
        })
    }

    /// Deletes every selected range after checking the revision. Collapsed
    /// selections are ignored.
    ///
    /// # Errors
    /// Returns an error for an unknown document, stale revision, or storage failure.
    pub fn delete_selection(
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
        let rectangle = session.rectangular_selection;
        let ranges = if rectangle.is_some() {
            rectangular_ranges(session)?
                .into_iter()
                .map(|range| (range.start, range.end))
                .filter(|(start, end)| start != end)
                .collect::<Vec<_>>()
        } else {
            editable_ranges(&session.selections)
                .into_iter()
                .filter(|(start, end)| start != end)
                .collect::<Vec<_>>()
        };
        if !ranges.is_empty() {
            let batch = ranges
                .iter()
                .rev()
                .map(|(start, end)| (*start, end - start, Vec::new()))
                .collect::<Vec<_>>();
            session.document.replace_many(&batch)?;
            let bookmark_edits = ranges
                .iter()
                .map(|(start, end)| (*start, *end, 0))
                .collect::<Vec<_>>();
            relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
            record_bookmark_edit(session, bookmark_edits);
            let carets = carets_after_replacements(&ranges, 0)?;
            session.selections = active_rectangle_caret(rectangle, carets);
            session.rectangular_selection = None;
            session.preferred_scalar_columns = vec![None; session.selections.len()];
            session.navigation_index.reset();
        }
        Ok(summary(document_id, session))
    }

    /// Deletes the active selection or the preceding UTF-8 scalar. Malformed
    /// content falls back to deleting one byte so arbitrary source data remains editable.
    ///
    /// # Errors
    /// Returns an error for unknown documents, stale revisions, or storage failure.
    pub fn delete_backward(
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
        let actual_revision = session.document.table().revision();
        if actual_revision != expected_revision {
            return Err(EditorError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        let rectangle = session.rectangular_selection;
        let rectangular = rectangle.is_some();
        let source_ranges: Vec<(u64, u64)> = if rectangular {
            rectangular_ranges(session)?
                .into_iter()
                .map(|range| (range.start, range.end))
                .collect()
        } else {
            editable_ranges(&session.selections)
        };
        let rectangular_rows = source_ranges.len();
        let mut ranges = Vec::new();
        for (mut start, end) in source_ranges {
            if start == end {
                if start == 0
                    || (rectangular && start == find_line_start(session.document.table(), start)?)
                {
                    continue;
                }
                // Inside indentation made of spaces: back to the previous stop.
                if !rectangular {
                    if let Some(width) = self.indentation_backspace(session, start)? {
                        ranges.push((start - width, end));
                        continue;
                    }
                }
                let probe_start = start.saturating_sub(4);
                let bytes = session
                    .document
                    .table()
                    .read_vec(probe_start, start - probe_start)?;
                let relative = match session.encoding {
                    TextEncoding::Utf8 => previous_scalar_start(&bytes).unwrap_or(bytes.len() - 1),
                    TextEncoding::Windows1252 => bytes.len() - 1,
                };
                start = probe_start
                    .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
                    .ok_or(EditorError::OffsetOverflow)?;
            }
            ranges.push((start, end));
        }
        let ranges = merge_ranges(ranges);
        if ranges.is_empty() {
            return Ok(summary(document_id, session));
        }
        let batch = ranges
            .iter()
            .rev()
            .map(|(start, end)| (*start, end - start, Vec::new()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let bookmark_edits = ranges
            .iter()
            .map(|(start, end)| (*start, *end, 0))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(session, bookmark_edits);
        let carets = carets_after_replacements(&ranges, 0)?;
        // Backspace keeps the column while every row took part in the edit.
        let kept = rectangle
            .filter(|_| carets.len() == rectangular_rows)
            .and_then(|rectangle| {
                let left = rectangle.anchor_column.min(rectangle.active_column);
                let collapsed = rectangle.anchor_column == rectangle.active_column;
                column_after_edit(rectangle, &carets, left - u64::from(collapsed && left > 0))
            });
        session.selections = active_rectangle_caret(rectangle, carets);
        session.rectangular_selection = kept;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }

    /// Replaces all selection state after validating every 64-bit offset.
    ///
    /// # Errors
    /// Returns an error for unknown documents, no selections, or out-of-range offsets.
    pub fn set_selections(
        &self,
        document_id: &DocumentId,
        selections: Vec<Selection>,
    ) -> Result<Vec<Selection>, EditorError> {
        if selections.is_empty() {
            return Err(EditorError::EmptySelections);
        }
        if selections.len() > MAX_SELECTIONS {
            return Err(EditorError::TooManySelections {
                count: selections.len(),
                maximum: MAX_SELECTIONS,
            });
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let len = session.document.table().len();
        if let Some(offset) = selections
            .iter()
            .flat_map(|selection| [selection.anchor_byte, selection.active_byte])
            .find(|offset| *offset > len)
        {
            return Err(EditorError::OffsetOutOfRange {
                offset,
                document_len: len,
            });
        }
        session.selections = deduplicate_selections(selections);
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        Ok(session.selections.clone())
    }

    /// Replaces normal selection state with one constant-size rectangular
    /// descriptor. Line-local byte ranges remain derived and bounded.
    ///
    /// # Errors
    /// Returns an error for an unknown document, invalid offsets, or a
    /// rectangle taller than [`MAX_RECTANGULAR_ROWS`].
    pub fn set_rectangular_selection(
        &self,
        document_id: &DocumentId,
        anchor_byte: u64,
        active_byte: u64,
        requested_anchor_column: u64,
        requested_active_column: u64,
    ) -> Result<RectangularSelection, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let document_len = session.document.table().len();
        if let Some(offset) = [anchor_byte, active_byte]
            .into_iter()
            .find(|offset| *offset > document_len)
        {
            return Err(EditorError::OffsetOutOfRange {
                offset,
                document_len,
            });
        }
        let table = session.document.table();
        let anchor_start = find_line_start(table, anchor_byte)?;
        let active_start = find_line_start(table, active_byte)?;
        let anchor_boundary = find_line_boundary(table, anchor_start)?;
        let active_boundary = find_line_boundary(table, active_start)?;
        let anchor_line = line_number_at_offset(table, &session.navigation_index, anchor_start)?;
        let active_line = line_number_at_offset(table, &session.navigation_index, active_start)?;
        let row_count = anchor_line.abs_diff(active_line).saturating_add(1);
        if row_count > MAX_RECTANGULAR_ROWS as u64 {
            return Err(EditorError::RectangularSelectionTooTall {
                rows: row_count,
                maximum: MAX_RECTANGULAR_ROWS,
            });
        }
        let anchor_column = scalar_count(
            table,
            anchor_start,
            anchor_byte.min(anchor_boundary.content_end),
            session.encoding,
        )?;
        let active_column = scalar_count(
            table,
            active_start,
            active_byte.min(active_boundary.content_end),
            session.encoding,
        )?;
        let rectangle = RectangularSelection {
            anchor_byte,
            active_byte,
            anchor_line,
            active_line,
            anchor_column: if anchor_byte >= anchor_boundary.content_end {
                anchor_column.max(requested_anchor_column)
            } else {
                anchor_column
            },
            active_column: if active_byte >= active_boundary.content_end {
                active_column.max(requested_active_column)
            } else {
                active_column
            },
        };
        session.selections = vec![Selection::caret(active_byte)];
        session.rectangular_selection = Some(rectangle);
        session.preferred_scalar_columns = vec![None];
        Ok(rectangle)
    }

    /// Moves every caret by one decoded UTF-8 scalar while preserving
    /// malformed bytes as individually navigable units, or to the start or
    /// end of its line or of the document.
    ///
    /// # Errors
    /// Returns an error for an unknown document or backing-store read failure.
    pub fn move_horizontal(
        &self,
        document_id: &DocumentId,
        direction: HorizontalDirection,
        extend: bool,
    ) -> Result<Vec<Selection>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let mut moved = Vec::with_capacity(session.selections.len());
        for current in &session.selections {
            let active = match direction {
                HorizontalDirection::Left if current.active_byte > 0 => {
                    let probe_start = current.active_byte.saturating_sub(4);
                    let bytes = session
                        .document
                        .table()
                        .read_vec(probe_start, current.active_byte - probe_start)?;
                    let relative = match session.encoding {
                        TextEncoding::Utf8 => {
                            previous_scalar_start(&bytes).unwrap_or(bytes.len() - 1)
                        }
                        TextEncoding::Windows1252 => bytes.len() - 1,
                    };
                    probe_start
                        .checked_add(
                            u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?,
                        )
                        .ok_or(EditorError::OffsetOverflow)?
                }
                HorizontalDirection::Left | HorizontalDirection::DocumentStart => 0,
                HorizontalDirection::Right
                    if current.active_byte < session.document.table().len() =>
                {
                    let available = (session.document.table().len() - current.active_byte).min(4);
                    let bytes = session
                        .document
                        .table()
                        .read_vec(current.active_byte, available)?;
                    let width = navigable_scalar_width(session.encoding, &bytes);
                    current
                        .active_byte
                        .checked_add(u64::try_from(width).map_err(|_| EditorError::OffsetOverflow)?)
                        .ok_or(EditorError::OffsetOverflow)?
                }
                HorizontalDirection::Right | HorizontalDirection::DocumentEnd => {
                    session.document.table().len()
                }
                HorizontalDirection::LineStart => {
                    line_ops::smart_line_start(session.document.table(), current.active_byte)?
                }
                HorizontalDirection::LineEnd => {
                    let table = session.document.table();
                    find_line_boundary(table, find_line_start(table, current.active_byte)?)?
                        .content_end
                }
            };
            moved.push(if extend {
                Selection {
                    anchor_byte: current.anchor_byte,
                    active_byte: active,
                }
            } else {
                Selection::caret(active)
            });
        }
        session.selections = deduplicate_selections(moved);
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        Ok(session.selections.clone())
    }

    /// Moves every caret by one logical line while retaining its desired
    /// UTF-8 scalar column across shorter lines. Line discovery and scalar
    /// counting use fixed-size reads, including for multi-gigabyte documents.
    ///
    /// # Errors
    /// Returns an error for an unknown document or backing-store read failure.
    pub fn move_vertical(
        &self,
        document_id: &DocumentId,
        direction: VerticalDirection,
        extend: bool,
    ) -> Result<Vec<Selection>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let table = session.document.table();
        let mut moved = Vec::with_capacity(session.selections.len());
        let mut preferred = Vec::with_capacity(session.selections.len());
        for (index, current) in session.selections.iter().copied().enumerate() {
            let current_start = find_line_start(table, current.active_byte)?;
            let current_line = find_line_boundary(table, current_start)?;
            let current_text_offset = current.active_byte.min(current_line.content_end);
            let preferred_column = session
                .preferred_scalar_columns
                .get(index)
                .copied()
                .flatten()
                .map_or_else(
                    || scalar_count(table, current_start, current_text_offset, session.encoding),
                    Ok,
                )?;
            let target_range = match direction {
                VerticalDirection::Up => previous_line_range(table, current_start)?,
                VerticalDirection::Down => match current_line.next_start {
                    Some(start) => {
                        let boundary = find_line_boundary(table, start)?;
                        Some((start, boundary.content_end))
                    }
                    None => None,
                },
            };
            let active = match target_range {
                Some((start, end)) => {
                    byte_at_scalar_column(table, start, end, preferred_column, session.encoding)?
                }
                None => current.active_byte,
            };
            preferred.push(Some(preferred_column));
            moved.push(if extend {
                Selection {
                    anchor_byte: current.anchor_byte,
                    active_byte: active,
                }
            } else {
                Selection::caret(active)
            });
        }
        session.selections = moved;
        session.rectangular_selection = None;
        session.preferred_scalar_columns = preferred;
        Ok(session.selections.clone())
    }

    /// Changes how source bytes are decoded and how subsequently entered text
    /// is encoded. Existing bytes are preserved.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn set_encoding(
        &self,
        document_id: &DocumentId,
        encoding: TextEncoding,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        session.encoding = encoding;
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        Ok(summary(document_id, session))
    }

    /// Applies a Unicode-aware transform to every non-empty selected range as
    /// one undoable action.
    ///
    /// # Errors
    /// Returns an error for stale revisions, invalid UTF-8, oversized output,
    /// or backing-store failure.
    pub fn transform_selections(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        transform: TextTransform,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let rectangle = session.rectangular_selection;
        let ranges = if rectangle.is_some() {
            rectangular_ranges(session)?
                .into_iter()
                .map(|range| (range.start, range.end))
                .filter(|(start, end)| start != end)
                .collect::<Vec<_>>()
        } else {
            editable_ranges(&session.selections)
                .into_iter()
                .filter(|(start, end)| start != end)
                .collect::<Vec<_>>()
        };
        if ranges.is_empty() {
            return Ok(summary(document_id, session));
        }

        let selected_bytes = ranges.iter().try_fold(0_usize, |total, (start, end)| {
            let len = usize::try_from(end - start).map_err(|_| EditorError::OffsetOverflow)?;
            total.checked_add(len).ok_or(EditorError::OffsetOverflow)
        })?;
        if selected_bytes > MAX_EDIT_BYTES {
            return Err(EditorError::EditTooLarge(selected_bytes));
        }

        let mut transformed = Vec::with_capacity(ranges.len());
        let mut output_bytes = 0_usize;
        for (start, end) in &ranges {
            let bytes = session.document.table().read_vec(*start, end - start)?;
            let output = if transform == TextTransform::Base64Encode {
                // Any bytes can be encoded, including text that is not valid in the encoding.
                BASE64.encode(&bytes).into_bytes()
            } else {
                if session.encoding == TextEncoding::Utf8 && std::str::from_utf8(&bytes).is_err() {
                    return Err(EditorError::InvalidUtf8Selection);
                }
                let text = decode_text(session.encoding, &bytes).0;
                match transform {
                    TextTransform::Base64Decode => {
                        let compact = text.split_whitespace().collect::<String>();
                        let decoded = BASE64
                            .decode(compact.as_bytes())
                            .map_err(|_| EditorError::InvalidBase64)?;
                        if session.encoding == TextEncoding::Utf8
                            && std::str::from_utf8(&decoded).is_err()
                        {
                            return Err(EditorError::Base64NotText);
                        }
                        decoded
                    }
                    _ => encode_text(
                        session.encoding,
                        &match transform {
                            TextTransform::Uppercase => text.to_uppercase(),
                            TextTransform::Lowercase => text.to_lowercase(),
                            TextTransform::TrimWhitespace => text.trim().to_owned(),
                            TextTransform::InvertCase => invert_case(&text),
                            TextTransform::Capitalize => capitalize_words(&text),
                            TextTransform::Base64Encode | TextTransform::Base64Decode => {
                                unreachable!("handled above")
                            }
                        },
                    )?,
                }
            };
            output_bytes = output_bytes
                .checked_add(output.len())
                .ok_or(EditorError::OffsetOverflow)?;
            if output_bytes > MAX_EDIT_BYTES {
                return Err(EditorError::EditTooLarge(output_bytes));
            }
            transformed.push(output);
        }

        let batch = ranges
            .iter()
            .zip(&transformed)
            .rev()
            .map(|((start, end), output)| (*start, end - start, output.clone()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let bookmark_edits = ranges
            .iter()
            .zip(&transformed)
            .map(|((start, end), output)| (*start, *end, output.len()))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(session, bookmark_edits);
        let selections = selections_after_replacements(&ranges, &transformed)?;
        session.selections = active_rectangle_caret(rectangle, selections);
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.navigation_index.reset();
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

    /// Toggles a bookmark at the primary caret.
    ///
    /// # Errors
    /// Returns an error for an unknown document or when the bookmark limit is reached.
    pub fn toggle_bookmark(&self, document_id: &DocumentId) -> Result<Vec<u64>, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let offset = session
            .selections
            .first()
            .map_or(0, |selection| selection.active_byte);
        if !session.bookmarks.remove(&offset) {
            if session.bookmarks.len() == MAX_BOOKMARKS {
                return Err(EditorError::TooManyBookmarks(MAX_BOOKMARKS));
            }
            session.bookmarks.insert(offset);
        }
        Ok(session.bookmarks.iter().copied().collect())
    }

    /// Moves the primary caret to the next or previous bookmark, wrapping at
    /// the ends of the document. A document with no bookmarks is unchanged.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn move_to_bookmark(
        &self,
        document_id: &DocumentId,
        direction: BookmarkDirection,
    ) -> Result<Vec<Selection>, EditorError> {
        use std::ops::Bound::{Excluded, Unbounded};

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let current = session
            .selections
            .first()
            .map_or(0, |selection| selection.active_byte);
        let target = match direction {
            BookmarkDirection::Next => session
                .bookmarks
                .range((Excluded(current), Unbounded))
                .next()
                .or_else(|| session.bookmarks.first()),
            BookmarkDirection::Previous => session
                .bookmarks
                .range((Unbounded, Excluded(current)))
                .next_back()
                .or_else(|| session.bookmarks.last()),
        }
        .copied();
        if let Some(target) = target {
            session.selections = vec![Selection::caret(target)];
            session.rectangular_selection = None;
            session.preferred_scalar_columns = vec![None];
        }
        Ok(session.selections.clone())
    }

    /// Removes all bookmarks from a document.
    ///
    /// # Errors
    /// Returns an error for an unknown document.
    pub fn clear_bookmarks(&self, document_id: &DocumentId) -> Result<(), EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        session.bookmarks.clear();
        Ok(())
    }

    /// Checks a document with its language's validator and reports the
    /// first problem, if any, with what Beautify would lose.
    ///
    /// # Errors
    /// Returns an error for an unknown document, a language without tools, or
    /// a document above the tool size limit.
    pub fn validate_document(
        &self,
        document_id: &DocumentId,
    ) -> Result<ValidationReport, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let (label, source) = self.tool_source(session)?;
        let registry = self.capability_registry();
        let validator = registry
            .resolve(&session.language_id, CapabilityKind::Diagnostics)
            .ok_or_else(|| {
                EditorError::Tool(format!("Validate is not available for {label} documents"))
            })?;
        let Provider::Validator(check) = validator.provider.clone() else {
            return Err(EditorError::Tool(format!(
                "Validate is not available for {label} documents"
            )));
        };
        let formatter = registry.resolve(&session.language_id, CapabilityKind::Format);
        let checked = source.and_then(|text| {
            let input = DocumentText {
                language: &session.language_id,
                text: &text,
            };
            let problem = registry
                .call(&validator.info, || check.validate(&input))
                .map_err(|error| ToolProblem::from_provider(session.encoding, &text, error))?;
            if let Some(problem) = problem {
                return Err(ToolProblem::from_provider(
                    session.encoding,
                    &text,
                    ProviderError::Content(problem),
                ));
            }
            // What Beautify would do to this document, so the editor can ask first.
            let assessment = match &formatter {
                Some(resolved) => match &resolved.provider {
                    Provider::Formatter(format) => registry
                        .call(&resolved.info, || format.assess(&input))
                        .unwrap_or(FormatAssessment::Ready),
                    _ => FormatAssessment::Ready,
                },
                None => FormatAssessment::Ready,
            };
            Ok(assessment)
        });
        Ok(match checked {
            Ok(assessment) => {
                let (beautify_note, beautify_unavailable) = match assessment {
                    FormatAssessment::Ready => (None, false),
                    FormatAssessment::Lossy(note) => (Some(note), false),
                    FormatAssessment::Unavailable(note) => (Some(note), true),
                };
                ValidationReport {
                    valid: true,
                    message: format!("Valid {label}"),
                    byte_offset: None,
                    line: None,
                    column: None,
                    beautify_note,
                    beautify_unavailable,
                }
            }
            Err(problem) => ValidationReport {
                valid: false,
                message: problem.describe(&label),
                byte_offset: Some(problem.byte_offset.to_string()),
                line: Some(problem.line),
                column: Some(problem.column),
                beautify_note: None,
                beautify_unavailable: false,
            },
        })
    }

    /// Re-indents a document with its language's formatter, or minifies it,
    /// as one revision-checked undo step. An already formatted document is
    /// unchanged. A formatter that would lose something (YAML comments and
    /// anchors) runs only with `accept_loss`.
    ///
    /// # Errors
    /// Returns an error for a stale revision, a language without tools, an
    /// oversized or invalid document, a loss not accepted, or a storage failure.
    pub fn format_document(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        minify: bool,
        accept_loss: bool,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let (label, source) = self.tool_source(session)?;
        let (kind, mode) = if minify {
            (CapabilityKind::Compact, FormatMode::Compact)
        } else {
            (CapabilityKind::Format, FormatMode::Beautify)
        };
        let resolved = self
            .capability_registry()
            .resolve(&session.language_id, kind);
        let formatted = source.and_then(|text| {
            let Some(resolved) = resolved else {
                return Err(ToolProblem::plain(&if minify {
                    format!(
                        "Minify is available for {} only",
                        self.provider_labels(CapabilityKind::Compact)
                    )
                } else {
                    format!("Beautify is not available for {label} documents")
                }));
            };
            let Provider::Formatter(format) = &resolved.provider else {
                return Err(ToolProblem::plain("This provider cannot format"));
            };
            let input = DocumentText {
                language: &session.language_id,
                text: &text,
            };
            match self
                .capability_registry()
                .call(&resolved.info, || format.format(&input, mode, accept_loss))
            {
                Ok(FormatOutcome::Unchanged) => Ok(None),
                Ok(FormatOutcome::Replaced(formatted)) => Ok(Some(formatted)),
                Err(error) => Err(ToolProblem::from_provider(session.encoding, &text, error)),
            }
        });
        let formatted = formatted.map_err(|problem| EditorError::Tool(problem.describe(&label)))?;
        if let Some(formatted) = formatted {
            replace_whole_text(session, &formatted)?;
        }
        Ok(summary(document_id, session))
    }

    /// Undoes one edit and returns current metadata.
    ///
    /// # Errors
    /// Returns an error for unknown documents or backing-store failure.
    pub fn undo(&self, document_id: &DocumentId) -> Result<DocumentSummary, EditorError> {
        self.history_action(document_id, HistoryDirection::Undo)
    }

    /// Redoes one edit and returns current metadata.
    ///
    /// # Errors
    /// Returns an error for unknown documents or backing-store failure.
    pub fn redo(&self, document_id: &DocumentId) -> Result<DocumentSummary, EditorError> {
        self.history_action(document_id, HistoryDirection::Redo)
    }

    fn history_action(
        &self,
        document_id: &DocumentId,
        direction: HistoryDirection,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let changed = match direction {
            HistoryDirection::Undo => session.document.undo()?,
            HistoryDirection::Redo => session.document.redo()?,
        };
        session.snippet = None;
        if changed {
            let entry = match direction {
                HistoryDirection::Undo => session.bookmark_undo.pop_back(),
                HistoryDirection::Redo => session.bookmark_redo.pop_back(),
            };
            if let Some(mut entry) = entry {
                match direction {
                    HistoryDirection::Undo => {
                        let inverse = inverse_bookmark_edits(&entry.edits)?;
                        relocate_bookmarks(&mut session.bookmarks, &inverse)?;
                        // Markers recorded before a save are no longer exact;
                        // the undone bytes then differ from the saved file.
                        if !session.changes.revert(&entry.changes) {
                            session.changes.apply(&inverse);
                        }
                        push_bounded_history(&mut session.bookmark_redo, entry);
                    }
                    HistoryDirection::Redo => {
                        relocate_bookmarks(&mut session.bookmarks, &entry.edits)?;
                        entry.changes = session.changes.apply(&entry.edits);
                        push_bounded_history(&mut session.bookmark_undo, entry);
                    }
                }
            } else {
                let document_len = session.document.table().len();
                session.bookmarks.retain(|offset| *offset <= document_len);
                session.changes.invalidate();
            }
        }
        let caret = session
            .selections
            .first()
            .map_or(0, |selection| selection.active_byte)
            .min(session.document.table().len());
        session.selections = vec![Selection::caret(caret)];
        session.rectangular_selection = None;
        session.preferred_scalar_columns = vec![None];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }
}

/// A tool failure placed in document coordinates.
struct ToolProblem {
    message: String,
    byte_offset: u64,
    line: u64,
    column: u64,
    located: bool,
}

impl ToolProblem {
    fn plain(message: &str) -> Self {
        Self {
            message: message.into(),
            byte_offset: 0,
            line: 1,
            column: 1,
            located: false,
        }
    }

    /// Converts an offset into the decoded text to a document byte offset and
    /// a one-based line and character column.
    fn locate(encoding: TextEncoding, text: &str, error: format_tools::ToolError) -> Self {
        let before = &text[..error.offset.min(text.len())];
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let byte_offset = match encoding {
            TextEncoding::Utf8 => before.len(),
            // Every Windows-1252 character is one stored byte.
            TextEncoding::Windows1252 => before.chars().count(),
        };
        Self {
            message: error.message,
            byte_offset: byte_offset as u64,
            line: before.matches('\n').count() as u64 + 1,
            column: before[line_start..].chars().count() as u64 + 1,
            located: true,
        }
    }

    /// Places a provider's error: a content problem gets a line and column.
    fn from_provider(encoding: TextEncoding, text: &str, error: ProviderError) -> Self {
        match error {
            ProviderError::Content(problem) => Self::locate(
                encoding,
                text,
                format_tools::ToolError {
                    message: problem.message,
                    offset: problem.offset,
                },
            ),
            ProviderError::Refused(message) | ProviderError::Failed(message) => {
                Self::plain(&message)
            }
        }
    }

    fn describe(&self, label: &str) -> String {
        if self.located {
            format!(
                "Invalid {label}: {} (line {}, column {})",
                self.message, self.line, self.column
            )
        } else {
            self.message.clone()
        }
    }
}

/// Reads the whole document for a built-in tool. The outer error means the
/// tool cannot run at all; the inner one is a problem with the content.
fn tool_source(
    session: &EditorSession,
    available_for: impl FnOnce() -> String,
) -> Result<(String, Result<String, ToolProblem>), EditorError> {
    let label = session
        .capabilities
        .tool_label()
        .map(str::to_owned)
        .ok_or_else(|| {
            EditorError::Tool(format!(
                "These tools are available for {} documents",
                available_for()
            ))
        })?;
    let table = session.document.table();
    if table.len() > MAX_TOOL_BYTES {
        return Err(EditorError::Tool(format!(
            "{label} tools handle documents up to {} MiB",
            MAX_TOOL_BYTES / (1024 * 1024)
        )));
    }
    let bytes = table.read_vec(0, table.len())?;
    let text = match session.encoding {
        TextEncoding::Utf8 => String::from_utf8(bytes).map_err(|error| {
            let valid = error.utf8_error().valid_up_to();
            let text = String::from_utf8_lossy(&error.as_bytes()[..valid]).into_owned();
            ToolProblem::locate(
                TextEncoding::Utf8,
                &text,
                format_tools::ToolError {
                    message: "the file is not valid UTF-8 here".into(),
                    offset: valid,
                },
            )
        }),
        TextEncoding::Windows1252 => Ok(decode_text(session.encoding, &bytes).0),
    };
    Ok((label, text))
}

fn primary_selection_range(session: &EditorSession) -> (u64, u64) {
    session.selections.first().map_or((0, 0), |selection| {
        (
            selection.anchor_byte.min(selection.active_byte),
            selection.anchor_byte.max(selection.active_byte),
        )
    })
}

/// Replaces `start..end` with `encoded` as one undo step, leaving the caret
/// after the new text and moving bookmarks with the edit.
fn replace_range(
    session: &mut EditorSession,
    start_byte: u64,
    end_byte: u64,
    encoded: &[u8],
) -> Result<(), EditorError> {
    if encoded.len() > MAX_EDIT_BYTES {
        return Err(EditorError::EditTooLarge(encoded.len()));
    }
    session
        .document
        .replace(start_byte, end_byte - start_byte, encoded)?;
    let inserted_len = u64::try_from(encoded.len()).map_err(|_| EditorError::OffsetOverflow)?;
    let caret = start_byte
        .checked_add(inserted_len)
        .ok_or(EditorError::OffsetOverflow)?;
    session.selections = vec![Selection::caret(caret)];
    session.rectangular_selection = None;
    let bookmark_edits = vec![(start_byte, end_byte, encoded.len())];
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);
    session.preferred_scalar_columns = vec![None];
    session.navigation_index.reset();
    Ok(())
}

fn apply_hex_replacement(
    session: &mut EditorSession,
    document_id: &DocumentId,
    start: u64,
    end: u64,
    inserted: &[u8],
    caret: HexCaret,
) -> Result<HexEditResult, EditorError> {
    if end < start {
        return Err(EditorError::InvalidRange { start, end });
    }
    session.document.replace(start, end - start, inserted)?;
    let bookmark_edits = vec![(start, end, inserted.len())];
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);
    session.selections = vec![Selection::caret(caret.byte_offset)];
    session.rectangular_selection = None;
    session.preferred_scalar_columns = vec![None];
    session.navigation_index.reset();
    Ok(HexEditResult {
        document: summary(document_id, session),
        caret,
    })
}

fn insert_rectangular_text(
    session: &mut EditorSession,
    text: &str,
    overwrite: bool,
) -> Result<(), EditorError> {
    let rectangle = session
        .rectangular_selection
        .ok_or(EditorError::EmptySelections)?;
    let mut rectangular = rectangular_ranges(session)?;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let row_texts = normalized.split('\n').collect::<Vec<_>>();
    if row_texts.len() > 1 && row_texts.len() != rectangular.len() {
        return Err(EditorError::RectangularPasteRowMismatch {
            supplied: row_texts.len(),
            selected: rectangular.len(),
        });
    }
    let row_text = |index: usize| {
        if row_texts.len() == 1 {
            row_texts[0]
        } else {
            row_texts[index]
        }
    };
    if overwrite && rectangle.anchor_column == rectangle.active_column {
        for (index, range) in rectangular.iter_mut().enumerate() {
            if range.missing_columns == 0 {
                range.end = overwrite_end(session, range.start, row_text(index).chars().count())?;
            }
        }
    }
    let mut inserted_by_range = Vec::with_capacity(rectangular.len());
    let mut total = 0_usize;
    for (index, range) in rectangular.iter().enumerate() {
        let padding =
            usize::try_from(range.missing_columns).map_err(|_| EditorError::OffsetOverflow)?;
        let row_bytes = encode_text(session.encoding, row_text(index))?;
        let row_len = padding
            .checked_add(row_bytes.len())
            .ok_or(EditorError::OffsetOverflow)?;
        total = total
            .checked_add(row_len)
            .ok_or(EditorError::OffsetOverflow)?;
        if total > MAX_EDIT_BYTES {
            return Err(EditorError::EditTooLarge(total));
        }
        let mut bytes = Vec::with_capacity(row_len);
        bytes.resize(padding, b' ');
        bytes.extend_from_slice(&row_bytes);
        inserted_by_range.push(bytes);
    }
    let ranges = rectangular
        .iter()
        .map(|range| (range.start, range.end))
        .collect::<Vec<_>>();
    let batch = ranges
        .iter()
        .zip(&inserted_by_range)
        .rev()
        .map(|((start, end), bytes)| (*start, end - start, bytes.clone()))
        .collect::<Vec<_>>();
    session.document.replace_many(&batch)?;
    let bookmark_edits = ranges
        .iter()
        .zip(&inserted_by_range)
        .map(|((start, end), bytes)| (*start, *end, bytes.len()))
        .collect::<Vec<_>>();
    relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
    record_bookmark_edit(session, bookmark_edits);
    let adjusted = selections_after_replacements(&ranges, &inserted_by_range)?;
    // Typing keeps the column: the next keystroke lands on every row again.
    let typed = (row_texts.len() == 1).then(|| row_texts[0].chars().count() as u64);
    let kept = typed.and_then(|width| {
        let column = rectangle
            .anchor_column
            .min(rectangle.active_column)
            .checked_add(width)?;
        column_after_edit(rectangle, &adjusted, column)
    });
    session.selections = active_rectangle_caret(Some(rectangle), adjusted);
    session.rectangular_selection = kept;
    session.preferred_scalar_columns = vec![None];
    session.navigation_index.reset();
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryDirection {
    Undo,
    Redo,
}

fn rebase_saved_session(
    session: &mut EditorSession,
    session_root: &Path,
    destination: &Path,
) -> Result<(), EditorError> {
    let source = FileSource::open(destination).map_err(|error| EditorError::io(&error))?;
    if fingerprint(destination).map_err(|error| EditorError::io(&error))?
        != session.source_fingerprint
    {
        return Err(EditorError::SourceChangedWhileOpening);
    }
    let temporary = tempfile::Builder::new()
        .prefix("document-")
        .suffix(".add")
        .tempfile_in(session_root)
        .map_err(|error| EditorError::io(&error))?;
    let (file, additions_path) = temporary
        .keep()
        .map_err(|error| EditorError::io(&error.error))?;
    drop(file);
    let additions =
        DiskAppendStore::open(&additions_path).map_err(|error| EditorError::io(&error))?;
    session
        .document
        .rebase_saved_source(std::sync::Arc::new(source), Box::new(additions));
    let old_additions = session.additions_path.replace(additions_path);
    let old_original = session.original_path.take();
    session.document.mark_clean();
    session.changes.mark_saved();
    if let Some(id) = session.recovery_id.take() {
        let _ = fs::remove_file(recovery_manifest_path(session_root, &id));
    }
    if let Some(path) = old_additions {
        if path != destination {
            let _ = fs::remove_file(path);
        }
    }
    if let Some(path) = old_original {
        if path != destination {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn recovery_manifest_path(session_root: &Path, recovery_id: &str) -> PathBuf {
    session_root
        .join("recovery")
        .join(format!("{recovery_id}.json"))
}

fn valid_recovery_id(id: &str) -> bool {
    id.starts_with("document-")
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn remove_if_present(path: &Path) -> Result<(), EditorError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EditorError::io(&error)),
    }
}

fn summary(document_id: &DocumentId, session: &EditorSession) -> DocumentSummary {
    DocumentSummary {
        document_id: document_id.clone(),
        name: session.name.clone(),
        byte_len: session.document.table().len(),
        revision: session.document.table().revision(),
        encoding: session.encoding,
        dirty: !session.document.is_clean(),
        source_path: session
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        recovery_id: session.recovery_id.clone(),
        language_id: session.language_id.as_str().to_owned(),
        language_override: session.language_override,
        read_only: session.document.is_read_only(),
        capabilities: if binary_format(session).is_some() {
            DocumentCapabilities::default()
        } else {
            session.capabilities.clone()
        },
        indentation: session.indentation,
        binary_format: binary_format(session).map(str::to_owned),
    }
}

/// Replaces a document's whole text as one revision and one undo step,
/// with the caret at the start (Beautify, a language server's formatting).
fn replace_whole_text(session: &mut EditorSession, text: &str) -> Result<(), EditorError> {
    let bytes = encode_text(session.encoding, text)?;
    let old_len = session.document.table().len();
    session.document.replace(0, old_len, &bytes)?;
    session.selections = vec![Selection::caret(0)];
    session.rectangular_selection = None;
    let edits = vec![(0, old_len, bytes.len())];
    relocate_bookmarks(&mut session.bookmarks, &edits)?;
    record_bookmark_edit(session, edits);
    session.preferred_scalar_columns = vec![None];
    session.navigation_index.reset();
    Ok(())
}

/// Binary formats recognized by their first bytes: they are not text, so
/// they open read-only in the hexadecimal view.
fn binary_kind(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"bplist") {
        Some("binary property list")
    } else if head.starts_with(b"FasdUAS") {
        Some("compiled AppleScript")
    } else {
        None
    }
}

fn binary_format(session: &EditorSession) -> Option<&'static str> {
    let table = session.document.table();
    let head = table.read_vec(0, table.len().min(8)).ok()?;
    binary_kind(&head)
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

fn search_pattern(
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

/// Records one applied edit for bookmark relocation and line change markers.
fn record_bookmark_edit(session: &mut EditorSession, edits: Vec<(u64, u64, usize)>) {
    if let Some(snippet) = session.snippet.as_mut() {
        if !snippet.relocate(&edits) {
            session.snippet = None;
        }
    }
    let changes = session.changes.apply(&edits);
    push_bounded_history(&mut session.bookmark_undo, HistoryEntry { edits, changes });
    session.bookmark_redo.clear();
}

fn push_bounded_history(history: &mut VecDeque<HistoryEntry>, entry: HistoryEntry) {
    if history.len() == MAX_BOOKMARK_HISTORY {
        history.pop_front();
    }
    history.push_back(entry);
}

fn inverse_bookmark_edits(
    ascending_edits: &[(u64, u64, usize)],
) -> Result<Vec<(u64, u64, usize)>, EditorError> {
    let mut delta = 0_i128;
    ascending_edits
        .iter()
        .map(|(start, end, inserted_len)| {
            let adjusted_start = i128::from(*start)
                .checked_add(delta)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or(EditorError::OffsetOverflow)?;
            let inserted_len =
                u64::try_from(*inserted_len).map_err(|_| EditorError::OffsetOverflow)?;
            let adjusted_end = adjusted_start
                .checked_add(inserted_len)
                .ok_or(EditorError::OffsetOverflow)?;
            delta = delta
                .checked_add(i128::from(inserted_len) - i128::from(end - start))
                .ok_or(EditorError::OffsetOverflow)?;
            let removed_len =
                usize::try_from(end - start).map_err(|_| EditorError::OffsetOverflow)?;
            Ok((adjusted_start, adjusted_end, removed_len))
        })
        .collect()
}

fn deduplicate_selections(selections: Vec<Selection>) -> Vec<Selection> {
    let mut seen = BTreeSet::new();
    selections
        .into_iter()
        .filter(|selection| seen.insert((selection.anchor_byte, selection.active_byte)))
        .collect()
}

fn editable_ranges(selections: &[Selection]) -> Vec<(u64, u64)> {
    merge_ranges(
        selections
            .iter()
            .map(|selection| {
                (
                    selection.anchor_byte.min(selection.active_byte),
                    selection.anchor_byte.max(selection.active_byte),
                )
            })
            .collect(),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RectangularRange {
    start: u64,
    end: u64,
    missing_columns: u64,
}

fn rectangular_ranges(session: &EditorSession) -> Result<Vec<RectangularRange>, EditorError> {
    let Some(rectangle) = session.rectangular_selection else {
        return Ok(Vec::new());
    };
    let first_line = rectangle.anchor_line.min(rectangle.active_line);
    let last_line = rectangle.anchor_line.max(rectangle.active_line);
    let rows = last_line
        .checked_sub(first_line)
        .and_then(|value| value.checked_add(1))
        .ok_or(EditorError::OffsetOverflow)?;
    if rows > MAX_RECTANGULAR_ROWS as u64 {
        return Err(EditorError::RectangularSelectionTooTall {
            rows,
            maximum: MAX_RECTANGULAR_ROWS,
        });
    }
    let start_column = rectangle.anchor_column.min(rectangle.active_column);
    let end_column = rectangle.anchor_column.max(rectangle.active_column);
    let first_byte = if rectangle.anchor_line <= rectangle.active_line {
        rectangle.anchor_byte
    } else {
        rectangle.active_byte
    };
    let mut line_start = find_line_start(session.document.table(), first_byte)?;
    let mut ranges =
        Vec::with_capacity(usize::try_from(rows).map_err(|_| EditorError::OffsetOverflow)?);
    for row in 0..rows {
        let boundary = find_line_boundary(session.document.table(), line_start)?;
        let line_columns = scalar_count(
            session.document.table(),
            line_start,
            boundary.content_end,
            session.encoding,
        )?;
        let start = byte_at_scalar_column(
            session.document.table(),
            line_start,
            boundary.content_end,
            start_column,
            session.encoding,
        )?;
        let end = byte_at_scalar_column(
            session.document.table(),
            line_start,
            boundary.content_end,
            end_column,
            session.encoding,
        )?;
        ranges.push(RectangularRange {
            start,
            end,
            missing_columns: start_column.saturating_sub(line_columns),
        });
        if row + 1 < rows {
            line_start = boundary.next_start.ok_or(EditorError::OffsetOverflow)?;
        }
    }
    Ok(ranges)
}

fn merge_ranges(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(previous) = merged.last_mut() {
            if start <= previous.1 && (start != end || previous.0 != previous.1) {
                previous.1 = previous.1.max(end);
                continue;
            }
            if start == previous.0 && end == previous.1 {
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

fn carets_after_replacements(
    ascending_ranges: &[(u64, u64)],
    inserted_len: usize,
) -> Result<Vec<Selection>, EditorError> {
    let inserted_len = i128::try_from(inserted_len).map_err(|_| EditorError::OffsetOverflow)?;
    let mut delta = 0_i128;
    let mut carets = Vec::with_capacity(ascending_ranges.len());
    for (start, end) in ascending_ranges {
        let removed = i128::from(end - start);
        let start = i128::from(*start);
        let caret = start
            .checked_add(delta)
            .and_then(|value| value.checked_add(inserted_len))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(EditorError::OffsetOverflow)?;
        carets.push(Selection::caret(caret));
        delta = delta
            .checked_add(inserted_len - removed)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(carets)
}

/// The zero-width column left after a rectangular edit, one caret per row.
/// `carets` are the post-edit carets of the rows from top to bottom.
fn column_after_edit(
    rectangle: RectangularSelection,
    carets: &[Selection],
    column: u64,
) -> Option<RectangularSelection> {
    let top = carets.first()?.active_byte;
    let bottom = carets.last()?.active_byte;
    let downward = rectangle.active_line >= rectangle.anchor_line;
    Some(RectangularSelection {
        anchor_byte: if downward { top } else { bottom },
        active_byte: if downward { bottom } else { top },
        anchor_column: column,
        active_column: column,
        ..rectangle
    })
}

fn active_rectangle_caret(
    rectangle: Option<RectangularSelection>,
    candidates: Vec<Selection>,
) -> Vec<Selection> {
    let Some(rectangle) = rectangle else {
        return candidates;
    };
    let index = if rectangle.active_line >= rectangle.anchor_line {
        candidates.len().saturating_sub(1)
    } else {
        0
    };
    vec![Selection::caret(
        candidates
            .get(index)
            .map_or(rectangle.active_byte, |selection| selection.active_byte),
    )]
}

fn selections_after_replacements(
    ascending_ranges: &[(u64, u64)],
    inserted: &[Vec<u8>],
) -> Result<Vec<Selection>, EditorError> {
    let mut delta = 0_i128;
    ascending_ranges
        .iter()
        .zip(inserted)
        .map(|((start, end), bytes)| {
            let transformed_len =
                i128::try_from(bytes.len()).map_err(|_| EditorError::OffsetOverflow)?;
            let adjusted_start = i128::from(*start)
                .checked_add(delta)
                .ok_or(EditorError::OffsetOverflow)?;
            let adjusted_end = adjusted_start
                .checked_add(transformed_len)
                .ok_or(EditorError::OffsetOverflow)?;
            delta = delta
                .checked_add(transformed_len - i128::from(end - start))
                .ok_or(EditorError::OffsetOverflow)?;
            Ok(Selection {
                anchor_byte: u64::try_from(adjusted_start)
                    .map_err(|_| EditorError::OffsetOverflow)?,
                active_byte: u64::try_from(adjusted_end)
                    .map_err(|_| EditorError::OffsetOverflow)?,
            })
        })
        .collect()
}

fn relocate_bookmarks(
    bookmarks: &mut BTreeSet<u64>,
    ascending_edits: &[(u64, u64, usize)],
) -> Result<(), EditorError> {
    let mut relocated = BTreeSet::new();
    for bookmark in bookmarks.iter().copied() {
        let mut delta = 0_i128;
        let mut mapped = None;
        for (start, end, inserted_len) in ascending_edits {
            if bookmark < *start {
                break;
            }
            let inserted_len =
                i128::try_from(*inserted_len).map_err(|_| EditorError::OffsetOverflow)?;
            if bookmark < *end {
                mapped = Some(
                    i128::from(*start)
                        .checked_add(delta)
                        .ok_or(EditorError::OffsetOverflow)?,
                );
                break;
            }
            delta = delta
                .checked_add(inserted_len - i128::from(end - start))
                .ok_or(EditorError::OffsetOverflow)?;
        }
        let mapped = mapped.unwrap_or(
            i128::from(bookmark)
                .checked_add(delta)
                .ok_or(EditorError::OffsetOverflow)?,
        );
        relocated.insert(u64::try_from(mapped).map_err(|_| EditorError::OffsetOverflow)?);
    }
    *bookmarks = relocated;
    Ok(())
}

fn previous_scalar_start(bytes: &[u8]) -> Option<usize> {
    (0..bytes.len()).find(|start| {
        std::str::from_utf8(&bytes[*start..]).is_ok_and(|suffix| suffix.chars().count() == 1)
    })
}

fn next_scalar_len(bytes: &[u8]) -> Option<usize> {
    (1..=bytes.len()).find(|end| {
        std::str::from_utf8(&bytes[..*end]).is_ok_and(|prefix| prefix.chars().count() == 1)
    })
}

fn encode_text(encoding: TextEncoding, text: &str) -> Result<Vec<u8>, EditorError> {
    match encoding {
        TextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        TextEncoding::Windows1252 => text
            .chars()
            .map(|character| {
                windows_1252_byte(character).ok_or(EditorError::UnrepresentableText {
                    character,
                    encoding,
                })
            })
            .collect(),
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

fn decode_text(encoding: TextEncoding, bytes: &[u8]) -> (String, Vec<u8>) {
    match encoding {
        TextEncoding::Utf8 => decode_utf8(bytes),
        TextEncoding::Windows1252 => (
            bytes.iter().copied().map(windows_1252_character).collect(),
            vec![1; bytes.len()],
        ),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum WordKind {
    Word,
    Space,
    Punctuation,
    LineBreak,
}

fn word_kind(grapheme: &str) -> WordKind {
    let first = grapheme.chars().next().unwrap_or(' ');
    if grapheme_has_line_break(grapheme) {
        WordKind::LineBreak
    } else if first.is_alphanumeric() || first == '_' {
        WordKind::Word
    } else if first.is_whitespace() {
        WordKind::Space
    } else {
        WordKind::Punctuation
    }
}

fn grapheme_has_line_break(grapheme: &str) -> bool {
    grapheme.contains('\r') || grapheme.contains('\n')
}

fn word_selection(
    table: &PieceTable,
    encoding: TextEncoding,
    byte_offset: u64,
) -> Result<Selection, EditorError> {
    let len = table.len();
    if len == 0 {
        return Ok(Selection::caret(0));
    }
    let half = MAX_WORD_SCAN_BYTES / 2;
    let start = byte_offset.saturating_sub(half);
    let end = byte_offset.saturating_add(half).min(len);
    let bytes = table.read_vec(start, end - start)?;
    let (text, widths) = decode_text(encoding, &bytes);
    let mut source_boundaries = Vec::with_capacity(widths.len() + 1);
    let mut source_offset = 0_u64;
    for ((utf8_index, _), width) in text.char_indices().zip(widths) {
        source_boundaries.push((utf8_index, source_offset));
        source_offset += u64::from(width);
    }
    source_boundaries.push((text.len(), source_offset));
    let mut runs = Vec::new();
    for (utf8_index, grapheme) in text.grapheme_indices(true) {
        let next_index = utf8_index + grapheme.len();
        let start_index = source_boundaries.partition_point(|(position, _)| *position < utf8_index);
        let end_index = source_boundaries.partition_point(|(position, _)| *position < next_index);
        let start_byte = start + source_boundaries[start_index].1;
        let end_byte = start + source_boundaries[end_index].1;
        runs.push((start_byte, end_byte, word_kind(grapheme)));
    }
    let mut index = runs.partition_point(|(_, end_byte, _)| *end_byte <= byte_offset);
    if index == runs.len() {
        index = index.saturating_sub(1);
    }
    if runs[index].2 == WordKind::LineBreak {
        if index > 0 && runs[index - 1].2 != WordKind::LineBreak {
            index -= 1;
        } else if index + 1 < runs.len() {
            index += 1;
        } else {
            return Ok(Selection::caret(byte_offset));
        }
    }
    let kind = runs[index].2;
    let mut first = index;
    let mut last = index;
    while first > 0 && runs[first - 1].2 == kind {
        first -= 1;
    }
    while last + 1 < runs.len() && runs[last + 1].2 == kind {
        last += 1;
    }
    if (first == 0 && start > 0) || (last + 1 == runs.len() && end < len) {
        return Err(EditorError::WordSelectionTooLong);
    }
    Ok(Selection {
        anchor_byte: runs[first].0,
        active_byte: runs[last].1,
    })
}

fn decode_utf8(bytes: &[u8]) -> (String, Vec<u8>) {
    let mut text = String::new();
    let mut widths = Vec::new();
    let mut cursor = 0_usize;
    while cursor < bytes.len() {
        match std::str::from_utf8(&bytes[cursor..]) {
            Ok(valid) => {
                for character in valid.chars() {
                    text.push(character);
                    widths.push(u8::try_from(character.len_utf8()).expect("UTF-8 width fits u8"));
                }
                break;
            }
            Err(error) => {
                let valid_end = cursor + error.valid_up_to();
                let valid = std::str::from_utf8(&bytes[cursor..valid_end])
                    .expect("prefix before UTF-8 error is valid");
                for character in valid.chars() {
                    text.push(character);
                    widths.push(u8::try_from(character.len_utf8()).expect("UTF-8 width fits u8"));
                }
                let invalid_len = error.error_len().unwrap_or(bytes.len() - valid_end);
                text.push('\u{FFFD}');
                widths
                    .push(u8::try_from(invalid_len).expect("invalid UTF-8 run is at most 3 bytes"));
                cursor = valid_end + invalid_len;
            }
        }
    }
    (text, widths)
}

fn windows_1252_character(byte: u8) -> char {
    match byte {
        0x80 => '\u{20AC}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        _ => char::from(byte),
    }
}

fn windows_1252_byte(character: char) -> Option<u8> {
    match character {
        '\u{20AC}' => Some(0x80),
        '\u{201A}' => Some(0x82),
        '\u{0192}' => Some(0x83),
        '\u{201E}' => Some(0x84),
        '\u{2026}' => Some(0x85),
        '\u{2020}' => Some(0x86),
        '\u{2021}' => Some(0x87),
        '\u{02C6}' => Some(0x88),
        '\u{2030}' => Some(0x89),
        '\u{0160}' => Some(0x8A),
        '\u{2039}' => Some(0x8B),
        '\u{0152}' => Some(0x8C),
        '\u{017D}' => Some(0x8E),
        '\u{2018}' => Some(0x91),
        '\u{2019}' => Some(0x92),
        '\u{201C}' => Some(0x93),
        '\u{201D}' => Some(0x94),
        '\u{2022}' => Some(0x95),
        '\u{2013}' => Some(0x96),
        '\u{2014}' => Some(0x97),
        '\u{02DC}' => Some(0x98),
        '\u{2122}' => Some(0x99),
        '\u{0161}' => Some(0x9A),
        '\u{203A}' => Some(0x9B),
        '\u{0153}' => Some(0x9C),
        '\u{017E}' => Some(0x9E),
        '\u{0178}' => Some(0x9F),
        character if u32::from(character) <= 0xFF => Some(character as u8),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalDirection {
    Left,
    Right,
    /// Home: the first non-blank character, or the line start from there.
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VerticalDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LineBoundary {
    content_end: u64,
    next_start: Option<u64>,
}

fn find_line_start(table: &PieceTable, offset: u64) -> Result<u64, EditorError> {
    let mut cursor = offset.min(table.len());
    while cursor > 0 {
        let start = cursor.saturating_sub(NAVIGATION_CHUNK_BYTES);
        let bytes = table.read_vec(start, cursor - start)?;
        for relative in (0..bytes.len()).rev() {
            let absolute = start
                .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
                .ok_or(EditorError::OffsetOverflow)?;
            match bytes[relative] {
                b'\n' => return absolute.checked_add(1).ok_or(EditorError::OffsetOverflow),
                b'\r' => {
                    let next = absolute.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
                    if next == offset && next < table.len() && read_byte(table, next)? == b'\n' {
                        continue;
                    }
                    return Ok(next);
                }
                _ => {}
            }
        }
        cursor = start;
    }
    Ok(0)
}

fn find_line_boundary(table: &PieceTable, start: u64) -> Result<LineBoundary, EditorError> {
    let mut cursor = start;
    while cursor < table.len() {
        let remaining = table.len() - cursor;
        let core_len = remaining.min(NAVIGATION_CHUNK_BYTES);
        let read_len = remaining.min(core_len.saturating_add(1));
        let bytes = table.read_vec(cursor, read_len)?;
        let core_size = usize::try_from(core_len).map_err(|_| EditorError::OffsetOverflow)?;
        for relative in 0..core_size {
            let ending_len = match bytes[relative] {
                b'\r' if bytes.get(relative + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => continue,
            };
            let content_end = cursor
                .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
                .ok_or(EditorError::OffsetOverflow)?;
            let next_start = content_end
                .checked_add(ending_len)
                .ok_or(EditorError::OffsetOverflow)?;
            return Ok(LineBoundary {
                content_end,
                next_start: Some(next_start),
            });
        }
        cursor = cursor
            .checked_add(core_len)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(LineBoundary {
        content_end: table.len(),
        next_start: None,
    })
}

fn previous_line_range(
    table: &PieceTable,
    current_start: u64,
) -> Result<Option<(u64, u64)>, EditorError> {
    if current_start == 0 {
        return Ok(None);
    }
    let last_ending_byte = read_byte(table, current_start - 1)?;
    let ending_len = if last_ending_byte == b'\n'
        && current_start >= 2
        && read_byte(table, current_start - 2)? == b'\r'
    {
        2
    } else {
        1
    };
    let content_end = current_start - ending_len;
    Ok(Some((find_line_start(table, content_end)?, content_end)))
}

fn scalar_count(
    table: &PieceTable,
    start: u64,
    end: u64,
    encoding: TextEncoding,
) -> Result<u64, EditorError> {
    let mut cursor = start;
    let mut count = 0_u64;
    while cursor < end {
        let (bytes, core_size) = scalar_chunk(table, cursor, end)?;
        let mut relative = 0_usize;
        while relative < core_size {
            relative = relative
                .checked_add(navigable_scalar_width(encoding, &bytes[relative..]))
                .ok_or(EditorError::OffsetOverflow)?;
            count = count.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
        }
        cursor = cursor
            .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(count)
}

fn byte_at_scalar_column(
    table: &PieceTable,
    start: u64,
    end: u64,
    column: u64,
    encoding: TextEncoding,
) -> Result<u64, EditorError> {
    let mut cursor = start;
    let mut remaining_column = column;
    while cursor < end && remaining_column > 0 {
        let (bytes, core_size) = scalar_chunk(table, cursor, end)?;
        let mut relative = 0_usize;
        while relative < core_size && remaining_column > 0 {
            relative = relative
                .checked_add(navigable_scalar_width(encoding, &bytes[relative..]))
                .ok_or(EditorError::OffsetOverflow)?;
            remaining_column -= 1;
        }
        cursor = cursor
            .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(cursor.min(end))
}

fn scalar_chunk(
    table: &PieceTable,
    cursor: u64,
    end: u64,
) -> Result<(Vec<u8>, usize), EditorError> {
    let remaining = end - cursor;
    let core_len = remaining.min(NAVIGATION_CHUNK_BYTES);
    let read_len = remaining.min(core_len.saturating_add(3));
    let bytes = table.read_vec(cursor, read_len)?;
    let core_size = usize::try_from(core_len).map_err(|_| EditorError::OffsetOverflow)?;
    Ok((bytes, core_size))
}

fn navigable_scalar_width(encoding: TextEncoding, bytes: &[u8]) -> usize {
    match encoding {
        TextEncoding::Utf8 => next_scalar_len(&bytes[..bytes.len().min(4)]).unwrap_or(1),
        TextEncoding::Windows1252 => 1,
    }
}

fn read_byte(table: &PieceTable, offset: u64) -> Result<u8, EditorError> {
    Ok(table.read_vec(offset, 1)?[0])
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

fn line_number_at_offset(
    table: &PieceTable,
    index: &SparseNavigationIndex,
    target_offset: u64,
) -> Result<u64, EditorError> {
    let (start_offset, start_line) = index.checkpoint_for_offset(target_offset);
    let additional = count_line_endings(table, start_offset, target_offset)?;
    start_line
        .checked_add(additional)
        .ok_or(EditorError::OffsetOverflow)
}

fn count_line_endings(
    table: &PieceTable,
    start_offset: u64,
    end_offset: u64,
) -> Result<u64, EditorError> {
    let mut cursor = start_offset;
    let mut count = 0_u64;
    let mut pending_cr = false;
    while cursor < end_offset {
        let read_len = (end_offset - cursor).min(NAVIGATION_CHUNK_BYTES);
        let bytes = table.read_vec(cursor, read_len)?;
        for byte in bytes {
            if pending_cr {
                pending_cr = false;
                count = count.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => pending_cr = true,
                b'\n' => count = count.checked_add(1).ok_or(EditorError::OffsetOverflow)?,
                _ => {}
            }
        }
        cursor = cursor
            .checked_add(read_len)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    if pending_cr {
        count = count.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
    }
    Ok(count)
}

fn line_offset_for_number(
    table: &PieceTable,
    index: &SparseNavigationIndex,
    target_line: u64,
) -> Result<Option<u64>, EditorError> {
    let (mut cursor, mut line) = index.checkpoint_for_line(target_line);
    if line == target_line {
        return Ok(Some(cursor));
    }
    let mut pending_cr = false;
    while cursor < table.len() {
        let read_len = (table.len() - cursor).min(NAVIGATION_CHUNK_BYTES);
        let bytes = table.read_vec(cursor, read_len)?;
        for (relative, byte) in bytes.iter().copied().enumerate() {
            let absolute = cursor
                .checked_add(u64::try_from(relative).map_err(|_| EditorError::OffsetOverflow)?)
                .ok_or(EditorError::OffsetOverflow)?;
            if pending_cr {
                pending_cr = false;
                line = line.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
                let next_start = if byte == b'\n' {
                    absolute.checked_add(1).ok_or(EditorError::OffsetOverflow)?
                } else {
                    absolute
                };
                if line == target_line {
                    return Ok(Some(next_start));
                }
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => pending_cr = true,
                b'\n' => {
                    line = line.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
                    if line == target_line {
                        return absolute
                            .checked_add(1)
                            .map(Some)
                            .ok_or(EditorError::OffsetOverflow);
                    }
                }
                _ => {}
            }
        }
        cursor = cursor
            .checked_add(read_len)
            .ok_or(EditorError::OffsetOverflow)?;
    }
    if pending_cr {
        line = line.checked_add(1).ok_or(EditorError::OffsetOverflow)?;
        if line == target_line {
            return Ok(Some(table.len()));
        }
    }
    Ok(None)
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

fn viewport_lines(
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

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum EditorError {
    #[error("unknown document {0:?}")]
    UnknownDocument(DocumentId),
    #[error("viewport byte limit must be in 2..={MAX_VIEWPORT_BYTES}, got {0}")]
    ViewportLimit(u64),
    #[error("hex viewport byte limit must be in {minimum}..={maximum}, got {requested}")]
    HexViewportLimit {
        requested: u64,
        minimum: u64,
        maximum: u64,
    },
    #[error("line {0} does not exist")]
    LineOutOfRange(u64),
    #[error("edit payload of {0} bytes exceeds the command limit")]
    EditTooLarge(usize),
    #[error("offset {offset} exceeds document length {document_len}")]
    OffsetOutOfRange { offset: u64, document_len: u64 },
    #[error("range end {end} precedes start {start}")]
    InvalidRange { start: u64, end: u64 },
    #[error("expected document revision {expected}, found {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("at least one selection is required")]
    EmptySelections,
    #[error("selection count {count} exceeds the maximum of {maximum}")]
    TooManySelections { count: usize, maximum: usize },
    #[error("word selection exceeds the bounded scan limit")]
    WordSelectionTooLong,
    #[error("{0}")]
    Tool(String),
    #[error("rectangular selection height {rows} exceeds the maximum of {maximum} rows")]
    RectangularSelectionTooTall { rows: u64, maximum: usize },
    #[error("rectangular paste supplied {supplied} rows for a {selected}-row selection")]
    RectangularPasteRowMismatch { supplied: usize, selected: usize },
    #[error("bookmark count exceeds the maximum of {0}")]
    TooManyBookmarks(usize),
    #[error("search match count exceeds the maximum of {0}")]
    TooManySearchMatches(usize),
    #[error("find-in-files result limit must be in 1..={MAX_WORKSPACE_SEARCH_RESULTS}, got {0}")]
    WorkspaceSearchLimit(usize),
    #[error("search root is not a regular file or directory: {0}")]
    InvalidSearchRoot(String),
    #[error("regular-expression search requires UTF-8 document interpretation")]
    RegexRequiresUtf8,
    #[error("text transforms require valid UTF-8 selections")]
    InvalidUtf8Selection,
    #[error("the selection is not valid Base64")]
    InvalidBase64,
    #[error("the decoded Base64 is binary data, not text; decode it in the hex editor")]
    Base64NotText,
    #[error("this language has no comment syntax")]
    NoCommentSyntax,
    #[error("character {character:?} cannot be represented in {encoding:?}")]
    UnrepresentableText {
        character: char,
        encoding: TextEncoding,
    },
    #[error("clipboard selection of {selected} bytes exceeds the {maximum}-byte limit")]
    ClipboardTooLarge { selected: u64, maximum: u64 },
    #[error("Markdown preview is limited to {maximum} bytes; this document has {size} bytes")]
    MarkdownPreviewTooLarge { size: u64, maximum: u64 },
    #[error("Markdown preview image is unavailable: {0}")]
    InvalidPreviewImage(String),
    #[error("hex selection of {selected} bytes exceeds the {maximum}-byte limit")]
    HexClipboardTooLarge { selected: u64, maximum: u64 },
    #[error("merge selection of {selected} bytes exceeds the {maximum}-byte limit")]
    MergeTooLarge { selected: u64, maximum: u64 },
    #[error("document offset overflow")]
    OffsetOverflow,
    #[error("document state is unavailable")]
    StateUnavailable,
    #[error("the source changed while it was being opened")]
    SourceChangedWhileOpening,
    #[error("this document needs a destination before it can be saved")]
    SavePathRequired,
    #[error("invalid save destination: {0}")]
    InvalidSavePath(String),
    #[error("document recovery failed: {0}")]
    Recovery(String),
    #[error("save failed: {0}")]
    Save(String),
    #[error("document operation failed: {0}")]
    Document(String),
    #[error("This document is read-only. Switch to Edit mode to change it.")]
    ReadOnly,
    #[error(
        "Converting line endings is limited to {maximum} bytes; this document has {size} bytes"
    )]
    LineEndingConversionTooLarge { size: u64, maximum: u64 },
    #[error("Converting line endings is limited to {0} changed lines")]
    TooManyLineEndingChanges(usize),
    #[error("Could not rename: {0}")]
    Rename(String),
    #[error("Could not export to PDF: {0}")]
    Export(String),
    #[error("Could not print: {0}")]
    Print(String),
    #[error("The PDF would have at least {estimated_pages} pages")]
    ExportNeedsConfirmation { estimated_pages: u64 },
    #[error("Export to PDF was cancelled")]
    ExportCancelled,
    #[error("The lines have no tab, comma, semicolon, or | between columns")]
    NoColumnDelimiter,
    #[error("Select a column block first (Alt Shift drag, or Column Editing)")]
    NeedsColumnSelection,
    #[error("The numbers are out of range")]
    NumberOutOfRange,
    #[error("selection of {selected} bytes exceeds the {maximum}-byte limit for this command")]
    SelectionTooLarge { selected: u64, maximum: u64 },
    #[error("search operation failed: {0}")]
    Search(String),
    #[error("syntax operation failed: {0}")]
    Syntax(String),
    #[error("hexadecimal operation failed: {0}")]
    Hex(String),
    #[error("comparison operation failed: {0}")]
    Diff(String),
    #[error("file operation failed: {0}")]
    Io(String),
}

fn validate_revision(session: &EditorSession, expected: u64) -> Result<(), EditorError> {
    let actual = session.document.table().revision();
    if actual == expected {
        Ok(())
    } else {
        Err(EditorError::StaleRevision { expected, actual })
    }
}

fn overwrite_end(
    session: &EditorSession,
    start: u64,
    scalar_count: usize,
) -> Result<u64, EditorError> {
    if scalar_count == 0 || start == session.document.table().len() {
        return Ok(start);
    }
    let maximum_width = match session.encoding {
        TextEncoding::Utf8 => 4,
        TextEncoding::Windows1252 => 1,
    };
    let maximum_probe = u64::try_from(scalar_count)
        .map_err(|_| EditorError::OffsetOverflow)?
        .checked_mul(maximum_width)
        .ok_or(EditorError::OffsetOverflow)?;
    let available = session.document.table().len() - start;
    let bytes = session
        .document
        .table()
        .read_vec(start, available.min(maximum_probe))?;
    let mut consumed = 0_usize;
    for _ in 0..scalar_count {
        let remaining = &bytes[consumed..];
        if remaining.is_empty() || matches!(remaining[0], b'\r' | b'\n') {
            break;
        }
        consumed += navigable_scalar_width(session.encoding, remaining);
    }
    start
        .checked_add(u64::try_from(consumed).map_err(|_| EditorError::OffsetOverflow)?)
        .ok_or(EditorError::OffsetOverflow)
}

impl EditorError {
    fn io(error: &std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<nulis_document::DocumentError> for EditorError {
    fn from(error: nulis_document::DocumentError) -> Self {
        if matches!(error, nulis_document::DocumentError::ReadOnly) {
            return Self::ReadOnly;
        }
        Self::Document(error.to_string())
    }
}

impl From<nulis_search::SearchError> for EditorError {
    fn from(error: nulis_search::SearchError) -> Self {
        Self::Search(error.to_string())
    }
}

impl From<nulis_syntax::SyntaxError> for EditorError {
    fn from(error: nulis_syntax::SyntaxError) -> Self {
        Self::Syntax(error.to_string())
    }
}

impl From<nulis_hex::HexError> for EditorError {
    fn from(error: nulis_hex::HexError) -> Self {
        Self::Hex(error.to_string())
    }
}

pub(super) mod decimal_u64 {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)] // Required by serde's `with` contract.
    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

mod decimal_u64_vec {
    use serde::{Serialize, Serializer};

    pub fn serialize<S>(values: &[u64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
}

#[cfg(test)]
mod tests;
