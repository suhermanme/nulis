# Building Plugins

This chapter explains how to write a Nulis plugin, from an empty folder to an installed, approved command. It covers the plugin package, the manifest, the runtime API, the TypeScript SDK, building, testing, and every rule and limit the plugin host enforces.

Two working examples are in the repository:

- `examples/plugins/hello-nulis` — the smallest possible plugin: one command, one message.
- `examples/plugins/nulis-tips` — a complete TypeScript project with two commands, a startup action, arguments, results, and a build step.

## What a plugin can do today

Plugin API version 1 is deliberately small. A plugin can:

- **add commands** to the command palette, each with a title and a category;
- **show a message** in the status bar;
- **run code at startup**;
- **receive an argument** and **return a result** from a command.

A plugin cannot read or change documents, open files, use the network, start processes, read the clipboard, or draw its own interface. There is no API for these in version 1, and the runtime has no other way to reach them. Later versions will add capabilities one permission at a time.

## How plugins run

Understanding the execution model avoids most surprises.

- **A plugin is one JavaScript file.** There is no module loader: `import`, `require`, Node.js APIs, and browser APIs such as `fetch`, `setTimeout`, `console`, or the DOM do not exist. Standard JavaScript built-ins (`Math`, `JSON`, `Date`, `Map`, `Promise`, string and array methods, and so on) are available.
- **Every run starts from nothing.** Each time Nulis needs your plugin — at startup, or when one of its commands is chosen — it starts a new, separate worker process, loads your file, calls `activate`, runs the one command if there is one, collects the outcome, and ends the process. Variables do not survive from one command to the next.
- **A run is short.** It has two seconds, and limits on loop iterations, recursion depth, and output size. A plugin that exceeds them is stopped and the command fails; the editor is unaffected.
- **One plugin runs at a time.**
- **Nothing happens without approval.** Installing a plugin does not run it. The user must approve what it asks for, and the approval is valid only for that exact version of the files.

## Quick start: your first plugin

This makes a plugin with one command, in plain JavaScript, with no tools needed.

**1. Make a folder** anywhere, for example `my-first-plugin`.

**2. Create `plugin.json`** in it:

```json
{
  "id": "yourname.first",
  "name": "My First Plugin",
  "version": "1.0.0",
  "engines": { "nulis": ">=0.1.0" },
  "permissions": ["showMessage"],
  "activationEvents": ["onCommand:yourname.first.greet"],
  "main": "index.js",
  "contributes": {
    "commands": [
      { "command": "yourname.first.greet", "title": "Greet Me", "category": "My Plugin" }
    ]
  }
}
```

**3. Create `index.js`** beside it:

```js
function activate(nulis, context) {
  context.subscriptions.push(
    nulis.commands.registerCommand("yourname.first.greet", async () => {
      await nulis.window.showInformationMessage("Hello from my first plugin!");
      return null;
    }),
  );
}
```

**4. Install it.** In Nulis, click **Plugins** in the main toolbar, click **Install from folder…**, and choose the folder.

**5. Approve it.** The panel lists the plugin with its commands and the permission it requests. Click **Approve requested access**.

**6. Run it.** Press Ctrl/⌘ K, type "Greet", and press Enter. The status bar shows "Hello from my first plugin!". The command is listed under the category "Plugin · My Plugin".

## The plugin package

A package is a folder with two files that matter:

| File | Required | Purpose |
| --- | --- | --- |
| `plugin.json` | Yes | The manifest: identity, permissions, commands. |
| The entry script | Yes | One UTF-8 JavaScript file, named by `main` in the manifest. |

Nulis copies exactly these two files into its own data folder when you install. Anything else in the folder — TypeScript sources, `package.json`, a README, `node_modules` — is ignored, so you can install straight from your project folder.

Rules for the package:

- The folder and both files must be real files and folders, not symbolic links.
- The entry script must have the extension `.js` and be at most 1 MiB.
- `plugin.json` must be at most 256 KiB.
- `main` is a path inside the folder. It may be in a subfolder (`dist/index.js`), but may not contain `..` or be absolute.

## The manifest: plugin.json

```json
{
  "id": "nulis.tips",
  "name": "Nulis Tips",
  "version": "1.0.0",
  "engines": { "nulis": ">=0.1.0" },
  "permissions": ["showMessage"],
  "activationEvents": [
    "onStartup",
    "onCommand:nulis.tips.show",
    "onCommand:nulis.tips.count"
  ],
  "main": "index.js",
  "contributes": {
    "commands": [
      { "command": "nulis.tips.show", "title": "Show a Tip", "category": "Tips" },
      { "command": "nulis.tips.count", "title": "Count Tips", "category": "Tips" }
    ]
  }
}
```

| Field | Required | Meaning and rules |
| --- | --- | --- |
| `id` | Yes | The plugin's unique identity. Letters, digits, `.`, `-`, and `_` only; it must contain at least one `.`; at most 128 characters. It may not start with `plugin.` or `automation.`. Use a prefix you own, such as `yourname.pluginname`. |
| `name` | Yes | The name shown in the Plugins panel. 1 to 128 characters. |
| `version` | Yes | A semantic version: `1.0.0`, `0.3.1`, `2.0.0-beta.1`. |
| `engines.nulis` | Yes | The Nulis versions the plugin works with, as a semantic-version range: `>=0.1.0`, `^0.1`, `>=0.1.0, <0.3.0`. Installation is refused if the running Nulis is outside the range. |
| `permissions` | No | What the plugin asks to be allowed to do. See [Permissions](#permissions). Default: none. |
| `activationEvents` | No* | When the plugin runs. See below. |
| `main` | Yes | The entry script, relative to the folder. |
| `contributes.commands` | No | The commands the plugin adds. |

Each command has:

| Field | Required | Meaning and rules |
| --- | --- | --- |
| `command` | Yes | The command's ID. It must start with the plugin's `id` followed by a dot: plugin `nulis.tips` may declare `nulis.tips.show` but not `tips.show`. Same character rules as `id`. |
| `title` | Yes | The text shown in the command palette. 1 to 128 characters. |
| `category` | No | A group name. The palette shows it as "Plugin · *category*". Default: "Plugin". |

A plugin may declare up to 256 commands.

**Unknown fields are an error.** The manifest is checked strictly: a misspelled or extra field makes installation fail, with a message naming it.

### Activation events

`activationEvents` says when the plugin is run.

| Event | When it fires |
| --- | --- |
| `onCommand:<command id>` | The user (or another caller) runs that command. |
| `onStartup` | Nulis has started and restored the session. |

Two rules connect events and commands, and installation is refused if either is broken:

1. **Every declared command needs its `onCommand:` event.** If `contributes.commands` has `nulis.tips.show`, `activationEvents` must contain `onCommand:nulis.tips.show`.
2. **Every `onCommand:` event needs its command.** An event that names an undeclared command is an error.

`onStartup` is optional and is the only event that is not tied to a command.

## The runtime API

Your script must define a function named `activate`, either as a top-level function declaration or by assigning it to `globalThis.activate`. Nulis calls it with two arguments.

```js
function activate(nulis, context) { /* ... */ }
```

### nulis.apiVersion

The number `1`. Check it if your plugin supports several API versions.

### nulis.commands.registerCommand(id, handler)

Connects a command ID from the manifest to a function.

```js
nulis.commands.registerCommand("nulis.tips.count", async () => {
  await nulis.window.showInformationMessage("There are 8 tips.");
  return 8;
});
```

- `id` must be a command declared in the manifest.
- `handler` may be a normal or an `async` function. It receives the command's argument (see [Arguments](#arguments-and-results)) and returns the command's result.
- The call returns a *disposable*: an object with a `dispose()` method that removes the registration. Push it onto `context.subscriptions`.
- Registering the same ID twice throws an error.

**The registrations must match the manifest exactly.** After `activate` finishes, Nulis compares the set of registered command IDs with the set declared in `plugin.json`. If a declared command was not registered, or an undeclared one was, the run fails with "plugin runtime registrations do not match its manifest". Register every command, unconditionally, every time `activate` is called.

### nulis.window.showInformationMessage(message)

Shows a text message in the status bar at the bottom of the window.

```js
await nulis.window.showInformationMessage("Done.");
```

- `message` must be a string, at most 64 KiB.
- It returns a promise that is already resolved. `await` it or not, as you prefer.
- It requires the `showMessage` permission. Without it the whole run fails with a permission error.
- **Only the last message of a run is displayed.** A run may send several messages, but the status bar has one line; the final one wins.

### context.subscriptions

An array. Push the disposables returned by `registerCommand` onto it. This mirrors the convention of other editors and keeps plugins ready for future API versions that clean up registrations.

### Arguments and results

A command handler receives one argument and returns one result. Both must be JSON values: `null`, a boolean, a number, a string, an array, or an object made of these.

- **Argument.** When a command is run from the command palette, a menu, or a shortcut, the argument is an empty object, `{}`. Another caller may pass an object with fields. Write handlers that work when their fields are missing:

  ```js
  nulis.commands.registerCommand("nulis.tips.show", async (argument) => {
    const index = argument && typeof argument.index === "number" ? argument.index : 0;
    // ...
  });
  ```

- **Result.** Whatever the handler returns is passed back to the caller. Return `null` when there is nothing to report. `undefined` is converted to `null`. A result that is not valid JSON (a function, a cyclic object) makes the run fail.

The arguments of a run may total at most 256 KiB, and the whole response at most 1 MiB.

### Errors

If `activate` or a handler throws, or a returned promise is rejected, the run fails and the error text is shown in the status bar as "Could not run *command title*" with the reason. Nothing else is affected.

### Startup activation

With `onStartup` in the manifest, `activate` is called once after Nulis starts, with no command. Use it for something the user should see at launch.

Remember that `activate` also runs before every command, and that a plugin cannot tell the two cases apart. Because only the last message of a run is shown, a message sent from `activate` appears at startup and is replaced by the command's own message when a command runs. The Nulis Tips example uses exactly this for its tip of the day.

## Permissions

A permission is something a plugin asks to be allowed to do. The user sees the request in the Plugins panel and grants it by approving.

| Permission | Allows |
| --- | --- |
| `showMessage` | `nulis.window.showInformationMessage` |

Rules:

- Ask only for what you use. A permission you do not request cannot be granted.
- An operation without its permission fails the whole run; it is not silently skipped.
- The grant is stored by Nulis, separately from the plugin, and is bound to the SHA-256 digest of `plugin.json` and the entry script together. The first 12 characters of the digest are shown in the Plugins panel as the plugin's *revision*.
- If either file changes in any way, the digest changes and the approval no longer applies. The plugin's commands disappear from the palette until the user approves the new revision.

## Writing a plugin in TypeScript

TypeScript gives you checked types for the whole API. The `nulis-plugin-sdk` package, in `packages/nulis-plugin-sdk`, contains the type definitions; it has no runtime code your plugin needs.

### What the SDK exports

| Export | Kind | Purpose |
| --- | --- | --- |
| `Activate` | type | The type of the `activate` function. |
| `NulisApi` | type | The first argument of `activate`: `apiVersion`, `commands`, `window`. |
| `ExtensionContext` | type | The second argument of `activate`: `subscriptions`. |
| `CommandHandler` | type | A command handler function. |
| `Disposable` | type | An object with `dispose()`. |
| `JsonValue`, `JsonPrimitive` | types | What arguments and results may be. |
| `PluginManifest`, `PluginCommandContribution`, `PluginPermission` | types | The shape of `plugin.json`. |
| `definePlugin(manifest)` | function | Returns the manifest unchanged while type-checking it. Useful if you generate `plugin.json` from a script. |
| `PLUGIN_API_VERSION` | constant | `1`. |

### Project layout

This is the layout of `examples/plugins/nulis-tips`:

```text
nulis-tips/
  plugin.json       the manifest
  src/
    index.ts        entry point: defines activate
    tips.ts         an ordinary module
  index.js          the built plugin — the file Nulis runs
  package.json      build scripts
  tsconfig.json     compiler settings
```

### The source

`src/tips.ts` is a normal module:

```ts
export const TIPS: readonly string[] = [
  "Press Ctrl/⌘ K to run any command by name.",
  "Hold Alt and click to add a second caret.",
  // ...
];

export function pickTip(index: unknown, today: Date = new Date()): { index: number; text: string } {
  const chosen = typeof index === "number" && Number.isInteger(index)
    ? index
    : Math.floor(today.getTime() / 86_400_000);
  const wrapped = ((chosen % TIPS.length) + TIPS.length) % TIPS.length;
  return { index: wrapped, text: TIPS[wrapped] };
}
```

`src/index.ts` imports it and defines `activate`:

```ts
import type { Activate, JsonValue } from "nulis-plugin-sdk";
import { pickTip, TIPS } from "./tips";

function requestedIndex(argument: JsonValue | undefined): unknown {
  return argument !== null && typeof argument === "object" && !Array.isArray(argument)
    ? argument.index
    : undefined;
}

const activate: Activate = (nulis, context) => {
  void nulis.window.showInformationMessage(`Tip of the day: ${pickTip(undefined).text}`);

  context.subscriptions.push(
    nulis.commands.registerCommand("nulis.tips.show", async (argument) => {
      const tip = pickTip(requestedIndex(argument));
      await nulis.window.showInformationMessage(`Tip ${tip.index + 1} of ${TIPS.length}: ${tip.text}`);
      return { index: tip.index, text: tip.text };
    }),
    nulis.commands.registerCommand("nulis.tips.count", async () => {
      await nulis.window.showInformationMessage(`Nulis Tips knows ${TIPS.length} tips.`);
      return TIPS.length;
    }),
  );
};

(globalThis as typeof globalThis & { activate?: Activate }).activate = activate;
```

The last line matters: a bundler wraps your code in a function, so a plain `function activate` would be hidden inside it. Assigning to `globalThis.activate` makes it visible to Nulis.

### Building

Nulis loads one script with no module loader, so the TypeScript files must be **bundled** into a single file. Any bundler that can produce a plain script works. With [esbuild](https://esbuild.github.io):

```sh
npx esbuild src/index.ts --bundle --format=iife --target=es2020 --charset=utf8 --outfile=index.js
```

| Option | Why |
| --- | --- |
| `--bundle` | Combines `index.ts`, `tips.ts`, and any libraries into one file. |
| `--format=iife` | Produces a plain script, not a module. Module output (`esm`, `cjs`) will not load. |
| `--target=es2020` | Keeps the output to language features the plugin runtime supports. |
| `--charset=utf8` | Keeps non-ASCII text readable instead of escaping it. |

`import type` lines disappear during the build, so the SDK adds nothing to the output.

To type-check, point TypeScript at the SDK. Inside this repository the example does it with a path mapping in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "noEmit": true,
    "lib": ["ES2020"],
    "paths": { "nulis-plugin-sdk": ["../../../packages/nulis-plugin-sdk/src/index.ts"] }
  },
  "include": ["src"]
}
```

Setting `"lib": ["ES2020"]` without `"DOM"` is useful: TypeScript then reports an error if you use a browser API that will not exist when the plugin runs.

For a plugin outside this repository, copy `packages/nulis-plugin-sdk/src/index.ts` into your project and point the path mapping at your copy. The SDK is not published to a package registry yet.

### Using libraries

A library can be bundled into your plugin if it is pure JavaScript and does not need Node.js or browser APIs. Keep the built file under 1 MiB, and remember the two-second limit: a library that does heavy work when it loads does that work on every run.

## Install, approve, run

1. **Plugins ▸ Install from folder…** and choose the plugin folder. Nulis validates the package and copies the two files. The status bar reads "Installed *name*; review and approve its permissions".
2. The panel shows the plugin's name, version, commands, requested permissions, and revision. Click **Approve requested access**.
3. The plugin's commands now appear in the command palette.

Other actions in the panel:

| Action | Effect |
| --- | --- |
| **Revoke** | Withdraws the approval. The commands disappear at once. The plugin stays installed. |
| **Uninstall** | Revokes and removes the installed copy. Your source folder is not touched. |

Installed plugins live in `sessions/plugins/` inside Nulis's application-data folder, one subfolder per plugin ID. Up to 128 plugins can be installed. Do not edit the installed copy: any change invalidates the approval.

Commands of an approved plugin can also be given a keyboard shortcut, or placed in the File menu or on the main toolbar, under **Settings ▸ Keybindings** and **Menus & Toolbar**.

## The development loop

Nulis refuses to install a plugin whose ID is already installed, so updating a plugin during development is:

1. Edit the source.
2. Rebuild `index.js` (if you use a build step).
3. In the Plugins panel: **Uninstall**, then **Install from folder…**, then **Approve requested access**.
4. Run the command.

The approval step is required every time because the files changed.

## Testing a plugin

**Unit-test your logic with ordinary tools.** Keep the real work in plain functions that do not touch the `nulis` object — like `pickTip` in the example — and test them with any JavaScript test runner.

**Test `activate` with a fake API.** The API is small enough to fake in a few lines:

```js
function fakeNulis() {
  const handlers = {};
  const messages = [];
  return {
    handlers,
    messages,
    api: {
      apiVersion: 1,
      commands: {
        registerCommand(id, handler) {
          handlers[id] = handler;
          return { dispose() { delete handlers[id]; } };
        },
      },
      window: {
        showInformationMessage(message) { messages.push(message); return Promise.resolve(); },
      },
    },
  };
}

const fake = fakeNulis();
await activate(fake.api, { subscriptions: [] });
const result = await fake.handlers["nulis.tips.count"]({});
// Check `result` and `fake.messages`.
```

**Test in the real worker.** If you work inside the Nulis repository, the integration tests in `crates/nulis-plugin-host/tests/process_worker.rs` load an example package and run it in the same worker process the application uses. Both examples are tested there; copy one of those tests for your plugin and run:

```sh
cargo test -p nulis-plugin-host --test process_worker
```

## Limits

| Item | Limit |
| --- | --- |
| Entry script | 1 MiB |
| `plugin.json` | 256 KiB |
| Plugin ID, command ID | 128 characters |
| Name, title, category | 128 characters |
| Commands per plugin | 256 |
| Installed plugins | 128 |
| Time per run | 2 seconds |
| Loop iterations per run | 1,000,000 |
| Recursion depth | 128 |
| Command arguments | 256 KiB |
| Response (result and messages) | 1 MiB |
| One message | 64 KiB |
| Host operations per run | 256 |

## Troubleshooting

| What you see | Cause and fix |
| --- | --- |
| Installation fails mentioning an unknown field | `plugin.json` has a misspelled or unsupported field. The manifest is checked strictly. |
| "invalid plugin ID" or "invalid plugin command ID" | An ID contains a character other than letters, digits, `.`, `-`, `_`, has no `.`, or is longer than 128 characters. |
| "plugin command … is outside namespace …" | A command ID does not start with the plugin ID and a dot. |
| "plugin command has no matching activation event" | A declared command has no `onCommand:` event in `activationEvents`. |
| "invalid plugin activation event" | An event is not `onStartup`, or names a command that is not declared. |
| "plugin requires Nulis …, but this host is …" | The running Nulis version is outside `engines.nulis`. |
| "plugin is already installed" | Uninstall the old copy first. |
| The commands are not in the palette | The plugin is not approved, or its files changed after approval. Approve it in the Plugins panel. |
| "plugin must define activate(nulis, context)" | The script does not define a global `activate`. With a bundler, assign `globalThis.activate = activate`. |
| "plugin runtime registrations do not match its manifest" | `activate` did not register exactly the commands in `plugin.json`. |
| "command was not registered" | The handler for the command that was run is missing. |
| "plugin permission denied" | The plugin called `showInformationMessage` without `showMessage` in `permissions`. |
| "JavaScript runtime limit exceeded", or the command stops after two seconds | The run hit the loop, recursion, or time limit. Do less work per run. |
| `X is not defined` | The script uses an API that does not exist in the plugin runtime (`console`, `setTimeout`, `fetch`, `require`, …). |
| "a plugin is already running" | Another plugin run has not finished. Try again. |

## Checklist before sharing a plugin

- The ID uses a prefix that is yours.
- `version` is raised for every release.
- `engines.nulis` names the versions you tested.
- Only the permissions you use are requested.
- Every command has a clear title and a category.
- Handlers work with an empty argument.
- The built `index.js` is included, so the plugin installs without a build step.
- A license file says how others may use it.
