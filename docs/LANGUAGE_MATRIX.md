# Language capability matrix

**As of:** 2026-10-23, commit `1494931` (Nulis 0.4.0). Part of [IMPROVEMENT_PLAN_4.md](IMPROVEMENT_PLAN_4.md); contracts in [LANGUAGE_CAPABILITIES.md](LANGUAGE_CAPABILITIES.md).

Table 1 is **verified**: read from the code and confirmed by calling `LanguageRegistry` directly (comments, brackets, folds, detection) on 2026-10-23. Table 2 is **planned** and nothing in it exists yet. Update both as milestones land; move a cell from Table 2 to Table 1 only when its tests pass.

## Table 1 — current state (verified)

Highlighter: **TS** = Tree-sitter grammar; **Lex** = table-driven lexer (mode in brackets); **MD** = Markdown line scanner. Folds: **brace** = multi-line `()[]{}`; **element** = multi-line markup elements. Brackets = pairs used by Go to Matching Bracket. Tools: **V** Validate, **B** Beautify, **M** Minify. In 0.4.0 no language had indentation rules, completion, snippets or LSP, and Enter never carried indentation. **Since M44 (verified by tests):** every language has automatic indentation as listed in the Indent column of Table 2, except where that column says "later".

| Language | ID | Extensions; file names; content | Highlighter | Folds | Brackets | Toggle Comment | Tools | Preview |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Plain Text | `plaintext` | fallback | none | — | — | — | — | — |
| AppleScript | `applescript` | applescript, scpt; `#!osascript` | Lex (code) | brace | yes | `--`, `(* *)` | — | — |
| Bash | `bash` | sh, bash, zsh, ksh, command; .bashrc .bash_profile .bash_aliases .profile .zshrc .zprofile .zshenv; `#!` sh/bash/zsh/ksh/dash/ash | Lex (code) | brace | yes | `#` | — | — |
| Batch | `batch` | bat, cmd | Lex (code) | brace | yes | `::` | — | — |
| C | `c` | c, h | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| C# | `csharp` | cs, csx | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| C++ | `cpp` | cpp cc cxx c++ hpp hh hxx h++ ino ipp tpp | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| CSS | `css` | css, scss, less | Lex (CSS) | brace | yes | `//` ⚠, `/* */` | — | — |
| CSV | `csv` | csv | Lex (delimited) | — | — | — | — | — |
| Dockerfile | `dockerfile` | dockerfile; dockerfile, containerfile | Lex (code) | brace | yes | `#` | — | — |
| Go | `go` | go | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| HTML | `html` | html htm xhtml vue svelte; doctype or `<html` | Lex (markup) | element | — | `<!-- -->` | — | — |
| INI | `ini` | ini cfg conf properties editorconfig; .editorconfig .gitconfig | Lex (code) | brace | yes | `;` | — | — |
| Java | `java` | java | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| JavaScript | `javascript` | js mjs cjs jsx; `#!` node/nodejs/deno/bun | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| JSON | `json` | json; .babelrc .eslintrc | TS | object, array | yes | — | V B M | — |
| Kotlin | `kotlin` | kt, kts | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| Lua | `lua` | lua; `#!lua` | Lex (code) | brace | yes | `--`, `--[[ ]]` | — | — |
| Makefile | `makefile` | mk, mak; makefile, gnumakefile; `#!make` | Lex (code) | brace | yes | `#` | — | — |
| Markdown | `markdown` | md, markdown, mdown | MD | — | — | `<!-- -->` | — | View mode, print |
| MySQL / MariaDB | `mysql` | mysql, mariadb | Lex (code) | brace | yes | `-- `, `/* */` | — | — |
| Objective-C/C++ | `objective-c` | m, mm | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| Perl | `perl` | pl pm t perl; `#!perl` | Lex (code) | brace | yes | `#`, `=pod … =cut` | — | — |
| PHP | `php` | php phtml php3 php4 php5 phps; `<?php`, `#!php` | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| Property List | `plist` | plist (binary plists open read-only in hex) | Lex (markup) | element | — | `<!-- -->` | V B | — |
| Oracle SQL (PL/SQL) | `plsql` | pls plsql pks pkb plb ora | Lex (code) | brace | yes | `--`, `/* */` | — | — |
| PowerShell | `powershell` | ps1 psm1 psd1; `#!` pwsh/powershell | Lex (code) | brace | yes | `#`, `<# #>` | — | — |
| Python | `python` | py pyw pyi; sconstruct sconscript; `#!python` | TS | def, class | — ⚠ | `#` | — | — |
| Ruby | `ruby` | rb rake gemspec ru; rakefile gemfile podfile fastfile vagrantfile; `#!ruby` | Lex (code) | brace | yes | `#`, `=begin =end` | — | — |
| Rust | `rust` | rs | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| SQL | `sql` | sql ddl dml | Lex (code) | brace | yes | `--`, `/* */` | — | — |
| Swift | `swift` | swift | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| TOML | `toml` | toml; cargo.lock pipfile | Lex (code) | brace | yes | `#` | — | — |
| TSV | `tsv` | tsv, tab | Lex (delimited) | — | — | — | — | — |
| TypeScript | `typescript` | ts tsx mts cts | Lex (code) | brace | yes | `//`, `/* */` | — | — |
| Visual Basic | `vb` | vb bas cls frm vba | Lex (code) | brace | yes | `'` | — | — |
| VBScript ASP | `vbscript` | vbs vbe asp wsf | Lex (code) | brace | yes | `'` | — | — |
| XML | `xml` | xml xsd svg xaml csproj vbproj fsproj props targets resx rss atom wsdl kml gpx storyboard xib; `<?xml` | Lex (markup) | element | — | `<!-- -->` | V B (well-formedness) | — |
| XSL | `xsl` | xsl, xslt; `<?xml` with `<xsl:` | Lex (markup) | element | — | `<!-- -->` | V B (as XML) | — |
| YAML | `yaml` | yaml, yml; .clang-format | Lex (code, YAML) | brace (flow only) | yes (flow only) | `#` | V B (B lossy, confirmed; refused with tags) | — |
| Zig | `zig` | zig, zon | Lex (code) | brace | yes | `//` | — | — |

Only JSON has document symbols (`pair`). Since M42, detection resolves in explicit tiers (user associations, file names, longest extension, content, Plain Text; see LANGUAGE_CAPABILITIES.md §5.1); the built-in catalog has no conflicts, and a test checks that.

⚠ Known issues (fix planned in M46): `.css` Toggle Comment uses `//`, which plain CSS rejects; Python has no bracket pairs; `.scpt` is binary but opens as text; Markdown has no folds; YAML has no indentation folds. Unrecognized today: `Dockerfile.dev` and similar, `.jsonc`.

## Table 2 — providers (Indent: implemented in M44; Completion: implemented in M45; the other columns are planned)

Indent family as in LANGUAGE_CAPABILITIES.md §6.1; "carry" keeps the current line's indentation. Completion: **W** document words, **K** keywords/types from the lexer table, **S** built-in snippets. Since M45 every language has W; K where the language has a word list (every lexer language with keywords, plus HTML element names, Python, and JSON; not CSS, INI, CSV, TSV, XML, XSL, plist, Markdown, or Plain Text, which get W only); S for Rust, JavaScript/TypeScript, Python, Go, the C family (C, C++, C#, Java, Objective-C, Kotlin, PHP), HTML, the SQL dialects, Bash, Ruby, and Lua. The table below still lists the original plan; Swift, Zig and others marked S have no snippets yet. LSP candidates are from prior knowledge and are **unverified**: license, platform builds and maintenance are checked in M47 before any is chosen. Nulis will not bundle servers.

| Language | Indent (M44/M46) | Completion (M45/M46) | Validate | Format / compact | Preview | LSP candidates (unverified) |
| --- | --- | --- | --- | --- | --- | --- |
| Plain Text | carry | W (manual only) | — | — | — | — |
| AppleScript | keyword-block (`tell`/`on`/`if` … `end`) | W K | — | — | — | none known |
| Bash | keyword-block + brace (`then`/`do`/`{` … `fi`/`done`/`}`) | W K S | — | — | — | bash-language-server (Node) |
| Batch | brace (parentheses) | W K | — | — | — | none known |
| C | brace | W K S | — | — | — | clangd |
| C# | brace | W K S | — | — | — | csharp-ls, OmniSharp |
| C++ | brace | W K S | — | — | — | clangd |
| CSS | brace | W S (properties list later) | — | — | — | vscode-css-languageserver (Node) |
| CSV | carry | W (manual only) | — | — | — | — |
| Dockerfile | carry (`\` continuation: later) | W K | — | — | — | dockerfile-language-server (Node) |
| Go | brace | W K S | — | — | — | gopls |
| HTML | markup | W K (tag and attribute names) S | — | — | — | vscode-html-languageserver (Node) |
| INI | carry | W | — | — | — | — |
| Java | brace | W K S | — | — | — | Eclipse JDT LS (JVM) |
| JavaScript | brace | W K S | — | — | — | typescript-language-server (Node) |
| JSON | brace | W K (`true` `false` `null`) | existing V | existing B, M | — | vscode-json-languageserver (Node) |
| Kotlin | brace | W K S | — | — | — | kotlin-language-server (JVM) |
| Lua | keyword-block (`function`/`then`/`do` … `end`) | W K S | — | — | — | lua-language-server |
| Makefile | makefile (tab recipes) | W K | — | — | — | none known |
| Markdown | carry (list continuation: later) | W (manual only) | — | — | existing View mode | marksman |
| MySQL / MariaDB | carry | W K | — | — | — | sqls |
| Objective-C/C++ | brace | W K S | — | — | — | clangd |
| Perl | brace | W K | — | — | — | Perl Navigator |
| PHP | brace | W K S | — | — | — | phpactor |
| Property List | markup | W K (plist element names) | existing V | existing B | — | LemMinX (generic XML) |
| Oracle SQL (PL/SQL) | keyword-block (`BEGIN`/`LOOP`/`IF` … `END`) | W K | — | — | — | none known |
| PowerShell | brace | W K | — | — | — | PowerShell Editor Services |
| Python | python | W K (list added) S | — | — | — | pyright (Node), python-lsp-server |
| Ruby | keyword-block (`def`/`do`/`class` … `end`) | W K S | — | — | — | ruby-lsp |
| Rust | brace | W K S | — | — | — | rust-analyzer (first choice for M47) |
| SQL | carry | W K | — | — | — | sqls |
| Swift | brace | W K S | — | — | — | sourcekit-lsp |
| TOML | carry | W K | — | — | — | taplo |
| TSV | carry | W (manual only) | — | — | — | — |
| TypeScript | brace | W K S | — | — | — | typescript-language-server (Node) |
| Visual Basic | keyword-block (`If … Then`/`Sub`/`For` … `End`/`Next`) | W K | — | — | — | none known |
| VBScript ASP | keyword-block (as VB) | W K | — | — | — | none known |
| XML | markup | W K (element names from the document) | existing V | existing B | — | LemMinX |
| XSL | markup | W K (`xsl:` elements) | existing V | existing B | — | LemMinX |
| YAML | yaml (spaces only) | W K | existing V | existing B (lossy rules kept) | — | yaml-language-server (Node) |
| Zig | brace | W K S | — | — | — | zls |

"—" in Validate/Format means no provider is planned; a language server or a plugin may add one. No language is promised equivalent intelligence.

## Proposed association changes (owner decision, plan §7 question 4)

| Change | Today | Proposed | Note |
| --- | --- | --- | --- |
| `.jsonc` | Plain Text | JSON | Validate/Minify must then accept comments or be disabled for `.jsonc` |
| `Dockerfile.*`, `Containerfile.*`, `*.dockerfile` | Plain Text (except the last) | Dockerfile | file-name prefix matcher |
| `.scpt` | AppleScript text | binary, opened read-only in hex | as binary plists are today |
| `.h` | C | unchanged; users can map `*.h` to C++ or Objective-C with `files.associations` | no change by default |
