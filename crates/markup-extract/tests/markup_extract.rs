//! End-to-end tests over inline fixtures, one per format.
//!
//! Each test asserts the feature it names -- a level, a span, a marker --
//! and fails when that mapping is removed.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use markup_extract::{html, markdown, text, BlockKind};

fn heading_level(block: &markup_extract::Block) -> Option<u8> {
    match block.kind {
        BlockKind::Heading { level } => Some(level),
        _ => None,
    }
}

fn list_item(block: &markup_extract::Block) -> Option<(u8, Option<u32>)> {
    match block.kind {
        BlockKind::ListItem { level, ordinal } => Some((level, ordinal)),
        _ => None,
    }
}

fn cell(block: &markup_extract::Block) -> Option<(u16, u16, u16, u16, bool)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => Some((row, column, row_span, column_span, header)),
        _ => None,
    }
}

#[test]
fn html_headings_carry_their_levels() {
    let blocks = html::extract_blocks("<h1>Title</h1><h2>Section</h2><h6>Fine</h6><p>Body.</p>");
    assert_eq!(blocks.len(), 4);
    assert_eq!(heading_level(&blocks[0]), Some(1));
    assert_eq!(heading_level(&blocks[1]), Some(2));
    assert_eq!(heading_level(&blocks[2]), Some(6));
    assert!(matches!(blocks[3].kind, BlockKind::Paragraph));
    assert_eq!(blocks[3].text, "Body.");

    let markdown = html::extract_markdown("<h1>Title</h1><p>Body.</p>");
    assert!(markdown.starts_with("# Title\n\nBody.\n"));
}

#[test]
fn html_lists_nest_and_number() {
    let blocks = html::extract_blocks(
        "<ul><li>one<ul><li>nested</li></ul></li><li>two</li></ul><ol start=\"3\"><li>a</li><li>b</li></ol>",
    );
    assert_eq!(blocks.len(), 5);
    assert_eq!(list_item(&blocks[0]), Some((0, None)));
    assert_eq!(blocks[0].text, "one");
    assert_eq!(list_item(&blocks[1]), Some((1, None)));
    assert_eq!(blocks[1].text, "nested");
    assert_eq!(list_item(&blocks[2]), Some((0, None)));
    assert_eq!(list_item(&blocks[3]), Some((0, Some(3))));
    assert_eq!(list_item(&blocks[4]), Some((0, Some(4))));
}

#[test]
fn html_li_text_around_nested_list_stays_one_item() {
    let blocks = html::extract_blocks("<ul><li>Before<ul><li>Nested</li></ul>After</li></ul>");
    assert_eq!(blocks.len(), 2);
    assert_eq!(list_item(&blocks[0]), Some((0, None)));
    assert_eq!(blocks[0].text, "Before After");
    assert_eq!(list_item(&blocks[1]), Some((1, None)));
    assert_eq!(blocks[1].text, "Nested");

    let markdown = html::extract_markdown("<ul><li>Before<ul><li>Nested</li></ul>After</li></ul>");
    assert!(markdown.contains("- Before After"), "{markdown}");
    assert!(!markdown.contains("\n\nAfter"), "{markdown}");
}

#[test]
fn html_tables_place_colspan_and_rowspan() {
    let blocks = html::extract_blocks(
        "<table><tr><th>Name</th><th colspan=\"2\">Scores</th></tr>\
         <tr><td rowspan=\"2\">Ann</td><td>1</td><td>2</td></tr>\
         <tr><td>3</td><td>4</td></tr></table>",
    );
    // Seven placed cells: the colspan and the rowspan each hold a slot no
    // block is emitted for.
    assert_eq!(blocks.len(), 7);
    assert_eq!(cell(&blocks[0]), Some((0, 0, 1, 1, true)));
    assert_eq!(cell(&blocks[1]), Some((0, 1, 1, 2, true)));
    assert_eq!(blocks[1].text, "Scores");
    assert_eq!(cell(&blocks[2]), Some((1, 0, 2, 1, false)));
    assert_eq!(blocks[2].text, "Ann");
    assert_eq!(cell(&blocks[5]), Some((2, 1, 1, 1, false)));
    assert_eq!(blocks[6].text, "4");
}

#[test]
fn html_code_links_and_runs_mark_inline() {
    let blocks = html::extract_blocks(
        "<pre><code class=\"language-rust\">fn main() {}</code></pre>\
         <p>See <a href=\"https://pdfluent.com\">us</a> and <strong>bold</strong> <em>talk</em>.</p>\
         <p><img src=\"pic.png\" alt=\"a picture\"></p>",
    );
    assert_eq!(blocks.len(), 3);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
    // Verbatim, never fenced: fences in the text come out backslashed from
    // the engine's renderer.
    assert_eq!(blocks[0].text, "fn main() {}");
    assert_eq!(
        blocks[1].text,
        "See [us](https://pdfluent.com) and **bold** *talk*."
    );
    assert_eq!(blocks[2].text, "![a picture](pic.png)");
}

#[test]
fn html_drops_script_style_nav_footer_and_del() {
    let blocks = html::extract_blocks(
        "<head><title>Hidden</title><style>p { }</style></head>\
         <nav>Links</nav><p>Kept.</p><footer>Page 1</footer>\
         <script>if (a < b) { alert(1); }</script><p>Gone <del>now</del> here.</p>",
    );
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Kept.");
    assert_eq!(blocks[1].text, "Gone here.");
}

#[test]
fn html_hidden_content_is_dropped() {
    let blocks = html::extract_blocks(
        "<p hidden>Secret survey</p>\
         <p aria-hidden=\"true\">Quiet survey</p>\
         <p style=\"display:none\">Gone survey</p>\
         <div style=\"color:red; visibility:hidden\">Hidden<div>Nested secret</div></div>\
         <p>Kept.</p>",
    );
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "Kept.");

    let markdown = html::extract_markdown("<p hidden>Secret</p><p>Kept.</p>");
    assert!(!markdown.contains("Secret"));
    assert!(markdown.contains("Kept."));
}

#[test]
fn html_nested_tables_are_their_own_tables() {
    let blocks = html::extract_blocks(
        "<table><tr><td>outer-a<table><tr><td>in-a</td><td>in-b</td></tr></table>\
         </td><td>outer-b</td></tr></table>",
    );
    assert_eq!(blocks.len(), 4);
    // The outer table first, its cell keeping its own text.
    assert_eq!(cell(&blocks[0]), Some((0, 0, 1, 1, false)));
    assert_eq!(blocks[0].text, "outer-a");
    assert_eq!(cell(&blocks[1]), Some((0, 1, 1, 1, false)));
    assert_eq!(blocks[1].text, "outer-b");
    // Then the nested table's cells, row-major.
    assert_eq!(cell(&blocks[2]), Some((0, 0, 1, 1, false)));
    assert_eq!(blocks[2].text, "in-a");
    assert_eq!(cell(&blocks[3]), Some((0, 1, 1, 1, false)));
    assert_eq!(blocks[3].text, "in-b");
}

#[test]
fn html_blockquote_and_pre_are_plain_blocks() {
    let quote = "The harbour survey records every beacon along the northern channel.";
    let code = "  keep   spacing\n  and lines";
    let blocks = html::extract_blocks(&format!(
        "<blockquote>{quote}</blockquote><pre>{code}</pre>"
    ));
    assert_eq!(blocks.len(), 2);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
    assert_eq!(blocks[0].text, quote);
    assert!(matches!(blocks[1].kind, BlockKind::Paragraph));
    assert_eq!(blocks[1].text, code);

    // No markdown syntax reaches the text, so the renderer escapes nothing.
    let markdown = html::extract_markdown(&format!(
        "<blockquote>{quote}</blockquote><pre>{code}</pre>"
    ));
    assert!(!markdown.contains("\\>"), "{markdown}");
    assert!(!markdown.contains('`'), "{markdown}");
    assert!(markdown.contains(quote), "{markdown}");
    assert!(markdown.contains("keep   spacing"), "{markdown}");
}

#[test]
fn html_blocks_carry_no_pages() {
    let blocks = html::extract_blocks("<h1>T</h1><p>x</p><ul><li>y</li></ul>");
    assert_eq!(blocks.len(), 3);
    for (at, block) in blocks.iter().enumerate() {
        assert_eq!(block.page, 0);
        assert_eq!(block.bbox, [0.0; 4]);
        assert_eq!(block.index, at);
    }
}

#[test]
fn markdown_headings_lists_code_and_links() {
    let blocks = markdown::extract_blocks(
        "# Title\n\n### Deep\n\n- one\n  - nested\n\n1. first\n\n```rust\nfn main() {}\n```\n\nSee [us](https://pdfluent.com) and **bold**.\n",
    );
    assert_eq!(blocks.len(), 7);
    assert_eq!(heading_level(&blocks[0]), Some(1));
    assert_eq!(heading_level(&blocks[1]), Some(3));
    assert_eq!(list_item(&blocks[2]), Some((0, None)));
    assert_eq!(list_item(&blocks[3]), Some((1, None)));
    assert_eq!(list_item(&blocks[4]), Some((0, Some(1))));
    assert_eq!(blocks[5].text, "```rust\nfn main() {}\n```");
    assert_eq!(
        blocks[6].text,
        "See [us](https://pdfluent.com) and **bold**."
    );
}

#[test]
fn markdown_tables_mark_their_head() {
    let blocks = markdown::extract_blocks("| Name | N |\n| --- | --- |\n| Ann | 1 |\n");
    assert_eq!(blocks.len(), 4);
    assert_eq!(cell(&blocks[0]), Some((0, 0, 1, 1, true)));
    assert_eq!(cell(&blocks[1]), Some((0, 1, 1, 1, true)));
    assert_eq!(cell(&blocks[2]), Some((1, 0, 1, 1, false)));
    assert_eq!(blocks[3].text, "1");

    let markdown = markdown::extract_markdown("| Name | N |\n| --- | --- |\n| Ann | 1 |\n");
    assert!(markdown.contains("| Name | N |"));
}

#[test]
fn text_splits_paragraphs_and_lists() {
    let blocks = text::extract_blocks(
        "Hello.\nStill hello.\n\n- one\n* two\n\n1. first\n2. second\n\nBye.\n",
    );
    assert_eq!(blocks.len(), 6);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
    assert_eq!(blocks[0].text, "Hello. Still hello.");
    assert_eq!(list_item(&blocks[1]), Some((0, None)));
    assert_eq!(blocks[1].text, "one");
    assert_eq!(list_item(&blocks[2]), Some((0, None)));
    assert_eq!(list_item(&blocks[3]), Some((0, Some(1))));
    assert_eq!(list_item(&blocks[4]), Some((0, Some(2))));
    assert_eq!(blocks[5].text, "Bye.");
}
