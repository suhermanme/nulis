import { expect, test } from "vitest";
import {
  MAX_BAND_BYTES, cleanBandText, colorChoice, defaultPaper, defaultPrintSettings, marginPreset, normalizePrintSettings,
} from "./printSettings";

test("Letter is the default paper in the United States, A4 elsewhere", () => {
  expect(defaultPaper("en-US")).toBe("letter");
  expect(defaultPaper("en-GB")).toBe("a4");
  expect(defaultPaper("id-ID")).toBe("a4");
  expect(defaultPrintSettings("en-US").setup.paper).toBe("letter");
});

test("stored settings are brought into range and unknown values fall back", () => {
  const settings = normalizePrintSettings({
    setup: {
      paper: "tabloid", orientation: "landscape", fontSize: 99, tabWidth: 0,
      margins: { top: -4, right: 200, bottom: "wide", left: 12.3 },
      longLines: "cut", lineNumbers: "yes", header: { left: "{file}\nnext", center: 7 },
    },
    colors: "neon",
  }, "de-DE");
  expect(settings.setup).toMatchObject({
    paper: "a4", orientation: "landscape", fontSize: 24, tabWidth: 1, longLines: "cut", lineNumbers: true,
    margins: { top: 0, right: 80, bottom: 15, left: 12.5 },
  });
  expect(settings.setup.header).toEqual({ left: "{file}next", center: "", right: "{date}" });
  expect(settings.colors).toBe("print");
  expect(normalizePrintSettings(null, "de-DE")).toEqual(defaultPrintSettings("de-DE"));
});

test("header and footer text is one line within the core's limit", () => {
  expect(cleanBandText("a\tb\u0007c")).toBe("abc");
  const long = cleanBandText("é".repeat(400));
  expect(new TextEncoder().encode(long).length).toBeLessThanOrEqual(MAX_BAND_BYTES);
  expect(long).toBe("é".repeat(128));
});

test("margin presets are recognized only when every side matches", () => {
  expect(marginPreset({ top: 10, right: 10, bottom: 10, left: 10 })).toBe("narrow");
  expect(marginPreset({ top: 25, right: 25, bottom: 25, left: 25 })).toBe("wide");
  expect(marginPreset({ top: 15, right: 15, bottom: 15, left: 20 })).toBe("custom");
});

test("the current theme sends only the colors the print palette reads", () => {
  const theme = {
    id: "t", name: "T", version: 1, kind: "dark" as const, builtIn: true,
    colors: { "editor.background": "#000000", "syntax.keyword": "#ff0000", "ui.panel": "#222222" },
  };
  expect(colorChoice("theme", theme)).toEqual({
    kind: "theme", colors: { "editor.background": "#000000", "syntax.keyword": "#ff0000" },
  });
  expect(colorChoice("blackAndWhite", theme)).toEqual({ kind: "blackAndWhite" });
  expect(colorChoice("print", theme)).toEqual({ kind: "print" });
});
