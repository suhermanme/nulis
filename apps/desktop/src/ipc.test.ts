import { expect, it, vi } from "vitest";
import { commandError, executeCommand } from "./ipc";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

it("turns a failed command into an Error with the core's message", async () => {
  invoke.mockRejectedValueOnce({ kind: "failed", detail: "invalid regular expression: lookbehind is not supported" });
  await expect(executeCommand({ id: "search.validatePattern", context: { windowId: "main" }, arguments: {} }))
    .rejects.toThrow("invalid regular expression: lookbehind is not supported");
});

it("names other kinds of command failure", () => {
  expect((commandError({ kind: "unknown", detail: "x.y" }) as Error).message).toBe("Unknown command: x.y");
  expect((commandError("not allowed") as Error).message).toBe("not allowed");
  const error = new Error("kept");
  expect(commandError(error)).toBe(error);
});
