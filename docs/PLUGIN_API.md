# Nulis plugin API

## M12 contract

The initial executable plugin API is version 1. Plugins run as JavaScript in a fresh worker process and receive only a small, serializable host API. They never receive Rust objects, webview handles, Node APIs, Web APIs, module loading, or ambient filesystem, network, process, environment, credential, terminal, remote, or clipboard access.

## Package and manifest

An installable package is a directory containing `plugin.json` and one UTF-8 `.js` entry script. Nulis validates the source directory, copies only those two files below application data, and refuses symlinks, traversal, unsupported files, malformed semantic versions, incompatible engine ranges, duplicate identities, or existing installations.

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

See [`examples/plugins/hello-nulis`](../examples/plugins/hello-nulis) for a minimal installable package and [`examples/plugins/nulis-tips`](../examples/plugins/nulis-tips) for a complete TypeScript project. [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md) is the step-by-step guide for plugin authors.
