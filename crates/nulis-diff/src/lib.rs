//! Bounded text and binary comparison over random-access byte sources.

use nulis_large_file::ReadAt;
use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;

pub const DIFF_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_TEXT_WINDOW_BYTES: usize = 1024 * 1024;
pub const MAX_TEXT_WINDOW_LINES: usize = 1024;
pub const MAX_DIFF_CHANGES: usize = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareMode {
    Text,
    Binary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Insert,
    Delete,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRange {
    #[serde(with = "decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "decimal_u64")]
    pub end_byte: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffChange {
    pub kind: ChangeKind,
    pub left: DiffRange,
    pub right: DiffRange,
    #[serde(with = "decimal_u64")]
    pub left_start_line: u64,
    #[serde(with = "decimal_u64")]
    pub right_start_line: u64,
    pub left_line_count: usize,
    pub right_line_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPage {
    pub mode: CompareMode,
    pub changes: Vec<DiffChange>,
    #[serde(with = "option_decimal_u64")]
    pub next_left_byte: Option<u64>,
    #[serde(with = "option_decimal_u64")]
    pub next_right_byte: Option<u64>,
    #[serde(with = "option_decimal_u64")]
    pub next_left_line: Option<u64>,
    #[serde(with = "option_decimal_u64")]
    pub next_right_line: Option<u64>,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextDiffRequest {
    pub left_start_byte: u64,
    pub right_start_byte: u64,
    pub left_start_line: u64,
    pub right_start_line: u64,
    pub max_bytes: usize,
    pub max_changes: usize,
}

impl Default for TextDiffRequest {
    fn default() -> Self {
        Self {
            left_start_byte: 0,
            right_start_byte: 0,
            left_start_line: 1,
            right_start_line: 1,
            max_bytes: 256 * 1024,
            max_changes: 500,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BinaryDiffRequest {
    pub start_byte: u64,
    pub max_bytes: usize,
    pub max_changes: usize,
}

impl Default for BinaryDiffRequest {
    fn default() -> Self {
        Self {
            start_byte: 0,
            max_bytes: 256 * 1024,
            max_changes: 500,
        }
    }
}

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("comparison byte budget must be between 1 and {MAX_TEXT_WINDOW_BYTES}")]
    InvalidByteBudget,
    #[error("comparison result limit must be between 1 and {MAX_DIFF_CHANGES}")]
    InvalidChangeLimit,
    #[error("comparison offset is outside the source")]
    OffsetOutOfBounds,
    #[error("comparison coordinate overflow")]
    CoordinateOverflow,
    #[error("source read failed: {0}")]
    Io(#[from] io::Error),
}

/// Compares bounded line windows without retaining either complete source.
/// Line endings remain part of each record so LF/CRLF changes are visible.
/// The continuation coordinates allow callers to page through arbitrarily
/// large sources while keeping the dynamic-programming matrix bounded.
///
/// # Errors
/// Returns a typed error for invalid budgets, offsets, overflow, or reads.
pub fn compare_text(
    left: &dyn ReadAt,
    right: &dyn ReadAt,
    request: TextDiffRequest,
) -> Result<DiffPage, DiffError> {
    validate(request.max_bytes, request.max_changes)?;
    if request.left_start_byte > left.len() || request.right_start_byte > right.len() {
        return Err(DiffError::OffsetOutOfBounds);
    }
    let left_window = read_lines(left, request.left_start_byte, request.max_bytes)?;
    let right_window = read_lines(right, request.right_start_byte, request.max_bytes)?;
    let operations = line_operations(&left_window.records, &right_window.records);
    let (mut changes, limited) = changes_from_operations(
        &operations,
        &left_window,
        &right_window,
        request.left_start_line,
        request.right_start_line,
        request.max_changes,
    )?;
    let omitted = limited
        .then(|| changes.get(request.max_changes).cloned())
        .flatten();
    if omitted.is_some() {
        changes.truncate(request.max_changes);
    }
    let next_left_byte = omitted
        .as_ref()
        .map_or(left_window.next_byte, |change| Some(change.left.start_byte));
    let next_right_byte = omitted.as_ref().map_or(right_window.next_byte, |change| {
        Some(change.right.start_byte)
    });
    let has_next = next_left_byte.is_some() || next_right_byte.is_some();
    Ok(DiffPage {
        mode: CompareMode::Text,
        changes,
        next_left_byte,
        next_right_byte,
        next_left_line: omitted.as_ref().map_or_else(
            || {
                has_next.then(|| {
                    request
                        .left_start_line
                        .saturating_add(left_window.complete_lines)
                })
            },
            |change| Some(change.left_start_line),
        ),
        next_right_line: omitted.as_ref().map_or_else(
            || {
                has_next.then(|| {
                    request
                        .right_start_line
                        .saturating_add(right_window.complete_lines)
                })
            },
            |change| Some(change.right_start_line),
        ),
        truncated: limited || has_next,
    })
}

/// Compares aligned raw-byte windows using fixed-size reads. Difference runs
/// are coalesced and result count is capped independently of source size.
///
/// # Errors
/// Returns a typed error for invalid budgets, offsets, overflow, or reads.
pub fn compare_binary(
    left: &dyn ReadAt,
    right: &dyn ReadAt,
    request: BinaryDiffRequest,
) -> Result<DiffPage, DiffError> {
    validate(request.max_bytes, request.max_changes)?;
    if request.start_byte > left.len().max(right.len()) {
        return Err(DiffError::OffsetOutOfBounds);
    }
    let end = request
        .start_byte
        .checked_add(request.max_bytes as u64)
        .ok_or(DiffError::CoordinateOverflow)?
        .min(left.len().max(right.len()));
    let mut offset = request.start_byte;
    let mut changes = Vec::new();
    let mut open: Option<(u64, ChangeKind)> = None;
    let mut limited = false;
    while offset < end {
        let count = usize::try_from((end - offset).min(DIFF_CHUNK_BYTES as u64))
            .map_err(|_| DiffError::CoordinateOverflow)?;
        let mut left_bytes = vec![0; count];
        let mut right_bytes = vec![0; count];
        let left_read = if offset < left.len() {
            left.read_at(offset, &mut left_bytes)?
        } else {
            0
        };
        let right_read = if offset < right.len() {
            right.read_at(offset, &mut right_bytes)?
        } else {
            0
        };
        let present = left_read.max(right_read);
        if present == 0 {
            break;
        }
        let mut processed = present;
        for index in 0..present {
            let left_value = (index < left_read).then(|| left_bytes[index]);
            let right_value = (index < right_read).then(|| right_bytes[index]);
            let kind = match (left_value, right_value) {
                (Some(left), Some(right)) if left != right => Some(ChangeKind::Replace),
                (Some(_), None) => Some(ChangeKind::Delete),
                (None, Some(_)) => Some(ChangeKind::Insert),
                _ => None,
            };
            let position = offset
                .checked_add(index as u64)
                .ok_or(DiffError::CoordinateOverflow)?;
            match (open, kind) {
                (Some((start, current)), Some(next)) if current != next => {
                    push_binary_change(&mut changes, start, position, current);
                    if changes.len() >= request.max_changes {
                        open = None;
                        processed = index;
                        limited = true;
                        break;
                    }
                    open = Some((position, next));
                }
                (None, Some(next)) => open = Some((position, next)),
                (Some((start, current)), None) => {
                    push_binary_change(&mut changes, start, position, current);
                    open = None;
                    if changes.len() >= request.max_changes {
                        processed = index + 1;
                        limited = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        offset = offset
            .checked_add(processed as u64)
            .ok_or(DiffError::CoordinateOverflow)?;
        if limited {
            break;
        }
    }
    if let Some((start, kind)) = open {
        push_binary_change(&mut changes, start, offset, kind);
    }
    if changes.len() > request.max_changes {
        changes.truncate(request.max_changes);
        limited = true;
    }
    let next = (offset < left.len().max(right.len())).then_some(offset);
    Ok(DiffPage {
        mode: CompareMode::Binary,
        changes,
        next_left_byte: next,
        next_right_byte: next,
        next_left_line: None,
        next_right_line: None,
        truncated: limited || next.is_some(),
    })
}

fn validate(max_bytes: usize, max_changes: usize) -> Result<(), DiffError> {
    if max_bytes == 0 || max_bytes > MAX_TEXT_WINDOW_BYTES {
        return Err(DiffError::InvalidByteBudget);
    }
    if max_changes == 0 || max_changes > MAX_DIFF_CHANGES {
        return Err(DiffError::InvalidChangeLimit);
    }
    Ok(())
}

#[derive(Debug)]
struct LineWindow {
    start_byte: u64,
    records: Vec<Vec<u8>>,
    starts: Vec<u64>,
    complete_lines: u64,
    next_byte: Option<u64>,
}

fn read_lines(
    source: &dyn ReadAt,
    start_byte: u64,
    max_bytes: usize,
) -> Result<LineWindow, DiffError> {
    if start_byte == source.len() {
        return Ok(LineWindow {
            start_byte,
            records: Vec::new(),
            starts: vec![start_byte],
            complete_lines: 0,
            next_byte: None,
        });
    }
    let available = source.len() - start_byte;
    let count = usize::try_from(available.min(max_bytes as u64))
        .map_err(|_| DiffError::CoordinateOverflow)?;
    let mut bytes = vec![0; count];
    source.read_exact_at(start_byte, &mut bytes)?;
    let mut records = Vec::new();
    let mut starts = vec![start_byte];
    let mut complete_lines = 0_u64;
    let mut record_start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            let end = index + 1;
            records.push(bytes[record_start..end].to_vec());
            complete_lines = complete_lines.saturating_add(1);
            starts.push(start_byte + end as u64);
            record_start = end;
            if records.len() == MAX_TEXT_WINDOW_LINES {
                break;
            }
        }
    }
    if records.len() < MAX_TEXT_WINDOW_LINES && record_start < bytes.len() {
        records.push(bytes[record_start..].to_vec());
        starts.push(start_byte + bytes.len() as u64);
    }
    let consumed = starts.last().copied().unwrap_or(start_byte);
    let next_byte = (consumed < source.len()).then_some(consumed);
    Ok(LineWindow {
        start_byte,
        records,
        starts,
        complete_lines,
        next_byte,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineOperation {
    Equal,
    Delete,
    Insert,
}

fn line_operations(left: &[Vec<u8>], right: &[Vec<u8>]) -> Vec<LineOperation> {
    let columns = right.len() + 1;
    let mut lengths = vec![0_u16; (left.len() + 1) * columns];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            let value = if left[left_index] == right[right_index] {
                lengths[(left_index + 1) * columns + right_index + 1].saturating_add(1)
            } else {
                lengths[(left_index + 1) * columns + right_index]
                    .max(lengths[left_index * columns + right_index + 1])
            };
            lengths[left_index * columns + right_index] = value;
        }
    }
    let mut operations = Vec::with_capacity(left.len() + right.len());
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() || right_index < right.len() {
        if left_index < left.len()
            && right_index < right.len()
            && left[left_index] == right[right_index]
        {
            operations.push(LineOperation::Equal);
            left_index += 1;
            right_index += 1;
        } else if right_index < right.len()
            && (left_index == left.len()
                || lengths[left_index * columns + right_index + 1]
                    >= lengths[(left_index + 1) * columns + right_index])
        {
            operations.push(LineOperation::Insert);
            right_index += 1;
        } else {
            operations.push(LineOperation::Delete);
            left_index += 1;
        }
    }
    operations
}

fn changes_from_operations(
    operations: &[LineOperation],
    left: &LineWindow,
    right: &LineWindow,
    left_start_line: u64,
    right_start_line: u64,
    max_changes: usize,
) -> Result<(Vec<DiffChange>, bool), DiffError> {
    let (mut left_index, mut right_index) = (0_usize, 0_usize);
    let mut changes = Vec::new();
    let mut operation_index = 0;
    while operation_index < operations.len() {
        if operations[operation_index] == LineOperation::Equal {
            left_index += 1;
            right_index += 1;
            operation_index += 1;
            continue;
        }
        let change_left = left_index;
        let change_right = right_index;
        while operation_index < operations.len()
            && operations[operation_index] != LineOperation::Equal
        {
            match operations[operation_index] {
                LineOperation::Delete => left_index += 1,
                LineOperation::Insert => right_index += 1,
                LineOperation::Equal => unreachable!(),
            }
            operation_index += 1;
        }
        let left_count = left_index - change_left;
        let right_count = right_index - change_right;
        let kind = match (left_count, right_count) {
            (0, _) => ChangeKind::Insert,
            (_, 0) => ChangeKind::Delete,
            _ => ChangeKind::Replace,
        };
        changes.push(DiffChange {
            kind,
            left: DiffRange {
                start_byte: offset_at(left, change_left)?,
                end_byte: offset_at(left, left_index)?,
            },
            right: DiffRange {
                start_byte: offset_at(right, change_right)?,
                end_byte: offset_at(right, right_index)?,
            },
            left_start_line: left_start_line.saturating_add(change_left as u64),
            right_start_line: right_start_line.saturating_add(change_right as u64),
            left_line_count: left_count,
            right_line_count: right_count,
        });
        if changes.len() > max_changes {
            return Ok((changes, true));
        }
    }
    Ok((changes, false))
}

fn offset_at(window: &LineWindow, index: usize) -> Result<u64, DiffError> {
    window
        .starts
        .get(index)
        .copied()
        .or_else(|| (index == 0).then_some(window.start_byte))
        .ok_or(DiffError::CoordinateOverflow)
}

fn push_binary_change(changes: &mut Vec<DiffChange>, start: u64, end: u64, kind: ChangeKind) {
    let (left, right) = match kind {
        ChangeKind::Insert => (
            DiffRange {
                start_byte: start,
                end_byte: start,
            },
            DiffRange {
                start_byte: start,
                end_byte: end,
            },
        ),
        ChangeKind::Delete => (
            DiffRange {
                start_byte: start,
                end_byte: end,
            },
            DiffRange {
                start_byte: start,
                end_byte: start,
            },
        ),
        ChangeKind::Replace => (
            DiffRange {
                start_byte: start,
                end_byte: end,
            },
            DiffRange {
                start_byte: start,
                end_byte: end,
            },
        ),
    };
    changes.push(DiffChange {
        kind,
        left,
        right,
        left_start_line: 0,
        right_start_line: 0,
        left_line_count: 0,
        right_line_count: 0,
    });
}

mod decimal_u64 {
    use serde::{ser::Error, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)] // Serde's `with` adapter requires this signature.
    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if value.to_string().is_empty() {
            return Err(S::Error::custom("invalid coordinate"));
        }
        serializer.serialize_str(&value.to_string())
    }
}

mod option_decimal_u64 {
    use serde::Serializer;

    #[allow(clippy::ref_option)] // Serde's `with` adapter requires this signature.
    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{compare_binary, compare_text, BinaryDiffRequest, ChangeKind, TextDiffRequest};
    use nulis_large_file::MemorySource;
    use nulis_large_file::ReadAt;
    use std::io;

    #[derive(Debug)]
    struct VirtualSource {
        len: u64,
        changed_at: Option<u64>,
    }

    impl ReadAt for VirtualSource {
        fn len(&self) -> u64 {
            self.len
        }

        fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
            if offset >= self.len {
                return Ok(0);
            }
            let count = usize::try_from((self.len - offset).min(buffer.len() as u64))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            buffer[..count].fill(0);
            if let Some(changed_at) = self.changed_at {
                if changed_at >= offset && changed_at < offset + count as u64 {
                    let index = usize::try_from(changed_at - offset)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
                    buffer[index] = 1;
                }
            }
            Ok(count)
        }
    }

    #[test]
    fn text_diff_detects_insert_delete_and_replace_with_byte_ranges() {
        let left = MemorySource::new(b"same\nold\ndeleted\ntail\n".to_vec());
        let right = MemorySource::new(b"same\nnew\ntail\nadded\n".to_vec());
        let page = compare_text(&left, &right, TextDiffRequest::default()).expect("diff");
        assert_eq!(page.changes.len(), 2);
        assert_eq!(page.changes[0].kind, ChangeKind::Replace);
        assert_eq!(
            (
                page.changes[0].left.start_byte,
                page.changes[0].left.end_byte
            ),
            (5, 17)
        );
        assert_eq!(
            (
                page.changes[0].right.start_byte,
                page.changes[0].right.end_byte
            ),
            (5, 9)
        );
        assert_eq!(page.changes[1].kind, ChangeKind::Insert);
        assert!(!page.truncated);
    }

    #[test]
    fn text_diff_pages_at_bounded_line_count() {
        let bytes = b"x\n".repeat(1_100);
        let left = MemorySource::new(bytes.clone());
        let right = MemorySource::new(bytes);
        let page = compare_text(
            &left,
            &right,
            TextDiffRequest {
                max_bytes: 1024 * 1024,
                ..TextDiffRequest::default()
            },
        )
        .expect("diff");
        assert!(page.truncated);
        assert_eq!(page.next_left_byte, Some(2_048));
        assert_eq!(page.changes, Vec::new());
    }

    #[test]
    fn binary_diff_coalesces_runs_and_preserves_length_changes() {
        let left = MemorySource::new(vec![0, 1, 2, 3, 4]);
        let right = MemorySource::new(vec![0, 9, 8, 3, 4, 5]);
        let page = compare_binary(&left, &right, BinaryDiffRequest::default()).expect("diff");
        assert_eq!(page.changes.len(), 2);
        assert_eq!(page.changes[0].kind, ChangeKind::Replace);
        assert_eq!(page.changes[0].left.start_byte, 1);
        assert_eq!(page.changes[0].left.end_byte, 3);
        assert_eq!(page.changes[1].kind, ChangeKind::Insert);
        assert_eq!(page.changes[1].right.end_byte, 6);
    }

    #[test]
    fn result_limits_return_lossless_text_and_binary_continuations() {
        let text_left = MemorySource::new(b"a\nsame\nb\n".to_vec());
        let text_right = MemorySource::new(b"x\nsame\ny\n".to_vec());
        let first = compare_text(
            &text_left,
            &text_right,
            TextDiffRequest {
                max_changes: 1,
                ..TextDiffRequest::default()
            },
        )
        .expect("first text page");
        assert_eq!(first.changes.len(), 1);
        assert_eq!(first.next_left_byte, Some(7));
        let second = compare_text(
            &text_left,
            &text_right,
            TextDiffRequest {
                left_start_byte: first.next_left_byte.expect("left cursor"),
                right_start_byte: first.next_right_byte.expect("right cursor"),
                left_start_line: first.next_left_line.expect("left line"),
                right_start_line: first.next_right_line.expect("right line"),
                max_changes: 1,
                ..TextDiffRequest::default()
            },
        )
        .expect("second text page");
        assert_eq!(second.changes.len(), 1);
        assert_eq!(second.changes[0].left.start_byte, 7);

        let binary_left = MemorySource::new(vec![0, 0, 0, 0]);
        let binary_right = MemorySource::new(vec![1, 0, 1, 0]);
        let binary = compare_binary(
            &binary_left,
            &binary_right,
            BinaryDiffRequest {
                max_changes: 1,
                ..BinaryDiffRequest::default()
            },
        )
        .expect("binary page");
        assert_eq!(binary.changes.len(), 1);
        assert_eq!(binary.next_left_byte, Some(2));
    }

    #[test]
    fn rejects_unbounded_requests() {
        let source = MemorySource::new(Vec::new());
        assert!(compare_text(
            &source,
            &source,
            TextDiffRequest {
                max_bytes: usize::MAX,
                ..TextDiffRequest::default()
            }
        )
        .is_err());
    }

    #[test]
    fn binary_diff_preserves_offsets_beyond_four_gibibytes() {
        let changed_at = 5 * 1024 * 1024 * 1024 + 7;
        let left = VirtualSource {
            len: changed_at + 10,
            changed_at: None,
        };
        let right = VirtualSource {
            len: changed_at + 10,
            changed_at: Some(changed_at),
        };
        let page = compare_binary(
            &left,
            &right,
            BinaryDiffRequest {
                start_byte: changed_at - 4,
                max_bytes: 12,
                max_changes: 10,
            },
        )
        .expect("deep comparison");
        assert_eq!(page.changes[0].left.start_byte, changed_at);
        assert_eq!(page.changes[0].left.end_byte, changed_at + 1);
    }
}
