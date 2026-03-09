pub mod annot_create;
pub mod annotations;
pub mod bookmarks;
pub mod compliance;
pub mod content_roundtrip;
pub mod form_fields;
pub mod form_write;
pub mod geometry;
pub mod images;
pub mod manipulation;
pub mod metadata;
pub mod metadata_oracle;
pub mod parse;
pub mod render;
#[cfg(feature = "pdfium-oracle")]
pub mod render_oracle;
pub mod search;
pub mod sign_verify;
pub mod signatures;
pub mod text_extract;
pub mod text_oracle;
pub mod text_replace;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

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
    #[cfg(feature = "pdfium-oracle")]
    pub diff_dir: Option<std::path::PathBuf>,
}

pub fn all_tests(config: TestConfig) -> Vec<Box<dyn PdfTest>> {
    let compliance = if let Some(oracle) = config.verapdf_oracle {
        compliance::ComplianceTest::new().with_verapdf(oracle)
    } else {
        compliance::ComplianceTest::new()
    };

    #[allow(unused_mut)]
    let mut tests: Vec<Box<dyn PdfTest>> = vec![
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
        Box::new(search::SearchTest),
        Box::new(text_oracle::TextOracleTest),
        Box::new(metadata_oracle::MetadataOracleTest),
        Box::new(manipulation::ManipulationTest),
        Box::new(sign_verify::SignVerifyTest),
        Box::new(form_write::FormWriteTest),
        Box::new(annot_create::AnnotCreateTest),
        Box::new(content_roundtrip::ContentRoundtripTest),
        Box::new(text_replace::TextReplaceTest),
    ];

    #[cfg(feature = "pdfium-oracle")]
    {
        tests.push(Box::new(render_oracle::RenderOracleTest {
            diff_dir: config.diff_dir,
        }));
    }

    tests
}
