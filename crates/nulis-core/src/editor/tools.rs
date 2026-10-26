//! Validate and Beautify: running a language's validator or formatter on the
//! whole document, and reporting where a problem is.

use super::{
    decode_text, format_tools, replace_whole_text, summary, validate_revision, DocumentId,
    DocumentManager, DocumentSummary, EditorError, EditorSession, TextEncoding, ValidationReport,
    MAX_TOOL_BYTES,
};
use nulis_language::{
    CapabilityKind, DocumentText, FormatAssessment, FormatMode, FormatOutcome, Provider,
    ProviderError,
};

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
