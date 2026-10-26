import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const windowReady = vi.fn(() => Promise.resolve());
vi.mock("./ipc", () => ({ windowReady: () => windowReady(), executeCommand: vi.fn() }));

const { useShowWhenReady } = await import("./windowTheme");

describe("useShowWhenReady", () => {
  afterEach(() => { windowReady.mockClear(); vi.restoreAllMocks(); });

  it("tells the shell without waiting for an animation frame, which a hidden window never gets", () => {
    const frame = vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 0);
    const { rerender } = renderHook(({ ready }) => useShowWhenReady(ready), { initialProps: { ready: false } });
    expect(windowReady).not.toHaveBeenCalled();
    rerender({ ready: true });
    expect(windowReady).toHaveBeenCalledTimes(1);
    expect(frame).not.toHaveBeenCalled();
  });

  it("tells the shell only once", () => {
    const { rerender } = renderHook(({ ready }) => useShowWhenReady(ready), { initialProps: { ready: true } });
    rerender({ ready: false });
    rerender({ ready: true });
    expect(windowReady).toHaveBeenCalledTimes(1);
  });
});
