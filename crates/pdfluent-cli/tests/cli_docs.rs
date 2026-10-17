//! The generated CLI reference is current.
//!
//! `docs/cli/*.md` are generated from clap `--help` and `docs/cli/man/*.1`
//! from the clap tree itself (clap_mangen) by
//! `scripts/ci/generate_cli_docs.sh`. This test runs that script with
//! `--check`: a command whose help changed without regenerated docs fails
//! here, in CI, on the commit that changed it. The man pages carry their own
//! second check in `tests/cli_man.rs`, straight from the binary.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn generated_cli_docs_are_current() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("scripts/ci/generate_cli_docs.sh");
    assert!(script.is_file(), "generator script exists");
    let out = Command::new("bash")
        .arg(&script)
        .arg("--check")
        .env("BIN", env!("CARGO_BIN_EXE_pdfluent"))
        .output()
        .expect("run generate_cli_docs.sh --check");
    assert!(
        out.status.success(),
        "--check failed (regenerate docs):\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
