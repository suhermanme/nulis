//! The editor's message types: what the interface sends in a request and
//! receives back. All of them are re-exported from `editor`.

use super::{
    decimal_u64, decimal_u64_vec, IndentationSummary, LineChange, LineChangeStatus, LineEnding,
    LineEndingStyle,
};
use nulis_language::CapabilityKind;
use nulis_syntax::{SymbolKind, SyntaxToken};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub(super) fn tool_label(&self) -> Option<&str> {
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
    /// The start of the matching line (see [`MAX_LINE_PREVIEW_BYTES`](super::MAX_LINE_PREVIEW_BYTES)).
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
    pub(super) fn includes(&self, path: &Path) -> bool {
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
pub(super) fn wildcard_match(pattern: &str, name: &str) -> bool {
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
