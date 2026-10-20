//! Bounded, storage-independent language modules: Tree-sitter grammars for a
//! few languages and table-driven lexers for the rest.

mod lexer;

use serde::Serialize;
use std::{collections::BTreeMap, path::Path, time::Duration};
use thiserror::Error;
use tree_sitter::{Language, Node, Parser};

pub const MAX_SYNTAX_BYTES: usize = 256 * 1024;
pub const MAX_SYNTAX_NODES: usize = 100_000;
pub const MAX_HIGHLIGHT_SPANS: usize = 20_000;
pub const MAX_FOLD_RANGES: usize = 2_000;
pub const MAX_SYMBOLS: usize = 2_000;
pub const MAX_BRACKET_PAIRS: usize = 20_000;
pub const MAX_PARSE_TIME: Duration = Duration::from_millis(25);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct LanguageId(String);

impl LanguageId {
    /// Creates a validated, stable language identifier.
    ///
    /// # Errors
    /// Returns an error when the ID is empty, too long, or contains unsupported characters.
    pub fn new(value: impl Into<String>) -> Result<Self, SyntaxError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
            })
        {
            return Err(SyntaxError::InvalidLanguageId(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyntaxToken {
    Comment,
    Keyword,
    String,
    Number,
    Function,
    Type,
    Property,
    Escape,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighlightSpan {
    pub start_byte: u64,
    pub end_byte: u64,
    pub token: SyntaxToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldRange {
    pub start_byte: u64,
    pub end_byte: u64,
    pub start_line: u64,
    pub end_line: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Property,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_byte: u64,
    pub line: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BracketPair {
    pub open_byte: u64,
    pub close_byte: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxAnalysis {
    pub language_id: LanguageId,
    pub parsed_bytes: usize,
    pub highlights: Vec<HighlightSpan>,
    pub folds: Vec<FoldRange>,
    pub symbols: Vec<DocumentSymbol>,
    pub bracket_pairs: Vec<BracketPair>,
    pub truncated: bool,
    pub timed_out: bool,
}

impl SyntaxAnalysis {
    fn plain_text(parsed_bytes: usize, truncated: bool) -> Self {
        Self {
            language_id: LanguageId("plaintext".into()),
            parsed_bytes,
            highlights: Vec::new(),
            folds: Vec::new(),
            symbols: Vec::new(),
            bracket_pairs: Vec::new(),
            truncated,
            timed_out: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageDescriptor {
    pub id: LanguageId,
    pub display_name: String,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenRule {
    pub node_kind: &'static str,
    pub token: SyntaxToken,
}

#[derive(Clone)]
pub struct LanguageModule {
    descriptor: LanguageDescriptor,
    grammar: Option<Language>,
    token_rules: Vec<TokenRule>,
    fold_node_kinds: Vec<&'static str>,
    symbol_node_kind: Option<&'static str>,
    bracket_node_kinds: Vec<&'static str>,
    lexer: Option<&'static lexer::LexerSpec>,
}

impl std::fmt::Debug for LanguageModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LanguageModule")
            .field("descriptor", &self.descriptor)
            .field("has_grammar", &self.grammar.is_some())
            .field("token_rules", &self.token_rules)
            .field("fold_node_kinds", &self.fold_node_kinds)
            .field("symbol_node_kind", &self.symbol_node_kind)
            .field("bracket_node_kinds", &self.bracket_node_kinds)
            .field("lexer", &self.lexer.map(|spec| spec.id))
            .finish()
    }
}

impl LanguageModule {
    #[must_use]
    pub fn tree_sitter(
        descriptor: LanguageDescriptor,
        grammar: Language,
        token_rules: Vec<TokenRule>,
        fold_node_kinds: Vec<&'static str>,
        symbol_node_kind: Option<&'static str>,
        bracket_node_kinds: Vec<&'static str>,
    ) -> Self {
        Self {
            descriptor,
            grammar: Some(grammar),
            token_rules,
            fold_node_kinds,
            symbol_node_kind,
            bracket_node_kinds,
            lexer: None,
        }
    }

    fn lexical(spec: &'static lexer::LexerSpec) -> Self {
        let owned = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        Self {
            descriptor: LanguageDescriptor {
                id: LanguageId(spec.id.into()),
                display_name: spec.name.into(),
                extensions: owned(spec.extensions),
                filenames: owned(spec.filenames),
            },
            grammar: None,
            token_rules: Vec::new(),
            fold_node_kinds: Vec::new(),
            symbol_node_kind: None,
            bracket_node_kinds: Vec::new(),
            lexer: Some(spec),
        }
    }

    #[must_use]
    pub fn descriptor(&self) -> &LanguageDescriptor {
        &self.descriptor
    }
}

/// How a language writes comments, for the Toggle Comment command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommentSyntax {
    /// A marker that comments out the rest of a line (`//`, `#`, `--`).
    pub line: Option<&'static str>,
    /// Markers around a comment, used when there is no line marker (`<!--`, `-->`).
    pub block: Option<(&'static str, &'static str)>,
}

#[derive(Debug)]
pub struct LanguageRegistry {
    modules: BTreeMap<LanguageId, LanguageModule>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl LanguageRegistry {
    #[must_use]
    pub fn with_builtins() -> Self {
        let json = json_module();
        let python = python_module();
        let markdown = markdown_module();
        let mut modules = BTreeMap::from([
            (json.descriptor.id.clone(), json),
            (python.descriptor.id.clone(), python),
            (markdown.descriptor.id.clone(), markdown),
        ]);
        for spec in lexer::SPECS {
            let module = LanguageModule::lexical(spec);
            modules.insert(module.descriptor.id.clone(), module);
        }
        Self { modules }
    }

    /// Registers a language module.
    ///
    /// # Errors
    /// Returns an error when a module with the same ID is already registered.
    pub fn register(&mut self, module: LanguageModule) -> Result<(), SyntaxError> {
        let id = module.descriptor.id.clone();
        if self.modules.contains_key(&id) {
            return Err(SyntaxError::DuplicateLanguage(id));
        }
        self.modules.insert(id, module);
        Ok(())
    }

    /// The comment markers of a language, or `None` when it has none
    /// (plain text, JSON).
    #[must_use]
    pub fn comment_syntax(&self, id: &LanguageId) -> Option<CommentSyntax> {
        let module = self.modules.get(id)?;
        let syntax = match (module.lexer, id.as_str()) {
            (Some(spec), _) => CommentSyntax {
                line: spec.line_comments.first().copied(),
                // Markup comments are recognized by the markup scanner itself.
                block: spec.block_comments.first().copied().or(matches!(
                    spec.mode,
                    lexer::Mode::Markup
                )
                .then_some(("<!--", "-->"))),
            },
            (None, "python") => CommentSyntax {
                line: Some("#"),
                block: None,
            },
            (None, "markdown") => CommentSyntax {
                line: None,
                block: Some(("<!--", "-->")),
            },
            (None, _) => return None,
        };
        (syntax.line.is_some() || syntax.block.is_some()).then_some(syntax)
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<LanguageDescriptor> {
        let mut descriptors: Vec<_> = self
            .modules
            .values()
            .map(|module| module.descriptor.clone())
            .collect();
        // Listed the way a picker shows them: by name, after Plain Text.
        descriptors.sort_by_key(|descriptor| descriptor.display_name.to_lowercase());
        descriptors.insert(0, plain_text_descriptor());
        descriptors
    }

    /// Guesses a language from the first bytes of a file whose name says
    /// nothing: a `#!` interpreter line, an XML or PHP prolog, or an HTML doctype.
    #[must_use]
    pub fn detect_content(&self, head: &[u8]) -> Option<LanguageId> {
        let id = LanguageId(lexer::detect_content(head)?.into());
        self.modules.contains_key(&id).then_some(id)
    }

    #[must_use]
    pub fn detect_path(&self, path: &Path) -> LanguageId {
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        self.modules
            .values()
            .find(|module| {
                module
                    .descriptor
                    .filenames
                    .iter()
                    .any(|item| item == &filename)
                    || module
                        .descriptor
                        .extensions
                        .iter()
                        .any(|item| item == &extension)
            })
            .map_or_else(
                || LanguageId("plaintext".into()),
                |module| module.descriptor.id.clone(),
            )
    }

    /// Parses one bounded source window and derives stable editor features.
    /// `base_byte` and `base_line` place results into document coordinates.
    ///
    /// # Errors
    /// Returns an error for an unknown language, coordinate overflow, or parser setup failure.
    pub fn analyze(
        &self,
        language_id: &LanguageId,
        source: &[u8],
        base_byte: u64,
        base_line: u64,
    ) -> Result<SyntaxAnalysis, SyntaxError> {
        let parsed_len = source.len().min(MAX_SYNTAX_BYTES);
        let truncated = source.len() > parsed_len;
        if language_id.as_str() == "plaintext" {
            return Ok(SyntaxAnalysis::plain_text(parsed_len, truncated));
        }
        if language_id.as_str() == "markdown" {
            return analyze_markdown(&source[..parsed_len], base_byte, truncated);
        }
        let module = self
            .modules
            .get(language_id)
            .ok_or_else(|| SyntaxError::UnknownLanguage(language_id.clone()))?;
        if let Some(spec) = module.lexer {
            return Ok(lexer::analyze(
                spec,
                &source[..parsed_len],
                base_byte,
                base_line,
                truncated,
            ));
        }
        analyze_tree_sitter(
            module,
            &source[..parsed_len],
            base_byte,
            base_line,
            truncated,
        )
    }
}

fn plain_text_descriptor() -> LanguageDescriptor {
    LanguageDescriptor {
        id: LanguageId("plaintext".into()),
        display_name: "Plain Text".into(),
        extensions: Vec::new(),
        filenames: Vec::new(),
    }
}

fn json_module() -> LanguageModule {
    LanguageModule::tree_sitter(
        LanguageDescriptor {
            id: LanguageId("json".into()),
            display_name: "JSON".into(),
            extensions: vec!["json".into()],
            filenames: vec![".babelrc".into(), ".eslintrc".into()],
        },
        tree_sitter_json::LANGUAGE.into(),
        vec![
            TokenRule {
                node_kind: "string",
                token: SyntaxToken::String,
            },
            TokenRule {
                node_kind: "escape_sequence",
                token: SyntaxToken::Escape,
            },
            TokenRule {
                node_kind: "number",
                token: SyntaxToken::Number,
            },
            TokenRule {
                node_kind: "true",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "false",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "null",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "comment",
                token: SyntaxToken::Comment,
            },
            TokenRule {
                node_kind: "ERROR",
                token: SyntaxToken::Invalid,
            },
        ],
        vec!["object", "array"],
        Some("pair"),
        vec!["object", "array"],
    )
}

fn python_module() -> LanguageModule {
    LanguageModule::tree_sitter(
        LanguageDescriptor {
            id: LanguageId("python".into()),
            display_name: "Python".into(),
            extensions: vec!["py".into(), "pyw".into(), "pyi".into()],
            filenames: vec!["sconstruct".into(), "sconscript".into()],
        },
        tree_sitter_python::LANGUAGE.into(),
        vec![
            TokenRule {
                node_kind: "comment",
                token: SyntaxToken::Comment,
            },
            TokenRule {
                node_kind: "string",
                token: SyntaxToken::String,
            },
            TokenRule {
                node_kind: "escape_sequence",
                token: SyntaxToken::Escape,
            },
            TokenRule {
                node_kind: "integer",
                token: SyntaxToken::Number,
            },
            TokenRule {
                node_kind: "float",
                token: SyntaxToken::Number,
            },
            TokenRule {
                node_kind: "true",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "false",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "none",
                token: SyntaxToken::Keyword,
            },
            TokenRule {
                node_kind: "ERROR",
                token: SyntaxToken::Invalid,
            },
        ],
        vec!["function_definition", "class_definition"],
        None,
        Vec::new(),
    )
}

fn markdown_module() -> LanguageModule {
    LanguageModule {
        descriptor: LanguageDescriptor {
            id: LanguageId("markdown".into()),
            display_name: "Markdown".into(),
            extensions: vec!["md".into(), "markdown".into(), "mdown".into()],
            filenames: Vec::new(),
        },
        grammar: None,
        token_rules: Vec::new(),
        fold_node_kinds: Vec::new(),
        symbol_node_kind: None,
        bracket_node_kinds: Vec::new(),
        lexer: None,
    }
}

fn analyze_markdown(
    source: &[u8],
    base_byte: u64,
    truncated: bool,
) -> Result<SyntaxAnalysis, SyntaxError> {
    let mut analysis = SyntaxAnalysis {
        language_id: LanguageId("markdown".into()),
        parsed_bytes: source.len(),
        highlights: Vec::new(),
        folds: Vec::new(),
        symbols: Vec::new(),
        bracket_pairs: Vec::new(),
        truncated,
        timed_out: false,
    };
    let mut cursor = 0_usize;
    let mut fenced = false;
    while cursor < source.len() {
        let line_end = source[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(source.len(), |relative| cursor + relative);
        let line = &source[cursor..line_end];
        let indent = line.iter().take_while(|byte| **byte == b' ').count().min(4);
        let trimmed = &line[indent..];
        let fence = trimmed.starts_with(b"```") || trimmed.starts_with(b"~~~");
        let token = if fence {
            fenced = !fenced;
            Some(SyntaxToken::Keyword)
        } else if fenced {
            Some(SyntaxToken::String)
        } else if trimmed.starts_with(b"> ") || trimmed == b">" {
            Some(SyntaxToken::Comment)
        } else {
            let hashes = trimmed.iter().take_while(|byte| **byte == b'#').count();
            if (1..=6).contains(&hashes) && trimmed.get(hashes) == Some(&b' ') {
                Some(SyntaxToken::Type)
            } else {
                None
            }
        };
        if let Some(token) = token {
            push_markdown_span(&mut analysis, base_byte, cursor + indent, line_end, token)?;
        } else {
            if trimmed.len() > 1 && matches!(trimmed[0], b'-' | b'+' | b'*') && trimmed[1] == b' ' {
                push_markdown_span(
                    &mut analysis,
                    base_byte,
                    cursor + indent,
                    cursor + indent + 1,
                    SyntaxToken::Keyword,
                )?;
            }
            let mut position = 0;
            while position < line.len() {
                if line[position] == b'`' {
                    if let Some(end) = line[position + 1..].iter().position(|byte| *byte == b'`') {
                        let end = position + end + 2;
                        push_markdown_span(
                            &mut analysis,
                            base_byte,
                            cursor + position,
                            cursor + end,
                            SyntaxToken::String,
                        )?;
                        position = end;
                        continue;
                    }
                }
                if line[position..].starts_with(b"](") {
                    if let Some(end) = line[position + 2..].iter().position(|byte| *byte == b')') {
                        push_markdown_span(
                            &mut analysis,
                            base_byte,
                            cursor + position + 2,
                            cursor + position + end + 2,
                            SyntaxToken::Function,
                        )?;
                        position += end + 3;
                        continue;
                    }
                }
                position += 1;
            }
        }
        cursor = if line_end < source.len() {
            line_end + 1
        } else {
            line_end
        };
    }
    Ok(analysis)
}

fn push_markdown_span(
    analysis: &mut SyntaxAnalysis,
    base_byte: u64,
    start: usize,
    end: usize,
    token: SyntaxToken,
) -> Result<(), SyntaxError> {
    if start >= end {
        return Ok(());
    }
    if analysis.highlights.len() >= MAX_HIGHLIGHT_SPANS {
        analysis.truncated = true;
        return Ok(());
    }
    analysis.highlights.push(HighlightSpan {
        start_byte: base_byte
            .checked_add(u64::try_from(start).map_err(|_| SyntaxError::OffsetOverflow)?)
            .ok_or(SyntaxError::OffsetOverflow)?,
        end_byte: base_byte
            .checked_add(u64::try_from(end).map_err(|_| SyntaxError::OffsetOverflow)?)
            .ok_or(SyntaxError::OffsetOverflow)?,
        token,
    });
    Ok(())
}

fn analyze_tree_sitter(
    module: &LanguageModule,
    source: &[u8],
    base_byte: u64,
    base_line: u64,
    input_truncated: bool,
) -> Result<SyntaxAnalysis, SyntaxError> {
    let mut parser = Parser::new();
    let grammar = module.grammar.as_ref().ok_or(SyntaxError::MissingGrammar)?;
    parser
        .set_language(grammar)
        .map_err(|error| SyntaxError::Parser(error.to_string()))?;
    parser.set_timeout_micros(
        u64::try_from(MAX_PARSE_TIME.as_micros()).map_err(|_| SyntaxError::OffsetOverflow)?,
    );
    let Some(tree) = parser.parse(source, None) else {
        return Ok(SyntaxAnalysis {
            language_id: module.descriptor.id.clone(),
            parsed_bytes: source.len(),
            highlights: Vec::new(),
            folds: Vec::new(),
            symbols: Vec::new(),
            bracket_pairs: Vec::new(),
            truncated: true,
            timed_out: true,
        });
    };
    let mut analysis = SyntaxAnalysis {
        language_id: module.descriptor.id.clone(),
        parsed_bytes: source.len(),
        highlights: Vec::new(),
        folds: Vec::new(),
        symbols: Vec::new(),
        bracket_pairs: Vec::new(),
        truncated: input_truncated,
        timed_out: false,
    };
    let mut visited = 0_usize;
    walk_tree(tree.root_node(), |node| {
        visited = visited.saturating_add(1);
        if visited > MAX_SYNTAX_NODES {
            analysis.truncated = true;
            return false;
        }
        collect_node(module, source, node, base_byte, base_line, &mut analysis);
        true
    });
    Ok(analysis)
}

fn walk_tree(mut node: Node<'_>, mut visit: impl FnMut(Node<'_>) -> bool) {
    let mut cursor = node.walk();
    loop {
        if !visit(node) {
            return;
        }
        if cursor.goto_first_child() {
            node = cursor.node();
            continue;
        }
        if cursor.goto_next_sibling() {
            node = cursor.node();
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return;
            }
            if cursor.goto_next_sibling() {
                node = cursor.node();
                break;
            }
        }
    }
}

fn collect_node(
    module: &LanguageModule,
    source: &[u8],
    node: Node<'_>,
    base_byte: u64,
    base_line: u64,
    analysis: &mut SyntaxAnalysis,
) {
    let kind = node.kind();
    if analysis.highlights.len() < MAX_HIGHLIGHT_SPANS {
        let token = semantic_token(module.descriptor.id.as_str(), node).or_else(|| {
            module
                .token_rules
                .iter()
                .find(|rule| rule.node_kind == kind)
                .map(|rule| rule.token)
        });
        if let Some(token) = token {
            if let Some((start_byte, end_byte)) = absolute_range(node, base_byte) {
                analysis.highlights.push(HighlightSpan {
                    start_byte,
                    end_byte,
                    token,
                });
            }
        }
    } else {
        analysis.truncated = true;
    }
    if module.fold_node_kinds.contains(&kind) && node.end_position().row > node.start_position().row
    {
        if analysis.folds.len() < MAX_FOLD_RANGES {
            if let (Some((start_byte, end_byte)), Some(start_line), Some(end_line)) = (
                absolute_range(node, base_byte),
                absolute_line(base_line, node.start_position().row),
                absolute_line(base_line, node.end_position().row),
            ) {
                analysis.folds.push(FoldRange {
                    start_byte,
                    end_byte,
                    start_line,
                    end_line,
                });
            }
        } else {
            analysis.truncated = true;
        }
    }
    if Some(kind) == module.symbol_node_kind {
        collect_symbol(source, node, base_byte, base_line, analysis);
    }
    if module.bracket_node_kinds.contains(&kind) && node.end_byte() > node.start_byte() + 1 {
        if analysis.bracket_pairs.len() < MAX_BRACKET_PAIRS {
            if let Some((open_byte, end_byte)) = absolute_range(node, base_byte) {
                analysis.bracket_pairs.push(BracketPair {
                    open_byte,
                    close_byte: end_byte - 1,
                });
            }
        } else {
            analysis.truncated = true;
        }
    }
}

fn semantic_token(language_id: &str, node: Node<'_>) -> Option<SyntaxToken> {
    if language_id == "python" {
        return python_token(node);
    }
    let parent = node.parent()?;
    (node.kind() == "string"
        && parent.kind() == "pair"
        && parent.child_by_field_name("key") == Some(node))
    .then_some(SyntaxToken::Property)
}

fn python_token(node: Node<'_>) -> Option<SyntaxToken> {
    const KEYWORDS: &[&str] = &[
        "as", "assert", "async", "await", "break", "case", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "match", "nonlocal", "not", "or", "and", "pass", "raise", "return", "try",
        "while", "with", "yield",
    ];
    if KEYWORDS.contains(&node.kind()) {
        return Some(SyntaxToken::Keyword);
    }
    if node.kind() != "identifier" {
        return None;
    }
    let parent = node.parent()?;
    match parent.kind() {
        "function_definition" if parent.child_by_field_name("name") == Some(node) => {
            Some(SyntaxToken::Function)
        }
        "class_definition" if parent.child_by_field_name("name") == Some(node) => {
            Some(SyntaxToken::Type)
        }
        "call" if parent.child_by_field_name("function") == Some(node) => {
            Some(SyntaxToken::Function)
        }
        "attribute" if parent.child_by_field_name("attribute") == Some(node) => {
            Some(SyntaxToken::Property)
        }
        "type" => Some(SyntaxToken::Type),
        _ => None,
    }
}

fn collect_symbol(
    source: &[u8],
    node: Node<'_>,
    base_byte: u64,
    base_line: u64,
    analysis: &mut SyntaxAnalysis,
) {
    if analysis.symbols.len() == MAX_SYMBOLS {
        analysis.truncated = true;
        return;
    }
    let Some(key) = node.child_by_field_name("key") else {
        return;
    };
    let Some((start_byte, _)) = absolute_range(key, base_byte) else {
        analysis.truncated = true;
        return;
    };
    let Some(line) = absolute_line(base_line, key.start_position().row) else {
        analysis.truncated = true;
        return;
    };
    let name = key
        .utf8_text(source)
        .unwrap_or_default()
        .trim_matches('"')
        .to_owned();
    analysis.symbols.push(DocumentSymbol {
        name,
        kind: SymbolKind::Property,
        start_byte,
        line,
    });
}

fn absolute_range(node: Node<'_>, base_byte: u64) -> Option<(u64, u64)> {
    Some((
        base_byte.checked_add(u64::try_from(node.start_byte()).ok()?)?,
        base_byte.checked_add(u64::try_from(node.end_byte()).ok()?)?,
    ))
}

fn absolute_line(base_line: u64, relative_row: usize) -> Option<u64> {
    base_line.checked_add(u64::try_from(relative_row).ok()?)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SyntaxError {
    #[error("invalid language identifier {0:?}")]
    InvalidLanguageId(String),
    #[error("language {0:?} is already registered")]
    DuplicateLanguage(LanguageId),
    #[error("unknown language {0:?}")]
    UnknownLanguage(LanguageId),
    #[error("language module has no parser grammar")]
    MissingGrammar,
    #[error("Tree-sitter parser setup failed: {0}")]
    Parser(String),
    #[error("syntax coordinate overflow")]
    OffsetOverflow,
}

#[cfg(test)]
mod tests {
    use super::{LanguageId, LanguageRegistry, SyntaxToken, MAX_HIGHLIGHT_SPANS, MAX_SYNTAX_BYTES};
    use std::path::Path;

    #[test]
    fn comment_syntax_comes_from_each_language() {
        let registry = LanguageRegistry::with_builtins();
        let line = |id: &str| {
            registry
                .comment_syntax(&LanguageId::new(id).expect("id"))
                .and_then(|syntax| syntax.line)
        };
        assert_eq!(line("rust"), Some("//"));
        assert_eq!(line("python"), Some("#"));
        assert_eq!(line("sql"), Some("--"));
        assert_eq!(
            registry
                .comment_syntax(&LanguageId::new("html").expect("id"))
                .and_then(|syntax| syntax.block),
            Some(("<!--", "-->"))
        );
        assert_eq!(
            registry.comment_syntax(&LanguageId::new("json").expect("id")),
            None
        );
        assert_eq!(
            registry.comment_syntax(&LanguageId::new("plaintext").expect("id")),
            None
        );
    }

    #[test]
    fn language_ids_and_registration_are_stable() {
        assert!(LanguageId::new("json").is_ok());
        assert!(LanguageId::new("Type Script").is_err());
        let registry = LanguageRegistry::default();
        assert_eq!(registry.descriptors()[0].id.as_str(), "plaintext");
        assert_eq!(
            registry.detect_path(Path::new("settings.JSON")).as_str(),
            "json"
        );
        assert_eq!(
            registry.detect_path(Path::new("main.PY")).as_str(),
            "python"
        );
        assert_eq!(
            registry.detect_path(Path::new("README.md")).as_str(),
            "markdown"
        );
        assert_eq!(
            registry.detect_path(Path::new("notes.txt")).as_str(),
            "plaintext"
        );
    }

    #[test]
    fn json_analysis_produces_highlights_folds_symbols_and_brackets() {
        let registry = LanguageRegistry::default();
        let source = br#"{
  "name": "Nulis",
  "count": 42,
  "nested": { "enabled": true }
}"#;
        let analysis = registry
            .analyze(&LanguageId::new("json").expect("ID"), source, 100, 20)
            .expect("analysis");
        assert_eq!(analysis.language_id.as_str(), "json");
        assert!(analysis
            .highlights
            .iter()
            .any(|span| span.token == SyntaxToken::Property));
        assert!(analysis
            .highlights
            .iter()
            .any(|span| span.token == SyntaxToken::Number));
        assert_eq!(analysis.folds[0].start_line, 20);
        assert_eq!(analysis.folds[0].end_line, 24);
        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "nested"));
        assert_eq!(analysis.bracket_pairs[0].open_byte, 100);
        assert_eq!(analysis.bracket_pairs[0].close_byte, 168);
        assert!(!analysis.timed_out);
    }

    #[test]
    fn malformed_json_still_returns_recovery_highlights() {
        let registry = LanguageRegistry::default();
        let analysis = registry
            .analyze(
                &LanguageId::new("json").expect("ID"),
                br#"{"broken": [1, }"#,
                0,
                1,
            )
            .expect("analysis");
        assert!(analysis
            .highlights
            .iter()
            .any(|span| span.token == SyntaxToken::Invalid));
    }

    #[test]
    fn python_analysis_colors_language_constructs() {
        let registry = LanguageRegistry::default();
        let source = b"# note\nclass Widget:\n    def run(self):\n        return print(42, 'ok')\n";
        let analysis = registry
            .analyze(&LanguageId::new("python").expect("ID"), source, 0, 1)
            .expect("Python analysis");
        for token in [
            SyntaxToken::Comment,
            SyntaxToken::Keyword,
            SyntaxToken::Type,
            SyntaxToken::Function,
            SyntaxToken::Number,
            SyntaxToken::String,
        ] {
            assert!(
                analysis.highlights.iter().any(|span| span.token == token),
                "missing {token:?}"
            );
        }
        assert_eq!(analysis.folds.len(), 2);
        assert!(!analysis.timed_out);
    }

    #[test]
    fn markdown_analysis_colors_headings_quotes_code_and_links() {
        let registry = LanguageRegistry::default();
        let source = b"# Readme\n> Important note\n- item with `code` and [link](https://example.test)\n```python\nprint(1)\n```\n";
        let analysis = registry
            .analyze(&LanguageId::new("markdown").expect("ID"), source, 10, 1)
            .expect("Markdown analysis");
        assert_eq!(analysis.language_id.as_str(), "markdown");
        for token in [
            SyntaxToken::Type,
            SyntaxToken::Comment,
            SyntaxToken::Keyword,
            SyntaxToken::String,
            SyntaxToken::Function,
        ] {
            assert!(
                analysis.highlights.iter().any(|span| span.token == token),
                "missing {token:?}"
            );
        }
        assert!(analysis
            .highlights
            .iter()
            .all(|span| span.start_byte >= 10 && span.end_byte <= 10 + source.len() as u64));
    }

    #[test]
    fn syntax_input_and_outputs_have_hard_bounds() {
        let registry = LanguageRegistry::default();
        let mut source = Vec::from(b"[".as_slice());
        source.extend(std::iter::repeat_n(b'0', MAX_SYNTAX_BYTES + 1024));
        let analysis = registry
            .analyze(&LanguageId::new("json").expect("ID"), &source, 0, 1)
            .expect("bounded analysis");
        assert_eq!(analysis.parsed_bytes, MAX_SYNTAX_BYTES);
        assert!(analysis.highlights.len() <= MAX_HIGHLIGHT_SPANS);
        assert!(analysis.truncated);
    }

    fn tokens(language: &str, source: &str) -> Vec<(String, SyntaxToken)> {
        let registry = LanguageRegistry::default();
        let analysis = registry
            .analyze(
                &LanguageId::new(language).expect("id"),
                source.as_bytes(),
                0,
                1,
            )
            .expect("analysis");
        assert_eq!(analysis.language_id.as_str(), language);
        analysis
            .highlights
            .iter()
            .map(|span| {
                let range = usize::try_from(span.start_byte).expect("start")
                    ..usize::try_from(span.end_byte).expect("end");
                (source[range].to_owned(), span.token)
            })
            .collect()
    }

    fn has(tokens: &[(String, SyntaxToken)], text: &str, token: SyntaxToken) -> bool {
        tokens
            .iter()
            .any(|(item, kind)| item == text && *kind == token)
    }

    #[test]
    fn popular_file_names_are_detected() {
        let registry = LanguageRegistry::default();
        for (path, expected) in [
            ("run.applescript", "applescript"),
            ("deploy.sh", "bash"),
            (".zshrc", "bash"),
            ("build.BAT", "batch"),
            ("main.c", "c"),
            ("util.h", "c"),
            ("engine.cpp", "cpp"),
            ("engine.hpp", "cpp"),
            ("Program.cs", "csharp"),
            ("site.css", "css"),
            ("theme.scss", "css"),
            ("index.html", "html"),
            ("Main.java", "java"),
            ("app.js", "javascript"),
            ("app.mjs", "javascript"),
            ("package.json", "json"),
            ("README.md", "markdown"),
            ("schema.sql", "sql"),
            ("dump.mysql", "mysql"),
            ("billing.pkb", "plsql"),
            ("AppDelegate.m", "objective-c"),
            ("View.mm", "objective-c"),
            ("report.pl", "perl"),
            ("index.php", "php"),
            ("Install.ps1", "powershell"),
            ("tool.py", "python"),
            ("app.rb", "ruby"),
            ("Gemfile", "ruby"),
            ("lib.rs", "rust"),
            ("App.swift", "swift"),
            ("login.asp", "vbscript"),
            ("Module1.vb", "vb"),
            ("pom.xml", "xml"),
            ("icon.svg", "xml"),
            ("Info.plist", "plist"),
            ("report.xsl", "xsl"),
            ("build.zig", "zig"),
            ("main.go", "go"),
            ("app.ts", "typescript"),
            ("Main.kt", "kotlin"),
            ("ci.yml", "yaml"),
            ("Cargo.toml", "toml"),
            ("settings.ini", "ini"),
            ("init.lua", "lua"),
            ("Dockerfile", "dockerfile"),
            ("Makefile", "makefile"),
            ("notes.txt", "plaintext"),
        ] {
            assert_eq!(
                registry.detect_path(Path::new(path)).as_str(),
                expected,
                "{path}"
            );
        }
        // Every supported language is selectable by name, after Plain Text.
        let names: Vec<_> = registry
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.display_name)
            .collect();
        assert_eq!(names[0], "Plain Text");
        for name in [
            "AppleScript",
            "Bash",
            "Batch",
            "C",
            "C++",
            "C#",
            "CSS",
            "HTML",
            "Java",
            "JavaScript",
            "JSON",
            "Markdown",
            "MySQL / MariaDB",
            "Oracle SQL",
            "Objective-C/C++",
            "Perl",
            "PHP",
            "PowerShell",
            "Python",
            "Ruby",
            "Rust",
            "Swift",
            "VBScript ASP",
            "Visual Basic",
            "XML",
            "XSL",
            "Zig",
        ] {
            assert!(names.iter().any(|item| item == name), "{name}");
        }
        let mut sorted = names[1..].to_vec();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(sorted, names[1..]);
    }

    #[test]
    fn extensionless_files_are_detected_from_their_first_bytes() {
        let registry = LanguageRegistry::default();
        let detect = |head: &str| {
            registry
                .detect_content(head.as_bytes())
                .map(|id| id.as_str().to_owned())
        };
        assert_eq!(detect("#!/bin/bash\necho hi").as_deref(), Some("bash"));
        assert_eq!(
            detect("#!/usr/bin/env python3\n").as_deref(),
            Some("python")
        );
        assert_eq!(
            detect("#!/usr/bin/env -S node --flag\n").as_deref(),
            Some("javascript")
        );
        assert_eq!(detect("#!/usr/bin/perl -w\n").as_deref(), Some("perl"));
        assert_eq!(
            detect("\u{feff}<?xml version=\"1.0\"?><a/>").as_deref(),
            Some("xml")
        );
        assert_eq!(
            detect("<?xml version=\"1.0\"?>\n<xsl:stylesheet>").as_deref(),
            Some("xsl")
        );
        assert_eq!(detect("<?php echo 1;").as_deref(), Some("php"));
        assert_eq!(detect("  <!DOCTYPE html>\n<html>").as_deref(), Some("html"));
        assert_eq!(detect("#!/opt/unknown-tool\n"), None);
        assert_eq!(detect("plain words"), None);
    }

    #[test]
    fn c_family_sources_color_comments_strings_keywords_types_and_calls() {
        let source = "#include <stdio.h>\n// note\nstatic int add(int a) {\n  /* two\n lines */\n  return printf(\"%d\\n\", a + 0x1F);\n}\n";
        let tokens = tokens("c", source);
        assert!(has(&tokens, "#include", SyntaxToken::Keyword));
        assert!(has(&tokens, "// note", SyntaxToken::Comment));
        assert!(has(&tokens, "/* two\n lines */", SyntaxToken::Comment));
        assert!(has(&tokens, "static", SyntaxToken::Keyword));
        assert!(has(&tokens, "int", SyntaxToken::Type));
        assert!(has(&tokens, "add", SyntaxToken::Function));
        assert!(has(&tokens, "printf", SyntaxToken::Function));
        assert!(has(&tokens, "\"%d\\n\"", SyntaxToken::String));
        assert!(has(&tokens, "0x1F", SyntaxToken::Number));

        let registry = LanguageRegistry::default();
        let analysis = registry
            .analyze(
                &LanguageId::new("c").expect("id"),
                source.as_bytes(),
                100,
                10,
            )
            .expect("analysis");
        // The function body folds, in document coordinates.
        assert_eq!(analysis.folds.len(), 1);
        assert_eq!(
            (analysis.folds[0].start_line, analysis.folds[0].end_line),
            (12, 16)
        );
        assert!(analysis.bracket_pairs.len() >= 3);
        assert!(analysis
            .highlights
            .iter()
            .all(|span| span.start_byte >= 100));
    }

    #[test]
    fn each_language_family_uses_its_own_rules() {
        let rust = tokens(
            "rust",
            "fn main<'a>() { let s: &'a str = \"x\"; for i in 0..10 { Vec::new(); } }",
        );
        assert!(has(&rust, "fn", SyntaxToken::Keyword));
        assert!(has(&rust, "str", SyntaxToken::Type));
        assert!(has(&rust, "Vec", SyntaxToken::Type));
        assert!(has(&rust, "\"x\"", SyntaxToken::String));
        // A lifetime is not an unterminated character literal, and a range is two numbers.
        assert!(has(&rust, "0", SyntaxToken::Number) && has(&rust, "10", SyntaxToken::Number));

        let bash = tokens(
            "bash",
            "# setup\nif [ -n \"$HOME\" ]; then echo ${USER}; fi\n",
        );
        assert!(has(&bash, "# setup", SyntaxToken::Comment));
        assert!(has(&bash, "if", SyntaxToken::Keyword));
        assert!(has(&bash, "${USER", SyntaxToken::Property));

        let batch = tokens(
            "batch",
            "@ECHO off\nREM build it\n:: also a comment\nSET X=1\n",
        );
        assert!(has(&batch, "ECHO", SyntaxToken::Keyword));
        assert!(has(&batch, "REM build it", SyntaxToken::Comment));
        assert!(has(&batch, ":: also a comment", SyntaxToken::Comment));

        let sql = tokens(
            "mysql",
            "SELECT `id` FROM users WHERE name = 'a''b' -- done\n",
        );
        assert!(has(&sql, "SELECT", SyntaxToken::Keyword));
        assert!(has(&sql, "`id`", SyntaxToken::String));
        assert!(has(&sql, "-- done", SyntaxToken::Comment));

        let vb = tokens("vb", "Dim total As Integer ' count\nMsgBox(\"hi\")\n");
        assert!(has(&vb, "Dim", SyntaxToken::Keyword));
        assert!(has(&vb, "Integer", SyntaxToken::Type));
        assert!(has(&vb, "' count", SyntaxToken::Comment));
        assert!(has(&vb, "MsgBox", SyntaxToken::Function));

        let powershell = tokens(
            "powershell",
            "<# doc #>\nforeach ($item in Get-ChildItem) { Write-Host $item }\n",
        );
        assert!(has(&powershell, "<# doc #>", SyntaxToken::Comment));
        assert!(has(&powershell, "foreach", SyntaxToken::Keyword));
        assert!(has(&powershell, "$item", SyntaxToken::Property));

        let yaml = tokens("yaml", "name: build # ci\nenabled: true\n");
        assert!(has(&yaml, "name", SyntaxToken::Property));
        assert!(has(&yaml, "true", SyntaxToken::Keyword));
        assert!(has(&yaml, "# ci", SyntaxToken::Comment));
    }

    #[test]
    fn markup_and_stylesheets_have_dedicated_scanners() {
        let html = "<!DOCTYPE html>\n<div class=\"a\">\n  <!-- note -->\n  <script>if (a < b) {}</script>\n  &amp; <br/>\n</div>\n";
        let tokens_html = tokens("html", html);
        assert!(has(&tokens_html, "<!DOCTYPE html>", SyntaxToken::Keyword));
        assert!(has(&tokens_html, "div", SyntaxToken::Keyword));
        assert!(has(&tokens_html, "class", SyntaxToken::Property));
        assert!(has(&tokens_html, "\"a\"", SyntaxToken::String));
        assert!(has(&tokens_html, "<!-- note -->", SyntaxToken::Comment));
        assert!(has(&tokens_html, "&amp;", SyntaxToken::Escape));
        // Embedded script text is not mistaken for a tag.
        assert!(!has(&tokens_html, "b", SyntaxToken::Keyword));
        let registry = LanguageRegistry::default();
        let analysis = registry
            .analyze(&LanguageId::new("html").expect("id"), html.as_bytes(), 0, 1)
            .expect("analysis");
        assert_eq!(analysis.folds.len(), 1);
        assert_eq!(
            (analysis.folds[0].start_line, analysis.folds[0].end_line),
            (2, 6)
        );

        let xsl = tokens(
            "xsl",
            "<?xml version=\"1.0\"?><xsl:template match=\"/\"><![CDATA[<raw>]]></xsl:template>",
        );
        assert!(has(&xsl, "<?xml", SyntaxToken::Keyword));
        assert!(has(&xsl, "xsl:template", SyntaxToken::Keyword));
        assert!(has(&xsl, "match", SyntaxToken::Property));
        assert!(has(&xsl, "<![CDATA[<raw>]]>", SyntaxToken::String));

        let css = tokens("css", "@media screen { .card, #main a:hover { color: #ff0000; margin: 0 auto 1.5em; background: url(\"x.png\") !important; } } /* end */");
        assert!(has(&css, "@media", SyntaxToken::Keyword));
        assert!(has(&css, ".card", SyntaxToken::Type));
        assert!(has(&css, "color", SyntaxToken::Property));
        assert!(has(&css, "#ff0000", SyntaxToken::Number));
        assert!(has(&css, "1.5em", SyntaxToken::Number));
        assert!(has(&css, "url", SyntaxToken::Function));
        assert!(has(&css, "\"x.png\"", SyntaxToken::String));
        assert!(has(&css, "!important", SyntaxToken::Keyword));
        assert!(has(&css, "/* end */", SyntaxToken::Comment));
    }

    #[test]
    fn lexical_analysis_is_bounded_and_survives_hostile_input() {
        let registry = LanguageRegistry::default();
        // Every table accepts arbitrary bytes, unterminated constructs, and deep nesting.
        let hostile: Vec<u8> = [
            b"/* open \"str 'c <tag attr=\" <!-- <![CDATA[ &#x; ".as_slice(),
            &[0xff, 0xfe, 0x00, b'\\'],
            "{([".repeat(4_000).as_bytes(),
            b"\n$ @ # ' \" ` \\",
        ]
        .concat();
        for descriptor in registry.descriptors() {
            let analysis = registry
                .analyze(&descriptor.id, &hostile, u64::MAX - 8, u64::MAX - 1)
                .expect("analysis");
            assert!(analysis.highlights.len() <= MAX_HIGHLIGHT_SPANS);
            assert!(analysis
                .highlights
                .iter()
                .all(|span| span.start_byte <= span.end_byte));
        }
        let many = "x(1);".repeat(60_000);
        let analysis = registry
            .analyze(
                &LanguageId::new("javascript").expect("id"),
                many.as_bytes(),
                0,
                1,
            )
            .expect("analysis");
        assert_eq!(analysis.parsed_bytes, MAX_SYNTAX_BYTES);
        assert_eq!(analysis.highlights.len(), MAX_HIGHLIGHT_SPANS);
        assert!(analysis.truncated);
    }
}
