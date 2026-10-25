import { expect, test } from "vitest";
import type { EditorPreferences, ThemeRecord } from "./contracts";
import {
  BUILT_IN_THEMES,
  DEFAULT_EDITOR_PREFERENCES,
  DEFAULT_VIEW_OPTIONS,
  normalizeEditorPreferences,
  normalizeViewOptions,
  preferenceWindowTheme,
  stepZoom,
} from "./preferences";

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

test("stored preferences written before the theme-colors switch normalize without it", () => {
  const user: ThemeRecord = {
    schema: "nulis.theme/1",
    id: "test.theme",
    name: "Test Theme",
    version: 1,
    kind: "dark",
    colors: { "editor.background": "#101820" },
    builtIn: false,
    custom: { maxSaturation: 620, minLuma: 200, maxLuma: 920, minChroma: 30 },
    uiTokens: ["ui.background"],
  };
  // Older localStorage and older customization documents carry applyUiTheme. Theme colors paint
  // every window now, so normalization drops the key and nothing reads it to decide what to paint.
  const stored = { theme: "test.theme", applyUiTheme: true };
  const normalized = normalizeEditorPreferences(stored, [user]);
  expect(normalized).not.toHaveProperty("applyUiTheme");
  expect(normalized.theme).toBe("test.theme");
  expect(normalizeEditorPreferences(null, [user])).not.toHaveProperty("applyUiTheme");
});

test("a user theme's window payload carries its floors and tokens", () => {
  const user: ThemeRecord = {
    schema: "nulis.theme/1",
    id: "test.theme",
    name: "Test Theme",
    version: 1,
    kind: "dark",
    colors: { "editor.background": "#101820" },
    builtIn: false,
    custom: { maxSaturation: 620, minLuma: 200, maxLuma: 920, minChroma: 30 },
    uiTokens: ["ui.background"],
  };
  const preferences: EditorPreferences = {
    theme: "test.theme", fontFamily: "Mono", fontSize: 14, lineHeight: 1.6, fontLigatures: true,
  };
  const payload = preferenceWindowTheme(preferences, [user]);
  expect(payload.theme).toBe("test.theme");
  expect(payload.baseTheme).toBe("test.theme");
  expect(payload.customTheme).toEqual({ maxSaturation: 620, minLuma: 200, maxLuma: 920, minChroma: 30 });
  expect(payload.uiTokens).toEqual(["ui.background"]);
  // The exported color is the authored one clamped to the floors, never the raw pick.
  expect(payload.colors["editor.background"]).toMatch(/^#[0-9a-f]{6}$/);
  expect(payload.colors["editor.background"]).not.toBe("#101820");
});

test("a built-in theme's payload keeps its own palette and carries no custom floors", () => {
  const builtIn = BUILT_IN_THEMES[0];
  const preferences: EditorPreferences = {
    theme: builtIn.id, fontFamily: "Mono", fontSize: 14, lineHeight: 1.6, fontLigatures: true,
  };
  const payload = preferenceWindowTheme(preferences, BUILT_IN_THEMES);
  expect(payload.customTheme).toBeUndefined();
  expect(payload.uiTokens).toBeUndefined();
  expect(payload.colors).toEqual(builtIn.colors);
});
