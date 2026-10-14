//! Scalable document storage and public coordinates.

mod history;
mod piece_table;
mod text_window;

pub use history::Document;
pub use piece_table::{DocumentError, PieceSnapshot, PieceSource, PieceTable, PieceTableSnapshot};
pub use text_window::Utf8Window;

use serde::{Deserialize, Serialize};

/// A byte offset in a document. The newtype prevents accidental 32-bit narrowing.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteOffset(pub u64);

/// A half-open byte range `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl ByteRange {
    /// Constructs a validated range.
    ///
    /// # Errors
    /// Returns [`RangeError`] if `end` precedes `start`.
    pub fn new(start: ByteOffset, end: ByteOffset) -> Result<Self, RangeError> {
        if end < start {
            return Err(RangeError { start, end });
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub fn len(self) -> u64 {
        self.end.0 - self.start.0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeError {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl std::fmt::Display for RangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "range end {} precedes start {}",
            self.end.0, self.start.0
        )
    }
}

impl std::error::Error for RangeError {}

#[cfg(test)]
mod tests {
    use super::{ByteOffset, ByteRange};

    #[test]
    fn offsets_support_positions_beyond_four_gibibytes() {
        let start = ByteOffset(u64::from(u32::MAX) + 1);
        let range = ByteRange::new(start, ByteOffset(start.0 + 20)).expect("valid range");
        assert_eq!(range.len(), 20);
    }

    #[test]
    fn reversed_ranges_are_rejected() {
        assert!(ByteRange::new(ByteOffset(5), ByteOffset(4)).is_err());
    }
}
