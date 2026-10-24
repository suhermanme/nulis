//! Bounded, storage-independent language modules: Tree-sitter grammars for a
//! few languages and table-driven lexers for the rest.

mod lexer;

use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};
use thiserror::Error;
use tree_sitter::{Language, Node, Parser};

pub const MAX_SYNTAX_BYTES: usize = 256 * 1024;
pub const MAX_SYNTAX_NODES: usize = 100_000;
pub const MAX_HIGHLIGHT_SPANS: usize = 20_000;
pub const MAX_FOLD_RANGES: usize = 2_000;
pub const MAX_SYMBOLS: usize = 2_000;
pub const MAX_BRACKET_PAIRS: usize = 20_000;
pub const MAX_PARSE_TIME: Duration = Duration::from_millis(25);
/// Longest `firstLine` pattern a declarative language may supply.
pub const MAX_DECLARATIVE_REGEX_BYTES: usize = 4_096;
/// Compiled size cap for declarative regexes, so a hostile pattern cannot
/// allocate an unbounded automaton.
const MAX_DECLARATIVE_REGEX_SIZE: usize = 1_048_576;

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

/// How a module produces its analysis.
#[derive(Clone, Debug)]
enum Scanner {
    /// No analysis: Plain Text.
    None,
    /// A Tree-sitter grammar walked with the module's node rules.
    Grammar,
    /// A table-driven lexer, owned by the module so plugins can supply one.
    Lexer(Arc<lexer::LexerSpec>),
    /// The bounded Markdown line scanner.
    MarkdownLines,
}

#[derive(Clone)]
pub struct LanguageModule {
    descriptor: LanguageDescriptor,
    scanner: Scanner,
    grammar: Option<Language>,
    token_rules: Vec<TokenRule>,
    fold_node_kinds: Vec<&'static str>,
    symbol_node_kind: Option<&'static str>,
    bracket_node_kinds: Vec<&'static str>,
    comments: Option<CommentSyntax>,
    vocabulary: Option<Vocabulary>,
    /// A plugin language's `firstLine` pattern, tried when the file name says nothing.
    first_line: Option<Regex>,
}

impl std::fmt::Debug for LanguageModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LanguageModule")
            .field("descriptor", &self.descriptor)
            .field("scanner", &self.scanner)
            .field("token_rules", &self.token_rules)
            .field("fold_node_kinds", &self.fold_node_kinds)
            .field("symbol_node_kind", &self.symbol_node_kind)
            .field("bracket_node_kinds", &self.bracket_node_kinds)
            .field("comments", &self.comments)
            .finish_non_exhaustive()
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
            scanner: Scanner::Grammar,
            grammar: Some(grammar),
            token_rules,
            fold_node_kinds,
            symbol_node_kind,
            bracket_node_kinds,
            comments: None,
            vocabulary: None,
            first_line: None,
        }
    }

    fn with_scanner(descriptor: LanguageDescriptor, scanner: Scanner) -> Self {
        Self {
            descriptor,
            scanner,
            grammar: None,
            token_rules: Vec::new(),
            fold_node_kinds: Vec::new(),
            symbol_node_kind: None,
            bracket_node_kinds: Vec::new(),
            comments: None,
            vocabulary: None,
            first_line: None,
        }
    }

    fn lexical(spec: &'static lexer::LexerSeed) -> Self {
        let comments = CommentSyntax {
            line: spec
                .line_comments
                .first()
                .map(|marker| (*marker).to_owned())
                .filter(|_| spec.toggles_line_comments),
            // Markup comments are recognized by the markup scanner itself.
            block: spec
                .block_comments
                .first()
                .map(|(open, close)| ((*open).to_owned(), (*close).to_owned()))
                .or(matches!(spec.mode, lexer::Mode::Markup)
                    .then_some(("<!--".to_owned(), "-->".to_owned()))),
        };
        let runtime = lexer::LexerSpec::from_seed(spec);
        let vocabulary = Vocabulary {
            keywords: runtime
                .keywords
                .iter()
                .chain(&runtime.completion_words)
                .cloned()
                .collect(),
            types: runtime.types.clone(),
            case_insensitive: runtime.case_insensitive,
            word_bytes: runtime.ident_chars.clone(),
        };
        Self::with_scanner(
            LanguageDescriptor {
                id: LanguageId(runtime.id.clone()),
                display_name: runtime.name.clone(),
                extensions: runtime.extensions.clone(),
                filenames: runtime.filenames.clone(),
            },
            Scanner::Lexer(Arc::new(runtime)),
        )
        .with_comments(comments)
        .with_vocabulary(vocabulary)
    }

    /// Builds a language module from validated declarative data (a plugin's
    /// `contributes.languages`). `first_line` is compiled with a bounded regex
    /// builder and is only consulted when the file name says nothing.
    ///
    /// # Errors
    /// Returns an error for an invalid identifier, an invalid regex, or an
    /// empty string delimiter.
    pub fn declarative(
        descriptor: LanguageDescriptor,
        lexer: Option<DeclarativeLexer>,
        comments: CommentSyntax,
        vocabulary: Vocabulary,
        first_line: Option<&str>,
    ) -> Result<Self, SyntaxError> {
        let first_line = first_line
            .map(|pattern| {
                if pattern.len() > MAX_DECLARATIVE_REGEX_BYTES {
                    return Err(SyntaxError::InvalidDeclarative(
                        "firstLine pattern is too long".into(),
                    ));
                }
                RegexBuilder::new(pattern)
                    .size_limit(MAX_DECLARATIVE_REGEX_SIZE)
                    .dfa_size_limit(MAX_DECLARATIVE_REGEX_SIZE)
                    .build()
                    .map_err(|error| SyntaxError::InvalidDeclarative(error.to_string()))
            })
            .transpose()?;
        let scanner = match lexer {
            Some(lexer) => Scanner::Lexer(Arc::new(lexer.into_spec(comments.clone())?)),
            None => Scanner::None,
        };
        let mut module = Self::with_scanner(descriptor, scanner)
            .with_comments(comments)
            .with_vocabulary(vocabulary);
        module.first_line = first_line;
        Ok(module)
    }

    /// Sets the words completion offers; an empty vocabulary means none.
    #[must_use]
    pub fn with_vocabulary(mut self, vocabulary: Vocabulary) -> Self {
        self.vocabulary =
            (!vocabulary.keywords.is_empty() || !vocabulary.types.is_empty()).then_some(vocabulary);
        self
    }

    /// Sets the markers Toggle Comment uses; a syntax with neither marker means none.
    #[must_use]
    pub fn with_comments(mut self, comments: CommentSyntax) -> Self {
        self.comments = (comments.line.is_some() || comments.block.is_some()).then_some(comments);
        self
    }

    #[must_use]
    pub fn descriptor(&self) -> &LanguageDescriptor {
        &self.descriptor
    }
}

/// Words a language's completion offers, and what counts as a word.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Vocabulary {
    pub keywords: Vec<String>,
    pub types: Vec<String>,
    /// Keywords match in any letter case; they are stored in lowercase.
    pub case_insensitive: bool,
    /// Bytes other than letters, digits, and `_` that belong to a word (`$`, `-`).
    pub word_bytes: Vec<u8>,
}

/// How a language writes comments, for the Toggle Comment command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommentSyntax {
    /// A marker that comments out the rest of a line (`//`, `#`, `--`).
    pub line: Option<String>,
    /// Markers around a comment, used when there is no line marker (`<!--`, `-->`).
    pub block: Option<(String, String)>,
}

/// The scanner a declarative language asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeclarativeMode {
    Code,
    Markup,
    Css,
    Delimited,
}

/// One string delimiter in a declarative language.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarativeString {
    pub open: String,
    pub close: String,
    /// A backslash hides the next byte from the closing delimiter.
    pub escape: bool,
    pub multiline: bool,
}

/// A plugin language's highlighting table, before it becomes a runtime lexer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeclarativeLexer {
    pub mode: Option<DeclarativeMode>,
    pub keywords: Vec<String>,
    pub types: Vec<String>,
    pub strings: Vec<DeclarativeString>,
    pub case_insensitive: bool,
    /// Extra bytes allowed inside an identifier (`$`, `-`).
    pub ident_chars: Vec<u8>,
}

impl DeclarativeLexer {
    fn into_spec(self, comments: CommentSyntax) -> Result<lexer::LexerSpec, SyntaxError> {
        if self.strings.iter().any(|rule| rule.open.is_empty()) {
            return Err(SyntaxError::InvalidDeclarative(
                "a string delimiter is empty".into(),
            ));
        }
        Ok(lexer::LexerSpec {
            id: String::new(),
            name: String::new(),
            extensions: Vec::new(),
            filenames: Vec::new(),
            mode: match self.mode.unwrap_or(DeclarativeMode::Code) {
                DeclarativeMode::Code => lexer::Mode::Code,
                DeclarativeMode::Markup => lexer::Mode::Markup,
                DeclarativeMode::Css => lexer::Mode::Css,
                DeclarativeMode::Delimited => lexer::Mode::Delimited,
            },
            keywords: self.keywords,
            types: self.types,
            line_comments: comments.line.into_iter().collect(),
            comment_words: Vec::new(),
            block_comments: comments.block.into_iter().collect(),
            strings: self
                .strings
                .into_iter()
                .map(|rule| lexer::StringRule {
                    open: rule.open,
                    close: rule.close,
                    escape: rule.escape,
                    multiline: rule.multiline,
                })
                .collect(),
            case_insensitive: self.case_insensitive,
            sigils: Vec::new(),
            ident_chars: self.ident_chars,
            preprocessor: false,
            capital_types: false,
            key_suffix: None,
            raw_text_elements: false,
            delimiter: None,
            yaml: false,
            completion_words: Vec::new(),
            ecmascript: false,
            jsx: false,
        })
    }
}

/// Where a language or an association came from. When two associations of
/// the same kind claim a file, the earlier variant wins, then the lower
/// plugin ID.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AssociationSource {
    /// `files.associations` in the user's settings.
    User,
    /// Shipped with Nulis.
    Builtin,
    /// Contributed by the plugin with this ID.
    Plugin(String),
}

/// What part of a file name an association looks at. Values are lowercase.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FileMatcher {
    /// The whole file name (`dockerfile`, `.bashrc`).
    FileName(String),
    /// The end of the file name after a dot; may itself contain dots (`d.ts`).
    Extension(String),
    /// The start of the file name, written `dockerfile.*` (`Dockerfile.dev`).
    Prefix(String),
}

impl FileMatcher {
    /// Reads a `files.associations` key: `*.ext`, `name.*`, or an exact file name.
    ///
    /// # Errors
    /// Returns an error for an empty pattern, a path, or any other wildcard.
    pub fn parse(pattern: &str) -> Result<Self, SyntaxError> {
        let invalid = || SyntaxError::InvalidAssociation(pattern.to_owned());
        let lower = pattern.trim().to_ascii_lowercase();
        let (matcher, value) = match (lower.strip_prefix("*."), lower.strip_suffix('*')) {
            (Some(extension), _) => (Self::Extension(extension.to_owned()), extension.to_owned()),
            (None, Some(prefix)) => (Self::Prefix(prefix.to_owned()), prefix.to_owned()),
            (None, None) => (Self::FileName(lower.clone()), lower),
        };
        if value.is_empty()
            || value.len() > 255
            || value.contains(['*', '?', '/', '\\'])
            || value.starts_with('.') && matches!(matcher, Self::Extension(_))
        {
            return Err(invalid());
        }
        Ok(matcher)
    }

    fn matches(&self, file_name: &str) -> bool {
        match self {
            Self::FileName(name) => file_name == name,
            Self::Prefix(prefix) => {
                file_name.len() > prefix.len() && file_name.starts_with(prefix.as_str())
            }
            Self::Extension(extension) => {
                // `name.ext`: a dot, and something before it, so `.bashrc` has no extension.
                file_name.len() > extension.len() + 1
                    && file_name.ends_with(extension.as_str())
                    && file_name.as_bytes()[file_name.len() - extension.len() - 1] == b'.'
            }
        }
    }

    /// The `files.associations` spelling of this matcher (`*.rs`, `Dockerfile.*`).
    #[must_use]
    pub fn pattern(&self) -> String {
        match self {
            Self::FileName(name) => name.clone(),
            Self::Prefix(prefix) => format!("{prefix}*"),
            Self::Extension(extension) => format!("*.{extension}"),
        }
    }

    /// Ranks matches: exact file names, then name prefixes, then
    /// extensions; longer ones first within each.
    fn specificity(&self) -> (u8, usize) {
        match self {
            Self::FileName(name) => (2, name.len()),
            Self::Prefix(prefix) => (1, prefix.len()),
            Self::Extension(extension) => (0, extension.len()),
        }
    }
}

/// One rule mapping files to a language.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Association {
    pub matcher: FileMatcher,
    pub language: LanguageId,
    pub source: AssociationSource,
}

/// Several languages claim the same file name or extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssociationConflict {
    pub matcher: FileMatcher,
    /// Every claim, in the order they are considered; the first one wins.
    pub claims: Vec<(LanguageId, AssociationSource)>,
}

#[derive(Debug)]
struct RegisteredModule {
    module: LanguageModule,
    source: AssociationSource,
}

#[derive(Debug)]
pub struct LanguageRegistry {
    modules: BTreeMap<LanguageId, RegisteredModule>,
    user_associations: Vec<Association>,
    /// Associations a plugin added to a language it does not own.
    plugin_associations: Vec<Association>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl LanguageRegistry {
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            modules: BTreeMap::new(),
            user_associations: Vec::new(),
            plugin_associations: Vec::new(),
        };
        let builtins = [
            plain_text_module(),
            json_module(),
            json_with_comments_module(),
            python_module(),
            markdown_module(),
        ]
        .into_iter()
        .chain(lexer::SPECS.iter().map(LanguageModule::lexical));
        for module in builtins {
            registry.modules.insert(
                module.descriptor.id.clone(),
                RegisteredModule {
                    module,
                    source: AssociationSource::Builtin,
                },
            );
        }
        registry
    }

    /// Registers a language module from a plugin (or another non-user source).
    ///
    /// # Errors
    /// Returns an error when a module with the same ID is already registered,
    /// or when the source is `User` (users add associations, not languages).
    pub fn register(
        &mut self,
        module: LanguageModule,
        source: AssociationSource,
    ) -> Result<(), SyntaxError> {
        let id = module.descriptor.id.clone();
        if source == AssociationSource::User {
            return Err(SyntaxError::InvalidAssociation(id.0));
        }
        if self.modules.contains_key(&id) {
            return Err(SyntaxError::DuplicateLanguage(id));
        }
        self.modules.insert(id, RegisteredModule { module, source });
        Ok(())
    }

    /// Removes a language that is not built in, with its file associations.
    /// Documents keep their language ID; analysis then reports an unknown
    /// language and the caller falls back to Plain Text.
    ///
    /// # Errors
    /// Returns an error for an unknown or built-in language.
    pub fn unregister(&mut self, id: &LanguageId) -> Result<LanguageModule, SyntaxError> {
        match self.modules.remove(id) {
            None => Err(SyntaxError::UnknownLanguage(id.clone())),
            Some(registered) if registered.source == AssociationSource::Builtin => {
                self.modules.insert(id.clone(), registered);
                Err(SyntaxError::BuiltinLanguage(id.clone()))
            }
            Some(registered) => {
                self.plugin_associations
                    .retain(|association| association.language != *id);
                Ok(registered.module)
            }
        }
    }

    /// Adds a plugin's file associations to a language it does not own. The
    /// language must already be registered; duplicates are ignored.
    ///
    /// # Errors
    /// Returns an error for an unknown language or a bad pattern.
    pub fn add_plugin_associations(
        &mut self,
        plugin_id: &str,
        language: &LanguageId,
        extensions: &[String],
        filenames: &[String],
    ) -> Result<(), SyntaxError> {
        if !self.modules.contains_key(language) {
            return Err(SyntaxError::UnknownLanguage(language.clone()));
        }
        let source = AssociationSource::Plugin(plugin_id.to_owned());
        for extension in extensions {
            let matcher = FileMatcher::parse(&format!("*.{extension}"))?;
            let association = Association {
                matcher,
                language: language.clone(),
                source: source.clone(),
            };
            if !self.plugin_associations.contains(&association) {
                self.plugin_associations.push(association);
            }
        }
        for filename in filenames {
            let matcher = FileMatcher::parse(filename)?;
            let association = Association {
                matcher,
                language: language.clone(),
                source: source.clone(),
            };
            if !self.plugin_associations.contains(&association) {
                self.plugin_associations.push(association);
            }
        }
        Ok(())
    }

    /// Removes every language and association contributed by plugins.
    pub fn clear_plugin_languages(&mut self) {
        self.modules
            .retain(|_, registered| !matches!(registered.source, AssociationSource::Plugin(_)));
        self.plugin_associations.clear();
    }

    #[must_use]
    pub fn contains(&self, id: &LanguageId) -> bool {
        self.modules.contains_key(id)
    }

    /// Replaces the user's associations (`files.associations`: pattern to
    /// language ID). Entries with a bad pattern or an unknown language are
    /// left out and returned so the caller can report them.
    pub fn set_user_associations<'a>(
        &mut self,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Vec<String> {
        let mut rejected = Vec::new();
        self.user_associations.clear();
        for (pattern, language) in entries {
            let parsed = FileMatcher::parse(pattern).ok().zip(
                LanguageId::new(language)
                    .ok()
                    .filter(|id| self.modules.contains_key(id)),
            );
            match parsed {
                Some((matcher, language)) => self.user_associations.push(Association {
                    matcher,
                    language,
                    source: AssociationSource::User,
                }),
                None => rejected.push(pattern.to_owned()),
            }
        }
        rejected
    }

    /// Every association: the user's first, then each language's file names
    /// and extensions.
    #[must_use]
    pub fn associations(&self) -> Vec<Association> {
        let mut all = self.user_associations.clone();
        all.extend(self.plugin_associations.iter().cloned());
        for (id, registered) in &self.modules {
            let descriptor = &registered.module.descriptor;
            let matchers = descriptor
                .filenames
                .iter()
                .map(|name| {
                    let name = name.to_ascii_lowercase();
                    match name.strip_suffix('*') {
                        Some(prefix) => FileMatcher::Prefix(prefix.to_owned()),
                        None => FileMatcher::FileName(name),
                    }
                })
                .chain(
                    descriptor
                        .extensions
                        .iter()
                        .map(|extension| FileMatcher::Extension(extension.to_ascii_lowercase())),
                );
            all.extend(matchers.map(|matcher| Association {
                matcher,
                language: id.clone(),
                source: registered.source.clone(),
            }));
        }
        all
    }

    /// File names and extensions that more than one language claims.
    #[must_use]
    pub fn conflicts(&self) -> Vec<AssociationConflict> {
        let mut claims: BTreeMap<FileMatcher, Vec<(LanguageId, AssociationSource)>> =
            BTreeMap::new();
        for association in self.associations() {
            if association.source != AssociationSource::User {
                claims
                    .entry(association.matcher)
                    .or_default()
                    .push((association.language, association.source));
            }
        }
        claims
            .into_iter()
            .filter(|(_, claims)| claims.len() > 1)
            .map(|(matcher, mut claims)| {
                claims.sort_by(|left, right| (&left.1, &left.0).cmp(&(&right.1, &right.0)));
                AssociationConflict { matcher, claims }
            })
            .collect()
    }

    /// The words completion offers for a language, if it has any.
    #[must_use]
    pub fn vocabulary(&self, id: &LanguageId) -> Option<Vocabulary> {
        self.modules.get(id)?.module.vocabulary.clone()
    }

    /// Bytes besides letters, digits, and `_` that are part of a word in a
    /// language (`$` in JavaScript, `-` in CSS), for every language.
    #[must_use]
    pub fn word_bytes(&self, id: &LanguageId) -> &[u8] {
        self.modules
            .get(id)
            .and_then(|registered| registered.module.vocabulary.as_ref())
            .map_or(&[], |vocabulary| vocabulary.word_bytes.as_slice())
    }

    /// The comment markers of a language, or `None` when it has none
    /// (plain text, JSON).
    #[must_use]
    pub fn comment_syntax(&self, id: &LanguageId) -> Option<CommentSyntax> {
        self.modules.get(id)?.module.comments.clone()
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<LanguageDescriptor> {
        let mut descriptors: Vec<_> = self
            .modules
            .values()
            .map(|registered| registered.module.descriptor.clone())
            .collect();
        // Listed the way a picker shows them: Plain Text first, then by name.
        descriptors.sort_by_key(|descriptor| {
            (
                descriptor.id.as_str() != PLAIN_TEXT,
                descriptor.display_name.to_lowercase(),
            )
        });
        descriptors
    }

    /// Guesses a language from the first bytes of a file whose name says
    /// nothing: a `#!` interpreter line, an XML or PHP prolog, or an HTML doctype.
    #[must_use]
    pub fn detect_content(&self, head: &[u8]) -> Option<LanguageId> {
        if let Some(id) = lexer::detect_content(head) {
            let id = LanguageId(id.into());
            if self.modules.contains_key(&id) {
                return Some(id);
            }
        }
        let first_line = head
            .split(|byte| *byte == b'\n')
            .next()
            .map(|line| {
                String::from_utf8_lossy(line)
                    .trim_end_matches('\r')
                    .to_owned()
            })
            .unwrap_or_default();
        self.modules
            .iter()
            .filter_map(|(id, registered)| {
                registered
                    .module
                    .first_line
                    .as_ref()
                    .filter(|pattern| pattern.is_match(&first_line))
                    .map(|_| (registered.source.clone(), id.clone()))
            })
            // Built-in patterns first, then plugins by ID.
            .min()
            .map(|(_, id)| id)
    }

    /// The language a file name selects. In order: the user's associations,
    /// then exact file names, then the longest matching extension; within a
    /// step, built-in languages before plugins and plugins by ID. Plain Text
    /// when nothing matches.
    #[must_use]
    pub fn detect_path(&self, path: &Path) -> LanguageId {
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let best = |associations: &mut dyn Iterator<Item = Association>| {
            associations
                .filter(|association| association.matcher.matches(&file_name))
                .max_by(|left, right| {
                    left.matcher
                        .specificity()
                        .cmp(&right.matcher.specificity())
                        // Earlier sources and lower IDs win, so they compare greater.
                        .then_with(|| right.source.cmp(&left.source))
                        .then_with(|| right.language.cmp(&left.language))
                })
                .map(|association| association.language)
        };
        // An association to a language removed since it was set is skipped.
        let user = best(
            &mut self
                .user_associations
                .iter()
                .filter(|association| self.modules.contains_key(&association.language))
                .cloned(),
        );
        user.or_else(|| {
            let mut others = self
                .associations()
                .into_iter()
                .filter(|association| association.source != AssociationSource::User);
            best(&mut others)
        })
        .unwrap_or_else(|| LanguageId(PLAIN_TEXT.into()))
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
        let module = &self
            .modules
            .get(language_id)
            .ok_or_else(|| SyntaxError::UnknownLanguage(language_id.clone()))?
            .module;
        let window = &source[..parsed_len];
        match &module.scanner {
            Scanner::None => Ok(SyntaxAnalysis::plain_text(parsed_len, truncated)),
            Scanner::MarkdownLines => analyze_markdown(window, base_byte, base_line, truncated),
            Scanner::Lexer(spec) => Ok(lexer::analyze(
                spec, window, base_byte, base_line, truncated,
            )),
            Scanner::Grammar => {
                analyze_tree_sitter(module, window, base_byte, base_line, truncated)
            }
        }
    }
}

const PLAIN_TEXT: &str = "plaintext";

fn plain_text_module() -> LanguageModule {
    LanguageModule::with_scanner(
        LanguageDescriptor {
            id: LanguageId(PLAIN_TEXT.into()),
            display_name: "Plain Text".into(),
            extensions: Vec::new(),
            filenames: Vec::new(),
        },
        Scanner::None,
    )
}

fn json_module() -> LanguageModule {
    LanguageModule::tree_sitter(
        LanguageDescriptor {
            id: LanguageId("json".into()),
            display_name: "JSON".into(),
            // TypeScript's incremental build information is JSON too.
            extensions: vec!["json".into(), "tsbuildinfo".into()],
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
    .with_vocabulary(Vocabulary {
        keywords: vec!["true".into(), "false".into(), "null".into()],
        ..Vocabulary::default()
    })
}

/// JSON with `//` and `/* */` comments (`.jsonc`). The JSON grammar reads
/// comments; the JSON tools do not, so this language has none.
fn json_with_comments_module() -> LanguageModule {
    let json = json_module();
    LanguageModule::tree_sitter(
        LanguageDescriptor {
            id: LanguageId("jsonc".into()),
            display_name: "JSON with Comments".into(),
            extensions: vec!["jsonc".into()],
            filenames: Vec::new(),
        },
        tree_sitter_json::LANGUAGE.into(),
        json.token_rules.clone(),
        json.fold_node_kinds.clone(),
        json.symbol_node_kind,
        json.bracket_node_kinds.clone(),
    )
    .with_comments(CommentSyntax {
        line: Some("//".into()),
        block: Some(("/*".into(), "*/".into())),
    })
    .with_vocabulary(Vocabulary {
        keywords: vec!["true".into(), "false".into(), "null".into()],
        ..Vocabulary::default()
    })
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
        vec![
            "argument_list",
            "parameters",
            "list",
            "dictionary",
            "set",
            "tuple",
            "parenthesized_expression",
            "list_comprehension",
            "dictionary_comprehension",
            "set_comprehension",
            "generator_expression",
        ],
    )
    .with_comments(CommentSyntax {
        line: Some("#".into()),
        block: None,
    })
    .with_vocabulary(Vocabulary {
        keywords: lexer::PYTHON_WORDS
            .iter()
            .map(|word| (*word).to_owned())
            .collect(),
        ..Vocabulary::default()
    })
}

fn markdown_module() -> LanguageModule {
    LanguageModule::with_scanner(
        LanguageDescriptor {
            id: LanguageId("markdown".into()),
            display_name: "Markdown".into(),
            extensions: vec!["md".into(), "markdown".into(), "mdown".into()],
            filenames: Vec::new(),
        },
        Scanner::MarkdownLines,
    )
    .with_comments(CommentSyntax {
        line: None,
        block: Some(("<!--".into(), "-->".into())),
    })
}

fn analyze_markdown(
    source: &[u8],
    base_byte: u64,
    base_line: u64,
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
    // (level, row, line start) of each heading outside code blocks, and where each row ends.
    let mut headings: Vec<(usize, usize, usize)> = Vec::new();
    let mut line_ends: Vec<usize> = Vec::new();
    while cursor < source.len() {
        let line_end = source[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(source.len(), |relative| cursor + relative);
        line_ends.push(line_end);
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
                headings.push((hashes, line_ends.len() - 1, cursor));
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
    push_heading_folds(&mut analysis, &headings, &line_ends, base_byte, base_line)?;
    Ok(analysis)
}

/// Folds each Markdown heading down to the line before the next heading of
/// the same or a higher level. A last section whose end is past a cut-off
/// window is not offered: its end is unknown.
fn push_heading_folds(
    analysis: &mut SyntaxAnalysis,
    headings: &[(usize, usize, usize)],
    line_ends: &[usize],
    base_byte: u64,
    base_line: u64,
) -> Result<(), SyntaxError> {
    // or a higher level. A last section whose end is past the window is not
    // offered: its end is unknown.
    for (index, (level, row, start)) in headings.iter().enumerate() {
        let next = headings[index + 1..]
            .iter()
            .find(|(other, ..)| other <= level)
            .map(|(_, next_row, _)| *next_row);
        let end_row = match next {
            Some(next_row) => next_row - 1,
            None if analysis.truncated => continue,
            None => line_ends.len() - 1,
        };
        if end_row <= *row {
            continue;
        }
        if analysis.folds.len() >= MAX_FOLD_RANGES {
            analysis.truncated = true;
            break;
        }
        let offset = |value: usize| {
            base_byte
                .checked_add(u64::try_from(value).map_err(|_| SyntaxError::OffsetOverflow)?)
                .ok_or(SyntaxError::OffsetOverflow)
        };
        analysis.folds.push(FoldRange {
            start_byte: offset(*start)?,
            end_byte: offset(line_ends[end_row])?,
            start_line: base_line.saturating_add(*row as u64),
            end_line: base_line.saturating_add(end_row as u64),
        });
    }
    Ok(())
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
    // A pair needs matching bracket characters at both ends (a Python call
    // or list node also covers what precedes the bracket in some grammars).
    let brackets_at_ends = matches!(
        (
            source.get(node.start_byte()),
            node.end_byte()
                .checked_sub(1)
                .and_then(|last| source.get(last))
        ),
        (Some(b'('), Some(b')')) | (Some(b'['), Some(b']')) | (Some(b'{'), Some(b'}'))
    );
    if module.bracket_node_kinds.contains(&kind)
        && node.end_byte() > node.start_byte() + 1
        && brackets_at_ends
    {
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
    #[error("language {0:?} is built in and cannot be removed")]
    BuiltinLanguage(LanguageId),
    #[error("unsupported file association {0:?}: use *.ext or a file name")]
    InvalidAssociation(String),
    #[error("invalid declarative language: {0}")]
    InvalidDeclarative(String),
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
        assert_eq!(line("rust").as_deref(), Some("//"));
        assert_eq!(line("python").as_deref(), Some("#"));
        assert_eq!(line("sql").as_deref(), Some("--"));
        assert_eq!(
            registry
                .comment_syntax(&LanguageId::new("html").expect("id"))
                .and_then(|syntax| syntax.block)
                .as_ref()
                .map(|(open, close)| (open.as_str(), close.as_str())),
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
            ("export.csv", "csv"),
            ("table.tsv", "tsv"),
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
    fn yaml_markers_anchors_tags_block_text_and_quoted_keys() {
        let yaml = tokens(
            "yaml",
            "---\ndefaults: &base\n  retries: 3\n\"quoted key\": 1\nservice:\n  <<: *base\n  ref: !Ref Bucket\n  script: |\n    echo one\n\n    echo two\n  after: x\n...\n",
        );
        assert!(has(&yaml, "---", SyntaxToken::Keyword));
        assert!(has(&yaml, "...", SyntaxToken::Keyword));
        assert!(has(&yaml, "&base", SyntaxToken::Function));
        assert!(has(&yaml, "*base", SyntaxToken::Function));
        assert!(has(&yaml, "!Ref", SyntaxToken::Type));
        assert!(has(&yaml, "\"quoted key\"", SyntaxToken::Property));
        assert!(has(&yaml, "|", SyntaxToken::Keyword));
        assert!(has(
            &yaml,
            "    echo one\n\n    echo two",
            SyntaxToken::String
        ));
        // The key after the block is a key again.
        assert!(has(&yaml, "after", SyntaxToken::Property));
        // A quoted value stays a string; a dash in a value is not a marker.
        let values = tokens("yaml", "a: \"text\"\nb: x---y\n");
        assert!(has(&values, "\"text\"", SyntaxToken::String));
        assert!(!values.iter().any(|(_, kind)| *kind == SyntaxToken::Keyword));
    }

    #[test]
    fn csv_columns_keep_their_color_down_the_file() {
        let csv = tokens("csv", "id,name,note\n1,\"Doe, Jane\",\"said \"\"hi\"\"\"\n2,Bob,\"two\nlines\"\n3,\"x\"junk,z\n");
        let column = |text: &str| {
            csv.iter()
                .find(|(item, _)| item == text)
                .map(|(_, kind)| *kind)
        };
        assert_eq!(column("id"), column("1"));
        assert_eq!(column("name"), column("\"Doe, Jane\""));
        assert_eq!(column("note"), column("\"said \"\"hi\"\"\""));
        assert_eq!(column("note"), column("\"two\nlines\""));
        assert_ne!(column("id"), column("name"));
        assert!(has(&csv, "junk", SyntaxToken::Invalid));
        // Semicolons are recognized when the first line uses them.
        let semicolons = tokens("csv", "a;b;c\n1;2,5;3\n");
        assert!(has(
            &semicolons,
            "2,5",
            semicolons
                .iter()
                .find(|(item, _)| item == "b")
                .expect("b")
                .1
        ));
        let tsv = tokens("tsv", "a\tb\n1\t2\n");
        assert_eq!(tsv.len(), 4);
        assert_ne!(tsv[0].1, tsv[1].1);
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

#[cfg(test)]
mod catalog_tests {
    use super::{
        AssociationSource, CommentSyntax, DeclarativeLexer, DeclarativeMode, DeclarativeString,
        FileMatcher, LanguageDescriptor, LanguageId, LanguageModule, LanguageRegistry, Scanner,
        SyntaxError, SyntaxToken, Vocabulary, MAX_DECLARATIVE_REGEX_BYTES,
    };
    use std::path::Path;

    fn id(value: &str) -> LanguageId {
        LanguageId::new(value).expect("valid id")
    }

    fn plugin_language(language: &str, extensions: &[&str], filenames: &[&str]) -> LanguageModule {
        LanguageModule::with_scanner(
            LanguageDescriptor {
                id: id(language),
                display_name: language.to_uppercase(),
                extensions: extensions.iter().map(|item| (*item).to_owned()).collect(),
                filenames: filenames.iter().map(|item| (*item).to_owned()).collect(),
            },
            Scanner::None,
        )
    }

    fn plugin(name: &str) -> AssociationSource {
        AssociationSource::Plugin(name.into())
    }

    #[test]
    fn a_user_association_overrides_a_built_in_one() {
        let mut registry = LanguageRegistry::with_builtins();
        assert_eq!(registry.detect_path(Path::new("x.h")).as_str(), "c");
        let rejected = registry.set_user_associations([("*.h", "cpp"), ("Jenkinsfile", "ruby")]);
        assert_eq!(rejected, Vec::<String>::new());
        assert_eq!(registry.detect_path(Path::new("src/x.H")).as_str(), "cpp");
        assert_eq!(
            registry.detect_path(Path::new("ci/Jenkinsfile")).as_str(),
            "ruby"
        );
        // Other mappings are untouched, and clearing restores the built-in one.
        assert_eq!(registry.detect_path(Path::new("x.c")).as_str(), "c");
        registry.set_user_associations([]);
        assert_eq!(registry.detect_path(Path::new("x.h")).as_str(), "c");
    }

    #[test]
    fn bad_patterns_and_unknown_languages_are_rejected_and_reported() {
        let mut registry = LanguageRegistry::with_builtins();
        let rejected = registry.set_user_associations([
            ("*.h", "cpp"),
            ("src/*.rs", "rust"),
            ("*", "rust"),
            ("*.", "rust"),
            ("*..x", "rust"),
            ("*.foo", "no-such-language"),
            ("", "rust"),
        ]);
        assert_eq!(rejected, ["src/*.rs", "*", "*.", "*..x", "*.foo", ""]);
        assert_eq!(registry.detect_path(Path::new("x.h")).as_str(), "cpp");
        assert!(matches!(
            FileMatcher::parse("a/b"),
            Err(SyntaxError::InvalidAssociation(_))
        ));
    }

    #[test]
    fn the_longest_extension_wins_and_file_names_beat_extensions() {
        let mut registry = LanguageRegistry::with_builtins();
        registry
            .register(plugin_language("dts", &["d.ts"], &[]), plugin("acme.dts"))
            .expect("register");
        registry
            .register(
                plugin_language("special", &[], &["special.ts"]),
                plugin("acme.special"),
            )
            .expect("register");
        assert_eq!(
            registry.detect_path(Path::new("index.d.ts")).as_str(),
            "dts"
        );
        assert_eq!(
            registry.detect_path(Path::new("index.ts")).as_str(),
            "typescript"
        );
        assert_eq!(
            registry.detect_path(Path::new("special.ts")).as_str(),
            "special"
        );
        // `.d.ts` alone has no name before the extension.
        assert_eq!(
            registry.detect_path(Path::new(".d.ts")).as_str(),
            "typescript"
        );
    }

    #[test]
    fn built_in_claims_beat_plugins_and_plugins_are_ordered_by_id() {
        let mut registry = LanguageRegistry::with_builtins();
        registry
            .register(
                plugin_language("better-json", &["json"], &[]),
                plugin("zeta.json"),
            )
            .expect("register");
        registry
            .register(plugin_language("nim-b", &["nim"], &[]), plugin("beta.nim"))
            .expect("register");
        registry
            .register(plugin_language("nim-a", &["nim"], &[]), plugin("alpha.nim"))
            .expect("register");
        assert_eq!(registry.detect_path(Path::new("x.json")).as_str(), "json");
        assert_eq!(registry.detect_path(Path::new("x.nim")).as_str(), "nim-a");

        let conflicts = registry.conflicts();
        let json = conflicts
            .iter()
            .find(|conflict| conflict.matcher == FileMatcher::Extension("json".into()))
            .expect("json conflict");
        assert_eq!(
            json.claims,
            [
                (id("json"), AssociationSource::Builtin),
                (id("better-json"), plugin("zeta.json"))
            ]
        );
        let nim = conflicts
            .iter()
            .find(|conflict| conflict.matcher == FileMatcher::Extension("nim".into()))
            .expect("nim conflict");
        assert_eq!(nim.claims[0].0, id("nim-a"));

        // The user settles a conflict with an association.
        registry.set_user_associations([("*.json", "better-json"), ("*.nim", "nim-b")]);
        assert_eq!(
            registry.detect_path(Path::new("x.json")).as_str(),
            "better-json"
        );
        assert_eq!(registry.detect_path(Path::new("x.nim")).as_str(), "nim-b");
    }

    #[test]
    fn python_brackets_pair_up_and_css_comments_with_blocks() {
        let registry = LanguageRegistry::with_builtins();
        let analysis = registry
            .analyze(&id("python"), b"f(a, [1, {2: (3)}])\n", 0, 1)
            .expect("analysis");
        let mut pairs: Vec<(u64, u64)> = analysis
            .bracket_pairs
            .iter()
            .map(|pair| (pair.open_byte, pair.close_byte))
            .collect();
        pairs.sort_unstable();
        assert_eq!(pairs, [(1, 18), (5, 17), (9, 16), (13, 15)]);
        let css = registry.comment_syntax(&id("css")).expect("css");
        assert_eq!(css.line, None);
        assert_eq!(
            css.block
                .as_ref()
                .map(|(open, close)| (open.as_str(), close.as_str())),
            Some(("/*", "*/"))
        );
        // SCSS `//` comments are still highlighted as comments.
        let analysis = registry
            .analyze(&id("css"), b"// note\na { }\n", 0, 1)
            .expect("analysis");
        assert!(analysis
            .highlights
            .iter()
            .any(|span| span.token == SyntaxToken::Comment && span.start_byte == 0));
    }

    /// The highlighted pieces of `source`, as (text, token).
    fn tokens(language: &str, source: &str) -> Vec<(String, SyntaxToken)> {
        let analysis = LanguageRegistry::with_builtins()
            .analyze(&id(language), source.as_bytes(), 0, 1)
            .expect("analysis");
        analysis
            .highlights
            .iter()
            .map(|span| {
                let text = &source[usize::try_from(span.start_byte).expect("small")
                    ..usize::try_from(span.end_byte).expect("small")];
                (text.to_owned(), span.token)
            })
            .collect()
    }

    fn has(tokens: &[(String, SyntaxToken)], text: &str, token: SyntaxToken) -> bool {
        tokens
            .iter()
            .any(|(found, kind)| found == text && *kind == token)
    }

    #[test]
    fn javascript_regular_expressions_are_told_from_division() {
        let found = tokens("javascript", "const re = /a[/]b\\/c/gi;\nconst half = total / 2 / 3;\nif (x) return /end$/.test(s);\n");
        assert!(
            has(&found, "/a[/]b\\/c/gi", SyntaxToken::String),
            "{found:?}"
        );
        assert!(has(&found, "/end$/", SyntaxToken::String), "{found:?}");
        assert!(
            !found.iter().any(|(text, _)| text.contains("/ 2")),
            "{found:?}"
        );
        // A comment is still a comment.
        assert!(has(
            &tokens("javascript", "x = 1 // note\n"),
            "// note",
            SyntaxToken::Comment
        ));
    }

    #[test]
    fn template_strings_highlight_their_expressions_as_code() {
        let source = "const s = `total: ${count + 1} of ${items.map((i) => { return i; }).length}`;\nlet n = 2;\n";
        let found = tokens("typescript", source);
        assert!(has(&found, "`total: ", SyntaxToken::String), "{found:?}");
        assert!(has(&found, "${", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "1", SyntaxToken::Number), "{found:?}");
        assert!(has(&found, "return", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "`", SyntaxToken::String), "{found:?}");
        // Code after the template is code again.
        assert!(has(&found, "let", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "2", SyntaxToken::Number), "{found:?}");
    }

    #[test]
    fn jsx_elements_attributes_and_children_are_highlighted() {
        let source = "function App() {\n  return (\n    <Panel title=\"Hi\" onClick={() => go(1)}>\n      Hello {name} and if\n      <br />\n      <>frag</>\n    </Panel>\n  );\n}\nconst after = 3;\n";
        let found = tokens("tsx", source);
        assert!(has(&found, "Panel", SyntaxToken::Type), "{found:?}");
        assert!(has(&found, "title", SyntaxToken::Property), "{found:?}");
        assert!(has(&found, "\"Hi\"", SyntaxToken::String), "{found:?}");
        assert!(has(&found, "onClick", SyntaxToken::Property), "{found:?}");
        assert!(has(&found, "go", SyntaxToken::Function), "{found:?}");
        assert!(has(&found, "br", SyntaxToken::Keyword), "{found:?}");
        // Text between tags is text, even the word `if`.
        assert!(!has(&found, "if", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "const", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "3", SyntaxToken::Number), "{found:?}");
        // The function body still folds across the element.
        let analysis = LanguageRegistry::with_builtins()
            .analyze(&id("tsx"), source.as_bytes(), 0, 1)
            .expect("analysis");
        assert!(analysis
            .folds
            .iter()
            .any(|fold| fold.start_line == 1 && fold.end_line == 9));
    }

    #[test]
    fn typescript_casts_and_comparisons_are_not_elements() {
        let found = tokens(
            "typescript",
            "const a = <Foo>bar;\nif (x < y) { z(); }\nlet b = 1;\n",
        );
        assert!(has(&found, "let", SyntaxToken::Keyword), "{found:?}");
        assert!(has(&found, "z", SyntaxToken::Function), "{found:?}");
        let found = tokens("javascript", "if (a < b && c > d) { run(); }\n");
        assert!(has(&found, "run", SyntaxToken::Function), "{found:?}");
    }

    fn fold_lines(language: &str, source: &str) -> Vec<(u64, u64)> {
        let mut folds: Vec<(u64, u64)> = LanguageRegistry::with_builtins()
            .analyze(&id(language), source.as_bytes(), 0, 1)
            .expect("analysis")
            .folds
            .iter()
            .map(|fold| (fold.start_line, fold.end_line))
            .collect();
        folds.sort_unstable();
        folds
    }

    #[test]
    fn markdown_sections_fold_under_their_headings() {
        let source =
            "# Title\nintro\n## One\na\n\n```\n# not a heading\n```\n## Two\nb\n# Next\nc\n";
        assert_eq!(
            fold_lines("markdown", source),
            [(1, 10), (3, 8), (9, 10), (11, 12)]
        );
        // A heading with nothing under it does not fold.
        assert_eq!(fold_lines("markdown", "# A\n# B\nx\n"), [(2, 3)]);
    }

    #[test]
    fn yaml_blocks_fold_by_indentation() {
        let source = "jobs:\n  build:\n    runs-on: x\n    # note\n\n    steps:\n      - run: |\n          echo\n  test: y\nother: 1\n";
        assert_eq!(fold_lines("yaml", source), [(1, 9), (2, 8), (6, 8), (7, 8)]);
        // Flow collections still fold at their brackets.
        assert!(fold_lines("yaml", "a: [1,\n  2]\n").contains(&(1, 2)));
    }

    #[test]
    fn languages_offer_their_words_to_completion() {
        let registry = LanguageRegistry::with_builtins();
        let rust = registry.vocabulary(&id("rust")).expect("rust");
        assert!(
            rust.keywords.iter().any(|word| word == "fn")
                && rust.types.iter().any(|word| word == "usize")
        );
        let sql = registry.vocabulary(&id("sql")).expect("sql");
        assert!(sql.case_insensitive && sql.keywords.iter().any(|word| word == "select"));
        assert!(registry
            .vocabulary(&id("html"))
            .expect("html")
            .keywords
            .iter()
            .any(|word| word == "div"));
        assert!(registry
            .vocabulary(&id("python"))
            .expect("python")
            .keywords
            .iter()
            .any(|word| word == "def"));
        assert!(registry
            .vocabulary(&id("json"))
            .expect("json")
            .keywords
            .iter()
            .any(|word| word == "null"));
        assert_eq!(registry.word_bytes(&id("javascript")), b"$");
        for none in ["plaintext", "markdown", "csv"] {
            assert_eq!(registry.vocabulary(&id(none)), None, "{none}");
        }
    }

    #[test]
    fn name_prefixes_rank_between_exact_names_and_extensions() {
        let mut registry = LanguageRegistry::with_builtins();
        assert_eq!(
            registry.detect_path(Path::new("Dockerfile.prod")).as_str(),
            "dockerfile"
        );
        assert_eq!(
            registry.detect_path(Path::new("Dockerfile.")).as_str(),
            "plaintext"
        );
        // An exact name beats a prefix; a prefix beats an extension.
        registry
            .register(
                plugin_language("special", &["yaml"], &["dockerfile.yaml"]),
                plugin("acme.special"),
            )
            .expect("register");
        assert_eq!(
            registry.detect_path(Path::new("Dockerfile.yaml")).as_str(),
            "special"
        );
        assert_eq!(
            registry.detect_path(Path::new("Dockerfile.yml")).as_str(),
            "dockerfile"
        );
        let rejected = registry.set_user_associations([("Jenkinsfile.*", "ruby")]);
        assert_eq!(rejected, Vec::<String>::new());
        assert_eq!(
            registry
                .detect_path(Path::new("Jenkinsfile.release"))
                .as_str(),
            "ruby"
        );
        assert_eq!(
            FileMatcher::parse("Jenkinsfile.*").expect("prefix"),
            FileMatcher::Prefix("jenkinsfile.".into())
        );
    }

    #[test]
    fn the_built_in_catalog_has_no_conflicts() {
        assert_eq!(LanguageRegistry::with_builtins().conflicts(), Vec::new());
    }

    #[test]
    fn plugin_languages_can_be_removed_but_built_in_ones_cannot() {
        let mut registry = LanguageRegistry::with_builtins();
        registry
            .register(plugin_language("nim", &["nim"], &[]), plugin("acme.nim"))
            .expect("register");
        assert!(matches!(
            registry.register(plugin_language("nim", &[], &[]), plugin("other")),
            Err(SyntaxError::DuplicateLanguage(_))
        ));
        assert!(matches!(
            registry.register(plugin_language("mine", &[], &[]), AssociationSource::User),
            Err(SyntaxError::InvalidAssociation(_))
        ));
        assert_eq!(registry.detect_path(Path::new("x.nim")).as_str(), "nim");
        assert!(registry
            .descriptors()
            .iter()
            .any(|descriptor| descriptor.id == id("nim")));

        registry.unregister(&id("nim")).expect("unregister");
        assert_eq!(
            registry.detect_path(Path::new("x.nim")).as_str(),
            "plaintext"
        );
        assert!(matches!(
            registry.analyze(&id("nim"), b"echo 1", 0, 1),
            Err(SyntaxError::UnknownLanguage(_))
        ));
        assert!(matches!(
            registry.unregister(&id("json")),
            Err(SyntaxError::BuiltinLanguage(_))
        ));
        assert!(matches!(
            registry.unregister(&id("nim")),
            Err(SyntaxError::UnknownLanguage(_))
        ));
    }

    fn declarative_module(first_line: Option<&str>) -> Result<LanguageModule, SyntaxError> {
        LanguageModule::declarative(
            LanguageDescriptor {
                id: id("nim"),
                display_name: "Nim".into(),
                extensions: vec!["nim".into()],
                filenames: vec!["nim.cfg".into()],
            },
            Some(DeclarativeLexer {
                mode: Some(DeclarativeMode::Code),
                keywords: vec!["proc".into(), "let".into()],
                types: vec!["int".into()],
                strings: vec![DeclarativeString {
                    open: "\"".into(),
                    close: "\"".into(),
                    escape: true,
                    multiline: false,
                }],
                case_insensitive: false,
                ident_chars: vec![b'_'],
            }),
            CommentSyntax {
                line: Some("#".into()),
                block: None,
            },
            Vocabulary {
                keywords: vec!["proc".into(), "let".into()],
                types: vec!["int".into()],
                case_insensitive: false,
                word_bytes: vec![b'_'],
            },
            first_line,
        )
    }

    #[test]
    fn declarative_language_data_builds_highlighting_completion_and_detection() {
        let mut registry = LanguageRegistry::with_builtins();
        let module = declarative_module(Some(r"^#!.*\bnim\b")).expect("declarative module");
        registry
            .register(module, plugin("acme.nim"))
            .expect("register");

        assert_eq!(registry.detect_path(Path::new("prog.nim")).as_str(), "nim");
        assert_eq!(registry.detect_path(Path::new("nim.cfg")).as_str(), "nim");
        assert_eq!(
            registry.detect_content(b"#!/usr/bin/env nim\n"),
            Some(id("nim"))
        );
        assert_eq!(registry.detect_content(b"echo 1\n"), None);
        let vocabulary = registry.vocabulary(&id("nim")).expect("vocabulary");
        assert!(vocabulary.keywords.contains(&"proc".to_owned()));
        assert_eq!(vocabulary.word_bytes, [b'_']);
        assert_eq!(
            registry
                .comment_syntax(&id("nim"))
                .and_then(|syntax| syntax.line),
            Some("#".into())
        );

        let analysis = registry
            .analyze(
                &id("nim"),
                b"proc go() =\n  let answer: int = 42 # note\n  echo \"hi\"\n",
                0,
                1,
            )
            .expect("analysis");
        let highlighted: Vec<(&str, SyntaxToken)> =
            analysis
                .highlights
                .iter()
                .map(|span| {
                    (
                        &"proc go() =\n  let answer: int = 42 # note\n  echo \"hi\"\n"
                            [usize::try_from(span.start_byte).expect("small")
                                ..usize::try_from(span.end_byte).expect("small")],
                        span.token,
                    )
                })
                .collect();
        assert!(highlighted.contains(&("proc", SyntaxToken::Keyword)));
        assert!(highlighted.contains(&("let", SyntaxToken::Keyword)));
        assert!(highlighted.contains(&("int", SyntaxToken::Type)));
        assert!(highlighted.contains(&("42", SyntaxToken::Number)));
        assert!(highlighted.contains(&("\"hi\"", SyntaxToken::String)));
        assert!(highlighted.contains(&("# note", SyntaxToken::Comment)));
    }

    #[test]
    fn hostile_declarative_data_is_rejected_with_a_typed_error() {
        assert!(matches!(
            declarative_module(Some("(")),
            Err(SyntaxError::InvalidDeclarative(_))
        ));
        let long = "a".repeat(MAX_DECLARATIVE_REGEX_BYTES + 1);
        assert!(matches!(
            declarative_module(Some(&long)),
            Err(SyntaxError::InvalidDeclarative(_))
        ));

        let lexer = DeclarativeLexer {
            strings: vec![DeclarativeString {
                open: String::new(),
                close: "\"".into(),
                escape: false,
                multiline: false,
            }],
            ..DeclarativeLexer::default()
        };
        assert!(matches!(
            LanguageModule::declarative(
                LanguageDescriptor {
                    id: id("nim"),
                    display_name: "Nim".into(),
                    extensions: Vec::new(),
                    filenames: Vec::new(),
                },
                Some(lexer),
                CommentSyntax::default(),
                Vocabulary::default(),
                None,
            ),
            Err(SyntaxError::InvalidDeclarative(_))
        ));
    }

    #[test]
    fn clearing_plugin_languages_keeps_builtins_and_removes_plugin_associations() {
        let mut registry = LanguageRegistry::with_builtins();
        registry
            .register(plugin_language("nim", &["nim"], &[]), plugin("acme.nim"))
            .expect("register");
        registry
            .add_plugin_associations("acme.json", &id("json"), &["acmejson".into()], &[])
            .expect("association");
        assert_eq!(
            registry.detect_path(Path::new("x.acmejson")).as_str(),
            "json"
        );
        assert_eq!(registry.detect_path(Path::new("x.nim")).as_str(), "nim");

        registry.clear_plugin_languages();
        assert!(!registry.contains(&id("nim")));
        assert!(registry.contains(&id("json")));
        assert_eq!(
            registry.detect_path(Path::new("x.nim")).as_str(),
            "plaintext"
        );
        assert_eq!(
            registry.detect_path(Path::new("x.acmejson")).as_str(),
            "plaintext"
        );
        assert!(matches!(
            registry.add_plugin_associations("acme.json", &id("gone"), &["x".into()], &[]),
            Err(SyntaxError::UnknownLanguage(_))
        ));
    }

    #[test]
    fn a_user_association_to_a_removed_language_is_ignored() {
        let mut registry = LanguageRegistry::with_builtins();
        registry
            .register(plugin_language("nim", &[], &[]), plugin("acme.nim"))
            .expect("register");
        assert_eq!(
            registry.set_user_associations([("*.nimble", "nim")]),
            Vec::<String>::new()
        );
        assert_eq!(registry.detect_path(Path::new("a.nimble")).as_str(), "nim");
        registry.unregister(&id("nim")).expect("unregister");
        assert_eq!(
            registry.detect_path(Path::new("a.nimble")).as_str(),
            "plaintext"
        );
        assert_eq!(
            registry.set_user_associations([("*.nimble", "nim")]),
            ["*.nimble"]
        );
        assert_eq!(
            registry.detect_path(Path::new("a.nimble")).as_str(),
            "plaintext"
        );
    }
}
