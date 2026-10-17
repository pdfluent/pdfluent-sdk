//! `office_markdown` over files the tests build themselves.
//!
//! Each fixture is written in-memory to a temp dir -- the ZIP containers are
//! assembled here, the same way the extractors' own tests assemble them --
//! and driven through the example's real argument parsing in-process. Every
//! test asserts its format's own heading text, so removing that format's
//! dispatch fails it rather than weakening it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[path = "../examples/office_markdown.rs"]
mod office_markdown;

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// A scratch dir under the system temp dir, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "office_markdown_{}_{}_{}",
            std::process::id(),
            unique,
            name
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn write(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.path.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The example's real entry, in-process: exit code, stdout, stderr.
fn run(args: &[&str]) -> (i32, String, String) {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = office_markdown::run_with_args(&owned, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// A ZIP container from `(archive path, bytes)` parts.
fn zip_bytes(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in parts {
        zip.start_file(name, options).unwrap();
        zip.write_all(content).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A one-heading `.docx`: a `Heading1` paragraph plus a body paragraph.
fn docx_bytes() -> Vec<u8> {
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"{W_NS}\"><w:body>\
         <w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>\
         <w:r><w:t>Docx Title</w:t></w:r></w:p>\
         <w:p><w:r><w:t>Docx body.</w:t></w:r></w:p>\
         </w:body></w:document>"
    );
    zip_bytes(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#,
        ),
        ("word/document.xml", document.as_bytes()),
    ])
}

const P_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A one-slide `.pptx`: a title placeholder plus a body shape.
fn pptx_bytes() -> Vec<u8> {
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:sld xmlns:p=\"{P_NS}\" xmlns:a=\"{A_NS}\" xmlns:r=\"{R_NS}\">\
         <p:cSld><p:spTree>\
         <p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Title\"/>\
         <p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr>\
         <p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>\
         <a:p><a:r><a:t>Deck Title</a:t></a:r></a:p>\
         </p:txBody></p:sp>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Body\"/></p:nvSpPr>\
         <p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>\
         <a:p><a:r><a:t>Slide body.</a:t></a:r></a:p>\
         </p:txBody></p:sp>\
         </p:spTree></p:cSld></p:sld>"
    );
    zip_bytes(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"#,
        ),
        (
            "_rels/.rels",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"{R_NS}/officeDocument\" Target=\"ppt/presentation.xml\"/>\
                 </Relationships>"
            )
            .as_bytes(),
        ),
        (
            "ppt/presentation.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <p:presentation xmlns:p=\"{P_NS}\" xmlns:r=\"{R_NS}\">\
                 <p:sldSz cx=\"9144000\" cy=\"6858000\"/>\
                 <p:sldIdLst><p:sldId id=\"256\" r:id=\"rId2\"/></p:sldIdLst></p:presentation>"
            )
            .as_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId2\" Type=\"{R_NS}/slide\" Target=\"slides/slide1.xml\"/>\
                 </Relationships>"
            )
            .as_bytes(),
        ),
        ("ppt/slides/slide1.xml", slide.as_bytes()),
    ])
}

/// A one-sheet `.xlsx`: tab `Figures` over a 2x2 grid of shared strings and a number.
fn xlsx_bytes() -> Vec<u8> {
    let shared =
        "<?xml version=\"1.0\"?><sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"3\" uniqueCount=\"3\">\
         <si><t>Name</t></si><si><t>Qty</t></si><si><t>Apples</t></si></sst>";
    let rows = "<sheetData>\
         <row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c></row>\
         <row r=\"2\"><c r=\"A2\" t=\"s\"><v>2</v></c><c r=\"B2\"><v>3</v></c></row>\
         </sheetData>";
    zip_bytes(&[
        (
            "xl/workbook.xml",
            br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Figures" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            format!(
                "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{rows}</worksheet>"
            )
            .as_bytes(),
        ),
        ("xl/sharedStrings.xml", shared.as_bytes()),
    ])
}

/// A one-chapter `.epub`: a heading plus a body paragraph.
fn epub_bytes() -> Vec<u8> {
    let chapter = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>ch</title></head>\
         <body><h1>Epub Title</h1><p>Epub body.</p></body></html>";
    zip_bytes(&[
        ("mimetype", b"application/epub+zip"),
        (
            "META-INF/container.xml",
            br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        ),
        (
            "OEBPS/content.opf",
            br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>t</dc:title></metadata><manifest><item id="c0" href="ch.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c0"/></spine></package>"#,
        ),
        ("OEBPS/ch.xhtml", chapter.as_bytes()),
    ])
}

#[test]
fn docx_prints_its_heading_as_markdown() {
    let dir = TempDir::new("docx");
    let path = dir.write("report.docx", &docx_bytes());
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Docx Title"), "{out}");
}

#[test]
fn pptx_prints_its_title_as_markdown() {
    let dir = TempDir::new("pptx");
    let path = dir.write("deck.pptx", &pptx_bytes());
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Deck Title"), "{out}");
    assert!(out.contains("Slide body."), "{out}");
}

#[test]
fn xlsx_prints_its_sheet_as_a_heading_and_a_table() {
    let dir = TempDir::new("xlsx");
    let path = dir.write("book.xlsx", &xlsx_bytes());
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Figures"), "{out}");
    assert!(out.contains("| Name | Qty |"), "{out}");
}

#[test]
fn epub_prints_its_chapter_as_markdown() {
    let dir = TempDir::new("epub");
    let path = dir.write("book.epub", &epub_bytes());
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Epub Title"), "{out}");
}

#[test]
fn html_prints_its_heading_as_markdown() {
    let dir = TempDir::new("html");
    let path = dir.write("page.html", b"<h1>Html Title</h1><p>Html body.</p>");
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Html Title"), "{out}");
}

#[test]
fn markdown_prints_its_heading_as_markdown() {
    let dir = TempDir::new("md");
    let path = dir.write("notes.md", b"# Md Title\n\nMd body.\n");
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("# Md Title"), "{out}");
}

#[test]
fn text_prints_its_paragraphs_as_markdown() {
    // Plain text has no heading syntax: the extractor reads paragraphs, and
    // the test pins the known first line rather than a `#` that cannot exist.
    let dir = TempDir::new("txt");
    let path = dir.write("notes.txt", b"Txt first line.\n\nTxt second line.\n");
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.is_empty(), "empty markdown for {path}");
    assert!(out.contains("Txt first line."), "{out}");
}

#[test]
fn json_prints_the_engine_blocks() {
    let dir = TempDir::new("json");
    let path = dir.write("report.docx", &docx_bytes());
    let (code, out, err) = run(&["--json", &path]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"kind\": \"heading\""), "{out}");
    assert!(out.contains("Docx Title"), "{out}");
}

#[test]
fn batch_writes_one_file_and_one_timings_line_per_input() {
    let dir = TempDir::new("batch");
    let docx = dir.write("report.docx", &docx_bytes());
    let md = dir.write("notes.md", b"# Md Title\n\nMd body.\n");
    let txt = dir.write("notes.txt", b"Txt first line.\n");
    let missing = dir.path.join("gone.docx").to_string_lossy().into_owned();
    let list = dir.write(
        "list.txt",
        format!("{docx}\n{md}\n{txt}\n{missing}\n").as_bytes(),
    );
    let out_dir = dir.path.join("out").to_string_lossy().into_owned();

    let (code, _, err) = run(&["--batch", &list, "--out-dir", &out_dir]);
    assert_eq!(code, 1, "a crashed input must fail the batch: {err}");

    let out = std::path::Path::new(&out_dir);
    let report = std::fs::read_to_string(out.join("report.docx.md")).unwrap();
    assert!(report.contains("# Docx Title"), "{report}");
    let notes = std::fs::read_to_string(out.join("notes.md.md")).unwrap();
    assert!(notes.contains("# Md Title"), "{notes}");
    let text = std::fs::read_to_string(out.join("notes.txt.md")).unwrap();
    assert!(text.contains("Txt first line."), "{text}");

    let timings = std::fs::read_to_string(out.join("timings.tsv")).unwrap();
    let lines: Vec<&str> = timings.lines().collect();
    assert_eq!(lines.len(), 4, "{timings}");
    for (line, name, outcome) in [
        (lines[0], "report.docx", "ok"),
        (lines[1], "notes.md", "ok"),
        (lines[2], "notes.txt", "ok"),
        (lines[3], "gone.docx", "crash"),
    ] {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "{line}");
        assert_eq!(fields[0], name, "{line}");
        assert!(fields[1].parse::<u128>().is_ok(), "{line}");
        assert_eq!(fields[2], outcome, "{line}");
    }
}

#[test]
fn a_missing_file_exits_1_with_the_message_on_stderr() {
    let dir = TempDir::new("missing");
    let missing = dir.path.join("gone.docx").to_string_lossy().into_owned();
    let (code, out, err) = run(&[&missing]);
    assert_eq!(code, 1);
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("read"), "{err}");
    assert!(err.contains("gone.docx"), "{err}");
}

#[test]
fn an_unknown_extension_exits_1_with_the_message_on_stderr() {
    let dir = TempDir::new("unknown");
    let path = dir.write("report.xyz", b"not an office file");
    let (code, out, err) = run(&[&path]);
    assert_eq!(code, 1);
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("unsupported extension"), "{err}");
}
