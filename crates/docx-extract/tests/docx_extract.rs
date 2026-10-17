//! End-to-end tests over .docx files the tests build themselves.
//!
//! Each fixture is a minimal ZIP container written in code: no downloads, no
//! checked-in binaries. Every test asserts the feature it names, and fails
//! when that mapping is removed.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::{Cursor, Write};

use docx_extract::{extract_blocks, extract_markdown, BlockKind};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WP_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

/// Wrap body XML in a minimal `w:document` envelope.
fn document(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="{WP_NS}" xmlns:a="{A_NS}" xmlns:pic="{PIC_NS}">
<w:body>{body}</w:body></w:document>"#
    )
}

/// Build a minimal .docx container in memory. `rels`, `numbering` and
/// `styles` are the full XML of their parts; `None` leaves the part out of
/// the container.
fn build_docx(
    body: &str,
    rels: Option<&str>,
    numbering: Option<&str>,
    styles: Option<&str>,
) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut part = |name: &str, content: &str| {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    };
    part(
        "[Content_Types].xml",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    );
    part(
        "_rels/.rels",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#,
    );
    part("word/document.xml", &document(body));
    if let Some(rels) = rels {
        part("word/_rels/document.xml.rels", rels);
    }
    if let Some(numbering) = numbering {
        part("word/numbering.xml", numbering);
    }
    if let Some(styles) = styles {
        part("word/styles.xml", styles);
    }
    zip.finish().unwrap().into_inner()
}

fn rels(relationships: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
    )
}

fn heading_kind(block: &docx_extract::Block) -> Option<u8> {
    match block.kind {
        BlockKind::Heading { level } => Some(level),
        _ => None,
    }
}

fn list_kind(block: &docx_extract::Block) -> Option<(u8, Option<u32>)> {
    match block.kind {
        BlockKind::ListItem { level, ordinal } => Some((level, ordinal)),
        _ => None,
    }
}

fn cell_kind(block: &docx_extract::Block) -> Option<(u16, u16, u16, u16)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            ..
        } => Some((row, column, row_span, column_span)),
        _ => None,
    }
}

#[test]
fn headings_come_from_styles_and_outline_levels() {
    let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Section</w:t></w:r></w:p>
<w:p><w:r><w:t>Body text.</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Normal"/><w:outlineLvl w:val="2"/></w:pPr><w:r><w:t>Outline three</w:t></w:r></w:p>
"#;
    let blocks = extract_blocks(&build_docx(body, None, None, None)).unwrap();
    assert_eq!(blocks.len(), 4);
    assert_eq!(heading_kind(&blocks[0]), Some(1));
    assert_eq!(heading_kind(&blocks[1]), Some(2));
    assert!(matches!(blocks[2].kind, BlockKind::Paragraph));
    // A direct outline level wins over the (non-heading) style.
    assert_eq!(heading_kind(&blocks[3]), Some(3));
    assert_eq!(blocks[3].text, "Outline three");

    let markdown = extract_markdown(&build_docx(body, None, None, None)).unwrap();
    assert!(markdown.starts_with("# Title\n\n## Section\n\nBody text.\n\n### Outline three\n"));
}

#[test]
fn lists_nest_and_number_from_numbering_xml() {
    let numbering = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<w:numbering xmlns:w="{W_NS}">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#
    );
    let item = |num: u32, level: u8, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num}"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        "{}{}{}{}{}",
        item(1, 0, "Bullet one"),
        item(1, 1, "Nested bullet"),
        item(2, 0, "First"),
        item(2, 1, "Sub"),
        item(2, 0, "Second"),
    );
    let docx = build_docx(&body, None, Some(&numbering), None);
    let blocks = extract_blocks(&docx).unwrap();
    assert_eq!(blocks.len(), 5);
    assert_eq!(list_kind(&blocks[0]), Some((0, None)));
    assert_eq!(list_kind(&blocks[1]), Some((1, None)));
    assert_eq!(list_kind(&blocks[2]), Some((0, Some(1))));
    assert_eq!(list_kind(&blocks[3]), Some((1, Some(1))));
    assert_eq!(list_kind(&blocks[4]), Some((0, Some(2))));

    let markdown = extract_markdown(&docx).unwrap();
    assert!(markdown.contains("- Bullet one\n  - Nested bullet\n"));
    assert!(markdown.contains("1. First\n   1. Sub\n"));
    assert!(markdown.contains("2. Second\n"));
}

#[test]
fn tables_map_grid_span_and_vertical_merge_to_spans() {
    let cell = |content: &str, props: &str| {
        format!(
            r#"<w:tc><w:tcPr>{props}</w:tcPr><w:p><w:r><w:t>{content}</w:t></w:r></w:p></w:tc>"#
        )
    };
    let body = format!(
        r"<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        cell("Across", r#"<w:gridSpan w:val="2"/>"#),
        cell("Top right", ""),
        cell("Tall", r#"<w:vMerge w:val="restart"/>"#),
        cell("Middle", ""),
        cell("Right", ""),
        cell("", r#"<w:vMerge/>"#),
        cell("Bottom", ""),
        cell("Bottom right", ""),
    );
    let blocks = extract_blocks(&build_docx(&body, None, None, None)).unwrap();
    let cells: Vec<(u16, u16, u16, u16)> = blocks.iter().map(|b| cell_kind(b).unwrap()).collect();
    assert_eq!(
        cells,
        vec![
            (0, 0, 1, 2), // "Across" spans two columns.
            (0, 2, 1, 1), // ... so this one sits in column 2.
            (1, 0, 2, 1), // "Tall" spans two rows; its continuation places no cell.
            (1, 1, 1, 1),
            (1, 2, 1, 1),
            (2, 1, 1, 1), // Column 0 of row 2 belongs to "Tall".
            (2, 2, 1, 1),
        ]
    );
    assert_eq!(blocks[0].text, "Across");
    assert_eq!(blocks[2].text, "Tall");

    // Spanned tables serialize as HTML, which can say colspan/rowspan.
    let markdown = extract_markdown(&build_docx(&body, None, None, None)).unwrap();
    assert!(markdown.contains("<table>"));
    assert!(markdown.contains(r#"colspan="2""#));
    assert!(markdown.contains(r#"rowspan="2""#));
}

#[test]
fn bold_italic_and_hyperlinks_become_inline_markdown() {
    let body = r#"
<w:p>
  <w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r>
  <w:r><w:t xml:space="preserve"> and </w:t></w:r>
  <w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r>
  <w:r><w:t xml:space="preserve"> and </w:t></w:r>
  <w:hyperlink r:id="rId5"><w:r><w:t>Example</w:t></w:r></w:hyperlink>
</w:p>
"#;
    let rels = rels(
        r#"<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/docs" TargetMode="External"/>"#,
    );
    let blocks = extract_blocks(&build_docx(body, Some(&rels), None, None)).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        blocks[0].text,
        "**Bold** and *italic* and [Example](https://example.com/docs)"
    );

    let markdown = extract_markdown(&build_docx(body, Some(&rels), None, None)).unwrap();
    assert_eq!(
        markdown,
        "**Bold** and *italic* and [Example](https://example.com/docs)\n"
    );
}

#[test]
fn images_are_references_and_blocks_carry_no_pages() {
    let body = r#"
<w:p>
  <w:r><w:t xml:space="preserve">See </w:t></w:r>
  <w:r><w:drawing><wp:inline>
    <wp:docPr id="1" name="Figure" descr="A cat"/>
    <a:graphic><a:graphicData><pic:pic><pic:blipFill>
      <a:blip r:embed="rId6"/>
    </pic:blipFill></pic:pic></a:graphicData></a:graphic>
  </wp:inline></w:drawing></w:r>
</w:p>
"#;
    let rels = rels(
        r#"<Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#,
    );
    let docx = build_docx(body, Some(&rels), None, None);
    let blocks = extract_blocks(&docx).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "See ![A cat](media/image1.png)");
    assert_eq!(blocks[0].page, 0);
    assert_eq!(blocks[0].bbox, [0.0; 4]);

    // Word has no pages, so the serializer never owes a page marker.
    let markdown = extract_markdown(&docx).unwrap();
    assert!(!markdown.contains("<!-- page"));
}

/// A `styles.xml` part pinning the heading and list resolutions: a basedOn
/// chain onto `Heading2`, a style named `Heading 4`, `Title`, a style cycle,
/// and a style-carried `numPr` with a chained child.
fn styles_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="{W_NS}">
  <w:style w:styleId="Heading2"><w:name w:val="heading 2"/>
    <w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
  <w:style w:styleId="C1Harbour"><w:name w:val="C1Harbour"/>
    <w:basedOn w:val="Heading2"/></w:style>
  <w:style w:styleId="C2Harbour"><w:name w:val="C2Harbour"/>
    <w:basedOn w:val="C1Harbour"/></w:style>
  <w:style w:styleId="CustomHarbour"><w:name w:val="Heading 4"/></w:style>
  <w:style w:styleId="Title"><w:name w:val="Title"/></w:style>
  <w:style w:styleId="CycA"><w:basedOn w:val="CycB"/></w:style>
  <w:style w:styleId="CycB"><w:basedOn w:val="CycA"/></w:style>
  <w:style w:styleId="CListHarbour"><w:pPr><w:numPr>
    <w:ilvl w:val="0"/><w:numId w:val="5"/>
  </w:numPr></w:pPr></w:style>
  <w:style w:styleId="CListChild"><w:basedOn w:val="CListHarbour"/></w:style>
</w:styles>"#
    )
}

fn numbering_with_none() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<w:numbering xmlns:w="{W_NS}">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="none"/></w:lvl>
  </w:abstractNum>
  <w:num w:numId="5"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="6"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#
    )
}

#[test]
fn headings_resolve_through_style_chains_names_and_title() {
    let para = |style: &str, outline: Option<u8>, text: &str| {
        let outline = outline.map_or(String::new(), |v| format!(r#"<w:outlineLvl w:val="{v}"/>"#));
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/>{outline}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        "{}{}{}{}{}{}",
        para("C1Harbour", None, "Chain one"),
        para("C2Harbour", None, "Chain two"),
        para("C2Harbour", Some(0), "Direct beats chain"),
        para("CustomHarbour", None, "Named four"),
        para("Title", None, "Doc title"),
        para("CycA", None, "Cyclic stays plain"),
    );
    let docx = build_docx(&body, None, None, Some(&styles_xml()));
    let blocks = extract_blocks(&docx).unwrap();
    assert_eq!(blocks.len(), 6);
    assert_eq!(heading_kind(&blocks[0]), Some(2));
    assert_eq!(heading_kind(&blocks[1]), Some(2));
    assert_eq!(heading_kind(&blocks[2]), Some(1));
    assert_eq!(heading_kind(&blocks[3]), Some(4));
    assert_eq!(heading_kind(&blocks[4]), Some(1));
    assert!(matches!(blocks[5].kind, BlockKind::Paragraph));

    let markdown = extract_markdown(&docx).unwrap();
    assert!(markdown.contains("## Chain one\n\n## Chain two\n\n# Direct beats chain\n"));
    assert!(markdown.contains("#### Named four\n\n# Doc title\n\nCyclic stays plain\n"));
}

#[test]
fn lists_resolve_through_style_num_pr_with_none_and_zero_plain() {
    let styled = |style: &str, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    let direct = |num: u32, level: u8, text: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num}"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#
        )
    };
    // A direct numPr wins over the style's: level 1 of list 6 marks
    // "none", so this paragraph stays plain.
    let direct_none = r#"<w:p><w:pPr><w:pStyle w:val="CListHarbour"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="6"/></w:numPr></w:pPr><w:r><w:t>Direct none beats style</w:t></w:r></w:p>"#;
    // An outline level wins over numbering, direct or styled alike.
    let styled_heading = r#"<w:p><w:pPr><w:pStyle w:val="CListHarbour"/><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>Numbered heading</w:t></w:r></w:p>"#;
    let direct_both = r#"<w:p><w:pPr><w:outlineLvl w:val="2"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="5"/></w:numPr></w:pPr><w:r><w:t>Direct both</w:t></w:r></w:p>"#;
    let body = format!(
        "{}{}{}{}{}{}{}",
        styled("CListHarbour", "Styled bullet"),
        styled("CListChild", "Chained bullet"),
        direct_none,
        direct(0, 0, "Zero is no list"),
        direct(6, 1, "Direct none stays plain"),
        styled_heading,
        direct_both,
    );
    let docx = build_docx(
        &body,
        None,
        Some(&numbering_with_none()),
        Some(&styles_xml()),
    );
    let blocks = extract_blocks(&docx).unwrap();
    assert_eq!(blocks.len(), 7);
    assert_eq!(list_kind(&blocks[0]), Some((0, None)));
    assert_eq!(list_kind(&blocks[1]), Some((0, None)));
    assert!(matches!(blocks[2].kind, BlockKind::Paragraph));
    assert!(matches!(blocks[3].kind, BlockKind::Paragraph));
    assert!(matches!(blocks[4].kind, BlockKind::Paragraph));
    assert_eq!(heading_kind(&blocks[5]), Some(2));
    assert_eq!(heading_kind(&blocks[6]), Some(3));

    let markdown = extract_markdown(&docx).unwrap();
    assert!(markdown.contains("- Styled bullet\n- Chained bullet\n"));
    assert!(markdown.contains("## Numbered heading\n\n### Direct both\n"));
}
