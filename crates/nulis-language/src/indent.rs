//! Indentation rules: what Enter puts at the start of a new line, and when a
//! typed character moves its line left. Pure functions over line text; the
//! core reads the context from the document and applies the result.
//!
//! Every family starts from the current line's indentation and adjusts it
//! with language rules, so a language without rules still keeps its indent.

use serde::{Deserialize, Serialize};

/// Most lines above the caret an indentation rule looks at.
pub const MAX_CONTEXT_LINES: usize = 64;
/// Most bytes of those lines an indentation rule looks at.
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;

/// One level of indentation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "width")]
pub enum IndentUnit {
    Tab,
    Spaces(u8),
}

impl IndentUnit {
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Tab => "\t".into(),
            Self::Spaces(width) => " ".repeat(usize::from(width.max(1))),
        }
    }

    /// Columns one level takes.
    #[must_use]
    pub fn columns(self, tab_width: u8) -> usize {
        match self {
            Self::Tab => usize::from(tab_width.max(1)),
            Self::Spaces(width) => usize::from(width.max(1)),
        }
    }
}

/// What a rule sees: the current line split at the caret, and the lines above.
#[derive(Clone, Copy, Debug)]
pub struct IndentContext<'a> {
    /// The line from its start to the caret.
    pub before: &'a str,
    /// The line from the caret to its end, without the line ending.
    pub after: &'a str,
    /// Lines above the current one, nearest first, at most [`MAX_CONTEXT_LINES`].
    pub previous: &'a [&'a str],
    pub unit: IndentUnit,
    pub tab_width: u8,
}

/// What Enter does.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NewlineAction {
    /// New leading whitespace for the current line (a closing keyword moved left).
    pub current_indent: Option<String>,
    /// Leading whitespace of the new line, where the caret goes.
    pub indent: String,
    /// When the caret was between a pair (`{|}`, `<a>|</a>`): the closing
    /// part moves to a third line with this indentation.
    pub closing_indent: Option<String>,
}

/// An indentation rule set for one or more languages.
pub trait IndentProvider: Send + Sync {
    fn on_enter(&self, context: &IndentContext<'_>) -> NewlineAction;

    /// Called before `typed` is inserted at the caret; returns the line's new
    /// leading whitespace when the character should move the line.
    fn on_type(&self, context: &IndentContext<'_>, typed: char) -> Option<String>;

    /// A language whose files require one kind of indentation: YAML forbids
    /// tabs, Makefile recipes need them.
    fn required_unit(&self) -> Option<RequiredUnit> {
        None
    }
}

/// See [`IndentProvider::required_unit`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequiredUnit {
    Tabs,
    Spaces,
}

/// Block keywords for languages without braces.
#[derive(Clone, Copy, Debug)]
pub struct KeywordRules {
    pub case_insensitive: bool,
    /// A line that starts with one of these opens a block (`def`, `while`).
    pub start_openers: &'static [&'static str],
    /// A line that ends with one of these opens a block (`then`, `do`).
    pub end_openers: &'static [&'static str],
    /// Lines that close one block and open the next (`else`, `elsif`).
    pub middles: &'static [&'static str],
    /// Lines that close a block (`end`, `fi`).
    pub closers: &'static [&'static str],
    /// Leading words that do not change the meaning (`Public`, `Private`).
    pub modifiers: &'static [&'static str],
    /// The language also uses braces for blocks.
    pub braces: bool,
    /// Words that, after a start opener, make the line a one-line statement
    /// (in `AppleScript`, `tell app "X" to activate`).
    pub one_liner_words: &'static [&'static str],
    /// Block parameters `|x, y|` at the end of a line are ignored (Ruby `do |x|`).
    pub block_parameters: bool,
}

/// The built-in rule families.
#[derive(Clone, Copy, Debug)]
pub enum Family {
    /// Keep the current line's indentation.
    Carry,
    /// Blocks in `{}`, `[]`, `()`.
    Brace,
    /// A `:` opens a block; `else`/`elif`/`except`/`finally` move left.
    Python,
    /// A key without a value or a block scalar opens a level; spaces only.
    Yaml,
    /// A rule line is followed by tab-indented recipe lines.
    Makefile,
    /// Elements open levels.
    Markup,
    Keywords(&'static KeywordRules),
}

/// A family plus the language's line comment marker.
#[derive(Clone, Copy, Debug)]
pub struct RuleIndenter {
    pub family: Family,
    pub line_comment: Option<&'static str>,
}

const BRACE_PAIRS: [(char, char); 3] = [('{', '}'), ('[', ']'), ('(', ')')];

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The leading spaces and tabs of a line.
#[must_use]
pub fn leading_whitespace(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// Columns a run of whitespace takes, with tab stops every `tab_width`.
#[must_use]
pub fn indent_columns(whitespace: &str, tab_width: u8) -> usize {
    let tab = usize::from(tab_width.max(1));
    whitespace
        .chars()
        .fold(0, |column, character| match character {
            '\t' => column + tab - column % tab,
            _ => column + 1,
        })
}

/// Whitespace reaching `columns`, written in `unit`.
#[must_use]
pub fn indent_text(columns: usize, unit: IndentUnit, tab_width: u8) -> String {
    match unit {
        IndentUnit::Tab => {
            let tab = usize::from(tab_width.max(1));
            format!(
                "{}{}",
                "\t".repeat(columns / tab),
                " ".repeat(columns % tab)
            )
        }
        IndentUnit::Spaces(_) => " ".repeat(columns),
    }
}

impl IndentContext<'_> {
    fn base(&self) -> &str {
        leading_whitespace(self.before)
    }

    fn deeper(&self, whitespace: &str) -> String {
        format!("{whitespace}{}", self.unit.text())
    }

    fn shallower(&self, whitespace: &str) -> String {
        let columns = indent_columns(whitespace, self.tab_width)
            .saturating_sub(self.unit.columns(self.tab_width));
        indent_text(columns, self.unit, self.tab_width)
    }

    fn columns(&self, whitespace: &str) -> usize {
        indent_columns(whitespace, self.tab_width)
    }

    fn previous_code_line(&self) -> Option<&str> {
        self.previous
            .iter()
            .copied()
            .find(|line| !line.trim().is_empty())
    }
}

/// The line without a trailing comment. Quotes are respected, so `"//"`
/// inside a string is kept.
fn without_comment<'a>(line: &'a str, marker: Option<&str>) -> &'a str {
    let Some(marker) = marker else {
        return line;
    };
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == open {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'' | '`') {
            quote = Some(character);
        } else if line[index..].starts_with(marker) {
            return &line[..index];
        }
    }
    line
}

/// `items.each do |item|` without the trailing `|item|`.
fn strip_block_parameters(code: &str) -> &str {
    let trimmed = code.trim_end();
    if let Some(inner) = trimmed.strip_suffix('|') {
        if let Some(open) = inner.rfind('|') {
            return inner[..open].trim_end();
        }
    }
    trimmed
}

fn closer_for(open: char) -> Option<char> {
    BRACE_PAIRS
        .iter()
        .find(|(left, _)| *left == open)
        .map(|(_, right)| *right)
}

impl RuleIndenter {
    fn code<'a>(&self, line: &'a str) -> &'a str {
        without_comment(line, self.line_comment).trim_end()
    }

    /// Enter after an opening bracket: one level deeper, and `{|}` splits.
    fn brace_enter(&self, context: &IndentContext<'_>) -> Option<NewlineAction> {
        let last = self.code(context.before).chars().last()?;
        let close = closer_for(last)?;
        let base = context.base();
        let splits = context.after.trim_start().starts_with(close);
        Some(NewlineAction {
            current_indent: None,
            indent: context.deeper(base),
            closing_indent: splits.then(|| base.to_owned()),
        })
    }

    /// The indentation of the line holding the bracket that `close` matches.
    fn matching_open_indent(&self, context: &IndentContext<'_>, close: char) -> Option<String> {
        let open = BRACE_PAIRS.iter().find(|(_, right)| *right == close)?.0;
        let mut depth = 0_usize;
        for line in context.previous {
            for character in self.code(line).chars().rev() {
                if character == close {
                    depth += 1;
                } else if character == open {
                    if depth == 0 {
                        return Some(leading_whitespace(line).to_owned());
                    }
                    depth -= 1;
                }
            }
        }
        None
    }

    /// One level left of the line above, unless that line opened the block.
    fn outdent_from_previous(
        context: &IndentContext<'_>,
        opens: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let previous = context.previous_code_line()?;
        let previous_indent = leading_whitespace(previous);
        let target = if opens(previous) {
            previous_indent.to_owned()
        } else {
            context.shallower(previous_indent)
        };
        (context.columns(&target) < context.columns(context.base())).then_some(target)
    }

    fn python_enter(&self, context: &IndentContext<'_>) -> NewlineAction {
        if let Some(action) = self.brace_enter(context) {
            return action;
        }
        let code = self.code(context.before);
        let base = context.base();
        let first_word = code.split_whitespace().next().unwrap_or_default();
        let indent = if code.ends_with(':') {
            context.deeper(base)
        } else if matches!(
            first_word,
            "return" | "pass" | "break" | "continue" | "raise"
        ) {
            context.shallower(base)
        } else {
            base.to_owned()
        };
        NewlineAction {
            current_indent: None,
            indent,
            closing_indent: None,
        }
    }

    fn yaml_enter(&self, context: &IndentContext<'_>) -> NewlineAction {
        let code = self.code(context.before);
        let opens = code.ends_with(':')
            || ["|", ">", "|-", ">-", "|+", ">+"]
                .iter()
                .any(|indicator| code.ends_with(&format!(": {indicator}")));
        let base = context.base();
        NewlineAction {
            current_indent: None,
            indent: if opens {
                context.deeper(base)
            } else {
                base.to_owned()
            },
            closing_indent: None,
        }
    }

    fn makefile_enter(&self, context: &IndentContext<'_>) -> NewlineAction {
        let line = context.before;
        let code = self.code(line);
        let is_rule = !line.starts_with('\t')
            && !code.trim_start().starts_with('#')
            && code.find(':').is_some_and(|colon| {
                let rest = &code[colon..];
                !rest.starts_with(":=") && !rest.starts_with("::=") && !code[..colon].contains('=')
            });
        let indent = if is_rule {
            "\t".to_owned()
        } else {
            context.base().to_owned()
        };
        NewlineAction {
            current_indent: None,
            indent,
            closing_indent: None,
        }
    }

    /// The name of an element the text opens at its end and does not close.
    fn open_tag_at_end(text: &str) -> Option<&str> {
        let text = text.trim_end();
        if !text.ends_with('>') || text.ends_with("/>") {
            return None;
        }
        let start = text.rfind('<')?;
        let tag = &text[start + 1..];
        if tag.starts_with(['/', '?', '!']) {
            return None;
        }
        let name_end = tag
            .find(|character: char| character.is_whitespace() || character == '>')
            .unwrap_or(tag.len());
        let name = &tag[..name_end];
        if name.is_empty() || VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(name)
    }

    fn markup_enter(context: &IndentContext<'_>) -> NewlineAction {
        let base = context.base();
        match Self::open_tag_at_end(context.before) {
            Some(name) => {
                let splits = context
                    .after
                    .trim_start()
                    .strip_prefix("</")
                    .is_some_and(|rest| rest.starts_with(name));
                NewlineAction {
                    current_indent: None,
                    indent: context.deeper(base),
                    closing_indent: splits.then(|| base.to_owned()),
                }
            }
            None => NewlineAction {
                current_indent: None,
                indent: base.to_owned(),
                closing_indent: None,
            },
        }
    }

    fn keyword_words(rules: &KeywordRules, code: &str) -> Vec<String> {
        let words = code
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .filter(|word| !word.is_empty())
            .map(|word| {
                if rules.case_insensitive {
                    word.to_lowercase()
                } else {
                    word.to_owned()
                }
            });
        let mut words: Vec<String> = words.collect();
        while words
            .first()
            .is_some_and(|word| rules.modifiers.contains(&word.as_str()))
        {
            words.remove(0);
        }
        words
    }

    /// Whether a line opens a keyword block (or a brace block).
    fn keyword_opens(&self, rules: &KeywordRules, line: &str) -> bool {
        let code = self.code(line);
        if rules.braces && code.ends_with('{') {
            return true;
        }
        let code = if rules.block_parameters {
            strip_block_parameters(code)
        } else {
            code
        };
        let words = Self::keyword_words(rules, code);
        let (Some(_), Some(last)) = (words.first(), words.last()) else {
            return false;
        };
        if words
            .iter()
            .skip(1)
            .any(|word| rules.one_liner_words.contains(&word.as_str()))
        {
            return false;
        }
        if words.len() > 1 && rules.closers.contains(&last.as_str()) {
            // `def x; end`, `function() end`: opened and closed on one line.
            return false;
        }
        let starts = |list: &[&str]| {
            list.iter().any(|phrase| {
                let phrase_words: Vec<&str> = phrase.split(' ').collect();
                words.len() >= phrase_words.len()
                    && phrase_words
                        .iter()
                        .zip(&words)
                        .all(|(expected, word)| expected == word)
            })
        };
        starts(rules.start_openers)
            || starts(rules.middles)
            || rules.end_openers.contains(&last.as_str())
    }

    /// The whole line is a closing or middle keyword (`end`, `else`).
    fn keyword_line(rules: &KeywordRules, line: &str) -> Option<bool> {
        let code = line.trim();
        let code = if rules.case_insensitive {
            code.to_lowercase()
        } else {
            code.to_owned()
        };
        let closes = rules.closers.iter().any(|word| {
            code == *word
                || code
                    .strip_prefix(word)
                    .is_some_and(|rest| rest.starts_with([' ', ';']))
        }) || (rules.braces && code.starts_with('}'));
        let middle = rules.middles.iter().any(|word| {
            code == *word
                || code
                    .strip_prefix(word)
                    .is_some_and(|rest| rest.starts_with([' ', ';', ':']))
        });
        if middle {
            Some(true)
        } else if closes {
            Some(false)
        } else {
            None
        }
    }

    fn keyword_enter(&self, rules: &KeywordRules, context: &IndentContext<'_>) -> NewlineAction {
        let whole = format!("{}{}", context.before, context.after);
        let mut current_indent = None;
        let mut base = context.base().to_owned();
        if context.after.trim().is_empty() {
            if let Some(middle) = Self::keyword_line(rules, &whole) {
                // `end` or `else` typed on its own line moves left first.
                if let Some(target) =
                    Self::outdent_from_previous(context, |line| self.keyword_opens(rules, line))
                {
                    base.clone_from(&target);
                    current_indent = Some(target);
                }
                let indent = if middle { context.deeper(&base) } else { base };
                return NewlineAction {
                    current_indent,
                    indent,
                    closing_indent: None,
                };
            }
        }
        if rules.braces {
            if let Some(action) = self.brace_enter(context) {
                return action;
            }
        }
        let indent = if self.keyword_opens(rules, context.before) {
            context.deeper(&base)
        } else {
            base
        };
        NewlineAction {
            current_indent,
            indent,
            closing_indent: None,
        }
    }
}

impl IndentProvider for RuleIndenter {
    fn on_enter(&self, context: &IndentContext<'_>) -> NewlineAction {
        let carry = || NewlineAction {
            current_indent: None,
            indent: context.base().to_owned(),
            closing_indent: None,
        };
        match self.family {
            Family::Carry => carry(),
            Family::Brace => self.brace_enter(context).unwrap_or_else(carry),
            Family::Python => self.python_enter(context),
            Family::Yaml => self.yaml_enter(context),
            Family::Makefile => self.makefile_enter(context),
            Family::Markup => Self::markup_enter(context),
            Family::Keywords(rules) => self.keyword_enter(rules, context),
        }
    }

    fn on_type(&self, context: &IndentContext<'_>, typed: char) -> Option<String> {
        let only_indent = context.before.trim().is_empty();
        let target = match (self.family, typed) {
            (Family::Brace | Family::Python, '}' | ']' | ')') if only_indent => self
                .matching_open_indent(context, typed)
                .or_else(|| Some(context.shallower(context.base()))),
            (Family::Keywords(rules), '}') if rules.braces && only_indent => self
                .matching_open_indent(context, typed)
                .or_else(|| Some(context.shallower(context.base()))),
            (Family::Python, ':') => {
                let line = context.before.trim();
                let word = line.split([' ', ':']).next().unwrap_or_default();
                if matches!(word, "else" | "elif" | "except" | "finally" | "case")
                    && context.after.trim().is_empty()
                {
                    let current = context.columns(context.base());
                    context
                        .previous
                        .iter()
                        .find(|line| {
                            !line.trim().is_empty()
                                && context.columns(leading_whitespace(line)) < current
                        })
                        .map(|line| leading_whitespace(line).to_owned())
                } else {
                    None
                }
            }
            (Family::Markup, '/') if context.before.trim() == "<" => {
                Self::outdent_from_previous(context, |line| Self::open_tag_at_end(line).is_some())
            }
            _ => None,
        }?;
        (target != context.base()).then_some(target)
    }

    fn required_unit(&self) -> Option<RequiredUnit> {
        match self.family {
            Family::Yaml => Some(RequiredUnit::Spaces),
            Family::Makefile => Some(RequiredUnit::Tabs),
            _ => None,
        }
    }
}

/// Guesses how a document is indented from its text (the core passes the
/// first 64 KiB): tabs, or spaces with the most common step between
/// neighbouring lines. `None` when too few lines are indented to tell.
#[must_use]
pub fn detect_unit(text: &str) -> Option<IndentUnit> {
    const MIN_INDENTED_LINES: usize = 3;
    let mut tab_lines = 0_usize;
    let mut space_lines = 0_usize;
    let mut steps = [0_usize; 9];
    let mut previous_spaces = 0_usize;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if line.starts_with('\t') {
            tab_lines += 1;
            continue;
        }
        let spaces = line.len() - line.trim_start_matches(' ').len();
        if spaces > 0 {
            space_lines += 1;
        }
        if spaces > previous_spaces {
            if let Some(count) = steps.get_mut(spaces - previous_spaces) {
                *count += 1;
            }
        }
        previous_spaces = spaces;
    }
    if tab_lines.max(space_lines) < MIN_INDENTED_LINES {
        return None;
    }
    if tab_lines > space_lines {
        return Some(IndentUnit::Tab);
    }
    // Ties go to the wider step: a 4-space file often has some 2-space continuations.
    [2_u8, 4, 8, 3]
        .into_iter()
        .max_by_key(|width| (steps[usize::from(*width)], *width == 4))
        .filter(|width| steps[usize::from(*width)] > 0)
        .map(IndentUnit::Spaces)
}

/// Ruby blocks.
pub const RUBY: KeywordRules = KeywordRules {
    case_insensitive: false,
    start_openers: &[
        "def", "class", "module", "if", "unless", "while", "until", "case", "begin", "for",
    ],
    end_openers: &["do"],
    middles: &["else", "elsif", "when", "in", "rescue", "ensure"],
    closers: &["end"],
    modifiers: &["private", "protected", "public"],
    braces: false,
    one_liner_words: &[],
    block_parameters: true,
};

/// Lua blocks.
pub const LUA: KeywordRules = KeywordRules {
    case_insensitive: false,
    start_openers: &["function", "local function", "repeat"],
    end_openers: &["then", "do"],
    middles: &["else", "elseif"],
    closers: &["end", "until"],
    modifiers: &[],
    braces: false,
    one_liner_words: &[],
    block_parameters: false,
};

/// Bash blocks.
pub const BASH: KeywordRules = KeywordRules {
    case_insensitive: false,
    start_openers: &[],
    end_openers: &["then", "do", "in"],
    middles: &["else", "elif"],
    closers: &["fi", "done", "esac"],
    modifiers: &[],
    braces: true,
    one_liner_words: &[],
    block_parameters: false,
};

/// Visual Basic and `VBScript` blocks.
pub const VISUAL_BASIC: KeywordRules = KeywordRules {
    case_insensitive: true,
    start_openers: &[
        "sub",
        "function",
        "property",
        "class",
        "module",
        "structure",
        "enum",
        "interface",
        "namespace",
        "for",
        "do",
        "while",
        "with",
        "select case",
        "try",
    ],
    end_openers: &["then"],
    middles: &["else", "elseif", "case", "catch", "finally"],
    closers: &["end", "next", "loop", "wend"],
    modifiers: &[
        "public",
        "private",
        "friend",
        "protected",
        "shared",
        "static",
        "overrides",
        "overridable",
        "overloads",
        "mustoverride",
        "notinheritable",
        "partial",
        "default",
    ],
    braces: false,
    one_liner_words: &[],
    block_parameters: false,
};

/// Oracle PL/SQL blocks.
pub const PLSQL: KeywordRules = KeywordRules {
    case_insensitive: true,
    start_openers: &["declare", "begin"],
    end_openers: &["begin", "loop", "then", "is", "as"],
    middles: &["else", "elsif", "exception"],
    closers: &["end"],
    modifiers: &["create", "or", "replace"],
    braces: false,
    one_liner_words: &[],
    block_parameters: false,
};

/// `AppleScript` blocks.
pub const APPLESCRIPT: KeywordRules = KeywordRules {
    case_insensitive: true,
    start_openers: &[
        "tell",
        "on",
        "to",
        "repeat",
        "try",
        "considering",
        "ignoring",
        "using terms from",
        "with timeout",
        "with transaction",
    ],
    end_openers: &["then"],
    middles: &["else", "on error"],
    closers: &["end"],
    modifiers: &[],
    braces: false,
    one_liner_words: &["to"],
    block_parameters: false,
};

#[cfg(test)]
mod tests;
