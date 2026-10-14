use crate::ByteOffset;

/// A bounded UTF-8 decoding result with source-byte coordinates for every
/// displayed Unicode scalar. Invalid sequences are represented by U+FFFD while
/// retaining the original bytes in document storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf8Window {
    source_start: ByteOffset,
    source_len: usize,
    text: String,
    char_starts: Vec<usize>,
}

impl Utf8Window {
    #[must_use]
    pub fn decode(source_start: ByteOffset, bytes: &[u8]) -> Self {
        let mut text = String::new();
        let mut char_starts = Vec::new();
        let mut cursor = 0_usize;
        while cursor < bytes.len() {
            match std::str::from_utf8(&bytes[cursor..]) {
                Ok(valid) => {
                    push_valid(valid, cursor, &mut text, &mut char_starts);
                    cursor = bytes.len();
                }
                Err(error) => {
                    let valid_end = cursor + error.valid_up_to();
                    let valid = String::from_utf8_lossy(&bytes[cursor..valid_end]);
                    push_valid(&valid, cursor, &mut text, &mut char_starts);
                    char_starts.push(valid_end);
                    text.push('\u{FFFD}');
                    let invalid_len = error.error_len().unwrap_or(bytes.len() - valid_end);
                    cursor = valid_end + invalid_len;
                }
            }
        }
        Self {
            source_start,
            source_len: bytes.len(),
            text,
            char_starts,
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn char_count(&self) -> usize {
        self.char_starts.len()
    }

    /// Maps a decoded scalar index to its absolute source-byte offset. The
    /// one-past-last index maps to the end of the source window.
    #[must_use]
    pub fn byte_offset_for_char(&self, char_index: usize) -> Option<ByteOffset> {
        if char_index == self.char_starts.len() {
            let relative = u64::try_from(self.source_len).ok()?;
            return self.source_start.0.checked_add(relative).map(ByteOffset);
        }
        self.char_starts
            .get(char_index)
            .and_then(|relative| u64::try_from(*relative).ok())
            .and_then(|relative| self.source_start.0.checked_add(relative))
            .map(ByteOffset)
    }

    /// Returns the decoded scalar containing, or immediately preceding, an
    /// absolute byte offset. The end offset maps one past the last scalar.
    #[must_use]
    pub fn char_index_at_or_before(&self, byte_offset: ByteOffset) -> Option<usize> {
        let relative = usize::try_from(byte_offset.0.checked_sub(self.source_start.0)?).ok()?;
        if relative > self.source_len {
            return None;
        }
        if relative == self.source_len {
            return Some(self.char_starts.len());
        }
        Some(
            self.char_starts
                .partition_point(|start| *start <= relative)
                .saturating_sub(1),
        )
    }
}

fn push_valid(valid: &str, source_start: usize, text: &mut String, starts: &mut Vec<usize>) {
    for (offset, character) in valid.char_indices() {
        starts.push(source_start + offset);
        text.push(character);
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8Window;
    use crate::ByteOffset;

    #[test]
    fn maps_multilingual_scalar_boundaries_to_source_bytes() {
        let window = Utf8Window::decode(ByteOffset(100), "a中🙂".as_bytes());
        assert_eq!(window.text(), "a中🙂");
        assert_eq!(window.char_count(), 3);
        assert_eq!(window.byte_offset_for_char(0), Some(ByteOffset(100)));
        assert_eq!(window.byte_offset_for_char(1), Some(ByteOffset(101)));
        assert_eq!(window.byte_offset_for_char(2), Some(ByteOffset(104)));
        assert_eq!(window.byte_offset_for_char(3), Some(ByteOffset(108)));
        assert_eq!(window.char_index_at_or_before(ByteOffset(106)), Some(2));
    }

    #[test]
    fn malformed_bytes_are_visible_without_losing_source_mapping() {
        let window = Utf8Window::decode(ByteOffset(8), &[b'a', 0xFF, b'b', 0xE2, 0x82]);
        assert_eq!(window.text(), "a\u{FFFD}b\u{FFFD}");
        assert_eq!(window.char_count(), 4);
        assert_eq!(window.byte_offset_for_char(1), Some(ByteOffset(9)));
        assert_eq!(window.byte_offset_for_char(3), Some(ByteOffset(11)));
        assert_eq!(window.byte_offset_for_char(4), Some(ByteOffset(13)));
    }
}
