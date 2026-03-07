pub mod annotations;
pub mod bookmarks;
pub mod compliance;
pub mod form_fields;
pub mod geometry;
pub mod images;
pub mod metadata;
pub mod parse;
pub mod render;
pub mod search;
pub mod signatures;
pub mod text_extract;

use std::collections::HashMap;
use std::path::Path;

pub trait PdfTest: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult;
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
    let compliance = if let Some(oracle) = config.verapdf_oracle {
        compliance::ComplianceTest::new().with_verapdf(oracle)
    } else {
        compliance::ComplianceTest::new()
    };

    vec![
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
    ]
}
