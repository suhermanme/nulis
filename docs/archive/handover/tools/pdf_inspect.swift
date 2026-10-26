// Prints a PDF's page count, size, title, first and last text lines, and
// outline, and optionally renders page 1 to a PNG (M37–M38 export checks).
// swiftc -O -o /tmp/pdf_inspect docs/handover/tools/pdf_inspect.swift && /tmp/pdf_inspect file.pdf [page1.png]

import PDFKit
import AppKit
let url = URL(fileURLWithPath: CommandLine.arguments[1])
guard let doc = PDFDocument(url: url) else { print("CANNOT OPEN"); exit(1) }
let mm = 25.4 / 72.0
let page = doc.page(at: 0)!
let box = page.bounds(for: .mediaBox)
print("pages:", doc.pageCount, String(format: "size %.1f x %.1f mm", box.width * mm, box.height * mm))
print("title:", doc.documentAttributes?[PDFDocumentAttribute.titleAttribute] ?? "-", "| producer:", doc.documentAttributes?[PDFDocumentAttribute.producerAttribute] ?? "-")
let lines = (page.string ?? "").split(separator: "\n", omittingEmptySubsequences: false)
for line in lines.prefix(4) { print("  |" + line) }
print("  ...")
for line in lines.suffix(2) { print("  |" + line) }
if let outline = doc.outlineRoot { print("outline items:", outline.numberOfChildren) }
if CommandLine.arguments.count > 2 {
  let image = page.thumbnail(of: NSSize(width: box.width * 2, height: box.height * 2), for: .mediaBox)
  let rep = NSBitmapImageRep(data: image.tiffRepresentation!)!
  try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
}
