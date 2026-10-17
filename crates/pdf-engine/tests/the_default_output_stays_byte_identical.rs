// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The default output stays byte-identical while raw order lands (#643).
//!
//! The raw content-stream order option (`ExtractOptions::raw_order`, E1) and
//! the grouping under it (E2a/b/c) add a new path through `extract_blocks`;
//! the default path must not move a byte. These goldens freeze what the
//! default options produce today -- `to_json` and markdown over the whole
//! document -- on the L1 fixtures and on the two compat pins, so every later
//! commit on this branch proves the default untouched by keeping them green.
//!
//! Fixtures (workspace root; byte-identical to the compat lane's copies):
//! `tests/corpus-mini/simple.pdf` (md5 0b5d49260188edba06540977cf8b8ac8),
//! `fixtures/structured.pdf` (md5 43ca50b8e8ee50e10b4a663bdb353e54),
//! `corpus/f1040.pdf` (md5 ac8ba4b22e4c1b8640b6e96fb0ff28bd).
//!
//! Regenerate with `PDFLUENT_WRITE_GOLDEN=1`, the same switch the serializer
//! goldens use -- and read the diff first: a golden that moves here means the
//! default path moved, which is never what an E1/E2 commit intends.

use pdf_engine::{to_json, to_markdown, ExtractOptions, PdfDocument};
use std::path::PathBuf;

/// The three documents the default output is pinned on, by golden name.
const CASES: &[(&str, &str)] = &[
    ("e1e2-simple", "tests/corpus-mini/simple.pdf"),
    ("e1e2-structured", "fixtures/structured.pdf"),
    ("e1e2-f1040", "corpus/f1040.pdf"),
];

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn golden_path(name: &str, extension: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.{extension}"))
}

fn against_golden(name: &str, extension: &str, produced: &str) {
    let path = golden_path(name, extension);
    if std::env::var_os("PDFLUENT_WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden directory")).expect("make directory");
        std::fs::write(&path, produced).expect("write golden");
        return;
    }
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("no golden at {}: {why}", path.display()));
    assert_eq!(
        produced,
        golden,
        "{} has moved. Read the diff; regenerate with PDFLUENT_WRITE_GOLDEN=1 only once you \
         are sure the default path was meant to move",
        path.display()
    );
}

fn blocks_of(relative: &str) -> Vec<pdf_engine::Block> {
    let bytes = std::fs::read(workspace_path(relative)).expect("read the fixture");
    let document = PdfDocument::open(bytes).expect("open the fixture");
    document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options")
}

/// The JSON of the default blocks, byte for byte.
#[test]
fn the_default_json_is_pinned() {
    for (name, fixture) in CASES {
        against_golden(name, "json", &to_json(&blocks_of(fixture)));
    }
}

/// The markdown of the default blocks, byte for byte.
#[test]
fn the_default_markdown_is_pinned() {
    for (name, fixture) in CASES {
        against_golden(name, "md", &to_markdown(&blocks_of(fixture)));
    }
}
