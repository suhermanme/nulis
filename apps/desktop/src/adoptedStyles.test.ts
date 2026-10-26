import { afterEach, beforeEach, expect, test } from "vitest";
import { adoptStyleElements } from "./adoptedStyles";

// jsdom has no constructable stylesheets, so a recorder stands in for them. What the
// tests read is which rules reach the document, which is what the CSP decides.
class RecordingSheet {
  text = "";
  replaceSync(text: string) { this.text = text; }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const adopted = () => (document.adoptedStyleSheets as unknown as RecordingSheet[]).map((sheet) => sheet.text);
let root: HTMLElement;

beforeEach(() => {
  (window as unknown as { CSSStyleSheet: unknown }).CSSStyleSheet = RecordingSheet;
  Object.defineProperty(document, "adoptedStyleSheets", { value: [], writable: true, configurable: true });
  root = document.createElement("div");
  document.body.append(root);
});

afterEach(() => {
  root.remove();
  delete (document as unknown as { adoptedStyleSheets?: unknown }).adoptedStyleSheets;
});

test("style elements written under the root reach the document as adopted sheets", async () => {
  const release = adoptStyleElements(root);
  const style = document.createElement("style");
  root.append(style);
  style.textContent = ".xterm-rows { font-family: monospace; }";
  await flush();
  expect(adopted()).toEqual([".xterm-rows { font-family: monospace; }"]);

  // xterm rewrites its rules when the font or theme changes; the sheet follows.
  style.textContent = ".xterm-rows { font-family: Menlo; }";
  await flush();
  expect(adopted()).toEqual([".xterm-rows { font-family: Menlo; }"]);

  release();
  expect(adopted()).toEqual([]);
});

test("a removed style element takes its sheet with it, and other sheets stay", async () => {
  const foreign = new RecordingSheet();
  foreign.text = "app";
  document.adoptedStyleSheets = [foreign as unknown as CSSStyleSheet];
  const release = adoptStyleElements(root);
  const wrapper = document.createElement("div");
  const style = document.createElement("style");
  style.textContent = "a {}";
  wrapper.append(style);
  root.append(wrapper);
  await flush();
  expect(adopted()).toEqual(["app", "a {}"]);

  wrapper.remove();
  await flush();
  expect(adopted()).toEqual(["app"]);
  release();
  expect(adopted()).toEqual(["app"]);
});

test("a style attribute the CSP refused is applied through the CSSOM", async () => {
  adoptStyleElements(root);
  const span = document.createElement("span");
  span.setAttribute("style", "color: rgb(1, 2, 3);");
  // jsdom applies every attribute, so the refused declaration is stood in for.
  const refused = { length: 0, cssText: "" };
  Object.defineProperty(span, "style", { value: refused });
  root.append(span);
  await flush();
  expect(refused.cssText).toBe("color: rgb(1, 2, 3);");
});

test("a style attribute that already applied is left alone", async () => {
  adoptStyleElements(root);
  const span = document.createElement("span");
  span.setAttribute("style", "color: red;");
  root.append(span);
  await flush();
  expect(span.style.color).toBe("red");
  expect(span.getAttribute("style")).toBe("color: red;");
});
