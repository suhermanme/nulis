//! Column tools: a number sequence down a column, the sum of selected
//! numbers, and left, center, or right justification inside a column block.
//! Each edit is one revision-checked `replace_many`, so it is one undo step.

use super::{
    decode_text, editable_ranges, encode_text, find_line_boundary, find_line_start,
    insert_rectangular_text, record_bookmark_edit, rectangular_ranges, relocate_bookmarks,
    selections_after_replacements, summary, validate_revision, DocumentId, DocumentManager,
    DocumentSummary, EditorError, EditorSession, MAX_CLIPBOARD_BYTES, MAX_EDIT_BYTES,
    MAX_RECTANGULAR_ROWS,
};
use serde::{Deserialize, Serialize};

/// How numbers of different lengths are padded.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum NumberPadding {
    #[default]
    None,
    /// Leading zeros, after any minus sign: `007`.
    Zeros,
    /// Leading spaces, so the numbers line up on the right.
    Spaces,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberSequence {
    pub start: i64,
    pub step: i64,
    #[serde(default)]
    pub padding: NumberPadding,
}

/// The total of the numbers found in the selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberSum {
    /// Exact decimal, with as many decimals as the most precise number.
    pub sum: String,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Justify {
    Left,
    Center,
    Right,
}

/// Most decimals kept when adding numbers.
const MAX_SUM_DECIMALS: u32 = 18;

impl DocumentManager {
    /// Inserts `start`, `start + step`, … : one number per row of a column
    /// block, per caret, or at the start of each line of a multi-line
    /// selection. Selected text is replaced, as typing would.
    ///
    /// # Errors
    /// Returns an error for stale or read-only documents, too many targets,
    /// numbers out of range, or storage failure.
    pub fn insert_number_sequence(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        sequence: NumberSequence,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if session.document.is_read_only() {
            return Err(EditorError::ReadOnly);
        }
        if session.rectangular_selection.is_some() {
            let rows = rectangular_ranges(session)?.len();
            let numbers = number_texts(sequence, rows)?;
            insert_rectangular_text(session, &numbers.join("\n"), false)?;
            return Ok(summary(document_id, session));
        }
        let ranges = number_targets(session)?;
        let numbers = number_texts(sequence, ranges.len())?;
        let inserted = numbers
            .iter()
            .map(|number| encode_text(session.encoding, number))
            .collect::<Result<Vec<_>, _>>()?;
        let total = inserted.iter().map(Vec::len).sum::<usize>();
        if total > MAX_EDIT_BYTES {
            return Err(EditorError::EditTooLarge(total));
        }
        let batch = ranges
            .iter()
            .zip(&inserted)
            .rev()
            .map(|((start, end), bytes)| (*start, end - start, bytes.clone()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let bookmark_edits = ranges
            .iter()
            .zip(&inserted)
            .map(|((start, end), bytes)| (*start, *end, bytes.len()))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        record_bookmark_edit(session, bookmark_edits);
        session.selections = selections_after_replacements(&ranges, &inserted)?;
        session.preferred_scalar_columns = vec![None; session.selections.len()];
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }

    /// Adds the numbers in the column block or the selected text. A number
    /// is a run of digits with an optional sign and decimal part that is not
    /// part of a word, so `v2` and `x1.5y` are skipped.
    ///
    /// # Errors
    /// Returns an error for an unknown document, a selection above
    /// [`MAX_CLIPBOARD_BYTES`], a total that does not fit, or a read failure.
    pub fn sum_numbers(&self, document_id: &DocumentId) -> Result<NumberSum, EditorError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        let ranges = if session.rectangular_selection.is_some() {
            rectangular_ranges(session)?
                .into_iter()
                .map(|range| (range.start, range.end))
                .collect()
        } else {
            editable_ranges(&session.selections)
        };
        let selected = ranges.iter().map(|(start, end)| end - start).sum::<u64>();
        if selected > MAX_CLIPBOARD_BYTES {
            return Err(EditorError::SelectionTooLarge {
                selected,
                maximum: MAX_CLIPBOARD_BYTES,
            });
        }
        let mut total = Decimal::default();
        let mut count = 0;
        for (start, end) in ranges {
            let bytes = session.document.table().read_vec(start, end - start)?;
            for number in numbers_in(&decode_text(session.encoding, &bytes).0) {
                total = total.add(number)?;
                count += 1;
            }
        }
        Ok(NumberSum {
            sum: total.to_string(),
            count,
        })
    }

    /// Aligns the text of each row inside a column block. Blank cells are
    /// left alone, and a row that ends inside the block gets no trailing
    /// spaces. The block stays selected.
    ///
    /// # Errors
    /// Returns an error when there is no column block of some width, for
    /// stale or read-only documents, or for storage failure.
    pub fn justify_columns(
        &self,
        document_id: &DocumentId,
        expected_revision: u64,
        justify: Justify,
    ) -> Result<DocumentSummary, EditorError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        validate_revision(session, expected_revision)?;
        if session.document.is_read_only() {
            return Err(EditorError::ReadOnly);
        }
        let rectangle = session
            .rectangular_selection
            .filter(|rectangle| rectangle.anchor_column != rectangle.active_column)
            .ok_or(EditorError::NeedsColumnSelection)?;
        let width = usize::try_from(rectangle.anchor_column.abs_diff(rectangle.active_column))
            .map_err(|_| EditorError::OffsetOverflow)?;
        let mut edits = Vec::new();
        let mut total = 0_usize;
        for range in rectangular_ranges(session)? {
            if range.missing_columns > 0 || range.start == range.end {
                continue;
            }
            let table = session.document.table();
            let bytes = table.read_vec(range.start, range.end - range.start)?;
            let text = decode_text(session.encoding, &bytes).0;
            // A cell that would not survive decoding is left as it is.
            if encode_text(session.encoding, &text).ok().as_deref() != Some(&bytes[..]) {
                continue;
            }
            let trimmed = text.trim_matches(' ');
            if trimmed.is_empty() {
                continue;
            }
            let cell_columns = text.chars().count();
            let continues = cell_columns == width
                && find_line_boundary(table, range.end)?.content_end > range.end;
            let free = width.saturating_sub(trimmed.chars().count());
            let before = match justify {
                Justify::Left => 0,
                Justify::Center => free / 2,
                Justify::Right => free,
            };
            let after = if continues { free - before } else { 0 };
            let justified = format!("{}{trimmed}{}", " ".repeat(before), " ".repeat(after));
            if justified == text {
                continue;
            }
            let inserted = encode_text(session.encoding, &justified)?;
            total += inserted.len();
            if total > MAX_EDIT_BYTES {
                return Err(EditorError::EditTooLarge(total));
            }
            edits.push((range.start, range.end, inserted));
        }
        if edits.is_empty() {
            return Ok(summary(document_id, session));
        }
        let batch = edits
            .iter()
            .rev()
            .map(|(start, end, bytes)| (*start, end - start, bytes.clone()))
            .collect::<Vec<_>>();
        session.document.replace_many(&batch)?;
        let bookmark_edits = edits
            .iter()
            .map(|(start, end, bytes)| (*start, *end, bytes.len()))
            .collect::<Vec<_>>();
        relocate_bookmarks(&mut session.bookmarks, &bookmark_edits)?;
        // Keep the block: its rows and columns are unchanged; only the bytes
        // that locate its first and last rows move.
        let moved = |offset: u64| map_offset(&bookmark_edits, offset);
        session.rectangular_selection = Some(super::RectangularSelection {
            anchor_byte: moved(rectangle.anchor_byte),
            active_byte: moved(rectangle.active_byte),
            ..rectangle
        });
        record_bookmark_edit(session, bookmark_edits);
        session.navigation_index.reset();
        Ok(summary(document_id, session))
    }
}

/// Zero-width targets at each line start of a single multi-line selection;
/// otherwise the selections themselves.
fn number_targets(session: &EditorSession) -> Result<Vec<(u64, u64)>, EditorError> {
    let ranges = editable_ranges(&session.selections);
    let [(start, end)] = ranges[..] else {
        return Ok(ranges);
    };
    let table = session.document.table();
    let first = find_line_start(table, start)?;
    let boundary = find_line_boundary(table, first)?;
    if boundary.next_start.is_none_or(|next| next >= end) {
        return Ok(ranges);
    }
    let mut starts = Vec::new();
    let mut line = Some(first);
    while let Some(line_start) = line.filter(|line_start| *line_start < end) {
        if starts.len() == MAX_RECTANGULAR_ROWS {
            return Err(EditorError::RectangularSelectionTooTall {
                rows: MAX_RECTANGULAR_ROWS as u64 + 1,
                maximum: MAX_RECTANGULAR_ROWS,
            });
        }
        starts.push((line_start, line_start));
        line = find_line_boundary(table, line_start)?.next_start;
    }
    Ok(starts)
}

fn number_texts(sequence: NumberSequence, count: usize) -> Result<Vec<String>, EditorError> {
    if count > MAX_RECTANGULAR_ROWS {
        return Err(EditorError::RectangularSelectionTooTall {
            rows: count as u64,
            maximum: MAX_RECTANGULAR_ROWS,
        });
    }
    let values = (0..count)
        .map(|index| {
            i64::try_from(index)
                .ok()
                .and_then(|index| sequence.step.checked_mul(index))
                .and_then(|offset| sequence.start.checked_add(offset))
                .ok_or(EditorError::NumberOutOfRange)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let digits = values
        .iter()
        .map(|value| value.unsigned_abs().to_string().len())
        .max()
        .unwrap_or(1);
    let widest = values
        .iter()
        .map(|value| value.to_string().len())
        .max()
        .unwrap_or(1);
    Ok(values
        .into_iter()
        .map(|value| match sequence.padding {
            NumberPadding::None => value.to_string(),
            NumberPadding::Zeros => format!(
                "{}{:0digits$}",
                if value < 0 { "-" } else { "" },
                value.unsigned_abs()
            ),
            NumberPadding::Spaces => format!("{value:>widest$}"),
        })
        .collect())
}

/// Where `offset` lands after ascending `(start, end, inserted_len)` edits.
fn map_offset(edits: &[(u64, u64, usize)], offset: u64) -> u64 {
    let mut delta = 0_i128;
    for (start, end, inserted) in edits {
        if offset < *start {
            break;
        }
        if offset < *end {
            return u64::try_from(i128::from(*start) + delta).unwrap_or(*start);
        }
        delta += *inserted as i128 - i128::from(end - start);
    }
    u64::try_from(i128::from(offset) + delta).unwrap_or(offset)
}

/// An exact decimal: `mantissa / 10^scale`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    fn rescaled(self, scale: u32) -> Result<i128, EditorError> {
        10_i128
            .checked_pow(scale - self.scale)
            .and_then(|factor| self.mantissa.checked_mul(factor))
            .ok_or(EditorError::NumberOutOfRange)
    }

    fn add(self, other: Self) -> Result<Self, EditorError> {
        let scale = self.scale.max(other.scale);
        let mantissa = self
            .rescaled(scale)?
            .checked_add(other.rescaled(scale)?)
            .ok_or(EditorError::NumberOutOfRange)?;
        Ok(Self { mantissa, scale })
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        let digits = format!("{digits:0>width$}", width = scale + 1);
        let (whole, fraction) = digits.split_at(digits.len() - scale);
        let sign = if self.mantissa < 0 { "-" } else { "" };
        if fraction.is_empty() {
            write!(formatter, "{sign}{whole}")
        } else {
            write!(formatter, "{sign}{whole}.{fraction}")
        }
    }
}

/// The standalone numbers in `text`.
fn numbers_in(text: &str) -> Vec<Decimal> {
    let chars = text.chars().collect::<Vec<_>>();
    let word = |character: char| character.is_alphanumeric() || character == '_';
    let mut numbers = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let signed = matches!(chars[index], '-' | '+')
            && chars.get(index + 1).is_some_and(char::is_ascii_digit);
        let starts = chars[index].is_ascii_digit() || signed;
        let glued = index > 0 && (word(chars[index - 1]) || chars[index - 1] == '.');
        if !starts || glued {
            index += 1;
            continue;
        }
        let negative = chars[index] == '-';
        let mut end = index + usize::from(signed);
        let mut mantissa = 0_i128;
        let mut overflow = false;
        while end < chars.len() && chars[end].is_ascii_digit() {
            let digit = i128::from(chars[end] as u8 - b'0');
            match mantissa
                .checked_mul(10)
                .and_then(|value| value.checked_add(digit))
            {
                Some(value) => mantissa = value,
                None => overflow = true,
            }
            end += 1;
        }
        let mut scale = 0;
        if chars.get(end) == Some(&'.') && chars.get(end + 1).is_some_and(char::is_ascii_digit) {
            end += 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                if scale < MAX_SUM_DECIMALS {
                    let digit = i128::from(chars[end] as u8 - b'0');
                    match mantissa
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(digit))
                    {
                        Some(value) => {
                            mantissa = value;
                            scale += 1;
                        }
                        None => overflow = true,
                    }
                }
                end += 1;
            }
        }
        if !overflow && !chars.get(end).copied().is_some_and(word) {
            numbers.push(Decimal {
                mantissa: if negative { -mantissa } else { mantissa },
                scale,
            });
        }
        index = end;
    }
    numbers
}

#[cfg(test)]
mod tests {
    use super::super::{DocumentId, DocumentManager, EditorError, Selection};
    use super::{numbers_in, Justify, NumberPadding, NumberSequence};

    fn open(text: &str) -> (DocumentManager, DocumentId) {
        let manager = DocumentManager::default();
        let id = manager
            .create_untitled("t".into(), text.into())
            .expect("document")
            .document_id;
        (manager, id)
    }

    fn text(manager: &DocumentManager, id: &DocumentId) -> String {
        manager.markdown_preview_source(id).expect("text").text
    }

    fn revision(manager: &DocumentManager, id: &DocumentId) -> u64 {
        manager.summary(id).expect("summary").revision
    }

    /// A column block between two columns, from the byte `starts.0` (on the
    /// first row, at its column) to `starts.1` (on the last row).
    fn block(manager: &DocumentManager, id: &DocumentId, columns: (u64, u64), starts: (u64, u64)) {
        manager
            .set_rectangular_selection(id, starts.0, starts.1, columns.0, columns.1)
            .expect("block");
    }

    fn sequence(start: i64, step: i64, padding: NumberPadding) -> NumberSequence {
        NumberSequence {
            start,
            step,
            padding,
        }
    }

    #[test]
    fn numbers_go_down_a_column_block_and_pad_short_lines() {
        let (manager, id) = open("ab\nc\ndefg\n");
        block(&manager, &id, (2, 2), (2, 7));
        manager
            .insert_number_sequence(
                &id,
                revision(&manager, &id),
                sequence(8, 1, NumberPadding::Zeros),
            )
            .expect("numbers");
        assert_eq!(text(&manager, &id), "ab08\nc 09\nde10fg\n");
        manager.undo(&id).expect("undo");
        assert_eq!(text(&manager, &id), "ab\nc\ndefg\n");
    }

    #[test]
    fn numbers_go_at_each_caret_or_each_selected_line_start() {
        let (manager, id) = open("x\ny\nz\n");
        manager
            .set_selections(&id, vec![Selection::caret(1), Selection::caret(5)])
            .expect("carets");
        manager
            .insert_number_sequence(
                &id,
                revision(&manager, &id),
                sequence(10, -5, NumberPadding::Spaces),
            )
            .expect("numbers");
        assert_eq!(text(&manager, &id), "x10\ny\nz 5\n");

        let (manager, id) = open("x\ny\nz\n");
        manager
            .set_selections(
                &id,
                vec![Selection {
                    anchor_byte: 0,
                    active_byte: 4,
                }],
            )
            .expect("selection");
        manager
            .insert_number_sequence(
                &id,
                revision(&manager, &id),
                sequence(1, 1, NumberPadding::None),
            )
            .expect("numbers");
        // The selection ends at the start of line 3, which is not included.
        assert_eq!(text(&manager, &id), "1x\n2y\nz\n");
    }

    #[test]
    fn numbers_out_of_range_are_refused() {
        let (manager, id) = open("a\nb\n");
        block(&manager, &id, (0, 0), (0, 2));
        assert_eq!(
            manager.insert_number_sequence(
                &id,
                revision(&manager, &id),
                sequence(i64::MAX, 1, NumberPadding::None)
            ),
            Err(EditorError::NumberOutOfRange)
        );
    }

    #[test]
    fn sums_standalone_numbers_exactly() {
        let parsed = numbers_in("0.1 + 0.2, -3, v2, x1.5y, 1e3, 12.")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(parsed, vec!["0.1", "0.2", "-3", "12"]);

        let (manager, id) = open("price\n1.10\n2.25\n-0.35\ntotal\n");
        manager
            .set_selections(
                &id,
                vec![Selection {
                    anchor_byte: 0,
                    active_byte: 28,
                }],
            )
            .expect("selection");
        let sum = manager.sum_numbers(&id).expect("sum");
        assert_eq!((sum.sum.as_str(), sum.count), ("3.00", 3));

        // A column block adds only the numbers inside it.
        let (manager, id) = open("a 10 99\nb 20 99\n");
        block(&manager, &id, (2, 4), (2, 12));
        let sum = manager.sum_numbers(&id).expect("sum");
        assert_eq!((sum.sum.as_str(), sum.count), ("30", 2));
    }

    #[test]
    fn justifies_text_inside_a_column_block() {
        let (manager, id) = open("|ab    |\n|  cd  |\n|e\n");
        block(&manager, &id, (1, 7), (1, 20));
        manager
            .justify_columns(&id, revision(&manager, &id), Justify::Right)
            .expect("right");
        // The short last row gets no trailing spaces.
        assert_eq!(text(&manager, &id), "|    ab|\n|    cd|\n|     e\n");
        manager
            .justify_columns(&id, revision(&manager, &id), Justify::Center)
            .expect("center");
        assert_eq!(text(&manager, &id), "|  ab  |\n|  cd  |\n|  e\n");
        manager
            .justify_columns(&id, revision(&manager, &id), Justify::Left)
            .expect("left");
        assert_eq!(text(&manager, &id), "|ab    |\n|cd    |\n|e\n");
    }

    #[test]
    fn justify_needs_a_column_block() {
        let (manager, id) = open("abc\n");
        assert_eq!(
            manager.justify_columns(&id, revision(&manager, &id), Justify::Left),
            Err(EditorError::NeedsColumnSelection)
        );
    }
}
