# Improvement plan 2: Find window, regular expressions and column search, theme contrast, YAML and plist tools, more file types (M31–M35)

**Status:** approved 2026-10-20; M31–M35 completed. Open question 2 answered: binary property lists open read-only in the hex editor. Open question 1 answered: YAML gets Validate and Beautify (Beautify asks before losing comments or anchors). Progress will live in `IMPLEMENTATION_TRACKER.md`; handovers in `docs/handover/`.
**Date:** 2026-10-20, against commit `b56de0c` (Nulis 0.2.1).
**Milestone numbering:** continues `IMPROVEMENT_PLAN.md` (M19–M30 are complete), so this plan starts at M31.

## 1. User requests

1. With NvChad themes the text selection is barely visible (screen recording `Theme Highlight.mp4`: NvChad · Github Dark, 203 bytes selected, almost invisible).
2. With dark themes the icons in the top toolbar and the side bar (activity bar) look too dim.
3. The Find and Replace window has a large empty area below its buttons on the Find tab.
4. Highlight more file types: plist, YAML/YML, CSV.
5. YAML/YML gets a validation tool like the JSON and XML tools.
6. plist gets a validation tool as well.
7. Regular-expression search feels gone. Wanted in the Find options: "Use regular expressions" with a style picker (Perl or Unix; later changed to one general style, see §3.0), and "Search in column" with From/To fields (user screenshots of a reference editor, 2026-10-20; used for feature discovery only, per `CLEAN_ROOM.md`).

## 2. Findings

### 2.1 Selection contrast (request 1)

The NvChad importer sets `editor.selection` to the theme's `base02` (`crates/nulis-theme/src/lib.rs:372`). In many Base46 themes `base02` is almost the background color and almost the current-line color (`base01`).

- Github Dark: selection `#383d42`, background `#24292E`, current line `#33383d`.
- Measured over all 97 bundled NvChad themes (WCAG contrast of selection against editor background): **86 are below 1.5:1, 53 below 1.3:1**. Worst: Onenord Light 1.07, Mountain 1.09, Rxyhn 1.09, Rosepine Dawn 1.10, Ayu Light 1.11.
- For comparison: Ember Dark 2.39:1, Paper Light 1.39:1 (also weak).

### 2.2 Icon contrast (request 2)

Top-toolbar buttons (`.icon-button`, `apps/desktop/src/styles.css:823`) and activity-bar buttons (`.activitybar button`, `styles.css:786`) use `--muted`, which comes from `ui.mutedText`. The importer fills `ui.mutedText` from Base46 `grey_fg`, a color meant for faint text such as comments. Disabled buttons then get `opacity: .4` on top of it (`styles.css:829`, `:140`).

- Contrast of `ui.mutedText` against `ui.toolbar`: Github Dark 2.28:1, Obsidian Ember 1.98:1, Vscode Light 1.47:1; Ember Night 6.69:1, Paper Current 4.17:1 (renamed "Nulis · Ember Night" and "Nulis · Paper Current" in M33 so they sort first).
- **76 of 77 dark NvChad themes and 20 light ones are below 3:1**, the WCAG minimum for controls.

### 2.3 Find and Replace window (request 3)

The window always opens at a fixed 480 × 620 (`apps/desktop/src-tauri/src/lib.rs:1133`, `open_find_window`). That height fits the tallest tab (Replace in Files); the Find tab needs much less, leaving the empty area.

### 2.4 File types and tools (requests 4–5)

- **YAML/YML is already detected and highlighted** (`crates/nulis-syntax/src/lexer.rs`, spec `yaml`): keys, `true`/`false`/`null`, strings, numbers, `#` comments. Missing: `---`/`...` document markers, anchors `&x` and aliases `*x`, tags `!!str`, block scalars `|` and `>`, quoted keys.
- **plist is already highlighted as XML** (extension listed in the `xml` spec), so the XML Validate and Beautify tools already apply to it (`ToolKind::for_language("xml")`). But XML Validate only checks that the tags are well-formed; it accepts a plist with a `<dict>` missing a `<key>`, an `<integer>` holding text, or a bad `<date>`, which plist readers such as `plutil -lint` reject. Gap 2: **binary plists** (`bplist00` header) are not text and cannot be shown.
- **CSV/TSV is not supported.**
- The JSON and XML tools live in `crates/nulis-core/src/format_tools.rs` (`ToolKind`, `validate_*`, `format_*`). The desktop repeats the language-to-tool mapping by hand (`apps/desktop/src/App.tsx:592`).

### 2.5 Regular expressions and column search (request 7)

The "Use regular expressions" checkbox is in the Find window on all four tabs (`FindReplacePanel.tsx:509`), but the engine behind it is very limited:

- **Common patterns are refused.** `nulis-search` reads files in 64 KiB chunks and requires every pattern to have a fixed maximum match length (`crates/nulis-search/src/lib.rs`, `Matcher::new`). So `\w+`, `a.*b`, `[0-9]+` and `\s*` all fail with "unbounded regular expressions are not safe for streaming search".
- **No lookaround or backreferences.** The Rust `regex` crate has no lookahead, lookbehind or backreferences (kept on purpose for guaranteed speed; see §3.0).
- **No capture groups in Replace.** `plan_replacements` inserts the replacement text as-is, so `$1` or `\1` are not expanded.
- **Highlights can disagree with search.** The editor paints "highlight all" with the browser's `RegExp` (`EditorViewport.tsx:366`), a different engine from the core.
- **No column search** exists.

## 3. Design

### 3.0 Regular expressions and column search (M32)

**Options UI**

One regex style, so there is no style picker: the "Use regular expressions" row is a plain checkbox. Options are laid out in two columns, in this order:

```
Find tab                                   Replace tab
☐ Match case        ☐ Highlight all items found      ☐ Match case        ☐ Match whole word
☐ Match whole word  ☐ List lines containing find…    ☐ Search in column:  From [   ]  To [   ]
☐ Search in column:  From [   ]  To [   ]             ☐ Use regular expressions  ☐ Close after replace all
☐ Use regular expressions  ☐ Close after find

Find in Files / Replace in Files tabs (no column search: many files)
☐ Match case        ☐ Match whole word
☐ Use regular expressions  ☐ Close after … (Replace in Files only)
☐ Include subdirectories (unchanged, with the folder fields)
```

- **Search in column:** the From and To fields are disabled until the box is ticked; they take whole numbers ≥ 1, the same numbers as the status bar's "Col". From alone searches to the end of each line; To alone from the line start; To smaller than From is shown as an error on the field. Find and Replace tabs only.
- **Use regular expressions:** a small "Regex help" link next to the checkbox opens the regex section of Nulis Help.
- **Pattern errors** (invalid pattern, or lookaround/backreferences, which the safe engine does not support) show inline under the Find field, naming the problem, e.g. "Lookbehind (?<=…) is not supported". Count All, Previous, Next and Replace are disabled while the pattern has an error.
- Options, including the column range, are remembered between sessions (new: today only the search history is remembered).
- Layout and wording are Nulis's own; the window grows to fit (M31).

**Regex engine: the `regex` crate** (already used; pure Rust, MIT/Apache-2.0; no new dependency)

Engines considered: `regex` alone, `fancy-regex` (adds lookaround and backreferences through backtracking), PCRE2 and Oniguruma (C libraries). Decided 2026-10-20: **the safe engine only**. `regex` guarantees linear-time matching, so no pattern can hang the editor, even on multi-gigabyte files. The cost is that lookahead, lookbehind and backreferences inside the pattern are not available; using them gives a clear error message that names the unsupported feature.

- **Syntax:** `\d \w \s \b`, Unicode classes `\p{L}`, character classes including POSIX `[[:alpha:]]`, `|`, `* + ? {m,n}`, non-greedy `*?`, inline flags `(?i)`, `(?m)`, `(?s)`, non-capturing groups `(?:…)`, named groups `(?<name>…)`, anchors `^ $ \A \z`.
- **Replacement:** `$1`, `${name}`, `$0` or `$&` for the whole match, `$$` for a dollar; `\1`…`\9` also accepted; `\n`, `\t`, `\\` escapes. `&` is a literal character. Capture groups come from the same `regex` match, so this needs no other engine.
- The case-sensitive and whole-word options apply as today.
- **Unbounded patterns are allowed**. A single match is capped at 1 MiB (the existing `MAX_REGEX_MATCH_BYTES`). The chunked reader keeps enough overlap and context that matches across a chunk boundary are found once, and a match that would exceed the cap is reported instead of silently cut.
- **Safety:** every pattern runs in linear time; searches stay cancellable as today.
- **Encoding:** regex search works on UTF-8 text; documents in other encodings are searched after decoding, as the editor shows them.
- **Capture groups in Replace:** `plan_replacements` expands the replacement per match; Replace All stays one undo step and keeps the existing size limits.
- **One source of truth for highlights:** "Highlight all items found" uses match ranges from the core for the visible lines, instead of the browser's `RegExp`.
- **Column search:** each line is searched only inside the column range, and a match must lie fully inside it. `^` and `$` match at the range edges. Works with regular expressions and with plain text.
- Saved searches and macros using `mode: "regex"` keep working unchanged: the new syntax is a superset of the current one.
- Nulis Help and `USER_GUIDE.md` get a regex section: syntax with examples and the replacement syntax.

### 3.1 Theme contrast (M33)

One contrast module in `nulis-theme`, used in two places:

1. **Importer (`import_base46`)**
   - `editor.selection`: blend the theme's accent (`blue`, else `base0D`) into `editor.background`, raising the blend until the selection reaches **≥ 1.6:1 against the background and ≥ 1.25:1 against the current line**. Each theme keeps its own hue.
   - `ui.mutedText`: start from `light_grey` (else `base04`) and move it toward `ui.text` until it reaches **≥ 4.5:1** against `ui.toolbar`, `ui.panel` and `ui.background`.
   - New token **`ui.icon`**: ≥ 3:1 against the toolbar and panel, set between `ui.mutedText` and `ui.text`.
   - New token **`ui.iconDisabled`**: a fixed blend that reads clearly as unavailable but is still visible (replaces stacked `opacity: .4`).
   - Regenerate `themes/nvchad-catalog.json` with `cargo run -p nulis-theme --example import_base46 -- themes/nvchad-source themes/nvchad-catalog.json`.
2. **When any theme loads** (bundled or installed through "Install Theme…"): the same checks run. A selection, muted text or icon color that is too faint is strengthened, and a missing `ui.icon`/`ui.iconDisabled` is derived. The stored theme file is not changed.
3. **Desktop CSS:** `.icon-button`, `.activitybar button` and toolbar captions use `--icon`; disabled states use `--icon-disabled` instead of opacity. Forced-colors (high contrast) rules stay as they are.
4. **Built-in themes:** Ember Dark and Paper Light get explicit `ui.icon`/`ui.iconDisabled` that match their current look; Paper Light's selection is raised to meet the 1.6:1 floor.
5. **`docs/THEME_SPEC.md`** documents the two new tokens and the contrast floors.

### 3.2 Find and Replace window fit (M31)

The Find page measures its content and asks Tauri to set the window's inner height to fit (width unchanged), when it opens, when the tab changes, and when Options is opened or closed. If the user resizes the window, that height is kept until the next tab change. Height is limited to the screen's work area; the minimum size stays 420 × 360. The initial height in `open_find_window` is lowered so there is no visible jump.

### 3.3 YAML and plist Validate (M34)

**YAML**

- `ToolKind::Yaml` in `format_tools.rs`, `validate_yaml` using **`yaml-rust2`** (pure Rust, MIT/Apache-2.0, compatible with GPL-3.0-or-later; added to `THIRD_PARTY_LICENSES.md`).
- Reports line and column (the editor jumps there, as for JSON/XML). Supports multi-document streams. Flags **duplicate keys**, which the parser accepts but which are almost always mistakes.
- **Validate only, no Beautify:** reformatting through a parser drops comments and can change how scalars are written. (Open question 1.)
- The desktop gets the tool kind from the core instead of repeating the mapping in `App.tsx`. Tools menu, toolbar and command palette show "Validate YAML" for YAML documents.

**plist**

- plist becomes its own language, `plist` (extension `.plist`, plus XML files whose doctype is `-//Apple//DTD PLIST`), still highlighted by the XML markup lexer, so it can have its own tool kind.
- `ToolKind::Plist` with **Validate** and **Beautify**. Beautify reuses the XML formatter, which changes only whitespace between elements. Minify is not offered.
- `validate_plist` runs the XML well-formedness check first, then checks the plist structure on the same tokens, so every error has an exact line and column:
  - the root element is `<plist>` (with `version="1.0"` if present) and holds exactly one value;
  - a `<dict>` alternates `<key>` and one value, with no key left without a value; **duplicate keys** in one `<dict>` are reported;
  - only plist elements are allowed: `dict`, `array`, `key`, `string`, `integer`, `real`, `date`, `data`, `true/`, `false/`;
  - `<integer>` is a decimal (or `0x` hex) integer that fits 64 bits; `<real>` is a number (`nan` and `inf` accepted); `<date>` is ISO 8601 UTC (`2026-10-20T09:14:02Z`); `<data>` is valid Base64 (whitespace allowed); `<true/>` and `<false/>` are empty.
- Written in Nulis's own code on the existing XML tokenizer, so no new dependency.
- Binary plists are not validated by this tool (see §3.4 and open question 2).
- Nulis Help and `USER_GUIDE.md`: the Tools section lists YAML and plist.

### 3.4 Highlighting (M35)

- **CSV/TSV:** new `csv` and `tsv` languages (`.csv`, `.tsv`, `.tab`) with a new lexer mode that colors each column with a rotating set of syntax colors. Quoted fields with delimiters, doubled quotes and line breaks inside are handled; a malformed quote colors the rest of the line as invalid, without failing. Very large files keep the existing bounded-viewport tokenizing.
- **YAML:** add document markers, anchors, aliases, tags, block-scalar indicators and quoted keys to the `yaml` spec.
- **plist:** XML-style highlighting under the new `plist` language (M34). Detect binary plists by their `bplist00` header and open them read-only in the hex editor, with a status message explaining why. (Open question 2.)

## 4. Milestones

| Milestone | Scope | Depends on | Cx |
| --- | --- | --- | --- |
| **M31** Find window fits its content | §3.2 | – | S |
| **M32** Regular expressions and column search | §3.0: one regex style on the safe `regex` engine, unbounded patterns, capture groups in Replace, core-driven highlights, Search in column | M31 | L |
| **M33** Theme contrast | §3.1: selection, muted text, icons, disabled icons; importer, load-time check, CSS, catalog, theme spec | – | M |
| **M34** YAML and plist Validate | §3.3 | – | M |
| **M35** CSV/TSV, YAML and plist highlighting | §3.4 | M34 (shares YAML tests) | M |

## 5. Testing and acceptance

- **M31:** Vitest for the measured height per tab and that a manual resize is kept until a tab change. Manual on macOS and the Windows VM: no empty area on any tab, and nothing is cut off.
- **M32:** search tests: unbounded patterns (`\w+`, `a.*b`) across chunk boundaries found once; anchors and `\b` at chunk edges; POSIX classes; lookaround and backreferences give a clear error; replacement expansion (`$1`, `${name}`, `$&`, `\1`), escapes and missing groups; 1 MiB cap reported; cancel works. Column search: ranges, From/To alone, tabs, CRLF, multi-byte characters, `^`/`$` at range edges. Vitest for the options UI and that highlights match core results. Manual: searching a 1 GB file stays responsive.
- **M33:** Rust tests that every bundled theme (97 NvChad + 2 built-in) meets the floors for selection, muted text and icons, and that a deliberately faint installed theme is corrected at load. Vitest for the new CSS variables. Manual check of Github Dark, Mountain, Obsidian Ember, Vscode Light, Onenord Light, Ember Dark and Paper Light: selection clearly visible, icons readable, disabled icons distinct.
- **M34:** core tests for YAML: valid single and multi-document, syntax error with correct line/column, duplicate key, tabs used as indentation, CRLF, BOM. Core tests for plist: a real `Info.plist` passes; each rule in §3.3 has a failing case with the right line/column; Beautify keeps every value byte for byte. Vitest that each tool appears only for its own language (JSON, XML, YAML, plist).
- **M35:** syntax tests for CSV quoting and line breaks, TSV, YAML anchors/tags/block scalars, and binary-plist detection.
- Every milestone runs: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `npm run lint`, `npm run typecheck`, `npm test`.

## 6. Risks

- **Changing theme colors** changes how existing NvChad themes look. Only selection, muted text and icons are touched, and only when below the floors; syntax colors are unchanged.
- **Installed third-party themes** are adjusted at load. Theme authors can still exceed the floors; they cannot go below them. This is noted in `THEME_SPEC.md`.
- **Window resizing** behaves differently per platform (macOS, Windows, Linux window managers). Tested on macOS and the Windows VM; Linux checked in CI build only.
- **No lookaround or backreferences** in patterns (deliberate, for guaranteed speed). Users who need them get a clear error; revisit only if requested often.
- **Behavior change for existing regex users:** patterns that were refused now work; bounded patterns that worked before give the same results.
- **New dependency** (`yaml-rust2`): license and maintenance check before adding it, as for other crates.

## 7. Open questions

1. YAML: Validate only (recommended), or also a Beautify that warns it removes comments?
2. Binary plist: open read-only in the hex editor (recommended), or leave out for now? Converting binary plists to editable XML is a separate, larger feature.

## 8. Out of scope

Converting binary plists to XML and back; CSV table view, sorting or column tools beyond what M28 already provides; YAML schema validation (for example Kubernetes or GitHub Actions schemas); a user setting for selection or icon contrast.
