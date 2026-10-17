// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Non-PDF input on the compatible open path (#646).
//!
//! EPUB, images and HTML open as PDF through `open_compat`, and
//! `convert_to_pdf` works on all three. Named formats without a converter
//! fail as unsupported; nameless junk still fails as invalid PDF.

use pdfluent::compat::DocumentOpenOptions;
use pdfluent::prelude::*;
use test_skip::skip_test;

const PNG_10X6: &[u8] = include_bytes!("fixtures/tiny_10x6.png");
const PNG_144DPI: &[u8] = include_bytes!("fixtures/tiny_144dpi.png");
const JPG_150DPI: &[u8] = include_bytes!("fixtures/tiny_150dpi.jpg");
const JPG_1X1: &[u8] = include_bytes!("fixtures/tiny_red.jpg");

fn open_data(
    data: &[u8],
    filetype: Option<&str>,
) -> std::result::Result<PdfDocument, pdfluent::Error> {
    let mut options = DocumentOpenOptions::default();
    options.data = Some(data.to_vec());
    options.filetype = filetype.map(str::to_string);
    PdfDocument::open_compat(&options)
}

/// Page boxes of a PDF, in points, via the raw page tree.
fn page_boxes(doc: &PdfDocument) -> Vec<[f64; 4]> {
    let bytes = doc.to_bytes().expect("serialise");
    let parsed = lopdf::Document::load_mem(&bytes).expect("re-parse");
    let mut pages: Vec<(u32, lopdf::ObjectId)> = parsed.get_pages().into_iter().collect();
    pages.sort_by_key(|(n, _)| *n);
    pages
        .iter()
        .map(|(_, id)| {
            let dict = parsed.get_dictionary(*id).expect("page dict");
            let media = dict.get(b"MediaBox").expect("MediaBox");
            let arr = media.as_array().expect("MediaBox array");
            let num = |i: usize| match &arr[i] {
                lopdf::Object::Integer(n) => *n as f64,
                lopdf::Object::Real(f) => f64::from(*f),
                other => panic!("bad MediaBox entry: {other:?}"),
            };
            [num(0), num(1), num(2), num(3)]
        })
        .collect()
}

fn text_of(doc: &PdfDocument) -> String {
    let bytes = doc.to_bytes().expect("serialise");
    let engine = pdf_engine::PdfDocument::open(bytes).expect("engine opens");
    engine
        .extract_blocks(&pdf_engine::ExtractOptions::default())
        .expect("extract")
        .iter()
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A minimal one-chapter EPUB: mimetype, container, package, chapter.
fn minimal_epub() -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)
        .unwrap();
    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>t</dc:title></metadata><manifest><item id="c0" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c0"/></spine></package>"#)
        .unwrap();
    zip.start_file("OEBPS/ch1.xhtml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><h1>Chapter One</h1><p>Some body text here.</p></body></html>"#)
        .unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn epub_opens_as_pdf_pages() {
    let epub = minimal_epub();
    let doc = open_data(&epub, None).expect("open epub bytes");
    assert!(doc.page_count() >= 1);
    let text = text_of(&doc);
    let words: Vec<&str> = text.split_whitespace().collect();
    assert!(
        words.windows(2).any(|w| w == ["Chapter", "One"]),
        "epub chapter text missing: {words:?}"
    );
    // convert_to_pdf works on the converted handle too.
    let out = doc.convert_to_pdf(0, -1, 0).expect("convert epub doc");
    let back = PdfDocument::from_bytes(&out).expect("converted epub re-opens");
    assert_eq!(back.page_count(), doc.page_count());
}

#[test]
fn png_opens_as_a_one_page_pdf() {
    let doc = open_data(PNG_10X6, None).expect("open png bytes");
    assert_eq!(doc.page_count(), 1);
    assert_eq!(page_boxes(&doc), vec![[0.0, 0.0, 10.0, 6.0]]);
    let out = doc.convert_to_pdf(0, -1, 0).expect("convert image doc");
    assert_eq!(
        PdfDocument::from_bytes(&out)
            .expect("converted image re-opens")
            .page_count(),
        1
    );
}

#[test]
fn image_dpi_sizes_the_page() {
    let doc = open_data(PNG_144DPI, None).expect("open 144dpi png");
    // 12 px at ~144 dpi: 6 pt a side, within pHYs rounding.
    let page = page_boxes(&doc);
    assert_eq!(page.len(), 1);
    assert!((page[0][2] - 6.0).abs() < 0.01, "width: {:?}", page[0]);
    assert!((page[0][3] - 6.0).abs() < 0.01, "height: {:?}", page[0]);

    let doc = open_data(JPG_150DPI, None).expect("open 150dpi jpeg");
    // 30x20 px at 150 dpi: 14.4 x 9.6 pt.
    let page = page_boxes(&doc);
    assert_eq!(page.len(), 1);
    assert!((page[0][2] - 14.4).abs() < 0.01, "width: {:?}", page[0]);
    assert!((page[0][3] - 9.6).abs() < 0.01, "height: {:?}", page[0]);

    // No DPI anywhere: one pixel per point.
    let doc = open_data(JPG_1X1, None).expect("open 1x1 jpeg");
    assert_eq!(page_boxes(&doc), vec![[0.0, 0.0, 1.0, 1.0]]);
}

#[test]
fn image_bytes_open_despite_a_wrong_hint() {
    // Magic wins: these bytes are a PNG whatever the hint claims.
    let doc = open_data(PNG_10X6, Some("pdf")).expect("magic beats the hint");
    assert_eq!(doc.page_count(), 1);
    assert_eq!(page_boxes(&doc), vec![[0.0, 0.0, 10.0, 6.0]]);
}

#[test]
fn named_but_unconvertible_formats_fail_by_name() {
    for format in ["cbz", "fb2", "mobi", "xps", "svg", "txt"] {
        let err = open_data(b"junk bytes here", Some(format)).unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("not supported in this version: {format}")),
            "wrong error for {format}: {err}"
        );
    }
    // The filename extension is the fallback hint.
    let mut options = DocumentOpenOptions::default();
    options.data = Some(b"junk bytes here".to_vec());
    options.filetype = None;
    let err = PdfDocument::open_compat(&options).unwrap_err();
    assert!(
        !err.to_string().contains("not supported"),
        "nameless junk must fail as invalid PDF, not unsupported: {err}"
    );
}

#[test]
fn unsupported_filename_extension_fails_by_name() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("pdfluent-open-test-{}.cbz", std::process::id()));
    std::fs::write(&path, b"junk bytes here").expect("write temp cbz");
    let mut options = DocumentOpenOptions::default();
    options.filename = Some(path.to_string_lossy().into_owned());
    let err = PdfDocument::open_compat(&options).unwrap_err();
    let _ = std::fs::remove_file(&path);
    assert!(
        err.to_string()
            .contains("not supported in this version: cbz"),
        "wrong error: {err}"
    );
}

/// Serialises the tests that mutate `PDFLUENT_CHROME`: the environment is
/// process-global and the runner is parallel.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Whether the real-Chrome tests may run: rendering needs a Chrome whose
/// sandbox survives the current one, which the build sandbox denies.
fn real_chrome_allowed() -> bool {
    std::env::var("PDFLUENT_REAL_CHROME").as_deref() == Ok("1")
}

/// A stand-in Chrome: records its arguments, writes a one-page PDF to the
/// `--print-to-pdf` target. Returns the script path and the args log path.
#[cfg(unix)]
fn fake_chrome() -> (String, String) {
    let dir = std::env::temp_dir();
    let tag = format!("pdfluent-fake-chrome-{}", std::process::id());
    let script = dir.join(format!("{tag}.sh"));
    let args_log = dir.join(format!("{tag}.args"));
    let pdf = "%PDF-1.7\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 100 100]/Contents 4 0 R/Resources<<>>>>endobj\n4 0 obj<</Length 0>>stream\n\nendstream\nendobj\ntrailer<</Root 1 0 R>>\n";
    let body = format!(
        "#!/bin/sh\necho \"$@\" > {}\nout=\"\"\nfor a in \"$@\"; do case \"$a\" in --print-to-pdf=*) out=\"${{a#--print-to-pdf=}}\";; esac; done\nprintf '%s' '{}' > \"$out\"\n",
        args_log.display(),
        pdf.replace('\'', "'\\''")
    );
    std::fs::write(&script, body).expect("write fake chrome");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script)
            .expect("stat fake chrome")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod fake chrome");
    }
    (
        script.to_string_lossy().into_owned(),
        args_log.to_string_lossy().into_owned(),
    )
}

#[test]
#[cfg(unix)]
fn html_plumbing_uses_env_chrome() {
    let _env = ENV_LOCK.lock().unwrap();
    let (script, args_log) = fake_chrome();
    std::env::set_var("PDFLUENT_CHROME", &script);
    let opened = open_data(b"<html><body><p>x</p></body></html>", Some("html"));
    std::env::remove_var("PDFLUENT_CHROME");
    let args = std::fs::read_to_string(&args_log).expect("fake chrome saw args");
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&args_log);

    let doc = opened.expect("open html through env chrome");
    assert_eq!(doc.page_count(), 1);
    assert!(
        args.contains("--headless") && args.contains("--print-to-pdf="),
        "chrome got wrong args: {args}"
    );
}

#[test]
fn html_env_chrome_garbage_fails_loud() {
    let _env = ENV_LOCK.lock().unwrap();
    std::env::set_var("PDFLUENT_CHROME", "/nonexistent-chrome-xyz");
    let err = open_data(b"<html></html>", Some("html")).unwrap_err();
    std::env::remove_var("PDFLUENT_CHROME");
    assert!(
        err.to_string().contains("PDFLUENT_CHROME"),
        "wrong error: {err}"
    );
}

#[test]
fn html_content_through_real_chrome() {
    if !real_chrome_allowed() {
        skip_test!("set PDFLUENT_REAL_CHROME=1 to render HTML with a real Chrome")
    }
    let _env = ENV_LOCK.lock().unwrap();
    std::env::remove_var("PDFLUENT_CHROME");
    let html = b"<html><body><h1>Hello from HTML</h1></body></html>";
    let doc = open_data(html, Some("html")).expect("open html with real chrome");
    assert!(doc.page_count() >= 1);
    let text = text_of(&doc);
    assert!(
        text.contains("Hello from HTML"),
        "rendered text missing: {text:?}"
    );
}

#[test]
fn html_opens_by_filename_extension() {
    if !real_chrome_allowed() {
        skip_test!("set PDFLUENT_REAL_CHROME=1 to render HTML with a real Chrome")
    }
    let _env = ENV_LOCK.lock().unwrap();
    std::env::remove_var("PDFLUENT_CHROME");
    let dir = std::env::temp_dir();
    let path = dir.join(format!("pdfluent-open-test-{}.html", std::process::id()));
    std::fs::write(&path, b"<html><body><p>Extension sniffed</p></body></html>")
        .expect("write temp html");
    let mut options = DocumentOpenOptions::default();
    options.filename = Some(path.to_string_lossy().into_owned());
    let opened = PdfDocument::open_compat(&options);
    let _ = std::fs::remove_file(&path);
    let doc = opened.expect("open html file with real chrome");
    assert!(doc.page_count() >= 1);
    let text = text_of(&doc);
    assert!(
        text.contains("Extension sniffed"),
        "rendered text missing: {text:?}"
    );
}

#[test]
fn html_missing_chrome_message() {
    if !real_chrome_allowed() {
        skip_test!("set PDFLUENT_REAL_CHROME=1 to check the missing-Chrome branch")
    }
    let _env = ENV_LOCK.lock().unwrap();
    std::env::remove_var("PDFLUENT_CHROME");
    match open_data(b"<html></html>", Some("html")) {
        Err(e) if e.to_string().contains("requires Chrome or Chromium") => {
            assert!(
                e.to_string().contains(
                    "HTML conversion requires Chrome or Chromium; see https://pdfluent.com/how-to/files/#generate-pdf-from-html-rust"
                ),
                "wrong message: {e}"
            );
        }
        Ok(_) => skip_test!("a Chrome is installed; the missing branch needs one without"),
        Err(e) => panic!("wrong error for html: {e}"),
    }
}
