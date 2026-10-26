import { expect, test } from "vitest";
import coreFacadeSource from "../../../crates/nulis-core/src/lib.rs?raw";
import builtinCommandsSource from "../../../crates/nulis-core/src/builtin_commands.rs?raw";
import shellSource from "../src-tauri/src/lib.rs?raw";
import appSource from "./App.tsx?raw";
import { EDITOR_ACTIONS, FIND_WINDOW_COMMANDS, WORKBENCH_ACTIONS } from "./commands";

/** The core's command registrations: the façade and the built-in commands it registers. */
const coreSource = `${coreFacadeSource}\n${builtinCommandsSource}`;

/** Command IDs listed in nulis-core's WORKBENCH_ACTIONS table. */
function coreWorkbenchActions(): string[] {
  const start = coreSource.indexOf("const WORKBENCH_ACTIONS");
  // A table that moved would leave nothing to compare, and every check here would pass.
  expect(start, "WORKBENCH_ACTIONS is in lib.rs or builtin_commands.rs").toBeGreaterThan(-1);
  const table = coreSource.slice(start, coreSource.indexOf("];", start));
  return [...table.matchAll(/\(\s*"([a-zA-Z0-9]+\.[a-zA-Z0-9]+)",/g)].map((match) => match[1]!);
}

/** Commands the native menus send, from the shell's menu tables and builders. */
function nativeMenuCommands(): string[] {
  const tables = [...shellSource.matchAll(/\(\s*"([a-zA-Z0-9]+\.[a-zA-Z0-9]+)",\s*"[^"]+",?\s*\)/g)].map((match) => match[1]!);
  const items = [...shellSource.matchAll(/with_id\(\s*"([a-zA-Z0-9]+\.[a-zA-Z0-9]+)"/g)].map((match) => match[1]!);
  const constants = [...shellSource.matchAll(/MENU_ID: &str = "([a-zA-Z0-9]+\.[a-zA-Z0-9]+)"/g)].map((match) => match[1]!);
  return [...new Set([...tables, ...items, ...constants])];
}

// Handled by the native shell itself, never sent to the page.
const SHELL_ONLY = new Set(["app.quit", "window.close"]);

test("the desktop mirrors the core's presentation commands exactly", () => {
  expect([...WORKBENCH_ACTIONS].sort()).toEqual([...new Set(coreWorkbenchActions())].sort());
});

test("every presentation command has a handler in the editor window", () => {
  const routed = new Set<string>([...Object.keys(EDITOR_ACTIONS), ...Object.keys(FIND_WINDOW_COMMANDS)]);
  const unhandled = coreWorkbenchActions().filter((id) => !routed.has(id) && !appSource.includes(`"${id}"`));
  expect(unhandled).toEqual([]);
});

test("every native menu item names a command the core knows or the shell handles", () => {
  const unknown = nativeMenuCommands()
    .filter((id) => !SHELL_ONLY.has(id) && !coreSource.includes(`"${id}"`));
  expect(unknown).toEqual([]);
  // The menus carry the commands added in M20–M29.
  for (const id of ["search.findNext", "format.lineEndingsCrlf", "view.toggleWordWrap", "file.quickOpen",
    "file.saveAll", "format.insertNumbers", "edit.pasteFromHistory"]) {
    expect(nativeMenuCommands()).toContain(id);
  }
});
