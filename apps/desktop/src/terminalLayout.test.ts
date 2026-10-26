import { expect, test } from "vitest";
import {
  clampTerminalHeight, loadTerminalHeight, saveTerminalHeight, TERMINAL_DEFAULT_HEIGHT, TERMINAL_EDITOR_MIN_HEIGHT,
  TERMINAL_HEIGHT_CEILING, TERMINAL_MIN_HEIGHT, terminalHeightFromDrag, terminalMaxHeight,
} from "./terminalLayout";

const HEIGHT_KEY = "nulis.terminalHeight.v1";

test("the panel leaves the editor its strip and never drops under a prompt", () => {
  expect(terminalMaxHeight(800)).toBe(800 - TERMINAL_EDITOR_MIN_HEIGHT);
  expect(clampTerminalHeight(400, 800)).toBe(400);
  expect(clampTerminalHeight(20, 800)).toBe(TERMINAL_MIN_HEIGHT);
  expect(clampTerminalHeight(799, 800)).toBe(800 - TERMINAL_EDITOR_MIN_HEIGHT);
});

test("a column shorter than both minimums gives the panel its own minimum", () => {
  // The panel cannot both keep a usable height and leave the strip; it keeps its
  // minimum and the editor area scrolls, rather than the bounds contradicting.
  expect(terminalMaxHeight(200)).toBe(TERMINAL_MIN_HEIGHT);
  expect(clampTerminalHeight(180, 200)).toBe(TERMINAL_MIN_HEIGHT);
  expect(clampTerminalHeight(60, 200)).toBe(TERMINAL_MIN_HEIGHT);
});

test("an unmeasured column clamps to the storage ceiling", () => {
  // Height gets saved before any drag knows the column's height.
  expect(terminalMaxHeight(0)).toBe(TERMINAL_HEIGHT_CEILING);
  expect(clampTerminalHeight(4000, 0)).toBe(TERMINAL_HEIGHT_CEILING);
  expect(clampTerminalHeight(230, 0)).toBe(230);
});

test("dragging the divider up lengthens the panel and dragging down shortens it", () => {
  const drag = { startHeight: 300, startY: 500 };
  expect(terminalHeightFromDrag(drag, 460, 800)).toBe(340);
  expect(terminalHeightFromDrag(drag, 540, 800)).toBe(260);
  expect(terminalHeightFromDrag(drag, 500, 800)).toBe(300);
  // A pointer that travels past either bound stops at the bound the column allows.
  expect(terminalHeightFromDrag(drag, 0, 800)).toBe(800 - TERMINAL_EDITOR_MIN_HEIGHT);
  expect(terminalHeightFromDrag(drag, 700, 800)).toBe(TERMINAL_MIN_HEIGHT);
});

test("a window resize pulls a saved height back inside the column", () => {
  const wide = clampTerminalHeight(640, 800);
  expect(wide).toBe(640);
  expect(clampTerminalHeight(wide, 500)).toBe(500 - TERMINAL_EDITOR_MIN_HEIGHT);
});

test("the height survives restarts and garbage falls back to the default", () => {
  window.localStorage.setItem(HEIGHT_KEY, "340");
  expect(loadTerminalHeight()).toBe(340);

  saveTerminalHeight(199);
  expect(window.localStorage.getItem(HEIGHT_KEY)).toBe("199");

  // Saving a height outside the bounds stores the bound, not the raw number.
  saveTerminalHeight(20);
  expect(window.localStorage.getItem(HEIGHT_KEY)).toBe(String(TERMINAL_MIN_HEIGHT));
  saveTerminalHeight(5000);
  expect(window.localStorage.getItem(HEIGHT_KEY)).toBe(String(TERMINAL_HEIGHT_CEILING));

  window.localStorage.setItem(HEIGHT_KEY, "not a height");
  expect(loadTerminalHeight()).toBe(TERMINAL_DEFAULT_HEIGHT);
  window.localStorage.setItem(HEIGHT_KEY, "-40");
  expect(loadTerminalHeight()).toBe(TERMINAL_DEFAULT_HEIGHT);
  window.localStorage.removeItem(HEIGHT_KEY);
  expect(loadTerminalHeight()).toBe(TERMINAL_DEFAULT_HEIGHT);
});

test("storage that throws does not take the panel down with it", () => {
  const storage = window.localStorage;
  const original = { getItem: storage.getItem, setItem: storage.setItem };
  storage.getItem = () => { throw new Error("storage disabled"); };
  storage.setItem = () => { throw new Error("storage disabled"); };
  try {
    expect(loadTerminalHeight()).toBe(TERMINAL_DEFAULT_HEIGHT);
    expect(() => saveTerminalHeight(300)).not.toThrow();
  } finally {
    storage.getItem = original.getItem;
    storage.setItem = original.setItem;
  }
});
