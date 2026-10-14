//! Bounded hexadecimal viewport and byte-editing utilities.

use nulis_document::ByteOffset;
use thiserror::Error;

pub const DEFAULT_BYTES_PER_ROW: u8 = 16;
pub const MIN_BYTES_PER_ROW: u8 = 8;
pub const MAX_BYTES_PER_ROW: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HexPosition {
    pub offset: ByteOffset,
    pub high_nibble: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HexLayout {
    bytes_per_row: u8,
}

impl HexLayout {
    /// Creates a validated fixed-width hexadecimal row layout.
    ///
    /// # Errors
    /// Returns [`HexError::BytesPerRow`] outside the supported range.
    pub fn new(bytes_per_row: u8) -> Result<Self, HexError> {
        if !(MIN_BYTES_PER_ROW..=MAX_BYTES_PER_ROW).contains(&bytes_per_row) {
            return Err(HexError::BytesPerRow(bytes_per_row));
        }
        Ok(Self { bytes_per_row })
    }

    #[must_use]
    pub fn bytes_per_row(self) -> u8 {
        self.bytes_per_row
    }

    #[must_use]
    pub fn align_offset(self, offset: ByteOffset) -> ByteOffset {
        let width = u64::from(self.bytes_per_row);
        ByteOffset(offset.0 - offset.0 % width)
    }

    /// Returns bounded row slices without copying their byte content.
    #[must_use]
    pub fn rows(self, start: ByteOffset, bytes: &[u8]) -> Vec<HexRow<'_>> {
        let width = usize::from(self.bytes_per_row);
        bytes
            .chunks(width)
            .enumerate()
            .map(|(index, bytes)| HexRow {
                offset: ByteOffset(start.0.saturating_add(
                    u64::try_from(index.saturating_mul(width)).unwrap_or(u64::MAX),
                )),
                bytes,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HexRow<'a> {
    pub offset: ByteOffset,
    pub bytes: &'a [u8],
}

/// Replaces one half-byte while preserving the other half.
///
/// # Errors
/// Returns [`HexError::Nibble`] when `nibble` is not a hexadecimal digit value.
pub fn replace_nibble(byte: u8, high_nibble: bool, nibble: u8) -> Result<u8, HexError> {
    if nibble > 0x0f {
        return Err(HexError::Nibble(nibble));
    }
    Ok(if high_nibble {
        (byte & 0x0f) | (nibble << 4)
    } else {
        (byte & 0xf0) | nibble
    })
}

/// Parses hexadecimal clipboard text. ASCII whitespace and `_` separators are
/// accepted; every other character must be a paired hexadecimal digit.
///
/// # Errors
/// Returns a typed error for invalid digits, an incomplete byte, or a result
/// larger than `maximum_bytes`.
pub fn parse_hex(text: &str, maximum_bytes: usize) -> Result<Vec<u8>, HexError> {
    let mut output = Vec::new();
    let mut high = None;
    for character in text.chars() {
        if character.is_ascii_whitespace() || character == '_' {
            continue;
        }
        let value = u8::try_from(
            character
                .to_digit(16)
                .ok_or(HexError::InvalidDigit(character))?,
        )
        .map_err(|_| HexError::InvalidDigit(character))?;
        if let Some(previous) = high.take() {
            if output.len() == maximum_bytes {
                return Err(HexError::InputTooLarge(maximum_bytes));
            }
            output.push((previous << 4) | value);
        } else {
            high = Some(value);
        }
    }
    if high.is_some() {
        return Err(HexError::IncompleteByte);
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum HexError {
    #[error("bytes per row must be in {MIN_BYTES_PER_ROW}..={MAX_BYTES_PER_ROW}, got {0}")]
    BytesPerRow(u8),
    #[error("nibble value must be in 0..=15, got {0}")]
    Nibble(u8),
    #[error("invalid hexadecimal digit {0:?}")]
    InvalidDigit(char),
    #[error("hexadecimal input contains an incomplete byte")]
    IncompleteByte,
    #[error("hexadecimal input exceeds the {0}-byte limit")]
    InputTooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::{parse_hex, replace_nibble, HexLayout, HexPosition};
    use nulis_document::ByteOffset;

    #[test]
    fn layout_preserves_offsets_beyond_four_gibibytes() {
        let layout = HexLayout::new(16).expect("layout");
        let start = ByteOffset(u64::from(u32::MAX) + 17);
        let aligned = layout.align_offset(start);
        let bytes = [0_u8; 20];
        let rows = layout.rows(aligned, &bytes);
        assert_eq!(rows[0].offset, ByteOffset(u64::from(u32::MAX) + 17));
        assert_eq!(rows[1].offset, ByteOffset(u64::from(u32::MAX) + 33));
        assert_eq!(rows[1].bytes.len(), 4);
        let position = HexPosition {
            offset: rows[1].offset,
            high_nibble: false,
        };
        assert_eq!(position.offset.0, u64::from(u32::MAX) + 33);
    }

    #[test]
    fn nibble_replacement_preserves_the_other_half() {
        assert_eq!(replace_nibble(0xab, true, 4).expect("high"), 0x4b);
        assert_eq!(replace_nibble(0xab, false, 4).expect("low"), 0xa4);
        assert!(replace_nibble(0, true, 16).is_err());
    }

    #[test]
    fn clipboard_parser_accepts_separators_and_enforces_limits() {
        assert_eq!(
            parse_hex("00 ff_A5\n10", 4).expect("hex"),
            [0, 255, 165, 16]
        );
        assert!(parse_hex("0", 1).is_err());
        assert!(parse_hex("zz", 1).is_err());
        assert!(parse_hex("0001", 1).is_err());
    }
}
