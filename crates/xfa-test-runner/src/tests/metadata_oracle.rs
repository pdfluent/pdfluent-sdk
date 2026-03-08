use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::poppler::PopplerOracle;

/// Compares our metadata extraction against Poppler's `pdfinfo`.
/// Always returns Pass with oracle_score indicating match ratio (0.0-1.0).
pub struct MetadataOracleTest;

impl PdfTest for MetadataOracleTest {
    fn name(&self) -> &str {
        "metadata_oracle"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        if !PopplerOracle::is_available() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("pdfinfo not available".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Our engine
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Pass,
                    error_message: Some(format!("Our engine failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: Some(0.0),
                    metadata: HashMap::new(),
                };
            }
        };

        // Poppler
        let poppler_info = match PopplerOracle::get_info(path) {
            Ok(info) => info,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("pdfinfo failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let our_page_count = doc.page_count();
        let our_info = doc.info();
        let mut checks = 0u32;
        let mut matches = 0u32;
        let mut mismatches = Vec::new();
        let mut result_meta = HashMap::new();

        // Page count — must match exactly
        if let Some(poppler_pages) = poppler_info.page_count {
            checks += 1;
            result_meta.insert("our_pages".to_string(), our_page_count.to_string());
            result_meta.insert("poppler_pages".to_string(), poppler_pages.to_string());
            if our_page_count == poppler_pages {
                matches += 1;
            } else {
                mismatches.push(format!(
                    "page_count: ours={our_page_count} poppler={poppler_pages}"
                ));
            }
        }

        // Title — case-insensitive comparison
        if let Some(poppler_title) = &poppler_info.title {
            if !poppler_title.is_empty() {
                checks += 1;
                let our_title = our_info.title.as_deref().unwrap_or("");
                result_meta.insert("our_title".to_string(), our_title.to_string());
                result_meta.insert("poppler_title".to_string(), poppler_title.clone());
                if our_title.eq_ignore_ascii_case(poppler_title) {
                    matches += 1;
                } else {
                    mismatches.push(format!(
                        "title: ours='{}' poppler='{}'",
                        our_title, poppler_title
                    ));
                }
            }
        }

        // Author — case-insensitive comparison
        if let Some(poppler_author) = &poppler_info.author {
            if !poppler_author.is_empty() {
                checks += 1;
                let our_author = our_info.author.as_deref().unwrap_or("");
                result_meta.insert("our_author".to_string(), our_author.to_string());
                result_meta.insert("poppler_author".to_string(), poppler_author.clone());
                if our_author.eq_ignore_ascii_case(poppler_author) {
                    matches += 1;
                } else {
                    mismatches.push(format!(
                        "author: ours='{}' poppler='{}'",
                        our_author, poppler_author
                    ));
                }
            }
        }

        let score = if checks == 0 {
            1.0
        } else {
            matches as f64 / checks as f64
        };

        result_meta.insert("checks".to_string(), checks.to_string());
        result_meta.insert("matches".to_string(), matches.to_string());
        result_meta.insert("mismatches".to_string(), mismatches.len().to_string());

        // Quality metric: always Pass, score captures accuracy
        TestResult {
            status: TestStatus::Pass,
            error_message: if mismatches.is_empty() {
                None
            } else {
                Some(mismatches.join("; "))
            },
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(score),
            metadata: result_meta,
        }
    }
}
