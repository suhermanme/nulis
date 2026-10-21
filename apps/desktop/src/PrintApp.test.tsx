import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";

const printThisWindow = vi.fn(() => Promise.resolve());
vi.mock("./ipc", () => ({ printThisWindow: () => printThisWindow() }));
vi.mock("./windowTheme", () => ({ useWindowTheme: () => ({}) }));

const { PrintApp, pageGeometry } = await import("./PrintApp");

afterEach(() => { cleanup(); printThisWindow.mockClear(); });

test("calibration geometry follows paper and orientation", () => {
  const a4 = pageGeometry("a4", "portrait");
  expect(a4).toMatchObject({ widthMm: 210, heightMm: 297 });
  // (297 − 30) mm = 756.9 pt; 68 lines of 11 pt, minus the header and footer bands.
  expect(a4.linesPerPage).toBe(66);
  // (210 − 30) mm = 510.2 pt; 5.4 pt per character.
  expect(a4.columns).toBe(94);
  const landscape = pageGeometry("a4", "landscape");
  expect(landscape).toMatchObject({ widthMm: 297, heightMm: 210 });
  expect(landscape.linesPerPage).toBeLessThan(a4.linesPerPage);
  expect(pageGeometry("letter", "portrait").widthMm).toBeCloseTo(215.9);
});

test("Print Preview shows three calibration pages and prints through the shell", () => {
  render(<PrintApp />);
  expect(screen.getAllByRole("region", { name: /^Page \d of 3$/ })).toHaveLength(3);
  fireEvent.change(screen.getByLabelText("Paper"), { target: { value: "a4" } });
  const first = screen.getByRole("region", { name: "Page 1 of 3" });
  expect(first).toHaveStyle({ width: "210mm", height: "297mm" });
  expect(first).toHaveTextContent("LAST LINE OF PAGE 1");
  fireEvent.change(screen.getByLabelText("Orientation"), { target: { value: "landscape" } });
  expect(screen.getByRole("region", { name: "Page 1 of 3" })).toHaveStyle({ width: "297mm", height: "210mm" });
  fireEvent.click(screen.getByRole("button", { name: "Print…" }));
  expect(printThisWindow).toHaveBeenCalledTimes(1);
});
