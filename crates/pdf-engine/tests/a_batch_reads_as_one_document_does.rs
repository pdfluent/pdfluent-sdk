//! The batch the structured benchmark times (#623) reads every document the
//! way a single call does, serially and over threads alike.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::structured_batch::{batch_markdown, markdown_of_bytes};

fn fixtures() -> Vec<Vec<u8>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .expect("fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|p| std::fs::read(p).expect("read"))
        .collect()
}

#[test]
fn serial_parallel_and_single_calls_agree_in_input_order() {
    let docs = fixtures();
    assert!(
        docs.len() >= 3,
        "the fixtures directory holds too few PDFs to share out"
    );
    let single: Vec<Result<String, String>> =
        docs.iter().map(|d| markdown_of_bytes(d.clone())).collect();
    for jobs in [1, 4] {
        let items = batch_markdown(docs.len(), jobs, |i| Ok(docs[i].clone()));
        assert_eq!(
            items.len(),
            docs.len(),
            "jobs={jobs}: a document went missing"
        );
        for (i, (item, alone)) in items.iter().zip(&single).enumerate() {
            assert_eq!(
                &item.markdown, alone,
                "jobs={jobs}: document {i} read differently"
            );
        }
    }
}

#[test]
fn an_unreadable_input_is_its_own_error_and_the_rest_still_read() {
    let docs = fixtures();
    let items = batch_markdown(3, 2, |i| {
        if i == 1 {
            Err("gone".into())
        } else {
            Ok(docs[i].clone())
        }
    });
    assert!(items[0].markdown.is_ok() && items[2].markdown.is_ok());
    assert_eq!(items[1].markdown, Err("gone".to_string()));
}
