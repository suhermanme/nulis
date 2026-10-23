# Language capabilities: design

**Status:** proposed in M41 (2026-10-23); nothing here is implemented yet. Plan and milestones: [IMPROVEMENT_PLAN_4.md](IMPROVEMENT_PLAN_4.md). Per-language state: [LANGUAGE_MATRIX.md](LANGUAGE_MATRIX.md). Type sketches are Rust-like and will change during implementation; the rules around them are the contract.

## 1. Vocabulary

- **Language:** a stable `LanguageId` (`json`, `python`, …) with a display name. It exists independently of any feature.
- **Association:** a rule that maps a file to a language (file name, extension, content).
- **Capability:** one kind of feature a language can have (§3).
- **Provider:** an implementation of one capability for one or more languages, from one **origin**: built-in, plugin, or language server.
- **Highlighting is not intelligence.** A language may have highlighting and nothing else; completion, diagnostics and formatting are separate capabilities with separate providers.

## 2. Where things live

```text
apps/desktop (React)        shared UI: completion popup, diagnostics, capability-driven menus, preview renderer registry
        │  IPC DTOs (capabilities, completion items, diagnostics)
nulis-core                  commands (editor.newline, completion.*, tools.*), settings, document lifecycle, provider wiring
        │
nulis-language (new)        contracts, CapabilityRegistry, resolution, isolation wrapper, indentation and completion engines
        │
nulis-syntax                LanguageCatalog (languages + associations), highlighting (lexers, Tree-sitter, Markdown)
nulis-plugin-host           plugin API v2 manifests (declarative languages), worker sandbox for whole-document tools
nulis-lsp (new, M47)        one LSP client, adapters into the same contracts
```

`nulis-language` and `nulis-syntax` stay free of Tauri, file I/O and processes, so they are testable alone.

## 3. Capabilities

| Kind | Cardinality per document | Built-in providers (M42–M46) | Contract summary |
| --- | --- | --- | --- |
| `highlight` | one | lexer tables, Tree-sitter JSON/Python, Markdown scanner | bounded window in, spans/folds/brackets/symbols out (existing `SyntaxAnalysis`) |
| `comment` | one | lexer data, Python, Markdown | line marker and/or block markers (existing `CommentSyntax`) |
| `indentation` | one | rule families (§6) | Enter, outdent-on-type, indent unit |
| `completion` | many, merged | words, keywords, snippets | ranked items with explicit replacement ranges |
| `diagnostics` | many, merged | JSON, XML/XSL, YAML, plist validators | list of ranged messages with severity |
| `format` | one | JSON, XML/XSL, YAML, plist | whole-document text in, text or "unchanged" out |
| `compact` | one | JSON | same contract as `format`, compact mode |
| `preview` | one | Markdown | a preview kind; the core serves bounded source, the UI renders |
| `commands` | many | — (plugins) | command contributions, as in plugin API v1 |
| `symbols` | one | JSON (`pair`) | existing bounded symbol list |

A language needs none of these to open; Plain Text has none.

## 4. Contracts

```rust
pub enum CapabilityKind { Highlight, Comment, Indentation, Completion, Diagnostics, Format, Compact, Preview, Commands, Symbols }

pub struct ProviderId(String);                     // "builtin.json.format", "plugin:acme.nim/indent", "lsp:rust-analyzer"
pub enum ProviderOrigin { Builtin, Plugin { plugin_id: String }, LanguageServer { server_id: String } }

pub struct ProviderInfo {
    pub id: ProviderId,
    pub origin: ProviderOrigin,
    pub kind: CapabilityKind,
    pub languages: Vec<LanguageId>,
    pub label: String,                              // shown in menus and Settings, e.g. "JSON"
}

/// Whole-document input for tools. The core reads and decodes it (8 MiB cap,
/// document encoding) before the call.
pub struct DocumentText<'a> { pub language: &'a LanguageId, pub text: &'a str }

// As implemented in M43 (crates/nulis-language):
pub trait Validator: Send + Sync {
    /// Ok(None) = valid; Ok(Some(problem)) = the first problem, at a byte offset into `text`.
    fn validate(&self, input: &DocumentText) -> Result<Option<TextProblem>, ProviderError>;
}
pub enum ProviderError { Content(TextProblem) /* the user can fix it */, Refused(String), Failed(String) /* counts toward suspension */ }

pub trait Formatter: Send + Sync {
    fn modes(&self) -> &[FormatMode];               // Beautify, Compact
    /// True when formatting can drop something; the editor then validates and asks first.
    fn may_lose_data(&self) -> bool { false }
    /// What this run would lose (YAML comments, anchors) or why it cannot run (tags).
    fn assess(&self, input: &DocumentText) -> Result<FormatAssessment, ProviderError>;
    fn format(&self, input: &DocumentText, mode: FormatMode, accept_loss: bool) -> Result<FormatOutcome, ProviderError>;
}
pub enum FormatAssessment { Ready, Lossy(String), Unavailable(String) }
pub enum FormatOutcome { Unchanged, Replaced(String) }   // the core applies it as one revision-checked undo step

// Planned (M47, shared with language servers): ranged diagnostics with severity,
// several per document, instead of the first problem only.

pub trait IndentProvider: Send + Sync {
    fn on_enter(&self, context: &IndentContext) -> IndentAction;
    fn electric_tokens(&self) -> &[&str];           // tokens that may outdent the line when typed: "}", "</", "end"
    fn on_type(&self, context: &IndentContext, typed: &str) -> Option<String>; // new leading whitespace
    fn forced_unit(&self) -> Option<IndentUnit>;    // YAML: spaces; Makefile recipe: tab
}
pub struct IndentContext<'a> {
    pub line_before_caret: &'a str,
    pub line_after_caret: &'a str,
    pub previous_lines: &'a [&'a str],              // nearest non-blank lines above, at most 64 lines / 16 KiB
    pub unit: IndentUnit,                           // Tab, or Spaces(n), from settings or detection
    pub tab_width: u8,
}
pub struct IndentAction { pub indent: String, pub closing_line: Option<String> } // closing_line: split "{|}"

pub trait CompletionProvider: Send + Sync {
    fn trigger_characters(&self) -> &[char];
    fn complete(&self, context: &CompletionContext, cancel: &CancelToken) -> Result<Vec<CompletionItem>, ProviderError>;
}
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,                       // Word, Keyword, Snippet, Function, Type, Property, …
    pub insert: Insert,                             // Text(String) or Snippet(String)
    pub replace: ByteRange,                         // document bytes replaced on accept
    pub detail: Option<String>,
    pub sort_text: Option<String>,
    pub source: ProviderId,
}

pub trait PreviewProvider: Send + Sync {
    fn kind(&self) -> &'static str;                 // "markdown"; the UI maps kinds to renderers
    fn max_source_bytes(&self) -> u64;              // 1 MiB for Markdown, as today
}
```

Rules every provider follows:

- **Pure with respect to the document.** Providers never mutate documents. Tools return text; the core applies it with the existing revision check, encoding, bookmark relocation and undo record. Completion returns ranges; the core applies the accepted item through `editor.insertText` semantics.
- **Bounded.** Inputs are capped by the core before the call (8 MiB tools, 256 KiB completion window, 64 lines for indentation). Outputs are capped (items, diagnostics, text size) and truncated with an explicit flag.
- **Cancellable.** Long work checks `Budget`/`CancelToken`. The core gives up at the deadline and treats it as a provider error.
- **Error, not panic.** A provider returns `ProviderError` for content problems ("Invalid JSON: … (line 3, column 7)"). A panic is a defect and is contained (§7).

## 5. Registry, resolution and lifecycle

### 5.1 Language catalog (in `nulis-syntax`)

```rust
pub struct Association { pub matcher: Matcher, pub language: LanguageId, pub source: AssociationSource }
pub enum Matcher { FileName(String), Extension(String) /* may contain dots: "d.ts" */, FirstLine(Regex) }
pub enum AssociationSource { User, Builtin, Plugin(String) }
```

**Resolution for a path** (first tier that produces a language wins):

1. User association (`files.associations` in `settings.json`; file name or `*.ext` pattern).
2. Exact file name, case-insensitive (`Dockerfile`, `Makefile`, `.bashrc`).
3. Longest matching extension, compound ones first (`d.ts` before `ts`).
4. Content: `FirstLine` matchers and the existing sniffers (`#!`, `<?xml`, `<?php`, doctype), first 256 bytes.
5. Plain Text.

**Same-tier conflicts:** built-in before plugin; between plugins, the user's recorded choice, otherwise the lowest plugin ID. Every conflict is listed in Settings ▸ Languages with a control to choose. Installing a plugin therefore never silently takes over a file type that already opens as something else; the user decides.

`unregister` removes a plugin's languages and associations. Open documents with a removed language keep the ID (shown as "Nim (unavailable)"), highlight as Plain Text, keep their content and dirty state, and recover when the language returns.

### 5.2 Capability registry (in `nulis-language`)

For each `(language, kind)` the registry keeps an ordered list of enabled providers.

- **Single-provider kinds** (highlight, comment, indentation, format, compact, preview, symbols): the user's choice for that language if set, else the built-in, else the first plugin provider by ID, else the language-server provider, else none.
- **Merged kinds** (completion, diagnostics, commands): all enabled providers are called; completion results are merged by rank (§6.2) and deduplicated by inserted text; diagnostics keep their source.
- **Enable/disable** works at three levels: a plugin (all its providers), a capability for a language (`"[json]": { "disabledCapabilities": ["format", "compact"] }` in `settings.json`, set from Settings ▸ Languages since M43), and a single provider. Disabling is immediate, reaches open documents, and never touches document content.

### 5.3 Lifecycle

```text
built-ins registered at core start ─┐
plugin approved / enabled ──────────┼─► registry updated ─► capabilities re-resolved for open documents
plugin revoked / disabled / removed ┘                        └─► DocumentSummary.capabilities pushed to the UI
LSP server: configured + approved ─► started on first matching document ─► stopped when the last one closes
```

Built-in providers live for the process. Plugin providers live while the plugin is installed, approved and enabled. Language-server providers live while their process is running; a crash removes them until restart (three tries with back-off, then reported).

## 6. Built-in engines

### 6.1 Indentation

The generic provider carries the previous line's indentation. Rule providers add patterns evaluated with the `regex` crate on the bounded context:

| Family | Increase after | Outdent when typed / next line | Notes |
| --- | --- | --- | --- |
| brace | line ends with `{`, `[`, `(` (outside strings/comments by lexer state) | `}`, `]`, `)` at line start | splits `{|}` onto three lines |
| python | line ends with `:` (not in a comment) | `else`, `elif`, `except`, `finally`, `case` | after `return`, `pass`, `break`, `continue`, `raise`: one level less |
| yaml | `key:` with no value; `- key:` | — | spaces only; never inserts a tab |
| makefile | rule line `target: deps` | — | recipe lines get a tab regardless of settings |
| markup | open tag not closed on the line (not void, not self-closing) | `</` | splits `<a>|</a>` |
| keyword-block | Ruby/Lua/Bash/VB/PL/SQL/AppleScript block openers | `end`, `fi`, `done`, `End If`, `else`… | per-language word lists, case-insensitive where the language is |
| carry | — | — | default for every other language |

Line endings follow the caret line, as Enter does today. Read-only documents refuse the command like any edit.

### 6.2 Completion

- **Words:** identifiers in a 256 KiB window around the caret (identifier rules from the language's lexer table), excluding the word being typed.
- **Keywords and types:** from the lexer tables; case follows the language (`case_insensitive` languages match either case and insert the table's spelling).
- **Snippets:** built-in data per language; body syntax `$1`, `${1:placeholder}`, `$0`, `\t` meaning one indent unit.
- **Ranking:** exact prefix > word-start match > subsequence; then provider rank (language server > snippets > keywords > words); then recency in this document; ties by label. Deduplicate by inserted text, keeping the best-ranked item.
- **Staleness:** every request carries `requestId` and `revision`. The UI shows only the latest request's response and only if the revision still matches; the core rejects an accept whose revision is stale.

## 7. Error isolation

- Every provider call goes through one wrapper: `catch_unwind` (with `AssertUnwindSafe`; providers hold no shared mutable state), input caps, output caps, and a deadline.
- A failure returns a typed error to the command, which shows the existing kind of status message and falls back (no highlighting, no completion items, no indentation rule → carry).
- Three consecutive failures disable that provider for the session and say so in Settings ▸ Languages.
- Plugin worker and LSP failures are process-isolated already; the wrapper adds the same accounting.

## 8. Security

| Input | Threat | Control |
| --- | --- | --- |
| Declarative language data from plugins | huge tables, pathological regex, path tricks | manifest size caps; per-list counts; `regex` crate (no backtracking) with a compiled-size limit; no paths or URLs in the schema |
| Snippet bodies | markup or command injection | plain text; inserted through normal edits; never rendered as HTML |
| Executable plugin providers (M48, optional) | data access beyond intent | existing worker sandbox; new permissions `document.readText` and `document.replaceText` shown at approval; bounded 8 MiB; 2-second deadline; never per keystroke |
| Language servers | arbitrary native code | only commands the user enters in user settings; approval bound to the exact program and arguments; never from workspace files or plugin manifests; argument vectors, no shell strings |
| Preview content | script execution in the privileged webview | Markdown only, `skipHtml`, existing image confinement; plugin preview renderers deferred until a sandboxed frame with no IPC exists |
| Settings | malformed or oversized values | schema-checked, size-capped, atomically written, as the customization file is |

## 9. Plugin API v2 (M48)

API v1 manifests stay valid. A v2 manifest declares `"apiVersion": 2`; `main` becomes optional so a language can be pure data.

```json
{
  "apiVersion": 2,
  "id": "acme.lang-nim",
  "name": "Nim",
  "version": "1.0.0",
  "engines": { "nulis": ">=0.5.0" },
  "contributes": {
    "languages": [{
      "id": "nim",
      "displayName": "Nim",
      "extensions": ["nim", "nims"],
      "filenames": [],
      "firstLine": "^#!.*\\bnim\\b",
      "comments": { "line": "#", "block": ["#[", "]#"] },
      "brackets": [["(", ")"], ["[", "]"], ["{", "}"]],
      "highlight": {
        "mode": "code",
        "keywords": ["proc", "func", "let", "var", "if", "elif", "else", "for", "while", "return"],
        "types": ["int", "float", "string", "bool"],
        "strings": [{ "open": "\"", "close": "\"", "escape": true }]
      },
      "indentation": {
        "increaseAfter": ":\\s*(#.*)?$",
        "outdentOnType": "^\\s*(elif|else|except|finally)\\b"
      },
      "completion": { "keywords": true, "words": true },
      "snippets": [{ "prefix": "proc", "body": "proc ${1:name}($2) =\n\t$0", "description": "Procedure" }]
    }]
  }
}
```

Validation: IDs in the plugin's namespace or new (a plugin may not redefine a built-in language ID; it may add associations to one, which then go through §5.1 conflict rules); caps on every list and string; regexes compiled at install time; capability declarations only, no code paths. A v2 manifest is rejected by Nulis versions that only know v1 (current `deny_unknown_fields` behavior), which fails safely.
