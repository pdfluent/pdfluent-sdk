//! `extract` writes figures again with `--images` (lane L4).
//!
//! The flag is gone: `extract` reads text and tables but offers no image
//! output, so figures never leave the document. The rebuilt command takes
//! `--images` again. Only the flag's presence is pinned; what it writes is
//! the engine's figure behaviour, specified beside it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::process::Command;

#[test]
fn extract_help_lists_the_images_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_pdfluent"))
        .args(["extract", "--help"])
        .output()
        .expect("run extract --help");
    assert!(output.status.success(), "extract --help failed");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--images"),
        "no --images flag in extract --help: {help:?}"
    );
}
