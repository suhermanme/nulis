//! What language servers need from documents: a copy of the text taken
//! under the lock (so no server call ever holds it), a revision-checked
//! whole-text replacement for formatting, and which languages have an
//! approved server.

use super::{
    decode_text, replace_whole_text, summary, validate_revision, DocumentId, DocumentManager,
    DocumentSummary, EditorError, TextEncoding, MAX_TOOL_BYTES,
};
use nulis_language::CapabilityKind;
use std::{collections::BTreeMap, path::PathBuf};

/// A document as a language server sees it.
#[derive(Clone, Debug)]
pub struct ServerDocument {
    pub path: PathBuf,
    pub language: String,
    pub revision: u64,
    /// The decoded text (invalid bytes become U+FFFD).
    pub text: String,
    pub encoding: TextEncoding,
    /// The primary caret, as a byte offset into the document.
    pub caret: u64,
}

impl ServerDocument {
    /// The document byte offset of a byte offset into `text`.
    #[must_use]
    pub fn document_offset(&self, text_offset: usize) -> u64 {
        let text_offset = text_offset.min(self.text.len());
        match self.encoding {
            TextEncoding::Utf8 => text_offset as u64,
            // Every Windows-1252 character is one stored byte.
            TextEncoding::Windows1252 => self.text[..text_offset].chars().count() as u64,
        }
    }

    /// The byte offset into `text` of a document byte offset.
    #[must_use]
    pub fn text_offset(&self, document_offset: u64) -> usize {
        match self.encoding {
            TextEncoding::Utf8 => usize::try_from(document_offset)
                .unwrap_or(usize::MAX)
                .min(self.text.len()),
            TextEncoding::Windows1252 => self
                .text
                .char_indices()
                .nth(usize::try_from(document_offset).unwrap_or(usize::MAX))
                .map_or(self.text.len(), |(index, _)| index),
        }
    }
}

impl DocumentManager {
    /// The document for a language server: only saved files up to the tool
    /// size limit have one.
    ///
    /// # Errors
    /// Returns an error for an unknown document or a storage failure.
    pub fn server_document(
        &self,
        document_id: &DocumentId,
    ) -> Result<Option<ServerDocument>, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let Some(path) = session.source_path.clone() else {
            return Ok(None);
        };
        let table = session.document.table();
        if table.len() > MAX_TOOL_BYTES {
            return Ok(None);
        }
        let bytes = table.read_vec(0, table.len())?;
        Ok(Some(ServerDocument {
            path,
            language: session.language_id.as_str().to_owned(),
            revision: table.revision(),
            text: decode_text(session.encoding, &bytes).0,
            encoding: session.encoding,
            caret: session
                .selections
                .first()
                .map_or(0, |selection| selection.active_byte),
        }))
    }

    /// Replaces the whole text if the document is still at `expected_revision`,
    /// as one undo step; the same text changes nothing.
    ///
    /// # Errors
    /// Returns an error for a stale revision, a read-only document, or a
    /// storage failure.
    pub fn replace_document_text(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        text: &str,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        let table = session.document.table();
        let current = decode_text(session.encoding, &table.read_vec(0, table.len())?).0;
        if current != text {
            replace_whole_text(session, text)?;
        }
        Ok(summary(document_id, session))
    }

    /// Whether the document's language has a built-in Beautify (or, with
    /// `compact`, Minify), which goes before a language server's formatting.
    #[must_use]
    pub fn has_builtin_formatter(&self, document_id: &DocumentId, compact: bool) -> bool {
        let Ok(sessions) = self.sessions.lock() else {
            return false;
        };
        let Some(session) = sessions.get(document_id) else {
            return false;
        };
        let kind = if compact {
            CapabilityKind::Compact
        } else {
            CapabilityKind::Format
        };
        self.capability_registry()
            .resolve(&session.language_id, kind)
            .is_some()
    }

    /// Records which languages have an approved server (language ID to
    /// server ID) and refreshes open documents' capabilities.
    pub fn set_language_servers(&self, servers: BTreeMap<String, String>) {
        *self
            .language_servers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = servers;
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                self.refresh_language_state(session);
            }
        }
    }

    /// The configured language servers and the ones running.
    #[must_use]
    pub fn servers(&self) -> &crate::language_servers::LanguageServers {
        &self.servers
    }

    /// The IDs of every open document.
    #[must_use]
    pub fn open_document_ids(&self) -> Vec<DocumentId> {
        self.sessions
            .lock()
            .map(|sessions| sessions.keys().cloned().collect())
            .unwrap_or_default()
    }
}
