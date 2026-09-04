// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod annot_create;
pub mod annotations;
pub mod bookmarks;
pub mod compliance;
pub mod compress_roundtrip;
pub mod content_roundtrip;
pub mod docx_convert;
pub mod encrypt_roundtrip;
pub mod form_fields;
pub mod form_write;
pub mod geometry;
pub mod header_footer;
pub mod image_extract_verify;
pub mod images;
pub mod manipulation;
pub mod metadata;
pub mod metadata_oracle;
pub mod ocr;
pub mod parse;
pub mod pdfa_convert;
pub mod pdfua_validate;
pub mod pdfx_validate;
pub mod pptx_convert;
pub mod redact;
pub mod render;
pub mod render_llm_review;
pub mod render_multi_oracle;
pub mod render_mupdf_oracle;
pub mod rotate_roundtrip;
pub mod search;
pub mod sign_roundtrip;
pub mod sign_verify;
pub mod signatures;
pub mod text_extract;
pub mod text_oracle;
pub mod text_replace;
pub mod watermark_roundtrip;
pub mod xfa_data_roundtrip;
pub mod xfa_extract;
pub mod xfa_flatten;
pub mod xfa_formcalc;
pub mod xlsx_convert;
pub mod zugferd_roundtrip;
pub mod zugferd_validate;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Detect malformed page trees (loops, duplicate refs) by byte scanning.
/// PDFs with these issues cause rotate/split/watermark/header-footer operations
/// to produce incorrect output. Tests should SKIP rather than FAIL on these.
pub fn has_malformed_page_tree(pdf_data: &[u8]) -> bool {
    // Check for duplicate page references in /Kids arrays
    let mut pos = 0;
    while pos + 5 < pdf_data.len() {
        if &pdf_data[pos..pos + 5] != b"/Kids" {
            pos += 1;
            continue;
        }
        // Find the [ ... ] array
        let Some(bracket_start) = pdf_data[pos..].iter().position(|&b| b == b'[') else {
            pos += 5;
            continue;
        };
        let abs_start = pos + bracket_start + 1;
        let Some(bracket_end) = pdf_data[abs_start..].iter().position(|&b| b == b']') else {
            pos += 5;
            continue;
        };
        let kids_slice = &pdf_data[abs_start..abs_start + bracket_end];
        // Extract "N N R" references
        let refs: Vec<&[u8]> = kids_slice
            .split(|&b| b == b' ' || b == b'\n' || b == b'\r')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .chunks(3)
            .filter(|c| c.len() == 3 && c[2] == b"R")
            .map(|c| {
                let start = c[0].as_ptr() as usize - kids_slice.as_ptr() as usize;
                let end = c[2].as_ptr() as usize - kids_slice.as_ptr() as usize + c[2].len();
                &kids_slice[start..end]
            })
            .collect();
        // Check for duplicates
        let unique: std::collections::HashSet<&[u8]> = refs.iter().copied().collect();
        if refs.len() > unique.len() {
            return true;
        }
        pos += bracket_start + bracket_end + 1;
    }
    // Check for page tree loops: a /Pages node referencing itself in /Kids.
    // Look for patterns like "N 0 obj<<.../Type /Pages.../Kids[... N 0 R ...]"
    // where the object number appears in its own /Kids array.
    use std::collections::HashSet;
    let mut pages_obj_nums: HashSet<Vec<u8>> = HashSet::new();

    // Collect all /Pages object numbers
    for i in 0..pdf_data.len().saturating_sub(20) {
        if !pdf_data[i..].starts_with(b"/Type /Pages") && !pdf_data[i..].starts_with(b"/Type/Pages")
        {
            continue;
        }
        // Walk backward to find "N 0 obj"
        let search_start = i.saturating_sub(200);
        let before = &pdf_data[search_start..i];
        if let Some(obj_pos) = before
            .windows(4)
            .rposition(|w| w == b" obj" || w == b"\nobj")
        {
            // Extract "N 0" before "obj"
            let line_start = before[..obj_pos]
                .iter()
                .rposition(|&b| b == b'\n' || b == b'\r')
                .map(|p| p + 1)
                .unwrap_or(0);
            let obj_header = &before[line_start..obj_pos];
            // obj_header is like "2 0 " — extract just the number
            if let Some(space) = obj_header.iter().position(|&b| b == b' ') {
                let obj_num = &obj_header[..space];
                pages_obj_nums.insert(obj_num.to_vec());
            }
        }
    }

    // Check if any /Kids array references a /Pages object number
    for i in 0..pdf_data.len().saturating_sub(6) {
        if !pdf_data[i..].starts_with(b"/Kids") {
            continue;
        }
        let after = &pdf_data[i..pdf_data.len().min(i + 1000)];
        let Some(bracket_start) = after.iter().position(|&b| b == b'[') else {
            continue;
        };
        let Some(bracket_end) = after[bracket_start..].iter().position(|&b| b == b']') else {
            continue;
        };
        let kids = &after[bracket_start + 1..bracket_start + bracket_end];
        // Check if any /Pages object appears in this /Kids
        for num in &pages_obj_nums {
            // Look for "N 0 R" in /Kids
            let pattern = [num.as_slice(), b" 0 R"].concat();
            if kids.windows(pattern.len()).any(|w| w == pattern.as_slice()) {
                return true; // /Pages node in /Kids = loop or structural issue
            }
        }
    }

    false
}

pub trait PdfTest: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult;

    /// Optional progress tracker: returns the name of the last-started sub-check.
    /// Used by the runner to include diagnostic info in timeout error messages.
    fn progress_tracker(&self) -> Option<Arc<Mutex<String>>> {
        None
    }
}

#[allow(dead_code)]
pub struct TestResult {
    pub status: TestStatus,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub oracle_score: Option<f64>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Crash,
    Timeout,
    Skip,
}

impl TestStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Crash => "crash",
            Self::Timeout => "timeout",
            Self::Skip => "skip",
        }
    }
}

pub struct TestConfig {
    pub verapdf_oracle: Option<std::sync::Arc<crate::oracles::verapdf::VeraPdfOracle>>,
}

pub fn all_tests(config: TestConfig) -> Vec<Box<dyn PdfTest>> {
    let verapdf_arc = config.verapdf_oracle;

    let compliance = if let Some(ref oracle) = verapdf_arc {
        compliance::ComplianceTest::new().with_verapdf(oracle.clone())
    } else {
        compliance::ComplianceTest::new()
    };

    let pdfa_convert = if let Some(ref oracle) = verapdf_arc {
        pdfa_convert::PdfAConvertTest::new().with_verapdf(oracle.clone())
    } else {
        pdfa_convert::PdfAConvertTest::new()
    };

    let pdfua_validate = if let Some(ref oracle) = verapdf_arc {
        pdfua_validate::PdfUaValidateTest::new().with_verapdf(oracle.clone())
    } else {
        pdfua_validate::PdfUaValidateTest::new()
    };

    let pdfx_validate = if let Some(ref oracle) = verapdf_arc {
        pdfx_validate::PdfXValidateTest::new().with_verapdf(oracle.clone())
    } else {
        pdfx_validate::PdfXValidateTest::new()
    };

    let tests: Vec<Box<dyn PdfTest>> = vec![
        Box::new(parse::ParseTest),
        Box::new(metadata::MetadataTest),
        Box::new(render::RenderTest),
        Box::new(text_extract::TextExtractTest),
        Box::new(form_fields::FormFieldsTest),
        Box::new(annotations::AnnotationsTest),
        Box::new(signatures::SignaturesTest),
        Box::new(compliance),
        Box::new(bookmarks::BookmarksTest),
        Box::new(geometry::GeometryTest),
        Box::new(images::ImageExtractTest),
        Box::new(image_extract_verify::ImageExtractVerifyTest),
        Box::new(search::SearchTest),
        Box::new(text_oracle::TextOracleTest),
        Box::new(metadata_oracle::MetadataOracleTest),
        Box::new(manipulation::ManipulationTest),
        Box::new(sign_verify::SignVerifyTest),
        Box::new(form_write::FormWriteTest),
        Box::new(annot_create::AnnotCreateTest),
        Box::new(content_roundtrip::ContentRoundtripTest),
        Box::new(text_replace::TextReplaceTest),
        Box::new(redact::RedactTest),
        Box::new(pdfa_convert),
        Box::new(sign_roundtrip::SignRoundtripTest),
        Box::new(ocr::OcrTest),
        Box::new(rotate_roundtrip::RotateRoundtripTest),
        Box::new(encrypt_roundtrip::EncryptRoundtripTest),
        Box::new(watermark_roundtrip::WatermarkRoundtripTest),
        Box::new(compress_roundtrip::CompressRoundtripTest),
        Box::new(xfa_extract::XfaExtractTest),
        Box::new(xfa_flatten::XfaFlattenTest),
        Box::new(xfa_formcalc::XfaFormCalcTest),
        Box::new(xfa_data_roundtrip::XfaDataRoundtripTest),
        Box::new(render_mupdf_oracle::RenderMupdfOracleTest),
        Box::new(render_multi_oracle::RenderMultiOracleTest),
        Box::new(render_llm_review::RenderLlmReviewTest),
        Box::new(docx_convert::DocxConvertTest),
        Box::new(xlsx_convert::XlsxConvertTest),
        Box::new(zugferd_roundtrip::ZugferdRoundtripTest),
        Box::new(zugferd_validate::ZugferdValidateTest),
        Box::new(pdfua_validate),
        Box::new(pdfx_validate),
        Box::new(pptx_convert::PptxConvertTest),
        Box::new(header_footer::HeaderFooterTest),
    ];

    tests
}
