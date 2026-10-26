//! The arguments each command accepts, as the interface sends them. Unknown
//! fields are refused, and omitted ones take the defaults below.

use super::{
    editor, BookmarkDirection, ChangeDirection, DocumentId, FindDirection, HorizontalDirection,
    Justify, LineEnding, LineFormatOptions, LineOperation, NumberSequence, ReplaceTarget,
    SearchOptions, Selection, TextEncoding, TextTransform, VerticalDirection,
};
use nulis_automation::{SandboxPolicy, ScriptDefinition};
use nulis_diff::CompareMode;
use nulis_plugin_host::{PluginCapability, PluginGrant};
use nulis_terminal::{TerminalId, TerminalSize};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NewDocumentArguments {
    pub(super) name: String,
    #[serde(default)]
    pub(super) initial_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StartRecordingArguments {
    pub(super) id: String,
    pub(super) name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MacroIdArguments {
    pub(super) id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ScriptIdArguments {
    pub(super) id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SaveScriptArguments {
    pub(super) id: String,
    pub(super) definition: ScriptDefinition,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ApproveScriptArguments {
    pub(super) id: String,
    pub(super) policy: SandboxPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PluginIdArguments {
    pub(super) id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PluginEnabledArguments {
    pub(super) id: String,
    pub(super) enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PluginCapabilityArguments {
    pub(super) id: String,
    pub(super) capability: PluginCapability,
    pub(super) enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PluginInstallArguments {
    pub(super) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ApprovePluginArguments {
    pub(super) id: String,
    pub(super) grant: PluginGrant,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ScriptEditDocumentArguments {
    pub(super) expected_revision: Value,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
    pub(super) text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ScriptReadWorkspaceArguments {
    #[serde(default)]
    pub(super) path: String,
    #[serde(default = "default_workspace_entry_limit")]
    pub(super) max_entries: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ScriptWriteWorkspaceArguments {
    pub(super) path: String,
    pub(super) contents: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OpenFileArguments {
    pub(super) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SaveAsArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RecoveryIdArguments {
    pub(super) recovery_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CloseDocumentArguments {
    pub(super) discard: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CompareDocumentsArguments {
    pub(super) left_document_id: DocumentId,
    pub(super) right_document_id: DocumentId,
    pub(super) mode: CompareMode,
    #[serde(default, with = "editor::decimal_u64")]
    pub(super) left_start_byte: u64,
    #[serde(default, with = "editor::decimal_u64")]
    pub(super) right_start_byte: u64,
    #[serde(default = "default_start_line", with = "editor::decimal_u64")]
    pub(super) left_start_line: u64,
    #[serde(default = "default_start_line", with = "editor::decimal_u64")]
    pub(super) right_start_line: u64,
    #[serde(default = "default_compare_bytes")]
    pub(super) max_bytes: usize,
    #[serde(default = "default_compare_changes")]
    pub(super) max_changes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ApplyCompareChangeArguments {
    pub(super) source_document_id: DocumentId,
    pub(super) target_document_id: DocumentId,
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_target_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) source_start: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) source_end: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) target_start: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) target_end: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CompareFoldersArguments {
    pub(super) left_root: String,
    pub(super) right_root: String,
    #[serde(default = "default_folder_compare_results")]
    pub(super) max_results: usize,
}

const fn default_start_line() -> u64 {
    1
}

const fn default_compare_bytes() -> usize {
    256 * 1024
}

const fn default_compare_changes() -> usize {
    500
}

const fn default_folder_compare_results() -> usize {
    1_000
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplaceArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
    pub(super) text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WordAtArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) byte_offset: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MoveSelectedTextArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) destination_byte: u64,
    #[serde(default)]
    pub(super) copy: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SelectionArguments {
    pub(super) selections: Vec<Selection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RectangularSelectionArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) anchor_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) active_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) anchor_column: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) active_column: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RevisionArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FormatArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    /// The user agreed that YAML Beautify may drop comments and expand anchors.
    #[serde(default)]
    pub(super) accept_loss: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InsertTextArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) text: String,
    #[serde(default)]
    pub(super) overwrite: bool,
    /// Typed from the keyboard: a closing character may move its line left.
    #[serde(default)]
    pub(super) typed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CompletionArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) request_id: u64,
    /// Asked for with the shortcut: offer suggestions even for no typed word.
    #[serde(default)]
    pub(super) explicit: bool,
    /// Without `explicit`: the shortest word that gets suggestions.
    #[serde(default = "default_min_prefix")]
    pub(super) min_prefix: usize,
}

const fn default_min_prefix() -> usize {
    2
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AcceptCompletionArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) replace_start: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) replace_end: u64,
    pub(super) text: String,
    #[serde(default)]
    pub(super) snippet: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MoveArguments {
    pub(super) direction: HorizontalDirection,
    #[serde(default)]
    pub(super) extend: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct VerticalMoveArguments {
    pub(super) direction: VerticalDirection,
    #[serde(default)]
    pub(super) extend: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BookmarkMoveArguments {
    pub(super) direction: BookmarkDirection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LineOperationArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) operation: LineOperation,
    /// Absent: the document's own indentation unit and tab width.
    #[serde(default)]
    pub(super) options: Option<LineFormatOptions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TransformArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) transform: TextTransform,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EncodingArguments {
    pub(super) encoding: TextEncoding,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LanguageArguments {
    pub(super) language_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct HexNibbleArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) high_nibble: bool,
    pub(super) nibble: u8,
    #[serde(default = "default_true")]
    pub(super) overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct HexReplaceArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) hex: String,
    #[serde(default = "default_true")]
    pub(super) overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FindArguments {
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    pub(super) direction: FindDirection,
    #[serde(default = "default_true")]
    pub(super) wrap: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplaceAllArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) query: String,
    pub(super) replacement: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    /// Limits the search to a byte range, for "selected text".
    #[serde(default)]
    pub(super) within: Option<ByteRangeArguments>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplaceMatchArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) query: String,
    pub(super) replacement: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ByteRangeArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
}

impl ByteRangeArguments {
    pub(super) fn pair(self) -> (u64, u64) {
        (self.start_byte, self.end_byte)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FindAllArguments {
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(default)]
    pub(super) within: Option<ByteRangeArguments>,
    #[serde(default = "default_find_all_limit")]
    pub(super) max_matches: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ValidatePatternArguments {
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MatchesInRangeArguments {
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(with = "editor::decimal_u64")]
    pub(super) start_byte: u64,
    #[serde(with = "editor::decimal_u64")]
    pub(super) end_byte: u64,
}

fn default_find_all_limit() -> usize {
    editor::MAX_FIND_ALL_MATCHES
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PreviewReplaceInFilesArguments {
    pub(super) root: String,
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(default)]
    pub(super) scope: editor::FileSearchScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplaceInFilesArguments {
    pub(super) files: Vec<ReplaceTarget>,
    pub(super) query: String,
    pub(super) replacement: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ChangeNearArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) from_byte: u64,
    pub(super) direction: ChangeDirection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ConvertLineEndingsArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) target: LineEnding,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NumberSequenceArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) sequence: NumberSequence,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct JustifyArguments {
    #[serde(with = "editor::decimal_u64")]
    pub(super) expected_revision: u64,
    pub(super) justify: Justify,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RenameFileArguments {
    pub(super) name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReloadArguments {
    /// Reload even though the document has unsaved edits.
    #[serde(default)]
    pub(super) discard: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AddBookmarksArguments {
    /// Decimal byte offsets.
    pub(super) offsets: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FindInFilesArguments {
    pub(super) root: String,
    pub(super) query: String,
    #[serde(default)]
    pub(super) options: SearchOptions,
    #[serde(default)]
    pub(super) scope: editor::FileSearchScope,
    #[serde(default = "default_workspace_result_limit")]
    pub(super) max_results: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OpenWorkspaceArguments {
    pub(super) root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ListWorkspaceArguments {
    #[serde(default)]
    pub(super) path: String,
    #[serde(default = "default_workspace_entry_limit")]
    pub(super) max_entries: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ListPlaceArguments {
    pub(super) path: String,
    #[serde(default = "default_workspace_entry_limit")]
    pub(super) max_entries: usize,
    #[serde(default)]
    pub(super) show_hidden: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OpenWorkspaceFileArguments {
    pub(super) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TerminalWriteArguments {
    pub(super) terminal_id: TerminalId,
    pub(super) bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TerminalReadArguments {
    pub(super) terminal_id: TerminalId,
    #[serde(with = "editor::decimal_u64")]
    pub(super) cursor: u64,
    pub(super) maximum: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TerminalResizeArguments {
    pub(super) terminal_id: TerminalId,
    pub(super) size: TerminalSize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TerminalIdArguments {
    pub(super) terminal_id: TerminalId,
}

const fn default_true() -> bool {
    true
}

const fn default_workspace_result_limit() -> usize {
    500
}

const fn default_workspace_entry_limit() -> usize {
    500
}
