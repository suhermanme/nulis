import { expect, test } from "vitest";
import { DEFAULT_VIEW_OPTIONS, normalizeViewOptions, stepZoom } from "./preferences";

test("view options keep known values and fall back for the rest", () => {
  expect(normalizeViewOptions(null)).toEqual(DEFAULT_VIEW_OPTIONS);
  expect(normalizeViewOptions({ wordWrap: true, zoom: 125, markerColumn: 120, lineNumbers: "no" })).toEqual({
    ...DEFAULT_VIEW_OPTIONS, wordWrap: true, zoom: 125, markerColumn: 120,
  });
  // A zoom that is not a level, and a column out of range, are corrected.
  expect(normalizeViewOptions({ zoom: 133, markerColumn: 9000 })).toMatchObject({ zoom: 100, markerColumn: 500 });
});

test("zoom steps through fixed levels and stops at the ends", () => {
  expect(stepZoom(100, 1)).toBe(110);
  expect(stepZoom(100, -1)).toBe(90);
  expect(stepZoom(300, 1)).toBe(300);
  expect(stepZoom(50, -1)).toBe(50);
  expect(stepZoom(175, 0)).toBe(100);
});
