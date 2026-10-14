import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";
import { CompareView } from "./CompareView";
import type { DocumentSummary } from "./contracts";
import { executeCommand } from "./ipc";

vi.mock("./ipc", () => ({ executeCommand: vi.fn() }));

const left: DocumentSummary = {
  documentId: "left", name: "before.txt", byteLen: "8", revision: "2", encoding: "utf8",
};
const right: DocumentSummary = {
  documentId: "right", name: "after.txt", byteLen: "9", revision: "4", encoding: "utf8",
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("loads a bounded comparison and applies one hunk in either direction", async () => {
  vi.mocked(executeCommand)
    .mockResolvedValueOnce({
      mode: "text",
      changes: [{
        kind: "replace",
        left: { startByte: "2", endByte: "5" },
        right: { startByte: "2", endByte: "6" },
        leftStartLine: "2", rightStartLine: "2", leftLineCount: 1, rightLineCount: 1,
      }],
      nextLeftByte: null, nextRightByte: null, nextLeftLine: null, nextRightLine: null,
      truncated: false,
    })
    .mockResolvedValueOnce({ ...right, revision: "5", byteLen: "8" });
  const onDocumentChange = vi.fn();
  render(<CompareView left={left} right={right} onDocumentChange={onDocumentChange} onStatus={vi.fn()} />);

  expect(await screen.findByText("bytes 2–5")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith(expect.objectContaining({
    id: "compare.documents",
    arguments: expect.objectContaining({
      leftDocumentId: "left", rightDocumentId: "right", mode: "text", maxBytes: 262144,
    }),
  }));

  fireEvent.click(screen.getByRole("button", { name: "Apply change to after.txt" }));
  await waitFor(() => expect(executeCommand).toHaveBeenLastCalledWith({
    id: "compare.applyChange",
    context: { windowId: "main", documentId: "right" },
    arguments: {
      sourceDocumentId: "left", targetDocumentId: "right", expectedTargetRevision: "4",
      sourceStart: "2", sourceEnd: "5", targetStart: "2", targetEnd: "6",
    },
  }));
  expect(onDocumentChange).toHaveBeenCalledWith(expect.objectContaining({ revision: "5" }));
});
