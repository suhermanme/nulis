# Nulis Tips

A complete example plugin written in TypeScript. It shows a tip of the day when Nulis starts and adds two commands to the command palette:

- **Show a Tip** shows a tip in the status bar and returns it as its result.
- **Count Tips** says how many tips there are and returns the number.

It demonstrates everything version 1 of the plugin API offers: the manifest, startup and command activation, command registration, messages, command arguments, command results, and bundling several source files into the single script Nulis loads.

## Files

| File | Purpose |
| --- | --- |
| `plugin.json` | The manifest Nulis reads. |
| `src/index.ts` | The entry point: registers the commands. |
| `src/tips.ts` | The tips and the function that picks one. |
| `index.js` | The built plugin. This is the file Nulis runs. It is checked in so the example installs without a build. |
| `package.json`, `tsconfig.json` | Build and type-check configuration. |

## Install it

1. In Nulis, click **Plugins** in the main toolbar.
2. Click **Install from folder…** and choose this folder.
3. Review the request (`showMessage`) and click **Approve requested access**.
4. Press Ctrl/⌘ K and run **Show a Tip**.

Nulis copies only `plugin.json` and `index.js`; the other files are ignored.

## Change it and rebuild

From the repository root, after `npm ci`:

```sh
cd examples/plugins/nulis-tips
npx esbuild src/index.ts --bundle --format=iife --target=es2020 --charset=utf8 --outfile=index.js
npx tsc --noEmit        # optional: type-check against the SDK
```

Then, in the Plugins panel, **Uninstall** the plugin, install it again from this folder, and approve it again. Approval is tied to the exact content of the package, so a rebuilt plugin always needs a new approval.

The complete guide to writing plugins is [docs/PLUGIN_GUIDE.md](../../../docs/PLUGIN_GUIDE.md).
