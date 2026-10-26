import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, test, vi } from "vitest";
import { remoteImagePolicyOf } from "./contracts";
import { MarkdownPreview, remoteImagesAllowed, resetRemoteImageApprovals } from "./MarkdownPreview";
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

describe("remote images", () => {
  const remote = "https://images.example.com/chart.png";
  const mixed = `![Remote](${remote})\n\n![Local](images/diagram.png)`;

  afterEach(() => { cleanup(); resetRemoteImageApprovals(); });

  test("ask, the default, fetches nothing until the reader asks, then loads every remote image of the document", async () => {
    render(<MarkdownPreview documentId="doc-ask" source={`${mixed}\n\n![Second](https://cdn.example.org/b.png)`} />);
    expect(screen.getByRole("img", { name: "Remote: Remote image from images.example.com" })).toBeInTheDocument();
    expect(document.querySelector(`img[src="${remote}"]`)).toBeNull();
    // A local image is not remote and renders as before.
    expect(await screen.findByRole("img", { name: "Local" })).toHaveAttribute("src", "data:image/png;base64,AQID");

    fireEvent.click(screen.getAllByRole("button", { name: "Load remote images" })[0]);
    expect(screen.getByRole("img", { name: "Remote" })).toHaveAttribute("src", remote);
    expect(screen.getByRole("img", { name: "Second" })).toHaveAttribute("src", "https://cdn.example.org/b.png");
    expect(screen.queryByRole("button", { name: "Load remote images" })).toBeNull();
  });

  test("an approval covers every preview of that document and no other document", () => {
    render(<>
      <MarkdownPreview documentId="doc-a" source={`![A1](${remote})`} remoteImages="ask" />
      <MarkdownPreview documentId="doc-a" source={`![A2](${remote})`} remoteImages="ask" />
      <MarkdownPreview documentId="doc-b" source={`![B](${remote})`} remoteImages="ask" />
    </>);
    fireEvent.click(screen.getAllByRole("button", { name: "Load remote images" })[0]);
    expect(screen.getByRole("img", { name: "A1" })).toHaveAttribute("src", remote);
    expect(screen.getByRole("img", { name: "A2" })).toHaveAttribute("src", remote);
    expect(screen.getByRole("img", { name: "B: Remote image from images.example.com" })).toBeInTheDocument();
    expect(remoteImagesAllowed("ask", "doc-a")).toBe(true);
    expect(remoteImagesAllowed("ask", "doc-b")).toBe(false);
  });

  test("block shows where the image would come from and offers no way to fetch it", async () => {
    render(<MarkdownPreview documentId="doc-block" source={mixed} remoteImages="block" />);
    expect(screen.getByRole("img", { name: "Remote: Remote image from images.example.com, blocked" })).toHaveAttribute("title", remote);
    expect(screen.queryByRole("button", { name: "Load remote images" })).toBeNull();
    expect(await screen.findByRole("img", { name: "Local" })).toBeInTheDocument();
    expect(remoteImagesAllowed("block", "doc-block")).toBe(false);
  });

  test("allow fetches https images directly", () => {
    render(<MarkdownPreview documentId="doc-allow" source={`![Remote](${remote})`} remoteImages="allow" />);
    expect(screen.getByRole("img", { name: "Remote" })).toHaveAttribute("src", remote);
  });

  test("plain http images are blocked under every policy", () => {
    for (const policy of ["block", "ask", "allow"] as const) {
      const { unmount } = render(<MarkdownPreview documentId={`doc-http-${policy}`} source="![Old](http://insecure.example.com/x.png)" remoteImages={policy} />);
      expect(screen.getByRole("img", { name: "Old: Insecure remote image from insecure.example.com, blocked" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Load remote images" })).toBeNull();
      expect(document.querySelector("img")).toBeNull();
      unmount();
    }
  });
});

test("the setting reads block, ask or allow and falls back to ask", () => {
  expect(remoteImagePolicyOf(undefined)).toBe("ask");
  expect(remoteImagePolicyOf({})).toBe("ask");
  expect(remoteImagePolicyOf({ markdown: { preview: { remoteImages: "block" } } })).toBe("block");
  expect(remoteImagePolicyOf({ markdown: { preview: { remoteImages: "allow" } } })).toBe("allow");
  expect(remoteImagePolicyOf({ markdown: { preview: { remoteImages: "always" } } })).toBe("ask");
  expect(remoteImagePolicyOf({ markdown: "yes" })).toBe("ask");
});
