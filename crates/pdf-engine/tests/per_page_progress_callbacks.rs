//! Extraction reports progress page by page (lane L6).
//!
//! One three-page document through the batch path: the batch callback fires
//! once per file -- `(1, 1)` here (G153) -- while the per-page callback
//! reports each page as it is read, `(1, 3)` through `(3, 3)`, so a caller
//! watching a long document hears it advance.
//!
//! The front door reports both granularities: its batch callback fires once
//! per file (G153) and its progress bar advances page by page (G160).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{process_batch_with_page_progress, BatchConfig};

/// A scratch dir under the system temp dir, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "per_page_progress_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A three-page document of one line per page.
fn three_pages() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for page_no in 1..=3 {
        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 10.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal(format!(
                    "The line on page {page_no}."
                ))],
            ),
            Operation::new("ET", vec![]),
        ];
        let content = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().expect("encode"),
        )));
        kids.push(
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            })
            .into(),
        );
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => Object::Array(kids), "Count" => 3,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_three_page_file_reports_each_page_as_it_is_read() {
    let dir = TempDir::new();
    let path = dir.path.join("three.pdf");
    std::fs::write(&path, three_pages()).expect("write pdf");
    let calls: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&calls);
    let page_calls: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_pages = Arc::clone(&page_calls);
    let config = BatchConfig {
        on_progress: Some(Box::new(move |done, total| {
            seen.lock().expect("calls").push((done, total));
        })),
        ..BatchConfig::default()
    };
    let result = process_batch_with_page_progress(
        &[path],
        config,
        Box::new(move |done, total| {
            seen_pages.lock().expect("page calls").push((done, total));
        }),
        |document| Ok(document.page_count()),
    );
    assert!(result.failures.is_empty(), "the batch run failed");
    let calls = calls.lock().expect("calls").clone();
    assert_eq!(
        calls,
        vec![(1, 1)],
        "the batch callback fires once per file"
    );
    assert_eq!(
        page_calls.lock().expect("page calls").clone(),
        vec![(1, 3), (2, 3), (3, 3)],
        "G160: progress advances page by page"
    );
}
