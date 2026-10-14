import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, expect, test, vi } from "vitest";
import { HexViewport } from "./HexViewport";
import { executeCommand, getHexViewport } from "./ipc";

vi.mock("./ipc", () => ({
  getHexViewport: vi.fn().mockResolvedValue({
    documentId: "document-1",
    revision: "0",
    documentByteLen: "4",
    bytesPerRow: 16,
    rows: [{ startByte: "0", bytes: [0x41, 0x00, 0xff, 0x20] }],
    selections: [{ anchorByte: "0", activeByte: "0" }],
  }),
  executeCommand: vi.fn().mockResolvedValue({
    document: {
      documentId: "document-1",
      name: "bytes.bin",
      byteLen: "4",
      revision: "1",
      encoding: "utf8",
    },
    caret: { byteOffset: "0", highNibble: false },
  }),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

beforeAll(() => {
  class ResizeObserverStub {
    observe() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("renders synchronized hexadecimal and ASCII bytes and dispatches nibble edits", async () => {
  const onDocumentChange = vi.fn();
  render(
    <HexViewport
      document={{
        documentId: "document-1",
        name: "bytes.bin",
        byteLen: "4",
        revision: "0",
        encoding: "utf8",
      }}
      onDocumentChange={onDocumentChange}
      onStatus={() => undefined}
    />,
  );

  const grid = await screen.findByRole("grid", { name: "bytes.bin hexadecimal editor" });
  expect(screen.getByRole("button", { name: "Byte 0 high nibble 4" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Byte 0 ASCII A" })).toBeInTheDocument();

  fireEvent.keyDown(grid, { key: "F" });
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "hex.typeNibble",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { expectedRevision: "0", highNibble: true, nibble: 15, overwrite: true },
  }));
  await waitFor(() => expect(onDocumentChange).toHaveBeenCalledWith(expect.objectContaining({
    revision: "1",
  })));
});

test("keeps the hexadecimal DOM bounded for a ten-gibibyte document", async () => {
  const rows = Array.from({ length: 4096 }, (_, index) => ({
    startByte: String(index * 16),
    bytes: Array.from({ length: 16 }, () => 0),
  }));
  vi.mocked(getHexViewport).mockResolvedValueOnce({
    documentId: "document-1",
    revision: "0",
    documentByteLen: String(10 * 1024 * 1024 * 1024),
    bytesPerRow: 16,
    rows,
    selections: [{ anchorByte: "0", activeByte: "0" }],
    next: { byteOffset: String(rows.length * 16) },
  });
  const { container } = render(
    <HexViewport
      document={{
        documentId: "document-1",
        name: "huge.bin",
        byteLen: String(10 * 1024 * 1024 * 1024),
        revision: "0",
        encoding: "utf8",
      }}
      onDocumentChange={() => undefined}
      onStatus={() => undefined}
    />,
  );

  await screen.findByRole("grid", { name: "huge.bin hexadecimal editor" });
  await waitFor(() => expect(container.querySelectorAll(".hex-row").length).toBeGreaterThan(0));
  expect(container.querySelectorAll(".hex-row").length).toBeLessThanOrEqual(40);
  expect(container.querySelector(".hex-virtual-rows")).toHaveStyle({
    height: `${rows.length * 24}px`,
  });
});
