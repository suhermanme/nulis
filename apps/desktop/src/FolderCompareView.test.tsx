import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { expect, test, vi } from "vitest";
import { FolderCompareView } from "./FolderCompareView";

test("renders bounded folder differences and skipped-entry feedback", () => {
  render(<FolderCompareView onClose={vi.fn()} result={{
    leftRoot: "/work/before",
    rightRoot: "/work/after",
    entries: [{
      relativePath: "src/main.rs",
      status: "different",
      leftKind: "file",
      rightKind: "file",
      leftByteLen: "10",
      rightByteLen: "12",
    }],
    identicalFiles: 4,
    comparedFiles: 5,
    skippedSymlinks: 2,
    truncated: true,
  }} />);
  expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  expect(screen.getByText(/4 identical files/)).toBeInTheDocument();
  expect(screen.getByText(/2 symlinks skipped/)).toBeInTheDocument();
  expect(screen.getByText(/result limited/)).toBeInTheDocument();
});
