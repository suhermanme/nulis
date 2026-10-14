# Hello Nulis

This is an installable M12 plugin package. In Nulis, open **Plugins**, choose **Install from folder…**, select this directory, review the requested `showMessage` permission, and approve it. The command **Hello from Example Plugin** then appears in the command palette.

`index.ts` is the typed source and `index.js` is the self-contained runtime artifact. Production plugins should bundle their TypeScript as a single script that assigns `globalThis.activate`; the isolated worker intentionally provides neither a module loader nor Node/Web APIs.
