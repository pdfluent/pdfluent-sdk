// Test-mode dead_code: these `pub fn`s are used by the Node binding via
// the `#[napi]` macro, but `cargo test` compiles without that macro
// generating callers, so the symbols look "never used".
#![allow(dead_code)]

//! Top-level convenience functions exposed to Node.js.
//!
//! These are module-level exports (not methods on a class) that cover the
//! most common path-based workflows:
//!   - `openPdf(path)`    → open a PDF file by path
//!   - `mergePdfs(paths, outputPath)` → merge multiple PDFs into one file
//!   - `validatePdfA(path, level)`    → validate a file against a PDF/A level

use crate::document::{compliance_to_info, parse_pdfa_level, ComplianceReportInfo, PdfDocument};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdf_engine::PdfDocument as RustDocument;

/// Open a PDF from a file path.
///
/// Convenience wrapper around `PdfDocument.open` for path-based workflows.
#[napi]
pub fn open_pdf(path: String) -> Result<PdfDocument> {
    let bytes = std::fs::read(&path)
        .map_err(|e| napi::Error::from_reason(format!("cannot read '{path}': {e}")))?;
    PdfDocument::open(Buffer::from(bytes))
}

/// Merge multiple PDF files into a single output file.
///
/// Reads each input path, concatenates all pages in order, and writes the
/// result to `output_path`. All input files must be valid, readable PDFs.
///
/// ```js
/// const { mergePdfs } = require('@pdfluent/node');
/// await mergePdfs(['a.pdf', 'b.pdf'], 'merged.pdf');
/// ```
#[napi]
pub fn merge_pdfs(paths: Vec<String>, output_path: String) -> Result<()> {
    if paths.is_empty() {
        return Err(napi::Error::from_reason("paths must not be empty"));
    }
    let mut doc = pdf_manip::pages::merge(&paths)
        .map_err(|e| napi::Error::from_reason(format!("merge failed: {e}")))?;
    doc.save(&output_path)
        .map_err(|e| napi::Error::from_reason(format!("cannot write '{output_path}': {e}")))?;
    Ok(())
}

/// Validate a PDF file against a PDF/A conformance level.
///
/// `level` must be one of: "1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u".
///
/// Returns a `ComplianceReportInfo` with `compliant`, `errorCount`,
/// `warningCount`, and a list of `issues`.
///
/// ```js
/// const { validatePdfA } = require('@pdfluent/node');
/// const report = validatePdfA('document.pdf', '2b');
/// console.log(report.compliant, report.errorCount);
/// ```
#[napi]
pub fn validate_pdfa(path: String, level: String) -> Result<ComplianceReportInfo> {
    let bytes = std::fs::read(&path)
        .map_err(|e| napi::Error::from_reason(format!("cannot read '{path}': {e}")))?;
    let doc = RustDocument::open(bytes)
        .map_err(|e| napi::Error::from_reason(format!("invalid PDF '{path}': {e}")))?;
    let pdfa_level = parse_pdfa_level(&level)?;
    let report = pdf_compliance::validate_pdfa(doc.pdf(), pdfa_level);
    Ok(compliance_to_info(report))
}
