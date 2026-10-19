//! Bounded-memory literal and regular-expression search over random-access sources.

use nulis_document::{ByteOffset, ByteRange};
use nulis_large_file::ReadAt;
use regex::bytes::{Regex, RegexBuilder};
use regex_syntax::ParserBuilder;
use serde::{Deserialize, Serialize};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};
use thiserror::Error;

pub const SEARCH_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_PATTERN_BYTES: usize = 64 * 1024;
pub const MAX_REGEX_MATCH_BYTES: usize = 1024 * 1024;
pub const MAX_PAGE_MATCHES: usize = 10_000;
pub const MAX_REPLACEMENT_PLAN_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    #[default]
    Literal,
    Regex,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchOptions {
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    /// Matches only where the text is not part of a longer word.
    #[serde(default)]
    pub whole_word: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            mode: SearchMode::Literal,
            case_sensitive: true,
            whole_word: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub range: ByteRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchPage {
    pub matches: Vec<SearchMatch>,
    pub next_offset: Option<u64>,
    pub scanned_through: u64,
    pub complete: bool,
    pub cancelled: bool,
}

#[derive(Debug, Default)]
pub struct CancellationToken(AtomicBool);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Searches from `start_offset` without materializing the source. Results and
/// working memory are bounded by the public constants in this crate.
///
/// Literal searches operate on bytes. Case-insensitive literal matching is
/// ASCII-only, which keeps source coordinates stable for arbitrary encodings.
/// Regex searches support multiline matches when the expression has a finite
/// maximum width no larger than [`MAX_REGEX_MATCH_BYTES`].
///
/// # Errors
/// Returns a typed error for invalid bounds, patterns, regexes, or source reads.
pub fn search(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    start_offset: u64,
    max_matches: usize,
    cancellation: &CancellationToken,
) -> Result<SearchPage, SearchError> {
    validate_request(source, pattern, start_offset, max_matches)?;
    let matcher = Matcher::new(pattern, options)?;
    search_with_matcher(source, &matcher, start_offset, max_matches, cancellation)
}

/// Validates and compiles a pattern without reading a source.
///
/// # Errors
/// Returns a typed error for empty, oversized, invalid, or unbounded patterns.
pub fn validate_pattern(pattern: &[u8], options: SearchOptions) -> Result<(), SearchError> {
    if pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(SearchError::PatternTooLarge(pattern.len()));
    }
    Matcher::new(pattern, options).map(|_| ())
}

/// Builds descending piece-table replacements from ordered, non-overlapping
/// search matches. The returned vector is ready for an atomic `replace_many`.
///
/// # Errors
/// Returns an error when matches overlap, are out of order, or exceed limits.
pub fn plan_replacements(
    matches: &[SearchMatch],
    replacement: &[u8],
) -> Result<Vec<(u64, u64, Vec<u8>)>, SearchError> {
    if matches.len() > MAX_PAGE_MATCHES {
        return Err(SearchError::TooManyMatches(matches.len()));
    }
    let total_inserted = replacement
        .len()
        .checked_mul(matches.len())
        .ok_or(SearchError::ReplacementTooLarge)?;
    let total_removed = matches.iter().try_fold(0_u64, |total, found| {
        total
            .checked_add(found.range.len())
            .ok_or(SearchError::ReplacementTooLarge)
    })?;
    if total_inserted > MAX_REPLACEMENT_PLAN_BYTES
        || total_removed > MAX_REPLACEMENT_PLAN_BYTES as u64
    {
        return Err(SearchError::ReplacementTooLarge);
    }
    let mut previous_end = 0_u64;
    for found in matches {
        if found.range.start.0 < previous_end || found.range.end < found.range.start {
            return Err(SearchError::OverlappingMatches);
        }
        previous_end = found.range.end.0;
    }
    matches
        .iter()
        .rev()
        .map(|found| Ok((found.range.start.0, found.range.len(), replacement.to_vec())))
        .collect()
}

#[derive(Debug)]
enum Matcher {
    Literal {
        pattern: Vec<u8>,
        case_sensitive: bool,
    },
    Regex {
        regex: Regex,
        maximum_len: usize,
    },
}

impl Matcher {
    fn new(pattern: &[u8], options: SearchOptions) -> Result<Self, SearchError> {
        match options.mode {
            SearchMode::Literal if !options.whole_word => Ok(Self::Literal {
                pattern: pattern.to_vec(),
                case_sensitive: options.case_sensitive,
            }),
            SearchMode::Literal | SearchMode::Regex => {
                let text =
                    std::str::from_utf8(pattern).map_err(|_| SearchError::RegexPatternNotUtf8)?;
                // A whole-word search is a regular expression between word boundaries.
                let wrapped;
                let expression = if options.whole_word {
                    let inner = if options.mode == SearchMode::Literal {
                        regex::escape(text)
                    } else {
                        text.to_owned()
                    };
                    wrapped = format!(r"\b(?:{inner})\b");
                    wrapped.as_str()
                } else {
                    text
                };
                let mut parser = ParserBuilder::new();
                parser.case_insensitive(!options.case_sensitive);
                let hir = parser
                    .build()
                    .parse(expression)
                    .map_err(|error| SearchError::InvalidRegex(error.to_string()))?;
                let maximum_len = hir
                    .properties()
                    .maximum_len()
                    .ok_or(SearchError::UnboundedRegex)?;
                if maximum_len == 0 {
                    return Err(SearchError::EmptyRegexMatch);
                }
                if maximum_len > MAX_REGEX_MATCH_BYTES {
                    return Err(SearchError::RegexMatchTooLarge(maximum_len));
                }
                let regex = RegexBuilder::new(expression)
                    .case_insensitive(!options.case_sensitive)
                    .multi_line(true)
                    .build()
                    .map_err(|error| SearchError::InvalidRegex(error.to_string()))?;
                Ok(Self::Regex { regex, maximum_len })
            }
        }
    }

    fn overlap(&self) -> usize {
        match self {
            Self::Literal { pattern, .. } => pattern.len().saturating_sub(1),
            Self::Regex { maximum_len, .. } => maximum_len.saturating_sub(1),
        }
    }

    fn boundary_context(&self) -> usize {
        usize::from(matches!(self, Self::Regex { .. }))
    }

    fn matches(&self, haystack: &[u8]) -> Vec<(usize, usize)> {
        match self {
            Self::Literal {
                pattern,
                case_sensitive,
            } => literal_matches(haystack, pattern, *case_sensitive),
            Self::Regex { regex, .. } => regex
                .find_iter(haystack)
                .map(|found| (found.start(), found.end()))
                .collect(),
        }
    }
}

fn search_with_matcher(
    source: &dyn ReadAt,
    matcher: &Matcher,
    start_offset: u64,
    max_matches: usize,
    cancellation: &CancellationToken,
) -> Result<SearchPage, SearchError> {
    let source_len = source.len();
    let overlap = matcher.overlap();
    let boundary_context = matcher.boundary_context();
    let window_capacity = SEARCH_CHUNK_BYTES
        .checked_add(overlap)
        .and_then(|size| size.checked_add(boundary_context.saturating_mul(2)))
        .ok_or(SearchError::OffsetOverflow)?;
    let mut window = vec![0_u8; window_capacity];
    let mut window_start = start_offset;
    let mut found_matches = Vec::new();

    while window_start < source_len {
        if cancellation.is_cancelled() {
            return Ok(SearchPage {
                matches: found_matches,
                next_offset: Some(window_start),
                scanned_through: window_start,
                complete: false,
                cancelled: true,
            });
        }
        let read_start = window_start.saturating_sub(
            u64::try_from(boundary_context).map_err(|_| SearchError::OffsetOverflow)?,
        );
        let prefix_len =
            usize::try_from(window_start - read_start).map_err(|_| SearchError::OffsetOverflow)?;
        let remaining = source_len - read_start;
        let request_len = usize::try_from(
            remaining.min(u64::try_from(window_capacity).map_err(|_| SearchError::OffsetOverflow)?),
        )
        .map_err(|_| SearchError::OffsetOverflow)?;
        let read = source.read_at(read_start, &mut window[..request_len])?;
        if read == 0 {
            break;
        }
        let primary_len = read.saturating_sub(prefix_len).min(SEARCH_CHUNK_BYTES);
        let primary_end = prefix_len
            .checked_add(primary_len)
            .ok_or(SearchError::OffsetOverflow)?;
        for (local_start, local_end) in matcher.matches(&window[..read]) {
            if local_start < prefix_len {
                continue;
            }
            if local_start >= primary_end {
                break;
            }
            let start = read_start
                .checked_add(u64::try_from(local_start).map_err(|_| SearchError::OffsetOverflow)?)
                .ok_or(SearchError::OffsetOverflow)?;
            let end = read_start
                .checked_add(u64::try_from(local_end).map_err(|_| SearchError::OffsetOverflow)?)
                .ok_or(SearchError::OffsetOverflow)?;
            found_matches.push(SearchMatch {
                range: ByteRange::new(ByteOffset(start), ByteOffset(end))
                    .map_err(|_| SearchError::OffsetOverflow)?,
            });
            if found_matches.len() == max_matches {
                return Ok(SearchPage {
                    matches: found_matches,
                    next_offset: Some(end),
                    scanned_through: end,
                    complete: end >= source_len,
                    cancelled: false,
                });
            }
        }
        let advanced = u64::try_from(primary_len).map_err(|_| SearchError::OffsetOverflow)?;
        window_start = window_start
            .checked_add(advanced)
            .ok_or(SearchError::OffsetOverflow)?;
    }

    Ok(SearchPage {
        matches: found_matches,
        next_offset: None,
        scanned_through: source_len,
        complete: true,
        cancelled: false,
    })
}

fn literal_matches(haystack: &[u8], needle: &[u8], case_sensitive: bool) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut offset = 0_usize;
    while offset.saturating_add(needle.len()) <= haystack.len() {
        let candidate = &haystack[offset..offset + needle.len()];
        let equal = if case_sensitive {
            candidate == needle
        } else {
            candidate.eq_ignore_ascii_case(needle)
        };
        if equal {
            matches.push((offset, offset + needle.len()));
            offset += needle.len();
        } else {
            offset += 1;
        }
    }
    matches
}

fn validate_request(
    source: &dyn ReadAt,
    pattern: &[u8],
    start_offset: u64,
    max_matches: usize,
) -> Result<(), SearchError> {
    if pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(SearchError::PatternTooLarge(pattern.len()));
    }
    if start_offset > source.len() {
        return Err(SearchError::StartOutOfRange {
            start: start_offset,
            source_len: source.len(),
        });
    }
    if max_matches == 0 || max_matches > MAX_PAGE_MATCHES {
        return Err(SearchError::InvalidMatchLimit(max_matches));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("search pattern must not be empty")]
    EmptyPattern,
    #[error("search pattern is {0} bytes; maximum is {MAX_PATTERN_BYTES}")]
    PatternTooLarge(usize),
    #[error("search start {start} exceeds source length {source_len}")]
    StartOutOfRange { start: u64, source_len: u64 },
    #[error("match limit {0} must be between 1 and {MAX_PAGE_MATCHES}")]
    InvalidMatchLimit(usize),
    #[error("regular-expression patterns must be valid UTF-8")]
    RegexPatternNotUtf8,
    #[error("invalid regular expression: {0}")]
    InvalidRegex(String),
    #[error("unbounded regular expressions are not safe for streaming search; use a finite repetition bound")]
    UnboundedRegex,
    #[error("regular expression can produce an empty match")]
    EmptyRegexMatch,
    #[error("regular-expression match bound {0} exceeds the 1 MiB streaming limit")]
    RegexMatchTooLarge(usize),
    #[error("search coordinates overflowed")]
    OffsetOverflow,
    #[error("search matches overlap or are not ordered")]
    OverlappingMatches,
    #[error("replacement plan contains {0} matches; maximum is {MAX_PAGE_MATCHES}")]
    TooManyMatches(usize),
    #[error("replacement plan exceeds the 8 MiB removed- or inserted-byte limit")]
    ReplacementTooLarge,
    #[error("search source read failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{
        plan_replacements, search, CancellationToken, SearchError, SearchMode, SearchOptions,
        SEARCH_CHUNK_BYTES,
    };
    use nulis_large_file::MemorySource;

    #[test]
    fn whole_word_search_skips_parts_of_longer_words() {
        let source = MemorySource::new(b"cat catalog concat cat_ (cat)".to_vec());
        for mode in [SearchMode::Literal, SearchMode::Regex] {
            let page = search(
                &source,
                b"cat",
                SearchOptions {
                    mode,
                    case_sensitive: true,
                    whole_word: true,
                },
                0,
                10,
                &CancellationToken::default(),
            )
            .expect("search");
            let starts = page
                .matches
                .iter()
                .map(|found| found.range.start.0)
                .collect::<Vec<_>>();
            assert_eq!(starts, vec![0, 25], "{mode:?}");
        }
    }

    #[test]
    fn literal_search_finds_cross_chunk_matches_without_duplicates() {
        let mut bytes = vec![b'x'; SEARCH_CHUNK_BYTES - 2];
        bytes.extend_from_slice(b"needle and needle");
        let page = search(
            &MemorySource::new(bytes),
            b"needle",
            SearchOptions::default(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("search");
        assert_eq!(page.matches.len(), 2);
        assert_eq!(
            page.matches[0].range.start.0,
            (SEARCH_CHUNK_BYTES - 2) as u64
        );
        assert!(page.complete);
    }

    #[test]
    fn result_pages_resume_after_the_last_match() {
        let source = MemorySource::new(b"one one one".to_vec());
        let first = search(
            &source,
            b"one",
            SearchOptions::default(),
            0,
            2,
            &CancellationToken::default(),
        )
        .expect("first page");
        assert_eq!(first.matches.len(), 2);
        let second = search(
            &source,
            b"one",
            SearchOptions::default(),
            first.next_offset.expect("continuation"),
            2,
            &CancellationToken::default(),
        )
        .expect("second page");
        assert_eq!(second.matches.len(), 1);
        assert_eq!(second.matches[0].range.start.0, 8);
    }

    #[test]
    fn finite_multiline_regex_crosses_a_chunk_boundary() {
        let mut bytes = vec![b'x'; SEARCH_CHUNK_BYTES - 3];
        bytes.extend_from_slice(b"abc\ndef");
        let page = search(
            &MemorySource::new(bytes),
            br"abc\ndef",
            SearchOptions {
                mode: SearchMode::Regex,
                case_sensitive: true,
                whole_word: false,
            },
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("regex search");
        assert_eq!(page.matches.len(), 1);
        assert_eq!(page.matches[0].range.len(), 7);
    }

    #[test]
    fn regex_anchors_use_document_context_not_window_edges() {
        let options = SearchOptions {
            mode: SearchMode::Regex,
            case_sensitive: true,
            whole_word: false,
        };
        let mut false_start = vec![b'x'; SEARCH_CHUNK_BYTES];
        false_start.extend_from_slice(b"foo");
        assert_eq!(
            search(
                &MemorySource::new(false_start),
                b"^foo",
                options,
                0,
                10,
                &CancellationToken::default(),
            )
            .expect("anchored search")
            .matches
            .len(),
            0
        );

        let mut true_start = vec![b'x'; SEARCH_CHUNK_BYTES - 1];
        true_start.extend_from_slice(b"\nfoo");
        let page = search(
            &MemorySource::new(true_start),
            b"^foo",
            options,
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("line-anchored search");
        assert_eq!(page.matches[0].range.start.0, SEARCH_CHUNK_BYTES as u64);

        let mut false_end = vec![b'x'; SEARCH_CHUNK_BYTES - 3];
        false_end.extend_from_slice(b"foox");
        assert_eq!(
            search(
                &MemorySource::new(false_end),
                b"foo$",
                options,
                0,
                10,
                &CancellationToken::default(),
            )
            .expect("end-anchored search")
            .matches
            .len(),
            0
        );
    }

    #[test]
    fn unbounded_regex_is_rejected_explicitly() {
        let error = search(
            &MemorySource::new(b"abc".to_vec()),
            b"a.*c",
            SearchOptions {
                mode: SearchMode::Regex,
                case_sensitive: true,
                whole_word: false,
            },
            0,
            10,
            &CancellationToken::default(),
        )
        .expect_err("unbounded expression");
        assert!(matches!(error, SearchError::UnboundedRegex));
    }

    #[test]
    fn replacement_plan_is_descending_and_rejects_overlap() {
        let page = search(
            &MemorySource::new(b"a a".to_vec()),
            b"a",
            SearchOptions::default(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("search");
        let plan = plan_replacements(&page.matches, b"long").expect("plan");
        assert_eq!(plan[0].0, 2);
        assert_eq!(plan[1].0, 0);
    }

    #[test]
    fn cancellation_returns_a_resumable_partial_page() {
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let page = search(
            &MemorySource::new(vec![b'a'; SEARCH_CHUNK_BYTES * 2]),
            b"a",
            SearchOptions::default(),
            0,
            10,
            &cancellation,
        )
        .expect("cancelled page");
        assert!(page.cancelled);
        assert_eq!(page.next_offset, Some(0));
    }
}
