//! The committed man pages equal what the clap tree generates.
//!
//! `docs/cli/man/*.1` are rendered from the real clap tree with `clap_mangen`
//! by the binary's internal `generate-man-pages` command. This test
//! regenerates them into a temp dir and diffs name set and bytes: a flag
//! whose help changed without regenerated pages fails here. Regenerate with:
//! `BIN=target/debug/pdfluent bash scripts/ci/generate_cli_docs.sh`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use std::path::PathBuf;

use common::{run, tempdir};

#[test]
fn committed_man_pages_match_the_clap_tree() {
    let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli/man");
    assert!(
        committed.is_dir(),
        "docs/cli/man exists (run BIN=$BIN bash scripts/ci/generate_cli_docs.sh)"
    );
    let dir = tempdir("cli-man");
    let generated = dir.join("gen");
    let out = run(&["generate-man-pages", &generated.to_string_lossy()]);
    assert!(
        out.status.success(),
        "generate-man-pages runs: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut expected: Vec<String> = std::fs::read_dir(&committed)
        .expect("read docs/cli/man")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    expected.sort();
    assert!(
        !expected.is_empty(),
        "docs/cli/man holds pages (run BIN=$BIN bash scripts/ci/generate_cli_docs.sh)"
    );
    let mut fresh: Vec<String> = std::fs::read_dir(&generated)
        .expect("read regenerated pages")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    fresh.sort();
    assert_eq!(
        expected, fresh,
        "man page name set changed (run BIN=$BIN bash scripts/ci/generate_cli_docs.sh)"
    );
    for name in &expected {
        let old = std::fs::read(committed.join(name)).expect("read committed page");
        let new = std::fs::read(generated.join(name)).expect("read regenerated page");
        assert!(
            old == new,
            "docs/cli/man/{name} is stale (run BIN=$BIN bash scripts/ci/generate_cli_docs.sh)"
        );
    }
}
