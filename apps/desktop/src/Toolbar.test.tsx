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
  resize?.(34 * 6);
  expect(visible()).toEqual(["New", "Open", "Save", "Undo", "Settings", "More toolbar actions"]);
  fireEvent.click(screen.getByRole("button", { name: "More toolbar actions" }));
  const menu = screen.getByRole("menu", { name: "More toolbar actions" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Redo", "Split Horizontal"]);
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Redo" }));
  expect(selected).toEqual(["redo"]);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();

  // Narrow: essential file actions remain before anything else.
  resize?.(34 * 4);
  expect(visible()).toEqual(["New", "Open", "Save", "More toolbar actions"]);
  fireEvent.click(screen.getByRole("button", { name: "More toolbar actions" }));
  expect(within(screen.getByRole("menu")).getByRole("menuitem", { name: /Undo/ })).toHaveTextContent("Ctrl+Z");
  expect(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Settings" })).toBeInTheDocument();
});
