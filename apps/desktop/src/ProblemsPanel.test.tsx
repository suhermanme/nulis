import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";
import type { DocumentProblem, DocumentSummary } from "./contracts";
import { problemCounts, ProblemsPanel, worstSeverity } from "./ProblemsPanel";

afterEach(cleanup);

const document: DocumentSummary = {
  documentId: "document-1", name: "main.rs", byteLen: "42", revision: "3", encoding: "utf8",
};

function problem(overrides: Partial<DocumentProblem> = {}): DocumentProblem {
  return {
    startByte: "10", endByte: "14", line: 2, severity: "error",
    message: "mismatched types", source: "rust-analyzer", ...overrides,
  };
}

test("counts problems by severity and reports the worst one", () => {
  const problems = [
    problem({ severity: "hint", message: "unused import" }),
    problem({ severity: "error" }),
    problem({ severity: "warning" }),
    problem({ severity: "error" }),
  ];
  expect(problemCounts(problems)).toEqual([
    { severity: "error", count: 2 },
    { severity: "warning", count: 1 },
    { severity: "hint", count: 1 },
  ]);
  expect(worstSeverity(problems)).toBe("error");
  expect(worstSeverity([problem({ severity: "hint" })])).toBe("hint");
  expect(worstSeverity([])).toBeNull();
});

test("lists problems and puts the caret on the one chosen", () => {
  const onSelect = vi.fn();
  const problems = [
    problem(),
    problem({ startByte: "20", endByte: "25", line: 4, severity: "warning", message: "unused variable `x`" }),
  ];
  render(<ProblemsPanel document={document} problems={problems} server="rust-analyzer"
    onSelect={onSelect} onClose={vi.fn()} />);

  expect(screen.getByRole("region", { name: "Problems in main.rs" })).toBeInTheDocument();
  expect(screen.getByText("1 error")).toBeInTheDocument();
  expect(screen.getByText("1 warning")).toBeInTheDocument();
  expect(screen.getByText("rust-analyzer")).toBeInTheDocument();

  const rows = screen.getAllByRole("button", { name: /^(Error|Warning) on line/ });
  expect(rows).toHaveLength(2);
  expect(rows[1]).toHaveAccessibleName("Warning on line 4: unused variable `x`");
  fireEvent.click(rows[1]);
  expect(onSelect).toHaveBeenCalledWith(problems[1]);
});

test("explains an empty list with and without a server", () => {
  const { rerender } = render(<ProblemsPanel document={document} problems={[]} server="rust-analyzer"
    onSelect={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByText("No problems")).toBeInTheDocument();
  expect(screen.getByText("rust-analyzer reports no problems in this document.")).toBeInTheDocument();

  rerender(<ProblemsPanel document={document} problems={[]} onSelect={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByText(/No language server is set up/)).toBeInTheDocument();
});

test("shows why problems could not be read and can open Settings", () => {
  const onOpenSettings = vi.fn();
  const onClose = vi.fn();
  render(<ProblemsPanel document={document} problems={[problem()]} error="rust-analyzer exited"
    onSelect={vi.fn()} onClose={onClose} onOpenSettings={onOpenSettings} />);

  expect(screen.getByRole("alert")).toHaveTextContent("rust-analyzer exited");
  fireEvent.click(screen.getByRole("button", { name: "Language Servers…" }));
  expect(onOpenSettings).toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Close problems" }));
  expect(onClose).toHaveBeenCalled();
});
