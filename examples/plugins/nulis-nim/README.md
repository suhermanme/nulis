# Nim language (declarative example)

This package is a Nulis API v2 plugin with no JavaScript at all: everything it
adds is data in `plugin.json`.

It contributes one language, `nim`, with:

- `.nim` and `.nims` file associations, plus `config.nims`;
- first-line detection (`#!/usr/bin/env nim`);
- line (`#`) and block (`#[ … ]#`) comments;
- highlighting keywords, types, and strings;
- indentation rules for `:` blocks and `elif`/`else`/`except`/`finally`/`of`;
- keyword and word completion;
- three snippets (`proc`, `if`, `for`).

## Install

1. Open **Plugins**.
2. Choose **Install from folder…** and select this directory.
3. Approve the plugin. It requests no permissions and runs no code.
4. Open a `.nim` file; the language and its features are now available.

## Turn features off

On the plugin card, clear a feature checkbox to switch that family off without
revoking the plugin. Disabling **languages** removes the language, its file
associations, and all of its providers. Turning **highlight** off keeps the
language but shows it without colors.

Settings ▸ Languages lists these features per language too, and lists a file
association conflict if another language or plugin claims the same pattern.
The user's choice is written to `files.associations` in `settings.json`.

## Learn more

`docs/PLUGIN_API.md` documents every manifest field, and
`docs/PLUGIN_GUIDE.md` walks through this declarative style and the executable
style used by `hello-nulis` and `nulis-tips`.
