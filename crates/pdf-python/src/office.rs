//! Office extraction: `.docx`, `.pptx` and `.xlsx` to Markdown or Docling JSON.
//!
//! The reverse of the Office *export* on [`crate::PyDocument`]
//! (`to_docx`/`to_xlsx`/`to_pptx` turn a PDF into Office files): this turns
//! Office files into the same structured output the engine produces for PDFs,
//! through the `docx-extract`, `pptx-extract` and `xlsx-extract` crates.
//!
//! Routing is by the container's own `[Content_Types].xml`, not by the file
//! extension: a `.docx` that holds a workbook extracts as a workbook.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::{Cursor, Read};

use pdf_engine::{Block, DoclingPage};
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::{PdfluentIoError, PdfluentParseError, PdfluentValidationError};

/// The Office formats this binding reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeKind {
    /// Word: `wordprocessingml` in `[Content_Types].xml`.
    Docx,
    /// PowerPoint: `presentationml` in `[Content_Types].xml`.
    Pptx,
    /// Excel: `spreadsheetml` in `[Content_Types].xml`.
    Xlsx,
}

/// Which Office format `bytes` hold, or `None` when it is not an Office file.
///
/// Reads only `[Content_Types].xml` out of the ZIP container. A non-ZIP file,
/// a ZIP without that part, or one naming no Office content type is not an
/// Office file — the caller reports that rather than guessing from the name.
///
/// Routing is by the MAIN document part (`/word/document.xml`,
/// `/ppt/presentation.xml`, `/xl/workbook.xml`, or a `...main+xml` content
/// type), never by a substring over the whole file: a `.docx` holding an
/// embedded chart or spreadsheet names `spreadsheetml` for that part, and the
/// old substring check sent the whole document to the xlsx extractor.
pub fn detect(bytes: &[u8]) -> Option<OfficeKind> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut part = archive.by_name("[Content_Types].xml").ok()?;
    let mut xml = String::new();
    part.read_to_string(&mut xml).ok()?;
    detect_from_content_types(&xml)
}

/// Which Office format a `[Content_Types].xml` body names, by main part.
fn detect_from_content_types(xml: &str) -> Option<OfficeKind> {
    // Main part names first: exact, and immune to whatever else the container
    // embeds (charts, embedded workbooks, OLE objects).
    if xml.contains("/word/document.xml") {
        return Some(OfficeKind::Docx);
    }
    if xml.contains("/ppt/presentation.xml") {
        return Some(OfficeKind::Pptx);
    }
    if xml.contains("/xl/workbook.xml") {
        return Some(OfficeKind::Xlsx);
    }
    // Then the main content types: only `...main+xml` counts, so an embedded
    // `spreadsheetml.sheet` or a `drawingml.chart` part cannot misroute.
    if xml.contains("wordprocessingml") && xml.contains(".main+xml") {
        return Some(OfficeKind::Docx);
    }
    if xml.contains("presentationml") && xml.contains(".main+xml") {
        return Some(OfficeKind::Pptx);
    }
    if xml.contains("spreadsheetml") && xml.contains(".main+xml") {
        return Some(OfficeKind::Xlsx);
    }
    None
}

/// The blocks of an Office file, in the engine's own block model.
fn blocks_of(kind: OfficeKind, bytes: &[u8]) -> Result<Vec<Block>, String> {
    match kind {
        OfficeKind::Docx => docx_extract::extract_blocks(bytes).map_err(|e| e.to_string()),
        OfficeKind::Pptx => pptx_extract::extract_blocks(bytes).map_err(|e| e.to_string()),
        OfficeKind::Xlsx => xlsx_extract::extract_blocks(bytes).map_err(|e| e.to_string()),
    }
}

/// One synthetic page per distinct block page, at US Letter.
///
/// Office files have no page sizes: Word has no pages at all (every block is
/// on page 0), slides and sheets are indexed, not measured. Docling accepts
/// blocks on pages missing from `pages`, but listing the touched pages at a
/// stated size is more useful downstream than an empty list, and the size is
/// documented here rather than smuggled in as measured.
fn synthetic_pages(blocks: &[Block]) -> Vec<DoclingPage> {
    let mut seen = std::collections::BTreeSet::new();
    for block in blocks {
        seen.insert(block.page);
    }
    seen.into_iter()
        .map(|page| DoclingPage {
            page,
            width: 612.0,
            height: 792.0,
        })
        .collect()
}

/// Extract an Office file (`.docx`, `.pptx`, `.xlsx`) to Markdown or Docling.
///
/// Parameters
/// ----------
/// source : str or bytes
///     File path or raw file bytes. Routing is by the container's content
///     types, not by the extension.
/// format : str, default "markdown"
///     `"markdown"` for the engine's Markdown, `"docling"` for a
///     DoclingDocument as JSON (load it with
///     `DoclingDocument.model_validate_json(...)`).
/// name : str, default "document"
///     `DoclingDocument.name` for `format="docling"`, which Docling fills
///     with the file's stem. Ignored for Markdown.
///
/// Raises
/// ------
/// PdfluentParseError
///     If the bytes are not an Office file, or the Office file is broken.
/// PdfluentValidationError
///     If `format` is neither `"markdown"` nor `"docling"`.
/// PdfluentIoError
///     If a file path is given and the file cannot be read.
#[pyfunction]
#[pyo3(signature = (source, format = "markdown", name = "document"))]
pub fn extract_office(source: &Bound<'_, PyAny>, format: &str, name: &str) -> PyResult<String> {
    if format != "markdown" && format != "docling" {
        return Err(PdfluentValidationError::new_err(format!(
            "format must be \"markdown\" or \"docling\", not {format:?}"
        )));
    }
    let data: Vec<u8> = if let Ok(path_str) = source.extract::<String>() {
        let path = std::path::PathBuf::from(&path_str);
        std::fs::read(&path).map_err(|e| PdfluentIoError::new_err(format!("{path_str}: {e}")))?
    } else if let Ok(bytes) = source.extract::<Vec<u8>>() {
        bytes
    } else {
        return Err(PdfluentParseError::new_err(
            "source must be a file path (str) or bytes",
        ));
    };
    let kind = detect(&data).ok_or_else(|| {
        PdfluentParseError::new_err(
            "not an Office file: no wordprocessingml, presentationml or \
             spreadsheetml content type in the container",
        )
    })?;
    let blocks = blocks_of(kind, &data)
        .map_err(|e| PdfluentParseError::new_err(format!("invalid Office file: {e}")))?;
    if format == "docling" {
        let pages = synthetic_pages(&blocks);
        Ok(pdf_engine::to_docling_json(name, &pages, &blocks))
    } else {
        Ok(pdf_engine::to_markdown(&blocks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal container naming one Office content type, with the one part
    /// the corresponding extractor cannot do without.
    fn container(content_type: &str, part_name: &str, part_body: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut part = |name: &str, content: &str| {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        };
        part(
            "[Content_Types].xml",
            &format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/{part_name}" ContentType="{content_type}"/></Types>"#
            ),
        );
        part(part_name, part_body);
        zip.finish().unwrap().into_inner()
    }

    const WORD_XML: &str = r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello docx.</w:t></w:r></w:p></w:body></w:document>"#;

    /// A container whose `[Content_Types].xml` carries several overrides.
    fn container_multi(overrides: &[(&str, &str)], parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut types = String::from(
            r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        );
        for (part_name, content_type) in overrides {
            types.push_str(&format!(
                r#"<Override PartName="/{part_name}" ContentType="{content_type}"/>"#
            ));
        }
        types.push_str("</Types>");
        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(types.as_bytes()).unwrap();
        for (name, content) in parts {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn detect_routes_on_content_types_not_the_container() {
        let docx = container(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            "word/document.xml",
            WORD_XML,
        );
        assert_eq!(detect(&docx), Some(OfficeKind::Docx));
        let xlsx = container(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
            "xl/workbook.xml",
            "<workbook/>",
        );
        assert_eq!(detect(&xlsx), Some(OfficeKind::Xlsx));
        let pptx = container(
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            "ppt/presentation.xml",
            "<presentation/>",
        );
        assert_eq!(detect(&pptx), Some(OfficeKind::Pptx));
    }

    #[test]
    fn detect_routes_a_docx_with_an_embedded_chart_by_its_main_part() {
        // The old substring check saw `spreadsheetml` in the embedded sheet
        // part and sent the document to the xlsx extractor.
        let docx = container_multi(
            &[
                (
                    "word/document.xml",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                ),
                (
                    "word/embeddings/Microsoft_Excel_Worksheet1.xlsx",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                ),
                (
                    "word/charts/chart1.xml",
                    "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
                ),
            ],
            &[("word/document.xml", WORD_XML)],
        );
        assert_eq!(detect(&docx), Some(OfficeKind::Docx));
    }

    #[test]
    fn detect_routes_a_pptx_with_an_embedded_chart_by_its_main_part() {
        let pptx = container_multi(
            &[
                (
                    "ppt/presentation.xml",
                    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
                ),
                (
                    "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                ),
            ],
            &[("ppt/presentation.xml", "<presentation/>")],
        );
        assert_eq!(detect(&pptx), Some(OfficeKind::Pptx));
    }

    #[test]
    fn detect_refuses_a_pdf_a_plain_zip_and_garbage() {
        assert_eq!(detect(b"%PDF-1.7 garbage"), None);
        assert_eq!(detect(b"not a zip at all"), None);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"no content types here").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(detect(&bytes), None);
    }

    #[test]
    fn docx_bytes_come_out_as_markdown() {
        let docx = container(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            "word/document.xml",
            WORD_XML,
        );
        let blocks = blocks_of(OfficeKind::Docx, &docx).expect("blocks");
        assert_eq!(blocks.len(), 1);
        let markdown = pdf_engine::to_markdown(&blocks);
        assert!(markdown.contains("Hello docx."), "{markdown:?}");
    }

    #[test]
    fn synthetic_pages_cover_every_touched_page_once() {
        let blocks = vec![
            Block::new(
                pdf_engine::BlockKind::Paragraph,
                0,
                [0.0; 4],
                "a".to_string(),
                0,
            ),
            Block::new(
                pdf_engine::BlockKind::Paragraph,
                2,
                [0.0; 4],
                "b".to_string(),
                1,
            ),
            Block::new(
                pdf_engine::BlockKind::Paragraph,
                0,
                [0.0; 4],
                "c".to_string(),
                2,
            ),
        ];
        let pages = synthetic_pages(&blocks);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].page, 0);
        assert_eq!(pages[1].page, 2);
    }
}
