import type { ButtonHTMLAttributes, ReactNode } from "react";

/**
 * One bundled, original vector icon set. Every glyph is drawn on a 24 px grid
 * with a 1.6 px round stroke so toolbars stay optically consistent; nothing is
 * downloaded at runtime and no third-party artwork is embedded.
 */
const PATHS = {
  new: <><path d="M14 3.5H7.5A1.5 1.5 0 0 0 6 5v14a1.5 1.5 0 0 0 1.5 1.5h9A1.5 1.5 0 0 0 18 19V7.5z" /><path d="M14 3.5v4h4" /><path d="M12 11.5v5M9.5 14h5" /></>,
  open: <><path d="M3.5 18V6.5A1.5 1.5 0 0 1 5 5h4l2 2.5h7A1.5 1.5 0 0 1 19.5 9v1.5" /><path d="M3.5 18l2.2-6.4a1.5 1.5 0 0 1 1.4-1.1H20a1 1 0 0 1 .95 1.3L19 18.4a1.5 1.5 0 0 1-1.4 1.1H5A1.5 1.5 0 0 1 3.5 18z" /></>,
  close: <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />,
  save: <><path d="M5.5 4h10.2L20 8.3V18.5A1.5 1.5 0 0 1 18.5 20h-13A1.5 1.5 0 0 1 4 18.5v-13A1.5 1.5 0 0 1 5.5 4z" /><path d="M8 4v4.5h6.5V4M7.5 20v-6h9v6" /></>,
  saveAs: <><path d="M12 20H5.5A1.5 1.5 0 0 1 4 18.5v-13A1.5 1.5 0 0 1 5.5 4h10.2L20 8.3V11" /><path d="M8 4v4.5h6.5V4M7.5 20v-6H12" /><path d="M14.5 20.5l.6-2.6 4.7-4.7 2 2-4.7 4.7z" /></>,
  undo: <><path d="M8.5 5L4.5 9l4 4" /><path d="M4.5 9h9.5a5 5 0 0 1 0 10H9" /></>,
  redo: <><path d="M15.5 5l4 4-4 4" /><path d="M19.5 9H10a5 5 0 0 0 0 10h5" /></>,
  wrap: <><path d="M4 6h16M4 18h6" /><path d="M4 12h12.5a3 3 0 0 1 0 6H14" /><path d="M16 15.8L13.8 18l2.2 2.2" /></>,
  column: <><rect x="4" y="4" width="16" height="16" rx="1.5" strokeDasharray="2.4 2.4" /><path d="M10 7.5v9M14 7.5v9" /></>,
  hex: <><path d="M12 3.5l7.4 4.25v8.5L12 20.5l-7.4-4.25v-8.5z" /><path d="M9.2 10v4M9.2 12h2.4M11.6 10v4M14.6 10v4" /></>,
  find: <><circle cx="10.5" cy="10.5" r="6" /><path d="M15 15l5 5" /></>,
  previous: <path d="M6 14.5l6-6 6 6" />,
  next: <path d="M6 9.5l6 6 6-6" />,
  replace: <><path d="M4 8h10.5a4 4 0 0 1 4 4" /><path d="M7 5L4 8l3 3" /><path d="M20 16H9.5a4 4 0 0 1-4-4" /><path d="M17 19l3-3-3-3" /></>,
  highlight: <><path d="M9 14.5l6.8-9.2 3.4 2.5-6.8 9.2z" /><path d="M9 14.5l-1.2 4 4.6-1.5" /><path d="M4 20.5h16" /></>,
  goTo: <><path d="M4 6h9M4 12h6M4 18h9" /><path d="M14 12h6.5M17.5 9l3 3-3 3" /></>,
  lineChanges: <><path d="M6 4v5M6 12v3.5M6 18.5v1.5" /><path d="M10.5 6.5H20M10.5 13.5H17M10.5 19H20" /></>,
  map: <><rect x="3.5" y="4" width="17" height="16" rx="1.5" /><path d="M15 4v16" /><path d="M6.5 8h5.5M6.5 11.5h4M6.5 15h5.5M17 8h1.5M17 11h1.5" /></>,
  preview: <><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" /><circle cx="12" cy="12" r="2.8" /></>,
  edit: <><path d="M5 19l.9-3.9L16.6 4.4a1.6 1.6 0 0 1 2.3 0l.7.7a1.6 1.6 0 0 1 0 2.3L8.9 18.1z" /><path d="M14.5 6.5l3 3" /></>,
  splitHorizontal: <><rect x="3.5" y="4.5" width="17" height="15" rx="1.5" /><path d="M12 4.5v15" /></>,
  splitVertical: <><rect x="3.5" y="4.5" width="17" height="15" rx="1.5" /><path d="M3.5 12h17" /></>,
  unsplit: <><rect x="3.5" y="4.5" width="17" height="15" rx="1.5" /><path d="M9.5 9.5l5 5M14.5 9.5l-5 5" /></>,
  files: <><path d="M8 3.5h6.5L19 8v9.5A1.5 1.5 0 0 1 17.5 19H8a1.5 1.5 0 0 1-1.5-1.5V5A1.5 1.5 0 0 1 8 3.5z" /><path d="M14.5 3.5V8H19" /><path d="M4 7.5V20a1 1 0 0 0 1 1h9.5" /></>,
  sidebar: <><rect x="3.5" y="4.5" width="17" height="15" rx="1.5" /><path d="M9 4.5v15" /><path d="M5.8 8.5h1M5.8 11.5h1" /></>,
  settings: <><path d="M4 7h9M17 7h3M4 12h3M11 12h9M4 17h10M18 17h2" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="12" r="2" /><circle cx="16" cy="17" r="2" /></>,
  remote: <><circle cx="12" cy="12" r="8.5" /><path d="M3.5 12h17" /><path d="M12 3.5c2.6 2.4 3.8 5.2 3.8 8.5s-1.2 6.1-3.8 8.5c-2.6-2.4-3.8-5.2-3.8-8.5S9.4 5.9 12 3.5z" /></>,
  symbols: <><path d="M9 5H7.5A1.5 1.5 0 0 0 6 6.5v3.2c0 1-.7 2.3-2 2.3 1.3 0 2 1.3 2 2.3v3.2A1.5 1.5 0 0 0 7.5 19H9" /><path d="M15 5h1.5A1.5 1.5 0 0 1 18 6.5v3.2c0 1 .7 2.3 2 2.3-1.3 0-2 1.3-2 2.3v3.2a1.5 1.5 0 0 1-1.5 1.5H15" /></>,
  terminal: <><rect x="3.5" y="4.5" width="17" height="15" rx="1.5" /><path d="M7 9.5l3 2.5-3 2.5M12.5 15h4.5" /></>,
  automation: <><path d="M13 3.5L5.5 13.5h6L10.5 20.5 18.5 10h-6z" /></>,
  plugins: <><path d="M9.5 4.5h5v3.2a1.8 1.8 0 1 0 3.5 0h1.5v5h-3.2a1.8 1.8 0 1 1 0 3.5H19.5v3.3h-15V4.5z" transform="translate(0 .2)" /></>,
  compare: <><rect x="3.5" y="5" width="6.5" height="14" rx="1.2" /><rect x="14" y="5" width="6.5" height="14" rx="1.2" /><path d="M11 9.5h2M11 14.5h2" /></>,
  folders: <><path d="M3.5 8V6A1.5 1.5 0 0 1 5 4.5h3L9.5 6H14a1.5 1.5 0 0 1 1.5 1.5V8" /><path d="M8.5 18.5V11a1.5 1.5 0 0 1 1.5-1.5h3l1.5 1.5H19a1.5 1.5 0 0 1 1.5 1.5v6A1.5 1.5 0 0 1 19 20h-9a1.5 1.5 0 0 1-1.5-1.5z" /><path d="M3.5 8v6A1.5 1.5 0 0 0 5 15.5h1" /></>,
  bookmark: <path d="M7 4.5h10a1 1 0 0 1 1 1V20l-6-4.2L6 20V5.5a1 1 0 0 1 1-1z" />,
  bookmarkPrevious: <><path d="M10.5 4.5h8a1 1 0 0 1 1 1V20l-5-3.8-5 3.8V5.5a1 1 0 0 1 1-1z" /><path d="M6.5 8.5l-3 3 3 3" /></>,
  bookmarkNext: <><path d="M5.5 4.5h8a1 1 0 0 1 1 1V20l-5-3.8-5 3.8V5.5a1 1 0 0 1 1-1z" /><path d="M17.5 8.5l3 3-3 3" /></>,
  bookmarkClear: <><path d="M7 4.5h10a1 1 0 0 1 1 1V20l-6-4.2L6 20V5.5a1 1 0 0 1 1-1z" /><path d="M9.8 8.3l4.4 4.4M14.2 8.3l-4.4 4.4" /></>,
  fold: <><path d="M4 6h16M4 18h16" /><path d="M9 10.5l3 2.8 3-2.8" /></>,
  more: <><circle cx="5.5" cy="12" r="1.1" /><circle cx="12" cy="12" r="1.1" /><circle cx="18.5" cy="12" r="1.1" /></>,
  plus: <path d="M12 5.5v13M5.5 12h13" />,
  command: <><path d="M5 8l4 4-4 4" /><path d="M11.5 16.5H19" /></>,
  caseSensitive: <><path d="M3.5 17L7.5 7h1l4 10M5 13.5h6" /><path d="M20.5 17v-5.2a2.4 2.4 0 0 0-4.6-.9M20.5 14.2a2.6 2.6 0 1 1-2.6-2.4" /></>,
  regex: <><path d="M16 4.5v8M12.5 6.5l7 4M19.5 6.5l-7 4" /><rect x="4.5" y="15" width="4" height="4" rx=".8" /></>,
  rename: <><path d="M4 18.5h16" /><path d="M7 15l.7-3L15.5 4.2l2.3 2.3L10 14.3z" /></>,
  duplicate: <><rect x="8" y="8" width="11.5" height="11.5" rx="1.5" /><path d="M5.5 15.5h-.5A1.5 1.5 0 0 1 3.5 14V5A1.5 1.5 0 0 1 5 3.5h9A1.5 1.5 0 0 1 15.5 5v.5" /></>,
  workspace: <><rect x="3.5" y="4" width="7" height="7" rx="1.2" /><rect x="13.5" y="4" width="7" height="7" rx="1.2" /><rect x="3.5" y="14" width="17" height="6" rx="1.2" /></>,
  appearance: <><circle cx="12" cy="12" r="8.5" /><path d="M12 3.5v17a8.5 8.5 0 0 0 0-17z" fill="currentColor" stroke="none" /></>,
  keyboard: <><rect x="2.5" y="6.5" width="19" height="11" rx="1.6" /><path d="M6 10h.5M9.5 10h.5M13 10h.5M16.5 10h1.5M6 13h.5M9 14h6M17.5 13h.5" /></>,
  language: <><path d="M4 6.5h9M8.5 4.5v2M11 6.5c-.6 3.6-3 6.4-6.5 8" /><path d="M6.5 10c1 2 2.8 3.5 5 4.5" /><path d="M12.5 20l3.8-9.5L20 20M14 16.5h4.5" /></>,
  general: <><circle cx="12" cy="12" r="3" /><path d="M12 3.5v2.5M12 18v2.5M3.5 12H6M18 12h2.5M6 6l1.8 1.8M16.2 16.2L18 18M6 18l1.8-1.8M16.2 7.8L18 6" /></>,
  security: <><path d="M12 3.5l7 2.8v5.2c0 4.3-2.9 7.6-7 9-4.1-1.4-7-4.7-7-9V6.3z" /><path d="M9 12l2.2 2.2L15.2 10" /></>,
  editor: <><path d="M4 6.5h16M4 11h10M4 15.5h13M4 20h7" /></>,
  menus: <><rect x="3.5" y="4" width="17" height="4.5" rx="1.2" /><path d="M6.5 12.5h11M6.5 16h8M6.5 19.5h9.5" /></>,
  overwrite: <><rect x="6.5" y="5" width="11" height="14" rx="1.2" /><path d="M9.5 9.5h5M9.5 14.5h5" /></>,
  insert: <path d="M9.5 4.5h5M12 4.5v15M9.5 19.5h5" />,
  folder: <path d="M3.5 7A1.5 1.5 0 0 1 5 5.5h4l2 2.5h8A1.5 1.5 0 0 1 20.5 9.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z" />,
  drive: <><rect x="3.5" y="12.5" width="17" height="6.5" rx="1.5" /><path d="M5.5 12.5L7.3 6a1.5 1.5 0 0 1 1.4-1h6.6a1.5 1.5 0 0 1 1.4 1l1.8 6.5" /><path d="M16.5 15.8h1" /></>,
  document: <><path d="M14 3.5H7.5A1.5 1.5 0 0 0 6 5v14a1.5 1.5 0 0 0 1.5 1.5h9A1.5 1.5 0 0 0 18 19V7.5z" /><path d="M14 3.5v4h4" /></>,
  validate: <><path d="M8 3.5H6.5A1.5 1.5 0 0 0 5 5v14a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 19v-6" /><path d="M9 12.5l3 3 8-9" /></>,
  beautify: <><path d="M4 6h10M8 10.5h12M8 15h9M4 19.5h10" /><path d="M4 9.5v6" /></>,
  minify: <><path d="M4 9h16M4 15h16" /><path d="M9.5 4.5L12 7l2.5-2.5M9.5 19.5L12 17l2.5 2.5" /></>,
  lock: <><rect x="5.5" y="10.5" width="13" height="9.5" rx="1.6" /><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5" /></>,
  help: <><circle cx="12" cy="12" r="8.5" /><path d="M9.6 9.6a2.5 2.5 0 1 1 3.7 2.2c-.8.5-1.3 1-1.3 2" /><path d="M12 16.9v.1" /></>,
  scrollLeft: <path d="M14.5 6l-6 6 6 6" />,
  scrollRight: <path d="M9.5 6l6 6-6 6" />,
  popOut: <><path d="M13.5 4.5h6v6" /><path d="M19.5 4.5l-8 8" /><path d="M10 6.5H6A1.5 1.5 0 0 0 4.5 8v10A1.5 1.5 0 0 0 6 19.5h10a1.5 1.5 0 0 0 1.5-1.5v-4" /></>,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof PATHS;

export const ICON_NAMES = Object.keys(PATHS) as IconName[];

export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"
    aria-hidden="true" focusable="false" data-icon={name}>{PATHS[name]}</svg>;
}

/** Presentation data only: the command ID stays the single source of the action. */
export const COMMAND_ICONS: Record<string, IconName> = {
  "app.about": "general",
  "file.new": "new",
  "file.open": "open",
  "file.save": "save",
  "file.saveAs": "saveAs",
  "workspace.open": "files",
  "workspace.close": "close",
  "workspace.rename": "rename",
  "editor.undo": "undo",
  "editor.redo": "redo",
  "editor.toggleReadOnly": "lock",
  "workbench.commandPalette": "command",
  "workbench.toggleSidebar": "sidebar",
  "workbench.toggleSplit": "splitHorizontal",
  "workbench.splitRight": "splitHorizontal",
  "workbench.splitDown": "splitVertical",
  "workbench.toggleEditorMode": "hex",
  "workbench.closeTab": "close",
  "workbench.closeFile": "close",
  "tools.validate": "validate",
  "tools.beautify": "beautify",
  "tools.minify": "minify",
  "workbench.openSettings": "settings",
  "workbench.openHelp": "help",
  "view.toggleLineChangeIndicators": "lineChanges",
  "workspace.groupRight": "splitHorizontal",
  "workspace.groupDown": "splitVertical",
  "workspace.ungroup": "unsplit",
  "workspace.popOutPane": "popOut",
  "workspace.closePane": "close",
};

export function iconForCommand(commandId: string): IconName {
  return COMMAND_ICONS[commandId] ?? (commandId.startsWith("workspace.") ? "workspace" : "command");
}

interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "aria-pressed" | "title" | "type"> {
  icon: IconName;
  /** Accessible name, also shown in the tooltip. */
  label: string;
  shortcut?: string;
  /** Present for toggles; renders a visible pressed state. */
  pressed?: boolean;
  tooltip?: string;
  /** Short visible text shown under the icon, in addition to the tooltip. */
  caption?: string;
}

/** An icon-only control with an accessible name and an action/shortcut tooltip. */
export function IconButton({ icon, label, shortcut, pressed, tooltip, caption, className, children, ...rest }: IconButtonProps) {
  const title = tooltip ?? label;
  return <button type="button" {...rest}
    className={`icon-button${caption ? " captioned" : ""}${className ? ` ${className}` : ""}`}
    aria-label={label} aria-pressed={pressed} title={shortcut ? `${title} (${shortcut})` : title}>
    <Icon name={icon} />{caption ? <span className="icon-caption" aria-hidden="true">{caption}</span> : null}{children}
  </button>;
}
