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
    /// Searches each line only between these columns.
    #[serde(default)]
    pub columns: Option<ColumnRange>,
}

/// Columns to search in each line, counted as the status bar counts them:
/// characters from 1, a tab being one character. Both ends are included;
/// a missing end means the start or the end of the line.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnRange {
    #[serde(default)]
    pub from: Option<u32>,
    #[serde(default)]
    pub to: Option<u32>,
}

impl ColumnRange {
    fn validate(self) -> Result<Option<Self>, SearchError> {
        if self.from == Some(0) || self.to == Some(0) {
            return Err(SearchError::InvalidColumns("columns start at 1".into()));
        }
        if let (Some(from), Some(to)) = (self.from, self.to) {
            if to < from {
                return Err(SearchError::InvalidColumns(format!(
                    "column {to} is before column {from}"
                )));
            }
        }
        Ok((self.from.is_some() || self.to.is_some()).then_some(self))
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            mode: SearchMode::Literal,
            case_sensitive: true,
            whole_word: false,
            columns: None,
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
/// Regex searches use the linear-time `regex` engine and support multiline
/// matches. One match is at most [`MAX_REGEX_MATCH_BYTES`]; an expression with
/// no upper bound (`\w+`, `a.*b`) is searched with that cap and reports
/// [`SearchError::RegexMatchTooLarge`] if a match would exceed it.
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
    search_until(
        source,
        pattern,
        options,
        start_offset,
        source.len(),
        max_matches,
        cancellation,
    )
}

/// Like [`search`], but only for matches that start before `stop`. Text
/// after `stop` is still read as far as a match may reach, so results are
/// the same as a full search's; this is for the matches in a visible range.
///
/// # Errors
/// Returns a typed error for invalid bounds, patterns, regexes, or source reads.
pub fn search_until(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    start_offset: u64,
    stop: u64,
    max_matches: usize,
    cancellation: &CancellationToken,
) -> Result<SearchPage, SearchError> {
    validate_request(source, pattern, start_offset, max_matches)?;
    let matcher = Matcher::new(pattern, options)?;
    let mut window = Vec::new();
    let range = Range {
        start: start_offset,
        stop: stop.min(source.len()),
    };
    match options
        .columns
        .map(ColumnRange::validate)
        .transpose()?
        .flatten()
    {
        Some(columns) => {
            search_columns(source, &matcher, columns, range, max_matches, cancellation)
        }
        None => search_with_matcher(
            source,
            &matcher,
            range,
            max_matches,
            cancellation,
            &mut window,
        ),
    }
}

/// How far a count has got. Counting a large document goes slice by slice
/// (see [`count_slice`]); this carries everything one slice needs from the
/// one before, so matches and lines are counted exactly once.
#[derive(Clone, Copy, Debug)]
pub struct CountProgress {
    /// Matches counted so far.
    pub matches: u64,
    /// Distinct lines holding at least one match (where a match starts).
    pub lines: u64,
    /// Where the next slice starts.
    pub next: u64,
    /// The whole range has been counted.
    pub done: bool,
    /// Where the next match may start: after the end of the last one.
    resume: u64,
    line_state: LineState,
}

impl CountProgress {
    /// A count that starts at `start`.
    #[must_use]
    pub fn starting_at(start: u64) -> Self {
        Self {
            matches: 0,
            lines: 0,
            next: start,
            done: false,
            resume: start,
            line_state: LineState::at(start),
        }
    }
}

/// Counts the matches that start between `progress.next` and `stop` (or the
/// end of `source`), adding to `progress`. Call again with a later `stop`
/// until `progress.done`. Memory stays bounded however many matches there are.
///
/// # Errors
/// Returns a typed error for an invalid pattern or column range, a match
/// above the size cap, or a source read failure.
pub fn count_slice(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    progress: &mut CountProgress,
    stop: u64,
    cancellation: &CancellationToken,
) -> Result<(), SearchError> {
    if pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(SearchError::PatternTooLarge(pattern.len()));
    }
    let matcher = Matcher::new(pattern, options)?;
    let stop = stop.min(source.len());
    let mut window = Vec::new();
    if let Some(columns) = options
        .columns
        .map(ColumnRange::validate)
        .transpose()?
        .flatten()
    {
        // Each line's columns are searched on their own; a line counts once.
        let mut blocks = Blocks::new(source);
        let mut line = line_start(source, progress.next)?;
        while line < stop {
            if cancellation.is_cancelled() {
                progress.next = line;
                return Ok(());
            }
            let segment = scan_line(&mut blocks, line, columns)?;
            if segment.end > segment.start {
                let slice = Slice {
                    source,
                    start: segment.start,
                    end: segment.end,
                };
                let mut found = 0_u64;
                let mut lines = LineState::at(0);
                scan(
                    &slice,
                    &matcher,
                    0,
                    0,
                    slice.len(),
                    &mut lines,
                    cancellation,
                    &mut window,
                    &mut |_, _, _| {
                        found += 1;
                        true
                    },
                )?;
                progress.matches += found;
                progress.lines += u64::from(found > 0);
            }
            match segment.next_line {
                Some(next) if next < source.len() => line = next,
                _ => {
                    progress.next = source.len();
                    progress.done = true;
                    return Ok(());
                }
            }
        }
        progress.next = line;
        progress.done = line >= source.len();
        return Ok(());
    }
    let (mut counted, mut new_lines) = (0_u64, 0_u64);
    let mut resume = progress.resume;
    let end = scan(
        source,
        &matcher,
        progress.next,
        progress.resume,
        stop,
        &mut progress.line_state,
        cancellation,
        &mut window,
        &mut |start, end, new_line| {
            counted += 1;
            new_lines += u64::from(new_line);
            resume = end.max(start + 1);
            true
        },
    )?;
    progress.matches += counted;
    progress.lines += new_lines;
    progress.resume = resume.max(progress.resume);
    match end {
        ScanEnd::Cancelled(at) => progress.next = at,
        ScanEnd::Finished | ScanEnd::Stopped(_) => {
            progress.next = stop;
            progress.done = stop >= source.len();
        }
    }
    Ok(())
}

/// Where a search starts, and the offset matches must start before.
#[derive(Clone, Copy, Debug)]
struct Range {
    start: u64,
    stop: u64,
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
    if let Some(columns) = options.columns {
        columns.validate()?;
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

/// One piece of a replacement template.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Piece {
    Text(Vec<u8>),
    Group(usize),
    Named(String),
}

/// A replacement as written by the user. With regular expressions it may
/// refer to the match: `$1`, `${name}`, `$0` or `$&` (the whole match), `$$`
/// (a dollar), `\1`…`\9`, and the escapes `\n`, `\t`, `\r`, `\\`. Literal
/// searches insert the text exactly as written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplacementTemplate {
    pieces: Vec<Piece>,
}

impl ReplacementTemplate {
    /// Reads `replacement` the way `options` asks for.
    #[must_use]
    pub fn parse(replacement: &[u8], options: SearchOptions) -> Self {
        if options.mode == SearchMode::Literal {
            return Self {
                pieces: vec![Piece::Text(replacement.to_vec())],
            };
        }
        let mut pieces = Vec::new();
        let mut text = Vec::new();
        let mut index = 0;
        let flush = |text: &mut Vec<u8>, pieces: &mut Vec<Piece>| {
            if !text.is_empty() {
                pieces.push(Piece::Text(std::mem::take(text)));
            }
        };
        while index < replacement.len() {
            let byte = replacement[index];
            let next = replacement.get(index + 1).copied();
            match (byte, next) {
                (b'$', Some(b'$')) => {
                    text.push(b'$');
                    index += 2;
                }
                (b'$', Some(b'&')) => {
                    flush(&mut text, &mut pieces);
                    pieces.push(Piece::Group(0));
                    index += 2;
                }
                (b'$', Some(digit)) if digit.is_ascii_digit() => {
                    let digits = replacement[index + 1..]
                        .iter()
                        .take_while(|byte| byte.is_ascii_digit())
                        .count()
                        .min(3);
                    let number = std::str::from_utf8(&replacement[index + 1..index + 1 + digits])
                        .ok()
                        .and_then(|digits| digits.parse().ok())
                        .unwrap_or(usize::MAX);
                    flush(&mut text, &mut pieces);
                    pieces.push(Piece::Group(number));
                    index += 1 + digits;
                }
                (b'$', Some(b'{')) => {
                    let close = replacement[index + 2..]
                        .iter()
                        .position(|byte| *byte == b'}');
                    let name = close.and_then(|close| {
                        std::str::from_utf8(&replacement[index + 2..index + 2 + close]).ok()
                    });
                    match (close, name) {
                        (Some(close), Some(name)) if !name.is_empty() => {
                            flush(&mut text, &mut pieces);
                            pieces.push(
                                name.parse()
                                    .map_or_else(|_| Piece::Named(name.to_owned()), Piece::Group),
                            );
                            index += 3 + close;
                        }
                        _ => {
                            text.push(b'$');
                            index += 1;
                        }
                    }
                }
                (b'\\', Some(digit)) if digit.is_ascii_digit() => {
                    flush(&mut text, &mut pieces);
                    pieces.push(Piece::Group(usize::from(digit - b'0')));
                    index += 2;
                }
                (b'\\', Some(escape @ (b'n' | b't' | b'r' | b'\\' | b'$'))) => {
                    text.push(match escape {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        other => other,
                    });
                    index += 2;
                }
                _ => {
                    text.push(byte);
                    index += 1;
                }
            }
        }
        flush(&mut text, &mut pieces);
        Self { pieces }
    }

    /// The template inserts the same text for every match.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.pieces
            .iter()
            .all(|piece| matches!(piece, Piece::Text(_)))
    }

    /// Checks every group the template refers to exists in `regex`.
    fn check(&self, regex: &Regex) -> Result<(), SearchError> {
        for piece in &self.pieces {
            match piece {
                Piece::Group(number) if *number >= regex.captures_len() => {
                    return Err(SearchError::UnknownGroup(if *number == usize::MAX {
                        "a very large number".into()
                    } else {
                        number.to_string()
                    }));
                }
                Piece::Named(name) if !regex.capture_names().flatten().any(|item| item == name) => {
                    return Err(SearchError::UnknownGroup(format!("\"{name}\"")));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn expand(&self, captures: Option<&regex::bytes::Captures<'_>>, out: &mut Vec<u8>) {
        for piece in &self.pieces {
            let group = match piece {
                Piece::Text(text) => {
                    out.extend_from_slice(text);
                    continue;
                }
                Piece::Group(number) => captures.and_then(|captures| captures.get(*number)),
                Piece::Named(name) => captures.and_then(|captures| captures.name(name)),
            };
            if let Some(group) = group {
                out.extend_from_slice(group.as_bytes());
            }
        }
    }
}

/// The text that replaces each of `matches` (found with `pattern` and
/// `options` in `source`), with group references filled in per match.
///
/// # Errors
/// Returns an error for an invalid pattern, a reference to a group the
/// pattern does not have, a match that is no longer there, too many matches,
/// or more than [`MAX_REPLACEMENT_PLAN_BYTES`] of replacement text.
pub fn expand_replacements(
    source: &dyn ReadAt,
    pattern: &[u8],
    options: SearchOptions,
    matches: &[SearchMatch],
    replacement: &[u8],
) -> Result<Vec<Vec<u8>>, SearchError> {
    if matches.len() > MAX_PAGE_MATCHES {
        return Err(SearchError::TooManyMatches(matches.len()));
    }
    let template = ReplacementTemplate::parse(replacement, options);
    let compiled = Matcher::new(pattern, options)?;
    let regex = match &compiled {
        Matcher::Regex { regex, .. } => Some(regex),
        Matcher::Literal { .. } => None,
    };
    if let Some(regex) = regex {
        template.check(regex)?;
    }
    let mut total = 0_usize;
    let mut buffer = Vec::new();
    let mut results = Vec::with_capacity(matches.len());
    for found in matches {
        let mut text = Vec::new();
        match regex {
            Some(regex) if !template.is_constant() => {
                // One byte either side keeps anchors and word boundaries as in
                // the search; a column search sees only its columns.
                let (low, high) = match options
                    .columns
                    .and_then(|range| range.validate().ok().flatten())
                {
                    Some(range) => {
                        let segment = segment_at(source, found.range.start.0, range)?;
                        (segment.start, segment.end)
                    }
                    None => (0, source.len()),
                };
                let read_start = found.range.start.0.saturating_sub(1).max(low);
                let read_end = found.range.end.0.saturating_add(1).min(high);
                let len = usize::try_from(read_end - read_start)
                    .map_err(|_| SearchError::OffsetOverflow)?;
                buffer.resize(len, 0);
                let read = source.read_at(read_start, &mut buffer)?;
                let local = usize::try_from(found.range.start.0 - read_start)
                    .map_err(|_| SearchError::OffsetOverflow)?;
                let expected =
                    usize::try_from(found.range.len()).map_err(|_| SearchError::OffsetOverflow)?;
                let captures = regex
                    .captures_at(&buffer[..read], local)
                    .filter(|captures| {
                        captures
                            .get(0)
                            .is_some_and(|whole| whole.start() == local && whole.len() == expected)
                    })
                    .ok_or(SearchError::MatchChanged)?;
                template.expand(Some(&captures), &mut text);
            }
            _ => template.expand(None, &mut text),
        }
        total = total
            .checked_add(text.len())
            .ok_or(SearchError::ReplacementTooLarge)?;
        if total > MAX_REPLACEMENT_PLAN_BYTES {
            return Err(SearchError::ReplacementTooLarge);
        }
        results.push(text);
    }
    Ok(results)
}

/// Like [`plan_replacements`], with its own replacement text for each match.
///
/// # Errors
/// Returns an error when matches overlap, are out of order, do not pair up
/// with the texts, or exceed limits.
pub fn plan_replacements_each(
    matches: &[SearchMatch],
    replacements: &[Vec<u8>],
) -> Result<Vec<(u64, u64, Vec<u8>)>, SearchError> {
    if matches.len() != replacements.len() {
        return Err(SearchError::OverlappingMatches);
    }
    let mut plan = plan_replacements(matches, b"")?;
    let inserted = replacements
        .iter()
        .map(Vec::len)
        .try_fold(0_usize, usize::checked_add);
    if inserted.is_none_or(|total| total > MAX_REPLACEMENT_PLAN_BYTES) {
        return Err(SearchError::ReplacementTooLarge);
    }
    // The plan is in descending order; the texts are in match order.
    for (step, text) in plan.iter_mut().zip(replacements.iter().rev()) {
        step.2.clone_from(text);
    }
    Ok(plan)
}

#[derive(Debug)]
enum Matcher {
    /// Exact bytes; letters may match either ASCII case. Searched with the
    /// regex engine's literal finder, which is many times faster than a
    /// byte-by-byte comparison on large files.
    Literal { pattern: Vec<u8>, finder: Regex },
    Regex {
        regex: Regex,
        maximum_len: usize,
        /// The expression has no upper bound; `maximum_len` is the cap.
        unbounded: bool,
    },
}

impl Matcher {
    fn new(pattern: &[u8], options: SearchOptions) -> Result<Self, SearchError> {
        match options.mode {
            SearchMode::Literal if !options.whole_word => Ok(Self::Literal {
                pattern: pattern.to_vec(),
                finder: literal_finder(pattern, options.case_sensitive)?,
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
                let prefix = if options.whole_word {
                    r"\b(?:".len()
                } else {
                    0
                };
                let hir = parser.build().parse(expression).map_err(|error| {
                    SearchError::InvalidRegex(readable_regex_error(&error, text, prefix))
                })?;
                let bound = hir.properties().maximum_len();
                let unbounded = bound.is_none();
                let maximum_len = bound.unwrap_or(MAX_REGEX_MATCH_BYTES);
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
                    .map_err(|error| SearchError::InvalidRegex(last_line(&error.to_string())))?;
                Ok(Self::Regex {
                    regex,
                    maximum_len,
                    unbounded,
                })
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

    /// How much a chunk scans, and how much is read with it. A long overlap
    /// is re-read for every chunk, so chunks grow with it to keep that cost
    /// near a quarter of the scan.
    fn window_sizes(&self) -> Result<(usize, usize), SearchError> {
        let overlap = self.overlap();
        let chunk = SEARCH_CHUNK_BYTES.max(overlap.saturating_mul(4));
        let capacity = chunk
            .checked_add(overlap)
            .and_then(|size| size.checked_add(self.boundary_context().saturating_mul(2)))
            .ok_or(SearchError::OffsetOverflow)?;
        Ok((chunk, capacity))
    }

    /// An unbounded match may be no longer than the cap; one that runs into
    /// the end of the window (`cut`) may continue past it.
    fn check_length(&self, len: usize, cut: bool) -> Result<(), SearchError> {
        let unbounded = matches!(
            self,
            Self::Regex {
                unbounded: true,
                ..
            }
        );
        if unbounded && (len > MAX_REGEX_MATCH_BYTES || cut) {
            return Err(SearchError::RegexMatchTooLarge(MAX_REGEX_MATCH_BYTES));
        }
        Ok(())
    }

    /// The first match in `haystack` that starts at or after `from`. Text
    /// before `from` still counts as context for anchors and word boundaries.
    fn find_at(&self, haystack: &[u8], from: usize) -> Option<(usize, usize)> {
        match self {
            Self::Literal { finder, .. } => finder
                .find_at(haystack, from)
                .map(|found| (found.start(), found.end())),
            Self::Regex { regex, .. } => regex
                .find_at(haystack, from)
                .map(|found| (found.start(), found.end())),
        }
    }
}

/// Where line counting stands while matches are visited: whether a line
/// break has been seen since the start of the last visited match.
#[derive(Clone, Copy, Debug)]
struct LineState {
    /// Start of the last visited match, or the scan start before any.
    scan_from: u64,
    seen_break: bool,
    first: bool,
}

impl LineState {
    fn at(start: u64) -> Self {
        Self {
            scan_from: start,
            seen_break: false,
            first: true,
        }
    }
}

/// How a scan ended: at the stop or the end of the source, at the match
/// where the visitor stopped it (its end), or cancelled at an offset.
enum ScanEnd {
    Finished,
    Stopped(u64),
    Cancelled(u64),
}

/// Visits every match that starts in `window_start..stop` (none starting
/// before `resume_at`), telling the visitor whether the match begins a new
/// line, that is, a line break lies between it and the previous match (the
/// first match always does). The visitor returns `false` to stop.
#[allow(clippy::too_many_arguments)] // One scan's position, state, and output.
fn scan(
    source: &dyn ReadAt,
    matcher: &Matcher,
    mut window_start: u64,
    mut resume_at: u64,
    stop: u64,
    lines: &mut LineState,
    cancellation: &CancellationToken,
    window: &mut Vec<u8>,
    visit: &mut dyn FnMut(u64, u64, bool) -> bool,
) -> Result<ScanEnd, SearchError> {
    let overflow = |_| SearchError::OffsetOverflow;
    let source_len = source.len();
    let boundary_context = matcher.boundary_context();
    let (chunk, window_capacity) = matcher.window_sizes()?;
    if window.len() < window_capacity {
        window.resize(window_capacity, 0);
    }
    while window_start < source_len.min(stop) {
        if cancellation.is_cancelled() {
            return Ok(ScanEnd::Cancelled(window_start));
        }
        let read_start =
            window_start.saturating_sub(u64::try_from(boundary_context).map_err(overflow)?);
        let prefix_len = usize::try_from(window_start - read_start).map_err(overflow)?;
        let remaining = source_len - read_start;
        let request_len =
            usize::try_from(remaining.min(u64::try_from(window_capacity).map_err(overflow)?))
                .map_err(overflow)?;
        let read = source.read_at(read_start, &mut window[..request_len])?;
        if read == 0 {
            break;
        }
        let primary_len = read
            .saturating_sub(prefix_len)
            .min(chunk)
            .min(usize::try_from(stop - window_start).unwrap_or(usize::MAX));
        let primary_end = prefix_len
            .checked_add(primary_len)
            .ok_or(SearchError::OffsetOverflow)?;
        let window_end = read_start
            .checked_add(u64::try_from(read).map_err(overflow)?)
            .ok_or(SearchError::OffsetOverflow)?;
        let haystack = &window[..read];
        // Line breaks are looked for from here on in this window.
        let local =
            |offset: u64| usize::try_from(offset.saturating_sub(read_start)).unwrap_or(usize::MAX);
        let mut break_from = local(lines.scan_from).max(prefix_len);
        let mut at = local(resume_at).max(prefix_len);
        while at <= read {
            let Some((local_start, local_end)) = matcher.find_at(haystack, at) else {
                break;
            };
            if local_start >= primary_end {
                break;
            }
            matcher.check_length(
                local_end - local_start,
                local_end == read && window_end < source_len,
            )?;
            at = if local_end == local_start {
                local_end + 1
            } else {
                local_end
            };
            let start = read_start
                .checked_add(u64::try_from(local_start).map_err(overflow)?)
                .ok_or(SearchError::OffsetOverflow)?;
            let end = read_start
                .checked_add(u64::try_from(local_end).map_err(overflow)?)
                .ok_or(SearchError::OffsetOverflow)?;
            let gap = haystack
                .get(break_from.min(local_start)..local_start)
                .unwrap_or_default();
            let new_line = lines.first || lines.seen_break || gap.contains(&b'\n');
            *lines = LineState {
                scan_from: start,
                seen_break: false,
                first: false,
            };
            break_from = local_start;
            resume_at = end.max(start + 1);
            if !visit(start, end, new_line) {
                return Ok(ScanEnd::Stopped(end));
            }
        }
        let rest = haystack
            .get(break_from.min(primary_end)..primary_end)
            .unwrap_or_default();
        lines.seen_break |= rest.contains(&b'\n');
        window_start = window_start
            .checked_add(u64::try_from(primary_len).map_err(overflow)?)
            .ok_or(SearchError::OffsetOverflow)?;
    }
    Ok(ScanEnd::Finished)
}

fn search_with_matcher(
    source: &dyn ReadAt,
    matcher: &Matcher,
    range: Range,
    max_matches: usize,
    cancellation: &CancellationToken,
    window: &mut Vec<u8>,
) -> Result<SearchPage, SearchError> {
    let source_len = source.len();
    let mut found_matches = Vec::new();
    let mut lines = LineState::at(range.start);
    let mut failure = None;
    let end = scan(
        source,
        matcher,
        range.start,
        range.start,
        range.stop,
        &mut lines,
        cancellation,
        window,
        &mut |start, end, _| {
            if let Ok(range) = ByteRange::new(ByteOffset(start), ByteOffset(end)) {
                found_matches.push(SearchMatch { range });
                found_matches.len() < max_matches
            } else {
                failure = Some(SearchError::OffsetOverflow);
                false
            }
        },
    )?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(match end {
        ScanEnd::Stopped(end) => SearchPage {
            matches: found_matches,
            next_offset: Some(end),
            scanned_through: end,
            complete: end >= source_len,
            cancelled: false,
        },
        ScanEnd::Cancelled(at) => SearchPage {
            matches: found_matches,
            next_offset: Some(at),
            scanned_through: at,
            complete: false,
            cancelled: true,
        },
        ScanEnd::Finished => SearchPage {
            matches: found_matches,
            next_offset: None,
            scanned_through: source_len,
            complete: true,
            cancelled: false,
        },
    })
}

/// A part of a source, seen as a source of its own: matching inside it
/// treats its edges as the start and end of the text.
struct Slice<'a> {
    source: &'a dyn ReadAt,
    start: u64,
    end: u64,
}

impl ReadAt for Slice<'_> {
    fn len(&self) -> u64 {
        self.end - self.start
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let available = self.len().saturating_sub(offset);
        let wanted = usize::try_from(available).map_or(buffer.len(), |left| left.min(buffer.len()));
        let offset = self
            .start
            .checked_add(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "offset overflow"))?;
        self.source.read_at(offset, &mut buffer[..wanted])
    }
}

/// Reads a source forward in blocks, for scanning lines.
struct Blocks<'a> {
    source: &'a dyn ReadAt,
    start: u64,
    data: Vec<u8>,
}

impl<'a> Blocks<'a> {
    fn new(source: &'a dyn ReadAt) -> Self {
        Self {
            source,
            start: 0,
            data: Vec::new(),
        }
    }

    /// The bytes from `offset` to the end of its block; empty at the end.
    fn at(&mut self, offset: u64) -> io::Result<&[u8]> {
        let end = self.start + self.data.len() as u64;
        if offset < self.start || offset >= end {
            self.data.resize(SEARCH_CHUNK_BYTES, 0);
            let read = self.source.read_at(offset, &mut self.data)?;
            self.data.truncate(read);
            self.start = offset;
        }
        let local = usize::try_from(offset - self.start).unwrap_or(usize::MAX);
        Ok(self.data.get(local..).unwrap_or_default())
    }

    fn byte(&mut self, offset: u64) -> io::Result<Option<u8>> {
        Ok(self.at(offset)?.first().copied())
    }
}

/// Where the line holding `offset` starts: just after the previous line
/// break (`\n`, `\r`, or `\r\n`).
fn line_start(source: &dyn ReadAt, offset: u64) -> io::Result<u64> {
    let mut end = offset.min(source.len());
    let mut buffer = vec![0_u8; SEARCH_CHUNK_BYTES];
    while end > 0 {
        let start = end.saturating_sub(SEARCH_CHUNK_BYTES as u64);
        let len = usize::try_from(end - start).unwrap_or(SEARCH_CHUNK_BYTES);
        let read = source.read_at(start, &mut buffer[..len])?;
        if let Some(position) = buffer[..read]
            .iter()
            .rposition(|byte| matches!(byte, b'\n' | b'\r'))
        {
            return Ok(start + position as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}

/// One line's searchable part: the bytes of the chosen columns, and where
/// the next line starts (`None` after the last line).
struct LineSegment {
    start: u64,
    end: u64,
    next_line: Option<u64>,
}

fn scan_line(
    blocks: &mut Blocks<'_>,
    line_start: u64,
    columns: ColumnRange,
) -> io::Result<LineSegment> {
    // Characters before the range, and the character the range stops before.
    let skip = u64::from(columns.from.unwrap_or(1) - 1);
    let stop = columns.to.map(u64::from);
    let mut characters = 0_u64;
    let mut start = None;
    let mut end = None;
    let mut position = line_start;
    let (line_end, next_line) = loop {
        let bytes = blocks.at(position)?;
        if bytes.is_empty() {
            break (position, None);
        }
        let mut line_break = None;
        for (index, &byte) in bytes.iter().enumerate() {
            let here = position + index as u64;
            if byte == b'\n' || byte == b'\r' {
                line_break = Some((here, byte));
                break;
            }
            // Count characters by their first byte; a stray byte counts as one.
            if byte & 0xC0 != 0x80 {
                if start.is_none() && characters == skip {
                    start = Some(here);
                }
                if end.is_none() && stop == Some(characters) {
                    end = Some(here);
                }
                characters += 1;
            }
        }
        if let Some((here, byte)) = line_break {
            let after = if byte == b'\r' && blocks.byte(here + 1)? == Some(b'\n') {
                here + 2
            } else {
                here + 1
            };
            break (here, Some(after));
        }
        position += bytes.len() as u64;
    };
    let start = start.unwrap_or(line_end);
    let end = end.unwrap_or(line_end).max(start);
    Ok(LineSegment {
        start,
        end,
        next_line,
    })
}

/// The searchable part of the line holding `offset`.
fn segment_at(source: &dyn ReadAt, offset: u64, columns: ColumnRange) -> io::Result<LineSegment> {
    let mut blocks = Blocks::new(source);
    scan_line(&mut blocks, line_start(source, offset)?, columns)
}

/// Searches each line only between the chosen columns. Matches stay inside
/// one line, and `^` and `$` match at the edges of the columns.
fn search_columns(
    source: &dyn ReadAt,
    matcher: &Matcher,
    columns: ColumnRange,
    range: Range,
    max_matches: usize,
    cancellation: &CancellationToken,
) -> Result<SearchPage, SearchError> {
    let Range {
        start: start_offset,
        stop,
    } = range;
    let mut blocks = Blocks::new(source);
    let mut window = Vec::new();
    let mut found_matches = Vec::new();
    let mut line = line_start(source, start_offset)?;
    loop {
        if cancellation.is_cancelled() {
            let resume = line.max(start_offset);
            return Ok(SearchPage {
                matches: found_matches,
                next_offset: Some(resume),
                scanned_through: resume,
                complete: false,
                cancelled: true,
            });
        }
        if line >= stop {
            break;
        }
        let segment = scan_line(&mut blocks, line, columns)?;
        if segment.end > segment.start && segment.end > start_offset && segment.start < stop {
            let slice = Slice {
                source,
                start: segment.start,
                end: segment.end,
            };
            let page = search_with_matcher(
                &slice,
                matcher,
                Range {
                    start: start_offset.saturating_sub(segment.start),
                    stop: stop - segment.start,
                },
                max_matches - found_matches.len(),
                cancellation,
                &mut window,
            )?;
            for found in page.matches {
                let start = found.range.start.0 + segment.start;
                let end = found.range.end.0 + segment.start;
                found_matches.push(SearchMatch {
                    range: ByteRange::new(ByteOffset(start), ByteOffset(end))
                        .map_err(|_| SearchError::OffsetOverflow)?,
                });
            }
            if found_matches.len() == max_matches {
                let end = found_matches
                    .last()
                    .map_or(segment.end, |found| found.range.end.0);
                return Ok(SearchPage {
                    matches: found_matches,
                    next_offset: Some(end),
                    scanned_through: end,
                    complete: end >= source.len(),
                    cancelled: false,
                });
            }
            if page.cancelled {
                return Ok(SearchPage {
                    matches: found_matches,
                    next_offset: Some(segment.start),
                    scanned_through: segment.start,
                    complete: false,
                    cancelled: true,
                });
            }
        }
        // After the last line break there is nothing left to search.
        match segment.next_line {
            Some(next) if next < source.len() => line = next,
            _ => break,
        }
    }
    Ok(SearchPage {
        matches: found_matches,
        next_offset: None,
        scanned_through: source.len(),
        complete: true,
        cancelled: false,
    })
}

/// The last line of a multi-line library message, without its "error:" label.
fn last_line(message: &str) -> String {
    let line = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(message);
    line.trim().trim_start_matches("error:").trim().to_owned()
}

/// One sentence for a pattern the parser refused, naming where in `pattern`
/// (whose regex was written after `prefix` bytes of wrapping) it went wrong.
fn readable_regex_error(error: &regex_syntax::Error, pattern: &str, prefix: usize) -> String {
    use regex_syntax::ast::ErrorKind;
    let (message, offset) = match error {
        regex_syntax::Error::Parse(error) => {
            let message = match error.kind() {
                ErrorKind::UnsupportedLookAround => {
                    "lookahead and lookbehind ((?=…), (?!…), (?<=…), (?<!…)) are not supported".to_owned()
                }
                ErrorKind::UnsupportedBackreference => {
                    "backreferences like \\1 are not supported in the pattern; use $1 in the replacement".to_owned()
                }
                kind => kind.to_string(),
            };
            (message, Some(error.span().start.offset))
        }
        regex_syntax::Error::Translate(error) => {
            (error.kind().to_string(), Some(error.span().start.offset))
        }
        other => (last_line(&other.to_string()), None),
    };
    match offset
        .and_then(|offset| offset.checked_sub(prefix))
        .filter(|offset| *offset <= pattern.len())
    {
        Some(offset) => {
            let column = pattern
                .get(..offset)
                .map_or(offset, |head| head.chars().count())
                + 1;
            format!("{message} (at character {column})")
        }
        None => message,
    }
}

/// A byte-exact pattern for the regex engine: every byte written as
/// `\xHH` with Unicode off, so any encoding works, and case folding (when
/// asked) is ASCII-only, matching the plain-text search's rules.
fn literal_finder(pattern: &[u8], case_sensitive: bool) -> Result<Regex, SearchError> {
    use std::fmt::Write as _;
    let mut expression = String::with_capacity(pattern.len() * 4);
    for byte in pattern {
        let _ = write!(expression, "\\x{byte:02X}");
    }
    RegexBuilder::new(&expression)
        .unicode(false)
        .case_insensitive(!case_sensitive)
        .size_limit(64 * 1024 * 1024)
        .build()
        .map_err(|error| SearchError::InvalidRegex(last_line(&error.to_string())))
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
    #[error("regular expression can produce an empty match")]
    EmptyRegexMatch,
    #[error("a match would be longer than {0} bytes (1 MiB), the most one match may span")]
    RegexMatchTooLarge(usize),
    #[error("search coordinates overflowed")]
    OffsetOverflow,
    #[error("search matches overlap or are not ordered")]
    OverlappingMatches,
    #[error("replacement plan contains {0} matches; maximum is {MAX_PAGE_MATCHES}")]
    TooManyMatches(usize),
    #[error("replacement plan exceeds the 8 MiB removed- or inserted-byte limit")]
    ReplacementTooLarge,
    #[error("the replacement refers to group {0}, which the pattern does not have")]
    UnknownGroup(String),
    #[error("the text changed since it was found; search again")]
    MatchChanged,
    #[error("invalid column range: {0}")]
    InvalidColumns(String),
    #[error("search source read failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{
        count_slice, expand_replacements, plan_replacements, plan_replacements_each, search,
        search_until, CancellationToken, ColumnRange, CountProgress, SearchError, SearchMode,
        SearchOptions, MAX_PAGE_MATCHES, MAX_REGEX_MATCH_BYTES, SEARCH_CHUNK_BYTES,
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
                    columns: None,
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
                columns: None,
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
            columns: None,
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

    fn regex() -> SearchOptions {
        SearchOptions {
            mode: SearchMode::Regex,
            case_sensitive: true,
            whole_word: false,
            columns: None,
        }
    }

    fn ranges(source: &[u8], pattern: &[u8]) -> Vec<(u64, u64)> {
        search(
            &MemorySource::new(source.to_vec()),
            pattern,
            regex(),
            0,
            MAX_PAGE_MATCHES,
            &CancellationToken::default(),
        )
        .expect("search")
        .matches
        .iter()
        .map(|found| (found.range.start.0, found.range.end.0))
        .collect()
    }

    #[test]
    fn unbounded_regex_finds_whole_matches() {
        assert_eq!(ranges(b"abc xabbbc", b"a.*c"), vec![(0, 10)]);
        assert_eq!(ranges(b"ab\nabbbc", b"ab+"), vec![(0, 2), (3, 7)]);
        assert_eq!(ranges(b"id 42, id 7", br"\d+"), vec![(3, 5), (10, 11)]);
    }

    #[test]
    fn a_word_across_a_chunk_boundary_is_found_once_and_whole() {
        // Bounded: chunks are SEARCH_CHUNK_BYTES long.
        let mut text = b".".repeat(SEARCH_CHUNK_BYTES - 3);
        let start = text.len() as u64;
        text.extend_from_slice(b"boundary word");
        assert_eq!(
            ranges(&text, br"[a-z]{3,8}"),
            vec![(start, start + 8), (start + 9, start + 13)]
        );
        // Unbounded: chunks are four times the 1 MiB cap.
        let chunk = 4 * (MAX_REGEX_MATCH_BYTES - 1);
        let mut text = b".".repeat(chunk - 3);
        let start = text.len() as u64;
        text.extend_from_slice(b"boundary word");
        assert_eq!(
            ranges(&text, br"[a-z]+"),
            vec![(start, start + 8), (start + 9, start + 13)]
        );
    }

    #[test]
    fn a_match_longer_than_the_cap_is_reported() {
        let mut text = b"<".to_vec();
        text.extend(std::iter::repeat_n(b'a', MAX_REGEX_MATCH_BYTES + 10));
        text.push(b'>');
        let error = search(
            &MemorySource::new(text),
            b"<a+>",
            regex(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect_err("too long");
        assert!(matches!(error, SearchError::RegexMatchTooLarge(_)));
    }

    #[test]
    fn unsupported_regex_features_name_the_problem() {
        for pattern in [&br"(?<=id=)\d+"[..], br"(\w)\1"] {
            let error = search(
                &MemorySource::new(b"id=1 aa".to_vec()),
                pattern,
                regex(),
                0,
                10,
                &CancellationToken::default(),
            )
            .expect_err("unsupported");
            assert!(matches!(error, SearchError::InvalidRegex(_)), "{error}");
            assert!(!error.to_string().contains('\n'), "{error}");
        }
        let lookbehind = search(
            &MemorySource::new(b"id=1".to_vec()),
            br"(?<=id=)\d+",
            regex(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect_err("lookbehind");
        assert_eq!(
            lookbehind.to_string(),
            "invalid regular expression: lookahead and lookbehind ((?=…), (?!…), (?<=…), (?<!…)) are not supported (at character 1)"
        );
        let backreference = search(
            &MemorySource::new(b"aa".to_vec()),
            br"(\w)\1",
            regex(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect_err("backreference");
        assert!(
            backreference
                .to_string()
                .contains("use $1 in the replacement (at character 5)"),
            "{backreference}"
        );
    }

    fn replaced(source: &[u8], pattern: &[u8], replacement: &[u8]) -> Result<Vec<u8>, SearchError> {
        let source = MemorySource::new(source.to_vec());
        let page = search(
            &source,
            pattern,
            regex(),
            0,
            MAX_PAGE_MATCHES,
            &CancellationToken::default(),
        )?;
        let texts = expand_replacements(&source, pattern, regex(), &page.matches, replacement)?;
        let mut text = source_bytes(&source);
        for (start, len, insert) in plan_replacements_each(&page.matches, &texts)? {
            let start = usize::try_from(start).expect("start");
            let end = start + usize::try_from(len).expect("len");
            text.splice(start..end, insert);
        }
        Ok(text)
    }

    fn source_bytes(source: &MemorySource) -> Vec<u8> {
        let mut bytes =
            vec![0; usize::try_from(nulis_large_file::ReadAt::len(source)).expect("len")];
        nulis_large_file::ReadAt::read_at(source, 0, &mut bytes).expect("read");
        bytes
    }

    #[test]
    fn replacements_fill_in_groups() {
        let swap = |replacement: &[u8]| {
            replaced(b"x=1, y=22", br"(\w)=(?<n>\d+)", replacement).expect("replace")
        };
        assert_eq!(swap(b"$2:$1"), b"1:x, 22:y");
        assert_eq!(swap(b"${n}<-${1}"), b"1<-x, 22<-y");
        assert_eq!(swap(br"\2/\1"), b"1/x, 22/y");
        assert_eq!(swap(b"[$&]"), b"[x=1], [y=22]");
        assert_eq!(swap(b"[$0]"), b"[x=1], [y=22]");
        assert_eq!(swap(b"$$1"), b"$1, $1");
        assert_eq!(swap(br"a\tb\nc\\d\$e"), b"a\tb\nc\\d$e, a\tb\nc\\d$e");
        // A dollar that starts nothing stays as written; so does `&`.
        assert_eq!(swap(b"$ & ${}"), b"$ & ${}, $ & ${}");
    }

    #[test]
    fn an_optional_group_that_did_not_take_part_inserts_nothing() {
        assert_eq!(
            replaced(b"ab b", br"(a)?b", b"<$1>").expect("replace"),
            b"<a> <>"
        );
    }

    #[test]
    fn a_reference_to_a_missing_group_is_an_error() {
        for replacement in [&b"$2"[..], br"\3", b"${name}"] {
            let error = replaced(b"ab", br"(a)b", replacement).expect_err("missing group");
            assert!(matches!(error, SearchError::UnknownGroup(_)), "{error}");
        }
    }

    #[test]
    fn literal_replacements_are_inserted_as_written() {
        let source = MemorySource::new(b"a.a".to_vec());
        let page = search(
            &source,
            b".",
            SearchOptions::default(),
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("search");
        let texts = expand_replacements(
            &source,
            b".",
            SearchOptions::default(),
            &page.matches,
            br"$1\n",
        )
        .expect("expand");
        assert_eq!(texts, vec![br"$1\n".to_vec()]);
    }

    fn in_columns(mode: SearchMode, from: Option<u32>, to: Option<u32>) -> SearchOptions {
        SearchOptions {
            mode,
            case_sensitive: true,
            whole_word: false,
            columns: Some(ColumnRange { from, to }),
        }
    }

    fn found(source: &[u8], pattern: &[u8], options: SearchOptions, start: u64) -> Vec<(u64, u64)> {
        search(
            &MemorySource::new(source.to_vec()),
            pattern,
            options,
            start,
            MAX_PAGE_MATCHES,
            &CancellationToken::default(),
        )
        .expect("search")
        .matches
        .iter()
        .map(|found| (found.range.start.0, found.range.end.0))
        .collect()
    }

    #[test]
    fn column_search_looks_only_between_the_columns() {
        // Columns 3..=5 of each line: "cde", "345", none, "xx" (bytes 22..24).
        let text = b"abcdefg\n1234567\r\nxx\rabxx";
        let literal = |pattern: &[u8], from, to| {
            found(text, pattern, in_columns(SearchMode::Literal, from, to), 0)
        };
        assert_eq!(literal(b"d", Some(3), Some(5)), vec![(3, 4)]);
        assert_eq!(literal(b"de", Some(3), Some(5)), vec![(3, 5)]);
        // A match must lie fully inside the columns.
        assert_eq!(literal(b"ef", Some(3), Some(5)), Vec::<(u64, u64)>::new());
        assert_eq!(literal(b"1", Some(3), Some(5)), Vec::<(u64, u64)>::new());
        assert_eq!(literal(b"x", Some(3), Some(5)), vec![(22, 23), (23, 24)]);
        // From alone runs to the end of the line; To alone from its start.
        assert_eq!(literal(b"7", Some(6), None), vec![(14, 15)]);
        assert_eq!(literal(b"a", None, Some(1)), vec![(0, 1), (20, 21)]);
    }

    #[test]
    fn columns_count_characters_and_a_tab_as_one() {
        let text = "\tédx d".as_bytes();
        // Column 3 is "d" (after a tab and the two-byte "é").
        assert_eq!(
            found(
                text,
                b"d",
                in_columns(SearchMode::Literal, Some(3), Some(3)),
                0
            ),
            vec![(3, 4)]
        );
    }

    #[test]
    fn anchors_match_at_the_edges_of_the_columns() {
        let text = b"ab12cd\nxy345z";
        let regex = |pattern: &[u8]| {
            found(
                text,
                pattern,
                in_columns(SearchMode::Regex, Some(3), Some(4)),
                0,
            )
        };
        assert_eq!(regex(br"^\d+$"), vec![(2, 4), (9, 11)]);
        assert_eq!(regex(br"\d+"), vec![(2, 4), (9, 11)]);
    }

    #[test]
    fn column_search_resumes_mid_line_and_pages() {
        let text = b"aaaa\naaaa";
        let options = in_columns(SearchMode::Literal, Some(2), Some(3));
        assert_eq!(found(text, b"a", options, 2), vec![(2, 3), (6, 7), (7, 8)]);
        let page = search(
            &MemorySource::new(text.to_vec()),
            b"a",
            options,
            0,
            3,
            &CancellationToken::default(),
        )
        .expect("page");
        assert_eq!(page.next_offset, Some(7));
        assert!(!page.complete);
    }

    #[test]
    fn bad_column_ranges_are_refused() {
        for (from, to) in [(Some(0), None), (Some(5), Some(2))] {
            let error = search(
                &MemorySource::new(b"abc".to_vec()),
                b"a",
                in_columns(SearchMode::Literal, from, to),
                0,
                10,
                &CancellationToken::default(),
            )
            .expect_err("bad range");
            assert!(matches!(error, SearchError::InvalidColumns(_)), "{error}");
        }
    }

    #[test]
    fn column_replacements_see_the_same_edges_as_the_search() {
        let source = MemorySource::new(b"ab12cd".to_vec());
        let options = in_columns(SearchMode::Regex, Some(3), Some(4));
        let page = search(
            &source,
            br"^(\d)(\d)$",
            options,
            0,
            10,
            &CancellationToken::default(),
        )
        .expect("search");
        let texts = expand_replacements(&source, br"^(\d)(\d)$", options, &page.matches, b"$2$1")
            .expect("expand");
        assert_eq!(texts, vec![b"21".to_vec()]);
    }

    #[test]
    fn search_until_returns_only_matches_that_start_before_the_stop() {
        let source = MemorySource::new(b"one two three two".to_vec());
        let page = search_until(
            &source,
            br"t\w+",
            regex(),
            0,
            9,
            10,
            &CancellationToken::default(),
        )
        .expect("search");
        // "two" (4..7) and "three" (8..13), which runs past the stop but starts before it.
        let found: Vec<_> = page
            .matches
            .iter()
            .map(|found| (found.range.start.0, found.range.end.0))
            .collect();
        assert_eq!(found, vec![(4, 7), (8, 13)]);
        let columns = in_columns(SearchMode::Literal, Some(1), Some(3));
        let lines = MemorySource::new(b"two\ntwo\ntwo".to_vec());
        let page = search_until(
            &lines,
            b"two",
            columns,
            0,
            5,
            10,
            &CancellationToken::default(),
        )
        .expect("column search");
        assert_eq!(page.matches.len(), 2);
    }

    fn count_in_slices(
        text: &[u8],
        pattern: &[u8],
        options: SearchOptions,
        slice: u64,
    ) -> (u64, u64) {
        let source = MemorySource::new(text.to_vec());
        let mut progress = CountProgress::starting_at(0);
        while !progress.done {
            let stop = progress.next + slice;
            count_slice(
                &source,
                pattern,
                options,
                &mut progress,
                stop,
                &CancellationToken::default(),
            )
            .expect("count");
        }
        (progress.matches, progress.lines)
    }

    #[test]
    fn counting_has_no_match_limit_and_is_the_same_in_any_slice_size() {
        // 3,000 matches on 1,000 lines: three per line.
        let text = b"ab ab ab\n".repeat(1_000);
        for slice in [7, 64, 1_000, 1 << 20] {
            assert_eq!(
                count_in_slices(&text, b"ab", SearchOptions::default(), slice),
                (3_000, 1_000),
                "{slice}"
            );
            assert_eq!(
                count_in_slices(&text, br"a\w", regex(), slice),
                (3_000, 1_000),
                "{slice}"
            );
        }
        // A match that runs across a line counts on the line where it starts.
        assert_eq!(
            count_in_slices(b"x a\nb x\nzz\nq", br"a\nb", regex(), 3),
            (1, 1)
        );
        assert_eq!(
            count_in_slices(b"one\ntwo\n", b"o", SearchOptions::default(), 2),
            (2, 2)
        );
        // Columns: each line's columns on their own.
        let columns = SearchOptions {
            columns: Some(ColumnRange {
                from: Some(1),
                to: Some(2),
            }),
            ..SearchOptions::default()
        };
        assert_eq!(count_in_slices(&text, b"ab", columns, 50), (1_000, 1_000));
    }

    #[test]
    fn counting_stops_when_cancelled_and_can_resume() {
        let text = b"ab\n".repeat(1_000);
        let source = MemorySource::new(text.clone());
        let mut progress = CountProgress::starting_at(0);
        let cancelled = CancellationToken::default();
        cancelled.cancel();
        count_slice(
            &source,
            b"ab",
            SearchOptions::default(),
            &mut progress,
            3_000,
            &cancelled,
        )
        .expect("count");
        assert!(!progress.done);
        assert_eq!(progress.matches, 0);
        count_slice(
            &source,
            b"ab",
            SearchOptions::default(),
            &mut progress,
            3_000,
            &CancellationToken::default(),
        )
        .expect("resume");
        assert!(progress.done);
        assert_eq!((progress.matches, progress.lines), (1_000, 1_000));
    }

    #[test]
    fn plain_text_search_folds_ascii_case_only_and_handles_any_bytes() {
        let any_case = SearchOptions {
            case_sensitive: false,
            ..SearchOptions::default()
        };
        let found = |text: &[u8], pattern: &[u8], options| {
            search(
                &MemorySource::new(text.to_vec()),
                pattern,
                options,
                0,
                10,
                &CancellationToken::default(),
            )
            .expect("search")
            .matches
            .len()
        };
        assert_eq!(found(b"Abc aBC abc", b"abc", any_case), 3);
        assert_eq!(found(b"Abc aBC abc", b"abc", SearchOptions::default()), 1);
        // Windows-1252 é (0xE9) and É (0xC9) are different bytes; only ASCII letters fold.
        assert_eq!(found(b"\xE9t\xC9", b"\xE9", any_case), 1);
        // Characters that mean something in a pattern are plain text here.
        assert_eq!(found(b"a.c abc (x)", b"a.c", SearchOptions::default()), 1);
        assert_eq!(found(b"a.c abc (x)", b"(x)", SearchOptions::default()), 1);
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
