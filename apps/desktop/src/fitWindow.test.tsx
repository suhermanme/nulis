import { useRef } from "react";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useFitToContent } from "./fitWindow";

let resize: () => void = () => undefined;
let bottom = 400;

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { resize = callback; }
    observe() { /* the test calls `resize` itself */ }
    disconnect() { /* nothing to release */ }
  });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { callback(0); return 1; });
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const top = 0;
    const height = this.classList.contains("content") ? bottom : 0;
    return { top, bottom: top + height, height, left: 0, right: 0, width: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function Window({ fit }: { fit(height: number): void }) {
  const ref = useRef<HTMLDivElement>(null);
  useFitToContent(ref, ".page", fit);
  return <div ref={ref}><main className="page"><nav>tabs</nav><section className="content">content</section></main></div>;
}

it("asks for the content's height on open and whenever it changes", () => {
  const fit = vi.fn();
  render(<Window fit={fit} />);
  expect(fit).toHaveBeenLastCalledWith(400);

  bottom = 520;
  act(() => resize());
  expect(fit).toHaveBeenLastCalledWith(520);
  expect(fit).toHaveBeenCalledTimes(2);
});

it("leaves the window alone while the content height is unchanged, so a manual resize is kept", () => {
  const fit = vi.fn();
  bottom = 400;
  render(<Window fit={fit} />);
  act(() => resize());
  act(() => resize());
  expect(fit).toHaveBeenCalledTimes(1);
});
