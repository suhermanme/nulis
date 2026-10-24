# Nulis plugin API

## Versions

Nulis speaks plugin API versions 1 and 2. Version 1 plugins run as JavaScript in a fresh worker process and receive only a small, serializable host API. Version 2 adds declarative language contributions that run no code at all. Neither version receives Rust objects, webview handles, Node APIs, Web APIs, module loading, or ambient filesystem, network, process, environment, credential, terminal, remote, or clipboard access.

A manifest may omit `apiVersion` (meaning `1`) or declare `1` or `2`. A host that only knows version 1 rejects a version 2 manifest because of its unknown fields, which fails safely.

## Package and manifest

An installable package is a directory containing `plugin.json` and, for an executable plugin, one UTF-8 `.js` entry script. Nulis validates the source directory, copies only those files below application data, and refuses symlinks, traversal, unsupported files, malformed semantic versions, incompatible engine ranges, duplicate identities, or existing installations.

```json
{
  "id": "nulis.example-plugin",
  "name": "Example Plugin",
  "version": "1.0.0",
  "engines": { "nulis": ">=0.1.0" },
  "permissions": ["showMessage"],
  "activationEvents": ["onCommand:nulis.example-plugin.hello"],
  "main": "index.js",
  "contributes": {
    "commands": [{
      "command": "nulis.example-plugin.hello",
      "title": "Say Hello",
      "category": "Examples"
    }]
  }
}
```

Plugin IDs and command IDs are capped ASCII namespaces. Every command must begin with the exact plugin ID followed by a dot and have one matching `onCommand:` event; every command event must name a declared command. The `plugin.*` and `automation.*` host namespaces are reserved, and `onStartup` is the only non-command activation event. The installed manifest, entry source, and their SHA-256 digest are authoritative; undeclared package assets are not installed.

## API v2: declarative languages

A version 2 manifest may leave out `main` and contribute languages as data. With no `main`, commands and activation events are rejected: a declarative package cannot run code. A manifest with `"apiVersion": 2` may still use `main` and commands exactly as version 1 does.

```json
{
  "apiVersion": 2,
  "id": "acme.lang-nim",
  "name": "Nim",
  "version": "1.0.0",
  "engines": { "nulis": ">=0.1.0" },
  "contributes": {
    "languages": [{
      "id": "nim",
      "displayName": "Nim",
      "extensions": ["nim", "nims"],
      "filenames": ["config.nims"],
      "firstLine": "^#!.*\\bnim\\b",
      "comments": { "line": "#", "block": ["#[", "]#"] },
      "brackets": [["(", ")"], ["[", "]"], ["{", "}"]],
      "highlight": {
        "mode": "code",
        "keywords": ["proc", "func", "let", "var", "if", "elif", "else"],
        "types": ["int", "float", "string", "bool"],
        "strings": [{ "open": "\"", "close": "\"", "escape": true }],
        "caseInsensitive": false,
        "identChars": ["_"]
      },
      "indentation": {
        "increaseAfter": ":\\s*(?:#.*)?$",
        "outdentOnType": "^\\s*(?:elif|else|except|finally)\\b"
      },
      "completion": { "keywords": true, "words": true },
      "snippets": [{
        "prefix": "proc",
        "body": "proc ${1:name}(${2:args}): ${3:void} =\n  $0",
        "description": "Procedure declaration"
      }]
    }]
  }
}
```

Each language object has these fields. Unknown fields are rejected.

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Lowercase letters, digits, `-`, and `.`; at most 64 bytes. A plugin may not redefine a built-in ID, but may add file associations to one. |
| `displayName` | yes | Name shown in Language and Settings menus; at most 128 bytes. |
| `extensions` | no | Up to 64 file extensions in total with `filenames`; each is written without a dot in the array and matched as `*.ext`. |
| `filenames` | no | Exact file names or `prefix*` forms, same total cap. Matching is case-insensitive. |
| `firstLine` | no | A regular expression tried against the first line of a file whose name matches nothing. Compiled at install time with a 1 MiB automaton limit and a 4,096-byte pattern cap. |
| `comments` | no | `line` marker (`#`, `//`, `--`) and/or `block` pair (`["#[", "]#"]`); each marker 1–16 visible bytes. Drives Toggle Comment. |
| `brackets` | no | Up to 16 pairs of two distinct single characters. Validated now; auto-pairing uses them in a later release. |
| `highlight` | no | Table-driven highlighting: `mode` (`code`, `markup`, `css`, `delimited`), `keywords`, `types`, `strings`, `caseInsensitive`, and extra identifier bytes in `identChars`. |
| `indentation` | no | `increaseAfter` opens a block after Enter; `outdentOnType` moves a matching line left as its first character is typed. Both are bounded regexes. |
| `completion` | no | `keywords` offers the declared keywords and types; `words` offers words already in the document. Both default to `true`. |
| `snippets` | no | Up to 256 entries with `prefix`, plain-text `body` (at most 4 KiB), and `description`; prefixes are unique per language. Snippets are inserted as normal text, never rendered as HTML. |

Caps: at most 32 languages per plugin, 1,024 keywords plus types per language, 32 string rules, and a 256 KiB manifest. The whole language is validated when the package is installed, before approval. Highlighting uses the Rust `regex` crate only for bounded detection and indentation patterns, so a hostile pattern cannot backtrack; a bad contribution is reported and every other plugin keeps working.

A declarative package is installed, approved, enabled, and removed exactly like an executable one, but it requests no permissions and never starts a worker process. If two languages claim the same file pattern, the conflict appears in Settings ▸ Languages, and choosing one writes the user's `files.associations` entry.

See [`examples/plugins/nulis-nim`](../examples/plugins/nulis-nim) for a complete declarative package.

## Turning contributions off

The Plugins panel controls each approved plugin and each capability family it declares:

- **Disable** keeps the approval, digest, and settings but makes the plugin inert.
- **Commands**, **Languages**, **Highlight**, **Indentation**, **Completion**, and **Snippets** checkboxes switch one family off. Turning **Languages** off removes the plugin's languages, associations, and providers. Turning **Highlight** off keeps the language and its other providers but draws no colors.
- **Revoke** removes the approval; **Uninstall** removes the package.

Switches are stored in the same per-user approval document as the grant (`plugin-grants.json` in the app's session folder), bound to the exact package digest. A document written before version 2 has no switches and means "approved and enabled". Changing the manifest or source changes the digest, revokes effective approval, and resets its switches.

## Runtime API

The `nulis-plugin-sdk` package exports the versioned TypeScript contract and `definePlugin` manifest helper. Runtime artifacts must be self-contained scripts that assign or declare a global `activate(nulis, context)` function:

```ts
const activate: Activate = (nulis, context) => {
  context.subscriptions.push(
    nulis.commands.registerCommand("nulis.example-plugin.hello", async () => {
      await nulis.window.showInformationMessage("Hello from a plugin");
      return null;
    }),
  );
};
globalThis.activate = activate;
```

Version 1 exposes:

- `nulis.apiVersion`, currently `1`;
- `nulis.commands.registerCommand(id, handler)`;
- `nulis.window.showInformationMessage(message)`; and
- `context.subscriptions`, which accepts disposable registrations.

Activation and command handlers may be asynchronous. Arguments and results must be JSON-compatible. Runtime registrations must exactly match the manifest contributions or the run fails. A plugin is activated in a new worker for each startup event or command invocation; state is not retained across invocations.

## Approval and permissions

Installation never activates a plugin. The Plugins panel shows its declared commands, requested permissions, and package digest. The user must explicitly approve the request before contributions appear in the command palette. The persisted grant is separate from the package and bound to its exact SHA-256 digest, so any manifest or source change revokes effective approval.

Version 1 defines one permission:

- `showMessage`: allows `nulis.window.showInformationMessage` results to reach the desktop status surface.

Every returned host operation is checked against the persisted grant at the trusted core boundary. A grant cannot include authority the manifest did not request. Revocation immediately removes command contributions; uninstall revokes first and then removes the exact installed package.

## Isolation and limits

Each run launches the Nulis executable in worker mode with piped JSON input/output and a pinned pure-Rust Boa engine. Source is capped at 1 MiB, command arguments at 256 KiB, responses at 1 MiB, declared commands and returned operations at 256 each, and messages at 64 KiB each. JavaScript loop iterations, recursion, and stack size are capped. The parent kills the worker on cancellation, protocol failure, or the two-second execution deadline.

The worker has no host functions beyond the bootstrap API. A crash or malformed response fails the invocation without modifying editor state. OS-enforced per-worker memory quotas and signed distribution are deferred to M14 hardening; process isolation and strict input/output/runtime limits keep the native core outside the plugin address space in M12.

See [`examples/plugins/hello-nulis`](../examples/plugins/hello-nulis) for a minimal executable package, [`examples/plugins/nulis-tips`](../examples/plugins/nulis-tips) for a complete TypeScript project, and [`examples/plugins/nulis-nim`](../examples/plugins/nulis-nim) for a declarative language. [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md) is the step-by-step guide for plugin authors.
