/**
 * Size rules for the integrated terminal panel.
 *
 * The panel shares the editor column, so its height is bounded from both ends: it
 * has to stay tall enough to show a prompt with a few lines above it, and it has to
 * leave enough of the column to the editor that editing stays possible. Pointer
 * drags, keyboard steps, restored sessions, and a shrinking window all run through
 * these helpers, so no path can land outside the bounds.
 */
export const TERMINAL_DEFAULT_HEIGHT = 230;
/** Header, a prompt row, and four rows of output — the panel's smallest useful height. */
export const TERMINAL_MIN_HEIGHT = 120;
/** The editor keeps this much of the column, so a terminal drag cannot seal it away. */
export const TERMINAL_EDITOR_MIN_HEIGHT = 160;
/** One press of the divider's arrow keys. */
export const TERMINAL_HEIGHT_STEP = 24;
/** Ceiling for a stored height when the available column is not measured yet. */
export const TERMINAL_HEIGHT_CEILING = 1000;

const HEIGHT_KEY = "nulis.terminalHeight.v1";

/** The tallest panel the column can carry: everything above the editor's reserved strip. */
export function terminalMaxHeight(available: number): number {
  return available > 0
    ? Math.max(TERMINAL_MIN_HEIGHT, available - TERMINAL_EDITOR_MIN_HEIGHT)
    : TERMINAL_HEIGHT_CEILING;
}

export function clampTerminalHeight(height: number, available: number): number {
  if (!Number.isFinite(height)) return TERMINAL_DEFAULT_HEIGHT;
  return Math.round(Math.min(Math.max(height, TERMINAL_MIN_HEIGHT), terminalMaxHeight(available)));
}

/**
 * The height a drag asks for. The divider is the panel's top edge, so moving the
 * pointer up lengthens the panel and moving it down shortens it.
 */
export function terminalHeightFromDrag(
  drag: { startHeight: number; startY: number },
  pointerY: number,
  available: number,
): number {
  return clampTerminalHeight(drag.startHeight + (drag.startY - pointerY), available);
}

/** The remembered height, clamped in case the window shrank since it was saved. */
export function loadTerminalHeight(): number {
  try {
    const stored = Number(window.localStorage.getItem(HEIGHT_KEY));
    return Number.isFinite(stored) && stored > 0
      ? clampTerminalHeight(stored, 0)
      : TERMINAL_DEFAULT_HEIGHT;
  } catch {
    return TERMINAL_DEFAULT_HEIGHT;
  }
}

export function saveTerminalHeight(height: number): void {
  try {
    // Available height 0 means "not measured", which clamps to the storage ceiling.
    window.localStorage.setItem(HEIGHT_KEY, String(clampTerminalHeight(height, 0)));
  } catch {
    // The panel keeps the height for this session when storage is unavailable.
  }
}
