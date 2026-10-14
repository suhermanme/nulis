use nulis_large_file::{AppendStore, MemoryAppendStore, ReadAt};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    sync::Arc,
};
use thiserror::Error;

const STREAM_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StoreKind {
    Original,
    Add,
}

const PIECE_TABLE_SNAPSHOT_VERSION: u32 = 1;

/// Stable source identifier used by recovery manifests instead of exposing
/// private tree nodes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PieceSource {
    Original,
    Add,
}

/// One immutable source range in a serialized piece-table snapshot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceSnapshot {
    pub source: PieceSource,
    pub offset: u64,
    pub len: u64,
}

/// Versioned, implementation-neutral state needed to recover current content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceTableSnapshot {
    pub format_version: u32,
    pub original_len: u64,
    pub additions_len: u64,
    pub document_len: u64,
    pub revision: u64,
    pub pieces: Vec<PieceSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Piece {
    store: StoreKind,
    offset: u64,
    len: u64,
}

type Link = Option<Box<Node>>;

#[derive(Debug)]
struct Node {
    piece: Piece,
    priority: u64,
    total_len: u64,
    left: Link,
    right: Link,
}

impl Node {
    fn new(piece: Piece, priority: u64) -> Self {
        Self {
            piece,
            priority,
            total_len: piece.len,
            left: None,
            right: None,
        }
    }

    fn refresh(&mut self) {
        self.total_len = total(&self.left) + self.piece.len + total(&self.right);
    }
}

fn total(link: &Link) -> u64 {
    link.as_ref().map_or(0, |node| node.total_len)
}

/// An implicit treap of immutable source pieces. The original source is never
/// copied; inserted bytes live in an append-only store.
pub struct PieceTable {
    original: Arc<dyn ReadAt>,
    additions: Box<dyn AppendStore>,
    root: Link,
    priority_state: u64,
    revision: u64,
}

impl std::fmt::Debug for PieceTable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PieceTable")
            .field("len", &self.len())
            .field("pieces", &self.piece_count())
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl PieceTable {
    #[must_use]
    pub fn new(original: Arc<dyn ReadAt>, additions: Box<dyn AppendStore>) -> Self {
        let len = original.len();
        let mut table = Self {
            original,
            additions,
            root: None,
            priority_state: 0x9E37_79B9_7F4A_7C15,
            revision: 0,
        };
        if len > 0 {
            let priority = table.next_priority();
            table.root = Some(Box::new(Node::new(
                Piece {
                    store: StoreKind::Original,
                    offset: 0,
                    len,
                },
                priority,
            )));
        }
        table
    }

    /// Starts a new source-backed table at an existing logical revision after
    /// the current contents have been saved to `original`.
    #[must_use]
    pub fn rebased(
        original: Arc<dyn ReadAt>,
        additions: Box<dyn AppendStore>,
        revision: u64,
    ) -> Self {
        let mut table = Self::new(original, additions);
        table.revision = revision;
        table
    }

    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self::new(
            Arc::new(nulis_large_file::MemorySource::new(bytes)),
            Box::new(MemoryAppendStore::new()),
        )
    }

    /// Reconstructs a logical document from a validated public snapshot.
    /// Orphan bytes at the end of the append store are allowed because a crash
    /// can occur after appending data but before committing the manifest.
    ///
    /// # Errors
    /// Returns an error if source lengths or any piece range are inconsistent.
    pub fn from_snapshot(
        original: Arc<dyn ReadAt>,
        additions: Box<dyn AppendStore>,
        snapshot: &PieceTableSnapshot,
    ) -> Result<Self, DocumentError> {
        if snapshot.format_version != PIECE_TABLE_SNAPSHOT_VERSION {
            return Err(DocumentError::SnapshotVersion(snapshot.format_version));
        }
        if original.len() != snapshot.original_len {
            return Err(DocumentError::OriginalLengthMismatch {
                expected: snapshot.original_len,
                actual: original.len(),
            });
        }
        if additions.len() < snapshot.additions_len {
            return Err(DocumentError::AdditionsTruncated {
                expected: snapshot.additions_len,
                actual: additions.len(),
            });
        }

        let mut table = Self {
            original,
            additions,
            root: None,
            priority_state: 0x9E37_79B9_7F4A_7C15,
            revision: snapshot.revision,
        };
        let mut logical_len = 0_u64;
        for piece in &snapshot.pieces {
            if piece.len == 0 {
                return Err(DocumentError::InvalidSnapshotPiece);
            }
            let source_len = match piece.source {
                PieceSource::Original => snapshot.original_len,
                PieceSource::Add => snapshot.additions_len,
            };
            let end = piece
                .offset
                .checked_add(piece.len)
                .ok_or(DocumentError::InvalidSnapshotPiece)?;
            if end > source_len {
                return Err(DocumentError::InvalidSnapshotPiece);
            }
            logical_len = logical_len
                .checked_add(piece.len)
                .ok_or(DocumentError::LengthOverflow)?;
            let store = match piece.source {
                PieceSource::Original => StoreKind::Original,
                PieceSource::Add => StoreKind::Add,
            };
            let priority = table.next_priority();
            table.root = merge(
                table.root.take(),
                Some(Box::new(Node::new(
                    Piece {
                        store,
                        offset: piece.offset,
                        len: piece.len,
                    },
                    priority,
                ))),
            );
        }
        if logical_len != snapshot.document_len {
            return Err(DocumentError::SnapshotLengthMismatch {
                expected: snapshot.document_len,
                actual: logical_len,
            });
        }
        Ok(table)
    }

    /// Captures only stable source ranges and scalar metadata. No source bytes
    /// or private balancing details are serialized.
    #[must_use]
    pub fn snapshot(&self) -> PieceTableSnapshot {
        let mut pieces = Vec::with_capacity(self.piece_count());
        collect_pieces(&self.root, &mut pieces);
        PieceTableSnapshot {
            format_version: PIECE_TABLE_SNAPSHOT_VERSION,
            original_len: self.original.len(),
            additions_len: self.additions.len(),
            document_len: self.len(),
            revision: self.revision,
            pieces,
        }
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        total(&self.root)
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub fn piece_count(&self) -> usize {
        count_nodes(&self.root)
    }

    /// Inserts bytes at a logical document offset.
    ///
    /// # Errors
    /// Returns an error for an invalid offset, overflow, or append-store failure.
    pub fn insert(&mut self, offset: u64, bytes: &[u8]) -> Result<(), DocumentError> {
        self.replace(offset, 0, bytes)
    }

    /// Deletes a logical byte range.
    ///
    /// # Errors
    /// Returns an error when the range is outside the document.
    pub fn delete(&mut self, offset: u64, len: u64) -> Result<(), DocumentError> {
        self.replace(offset, len, &[])
    }

    /// Atomically changes the logical tree after inserted bytes are safely
    /// appended. An append failure leaves the document tree unchanged.
    ///
    /// # Errors
    /// Returns an error for an invalid range, overflow, or append failure.
    pub fn replace(
        &mut self,
        offset: u64,
        delete_len: u64,
        inserted: &[u8],
    ) -> Result<(), DocumentError> {
        self.validate_range(offset, delete_len)?;
        let inserted_len =
            u64::try_from(inserted.len()).map_err(|_| DocumentError::LengthOverflow)?;
        self.len()
            .checked_sub(delete_len)
            .and_then(|len| len.checked_add(inserted_len))
            .ok_or(DocumentError::LengthOverflow)?;
        let add_offset = if inserted.is_empty() {
            0
        } else {
            self.additions.append(inserted)?
        };

        let root = self.root.take();
        let (left, remainder) = self.split(root, offset);
        let (_, right) = self.split(remainder, delete_len);
        let middle = if inserted.is_empty() {
            None
        } else {
            Some(Box::new(Node::new(
                Piece {
                    store: StoreKind::Add,
                    offset: add_offset,
                    len: inserted_len,
                },
                self.next_priority(),
            )))
        };
        self.root = merge(merge(left, middle), right);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Applies non-overlapping replacements as one logical revision. Input
    /// coordinates refer to the document before the batch and must be ordered
    /// from the highest offset to the lowest. All inserted bytes are appended
    /// before the tree is changed, so append failure leaves the document intact.
    ///
    /// # Errors
    /// Returns an error for invalid/overlapping ranges, overflow, or append failure.
    pub fn replace_many(
        &mut self,
        replacements: &[(u64, u64, Vec<u8>)],
    ) -> Result<(), DocumentError> {
        if replacements.is_empty() {
            return Ok(());
        }
        let original_len = self.len();
        let mut lower_bound = original_len;
        let mut _final_len = original_len;
        let mut additions = Vec::new();
        let mut add_ranges = Vec::with_capacity(replacements.len());

        for (offset, delete_len, inserted) in replacements {
            self.validate_range(*offset, *delete_len)?;
            let end = offset
                .checked_add(*delete_len)
                .ok_or(DocumentError::LengthOverflow)?;
            if end > lower_bound {
                return Err(DocumentError::OverlappingBatch);
            }
            lower_bound = *offset;
            let inserted_len =
                u64::try_from(inserted.len()).map_err(|_| DocumentError::LengthOverflow)?;
            _final_len = _final_len
                .checked_sub(*delete_len)
                .and_then(|len| len.checked_add(inserted_len))
                .ok_or(DocumentError::LengthOverflow)?;
            let relative =
                u64::try_from(additions.len()).map_err(|_| DocumentError::LengthOverflow)?;
            additions.extend_from_slice(inserted);
            add_ranges.push((relative, inserted_len));
        }
        let additions_start = if additions.is_empty() {
            0
        } else {
            self.additions.append(&additions)?
        };

        for ((offset, delete_len, _), (relative, inserted_len)) in
            replacements.iter().zip(add_ranges)
        {
            let add_offset = additions_start
                .checked_add(relative)
                .ok_or(DocumentError::LengthOverflow)?;
            self.replace_tree(*offset, *delete_len, add_offset, inserted_len);
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Copies a range into a new vector. Callers should use this only for
    /// bounded ranges such as viewports or undo records.
    ///
    /// # Errors
    /// Returns an error for an invalid range, platform-size overflow, or source read failure.
    pub fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError> {
        self.validate_range(offset, len)?;
        let size = usize::try_from(len).map_err(|_| DocumentError::AllocationTooLarge(len))?;
        let mut bytes = vec![0; size];
        self.read_exact_at(offset, &mut bytes)?;
        Ok(bytes)
    }

    /// Streams all logical pieces in order with a fixed-size transfer buffer.
    ///
    /// # Errors
    /// Returns an error from a backing store or output writer.
    pub fn write_to(&self, writer: &mut dyn Write) -> io::Result<u64> {
        let mut buffer = vec![0; STREAM_BUFFER_SIZE];
        write_node(&self.root, self, writer, &mut buffer)?;
        Ok(self.len())
    }

    /// Flushes append storage for session durability.
    ///
    /// # Errors
    /// Returns an I/O error from the append store.
    pub fn sync_additions(&mut self) -> io::Result<()> {
        self.additions.sync()
    }

    fn validate_range(&self, offset: u64, len: u64) -> Result<(), DocumentError> {
        let end = offset
            .checked_add(len)
            .ok_or(DocumentError::LengthOverflow)?;
        if end > self.len() {
            return Err(DocumentError::Range {
                offset,
                len,
                document_len: self.len(),
            });
        }
        Ok(())
    }

    fn replace_tree(&mut self, offset: u64, delete_len: u64, add_offset: u64, add_len: u64) {
        let root = self.root.take();
        let (left, remainder) = self.split(root, offset);
        let (_, right) = self.split(remainder, delete_len);
        let middle = (add_len > 0).then(|| {
            let priority = self.next_priority();
            Box::new(Node::new(
                Piece {
                    store: StoreKind::Add,
                    offset: add_offset,
                    len: add_len,
                },
                priority,
            ))
        });
        self.root = merge(merge(left, middle), right);
    }

    fn source(&self, kind: StoreKind) -> &dyn ReadAt {
        match kind {
            StoreKind::Original => self.original.as_ref(),
            StoreKind::Add => self.additions.as_ref(),
        }
    }

    fn next_priority(&mut self) -> u64 {
        self.priority_state = self.priority_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.priority_state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn split(&mut self, root: Link, at: u64) -> (Link, Link) {
        let Some(mut node) = root else {
            return (None, None);
        };
        let left_len = total(&node.left);
        let piece_end = left_len + node.piece.len;
        if at < left_len {
            let (left, inner) = self.split(node.left.take(), at);
            node.left = inner;
            node.refresh();
            return (left, Some(node));
        }
        if at > piece_end {
            let (inner, right) = self.split(node.right.take(), at - piece_end);
            node.right = inner;
            node.refresh();
            return (Some(node), right);
        }
        if at == left_len {
            let left = node.left.take();
            node.refresh();
            return (left, Some(node));
        }
        if at == piece_end {
            let right = node.right.take();
            node.refresh();
            return (Some(node), right);
        }

        let within = at - left_len;
        let left_piece = Piece {
            len: within,
            ..node.piece
        };
        let right_piece = Piece {
            offset: node.piece.offset + within,
            len: node.piece.len - within,
            ..node.piece
        };
        let left_priority = self.next_priority();
        let right_priority = self.next_priority();
        let left = merge(
            node.left.take(),
            Some(Box::new(Node::new(left_piece, left_priority))),
        );
        let right = merge(
            Some(Box::new(Node::new(right_piece, right_priority))),
            node.right.take(),
        );
        (left, right)
    }
}

impl ReadAt for PieceTable {
    fn len(&self) -> u64 {
        self.len()
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len() || buffer.is_empty() {
            return Ok(0);
        }
        let available = self.len() - offset;
        let wanted = usize::try_from(
            available.min(u64::try_from(buffer.len()).expect("usize always fits into u64")),
        )
        .expect("bounded by buffer length");
        read_node(&self.root, self, 0, offset, &mut buffer[..wanted])?;
        Ok(wanted)
    }
}

fn merge(left: Link, right: Link) -> Link {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(mut left), Some(right)) if left.priority >= right.priority => {
            left.right = merge(left.right.take(), Some(right));
            left.refresh();
            Some(left)
        }
        (Some(left), Some(mut right)) => {
            right.left = merge(Some(left), right.left.take());
            right.refresh();
            Some(right)
        }
    }
}

fn read_node(
    root: &Link,
    table: &PieceTable,
    base: u64,
    query_start: u64,
    output: &mut [u8],
) -> io::Result<()> {
    let Some(node) = root else {
        return Ok(());
    };
    let query_end = query_start + u64::try_from(output.len()).expect("usize always fits into u64");
    let left_len = total(&node.left);
    let piece_start = base + left_len;
    let piece_end = piece_start + node.piece.len;
    if query_start < piece_start {
        read_node(&node.left, table, base, query_start, output)?;
    }
    let overlap_start = query_start.max(piece_start);
    let overlap_end = query_end.min(piece_end);
    if overlap_start < overlap_end {
        let target = usize::try_from(overlap_start - query_start).expect("bounded by output");
        let count = usize::try_from(overlap_end - overlap_start).expect("bounded by output");
        table.source(node.piece.store).read_exact_at(
            node.piece.offset + overlap_start - piece_start,
            &mut output[target..target + count],
        )?;
    }
    if query_end > piece_end {
        read_node(&node.right, table, piece_end, query_start, output)?;
    }
    Ok(())
}

fn write_node(
    root: &Link,
    table: &PieceTable,
    writer: &mut dyn Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    let Some(node) = root else {
        return Ok(());
    };
    write_node(&node.left, table, writer, buffer)?;
    let mut offset = 0;
    while offset < node.piece.len {
        let count = usize::try_from(
            (node.piece.len - offset)
                .min(u64::try_from(buffer.len()).expect("usize always fits into u64")),
        )
        .expect("bounded by buffer");
        table
            .source(node.piece.store)
            .read_exact_at(node.piece.offset + offset, &mut buffer[..count])?;
        writer.write_all(&buffer[..count])?;
        offset += u64::try_from(count).expect("usize always fits into u64");
    }
    write_node(&node.right, table, writer, buffer)
}

fn count_nodes(root: &Link) -> usize {
    root.as_ref().map_or(0, |node| {
        1 + count_nodes(&node.left) + count_nodes(&node.right)
    })
}

fn collect_pieces(root: &Link, pieces: &mut Vec<PieceSnapshot>) {
    let Some(node) = root else {
        return;
    };
    collect_pieces(&node.left, pieces);
    pieces.push(PieceSnapshot {
        source: match node.piece.store {
            StoreKind::Original => PieceSource::Original,
            StoreKind::Add => PieceSource::Add,
        },
        offset: node.piece.offset,
        len: node.piece.len,
    });
    collect_pieces(&node.right, pieces);
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("range {offset}..+{len} exceeds document length {document_len}")]
    Range {
        offset: u64,
        len: u64,
        document_len: u64,
    },
    #[error("batch replacements overlap or are not ordered from highest offset to lowest")]
    OverlappingBatch,
    #[error("document length overflow")]
    LengthOverflow,
    #[error("the document is read-only")]
    ReadOnly,
    #[error("range of {0} bytes cannot be allocated on this platform")]
    AllocationTooLarge(u64),
    #[error("unsupported piece-table snapshot version {0}")]
    SnapshotVersion(u32),
    #[error("snapshot expects original source length {expected}, found {actual}")]
    OriginalLengthMismatch { expected: u64, actual: u64 },
    #[error("snapshot expects at least {expected} append bytes, found {actual}")]
    AdditionsTruncated { expected: u64, actual: u64 },
    #[error("snapshot contains an invalid piece range")]
    InvalidSnapshotPiece,
    #[error("snapshot declares document length {expected}, pieces total {actual}")]
    SnapshotLengthMismatch { expected: u64, actual: u64 },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{DocumentError, PieceTable};
    use nulis_large_file::{MemoryAppendStore, ReadAt};
    use std::{io, sync::Arc};

    #[test]
    fn edits_match_a_byte_vector_model() {
        let mut table = PieceTable::from_bytes(b"seed".to_vec());
        let mut model = b"seed".to_vec();
        let mut state = 7_u64;
        for step in 0..1_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let offset = usize::try_from(state % u64::try_from(model.len() + 1).expect("length"))
                .expect("offset");
            if step % 3 == 0 && !model.is_empty() && offset < model.len() {
                let count = (model.len() - offset).min(1 + step % 5);
                table
                    .delete(
                        u64::try_from(offset).expect("offset"),
                        u64::try_from(count).expect("count"),
                    )
                    .expect("delete");
                model.drain(offset..offset + count);
            } else {
                let bytes = [u8::try_from(step % 251).expect("byte"), b'x'];
                table
                    .insert(u64::try_from(offset).expect("offset"), &bytes)
                    .expect("insert");
                model.splice(offset..offset, bytes);
            }
            assert_eq!(table.read_vec(0, table.len()).expect("read"), model);
        }
    }

    #[test]
    fn generated_replacements_and_recovery_match_a_byte_vector() {
        for seed in 1_u64..=32 {
            let original = Arc::new(nulis_large_file::MemorySource::new(vec![0, 255, 10, 13]));
            let mut table = PieceTable::new(original.clone(), Box::new(MemoryAppendStore::new()));
            let mut expected = vec![0, 255, 10, 13];
            let mut state = seed;
            for step in 0..400 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let offset =
                    usize::try_from(state % u64::try_from(expected.len() + 1).expect("size"))
                        .expect("offset");
                let available = expected.len() - offset;
                let deleted = available.min(usize::try_from((state >> 8) % 7).expect("count"));
                let inserted = (0..usize::try_from((state >> 16) % 7).expect("count"))
                    .map(|index| u8::try_from((state >> ((index % 8) * 8)) & 255).expect("byte"))
                    .collect::<Vec<_>>();
                table
                    .replace(
                        u64::try_from(offset).expect("offset"),
                        u64::try_from(deleted).expect("delete length"),
                        &inserted,
                    )
                    .expect("replace");
                expected.splice(offset..offset + deleted, inserted);
                if step % 31 == 0 {
                    assert_eq!(
                        table.read_vec(0, table.len()).expect("read"),
                        expected,
                        "seed {seed} step {step}"
                    );
                }
            }
            let snapshot = table.snapshot();
            let recovered = PieceTable::from_snapshot(original, table.additions, &snapshot)
                .expect("snapshot restore");
            assert_eq!(
                recovered.read_vec(0, recovered.len()).expect("read"),
                expected,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn batch_replacements_share_one_revision_and_use_original_coordinates() {
        let mut table = PieceTable::from_bytes(b"alpha beta gamma".to_vec());
        table
            .replace_many(&[
                (11, 5, b"G".to_vec()),
                (6, 4, b"B".to_vec()),
                (0, 5, b"A".to_vec()),
            ])
            .expect("batch");
        assert_eq!(table.read_vec(0, table.len()).expect("read"), b"A B G");
        assert_eq!(table.revision(), 1);
    }

    #[test]
    fn batch_rejects_overlap_without_changing_the_tree() {
        let mut table = PieceTable::from_bytes(b"abcdef".to_vec());
        assert!(matches!(
            table.replace_many(&[(3, 2, b"x".to_vec()), (2, 2, b"y".to_vec())]),
            Err(DocumentError::OverlappingBatch)
        ));
        assert_eq!(table.read_vec(0, table.len()).expect("read"), b"abcdef");
        assert_eq!(table.revision(), 0);
    }

    #[derive(Debug)]
    struct VirtualSource {
        len: u64,
    }
    impl ReadAt for VirtualSource {
        fn len(&self) -> u64 {
            self.len
        }
        fn read_at(&self, offset: u64, output: &mut [u8]) -> io::Result<usize> {
            if offset >= self.len {
                return Ok(0);
            }
            let count = usize::try_from(
                (self.len - offset).min(u64::try_from(output.len()).expect("length")),
            )
            .expect("count");
            for (index, byte) in output[..count].iter_mut().enumerate() {
                let value = (offset + u64::try_from(index).expect("index")) & 0xff;
                *byte = u8::try_from(value).expect("value is masked to one byte");
            }
            Ok(count)
        }
    }

    #[test]
    fn edits_and_reads_support_offsets_beyond_four_gibibytes() {
        let boundary = u64::from(u32::MAX) + 8;
        let mut table = PieceTable::new(
            Arc::new(VirtualSource { len: boundary + 10 }),
            Box::new(MemoryAppendStore::new()),
        );
        table.insert(boundary, b"NULIS").expect("insert");
        assert_eq!(
            table.read_vec(boundary - 2, 9).expect("read"),
            vec![5, 6, b'N', b'U', b'L', b'I', b'S', 7, 8]
        );
    }

    #[test]
    fn stream_output_matches_logical_document() {
        let mut table = PieceTable::from_bytes(b"abcdef".to_vec());
        table.replace(2, 3, b"XYZ").expect("replace");
        let mut output = Vec::new();
        assert_eq!(table.write_to(&mut output).expect("write"), 6);
        assert_eq!(output, b"abXYZf");
    }

    fn height(root: &super::Link) -> usize {
        root.as_ref()
            .map_or(0, |node| 1 + height(&node.left).max(height(&node.right)))
    }

    #[test]
    fn sequential_edits_remain_balanced() {
        let mut table = PieceTable::from_bytes(Vec::new());
        for _ in 0..5_000 {
            table.insert(table.len(), b"x").expect("insert");
        }
        assert_eq!(table.piece_count(), 5_000);
        assert!(
            height(&table.root) < 128,
            "treap height was {}",
            height(&table.root)
        );
    }

    #[test]
    fn snapshot_round_trip_restores_logical_content() {
        let original = Arc::new(nulis_large_file::MemorySource::new(b"abcdef".to_vec()));
        let mut table = PieceTable::new(original.clone(), Box::new(MemoryAppendStore::new()));
        table.replace(2, 2, b"NULIS").expect("edit");
        table.insert(0, b">").expect("insert");
        let snapshot = table.snapshot();

        let recovered =
            PieceTable::from_snapshot(original, table.additions, &snapshot).expect("restore");
        assert_eq!(
            recovered.read_vec(0, recovered.len()).expect("read"),
            b">abNULISef"
        );
        assert_eq!(recovered.revision(), 2);
    }

    #[test]
    fn snapshot_rejects_changed_original_source() {
        let table = PieceTable::from_bytes(b"abc".to_vec());
        let snapshot = table.snapshot();
        let error = PieceTable::from_snapshot(
            Arc::new(nulis_large_file::MemorySource::new(b"abcd".to_vec())),
            Box::new(MemoryAppendStore::new()),
            &snapshot,
        )
        .expect_err("changed source must fail");
        assert!(matches!(
            error,
            DocumentError::OriginalLengthMismatch { .. }
        ));
    }
}
