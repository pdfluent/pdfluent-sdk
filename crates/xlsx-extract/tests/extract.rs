//! Extraction tests over workbooks the tests build themselves.
//!
//! Each fixture is a zip archive written in-memory: no downloads, no checked-in
//! binaries. Every test asserts its feature so that removing the feature fails
//! the test (shared strings would read as indices, dates as serials, and so on).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Write;

use pdf_engine::{Block, BlockKind};
use xlsx_extract::{
    extract_blocks, extract_blocks_with_limits, extract_workbook, extract_workbook_with_limits,
    ExtractLimits,
};

/// One sheet of a fixture: tab name plus the `<sheetData>` rows XML.
type FixtureSheet<'a> = (&'a str, String);

/// Build a workbook's bytes from sheets, an optional shared-string table and
/// optional styles. The archive holds exactly the parts the extractor reads.
fn workbook_bytes(sheets: &[FixtureSheet], shared: Option<&str>, styles: Option<&str>) -> Vec<u8> {
    let mut entries: Vec<(String, String)> = Vec::new();

    let sheet_refs: String = sheets
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            format!(
                r#"<sheet name="{name}" sheetId="{}" r:id="rId{}"/>"#,
                i + 1,
                i + 1
            )
        })
        .collect();
    entries.push((
        "xl/workbook.xml".to_string(),
        format!(
            r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_refs}</sheets></workbook>"#
        ),
    ));

    let rels: String = (1..=sheets.len())
        .map(|i| {
            format!(
                r#"<Relationship Id="rId{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i}.xml"/>"#
            )
        })
        .collect();
    entries.push((
        "xl/_rels/workbook.xml.rels".to_string(),
        format!(
            r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
        ),
    ));

    for (i, (_, rows)) in sheets.iter().enumerate() {
        entries.push((
            format!("xl/worksheets/sheet{}.xml", i + 1),
            format!(
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
            ),
        ));
    }

    if let Some(shared) = shared {
        entries.push(("xl/sharedStrings.xml".to_string(), shared.to_string()));
    }
    if let Some(styles) = styles {
        entries.push(("xl/styles.xml".to_string(), styles.to_string()));
    }

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in &entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A worksheet body with rows plus a `<mergeCell>` section.
fn sheet_with_merges(rows: &str, merges: &[&str]) -> String {
    let cells: String = merges
        .iter()
        .map(|range| format!(r#"<mergeCell ref="{range}"/>"#))
        .collect();
    format!("{rows}<mergeCells>{cells}</mergeCells>")
}

/// A worksheet body that also carries a (possibly lying) `<dimension>`.
fn sheet_with_dimension(dimension: &str, rows: &str) -> String {
    format!(r#"<dimension ref="{dimension}"/>{rows}"#)
}

/// The heading texts in block order.
fn headings(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter(|block| matches!(block.kind, BlockKind::Heading { .. }))
        .map(|block| block.text.as_str())
        .collect()
}

/// The table cell at `(page, row, column)`, if one was emitted.
fn cell(blocks: &[Block], page: usize, row: u16, column: u16) -> Option<&Block> {
    blocks.iter().find(|block| {
        block.page == page
            && matches!(
                block.kind,
                BlockKind::TableCell { row: r, column: c, .. } if r == row && c == column
            )
    })
}

/// The `(row_span, column_span)` of a table cell block.
fn span(block: &Block) -> (u16, u16) {
    match block.kind {
        BlockKind::TableCell {
            row_span,
            column_span,
            ..
        } => (row_span, column_span),
        _ => panic!("expected a table cell, got {:?}", block.kind),
    }
}

/// Grid extent as `(rows, columns)` from the table cells on one page.
fn grid(blocks: &[Block], page: usize) -> (u16, u16) {
    let mut rows = 0;
    let mut cols = 0;
    for block in blocks.iter().filter(|block| block.page == page) {
        if let BlockKind::TableCell { row, column, .. } = block.kind {
            rows = rows.max(row + 1);
            cols = cols.max(column + 1);
        }
    }
    (rows, cols)
}

#[test]
fn shared_strings_numbers_and_booleans_resolve_to_text() {
    let shared = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2"><si><t>Name</t></si><si><r><t>Ad</t></r><r><t>a</t></r></si></sst>"#;
    let rows = concat!(
        r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>"#,
        r#"<row r="2"><c r="A2"><v>42</v></c><c r="B2" t="b"><v>1</v></c><c r="C2" t="b"><v>0</v></c></row>"#,
    );
    let data = workbook_bytes(&[("Data", rows.to_string())], Some(shared), None);
    let blocks = extract_blocks(&data).unwrap();

    assert_eq!(headings(&blocks), ["Data"]);
    // Without shared-string support these read as the indices "0" and "1".
    assert_eq!(cell(&blocks, 0, 0, 0).unwrap().text, "Name");
    assert_eq!(cell(&blocks, 0, 0, 1).unwrap().text, "Ada");
    assert_eq!(cell(&blocks, 0, 1, 0).unwrap().text, "42");
    assert_eq!(cell(&blocks, 0, 1, 1).unwrap().text, "TRUE");
    assert_eq!(cell(&blocks, 0, 1, 2).unwrap().text, "FALSE");

    let markdown = pdf_engine::to_markdown(&blocks);
    assert!(
        markdown.contains("# Data"),
        "headings serialize: {markdown}"
    );
    assert!(
        markdown.contains("| Name | Ada |"),
        "pipe table: {markdown}"
    );
}

#[test]
fn merged_cells_emit_spans_and_skip_covered_cells() {
    let rows = concat!(
        r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Title</t></is></c></row>"#,
        r#"<row r="2"><c r="A2" t="inlineStr"><is><t>x</t></is></c><c r="B2" t="inlineStr"><is><t>y</t></is></c></row>"#,
    );
    let body = sheet_with_merges(rows, &["A1:B1"]);
    let data = workbook_bytes(&[("Merged", body)], None, None);
    let blocks = extract_blocks(&data).unwrap();

    let head = cell(&blocks, 0, 0, 0).unwrap();
    assert_eq!(head.text, "Title");
    // Without merge support this is (1, 1) and (0, 1) exists as an empty cell.
    assert_eq!(span(head), (1, 2));
    assert!(
        cell(&blocks, 0, 0, 1).is_none(),
        "covered cells emit no block"
    );
    assert_eq!(cell(&blocks, 0, 1, 0).unwrap().text, "x");
    assert_eq!(cell(&blocks, 0, 1, 1).unwrap().text, "y");

    let markdown = pdf_engine::to_markdown(&blocks);
    assert!(
        markdown.contains(r#"colspan="2""#),
        "spans need HTML: {markdown}"
    );
}

#[test]
fn formulas_show_the_cached_value_and_expose_their_text() {
    let rows = concat!(
        r#"<row r="1"><c r="A1"><v>10</v></c></row>"#,
        r#"<row r="2"><c r="A2"><v>20</v></c></row>"#,
        r#"<row r="3"><c r="A3"><f>SUM(A1:A2)</f><v>30</v></c></row>"#,
    );
    let data = workbook_bytes(&[("Calc", rows.to_string())], None, None);

    let blocks = extract_blocks(&data).unwrap();
    // Without cached-value support this is empty (a bare `<f>` holds no text node).
    assert_eq!(cell(&blocks, 0, 2, 0).unwrap().text, "30");

    let workbook = extract_workbook(&data).unwrap();
    assert_eq!(workbook.sheets.len(), 1);
    let formulas = &workbook.sheets[0].formulas;
    assert_eq!(formulas.len(), 1);
    assert_eq!(formulas[0].sheet, "Calc");
    assert_eq!((formulas[0].row, formulas[0].column), (2, 0));
    assert_eq!(formulas[0].formula, "SUM(A1:A2)");
    assert_eq!(formulas[0].cached, "30");

    let markdown = pdf_engine::to_markdown(&blocks);
    assert!(
        !markdown.contains("SUM("),
        "formulae never leak into text: {markdown}"
    );
}

#[test]
fn date_styled_serials_format_iso_and_plain_serials_stay_numbers() {
    let styles = concat!(
        r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
        r#"<cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="22"/></cellXfs>"#,
        r#"</styleSheet>"#,
    );
    let rows = concat!(
        r#"<row r="1"><c r="A1" s="1"><v>44927</v></c>"#,
        r#"<c r="B1"><v>44927</v></c>"#,
        r#"<c r="C1" s="2"><v>45161.5</v></c></row>"#,
    );
    let data = workbook_bytes(&[("Dates", rows.to_string())], None, Some(styles));
    let blocks = extract_blocks(&data).unwrap();

    // Without date support A1 reads as the serial "44927".
    assert_eq!(cell(&blocks, 0, 0, 0).unwrap().text, "2023-01-01");
    // ... and without the style check B1 would wrongly read as a date too.
    assert_eq!(cell(&blocks, 0, 0, 1).unwrap().text, "44927");
    assert_eq!(cell(&blocks, 0, 0, 2).unwrap().text, "2023-08-23T12:00:00");
}

#[test]
fn two_sheets_emit_two_headings_and_trim_trailing_empties() {
    let alpha = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c></row>"#.to_string();
    // The dimension claims a hundred rows; only A1:B2 holds content.
    let beta = sheet_with_dimension(
        "A1:Z100",
        concat!(
            r#"<row r="1"><c r="A1" t="inlineStr"><is><t>b</t></is></c></row>"#,
            r#"<row r="2"><c r="B2"><v>7</v></c></row>"#,
        ),
    );
    let data = workbook_bytes(&[("Alpha", alpha), ("Beta", beta)], None, None);
    let blocks = extract_blocks(&data).unwrap();

    assert_eq!(headings(&blocks), ["Alpha", "Beta"]);
    assert_eq!(grid(&blocks, 0), (1, 1));
    // Without trimming, the lying dimension yields a 100-row grid.
    assert_eq!(grid(&blocks, 1), (2, 2));
    assert_eq!(cell(&blocks, 1, 0, 0).unwrap().text, "b");
    assert_eq!(cell(&blocks, 1, 0, 1).unwrap().text, "");
    assert_eq!(cell(&blocks, 1, 1, 1).unwrap().text, "7");

    let markdown = pdf_engine::to_markdown(&blocks);
    let alpha_at = markdown.find("# Alpha").expect("first heading");
    let beta_at = markdown.find("# Beta").expect("second heading");
    assert!(alpha_at < beta_at, "sheet order survives: {markdown}");
}

#[test]
fn the_cell_cap_cuts_whole_rows_and_leaves_a_note() {
    let alpha = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c></row>"#.to_string();
    let beta = concat!(
        r#"<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row>"#,
        r#"<row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>4</v></c></row>"#,
    )
    .to_string();
    let data = workbook_bytes(&[("Alpha", alpha), ("Beta", beta)], None, None);
    // Beta's 2x2 grid is 4 cells; a cap of 3 keeps one whole row.
    let limits = ExtractLimits {
        max_cells_per_sheet: 3,
    };
    let workbook = extract_workbook_with_limits(&data, &limits).unwrap();

    assert!(!workbook.sheets[0].truncated);
    let beta = &workbook.sheets[1];
    assert!(beta.truncated, "the cap bites without a note otherwise");
    // Heading + two cells + the truncation paragraph; row 2 is gone whole.
    assert_eq!(beta.blocks.len(), 4);
    assert!(cell(&beta.blocks, 1, 1, 0).is_none());
    let note = beta.blocks.last().unwrap();
    assert!(matches!(note.kind, BlockKind::Paragraph));
    assert!(
        note.text.contains("first 1 of 2 rows"),
        "note says what was cut: {}",
        note.text
    );
    assert!(
        note.text.contains("3 cell limit"),
        "note names the cap: {}",
        note.text
    );

    // The flat shorthand honors the same limits.
    let flat = extract_blocks_with_limits(&data, &limits).unwrap();
    assert!(flat.iter().any(|block| {
        matches!(block.kind, BlockKind::Paragraph) && block.text.contains("first 1 of 2 rows")
    }));
}
