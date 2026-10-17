#![warn(missing_docs)]
//! EPUB content extraction: chapters in spine order as structured blocks.
//!
//! [`extract_epub`] takes the bytes of an `.epub` file and returns its text as
//! [`Block`]s: headings with levels, paragraphs, list items with nesting,
//! table cells with spans, links and image references, in the order the spine
//! states. There are no page numbers; every block carries its chapter instead.
//!
//! ```no_run
//! let bytes = std::fs::read("book.epub").unwrap();
//! let blocks = epub_extract::extract_epub(&bytes).unwrap();
//! ```
//!
//! [`extract_engine_blocks`] maps the same book onto `pdf-engine`'s blocks,
//! with the spine index as the page, so EPUBs serialize through the engine's
//! Markdown and JSON alongside Office files and PDFs.
//!
//! [`extract_metadata`] reads the catalogue fields, [`extract_toc`] the
//! navigation, [`is_epub`] sniffs the container for routing, and
//! [`to_pdf_bytes`] renders the book as a plain PDF for callers that only
//! open PDFs.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod blocks;
pub mod error;

mod engine;
mod epub;
mod meta;
mod pdf;
mod xhtml;

pub use blocks::{Block, BlockKind};
pub use engine::{extract_engine_blocks, extract_markdown};
pub use error::{EpubError, Result};
pub use meta::{extract_metadata, extract_toc, EpubMetadata, TocEntry};
pub use pdf::to_pdf_bytes;

/// Extract the text of an EPUB archive as structured blocks.
///
/// Reads `container.xml`, then the package document's spine, then each spine
/// chapter in spine order (never in archive order). Chapters are numbered from
/// 0 in that order; [`Block::index`] counts reading order across the whole
/// book. Spine items that are not chapters (images, stylesheets, navigation
/// documents) are skipped.
///
/// Fails when the archive cannot be opened, when `container.xml` or the
/// package document is missing or names nothing, or when a chapter is missing
/// or unparsable.
pub fn extract_epub(epub: &[u8]) -> Result<Vec<Block>> {
    epub::extract(epub)
}

/// Whether `bytes` look like an EPUB archive.
///
/// True for a readable ZIP carrying the EPUB mimetype or the container
/// document; a routing sniff, not a validation. Anything else -- a PDF, an
/// Office file, a plain ZIP -- reads false.
pub fn is_epub(bytes: &[u8]) -> bool {
    epub::is_epub(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    /// One chapter: its archive path, its href in the package document, and
    /// its XHTML body fragment.
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

    /// Build an EPUB in memory: `mimetype`, `container.xml`, a package
    /// document at `OEBPS/content.opf` whose manifest lists `chapters` and
    /// whose spine follows `spine_order`, then the chapters in slice order.
    fn build_epub(chapters: &[Chapter], spine_order: &[usize]) -> Vec<u8> {
        build_epub_full(chapters, spine_order, "<dc:title>t</dc:title>", "", &[])
    }

    /// One extra archive entry beyond the chapters: its path and its body.
    struct Extra<'a> {
        path: &'a str,
        body: &'a str,
    }

    /// [`build_epub`] with `metadata` as the package metadata body,
    /// `manifest_extra` spliced into the manifest, and `extras` written as
    /// further archive entries (navigation files, stylesheets).
    fn build_epub_full(
        chapters: &[Chapter],
        spine_order: &[usize],
        metadata: &str,
        manifest_extra: &str,
        extras: &[Extra],
    ) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
        manifest.push_str(manifest_extra);
        let mut spine = String::new();
        for index in spine_order {
            spine.push_str(&format!("<itemref idref=\"c{index}\"/>\n"));
        }
        let opf = format!(
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{metadata}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"
        );
        zip.start_file("OEBPS/content.opf", deflated).unwrap();
        zip.write_all(opf.as_bytes()).unwrap();
        for (i, chapter) in chapters.iter().enumerate() {
            zip.start_file(chapter.path, deflated).unwrap();
            zip.write_all(chapter_doc(&format!("ch{i}"), chapter.body).as_bytes())
                .unwrap();
        }
        for extra in extras {
            zip.start_file(extra.path, deflated).unwrap();
            zip.write_all(extra.body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// Three chapters whose ZIP order, OPF hrefs and spine order all differ:
    /// the spine runs one, two, three while the archive holds three, one,
    /// two, and two of the three hrefs need resolving (a percent escape, a
    /// subdirectory).
    fn spine_book() -> Vec<u8> {
        build_epub(
            &[
                Chapter {
                    path: "OEBPS/text/three.xhtml",
                    href: "text/three.xhtml",
                    body: "<h3>Three</h3><p>third body</p><h4>Four</h4><h5>Five</h5><h6>Six</h6>",
                },
                Chapter {
                    path: "OEBPS/one.xhtml",
                    href: "one.xhtml",
                    body: "<h1>One</h1><p>first body</p>",
                },
                Chapter {
                    path: "OEBPS/ch 2.xhtml",
                    href: "./ch%202.xhtml",
                    body: "<h2>Two</h2><p>second body</p>",
                },
            ],
            &[1, 2, 0],
        )
    }

    /// A nested list plus an ordered list, and a table with a header row, a
    /// `colspan` and a `rowspan`.
    fn lists_tables_book() -> Vec<u8> {
        build_epub(
            &[
                Chapter {
                    path: "OEBPS/lists.xhtml",
                    href: "lists.xhtml",
                    body: "<ul><li>top<ul><li>nested<ul><li>deep</li></ul></li><li>second</li></ul></li><li>tail</li></ul><ol><li>first</li><li>second</li><li>third</li></ol>",
                },
                Chapter {
                    path: "OEBPS/tables.xhtml",
                    href: "tables.xhtml",
                    body: "<table><thead><tr><th>Name</th><th>Qty</th><th>Price</th></tr></thead><tbody><tr><td>Apples</td><td colspan=\"2\">sold out</td></tr><tr><td rowspan=\"2\">Oranges</td><td>3</td><td>1.20</td></tr><tr><td>4</td><td>1.50</td></tr></tbody></table>",
                },
            ],
            &[0, 1],
        )
    }

    /// Links (inline and standalone), emphasis, and images (inline and
    /// standalone).
    fn links_media_book() -> Vec<u8> {
        build_epub(
            &[Chapter {
                path: "OEBPS/links.xhtml",
                href: "links.xhtml",
                body: "<p>Read the <a href=\"ch2.xhtml\">second chapter</a> about <strong>bold <em>and italic</em></strong> text.</p><a href=\"https://example.com\">Standalone</a><img src=\"img/cover.png\" alt=\"Cover\"/><p>Figure <img src=\"img/f1.png\" alt=\"one\"/> here</p>",
            }],
            &[0],
        )
    }

    #[test]
    fn spine_order_is_kept() {
        let blocks = extract_epub(&spine_book()).unwrap();
        let hrefs: Vec<&str> = blocks
            .iter()
            .map(|block| block.chapter_href.as_str())
            .collect();
        assert_eq!(
            hrefs,
            vec![
                "OEBPS/one.xhtml",
                "OEBPS/one.xhtml",
                "OEBPS/ch 2.xhtml",
                "OEBPS/ch 2.xhtml",
                "OEBPS/text/three.xhtml",
                "OEBPS/text/three.xhtml",
                "OEBPS/text/three.xhtml",
                "OEBPS/text/three.xhtml",
                "OEBPS/text/three.xhtml",
            ]
        );
        let chapters: Vec<usize> = blocks.iter().map(|block| block.chapter).collect();
        assert_eq!(chapters, vec![0, 0, 1, 1, 2, 2, 2, 2, 2]);
        let indexes: Vec<usize> = blocks.iter().map(|block| block.index).collect();
        assert_eq!(indexes, vec![0, 1, 2, 3, 4, 5, 6, 7, 8]);
        let texts: Vec<&str> = blocks.iter().map(|block| block.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "One",
                "first body",
                "Two",
                "second body",
                "Three",
                "third body",
                "Four",
                "Five",
                "Six"
            ]
        );
    }

    #[test]
    fn heading_levels_follow_h1_to_h6() {
        let blocks = extract_epub(&spine_book()).unwrap();
        let levels: Vec<u8> = blocks
            .iter()
            .filter_map(|block| match block.kind {
                BlockKind::Heading { level } => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn nested_list_keeps_levels_and_ordinals() {
        let blocks = extract_epub(&lists_tables_book()).unwrap();
        let items: Vec<(u8, Option<u32>, &str)> = blocks
            .iter()
            .filter_map(|block| match &block.kind {
                BlockKind::ListItem { level, ordinal } => {
                    Some((*level, *ordinal, block.text.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, None, "top"),
                (1, None, "nested"),
                (2, None, "deep"),
                (1, None, "second"),
                (0, None, "tail"),
                (0, Some(1), "first"),
                (0, Some(2), "second"),
                (0, Some(3), "third"),
            ]
        );
    }

    #[test]
    fn table_keeps_spans_headers_and_grid() {
        let blocks = extract_epub(&lists_tables_book()).unwrap();
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
                (0, 2, 1, 1, true, "Price"),
                (1, 0, 1, 1, false, "Apples"),
                (1, 1, 1, 2, false, "sold out"),
                (2, 0, 2, 1, false, "Oranges"),
                (2, 1, 1, 1, false, "3"),
                (2, 2, 1, 1, false, "1.20"),
                (3, 1, 1, 1, false, "4"),
                (3, 2, 1, 1, false, "1.50"),
            ]
        );
    }

    #[test]
    fn links_and_emphasis_become_inline_markdown() {
        let blocks = extract_epub(&links_media_book()).unwrap();
        assert_eq!(blocks.len(), 4);
        assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
        assert_eq!(
            blocks[0].text,
            "Read the [second chapter](ch2.xhtml) about **bold *and italic*** text."
        );
        match &blocks[1].kind {
            BlockKind::Link { href } => assert_eq!(href, "https://example.com"),
            kind => panic!("expected a Link block, got {kind:?}"),
        }
        assert_eq!(blocks[1].text, "Standalone");
    }

    #[test]
    fn images_become_references() {
        let blocks = extract_epub(&links_media_book()).unwrap();
        match &blocks[2].kind {
            BlockKind::Image { src, alt } => {
                assert_eq!(src, "img/cover.png");
                assert_eq!(alt, "Cover");
            }
            kind => panic!("expected an Image block, got {kind:?}"),
        }
        assert_eq!(blocks[2].text, "Cover");
        assert!(matches!(blocks[3].kind, BlockKind::Paragraph));
        assert_eq!(blocks[3].text, "Figure ![one](img/f1.png) here");
    }

    #[test]
    fn engine_blocks_keep_kinds_levels_pages_and_order() {
        let blocks = extract_engine_blocks(&spine_book()).unwrap();
        let kinds: Vec<String> = blocks
            .iter()
            .map(|block| match block.kind {
                pdf_engine::BlockKind::Heading { level } => format!("h{level}"),
                pdf_engine::BlockKind::Paragraph => "p".to_string(),
                ref kind => panic!("expected a heading or paragraph, got {kind:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["h1", "p", "h2", "p", "h3", "p", "h4", "h5", "h6"]
        );
        let pages: Vec<usize> = blocks.iter().map(|block| block.page).collect();
        assert_eq!(pages, vec![0, 0, 1, 1, 2, 2, 2, 2, 2]);
        let indexes: Vec<usize> = blocks.iter().map(|block| block.index).collect();
        assert_eq!(indexes, (0..9).collect::<Vec<usize>>());
        assert!(blocks.iter().all(|block| block.bbox == [0.0; 4]));
        assert_eq!(blocks[0].text, "One");
        assert_eq!(blocks[1].text, "first body");
    }

    #[test]
    fn engine_blocks_keep_list_levels_ordinals_and_cells() {
        let blocks = extract_engine_blocks(&lists_tables_book()).unwrap();
        let items: Vec<(u8, Option<u32>, &str)> = blocks
            .iter()
            .filter_map(|block| match &block.kind {
                pdf_engine::BlockKind::ListItem { level, ordinal } => {
                    Some((*level, *ordinal, block.text.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, None, "top"),
                (1, None, "nested"),
                (2, None, "deep"),
                (1, None, "second"),
                (0, None, "tail"),
                (0, Some(1), "first"),
                (0, Some(2), "second"),
                (0, Some(3), "third"),
            ]
        );
        let cells: Vec<(u16, u16, u16, u16, bool, &str)> = blocks
            .iter()
            .filter_map(|block| match &block.kind {
                pdf_engine::BlockKind::TableCell {
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
                (0, 2, 1, 1, true, "Price"),
                (1, 0, 1, 1, false, "Apples"),
                (1, 1, 1, 2, false, "sold out"),
                (2, 0, 2, 1, false, "Oranges"),
                (2, 1, 1, 1, false, "3"),
                (2, 2, 1, 1, false, "1.20"),
                (3, 1, 1, 1, false, "4"),
                (3, 2, 1, 1, false, "1.50"),
            ]
        );
    }

    #[test]
    fn engine_markdown_has_a_heading_a_list_and_a_table() {
        let book = build_epub(
            &[Chapter {
                path: "OEBPS/mixed.xhtml",
                href: "mixed.xhtml",
                body: "<h1>Harvest</h1><ul><li>apples</li><li>pears</li></ul><table><tr><th>Fruit</th><th>Kilos</th></tr><tr><td>Apples</td><td>3</td></tr></table><p>Read the <a href=\"ch2.xhtml\">second chapter</a>.</p>",
            }],
            &[0],
        );
        let markdown = extract_markdown(&book).unwrap();
        assert!(markdown.contains("# Harvest"), "{markdown}");
        assert!(markdown.contains("- apples"), "{markdown}");
        assert!(markdown.contains("| Fruit | Kilos |"), "{markdown}");
        assert!(
            markdown.contains("[second chapter](ch2.xhtml)"),
            "{markdown}"
        );
    }

    #[test]
    fn blockquote_and_pre_are_plain_blocks() {
        let quote = "The harbour survey records every beacon along the northern channel.";
        let book = build_epub(
            &[Chapter {
                path: "OEBPS/quote.xhtml",
                href: "quote.xhtml",
                body: &format!("<blockquote>{quote}</blockquote><pre>x = 1\n y = 2</pre>"),
            }],
            &[0],
        );
        let blocks = extract_epub(&book).unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
        assert_eq!(blocks[0].text, quote);
        assert!(matches!(blocks[1].kind, BlockKind::Paragraph));
        assert_eq!(blocks[1].text, "x = 1\n y = 2");

        let markdown = extract_markdown(&book).unwrap();
        assert!(!markdown.contains("\\>"), "{markdown}");
        assert!(!markdown.contains('`'), "{markdown}");
        assert!(markdown.contains(quote), "{markdown}");
    }

    #[test]
    fn engine_blocks_spell_out_standalone_links_and_images() {
        let blocks = extract_engine_blocks(&links_media_book()).unwrap();
        assert_eq!(blocks.len(), 4);
        assert!(matches!(blocks[1].kind, pdf_engine::BlockKind::Paragraph));
        assert_eq!(blocks[1].text, "[Standalone](https://example.com)");
        assert!(matches!(blocks[2].kind, pdf_engine::BlockKind::Paragraph));
        assert_eq!(blocks[2].text, "![Cover](img/cover.png)");
        let markdown = extract_markdown(&links_media_book()).unwrap();
        assert!(
            markdown.contains("[Standalone](https://example.com)"),
            "{markdown}"
        );
        assert!(markdown.contains("![Cover](img/cover.png)"), "{markdown}");
    }

    #[test]
    fn metadata_reads_title_and_creator() {
        let book = build_epub_full(
            &[Chapter {
                path: "OEBPS/ch.xhtml",
                href: "ch.xhtml",
                body: "<p>body</p>",
            }],
            &[0],
            "<dc:title>Night Shift</dc:title><dc:creator>Jo Day</dc:creator>\
             <dc:language>en</dc:language>",
            "",
            &[],
        );
        let metadata = extract_metadata(&book).unwrap();
        assert_eq!(metadata.title, "Night Shift");
        assert_eq!(metadata.author, "Jo Day");
    }

    #[test]
    fn metadata_absent_reads_empty() {
        let metadata = extract_metadata(&spine_book()).unwrap();
        assert_eq!(metadata.title, "t");
        assert_eq!(metadata.author, "");
    }

    #[test]
    fn toc_is_empty_without_navigation() {
        assert!(extract_toc(&spine_book()).unwrap().is_empty());
    }

    #[test]
    fn toc_reads_the_ncx_map_with_nesting() {
        let book = build_epub_full(
            &[
                Chapter {
                    path: "OEBPS/one.xhtml",
                    href: "one.xhtml",
                    body: "<h1>One</h1>",
                },
                Chapter {
                    path: "OEBPS/two.xhtml",
                    href: "two.xhtml",
                    body: "<h1>Two</h1>",
                },
            ],
            &[0, 1],
            "<dc:title>t</dc:title>",
            "<item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n",
            &[Extra {
                path: "OEBPS/toc.ncx",
                body: "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\">\
                 <navMap><navPoint id=\"a\"><navLabel><text>First</text></navLabel>\
                 <content src=\"one.xhtml\"/><navPoint id=\"a1\"><navLabel><text>Deep</text>\
                 </navLabel><content src=\"one.xhtml#deep\"/></navPoint></navPoint>\
                 <navPoint id=\"b\"><navLabel><text>Second</text></navLabel>\
                 <content src=\"two.xhtml\"/></navPoint></navMap></ncx>",
            }],
        );
        let toc = extract_toc(&book).unwrap();
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].title, "First");
        assert_eq!(toc[0].href, "OEBPS/one.xhtml");
        assert_eq!(toc[0].children.len(), 1);
        assert_eq!(toc[0].children[0].title, "Deep");
        assert_eq!(toc[0].children[0].href, "OEBPS/one.xhtml");
        assert_eq!(toc[1].title, "Second");
        assert_eq!(toc[1].href, "OEBPS/two.xhtml");
        assert!(toc[1].children.is_empty());
    }

    #[test]
    fn toc_reads_the_epub3_nav_document() {
        let book = build_epub_full(
            &[Chapter {
                path: "OEBPS/ch.xhtml",
                href: "ch.xhtml",
                body: "<h1>One</h1><h2>Two</h2>",
            }],
            &[0],
            "<dc:title>t</dc:title>",
            "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" \
             properties=\"nav\"/>\n",
            &[Extra {
                path: "OEBPS/nav.xhtml",
                body: "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\">\
                 <body><nav><ol><li><a href=\"ch.xhtml\">One</a><ol>\
                 <li><a href=\"ch.xhtml#two\">Two</a></li></ol></li></ol></nav></body></html>",
            }],
        );
        let toc = extract_toc(&book).unwrap();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "One");
        assert_eq!(toc[0].href, "OEBPS/ch.xhtml");
        assert_eq!(toc[0].children.len(), 1);
        assert_eq!(toc[0].children[0].title, "Two");
        assert_eq!(toc[0].children[0].href, "OEBPS/ch.xhtml");
    }

    #[test]
    fn is_epub_sniffs_the_container() {
        assert!(is_epub(&spine_book()));
        assert!(!is_epub(b"%PDF-1.7 not an archive"));
        assert!(!is_epub(b"not a zip at all"));
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"no container here").unwrap();
        let plain = zip.finish().unwrap().into_inner();
        assert!(!is_epub(&plain));
    }

    #[test]
    fn to_pdf_bytes_renders_chapters_as_pages() {
        let pdf = to_pdf_bytes(&spine_book()).unwrap();
        let document = pdf_engine::PdfDocument::open(pdf).expect("the rendering opens");
        assert_eq!(document.page_count(), 3);
        let text = document.extract_all_text();
        for word in ["One", "first body", "Two", "second body", "Six"] {
            assert!(text.contains(word), "missing {word:?} in {text:?}");
        }
    }

    #[test]
    fn to_pdf_bytes_prefixes_list_items() {
        let pdf = to_pdf_bytes(&lists_tables_book()).unwrap();
        let text = pdf_engine::PdfDocument::open(pdf)
            .expect("the rendering opens")
            .extract_all_text();
        assert!(text.contains("- top"), "{text:?}");
        assert!(text.contains("1. first"), "{text:?}");
        assert!(text.contains("sold out"), "{text:?}");
    }

    #[test]
    fn missing_container_is_an_error() {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file("mimetype", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        match extract_epub(&bytes) {
            Err(EpubError::MissingFile(path)) => assert_eq!(path, "META-INF/container.xml"),
            other => panic!("expected MissingFile, got {other:?}"),
        }
    }
}
