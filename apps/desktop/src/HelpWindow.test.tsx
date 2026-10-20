import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test } from "vitest";
import userGuide from "../../../docs/USER_GUIDE.md?raw";
import { HELP_SECTIONS, HelpWindow } from "./HelpWindow";
import { REGEX_HELP } from "./FindReplacePanel";

afterEach(cleanup);

test("every chapter of the user guide is a Help section with its own icon", () => {
  const chapters = [...userGuide.matchAll(/^## (.+)$/gm)].map((match) => match[1]!.trim());
  const titles = HELP_SECTIONS.map((section) => section.title);
  expect(titles.filter((title) => title !== "Building Plugins")).toEqual(chapters);
  // "document" is the fallback icon; only the Files chapter uses it on purpose.
  expect(HELP_SECTIONS.filter((section) => section.icon === "document").map((section) => section.title)).toEqual(["Files"]);
});

test("Help search finds the features added in M27–M30", () => {
  render(<HelpWindow mode="inline" onClose={() => undefined} />);
  const search = screen.getByRole("searchbox", { name: "Search help" });
  const sections = (text: string) => {
    fireEvent.change(search, { target: { value: text } });
    return within(screen.getByRole("tablist")).getAllByRole("tab").map((tab) => tab.textContent);
  };
  expect(sections("Quick Open")).toEqual(expect.arrayContaining(["Files", "Keyboard Shortcuts", "Limits"]));
  expect(sections("Paste from History")).toEqual(expect.arrayContaining(["Editing", "Limits"]));
  expect(sections("Convert Line Endings")).toEqual(["Languages and Encodings"]);
  expect(sections("Show in Places")).toEqual(expect.arrayContaining(["Files", "File View and Remote"]));
  expect(sections("batik")).toEqual(expect.arrayContaining(["About and License"]));
});

test("Help opens at a topic, such as the regex section the Find window links to", () => {
  const scrolled: string[] = [];
  HTMLElement.prototype.scrollIntoView = function scrollIntoView(this: HTMLElement) { scrolled.push(this.textContent ?? ""); };
  render(<HelpWindow mode="inline" onClose={() => undefined}
    topic={{ id: 1, section: REGEX_HELP.section, heading: REGEX_HELP.heading }} />);
  expect(screen.getByRole("tab", { name: "Search" })).toHaveAttribute("aria-selected", "true");
  expect(scrolled).toEqual(["Regular expressions"]);
});

test("Help search finds the features added in M31–M34", () => {
  render(<HelpWindow mode="inline" onClose={() => undefined} />);
  const search = screen.getByRole("searchbox", { name: "Search help" });
  const sections = (text: string) => {
    fireEvent.change(search, { target: { value: text } });
    return within(screen.getByRole("tablist")).queryAllByRole("tab").map((tab) => tab.textContent);
  };
  expect(sections("Search in column")).toContain("Search");
  expect(sections("lookbehind")).toContain("Search");
  expect(sections("Validate YAML")).toContain("Tools");
  expect(sections("<integer>")).toContain("Tools");
  expect(sections("Property List")).toContain("Languages and Encodings");
  expect(sections("Paper Current")).toContain("Settings and Themes");
});
