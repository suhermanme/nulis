use crate::{DocumentError, PieceTable};
use nulis_large_file::{AppendStore, ReadAt};
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct EditPart {
    offset: u64,
    removed: Vec<u8>,
    inserted: Vec<u8>,
}

impl EditPart {
    fn cost(&self) -> usize {
        self.removed.len().saturating_add(self.inserted.len())
    }
}

#[derive(Clone, Debug)]
struct Edit {
    parts: Vec<EditPart>,
    /// Content states on either side of the edit, restored by undo and redo.
    before: u64,
    after: u64,
}

impl Edit {
    fn cost(&self) -> usize {
        self.parts.iter().map(EditPart::cost).sum()
    }
}

/// A piece-table document with byte-budgeted undo and redo history.
#[derive(Debug)]
pub struct Document {
    table: PieceTable,
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    history_bytes: usize,
    history_budget: usize,
    /// Identifies the current content; undo and redo return to earlier identities.
    state: u64,
    next_state: u64,
    /// The content state that matches what is saved, when one is reachable.
    clean_state: Option<u64>,
    /// While set, every content change, undo and redo included, is refused.
    read_only: bool,
}

impl Document {
    #[must_use]
    pub fn new(table: PieceTable, history_budget: usize) -> Self {
        Self {
            table,
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_bytes: 0,
            history_budget,
            state: 0,
            next_state: 1,
            clean_state: Some(0),
            read_only: false,
        }
    }

    /// Whether content changes are currently refused.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    /// Refuses or allows content changes. Reading, saving, and history are kept.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }
    fn require_writable(&self) -> Result<(), DocumentError> {
        if self.read_only {
            Err(DocumentError::ReadOnly)
        } else {
            Ok(())
        }
    }

    /// Whether the content equals the saved content. A new document starts
    /// clean, and undoing or redoing back to the saved state makes it clean again.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.clean_state == Some(self.state)
    }
    /// Records the current content as the saved content.
    pub fn mark_clean(&mut self) {
        self.clean_state = Some(self.state);
    }
    /// Records that no reachable content state matches what is saved.
    pub fn mark_unsaved(&mut self) {
        self.clean_state = None;
    }

    fn advance_state(&mut self) -> (u64, u64) {
        let before = self.state;
        self.state = self.next_state;
        self.next_state = self.next_state.wrapping_add(1);
        (before, self.state)
    }

    #[must_use]
    pub fn table(&self) -> &PieceTable {
        &self.table
    }
    #[must_use]
    pub fn table_mut(&mut self) -> &mut PieceTable {
        &mut self.table
    }
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    #[must_use]
    pub fn history_bytes(&self) -> usize {
        self.history_bytes
    }

    /// Replaces backing storage after a successful save while retaining undo
    /// history and the document revision.
    pub fn rebase_saved_source(
        &mut self,
        source: Arc<dyn ReadAt>,
        additions: Box<dyn AppendStore>,
    ) {
        self.table = PieceTable::rebased(source, additions, self.table.revision());
    }

    /// Applies one logical replacement and records its inverse within the history budget.
    ///
    /// # Errors
    /// Returns a document error for invalid ranges, source reads, or append failures.
    pub fn replace(
        &mut self,
        offset: u64,
        delete_len: u64,
        inserted: &[u8],
    ) -> Result<(), DocumentError> {
        self.require_writable()?;
        let removed = self.table.read_vec(offset, delete_len)?;
        self.table.replace(offset, delete_len, inserted)?;
        self.redo.clear();
        let (before, after) = self.advance_state();
        self.push_undo(Edit {
            parts: vec![EditPart {
                offset,
                removed,
                inserted: inserted.to_vec(),
            }],
            before,
            after,
        });
        Ok(())
    }

    /// Applies a descending, non-overlapping replacement batch and records it
    /// as one undoable action. Coordinates refer to the pre-edit document.
    ///
    /// # Errors
    /// Returns a document error for invalid ranges, overlap, reads, or append failures.
    pub fn replace_many(
        &mut self,
        replacements: &[(u64, u64, Vec<u8>)],
    ) -> Result<(), DocumentError> {
        self.require_writable()?;
        if replacements.is_empty() {
            return Ok(());
        }
        let mut parts = Vec::with_capacity(replacements.len());
        for (offset, delete_len, inserted) in replacements {
            parts.push(EditPart {
                offset: *offset,
                removed: self.table.read_vec(*offset, *delete_len)?,
                inserted: inserted.clone(),
            });
        }
        self.table.replace_many(replacements)?;
        self.redo.clear();
        let (before, after) = self.advance_state();
        self.push_undo(Edit {
            parts,
            before,
            after,
        });
        Ok(())
    }

    /// Reverses the most recent recorded edit.
    ///
    /// # Errors
    /// Returns an error if backing storage fails. No-op when history is empty.
    pub fn undo(&mut self) -> Result<bool, DocumentError> {
        self.require_writable()?;
        let Some(edit) = self.undo.pop_back() else {
            return Ok(false);
        };
        self.history_bytes = self.history_bytes.saturating_sub(edit.cost());
        let replacements = inverse_replacements(&edit.parts)?;
        if let Err(error) = self.table.replace_many(&replacements) {
            self.history_bytes = self.history_bytes.saturating_add(edit.cost());
            self.undo.push_back(edit);
            return Err(error);
        }
        self.state = edit.before;
        self.redo.push(edit);
        Ok(true)
    }

    /// Reapplies the most recently undone edit.
    ///
    /// # Errors
    /// Returns an error if backing storage fails. No-op when redo is empty.
    pub fn redo(&mut self) -> Result<bool, DocumentError> {
        self.require_writable()?;
        let Some(edit) = self.redo.pop() else {
            return Ok(false);
        };
        let replacements = edit
            .parts
            .iter()
            .map(|part| {
                let removed_len =
                    u64::try_from(part.removed.len()).map_err(|_| DocumentError::LengthOverflow)?;
                Ok((part.offset, removed_len, part.inserted.clone()))
            })
            .collect::<Result<Vec<_>, DocumentError>>()?;
        if let Err(error) = self.table.replace_many(&replacements) {
            self.redo.push(edit);
            return Err(error);
        }
        self.state = edit.after;
        self.push_undo(edit);
        Ok(true)
    }

    fn push_undo(&mut self, edit: Edit) {
        let cost = edit.cost();
        if cost > self.history_budget {
            self.undo.clear();
            self.history_bytes = 0;
            return;
        }
        while self.history_bytes.saturating_add(cost) > self.history_budget {
            if let Some(removed) = self.undo.pop_front() {
                self.history_bytes = self.history_bytes.saturating_sub(removed.cost());
            } else {
                break;
            }
        }
        self.history_bytes += cost;
        self.undo.push_back(edit);
    }
}

fn inverse_replacements(parts: &[EditPart]) -> Result<Vec<(u64, u64, Vec<u8>)>, DocumentError> {
    let mut lower_delta = 0_i128;
    let mut ascending = Vec::with_capacity(parts.len());
    for part in parts.iter().rev() {
        let offset = i128::from(part.offset)
            .checked_add(lower_delta)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(DocumentError::LengthOverflow)?;
        let inserted_len =
            u64::try_from(part.inserted.len()).map_err(|_| DocumentError::LengthOverflow)?;
        ascending.push((offset, inserted_len, part.removed.clone()));

        let removed_len =
            i128::try_from(part.removed.len()).map_err(|_| DocumentError::LengthOverflow)?;
        let inserted_len = i128::from(inserted_len);
        lower_delta = lower_delta
            .checked_add(inserted_len - removed_len)
            .ok_or(DocumentError::LengthOverflow)?;
    }
    ascending.reverse();
    Ok(ascending)
}

#[cfg(test)]
mod tests {
    use super::Document;
    use crate::{DocumentError, PieceTable};
    use nulis_large_file::{AppendStore, MemoryAppendStore, MemorySource, ReadAt};
    use std::{
        io,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    #[test]
    fn undo_and_redo_return_to_the_clean_state() {
        let mut document = Document::new(PieceTable::from_bytes(b"abc".to_vec()), 1024);
        assert!(document.is_clean());
        document.replace(3, 0, b"d").expect("first edit");
        document.replace(4, 0, b"e").expect("second edit");
        assert!(!document.is_clean());
        document.undo().expect("undo");
        assert!(!document.is_clean());
        document.undo().expect("undo");
        assert!(document.is_clean());
        document.redo().expect("redo");
        assert!(!document.is_clean());

        // Saving moves the clean point; undo past it is dirty, redo returns to it.
        document.mark_clean();
        document.undo().expect("undo past save");
        assert!(!document.is_clean());
        document.redo().expect("redo to save");
        assert!(document.is_clean());

        // Typing the same text again is a new state, as is an unsaved start.
        document.undo().expect("undo");
        document.replace(3, 0, b"d").expect("retyped");
        assert!(!document.is_clean());
        document.mark_unsaved();
        document.undo().expect("undo");
        assert!(!document.is_clean());
    }

    #[test]
    fn a_read_only_document_refuses_every_change_and_keeps_its_history() {
        let mut document = Document::new(PieceTable::from_bytes(b"abc".to_vec()), 1024);
        document.replace(3, 0, b"d").expect("edit");
        document.set_read_only(true);
        assert!(document.is_read_only());
        assert!(matches!(
            document.replace(0, 1, b"x"),
            Err(DocumentError::ReadOnly)
        ));
        assert!(matches!(
            document.replace_many(&[(0, 1, b"x".to_vec())]),
            Err(DocumentError::ReadOnly)
        ));
        assert!(matches!(document.undo(), Err(DocumentError::ReadOnly)));
        assert_eq!(document.table().read_vec(0, 4).expect("read"), b"abcd");

        document.set_read_only(false);
        assert!(document.undo().expect("undo"));
        document.set_read_only(true);
        assert!(matches!(document.redo(), Err(DocumentError::ReadOnly)));
        document.set_read_only(false);
        assert!(document.redo().expect("redo"));
        assert_eq!(document.table().read_vec(0, 4).expect("read"), b"abcd");
    }

    #[test]
    fn undo_and_redo_restore_exact_bytes() {
        let mut document = Document::new(PieceTable::from_bytes(b"hello".to_vec()), 1024);
        document.replace(1, 3, b"i").expect("edit");
        assert_eq!(document.table().read_vec(0, 3).expect("read"), b"hio");
        assert!(document.undo().expect("undo"));
        assert_eq!(document.table().read_vec(0, 5).expect("read"), b"hello");
        assert!(document.redo().expect("redo"));
        assert_eq!(document.table().read_vec(0, 3).expect("read"), b"hio");
    }

    #[test]
    fn history_never_exceeds_its_byte_budget() {
        let mut document = Document::new(PieceTable::from_bytes(Vec::new()), 4);
        for _ in 0..10 {
            document
                .replace(document.table().len(), 0, b"xx")
                .expect("edit");
        }
        assert!(document.history_bytes() <= 4);
        assert!(document.undo().expect("recent edit remains"));
        assert!(document.undo().expect("second recent edit remains"));
        assert!(!document.undo().expect("older edits evicted"));
    }

    #[test]
    fn multi_replacement_is_one_undoable_action() {
        let mut document = Document::new(PieceTable::from_bytes(b"one two three".to_vec()), 1024);
        document
            .replace_many(&[(8, 5, b"3".to_vec()), (0, 3, b"1".to_vec())])
            .expect("batch");
        assert_eq!(document.table().read_vec(0, 7).expect("read"), b"1 two 3");
        assert!(document.undo().expect("undo"));
        assert_eq!(
            document.table().read_vec(0, 13).expect("read"),
            b"one two three"
        );
        assert!(document.redo().expect("redo"));
        assert_eq!(document.table().read_vec(0, 7).expect("read"), b"1 two 3");
    }

    struct ControlledStore {
        inner: MemoryAppendStore,
        fail: Arc<AtomicBool>,
    }

    impl ReadAt for ControlledStore {
        fn len(&self) -> u64 {
            self.inner.len()
        }
        fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
            self.inner.read_at(offset, buffer)
        }
    }

    impl AppendStore for ControlledStore {
        fn append(&mut self, bytes: &[u8]) -> io::Result<u64> {
            if self.fail.load(Ordering::Acquire) {
                return Err(io::Error::other("simulated append failure"));
            }
            self.inner.append(bytes)
        }
        fn sync(&mut self) -> io::Result<()> {
            self.inner.sync()
        }
    }

    #[test]
    fn failed_undo_preserves_document_and_history() {
        let fail = Arc::new(AtomicBool::new(false));
        let table = PieceTable::new(
            Arc::new(MemorySource::new(b"abc".to_vec())),
            Box::new(ControlledStore {
                inner: MemoryAppendStore::new(),
                fail: Arc::clone(&fail),
            }),
        );
        let mut document = Document::new(table, 1024);
        document.replace(1, 1, b"X").expect("edit");
        fail.store(true, Ordering::Release);
        assert!(document.undo().is_err());
        assert!(document.can_undo());
        assert_eq!(document.table().read_vec(0, 3).expect("read"), b"aXc");
        fail.store(false, Ordering::Release);
        assert!(document.undo().expect("retry undo"));
        assert_eq!(document.table().read_vec(0, 3).expect("read"), b"abc");
    }
}
