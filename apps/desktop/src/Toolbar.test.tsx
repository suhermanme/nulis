import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, expect, test, vi } from "vitest";
import { COMMAND_ICONS, ICON_NAMES, Icon, IconButton } from "./icons";
import { OverflowToolbar, type ToolbarItem } from "./Toolbar";

let resize: ((width: number) => void) | undefined;

beforeAll(() => {
  class ResizeObserverStub {
    constructor(callback: ResizeObserverCallback) {
      resize = (width) => act(() => callback([{ contentRect: { width } } as ResizeObserverEntry], this as unknown as ResizeObserver));
    }

    observe() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
});

afterEach(cleanup);

test("every icon shares one grid, stroke weight, and themable color", () => {
  const required = [
    "new", "open", "close", "save", "saveAs", "undo", "redo", "wrap", "column", "hex", "find", "previous", "next",
    "replace", "highlight", "goTo", "lineChanges", "map", "preview", "splitHorizontal", "splitVertical", "files", "settings",
  ];
  expect(ICON_NAMES).toEqual(expect.arrayContaining(required));
  for (const name of ICON_NAMES) {
    const { container } = render(<Icon name={name} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    expect(svg).toHaveAttribute("stroke", "currentColor");
    expect(svg).toHaveAttribute("stroke-width", "1.6");
    expect(svg).toHaveAttribute("width", "20");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(svg?.children.length).toBeGreaterThan(0);
    // No bitmap, external reference, or runtime download.
    expect(svg?.querySelector("image, use, foreignObject")).toBeNull();
    cleanup();
  }
  for (const icon of Object.values(COMMAND_ICONS)) expect(ICON_NAMES).toContain(icon);
});

test("an icon-only control exposes its name, shortcut tooltip, and pressed and disabled states", () => {
  const onClick = vi.fn();
  const { rerender } = render(<IconButton icon="wrap" label="Wrap" tooltip="Word wrap" shortcut="Alt+Z" pressed={false} onClick={onClick} />);
  const button = screen.getByRole("button", { name: "Wrap" });
  expect(button).toHaveAttribute("title", "Word wrap (Alt+Z)");
  expect(button).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(button);
  expect(onClick).toHaveBeenCalledOnce();
  rerender(<IconButton icon="wrap" label="Wrap" pressed disabled onClick={onClick} />);
  expect(button).toHaveAttribute("aria-pressed", "true");
  expect(button).toBeDisabled();
  // A plain action carries no toggle state.
  rerender(<IconButton icon="save" label="Save" onClick={onClick} />);
  expect(screen.getByRole("button", { name: "Save" })).not.toHaveAttribute("aria-pressed");
});

test("low-priority controls move to an overflow menu while essential actions stay reachable", () => {
  const selected: string[] = [];
  const items: ToolbarItem[] = [
    ["file.new", "New", "new", true], ["file.open", "Open", "open", true], ["file.save", "Save", "save", true],
    ["undo", "Undo", "undo", false], ["redo", "Redo", "redo", false], ["split", "Split Horizontal", "splitHorizontal", false],
    ["settings", "Settings", "settings", true],
  ].map(([id, title, icon, essential]) => ({
    id: String(id), label: String(title), title: String(title), icon: icon as ToolbarItem["icon"],
    essential: Boolean(essential), shortcut: id === "undo" ? "Ctrl+Z" : undefined,
    onSelect: () => selected.push(String(id)),
  }));
  render(<OverflowToolbar label="Main" items={items} />);
  const toolbar = screen.getByRole("toolbar", { name: "Main" });
  const visible = () => within(toolbar).getAllByRole("button").map((button) => button.getAttribute("aria-label"));

  // Wide: everything fits and there is no overflow control.
  resize?.(600);
  expect(visible()).toEqual(["New", "Open", "Save", "Undo", "Redo", "Split Horizontal", "Settings"]);

  // Medium: only non-essential controls leave, in their original order.
  resize?.(60 * 5);
  expect(visible()).toEqual(["New", "Open", "Save", "Settings", "More toolbar actions"]);
  fireEvent.click(screen.getByRole("button", { name: "More toolbar actions" }));
  const menu = screen.getByRole("menu", { name: "More toolbar actions" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.getAttribute("aria-label"))).toEqual(["Undo", "Redo", "Split Horizontal"]);
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Redo" }));
  expect(selected).toEqual(["redo"]);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();

  // Narrow: essential file actions remain before anything else.
  resize?.(60 * 3);
  expect(visible()).toEqual(["New", "Open", "More toolbar actions"]);
  fireEvent.click(screen.getByRole("button", { name: "More toolbar actions" }));
  expect(within(screen.getByRole("menu")).getByRole("menuitem", { name: /Undo/ })).toHaveTextContent("Ctrl+Z");
  expect(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Settings" })).toBeInTheDocument();
});

test("the overflow control shows its word, and its menu names each control and offers the toolbar editor", () => {
  const onCustomize = vi.fn();
  const items: ToolbarItem[] = [
    { id: "file.new", label: "New file", title: "New File", caption: "New", icon: "new", essential: true, onSelect: () => undefined },
    { id: "nulis.settings", label: "Open settings", title: "Settings", caption: "Settings", icon: "settings", essential: true, pressed: true, onSelect: () => undefined },
    { id: "nulis.compare", label: "Compare open documents", title: "Compare Documents", caption: "Compare", icon: "compare", shortcut: "Ctrl+Shift+C", onSelect: () => undefined },
  ];
  render(<OverflowToolbar label="Main" items={items} onCustomize={onCustomize} />);
  // Three labeled controls in the room of two: the last non-essential one leaves.
  resize?.(60 * 2);
  const more = screen.getByRole("button", { name: "More toolbar actions" });
  expect(more).toHaveTextContent("More");
  expect(more.querySelector("[data-icon='chevronRight']")).not.toBeNull();
  expect(more).toHaveAttribute("title", "More — Show extra toolbar items");
  fireEvent.click(more);
  const menu = screen.getByRole("menu", { name: "More toolbar actions" });
  const row = within(menu).getByRole("menuitem", { name: "Compare open documents" });
  expect(row).toHaveTextContent("Compare");
  expect(row).toHaveAttribute("title", "Compare Documents — Compare (Ctrl+Shift+C)");
  expect(row).toHaveTextContent("Ctrl+Shift+C");
  // A pressed control reports its state as a checkable menu row, not as a plain menuitem.
  expect(within(menu).getByRole("menuitemcheckbox", { name: "Open settings" })).toHaveAttribute("aria-checked", "true");
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Customize Toolbar…" }));
  expect(onCustomize).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});
