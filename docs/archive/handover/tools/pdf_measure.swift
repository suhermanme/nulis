// Checks a printed or exported PDF: page count, page size, and where the
// calibration header, last line, and footer sit (M36 print checks).
// swiftc -O -o /tmp/pdf_measure docs/handover/tools/pdf_measure.swift && /tmp/pdf_measure file.pdf

import PDFKit
let url = URL(fileURLWithPath: CommandLine.arguments[1])
guard let doc = PDFDocument(url: url) else { print("cannot open"); exit(1) }
let mm = 25.4 / 72.0
print("pages:", doc.pageCount)
for i in 0..<doc.pageCount {
  let page = doc.page(at: i)!
  let box = page.bounds(for: .mediaBox)
  print(String(format: "page %d: %.1f x %.1f mm", i + 1, box.width * mm, box.height * mm))
  let text = page.string ?? ""
  let lines = text.split(separator: "\n")
  print("  first:", lines.first ?? "", "| last:", lines.last ?? "", "| lines:", lines.count)
  for needle in ["Nulis print calibration", "LAST LINE OF PAGE \(i + 1)", "Page \(i + 1) of 3"] {
    if let sel = doc.findString(needle, withOptions: []).first(where: { $0.pages.first == page }) {
      let b = sel.bounds(for: page)
      print(String(format: "  %@: left %.1f mm, top %.1f mm, bottom %.1f mm", needle, b.minX * mm, (box.height - b.maxY) * mm, b.minY * mm))
    } else { print("  missing:", needle) }
  }
}
if let attrs = doc.documentAttributes { print("creator:", attrs[PDFDocumentAttribute.creatorAttribute] ?? "-") }
