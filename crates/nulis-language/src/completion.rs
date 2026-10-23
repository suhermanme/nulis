//! Completion without a language server: words from the document, the
//! language's keywords and types, and built-in snippets, ranked and
//! deduplicated. Pure functions; the core reads a bounded window of text
//! around the caret and applies the chosen item.

use nulis_syntax::Vocabulary;
use serde::Serialize;

/// Most items one request returns.
pub const MAX_COMPLETION_ITEMS: usize = 100;
/// Most distinct words the word provider collects from its window.
const MAX_WORDS: usize = 2_000;
/// Longest word the word provider offers.
const MAX_WORD_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionKind {
    Snippet,
    Keyword,
    Type,
    Word,
}

/// One suggestion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    /// Text that replaces the word being typed. For a snippet, the snippet
    /// body (see [`expand_snippet`]).
    pub insert: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub snippet: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What a provider sees.
#[derive(Clone, Copy, Debug)]
pub struct CompletionRequest<'a> {
    /// The word part just before the caret (may be empty).
    pub prefix: &'a str,
    /// Text of the window before the caret, ending at the caret.
    pub before: &'a str,
    /// Text of the window after the caret.
    pub after: &'a str,
    /// Bytes besides letters, digits, and `_` that belong to a word.
    pub word_bytes: &'a [u8],
}

/// A source of suggestions.
pub trait CompletionProvider: Send + Sync {
    fn complete(&self, request: &CompletionRequest<'_>) -> Vec<CompletionItem>;

    /// Higher ranks first among equally good matches (snippets 2, keywords 1, words 0).
    fn priority(&self) -> u8 {
        0
    }

    /// The language offers its own words (keywords, snippets), so the popup
    /// may open while typing; words alone wait to be asked for.
    fn opens_automatically(&self) -> bool {
        false
    }
}

/// Whether a character belongs to a word in a language.
#[must_use]
pub fn is_word_char(character: char, word_bytes: &[u8]) -> bool {
    character.is_alphanumeric()
        || character == '_'
        || (character.is_ascii() && word_bytes.contains(&(character as u8)))
}

/// The word part that ends at the end of `before`.
#[must_use]
pub fn word_prefix<'a>(before: &'a str, word_bytes: &[u8]) -> &'a str {
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, character)| is_word_char(*character, word_bytes))
        .last()
        .map_or(before.len(), |(index, _)| index);
    &before[start..]
}

/// Words found in the text around the caret, nearest first.
#[derive(Clone, Copy, Debug, Default)]
pub struct WordCompletion;

impl CompletionProvider for WordCompletion {
    fn complete(&self, request: &CompletionRequest<'_>) -> Vec<CompletionItem> {
        let split = |text: &str| -> Vec<String> {
            text.split(|character: char| !is_word_char(character, request.word_bytes))
                .filter(|word| {
                    let count = word.chars().count();
                    (2..=MAX_WORD_CHARS).contains(&count)
                        && !word
                            .chars()
                            .next()
                            .is_some_and(|first| first.is_ascii_digit())
                })
                .map(str::to_owned)
                .collect()
        };
        // The word being typed is cut at the caret; leave it out on both sides.
        let before = &request.before[..request.before.len() - request.prefix.len()];
        let after_word = request
            .after
            .trim_start_matches(|character: char| is_word_char(character, request.word_bytes));
        let mut seen = std::collections::BTreeSet::new();
        let mut items = Vec::new();
        let nearest_first = split(before)
            .into_iter()
            .rev()
            .zip(split(after_word))
            .flat_map(|(left, right)| [left, right])
            .chain(split(before).into_iter().rev())
            .chain(split(after_word));
        for word in nearest_first {
            if items.len() == MAX_WORDS {
                break;
            }
            if seen.insert(word.clone()) {
                items.push(CompletionItem {
                    label: word.clone(),
                    kind: CompletionKind::Word,
                    insert: word,
                    snippet: false,
                    detail: None,
                });
            }
        }
        items
    }
}

/// A language's keywords and types.
#[derive(Clone, Debug)]
pub struct KeywordCompletion {
    pub vocabulary: Vocabulary,
}

/// Writes a case-insensitive keyword the way the user started it:
/// `SEL` → `SELECT`, `Sel` → `Select`, `sel` → `select`.
fn in_prefix_case(keyword: &str, prefix: &str) -> String {
    let letters: Vec<char> = prefix.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        keyword.to_uppercase()
    } else if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut characters = keyword.chars();
        characters
            .next()
            .map(|first| first.to_uppercase().chain(characters).collect())
            .unwrap_or_default()
    } else {
        keyword.to_owned()
    }
}

impl CompletionProvider for KeywordCompletion {
    fn priority(&self) -> u8 {
        1
    }

    fn opens_automatically(&self) -> bool {
        true
    }

    fn complete(&self, request: &CompletionRequest<'_>) -> Vec<CompletionItem> {
        let vocabulary = &self.vocabulary;
        let item = |word: &str, kind| {
            let text = if vocabulary.case_insensitive {
                in_prefix_case(word, request.prefix)
            } else {
                word.to_owned()
            };
            CompletionItem {
                label: text.clone(),
                kind,
                insert: text,
                snippet: false,
                detail: None,
            }
        };
        vocabulary
            .keywords
            .iter()
            .map(|word| item(word, CompletionKind::Keyword))
            .chain(
                vocabulary
                    .types
                    .iter()
                    .map(|word| item(word, CompletionKind::Type)),
            )
            .collect()
    }
}

/// A built-in snippet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snippet {
    /// What the user types to find it.
    pub prefix: &'static str,
    /// `$1`, `${1:placeholder}`, `$0`; `\n` starts a line at the current
    /// indentation and `\t` is one indentation level.
    pub body: &'static str,
    pub description: &'static str,
}

/// A language's snippets.
#[derive(Clone, Copy, Debug)]
pub struct SnippetCompletion {
    pub snippets: &'static [Snippet],
}

impl CompletionProvider for SnippetCompletion {
    fn priority(&self) -> u8 {
        2
    }

    fn opens_automatically(&self) -> bool {
        true
    }

    fn complete(&self, _request: &CompletionRequest<'_>) -> Vec<CompletionItem> {
        self.snippets
            .iter()
            .map(|snippet| CompletionItem {
                label: snippet.prefix.to_owned(),
                kind: CompletionKind::Snippet,
                insert: snippet.body.to_owned(),
                snippet: true,
                detail: Some(snippet.description.to_owned()),
            })
            .collect()
    }
}

/// How well `label` matches what was typed; lower is better, `None` is no match.
fn match_tier(label: &str, prefix: &str) -> Option<u8> {
    if prefix.is_empty() {
        return Some(0);
    }
    if label.starts_with(prefix) {
        return Some(0);
    }
    let lower_label = label.to_lowercase();
    let lower_prefix = prefix.to_lowercase();
    if lower_label.starts_with(&lower_prefix) {
        return Some(1);
    }
    // Word starts: `gBN` matches `getByName`, `g_b` matches `get_by`.
    let mut starts = String::new();
    let mut previous: Option<char> = None;
    for character in label.chars() {
        let boundary = match previous {
            None => true,
            Some(before) => {
                (character.is_uppercase() && !before.is_uppercase())
                    || (!matches!(character, '_' | '-') && matches!(before, '_' | '-'))
            }
        };
        if boundary {
            starts.extend(character.to_lowercase());
        }
        previous = Some(character);
    }
    let plain_prefix: String = lower_prefix
        .chars()
        .filter(|c| !matches!(c, '_' | '-'))
        .collect();
    if !plain_prefix.is_empty() && starts.starts_with(&plain_prefix) {
        return Some(2);
    }
    let mut remaining = lower_label.chars();
    lower_prefix
        .chars()
        .all(|wanted| remaining.any(|character| character == wanted))
        .then_some(3)
}

/// Ranks suggestions from several providers. `sources` lists each
/// provider's items, best provider first (snippets, keywords, words);
/// within a provider, earlier items rank higher (the word provider lists
/// the nearest words first). Items equal to the prefix are left out, and
/// an inserted text appears once.
#[must_use]
pub fn rank(prefix: &str, sources: Vec<Vec<CompletionItem>>) -> Vec<CompletionItem> {
    let mut scored: Vec<(u8, usize, usize, CompletionItem)> = Vec::new();
    for (source, items) in sources.into_iter().enumerate() {
        for (position, item) in items.into_iter().enumerate() {
            if item.label == prefix && !item.snippet {
                continue;
            }
            if let Some(tier) = match_tier(&item.label, prefix) {
                scored.push((tier, source, position, item));
            }
        }
    }
    scored.sort_by(|left, right| {
        (left.0, left.1, left.2, &left.3.label).cmp(&(right.0, right.1, right.2, &right.3.label))
    });
    let mut seen = std::collections::BTreeSet::new();
    scored
        .into_iter()
        .filter(|(_, _, _, item)| seen.insert((item.insert.clone(), item.snippet)))
        .map(|(_, _, _, item)| item)
        .take(MAX_COMPLETION_ITEMS)
        .collect()
}

/// A snippet ready to insert.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedSnippet {
    pub text: String,
    /// Placeholders in visiting order (`$1`, `$2`, …, then `$0`), as byte
    /// ranges of `text`; `$0` (or the end) is always last and empty.
    pub stops: Vec<(usize, usize)>,
}

/// Expands a snippet body: placeholders get their default text, `\n`
/// becomes `line_ending` plus `line_indent`, and `\t` becomes `unit`.
#[must_use]
pub fn expand_snippet(
    body: &str,
    line_ending: &str,
    line_indent: &str,
    unit: &str,
) -> ExpandedSnippet {
    let mut text = String::with_capacity(body.len());
    let mut stops: std::collections::BTreeMap<u32, (usize, usize)> =
        std::collections::BTreeMap::new();
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                if let Some(next) = characters.next() {
                    text.push(next);
                }
            }
            '\n' => {
                text.push_str(line_ending);
                text.push_str(line_indent);
            }
            '\t' => text.push_str(unit),
            '$' if characters.peek().is_some_and(char::is_ascii_digit) => {
                let mut number = String::new();
                while let Some(digit) = characters.next_if(char::is_ascii_digit) {
                    number.push(digit);
                }
                let number: u32 = number.parse().unwrap_or(0);
                if let Some(&(start, end)) = stops.get(&number) {
                    // A repeated placeholder repeats its text (`$1` after `${1:i}`).
                    let repeated = text[start..end].to_owned();
                    text.push_str(&repeated);
                } else {
                    let position = text.len();
                    stops.insert(number, (position, position));
                }
            }
            '$' if characters.peek() == Some(&'{') => {
                characters.next();
                let mut number = String::new();
                while let Some(digit) = characters.next_if(char::is_ascii_digit) {
                    number.push(digit);
                }
                let start = text.len();
                if characters.next_if_eq(&':').is_some() {
                    while let Some(inner) = characters.next() {
                        match inner {
                            '}' => break,
                            '\\' => {
                                if let Some(next) = characters.next() {
                                    text.push(next);
                                }
                            }
                            other => text.push(other),
                        }
                    }
                } else {
                    characters.next_if_eq(&'}');
                }
                stops
                    .entry(number.parse().unwrap_or(0))
                    .or_insert((start, text.len()));
            }
            other => text.push(other),
        }
    }
    let end = text.len();
    let last = stops
        .remove(&0)
        .map_or((end, end), |(start, _)| (start, start));
    let mut ordered: Vec<(usize, usize)> = stops.into_values().collect();
    ordered.push(last);
    ExpandedSnippet {
        text,
        stops: ordered,
    }
}

const fn snippet(prefix: &'static str, body: &'static str, description: &'static str) -> Snippet {
    Snippet {
        prefix,
        body,
        description,
    }
}

pub const RUST_SNIPPETS: &[Snippet] = &[
    snippet("fn", "fn ${1:name}($2) {\n\t$0\n}", "Function"),
    snippet("if", "if ${1:condition} {\n\t$0\n}", "If"),
    snippet("for", "for ${1:item} in ${2:items} {\n\t$0\n}", "For loop"),
    snippet("match", "match ${1:value} {\n\t${2:_} => $0,\n}", "Match"),
    snippet("impl", "impl ${1:Type} {\n\t$0\n}", "Impl block"),
    snippet("struct", "struct ${1:Name} {\n\t$0\n}", "Struct"),
    snippet("test", "#[test]\nfn ${1:name}() {\n\t$0\n}", "Test"),
];

pub const JAVASCRIPT_SNIPPETS: &[Snippet] = &[
    snippet("function", "function ${1:name}($2) {\n\t$0\n}", "Function"),
    snippet(
        "arrow",
        "const ${1:name} = ($2) => {\n\t$0\n};",
        "Arrow function",
    ),
    snippet("if", "if (${1:condition}) {\n\t$0\n}", "If"),
    snippet(
        "for",
        "for (let ${1:i} = 0; $1 < ${2:length}; $1++) {\n\t$0\n}",
        "For loop",
    ),
    snippet(
        "forof",
        "for (const ${1:item} of ${2:items}) {\n\t$0\n}",
        "For…of loop",
    ),
    snippet(
        "class",
        "class ${1:Name} {\n\tconstructor($2) {\n\t\t$0\n\t}\n}",
        "Class",
    ),
    snippet("log", "console.log($0);", "Log to the console"),
];

pub const PYTHON_SNIPPETS: &[Snippet] = &[
    snippet("def", "def ${1:name}($2):\n\t${0:pass}", "Function"),
    snippet(
        "class",
        "class ${1:Name}:\n\tdef __init__(self$2):\n\t\t${0:pass}",
        "Class",
    ),
    snippet("if", "if ${1:condition}:\n\t${0:pass}", "If"),
    snippet(
        "for",
        "for ${1:item} in ${2:items}:\n\t${0:pass}",
        "For loop",
    ),
    snippet("while", "while ${1:condition}:\n\t${0:pass}", "While loop"),
    snippet(
        "try",
        "try:\n\t${1:pass}\nexcept ${2:Exception} as ${3:error}:\n\t${0:raise}",
        "Try/except",
    ),
    snippet(
        "with",
        "with ${1:expression} as ${2:name}:\n\t${0:pass}",
        "With",
    ),
    snippet(
        "main",
        "if __name__ == \"__main__\":\n\t${0:main()}",
        "Run as a script",
    ),
];

pub const GO_SNIPPETS: &[Snippet] = &[
    snippet("func", "func ${1:name}($2) {\n\t$0\n}", "Function"),
    snippet("if", "if ${1:condition} {\n\t$0\n}", "If"),
    snippet(
        "for",
        "for ${1:i} := 0; $1 < ${2:n}; $1++ {\n\t$0\n}",
        "For loop",
    ),
    snippet(
        "iferr",
        "if err != nil {\n\treturn ${0:err}\n}",
        "Return on error",
    ),
];

pub const C_FAMILY_SNIPPETS: &[Snippet] = &[
    snippet("if", "if (${1:condition}) {\n\t$0\n}", "If"),
    snippet(
        "for",
        "for (int ${1:i} = 0; $1 < ${2:n}; $1++) {\n\t$0\n}",
        "For loop",
    ),
    snippet("while", "while (${1:condition}) {\n\t$0\n}", "While loop"),
];

pub const HTML_SNIPPETS: &[Snippet] = &[snippet(
    "html5",
    "<!DOCTYPE html>\n<html lang=\"${1:en}\">\n<head>\n\t<meta charset=\"utf-8\">\n\t<title>${2:Title}</title>\n</head>\n<body>\n\t$0\n</body>\n</html>",
    "HTML document",
)];

pub const SQL_SNIPPETS: &[Snippet] = &[
    snippet(
        "select",
        "SELECT ${1:*}\nFROM ${2:table}\nWHERE ${0:condition};",
        "Select rows",
    ),
    snippet(
        "insert",
        "INSERT INTO ${1:table} (${2:columns})\nVALUES (${0:values});",
        "Insert a row",
    ),
    snippet(
        "update",
        "UPDATE ${1:table}\nSET ${2:column} = ${3:value}\nWHERE ${0:condition};",
        "Update rows",
    ),
];

pub const BASH_SNIPPETS: &[Snippet] = &[
    snippet("if", "if [ ${1:condition} ]; then\n\t$0\nfi", "If"),
    snippet(
        "for",
        "for ${1:item} in ${2:items}; do\n\t$0\ndone",
        "For loop",
    ),
    snippet("func", "${1:name}() {\n\t$0\n}", "Function"),
];

pub const RUBY_SNIPPETS: &[Snippet] = &[
    snippet("def", "def ${1:name}\n\t$0\nend", "Method"),
    snippet("class", "class ${1:Name}\n\t$0\nend", "Class"),
    snippet("each", "${1:items}.each do |${2:item}|\n\t$0\nend", "Each"),
];

pub const LUA_SNIPPETS: &[Snippet] = &[
    snippet("function", "function ${1:name}($2)\n\t$0\nend", "Function"),
    snippet("if", "if ${1:condition} then\n\t$0\nend", "If"),
    snippet(
        "for",
        "for ${1:i} = ${2:1}, ${3:n} do\n\t$0\nend",
        "For loop",
    ),
];

#[cfg(test)]
mod tests;
