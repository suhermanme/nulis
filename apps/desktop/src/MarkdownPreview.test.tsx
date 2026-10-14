import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { expect, test, vi } from "vitest";
import { MarkdownPreview } from "./MarkdownPreview";
import { getMarkdownPreviewImage } from "./ipc";

vi.mock("./ipc", () => ({
  getMarkdownPreviewImage: vi.fn().mockResolvedValue("data:image/png;base64,AQID"),
}));

test("renders a local Markdown image through the bounded native query", async () => {
  render(<MarkdownPreview documentId="document-1" source="![Diagram](images/diagram.png)" />);
  const image = await screen.findByRole("img", { name: "Diagram" });
  expect(image).toHaveAttribute("src", "data:image/png;base64,AQID");
  expect(getMarkdownPreviewImage).toHaveBeenCalledWith("document-1", "images/diagram.png");
});
