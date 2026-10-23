//! Table-driven lexical highlighting.
//!
//! Each language is plain data (comment and string delimiters, keyword lists,
//! a few flags), so adding one needs no grammar dependency. The scanner makes a
//! single bounded pass over a viewport window; like every windowed analysis it
//! cannot know that a comment or string began before the window.

use crate::{
    BracketPair, FoldRange, HighlightSpan, LanguageId, SyntaxAnalysis, SyntaxToken,
    MAX_BRACKET_PAIRS, MAX_FOLD_RANGES, MAX_HIGHLIGHT_SPANS,
};

const MAX_NESTING: usize = 1_024;
const MAX_ENTITY_BYTES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Statements, identifiers, and operators.
    Code,
    /// Tags, attributes, and entities.
    Markup,
    /// Selectors and declarations.
    Css,
    /// Delimiter-separated values (CSV, TSV): each column in its own color.
    Delimited,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StringRule {
    pub open: &'static str,
    pub close: &'static str,
    /// A backslash hides the next byte from the closing delimiter.
    pub escape: bool,
    pub multiline: bool,
}

const fn string(
    open: &'static str,
    close: &'static str,
    escape: bool,
    multiline: bool,
) -> StringRule {
    StringRule {
        open,
        close,
        escape,
        multiline,
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)] // Independent per-language switches read as a table.
pub(crate) struct LexerSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub mode: Mode,
    /// Lowercase when `case_insensitive` is set.
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub line_comments: &'static [&'static str],
    /// Words that start a comment running to the end of the line (`rem`).
    pub comment_words: &'static [&'static str],
    pub block_comments: &'static [(&'static str, &'static str)],
    pub strings: &'static [StringRule],
    pub case_insensitive: bool,
    /// Prefixes that mark a variable (`$name`, `@name`).
    pub sigils: &'static [u8],
    /// Extra bytes allowed inside an identifier.
    pub ident_chars: &'static [u8],
    /// `#directive` at the start of a line is a keyword.
    pub preprocessor: bool,
    /// An identifier in `CamelCase` is a type by convention.
    pub capital_types: bool,
    /// An identifier directly followed by this byte is a key (`key:` or `key =`).
    pub key_suffix: Option<u8>,
    /// Markup only: `<script>` and `<style>` bodies are skipped, not tokenized.
    pub raw_text_elements: bool,
    /// Delimited only: the field separator. A comma means "guess from the
    /// first line" among comma, semicolon, tab, and bar.
    pub delimiter: Option<u8>,
    /// YAML additions to the code scanner: document markers, anchors and
    /// aliases, tags, block scalars, and quoted keys.
    pub yaml: bool,
    /// Extra words offered by completion that are not highlighted as
    /// keywords (HTML element names).
    pub completion_words: &'static [&'static str],
}

const BASE: LexerSpec = LexerSpec {
    id: "",
    name: "",
    extensions: &[],
    filenames: &[],
    mode: Mode::Code,
    keywords: &[],
    types: &[],
    line_comments: &[],
    comment_words: &[],
    block_comments: &[],
    strings: &[],
    case_insensitive: false,
    sigils: &[],
    ident_chars: &[],
    preprocessor: false,
    capital_types: false,
    key_suffix: None,
    raw_text_elements: false,
    delimiter: None,
    yaml: false,
    completion_words: &[],
};

const C_STRINGS: &[StringRule] = &[
    string("\"", "\"", true, false),
    string("'", "'", true, false),
];
const QUOTES_MULTILINE: &[StringRule] =
    &[string("\"", "\"", true, true), string("'", "'", true, true)];
const SLASH_COMMENTS: &[&str] = &["//"];
const C_BLOCK: &[(&str, &str)] = &[("/*", "*/")];
const HASH: &[&str] = &["#"];

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for",
    "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static", "struct",
    "switch", "typedef", "union", "volatile", "while", "NULL", "true", "false",
];
const C_TYPES: &[&str] = &[
    "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
    "size_t", "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
    "uint32_t", "uint64_t", "wchar_t", "FILE",
];
const CPP_KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    "auto",
    "break",
    "case",
    "catch",
    "class",
    "co_await",
    "co_return",
    "co_yield",
    "concept",
    "const",
    "const_cast",
    "consteval",
    "constexpr",
    "constinit",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "final",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "nullptr",
    "operator",
    "override",
    "private",
    "protected",
    "public",
    "reinterpret_cast",
    "requires",
    "return",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "using",
    "virtual",
    "volatile",
    "while",
    "NULL",
];
const OBJC_KEYWORDS: &[&str] = &[
    "auto",
    "break",
    "case",
    "const",
    "continue",
    "default",
    "do",
    "else",
    "enum",
    "extern",
    "for",
    "goto",
    "if",
    "in",
    "inline",
    "return",
    "self",
    "sizeof",
    "static",
    "struct",
    "super",
    "switch",
    "typedef",
    "union",
    "volatile",
    "while",
    "nil",
    "Nil",
    "NULL",
    "YES",
    "NO",
    "true",
    "false",
    "interface",
    "implementation",
    "end",
    "property",
    "synthesize",
    "dynamic",
    "protocol",
    "optional",
    "required",
    "class",
    "selector",
    "import",
    "try",
    "catch",
    "finally",
    "throw",
    "autoreleasepool",
    "public",
    "private",
    "protected",
    "nonatomic",
    "atomic",
    "strong",
    "weak",
    "copy",
    "assign",
    "readonly",
    "readwrite",
];
const OBJC_TYPES: &[&str] = &[
    "BOOL",
    "char",
    "double",
    "float",
    "int",
    "long",
    "short",
    "signed",
    "unsigned",
    "void",
    "id",
    "instancetype",
    "SEL",
    "IMP",
    "Class",
    "NSInteger",
    "NSUInteger",
    "CGFloat",
    "NSString",
    "NSArray",
    "NSDictionary",
    "NSNumber",
    "NSObject",
];
const CSHARP_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "async",
    "await",
    "base",
    "break",
    "case",
    "catch",
    "checked",
    "class",
    "const",
    "continue",
    "default",
    "delegate",
    "do",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "for",
    "foreach",
    "get",
    "goto",
    "if",
    "implicit",
    "in",
    "init",
    "interface",
    "internal",
    "is",
    "lock",
    "namespace",
    "new",
    "null",
    "operator",
    "out",
    "override",
    "params",
    "partial",
    "private",
    "protected",
    "public",
    "readonly",
    "record",
    "ref",
    "return",
    "sealed",
    "set",
    "sizeof",
    "stackalloc",
    "static",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "unchecked",
    "unsafe",
    "using",
    "var",
    "virtual",
    "volatile",
    "when",
    "where",
    "while",
    "yield",
];
const CSHARP_TYPES: &[&str] = &[
    "bool", "byte", "char", "decimal", "double", "dynamic", "float", "int", "long", "nint",
    "nuint", "object", "sbyte", "short", "string", "uint", "ulong", "ushort", "void",
];
const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "interface",
    "native",
    "new",
    "null",
    "package",
    "permits",
    "private",
    "protected",
    "public",
    "record",
    "return",
    "sealed",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "var",
    "volatile",
    "while",
    "yield",
];
const JAVA_TYPES: &[&str] = &[
    "boolean", "byte", "char", "double", "float", "int", "long", "short", "void",
];
const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];
const TS_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "asserts",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "infer",
    "instanceof",
    "interface",
    "is",
    "keyof",
    "let",
    "module",
    "namespace",
    "new",
    "null",
    "of",
    "override",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "satisfies",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];
const TS_TYPES: &[&str] = &[
    "any", "bigint", "boolean", "never", "number", "object", "string", "symbol", "unknown",
];
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "union",
    "unsafe", "use", "where", "while",
];
const RUST_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8", "u16",
    "u32", "u64", "u128", "usize",
];
const SWIFT_KEYWORDS: &[&str] = &[
    "actor",
    "as",
    "associatedtype",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "continue",
    "default",
    "defer",
    "deinit",
    "do",
    "else",
    "enum",
    "extension",
    "fallthrough",
    "false",
    "fileprivate",
    "final",
    "for",
    "func",
    "get",
    "guard",
    "if",
    "import",
    "in",
    "init",
    "inout",
    "internal",
    "is",
    "lazy",
    "let",
    "mutating",
    "nil",
    "open",
    "operator",
    "override",
    "private",
    "protocol",
    "public",
    "repeat",
    "rethrows",
    "return",
    "self",
    "Self",
    "set",
    "some",
    "static",
    "struct",
    "subscript",
    "super",
    "switch",
    "throw",
    "throws",
    "true",
    "try",
    "typealias",
    "var",
    "weak",
    "where",
    "while",
];
const ZIG_KEYWORDS: &[&str] = &[
    "addrspace",
    "align",
    "allowzero",
    "and",
    "anyframe",
    "anytype",
    "asm",
    "async",
    "await",
    "break",
    "callconv",
    "catch",
    "comptime",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "errdefer",
    "error",
    "export",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "inline",
    "noalias",
    "nosuspend",
    "null",
    "opaque",
    "or",
    "orelse",
    "packed",
    "pub",
    "resume",
    "return",
    "struct",
    "suspend",
    "switch",
    "test",
    "threadlocal",
    "true",
    "try",
    "undefined",
    "union",
    "unreachable",
    "usingnamespace",
    "var",
    "volatile",
    "while",
];
const ZIG_TYPES: &[&str] = &[
    "bool",
    "void",
    "noreturn",
    "type",
    "anyerror",
    "anyopaque",
    "comptime_int",
    "comptime_float",
    "f16",
    "f32",
    "f64",
    "f128",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "isize",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "usize",
    "c_int",
    "c_uint",
    "c_char",
];
const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "false",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "iota",
    "map",
    "nil",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "true",
    "type",
    "var",
];
const GO_TYPES: &[&str] = &[
    "any",
    "bool",
    "byte",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
];
const KOTLIN_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "break",
    "by",
    "catch",
    "class",
    "companion",
    "const",
    "constructor",
    "continue",
    "data",
    "do",
    "else",
    "enum",
    "false",
    "final",
    "finally",
    "for",
    "fun",
    "get",
    "if",
    "import",
    "in",
    "init",
    "inline",
    "interface",
    "internal",
    "is",
    "lateinit",
    "null",
    "object",
    "open",
    "operator",
    "out",
    "override",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "sealed",
    "set",
    "super",
    "suspend",
    "this",
    "throw",
    "true",
    "try",
    "typealias",
    "val",
    "var",
    "vararg",
    "when",
    "where",
    "while",
];
const BASH_KEYWORDS: &[&str] = &[
    "alias", "break", "case", "cd", "continue", "declare", "do", "done", "echo", "elif", "else",
    "esac", "eval", "exec", "exit", "export", "fi", "for", "function", "if", "in", "local",
    "printf", "read", "readonly", "return", "select", "set", "shift", "source", "then", "time",
    "trap", "unset", "until", "while",
];
const BATCH_KEYWORDS: &[&str] = &[
    "call",
    "cd",
    "chdir",
    "cls",
    "copy",
    "defined",
    "del",
    "dir",
    "do",
    "echo",
    "else",
    "endlocal",
    "equ",
    "errorlevel",
    "exist",
    "exit",
    "for",
    "geq",
    "goto",
    "gtr",
    "if",
    "in",
    "leq",
    "lss",
    "md",
    "mkdir",
    "move",
    "neq",
    "not",
    "nul",
    "off",
    "on",
    "pause",
    "popd",
    "pushd",
    "rd",
    "ren",
    "rmdir",
    "set",
    "setlocal",
    "shift",
    "start",
    "title",
    "type",
];
const POWERSHELL_KEYWORDS: &[&str] = &[
    "begin",
    "break",
    "catch",
    "class",
    "continue",
    "data",
    "do",
    "dynamicparam",
    "else",
    "elseif",
    "end",
    "enum",
    "exit",
    "filter",
    "finally",
    "for",
    "foreach",
    "from",
    "function",
    "if",
    "in",
    "param",
    "process",
    "return",
    "switch",
    "throw",
    "trap",
    "try",
    "until",
    "using",
    "while",
];
const PERL_KEYWORDS: &[&str] = &[
    "chomp", "close", "defined", "delete", "die", "do", "each", "else", "elsif", "eq", "exists",
    "for", "foreach", "ge", "gt", "if", "keys", "last", "le", "local", "lt", "my", "ne", "next",
    "no", "open", "our", "package", "print", "push", "redo", "require", "return", "shift", "sub",
    "undef", "unless", "until", "use", "values", "while",
];
const PHP_KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "array",
    "as",
    "break",
    "case",
    "catch",
    "class",
    "clone",
    "const",
    "continue",
    "declare",
    "default",
    "do",
    "echo",
    "else",
    "elseif",
    "empty",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "function",
    "global",
    "if",
    "implements",
    "include",
    "include_once",
    "instanceof",
    "interface",
    "isset",
    "list",
    "match",
    "namespace",
    "new",
    "null",
    "or",
    "print",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "require_once",
    "return",
    "static",
    "switch",
    "throw",
    "trait",
    "true",
    "try",
    "unset",
    "use",
    "var",
    "while",
    "yield",
];
const RUBY_KEYWORDS: &[&str] = &[
    "alias",
    "and",
    "attr_accessor",
    "attr_reader",
    "attr_writer",
    "begin",
    "break",
    "case",
    "class",
    "def",
    "defined?",
    "do",
    "else",
    "elsif",
    "end",
    "ensure",
    "extend",
    "false",
    "for",
    "if",
    "in",
    "include",
    "module",
    "next",
    "nil",
    "not",
    "or",
    "private",
    "protected",
    "public",
    "raise",
    "redo",
    "require",
    "require_relative",
    "rescue",
    "retry",
    "return",
    "self",
    "super",
    "then",
    "true",
    "undef",
    "unless",
    "until",
    "when",
    "while",
    "yield",
];
const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const APPLESCRIPT_KEYWORDS: &[&str] = &[
    "about",
    "above",
    "after",
    "against",
    "and",
    "apart",
    "around",
    "as",
    "at",
    "back",
    "before",
    "beginning",
    "behind",
    "below",
    "beneath",
    "beside",
    "between",
    "but",
    "by",
    "considering",
    "contain",
    "contains",
    "continue",
    "copy",
    "div",
    "does",
    "eighth",
    "else",
    "end",
    "equal",
    "equals",
    "error",
    "every",
    "exit",
    "false",
    "fifth",
    "first",
    "for",
    "fourth",
    "from",
    "front",
    "get",
    "given",
    "global",
    "if",
    "ignoring",
    "in",
    "instead",
    "into",
    "is",
    "it",
    "its",
    "last",
    "local",
    "me",
    "middle",
    "mod",
    "my",
    "ninth",
    "not",
    "of",
    "on",
    "onto",
    "or",
    "out",
    "over",
    "prop",
    "property",
    "put",
    "ref",
    "reference",
    "repeat",
    "return",
    "returning",
    "script",
    "second",
    "set",
    "seventh",
    "since",
    "sixth",
    "some",
    "tell",
    "tenth",
    "that",
    "the",
    "then",
    "third",
    "through",
    "thru",
    "timeout",
    "times",
    "to",
    "transaction",
    "true",
    "try",
    "until",
    "where",
    "while",
    "whose",
    "with",
    "without",
    "application",
    "display",
    "dialog",
    "activate",
];
const VB_KEYWORDS: &[&str] = &[
    "addhandler",
    "addressof",
    "and",
    "andalso",
    "as",
    "byref",
    "byval",
    "call",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "dim",
    "do",
    "each",
    "else",
    "elseif",
    "end",
    "enum",
    "erase",
    "error",
    "event",
    "exit",
    "false",
    "finally",
    "for",
    "friend",
    "function",
    "get",
    "gettype",
    "global",
    "goto",
    "handles",
    "if",
    "implements",
    "imports",
    "in",
    "inherits",
    "interface",
    "is",
    "let",
    "lib",
    "like",
    "loop",
    "me",
    "mod",
    "module",
    "mustinherit",
    "mustoverride",
    "mybase",
    "namespace",
    "new",
    "next",
    "not",
    "nothing",
    "null",
    "on",
    "option",
    "optional",
    "or",
    "orelse",
    "overloads",
    "overridable",
    "overrides",
    "paramarray",
    "preserve",
    "private",
    "property",
    "protected",
    "public",
    "raiseevent",
    "readonly",
    "redim",
    "resume",
    "return",
    "select",
    "set",
    "shadows",
    "shared",
    "static",
    "step",
    "stop",
    "structure",
    "sub",
    "then",
    "throw",
    "to",
    "true",
    "try",
    "until",
    "wend",
    "when",
    "while",
    "with",
    "withevents",
    "writeonly",
    "xor",
    "response",
    "request",
    "server",
    "session",
    "wscript",
    "createobject",
    "empty",
    "explicit",
];
const VB_TYPES: &[&str] = &[
    "boolean", "byte", "char", "currency", "date", "decimal", "double", "integer", "long",
    "object", "sbyte", "short", "single", "string", "uinteger", "ulong", "ushort", "variant",
];
const SQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "any",
    "as",
    "asc",
    "begin",
    "between",
    "by",
    "cascade",
    "case",
    "check",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "exists",
    "foreign",
    "from",
    "full",
    "function",
    "grant",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "procedure",
    "references",
    "return",
    "returns",
    "revoke",
    "right",
    "rollback",
    "schema",
    "select",
    "set",
    "table",
    "then",
    "to",
    "transaction",
    "trigger",
    "truncate",
    "union",
    "unique",
    "update",
    "values",
    "view",
    "when",
    "where",
    "with",
    "true",
    "false",
];
const MYSQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "analyze",
    "and",
    "as",
    "asc",
    "auto_increment",
    "begin",
    "between",
    "by",
    "cascade",
    "case",
    "change",
    "character",
    "charset",
    "check",
    "collate",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "database",
    "databases",
    "default",
    "delayed",
    "delete",
    "delimiter",
    "desc",
    "describe",
    "distinct",
    "drop",
    "duplicate",
    "else",
    "end",
    "engine",
    "exists",
    "explain",
    "foreign",
    "from",
    "fulltext",
    "function",
    "grant",
    "group",
    "having",
    "if",
    "ignore",
    "in",
    "index",
    "inner",
    "insert",
    "into",
    "is",
    "join",
    "key",
    "keys",
    "left",
    "like",
    "limit",
    "lock",
    "modify",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "procedure",
    "references",
    "regexp",
    "rename",
    "replace",
    "return",
    "returns",
    "revoke",
    "right",
    "rollback",
    "schema",
    "select",
    "set",
    "show",
    "straight_join",
    "table",
    "tables",
    "then",
    "to",
    "transaction",
    "trigger",
    "truncate",
    "union",
    "unique",
    "unlock",
    "unsigned",
    "update",
    "use",
    "using",
    "values",
    "view",
    "when",
    "where",
    "with",
    "zerofill",
    "true",
    "false",
];
const PLSQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "any",
    "as",
    "asc",
    "begin",
    "between",
    "body",
    "bulk",
    "by",
    "cascade",
    "case",
    "check",
    "close",
    "collect",
    "column",
    "commit",
    "connect",
    "constant",
    "constraint",
    "create",
    "cursor",
    "declare",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "dual",
    "else",
    "elsif",
    "end",
    "exception",
    "execute",
    "exists",
    "exit",
    "fetch",
    "for",
    "forall",
    "foreign",
    "from",
    "function",
    "grant",
    "group",
    "having",
    "if",
    "immediate",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "loop",
    "merge",
    "minus",
    "not",
    "nowait",
    "null",
    "of",
    "on",
    "open",
    "or",
    "order",
    "out",
    "outer",
    "package",
    "pragma",
    "primary",
    "prior",
    "procedure",
    "raise",
    "record",
    "references",
    "replace",
    "return",
    "returning",
    "revoke",
    "right",
    "rollback",
    "rowtype",
    "savepoint",
    "select",
    "sequence",
    "set",
    "start",
    "synonym",
    "table",
    "then",
    "to",
    "trigger",
    "truncate",
    "type",
    "union",
    "unique",
    "update",
    "using",
    "values",
    "view",
    "when",
    "where",
    "while",
    "with",
    "true",
    "false",
    "sysdate",
    "rownum",
];
const SQL_TYPES: &[&str] = &[
    "bigint",
    "binary",
    "bit",
    "blob",
    "boolean",
    "char",
    "clob",
    "date",
    "datetime",
    "decimal",
    "double",
    "enum",
    "float",
    "int",
    "integer",
    "json",
    "longtext",
    "mediumint",
    "mediumtext",
    "nchar",
    "number",
    "numeric",
    "nvarchar",
    "nvarchar2",
    "pls_integer",
    "real",
    "smallint",
    "text",
    "time",
    "timestamp",
    "tinyint",
    "varbinary",
    "varchar",
    "varchar2",
    "year",
];
const SQL_STRINGS: &[StringRule] = &[
    string("'", "'", false, true),
    string("\"", "\"", false, false),
    string("`", "`", false, false),
];
const DOCKER_KEYWORDS: &[&str] = &[
    "add",
    "arg",
    "as",
    "cmd",
    "copy",
    "entrypoint",
    "env",
    "expose",
    "from",
    "healthcheck",
    "label",
    "maintainer",
    "onbuild",
    "run",
    "shell",
    "stopsignal",
    "user",
    "volume",
    "workdir",
];
const MAKE_KEYWORDS: &[&str] = &[
    "define", "else", "endef", "endif", "export", "ifdef", "ifeq", "ifndef", "ifneq", "include",
    "override", "unexport", "vpath",
];
const HTML_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "audio",
    "blockquote",
    "body",
    "button",
    "canvas",
    "caption",
    "code",
    "details",
    "dialog",
    "div",
    "dl",
    "dd",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "html",
    "iframe",
    "img",
    "input",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "meta",
    "nav",
    "noscript",
    "ol",
    "optgroup",
    "option",
    "p",
    "picture",
    "pre",
    "script",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "summary",
    "svg",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "ul",
    "video",
    "class",
    "href",
    "src",
    "alt",
    "type",
    "name",
    "value",
    "placeholder",
    "rel",
    "lang",
    "charset",
    "content",
    "width",
    "height",
    "hidden",
    "disabled",
    "checked",
    "selected",
];

/// Python's keywords and built-in names, for completion (Python is parsed by
/// a grammar, not a lexer table).
pub(crate) const PYTHON_WORDS: &[&str] = &[
    "False",
    "None",
    "True",
    "and",
    "as",
    "assert",
    "async",
    "await",
    "break",
    "class",
    "continue",
    "def",
    "del",
    "elif",
    "else",
    "except",
    "finally",
    "for",
    "from",
    "global",
    "if",
    "import",
    "in",
    "is",
    "lambda",
    "match",
    "case",
    "nonlocal",
    "not",
    "or",
    "pass",
    "raise",
    "return",
    "try",
    "while",
    "with",
    "yield",
    "self",
    "print",
    "len",
    "range",
    "enumerate",
    "isinstance",
    "super",
    "open",
    "dict",
    "list",
    "set",
    "tuple",
    "str",
    "int",
    "float",
    "bool",
    "bytes",
    "object",
    "Exception",
    "ValueError",
    "TypeError",
    "KeyError",
    "__init__",
    "__name__",
];

const YAML_KEYWORDS: &[&str] = &["true", "false", "null", "yes", "no", "on", "off"];

/// Every table-driven language. Order only matters for ties in detection.
pub(crate) const SPECS: &[LexerSpec] = &[
    LexerSpec {
        id: "applescript",
        name: "AppleScript",
        extensions: &["applescript", "scpt"],
        keywords: APPLESCRIPT_KEYWORDS,
        line_comments: &["--", "#"],
        block_comments: &[("(*", "*)")],
        strings: &[string("\"", "\"", true, false)],
        case_insensitive: true,
        ..BASE
    },
    LexerSpec {
        id: "bash",
        name: "Bash",
        extensions: &["sh", "bash", "zsh", "ksh", "command"],
        filenames: &[
            ".bashrc",
            ".bash_profile",
            ".bash_aliases",
            ".profile",
            ".zshrc",
            ".zprofile",
            ".zshenv",
        ],
        keywords: BASH_KEYWORDS,
        line_comments: HASH,
        strings: &[
            string("\"", "\"", true, true),
            string("'", "'", false, true),
        ],
        sigils: b"$",
        ..BASE
    },
    LexerSpec {
        id: "batch",
        name: "Batch",
        extensions: &["bat", "cmd"],
        keywords: BATCH_KEYWORDS,
        line_comments: &["::"],
        comment_words: &["rem"],
        strings: &[string("\"", "\"", false, false)],
        case_insensitive: true,
        ..BASE
    },
    LexerSpec {
        id: "c",
        name: "C",
        extensions: &["c", "h"],
        keywords: C_KEYWORDS,
        types: C_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: C_STRINGS,
        preprocessor: true,
        ..BASE
    },
    LexerSpec {
        id: "cpp",
        name: "C++",
        extensions: &[
            "cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ino", "ipp", "tpp",
        ],
        keywords: CPP_KEYWORDS,
        types: C_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: C_STRINGS,
        preprocessor: true,
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "csharp",
        name: "C#",
        extensions: &["cs", "csx"],
        keywords: CSHARP_KEYWORDS,
        types: CSHARP_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("@\"", "\"", false, true),
            string("\"", "\"", true, false),
            string("'", "'", true, false),
        ],
        preprocessor: true,
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "css",
        name: "CSS",
        extensions: &["css", "scss", "less"],
        mode: Mode::Css,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: C_STRINGS,
        ..BASE
    },
    LexerSpec {
        id: "csv",
        name: "CSV",
        extensions: &["csv"],
        mode: Mode::Delimited,
        delimiter: Some(b','),
        ..BASE
    },
    LexerSpec {
        id: "dockerfile",
        name: "Dockerfile",
        extensions: &["dockerfile"],
        filenames: &["dockerfile", "containerfile"],
        keywords: DOCKER_KEYWORDS,
        line_comments: HASH,
        strings: C_STRINGS,
        case_insensitive: true,
        sigils: b"$",
        ..BASE
    },
    LexerSpec {
        id: "go",
        name: "Go",
        extensions: &["go"],
        keywords: GO_KEYWORDS,
        types: GO_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"", "\"", true, false),
            string("`", "`", false, true),
            string("'", "'", true, false),
        ],
        ..BASE
    },
    LexerSpec {
        id: "html",
        name: "HTML",
        extensions: &["html", "htm", "xhtml", "vue", "svelte"],
        mode: Mode::Markup,
        raw_text_elements: true,
        case_insensitive: true,
        completion_words: HTML_ELEMENTS,
        ..BASE
    },
    LexerSpec {
        id: "ini",
        name: "INI",
        extensions: &["ini", "cfg", "conf", "properties", "editorconfig"],
        filenames: &[".editorconfig", ".gitconfig"],
        line_comments: &[";", "#"],
        strings: C_STRINGS,
        ident_chars: b"-.",
        key_suffix: Some(b'='),
        ..BASE
    },
    LexerSpec {
        id: "java",
        name: "Java",
        extensions: &["java"],
        keywords: JAVA_KEYWORDS,
        types: JAVA_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"\"\"", "\"\"\"", true, true),
            string("\"", "\"", true, false),
            string("'", "'", true, false),
        ],
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "javascript",
        name: "JavaScript",
        extensions: &["js", "mjs", "cjs", "jsx"],
        keywords: JS_KEYWORDS,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"", "\"", true, false),
            string("'", "'", true, false),
            string("`", "`", true, true),
        ],
        ident_chars: b"$",
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "kotlin",
        name: "Kotlin",
        extensions: &["kt", "kts"],
        keywords: KOTLIN_KEYWORDS,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"\"\"", "\"\"\"", false, true),
            string("\"", "\"", true, false),
            string("'", "'", true, false),
        ],
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "lua",
        name: "Lua",
        extensions: &["lua"],
        keywords: LUA_KEYWORDS,
        line_comments: &["--"],
        block_comments: &[("--[[", "]]")],
        strings: &[
            string("[[", "]]", false, true),
            string("\"", "\"", true, false),
            string("'", "'", true, false),
        ],
        ..BASE
    },
    LexerSpec {
        id: "makefile",
        name: "Makefile",
        extensions: &["mk", "mak"],
        filenames: &["makefile", "gnumakefile"],
        keywords: MAKE_KEYWORDS,
        line_comments: HASH,
        strings: C_STRINGS,
        sigils: b"$",
        ..BASE
    },
    LexerSpec {
        id: "mysql",
        name: "MySQL / MariaDB",
        extensions: &["mysql", "mariadb"],
        keywords: MYSQL_KEYWORDS,
        types: SQL_TYPES,
        line_comments: &["-- ", "--\t", "#"],
        block_comments: C_BLOCK,
        strings: SQL_STRINGS,
        case_insensitive: true,
        sigils: b"@",
        ..BASE
    },
    LexerSpec {
        id: "objective-c",
        name: "Objective-C/C++",
        extensions: &["m", "mm"],
        keywords: OBJC_KEYWORDS,
        types: OBJC_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("@\"", "\"", true, false),
            string("\"", "\"", true, false),
            string("'", "'", true, false),
        ],
        preprocessor: true,
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "perl",
        name: "Perl",
        extensions: &["pl", "pm", "t", "perl"],
        keywords: PERL_KEYWORDS,
        line_comments: HASH,
        block_comments: &[("=pod", "=cut"), ("=head1", "=cut")],
        strings: QUOTES_MULTILINE,
        sigils: b"$@%",
        ..BASE
    },
    LexerSpec {
        id: "php",
        name: "PHP",
        extensions: &["php", "phtml", "php3", "php4", "php5", "phps"],
        keywords: PHP_KEYWORDS,
        types: &[
            "bool", "float", "int", "iterable", "mixed", "object", "string", "void",
        ],
        line_comments: &["//", "#"],
        block_comments: C_BLOCK,
        strings: QUOTES_MULTILINE,
        sigils: b"$",
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "plsql",
        name: "Oracle SQL",
        extensions: &["pls", "plsql", "pks", "pkb", "plb", "ora"],
        keywords: PLSQL_KEYWORDS,
        types: SQL_TYPES,
        line_comments: &["--"],
        block_comments: C_BLOCK,
        strings: SQL_STRINGS,
        case_insensitive: true,
        ..BASE
    },
    LexerSpec {
        id: "powershell",
        name: "PowerShell",
        extensions: &["ps1", "psm1", "psd1"],
        keywords: POWERSHELL_KEYWORDS,
        line_comments: HASH,
        block_comments: &[("<#", "#>")],
        strings: &[
            string("@\"", "\"@", false, true),
            string("@'", "'@", false, true),
            string("\"", "\"", false, true),
            string("'", "'", false, true),
        ],
        case_insensitive: true,
        sigils: b"$",
        ident_chars: b"-",
        ..BASE
    },
    LexerSpec {
        id: "ruby",
        name: "Ruby",
        extensions: &["rb", "rake", "gemspec", "ru"],
        filenames: &["rakefile", "gemfile", "podfile", "fastfile", "vagrantfile"],
        keywords: RUBY_KEYWORDS,
        line_comments: HASH,
        block_comments: &[("=begin", "=end")],
        strings: QUOTES_MULTILINE,
        sigils: b"@$",
        ident_chars: b"?",
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "rust",
        name: "Rust",
        extensions: &["rs"],
        keywords: RUST_KEYWORDS,
        types: RUST_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        // No `'` rule: a lifetime would be mistaken for an unterminated character.
        strings: &[
            string("r#\"", "\"#", false, true),
            string("b\"", "\"", true, true),
            string("\"", "\"", true, true),
        ],
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "sql",
        name: "SQL",
        extensions: &["sql", "ddl", "dml"],
        keywords: SQL_KEYWORDS,
        types: SQL_TYPES,
        line_comments: &["--"],
        block_comments: C_BLOCK,
        strings: SQL_STRINGS,
        case_insensitive: true,
        ..BASE
    },
    LexerSpec {
        id: "swift",
        name: "Swift",
        extensions: &["swift"],
        keywords: SWIFT_KEYWORDS,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"\"\"", "\"\"\"", true, true),
            string("\"", "\"", true, false),
        ],
        preprocessor: true,
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "toml",
        name: "TOML",
        extensions: &["toml"],
        filenames: &["cargo.lock", "pipfile"],
        keywords: &["true", "false"],
        line_comments: HASH,
        strings: &[
            string("\"\"\"", "\"\"\"", true, true),
            string("'''", "'''", false, true),
            string("\"", "\"", true, false),
            string("'", "'", false, false),
        ],
        ident_chars: b"-.",
        key_suffix: Some(b'='),
        ..BASE
    },
    LexerSpec {
        id: "tsv",
        name: "TSV",
        extensions: &["tsv", "tab"],
        mode: Mode::Delimited,
        delimiter: Some(b'\t'),
        ..BASE
    },
    LexerSpec {
        id: "typescript",
        name: "TypeScript",
        extensions: &["ts", "tsx", "mts", "cts"],
        keywords: TS_KEYWORDS,
        types: TS_TYPES,
        line_comments: SLASH_COMMENTS,
        block_comments: C_BLOCK,
        strings: &[
            string("\"", "\"", true, false),
            string("'", "'", true, false),
            string("`", "`", true, true),
        ],
        ident_chars: b"$",
        capital_types: true,
        ..BASE
    },
    LexerSpec {
        id: "vb",
        name: "Visual Basic",
        extensions: &["vb", "bas", "cls", "frm", "vba"],
        keywords: VB_KEYWORDS,
        types: VB_TYPES,
        line_comments: &["'"],
        comment_words: &["rem"],
        strings: &[string("\"", "\"", false, false)],
        case_insensitive: true,
        preprocessor: true,
        ..BASE
    },
    LexerSpec {
        id: "vbscript",
        name: "VBScript ASP",
        extensions: &["vbs", "vbe", "asp", "wsf"],
        keywords: VB_KEYWORDS,
        types: VB_TYPES,
        line_comments: &["'"],
        comment_words: &["rem"],
        strings: &[string("\"", "\"", false, false)],
        case_insensitive: true,
        ..BASE
    },
    LexerSpec {
        id: "xml",
        name: "XML",
        extensions: &[
            "xml",
            "xsd",
            "svg",
            "xaml",
            "csproj",
            "vbproj",
            "fsproj",
            "props",
            "targets",
            "resx",
            "rss",
            "atom",
            "wsdl",
            "kml",
            "gpx",
            "storyboard",
            "xib",
        ],
        mode: Mode::Markup,
        ..BASE
    },
    LexerSpec {
        id: "plist",
        name: "Property List",
        extensions: &["plist"],
        mode: Mode::Markup,
        ..BASE
    },
    LexerSpec {
        id: "xsl",
        name: "XSL",
        extensions: &["xsl", "xslt"],
        mode: Mode::Markup,
        ..BASE
    },
    LexerSpec {
        id: "yaml",
        name: "YAML",
        extensions: &["yaml", "yml"],
        filenames: &[".clang-format"],
        yaml: true,
        keywords: YAML_KEYWORDS,
        line_comments: HASH,
        strings: C_STRINGS,
        ident_chars: b"-.",
        key_suffix: Some(b':'),
        ..BASE
    },
    LexerSpec {
        id: "zig",
        name: "Zig",
        extensions: &["zig", "zon"],
        keywords: ZIG_KEYWORDS,
        types: ZIG_TYPES,
        line_comments: SLASH_COMMENTS,
        strings: C_STRINGS,
        capital_types: true,
        ..BASE
    },
];

struct Scan<'a> {
    spec: &'a LexerSpec,
    src: &'a [u8],
    pos: usize,
    /// Rows below the first row of the window.
    row: u64,
    base_byte: u64,
    base_line: u64,
    brackets: Vec<(u8, usize, u64)>,
    out: SyntaxAnalysis,
}

/// Highlights one bounded window with a language table.
pub(crate) fn analyze(
    spec: &LexerSpec,
    source: &[u8],
    base_byte: u64,
    base_line: u64,
    truncated: bool,
) -> SyntaxAnalysis {
    let mut scan = Scan {
        spec,
        src: source,
        pos: 0,
        row: 0,
        base_byte,
        base_line,
        brackets: Vec::new(),
        out: SyntaxAnalysis {
            language_id: LanguageId(spec.id.into()),
            parsed_bytes: source.len(),
            highlights: Vec::new(),
            folds: Vec::new(),
            symbols: Vec::new(),
            bracket_pairs: Vec::new(),
            truncated,
            timed_out: false,
        },
    };
    match spec.mode {
        Mode::Code => scan.code(),
        Mode::Markup => scan.markup(),
        Mode::Css => scan.css(),
        Mode::Delimited => scan.delimited(),
    }
    scan.out
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

impl Scan<'_> {
    fn at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.src[self.pos..].starts_with(prefix.as_bytes())
    }

    fn find(&self, from: usize, needle: &str) -> Option<usize> {
        let needle = needle.as_bytes();
        self.src
            .get(from..)?
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|index| from + index)
    }

    fn line_end(&self, from: usize) -> usize {
        self.src[from..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(self.src.len(), |index| from + index)
    }

    fn span(&mut self, start: usize, end: usize, token: SyntaxToken) {
        if start >= end {
            return;
        }
        if self.out.highlights.len() >= MAX_HIGHLIGHT_SPANS {
            self.out.truncated = true;
            return;
        }
        self.out.highlights.push(HighlightSpan {
            start_byte: self.base_byte.saturating_add(start as u64),
            end_byte: self.base_byte.saturating_add(end as u64),
            token,
        });
    }

    /// Emits a token and moves past it, counting the rows it covers.
    fn take(&mut self, end: usize, token: SyntaxToken) {
        let end = end.min(self.src.len());
        self.span(self.pos, end, token);
        self.skip_to(end);
    }

    #[allow(clippy::naive_bytecount)] // Windows are bounded; no extra dependency is warranted.
    fn skip_to(&mut self, end: usize) {
        let end = end.min(self.src.len());
        self.row += self.src[self.pos..end]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u64;
        self.pos = end;
    }

    fn ident_end(&self, from: usize) -> usize {
        let extra = self.spec.ident_chars;
        self.src[from..]
            .iter()
            .position(|byte| {
                !(is_ident_start(*byte) || byte.is_ascii_digit() || extra.contains(byte))
            })
            .map_or(self.src.len(), |index| from + index)
    }

    /// The next byte that is not a space or tab, on the same line.
    fn next_significant(&self, from: usize) -> Option<u8> {
        self.src[from..]
            .iter()
            .copied()
            .find(|byte| !matches!(byte, b' ' | b'\t'))
    }

    fn word(&self, start: usize, end: usize) -> String {
        let text = String::from_utf8_lossy(&self.src[start..end]);
        if self.spec.case_insensitive {
            text.to_ascii_lowercase()
        } else {
            text.into_owned()
        }
    }

    fn comment(&mut self) -> bool {
        for (open, close) in self.spec.block_comments {
            if self.starts_with(open) {
                let end = self
                    .find(self.pos + open.len(), close)
                    .map_or(self.src.len(), |index| index + close.len());
                self.take(end, SyntaxToken::Comment);
                return true;
            }
        }
        for prefix in self.spec.line_comments {
            if self.starts_with(prefix) {
                self.take(self.line_end(self.pos), SyntaxToken::Comment);
                return true;
            }
        }
        false
    }

    fn string(&mut self) -> bool {
        for rule in self.spec.strings {
            if !self.starts_with(rule.open) {
                continue;
            }
            let close = rule.close.as_bytes();
            let mut cursor = self.pos + rule.open.len();
            let end = loop {
                let Some(byte) = self.src.get(cursor).copied() else {
                    break self.src.len();
                };
                if rule.escape && byte == b'\\' {
                    cursor += 2;
                } else if self.src[cursor..].starts_with(close) {
                    break cursor + close.len();
                } else if byte == b'\n' && !rule.multiline {
                    break cursor;
                } else {
                    cursor += 1;
                }
            };
            self.take(end, SyntaxToken::String);
            return true;
        }
        false
    }

    fn number(&mut self) -> bool {
        let Some(byte) = self.at(0) else { return false };
        let after_dot = self.pos > 0 && self.src[self.pos - 1] == b'.';
        let leading_dot =
            byte == b'.' && !after_dot && self.at(1).is_some_and(|next| next.is_ascii_digit());
        if !byte.is_ascii_digit() && !leading_dot {
            return false;
        }
        let mut end = self.pos + 1;
        while let Some(next) = self.src.get(end).copied() {
            // `1..10` is a range, not one number.
            let range = next == b'.' && self.src.get(end + 1) == Some(&b'.');
            if range || !(next.is_ascii_alphanumeric() || matches!(next, b'.' | b'_')) {
                break;
            }
            end += 1;
        }
        self.take(end, SyntaxToken::Number);
        true
    }

    fn open_bracket(&mut self, byte: u8) {
        if self.brackets.len() < MAX_NESTING {
            self.brackets.push((byte, self.pos, self.row));
        }
    }

    fn close_bracket(&mut self, byte: u8) {
        let open = match byte {
            b')' => b'(',
            b']' => b'[',
            _ => b'{',
        };
        // Recover from a mismatch by closing back to the nearest matching opener.
        let Some(index) = self.brackets.iter().rposition(|(kind, ..)| *kind == open) else {
            return;
        };
        let (_, start, row) = self.brackets[index];
        self.brackets.truncate(index);
        if self.out.bracket_pairs.len() < MAX_BRACKET_PAIRS {
            self.out.bracket_pairs.push(BracketPair {
                open_byte: self.base_byte.saturating_add(start as u64),
                close_byte: self.base_byte.saturating_add(self.pos as u64),
            });
        } else {
            self.out.truncated = true;
        }
        if open == b'{' && self.row > row {
            self.fold(start, self.pos + 1, row);
        }
    }

    fn fold(&mut self, start: usize, end: usize, start_row: u64) {
        if self.out.folds.len() >= MAX_FOLD_RANGES {
            self.out.truncated = true;
            return;
        }
        self.out.folds.push(FoldRange {
            start_byte: self.base_byte.saturating_add(start as u64),
            end_byte: self.base_byte.saturating_add(end as u64),
            start_line: self.base_line.saturating_add(start_row),
            end_line: self.base_line.saturating_add(self.row),
        });
    }

    fn code(&mut self) {
        let spec = self.spec;
        let mut fresh_line = true;
        let mut previous = b'\n';
        while let Some(byte) = self.at(0) {
            if byte == b'\n' {
                self.row += 1;
                self.pos += 1;
                fresh_line = true;
                previous = byte;
                continue;
            }
            if byte.is_ascii_whitespace() {
                self.pos += 1;
                continue;
            }
            let at_line_start = std::mem::replace(&mut fresh_line, false);
            if spec.yaml && self.yaml(previous) {
                previous = b'a';
                continue;
            }
            if self.comment() || self.string() {
                previous = b'"';
                continue;
            }
            if spec.preprocessor && at_line_start && byte == b'#' {
                let name = self.src[self.pos + 1..]
                    .iter()
                    .position(|next| !matches!(next, b' ' | b'\t'))
                    .map_or(self.src.len(), |index| self.pos + 1 + index);
                self.take(self.ident_end(name), SyntaxToken::Keyword);
                continue;
            }
            if self.number() {
                previous = b'0';
                continue;
            }
            if spec.sigils.contains(&byte)
                && self
                    .at(1)
                    .is_some_and(|next| is_ident_start(next) || next == b'{')
            {
                let start = self.pos + 1 + usize::from(self.at(1) == Some(b'{'));
                self.take(self.ident_end(start), SyntaxToken::Property);
                previous = b'a';
                continue;
            }
            if is_ident_start(byte) {
                let end = self.ident_end(self.pos);
                let word = self.word(self.pos, end);
                if spec.comment_words.contains(&word.as_str()) {
                    self.take(self.line_end(self.pos), SyntaxToken::Comment);
                    continue;
                }
                let token = if spec.keywords.contains(&word.as_str()) {
                    Some(SyntaxToken::Keyword)
                } else if spec.types.contains(&word.as_str()) {
                    Some(SyntaxToken::Type)
                } else if spec.key_suffix.is_some() && self.next_significant(end) == spec.key_suffix
                {
                    Some(SyntaxToken::Property)
                } else if self.next_significant(end) == Some(b'(') {
                    Some(SyntaxToken::Function)
                } else if previous == b'.' {
                    Some(SyntaxToken::Property)
                } else if spec.capital_types
                    && byte.is_ascii_uppercase()
                    && self.src[self.pos..end].iter().any(u8::is_ascii_lowercase)
                {
                    Some(SyntaxToken::Type)
                } else {
                    None
                };
                match token {
                    Some(token) => self.take(end, token),
                    None => self.pos = end,
                }
                previous = b'a';
                continue;
            }
            match byte {
                b'(' | b'[' | b'{' => self.open_bracket(byte),
                b')' | b']' | b'}' => self.close_bracket(byte),
                _ => {}
            }
            previous = byte;
            self.pos += 1;
        }
    }

    /// Indentation (spaces and tabs) of the line holding `at`.
    fn indent_of_line(&self, at: usize) -> usize {
        let start = self.src[..at]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        self.src[start..]
            .iter()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count()
    }

    /// YAML forms the generic code scanner does not know. `previous` is the
    /// last significant byte, which tells where a value may start.
    fn yaml(&mut self, previous: u8) -> bool {
        let Some(byte) = self.at(0) else { return false };
        let line_start = self.pos == 0 || self.src[self.pos - 1] == b'\n';
        let ends_word =
            |next: Option<u8>| matches!(next, None | Some(b' ' | b'\t' | b'\n' | b'\r'));
        // Document start and end markers.
        if line_start
            && (self.starts_with("---") || self.starts_with("..."))
            && ends_word(self.at(3))
        {
            self.take(self.pos + 3, SyntaxToken::Keyword);
            return true;
        }
        let value_position = matches!(previous, b':' | b'-' | b'[' | b'{' | b',' | b'?' | b'\n');
        let word_end = |src: &[u8], from: usize| {
            src[from..]
                .iter()
                .position(|byte| byte.is_ascii_whitespace() || matches!(byte, b',' | b']' | b'}'))
                .map_or(src.len(), |index| from + index)
        };
        match byte {
            // &anchor and *alias.
            b'&' | b'*' if value_position && !ends_word(self.at(1)) => {
                self.take(word_end(self.src, self.pos + 1), SyntaxToken::Function);
                true
            }
            // !tag, !!str, !<verbatim>.
            b'!' if value_position => {
                self.take(word_end(self.src, self.pos + 1), SyntaxToken::Type);
                true
            }
            // Block scalars: the indicator, then the indented lines below it as text.
            b'|' | b'>' if matches!(previous, b':' | b'-') => self.yaml_block_scalar(),
            // A quoted key: "key": or 'key':
            b'"' | b'\'' => self.yaml_quoted_key(byte),
            _ => false,
        }
    }

    /// A block scalar indicator (`|`, `>-`, `|2`) at the end of its line,
    /// and the lines indented deeper than that line, colored as text.
    fn yaml_block_scalar(&mut self) -> bool {
        let mut end = self.pos + 1;
        while self
            .src
            .get(end)
            .is_some_and(|next| matches!(next, b'-' | b'+' | b'0'..=b'9'))
        {
            end += 1;
        }
        let line_end = self.line_end(end);
        let rest = &self.src[end..line_end];
        let rest_blank = rest.iter().all(u8::is_ascii_whitespace);
        let rest_comment = rest.iter().find(|byte| !matches!(byte, b' ' | b'\t')) == Some(&b'#');
        if !rest_blank && !rest_comment {
            return false;
        }
        let parent = self.indent_of_line(self.pos);
        self.take(end, SyntaxToken::Keyword);
        if rest_comment {
            let start = end + rest.iter().position(|byte| *byte == b'#').unwrap_or(0);
            self.span(start, line_end, SyntaxToken::Comment);
        }
        self.skip_to(line_end);
        // Blank lines belong to the block; it ends at the last deeper-indented line.
        let body_start = (line_end + 1).min(self.src.len());
        let mut body_end = body_start;
        let mut cursor = body_start;
        while cursor < self.src.len() {
            let next_end = self.line_end(cursor);
            let line = &self.src[cursor..next_end];
            let blank = line.iter().all(u8::is_ascii_whitespace);
            let indent = line
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            if !blank && indent <= parent {
                break;
            }
            if !blank {
                body_end = next_end;
            }
            cursor = next_end + 1;
        }
        if body_end > body_start {
            self.skip_to(body_start);
            self.take(body_end, SyntaxToken::String);
        }
        true
    }

    /// A quoted mapping key (`"key":`, `'key':`), colored as a key; any other
    /// quoted text is left to the string rules.
    fn yaml_quoted_key(&mut self, quote: u8) -> bool {
        let mut end = self.pos + 1;
        while let Some(next) = self.src.get(end) {
            match next {
                b'\n' => return false,
                b'\\' if quote == b'"' => end += 2,
                b'\'' if quote == b'\'' && self.src.get(end + 1) == Some(&b'\'') => end += 2,
                _ if *next == quote => {
                    end += 1;
                    break;
                }
                _ => end += 1,
            }
        }
        let end = end.min(self.src.len());
        let colon = self.src[end..]
            .iter()
            .position(|next| !matches!(next, b' ' | b'\t'));
        let is_key = colon.is_some_and(|offset| {
            self.src[end + offset] == b':'
                && matches!(
                    self.src.get(end + offset + 1),
                    None | Some(b' ' | b'\t' | b'\n' | b'\r')
                )
        });
        if is_key {
            self.take(end, SyntaxToken::Property);
        }
        is_key
    }

    /// The separator a delimited file uses: the spec's, or for CSV the most
    /// frequent of comma, semicolon, tab, and bar on the first line, outside quotes.
    fn delimiter(&self) -> u8 {
        let fixed = self.spec.delimiter.unwrap_or(b',');
        if fixed != b',' {
            return fixed;
        }
        let mut counts = [0_usize; 4];
        let candidates = *b",;\t|";
        let mut quoted = false;
        for &byte in self.src {
            if byte == b'\n' && !quoted {
                break;
            }
            if byte == b'"' {
                quoted = !quoted;
            } else if !quoted {
                if let Some(index) = candidates.iter().position(|candidate| *candidate == byte) {
                    counts[index] += 1;
                }
            }
        }
        // Ties go to the comma.
        let best = (0..4)
            .max_by_key(|index| (counts[*index], usize::from(*index == 0)))
            .unwrap_or(0);
        if counts[best] == 0 {
            b','
        } else {
            candidates[best]
        }
    }

    /// CSV and TSV: every column takes the next color of a fixed cycle, so a
    /// column keeps its color down the file. A quoted field may hold the
    /// separator, doubled quotes, and line breaks; text after its closing
    /// quote is marked invalid up to the next separator.
    fn delimited(&mut self) {
        const COLUMN_COLORS: [SyntaxToken; 6] = [
            SyntaxToken::Keyword,
            SyntaxToken::String,
            SyntaxToken::Number,
            SyntaxToken::Function,
            SyntaxToken::Type,
            SyntaxToken::Property,
        ];
        let separator = self.delimiter();
        let field_end = |src: &[u8], from: usize| {
            src[from..]
                .iter()
                .position(|byte| *byte == separator || matches!(byte, b'\n' | b'\r'))
                .map_or(src.len(), |index| from + index)
        };
        let mut column = 0_usize;
        while let Some(byte) = self.at(0) {
            let color = COLUMN_COLORS[column % COLUMN_COLORS.len()];
            match byte {
                b'\n' => {
                    self.row += 1;
                    self.pos += 1;
                    column = 0;
                }
                b'\r' => self.pos += 1,
                _ if byte == separator => {
                    self.pos += 1;
                    column += 1;
                }
                b'"' => {
                    let mut end = self.pos + 1;
                    loop {
                        match self.src.get(end) {
                            None => break,
                            Some(b'"') if self.src.get(end + 1) == Some(&b'"') => end += 2,
                            Some(b'"') => {
                                end += 1;
                                break;
                            }
                            Some(_) => end += 1,
                        }
                    }
                    self.take(end, color);
                    let junk = field_end(self.src, self.pos);
                    if junk > self.pos {
                        self.take(junk, SyntaxToken::Invalid);
                    }
                }
                _ => {
                    let end = field_end(self.src, self.pos);
                    self.take(end, color);
                }
            }
        }
    }

    /// Attributes and quoted values up to the end of a tag.
    fn tag_body(&mut self, terminator: &str) -> bool {
        while let Some(byte) = self.at(0) {
            if self.starts_with(terminator) {
                self.skip_to(self.pos + terminator.len());
                return false;
            }
            if self.starts_with("/>") {
                self.pos += 2;
                return true;
            }
            match byte {
                b'>' => {
                    self.pos += 1;
                    return false;
                }
                b'"' | b'\'' => {
                    let end = self.src[self.pos + 1..]
                        .iter()
                        .position(|next| *next == byte)
                        .map_or(self.src.len(), |index| self.pos + index + 2);
                    self.take(end, SyntaxToken::String);
                }
                _ if is_ident_start(byte) => {
                    let end = self.name_end(self.pos);
                    self.take(end, SyntaxToken::Property);
                }
                _ => self.skip_to(self.pos + 1),
            }
        }
        false
    }

    fn name_end(&self, from: usize) -> usize {
        self.src[from..]
            .iter()
            .position(|byte| {
                !(is_ident_start(*byte)
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b':' | b'.'))
            })
            .map_or(self.src.len(), |index| from + index)
    }

    fn markup(&mut self) {
        let mut open: Vec<(String, usize, u64)> = Vec::new();
        while let Some(byte) = self.at(0) {
            if self.starts_with("<!--") {
                let end = self
                    .find(self.pos + 4, "-->")
                    .map_or(self.src.len(), |i| i + 3);
                self.take(end, SyntaxToken::Comment);
            } else if self.starts_with("<![CDATA[") {
                let end = self
                    .find(self.pos + 9, "]]>")
                    .map_or(self.src.len(), |i| i + 3);
                self.take(end, SyntaxToken::String);
            } else if self.starts_with("<!") {
                let end = self.find(self.pos, ">").map_or(self.src.len(), |i| i + 1);
                self.take(end, SyntaxToken::Keyword);
            } else if self.starts_with("<?") {
                self.take(self.name_end(self.pos + 2), SyntaxToken::Keyword);
                self.tag_body("?>");
            } else if byte == b'<'
                && self
                    .at(1)
                    .is_some_and(|next| is_ident_start(next) || next == b'/')
            {
                let start = self.pos;
                let row = self.row;
                let closing = self.at(1) == Some(b'/');
                let name_start = self.pos + 1 + usize::from(closing);
                let name_end = self.name_end(name_start);
                let name = self.word(name_start, name_end);
                self.pos = name_start;
                self.take(name_end, SyntaxToken::Keyword);
                let self_closed = self.tag_body(">");
                if closing {
                    if let Some(index) = open.iter().rposition(|(tag, ..)| *tag == name) {
                        let (_, opened_at, opened_row) = open[index].clone();
                        open.truncate(index);
                        if self.row > opened_row {
                            self.fold(opened_at, self.pos, opened_row);
                        }
                    }
                } else if !self_closed {
                    if self.spec.raw_text_elements && matches!(name.as_str(), "script" | "style") {
                        // Embedded code is skipped so `a < b` is not read as a tag.
                        let close = format!("</{name}");
                        let end = self
                            .src
                            .get(self.pos..)
                            .and_then(|rest| {
                                rest.windows(close.len()).position(|window| {
                                    window.eq_ignore_ascii_case(close.as_bytes())
                                })
                            })
                            .map_or(self.src.len(), |index| self.pos + index);
                        self.skip_to(end);
                    }
                    if open.len() < MAX_NESTING {
                        open.push((name, start, row));
                    }
                }
            } else if byte == b'&' {
                let limit = (self.pos + MAX_ENTITY_BYTES).min(self.src.len());
                let end = self.src[self.pos + 1..limit]
                    .iter()
                    .position(|next| !(next.is_ascii_alphanumeric() || *next == b'#'))
                    .map(|index| self.pos + 1 + index);
                match end {
                    Some(end) if self.src[end] == b';' && end > self.pos + 1 => {
                        self.take(end + 1, SyntaxToken::Escape);
                    }
                    _ => self.pos += 1,
                }
            } else {
                self.skip_to(self.pos + 1);
            }
        }
    }

    fn css(&mut self) {
        let mut depth = 0_usize;
        while let Some(byte) = self.at(0) {
            if byte == b'\n' {
                self.row += 1;
                self.pos += 1;
            } else if byte.is_ascii_whitespace() {
                self.pos += 1;
            } else if self.starts_with("/*")
                || (self.starts_with("//") && self.pos > 0 && self.src[self.pos - 1] != b':')
                || (self.starts_with("//") && self.pos == 0)
            {
                if !self.comment() {
                    self.pos += 1;
                }
            } else if self.string() || (byte != b'.' && self.number()) {
                // Emitted by the helper.
            } else if byte == b'@' || byte == b'$' {
                let token = if byte == b'@' {
                    SyntaxToken::Keyword
                } else {
                    SyntaxToken::Property
                };
                self.take(self.css_ident_end(self.pos + 1), token);
            } else if byte == b'#' {
                let end = self.css_ident_end(self.pos + 1);
                let token = if depth == 0 {
                    SyntaxToken::Type
                } else {
                    SyntaxToken::Number
                };
                self.take(end, token);
            } else if byte == b'.'
                && self
                    .at(1)
                    .is_some_and(|next| is_ident_start(next) || next == b'-')
            {
                self.take(self.css_ident_end(self.pos + 1), SyntaxToken::Type);
            } else if byte == b'!' {
                self.take(self.css_ident_end(self.pos + 1), SyntaxToken::Keyword);
            } else if is_ident_start(byte) || byte == b'-' {
                let end = self.css_ident_end(self.pos);
                let next = self.next_significant(end);
                if next == Some(b'(') {
                    self.take(end, SyntaxToken::Function);
                } else if depth > 0 && next == Some(b':') {
                    self.take(end, SyntaxToken::Property);
                } else if depth == 0 {
                    self.take(end, SyntaxToken::Keyword);
                } else {
                    self.pos = end.max(self.pos + 1);
                }
            } else {
                match byte {
                    b'{' => {
                        depth += 1;
                        self.open_bracket(byte);
                    }
                    b'}' => {
                        depth = depth.saturating_sub(1);
                        self.close_bracket(byte);
                    }
                    b'(' | b'[' => self.open_bracket(byte),
                    b')' | b']' => self.close_bracket(byte),
                    _ => {}
                }
                self.pos += 1;
            }
        }
    }

    fn css_ident_end(&self, from: usize) -> usize {
        self.src
            .get(from..)
            .unwrap_or_default()
            .iter()
            .position(|byte| !(is_ident_start(*byte) || byte.is_ascii_digit() || *byte == b'-'))
            .map_or(self.src.len(), |index| from + index)
    }
}

/// Guesses a language from the first bytes of a file with no telling name.
pub(crate) fn detect_content(head: &[u8]) -> Option<&'static str> {
    let head = head.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(head);
    let text = String::from_utf8_lossy(&head[..head.len().min(256)]);
    let first = text.lines().next().unwrap_or_default().trim();
    if let Some(interpreter) = first.strip_prefix("#!") {
        // `#!/usr/bin/env python3 -u` and `#!/bin/bash` both name the program.
        let program = interpreter
            .split_whitespace()
            .map(|part| part.rsplit('/').next().unwrap_or(part))
            .find(|part| *part != "env" && !part.starts_with('-'))?;
        let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        return match program {
            "sh" | "bash" | "zsh" | "ksh" | "dash" | "ash" => Some("bash"),
            "python" | "pypy" => Some("python"),
            "perl" => Some("perl"),
            "ruby" => Some("ruby"),
            "node" | "nodejs" | "deno" | "bun" => Some("javascript"),
            "php" => Some("php"),
            "pwsh" | "powershell" => Some("powershell"),
            "lua" => Some("lua"),
            "osascript" => Some("applescript"),
            "make" => Some("makefile"),
            _ => None,
        };
    }
    let start = text.trim_start().to_ascii_lowercase();
    if start.starts_with("<?php") {
        Some("php")
    } else if start.starts_with("<?xml") {
        Some(if start.contains("<xsl:") {
            "xsl"
        } else {
            "xml"
        })
    } else if start.starts_with("<!doctype html") || start.starts_with("<html") {
        Some("html")
    } else {
        None
    }
}
