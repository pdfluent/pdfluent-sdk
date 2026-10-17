//! The EPUB-to-engine mapping, held from outside the crate.
//!
//! `extract_engine_blocks` and `extract_markdown` are covered by unit tests
//! inside `src/lib.rs`; this file drives the same mapping through the public
//! API only, on EPUBs built in memory, so the kinds, levels, ordinals, table
//! grid, spine-order pages and Markdown rendering the engine consumes are
//! pinned where an outside caller sees them.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use epub_extract::{extract_engine_blocks, extract_markdown};
use pdf_engine::BlockKind;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

/// One chapter: its archive path, its href in the package document, and its
/// XHTML body fragment.
struct Chapter<'a> {
    path: &'a str,
    href: &'a str,
    body: &'a str,
}

fn chapter_doc(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body>{body}</body></html>"
    )
}

/// Build an EPUB in memory: `mimetype`, `container.xml`, a package document
/// at `OEBPS/content.opf` whose manifest lists `chapters` and whose spine
/// follows `spine_order`, then the chapters in slice order.
fn build_epub(chapters: &[Chapter], spine_order: &[usize]) -> Vec<u8> {
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
    let mut manifest = String::new();
    for (i, chapter) in chapters.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"c{i}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
            chapter.href
        ));
    }
    let mut spine = String::new();
    for index in spine_order {
        spine.push_str(&format!("<itemref idref=\"c{index}\"/>\n"));
    }
    let opf = format!(
        "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>t</dc:title></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"
    );
    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    for (i, chapter) in chapters.iter().enumerate() {
        zip.start_file(chapter.path, deflated).unwrap();
        zip.write_all(chapter_doc(&format!("ch{i}"), chapter.body).as_bytes())
            .unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn headings_keep_h1_and_h3_levels() {
    let book = build_epub(
        &[Chapter {
            path: "OEBPS/ch.xhtml",
            href: "ch.xhtml",
            body: "<h1>Title</h1><p>body</p><h3>Deep</h3><p>more</p>",
        }],
        &[0],
    );
    let blocks = extract_engine_blocks(&book).unwrap();
    let kinds: Vec<String> = blocks
        .iter()
        .map(|block| match block.kind {
            BlockKind::Heading { level } => format!("h{level}"),
            BlockKind::Paragraph => "p".to_string(),
            ref kind => panic!("expected a heading or paragraph, got {kind:?}"),
        })
        .collect();
    assert_eq!(kinds, vec!["h1", "p", "h3", "p"]);
    let levels: Vec<u8> = blocks
        .iter()
        .filter_map(|block| match block.kind {
            BlockKind::Heading { level } => Some(level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![1, 3]);
    assert_eq!(blocks[0].text, "Title");
    assert_eq!(blocks[2].text, "Deep");
}

#[test]
fn nested_lists_keep_levels_ordinals_and_restarts() {
    let book = build_epub(
        &[Chapter {
            path: "OEBPS/lists.xhtml",
            href: "lists.xhtml",
            body: "<ol><li>first<ol><li>nested</li></ol></li><li>second</li></ol><ul><li>bulleted<ul><li>deep</li></ul></li></ul><ol><li>again one</li><li>again two</li></ol>",
        }],
        &[0],
    );
    let blocks = extract_engine_blocks(&book).unwrap();
    let items: Vec<(u8, Option<u32>, &str)> = blocks
        .iter()
        .filter_map(|block| match &block.kind {
            BlockKind::ListItem { level, ordinal } => Some((*level, *ordinal, block.text.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        items,
        vec![
            (0, Some(1), "first"),
            (1, Some(1), "nested"),
            (0, Some(2), "second"),
            (0, None, "bulleted"),
            (1, None, "deep"),
            (0, Some(1), "again one"),
            (0, Some(2), "again two"),
        ]
    );
}

#[test]
fn table_with_colspan_and_header_row_keeps_the_grid() {
    let book = build_epub(
        &[Chapter {
            path: "OEBPS/table.xhtml",
            href: "table.xhtml",
            body: "<table><thead><tr><th>Name</th><th>Qty</th></tr></thead><tbody><tr><td>Apples</td><td>3</td></tr><tr><td colspan=\"2\">sold out</td></tr></tbody></table>",
        }],
        &[0],
    );
    let blocks = extract_engine_blocks(&book).unwrap();
    let cells: Vec<(u16, u16, u16, u16, bool, &str)> = blocks
        .iter()
        .filter_map(|block| match &block.kind {
            BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                header,
            } => Some((
                *row,
                *column,
                *row_span,
                *column_span,
                *header,
                block.text.as_str(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        cells,
        vec![
            (0, 0, 1, 1, true, "Name"),
            (0, 1, 1, 1, true, "Qty"),
            (1, 0, 1, 1, false, "Apples"),
            (1, 1, 1, 1, false, "3"),
            (2, 0, 1, 2, false, "sold out"),
        ]
    );
}

#[test]
fn spine_order_sets_pages_and_reading_order() {
    // The archive holds the second chapter first; the spine reads the first
    // one first, and the pages follow the spine.
    let book = build_epub(
        &[
            Chapter {
                path: "OEBPS/second.xhtml",
                href: "second.xhtml",
                body: "<h1>Second</h1><p>second body</p>",
            },
            Chapter {
                path: "OEBPS/first.xhtml",
                href: "first.xhtml",
                body: "<h1>First</h1><p>first body</p>",
            },
        ],
        &[1, 0],
    );
    let blocks = extract_engine_blocks(&book).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|block| block.text.as_str()).collect();
    assert_eq!(texts, vec!["First", "first body", "Second", "second body"]);
    let pages: Vec<usize> = blocks.iter().map(|block| block.page).collect();
    assert_eq!(pages, vec![0, 0, 1, 1]);
    let indexes: Vec<usize> = blocks.iter().map(|block| block.index).collect();
    assert_eq!(indexes, vec![0, 1, 2, 3]);
}

#[test]
fn markdown_holds_heading_list_and_table_in_order() {
    let book = build_epub(
        &[Chapter {
            path: "OEBPS/mixed.xhtml",
            href: "mixed.xhtml",
            body: "<h1>Harvest</h1><ul><li>apples</li><li>pears</li></ul><table><tr><th>Fruit</th><th>Kilos</th></tr><tr><td>Apples</td><td>3</td></tr></table>",
        }],
        &[0],
    );
    let markdown = extract_markdown(&book).unwrap();
    assert!(markdown.contains("# Harvest"), "{markdown}");
    assert!(markdown.contains("- apples"), "{markdown}");
    assert!(markdown.contains("- pears"), "{markdown}");
    assert!(markdown.contains("| Fruit | Kilos |"), "{markdown}");
    let heading = markdown.find("# Harvest").unwrap();
    let list = markdown.find("- apples").unwrap();
    let table = markdown.find("| Fruit | Kilos |").unwrap();
    assert!(heading < list && list < table, "{markdown}");
}
