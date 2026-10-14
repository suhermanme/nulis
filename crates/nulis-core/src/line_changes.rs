//! Sparse editing-session line change tracking.
//!
//! The tracker stores byte ranges touched since the document was opened. Its
//! memory is bounded by [`MAX_CHANGE_RANGES`], never by file length or line count,
//! and it is not a diff against a retained baseline.

use serde::{Deserialize, Serialize};

pub const MAX_CHANGE_RANGES: usize = 4_096;

/// Marker reported for one logical line.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineChange {
    Unsaved,
    Saved,
    UnsavedDeletion,
    SavedDeletion,
}

impl LineChange {
    #[must_use]
    pub fn is_unsaved(self) -> bool {
        matches!(self, Self::Unsaved | Self::UnsavedDeletion)
    }
}

/// Whether line markers describe the current revision exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChangeStatus {
    /// False when markers could not be computed and must not be shown as clean.
    pub available: bool,
    /// True when the range budget forced neighbouring markers to be merged.
    pub approximate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeRange {
    pub start: u64,
    pub end: u64,
    pub saved: bool,
}

/// Exact inverse of one applied edit, valid only within the epoch that produced it.
#[derive(Clone, Debug)]
pub(crate) struct ChangeUndo {
    index: usize,
    produced: usize,
    removed: Vec<ChangeRange>,
    delta: i128,
}

#[derive(Clone, Debug)]
pub(crate) struct ChangeHistory {
    epoch: u64,
    steps: Vec<ChangeUndo>,
}

#[derive(Clone, Debug)]
pub(crate) struct ChangeTracker {
    ranges: Vec<ChangeRange>,
    epoch: u64,
    available: bool,
    approximate: bool,
}

impl Default for ChangeTracker {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            epoch: 0,
            available: true,
            approximate: false,
        }
    }
}

impl ChangeTracker {
    /// Rebuilds unsaved markers from a recovery checkpoint. Invalid data makes
    /// the indicator unavailable rather than showing a misleading clean gutter.
    pub fn from_unsaved(ranges: &[(u64, u64)], document_len: u64) -> Self {
        let mut tracker = Self::default();
        let mut previous_end = None;
        for (start, end) in ranges {
            let ordered = previous_end.is_none_or(|previous| previous < *start);
            if ranges.len() > MAX_CHANGE_RANGES || start > end || *end > document_len || !ordered {
                tracker.ranges.clear();
                tracker.available = false;
                return tracker;
            }
            previous_end = Some(*end);
            tracker.ranges.push(ChangeRange {
                start: *start,
                end: *end,
                saved: false,
            });
        }
        tracker
    }

    pub fn unavailable() -> Self {
        Self {
            available: false,
            ..Self::default()
        }
    }

    pub fn status(&self) -> LineChangeStatus {
        LineChangeStatus {
            available: self.available,
            approximate: self.approximate,
        }
    }

    pub fn unsaved_ranges(&self) -> Vec<(u64, u64)> {
        self.ranges
            .iter()
            .filter(|range| !range.saved)
            .map(|range| (range.start, range.end))
            .collect()
    }

    pub fn ranges(&self) -> &[ChangeRange] {
        &self.ranges
    }

    /// Marks the tracker unusable until the next successful save.
    pub fn invalidate(&mut self) {
        self.ranges.clear();
        self.available = false;
        self.epoch += 1;
    }

    /// Applies ascending pre-edit `(start, end, inserted_len)` replacements.
    pub fn apply(&mut self, ascending_edits: &[(u64, u64, usize)]) -> ChangeHistory {
        let mut steps = Vec::with_capacity(ascending_edits.len());
        if self.available {
            for (start, end, inserted_len) in ascending_edits.iter().rev() {
                steps.push(self.apply_one(*start, *end, *inserted_len as u64));
            }
            if self.ranges.len() > MAX_CHANGE_RANGES {
                self.coarsen();
                steps.clear();
            }
        }
        ChangeHistory {
            epoch: self.epoch,
            steps,
        }
    }

    /// Restores the markers that preceded `history`. Returns false when the
    /// history belongs to another epoch and the caller must fall back.
    pub fn revert(&mut self, history: &ChangeHistory) -> bool {
        if !self.available || history.epoch != self.epoch {
            return false;
        }
        for step in history.steps.iter().rev() {
            let tail = self.ranges.split_off(step.index + step.produced);
            self.ranges.truncate(step.index);
            self.ranges.extend_from_slice(&step.removed);
            self.ranges
                .extend(tail.into_iter().map(|range| ChangeRange {
                    start: shift(range.start, -step.delta),
                    end: shift(range.end, -step.delta),
                    saved: range.saved,
                }));
        }
        true
    }

    /// Converts every unsaved marker to a saved marker after a successful save.
    pub fn mark_saved(&mut self) {
        self.epoch += 1;
        self.available = true;
        let mut merged: Vec<ChangeRange> = Vec::with_capacity(self.ranges.len());
        for range in self.ranges.drain(..) {
            match merged.last_mut() {
                Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
                _ => merged.push(ChangeRange {
                    saved: true,
                    ..range
                }),
            }
        }
        self.ranges = merged;
    }

    /// Reports the marker for a line occupying `[start, end)` including its
    /// line ending. `last_line` is true for the final line when it has no
    /// ending, and `is_line_start` lets an insertion that ends exactly at this line decide
    /// whether it split the line or only added whole lines above it.
    pub fn line_change(
        &self,
        start: u64,
        end: u64,
        last_line: bool,
        is_line_start: &dyn Fn(u64) -> bool,
    ) -> Option<LineChange> {
        let first = self.ranges.partition_point(|range| range.end < start);
        // (unsaved content, unsaved deletion point, saved content, saved point)
        let mut hits = [false; 4];
        for range in &self.ranges[first..] {
            if range.start > end {
                break;
            }
            let point = range.start == range.end;
            let hit = if point {
                range.start < end || last_line
            } else if range.end == start {
                !is_line_start(range.start)
            } else {
                range.start < end
            };
            if hit {
                hits[usize::from(range.saved) * 2 + usize::from(point)] = true;
            }
        }
        match hits {
            [true, ..] => Some(LineChange::Unsaved),
            [false, true, ..] => Some(LineChange::UnsavedDeletion),
            [false, false, true, _] => Some(LineChange::Saved),
            [false, false, false, true] => Some(LineChange::SavedDeletion),
            [false, false, false, false] => None,
        }
    }

    fn apply_one(&mut self, start: u64, end: u64, inserted: u64) -> ChangeUndo {
        let delta = i128::from(inserted) - i128::from(end - start);
        let index = self.ranges.partition_point(|range| range.end < start);
        let touched = self.ranges[index..]
            .iter()
            .take_while(|range| range.start <= end)
            .count();
        let removed: Vec<ChangeRange> = self.ranges[index..index + touched].to_vec();
        let mut merged = ChangeRange {
            start,
            end: start + inserted,
            saved: false,
        };
        let mut before = None;
        let mut after = None;
        for range in &removed {
            if range.saved {
                if range.start < start {
                    before = Some(ChangeRange {
                        end: start,
                        ..*range
                    });
                }
                if range.end > end {
                    after = Some(ChangeRange {
                        start: start + inserted,
                        end: shift(range.end, delta),
                        saved: true,
                    });
                }
            } else {
                merged.start = merged.start.min(range.start);
                if range.end > end {
                    merged.end = merged.end.max(shift(range.end, delta));
                }
            }
        }
        let produced: Vec<ChangeRange> = before.into_iter().chain([merged]).chain(after).collect();
        let produced_len = produced.len();
        let tail: Vec<ChangeRange> = self
            .ranges
            .drain(index + touched..)
            .map(|range| ChangeRange {
                start: shift(range.start, delta),
                end: shift(range.end, delta),
                saved: range.saved,
            })
            .collect();
        self.ranges.truncate(index);
        self.ranges.extend(produced);
        self.ranges.extend(tail);
        ChangeUndo {
            index,
            produced: produced_len,
            removed,
            delta,
        }
    }

    fn coarsen(&mut self) {
        self.epoch += 1;
        self.approximate = true;
        let mut merged = Vec::with_capacity(self.ranges.len() / 2 + 1);
        for pair in self.ranges.chunks(2) {
            merged.push(ChangeRange {
                start: pair[0].start,
                end: pair[pair.len() - 1].end,
                saved: pair.iter().all(|range| range.saved),
            });
        }
        self.ranges = merged;
    }
}

fn shift(offset: u64, delta: i128) -> u64 {
    u64::try_from((i128::from(offset) + delta).max(0)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{ChangeTracker, LineChange, MAX_CHANGE_RANGES};

    #[test]
    fn insertions_mark_only_touched_lines_and_follow_later_edits() {
        // "one\ntwo\nthree" with "X" inserted at the start of line 3.
        let mut tracker = ChangeTracker::default();
        tracker.apply(&[(8, 8, 1)]);
        assert_eq!(tracker.line_change(0, 4, false, &|_| true), None);
        assert_eq!(tracker.line_change(4, 8, false, &|_| true), None);
        assert_eq!(
            tracker.line_change(8, 14, true, &|_| true),
            Some(LineChange::Unsaved)
        );
        // A line inserted above moves the marker with its line.
        tracker.apply(&[(0, 0, 4)]);
        assert_eq!(
            tracker.line_change(0, 4, false, &|_| true),
            Some(LineChange::Unsaved)
        );
        assert_eq!(tracker.line_change(4, 8, false, &|_| true), None);
        assert_eq!(tracker.line_change(8, 12, false, &|_| true), None);
        assert_eq!(
            tracker.line_change(12, 18, true, &|_| true),
            Some(LineChange::Unsaved)
        );
    }

    #[test]
    fn deletion_is_shown_at_the_surviving_boundary_and_at_the_document_end() {
        // "one\ntwo\nthree": delete line 2 entirely.
        let mut tracker = ChangeTracker::default();
        tracker.apply(&[(4, 8, 0)]);
        assert_eq!(tracker.line_change(0, 4, false, &|_| true), None);
        assert_eq!(
            tracker.line_change(4, 9, true, &|_| true),
            Some(LineChange::UnsavedDeletion)
        );
        // Deleting the tail leaves an end-boundary marker on the last line.
        let mut tail = ChangeTracker::default();
        tail.apply(&[(3, 13, 0)]);
        assert_eq!(
            tail.line_change(0, 3, true, &|_| true),
            Some(LineChange::UnsavedDeletion)
        );
    }

    #[test]
    fn a_split_line_marks_both_halves_but_a_whole_inserted_line_does_not_spill() {
        // "on\ne\n": Enter typed inside "one" inserted the byte at offset 2.
        let mut tracker = ChangeTracker::default();
        tracker.apply(&[(2, 2, 1)]);
        assert_eq!(
            tracker.line_change(0, 3, false, &|_| false),
            Some(LineChange::Unsaved)
        );
        assert_eq!(
            tracker.line_change(3, 5, true, &|_| false),
            Some(LineChange::Unsaved)
        );
        // "new\none\n": a whole line inserted at a line start leaves "one" clean.
        let mut whole = ChangeTracker::default();
        whole.apply(&[(0, 0, 4)]);
        assert_eq!(whole.line_change(4, 8, true, &|_| true), None);
    }

    #[test]
    fn save_converts_markers_and_a_later_edit_makes_the_line_unsaved_again() {
        let mut tracker = ChangeTracker::default();
        tracker.apply(&[(0, 0, 1), (5, 5, 1)]);
        tracker.mark_saved();
        assert_eq!(
            tracker.line_change(0, 5, false, &|_| true),
            Some(LineChange::Saved)
        );
        assert_eq!(
            tracker.line_change(5, 9, false, &|_| true),
            Some(LineChange::Saved)
        );
        assert_eq!(tracker.unsaved_ranges(), Vec::new());
        tracker.apply(&[(2, 2, 1)]);
        assert_eq!(
            tracker.line_change(0, 6, false, &|_| true),
            Some(LineChange::Unsaved)
        );
        assert_eq!(
            tracker.line_change(6, 10, false, &|_| true),
            Some(LineChange::Saved)
        );
    }

    #[test]
    fn revert_restores_the_exact_prior_markers_within_one_epoch() {
        let mut tracker = ChangeTracker::default();
        tracker.apply(&[(0, 0, 2)]);
        tracker.mark_saved();
        let before = tracker.ranges().to_vec();
        let first = tracker.apply(&[(1, 1, 3), (10, 12, 0)]);
        let middle = tracker.ranges().to_vec();
        let second = tracker.apply(&[(0, 20, 1)]);
        assert!(tracker.revert(&second));
        assert_eq!(tracker.ranges(), middle.as_slice());
        assert!(tracker.revert(&first));
        assert_eq!(tracker.ranges(), before.as_slice());
        // History recorded before a save is not exact afterwards.
        let stale = tracker.apply(&[(0, 0, 1)]);
        tracker.mark_saved();
        assert!(!tracker.revert(&stale));
    }

    #[test]
    fn range_budget_is_hard_and_reports_approximation() {
        let mut tracker = ChangeTracker::default();
        for index in (0..=MAX_CHANGE_RANGES as u64).rev() {
            tracker.apply(&[(index * 10, index * 10, 1)]);
        }
        assert!(tracker.ranges().len() <= MAX_CHANGE_RANGES);
        assert!(tracker.status().approximate);
        assert!(tracker.status().available);
    }

    #[test]
    fn invalid_recovered_ranges_make_the_indicator_unavailable() {
        assert!(
            ChangeTracker::from_unsaved(&[(0, 3), (5, 9)], 9)
                .status()
                .available
        );
        assert!(
            !ChangeTracker::from_unsaved(&[(5, 9), (0, 3)], 9)
                .status()
                .available
        );
        assert!(
            !ChangeTracker::from_unsaved(&[(0, 30)], 9)
                .status()
                .available
        );
    }
}
