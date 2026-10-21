//! End-to-end export of small documents, checking the PDF's structure.

use nulis_print::{
    export_pdf, BandFields, ExportOptions, LineSource, PageSetup, Palette, PdfSummary, PrintError,
    SourceLine, Span,
};
use nulis_syntax::SyntaxToken;

/// Lines from memory, handed out in batches of 50 like the core's windows.
struct MemorySource {
    lines: Vec<SourceLine>,
    next: usize,
}

impl MemorySource {
    fn new(text: &str) -> Self {
        let lines = text
            .split('\n')
            .enumerate()
            .map(|(index, line)| SourceLine {
                number: u64::try_from(index).expect("small") + 1,
                continuation: false,
                separator_before: false,
                text: line.to_owned(),
                spans: line
                    .starts_with("fn")
                    .then_some(Span {
                        start: 0,
                        end: 2,
                        token: SyntaxToken::Keyword,
                    })
                    .into_iter()
                    .collect(),
            })
            .collect();
        Self { lines, next: 0 }
    }
}

impl LineSource for MemorySource {
    fn rewind(&mut self) -> Result<(), PrintError> {
        self.next = 0;
        Ok(())
    }

    fn next_lines(&mut self, highlight: bool) -> Result<Option<Vec<SourceLine>>, PrintError> {
        if self.next >= self.lines.len() {
            return Ok(None);
        }
        let end = (self.next + 50).min(self.lines.len());
        let mut batch = self.lines[self.next..end].to_vec();
        if !highlight {
            for line in &mut batch {
                line.spans.clear();
            }
        }
        self.next = end;
        Ok(Some(batch))
    }
}

fn export(text: &str, setup: PageSetup) -> (Vec<u8>, PdfSummary) {
    let mut source = MemorySource::new(text);
    let options = ExportOptions {
        setup,
        palette: Palette::print(),
        fields: BandFields {
            file: "sample.rs".into(),
            path: "/tmp/sample.rs".into(),
            date: "21 October 2026".into(),
            pages: None,
        },
        title: "sample.rs".into(),
        last_line: u64::try_from(source.lines.len()).expect("small"),
    };
    export_pdf(&mut source, Vec::new(), &options).expect("export")
}

fn numbered_lines(count: usize) -> String {
    (1..=count)
        .map(|line| {
            if line % 3 == 0 {
                format!("fn line_{line}() {{}}")
            } else {
                format!("line {line}\twith a tab")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Every in-use cross-reference entry points at its own `N 0 obj`.
fn check_cross_references(pdf: &[u8]) -> usize {
    let tail = std::str::from_utf8(&pdf[pdf.len() - 64..]).expect("ASCII trailer");
    let start: usize = tail
        .split("startxref\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .and_then(|number| number.parse().ok())
        .expect("startxref offset");
    let table = std::str::from_utf8(&pdf[start..]).expect("ASCII table");
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("xref"));
    let size: usize = lines
        .next()
        .and_then(|header| header.split(' ').nth(1))
        .and_then(|size| size.parse().ok())
        .expect("table size");
    let mut in_use = 0;
    for (object, entry) in lines.take(size).enumerate() {
        assert_eq!(entry.len(), 19, "entry {object}: {entry:?}");
        if entry.ends_with(" n ") {
            let offset: usize = entry[..10].parse().expect("offset");
            let expected = format!("{object} 0 obj");
            assert!(
                pdf[offset..].starts_with(expected.as_bytes()),
                "object {object} is not at {offset}"
            );
            in_use += 1;
        }
    }
    assert!(table.contains(&format!("/Size {size} ")));
    in_use
}

#[test]
fn a_small_document_exports_as_a_well_formed_pdf() {
    let (pdf, summary) = export(&numbered_lines(150), PageSetup::default());
    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    // 61 body rows per default A4 page: 150 lines take 3 pages.
    assert_eq!(summary.pages, 3);
    assert_eq!(summary.missing_glyphs, 0);
    assert_eq!(summary.bytes, u64::try_from(pdf.len()).expect("small"));
    assert!(find(&pdf, b"/Count 3").is_some());
    assert!(find(&pdf, b"/FontFile2").is_some());
    assert!(find(&pdf, b"/ToUnicode").is_some());
    // Keywords are bold in the print palette, so the bold font is embedded.
    assert!(find(&pdf, b"/JetBrainsMono-Bold").is_some());
    // Fewer than 100 pages: no outline.
    assert!(find(&pdf, b"/Outlines").is_none());
    assert!(check_cross_references(&pdf) > 10);
}

#[test]
fn export_is_deterministic() {
    let text = numbered_lines(80);
    assert_eq!(
        export(&text, PageSetup::default()).0,
        export(&text, PageSetup::default()).0
    );
}

#[test]
fn long_documents_get_an_outline_entry_per_hundred_pages() {
    let (pdf, summary) = export(&numbered_lines(61 * 101 + 5), PageSetup::default());
    assert_eq!(summary.pages, 102);
    assert!(find(&pdf, b"/Outlines").is_some());
    assert!(find(&pdf, b"/Type /Outlines").is_some() || find(&pdf, b"/First").is_some());
    check_cross_references(&pdf);
}

#[test]
fn characters_the_font_lacks_are_counted_and_plain_text_skips_the_bold_font() {
    let setup = PageSetup {
        syntax_highlighting: false,
        ..PageSetup::default()
    };
    let (pdf, summary) = export("hello 世界\nplain", setup);
    assert_eq!(summary.missing_glyphs, 2);
    assert_eq!(summary.pages, 1);
    assert!(find(&pdf, b"/JetBrainsMono-Bold").is_none());
    check_cross_references(&pdf);
}

#[test]
fn an_empty_document_exports_one_page() {
    let (pdf, summary) = export("", PageSetup::default());
    assert_eq!(summary.pages, 1);
    check_cross_references(&pdf);
    if let Ok(directory) = std::env::var("NULIS_PRINT_SAMPLE_DIR") {
        // Writes samples for a manual look in a PDF viewer.
        let samples = [
            ("sample-a4.pdf", numbered_lines(150), PageSetup::default()),
            (
                "sample-letter-landscape-cut.pdf",
                format!("{}\n{}", "x".repeat(300), numbered_lines(60)),
                PageSetup {
                    paper: nulis_print::PaperSize::Letter,
                    orientation: nulis_print::Orientation::Landscape,
                    long_lines: nulis_print::LongLines::Cut,
                    ..PageSetup::default()
                },
            ),
            (
                "sample-wrap.pdf",
                format!("{}\n{}", "y".repeat(300), numbered_lines(10)),
                PageSetup::default(),
            ),
        ];
        for (name, text, setup) in samples {
            let (bytes, _) = export(&text, setup);
            std::fs::write(std::path::Path::new(&directory).join(name), bytes)
                .expect("write sample");
        }
    }
}
