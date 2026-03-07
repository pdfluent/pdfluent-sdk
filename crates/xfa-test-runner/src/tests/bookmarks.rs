use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct BookmarksTest;

impl PdfTest for BookmarksTest {
    fn name(&self) -> &str {
        "bookmarks"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let bookmarks = doc.bookmarks();
        if bookmarks.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        fn count_bookmarks(items: &[pdf_engine::BookmarkItem]) -> usize {
            items.iter().map(|b| 1 + count_bookmarks(&b.children)).sum()
        }

        let total = count_bookmarks(&bookmarks);
        let mut metadata = HashMap::new();
        metadata.insert("bookmark_count".to_string(), total.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
