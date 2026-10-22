import { cleanup, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";
import { PageSetupDialog } from "./PageSetupDialog";
import { defaultPrintSettings } from "./printSettings";

afterEach(cleanup);

test("for Markdown in View mode the preview shows the formatted page, not the source", async () => {
  const preview = vi.fn(() => new Promise<never>(() => undefined));
  const settings = defaultPrintSettings("de-DE");
  render(<PageSetupDialog initial={{ ...settings, setup: { ...settings.setup, orientation: "landscape" } }} preview={preview}
    loadMarkdown={() => Promise.resolve({ documentId: "doc-1", revision: "1", text: "# Architecture\n\nSome *text*." })}
    onSave={() => undefined} onCancel={() => undefined} />);
  const shown = screen.getByLabelText("Preview of the first page");
  expect(await screen.findByRole("heading", { name: "Architecture" })).toBeInTheDocument();
  expect(shown.querySelector(".print-markdown")).not.toBeNull();
  expect(shown.querySelector(".print-markdown-sheet")).toHaveStyle({ width: "297mm", height: "210mm" });
  expect(screen.getByText(/Only paper, orientation, and margins apply/)).toBeInTheDocument();
  // The core still checks the settings.
  await waitFor(() => expect(preview).toHaveBeenCalled());
});
