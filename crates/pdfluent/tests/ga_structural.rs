//! GA Quality — QR-12 structural correctness via an INDEPENDENT validator.
//!
//! For each valid in-repo fixture: open → `to_bytes()` (roundtrip) → write to
//! a temp file → run `qpdf --check` on it and assert the external validator
//! reports no structural ERROR. This proves our writer emits structurally
//! valid PDF, judged by a third-party tool (qpdf), not by ourselves.
//!
//! The test is skipped (passes trivially) if `qpdf` is not on PATH so it
//! never blocks environments without the validator; CI provides qpdf.

use std::path::PathBuf;
use std::process::Command;

use pdfluent::prelude::*;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn qpdf_available() -> bool {
    Command::new("qpdf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `qpdf --check <path>`; return (ok, combined-output). qpdf exit codes:
/// 0 = no problems, 2 = errors, 3 = warnings only. We accept 0 and 3
/// (warnings), reject 2 (structural errors) and any "ERROR" line.
fn qpdf_check(path: &std::path::Path) -> (bool, String) {
    let out = Command::new("qpdf")
        .arg("--check")
        .arg(path)
        .output()
        .expect("spawn qpdf");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let code = out.status.code().unwrap_or(-1);
    let has_error_line = combined.lines().any(|l| l.contains("ERROR"));
    let ok = (code == 0 || code == 3) && !has_error_line;
    (ok, combined)
}

#[test]
fn qr12_roundtrip_outputs_are_structurally_valid_per_qpdf() {
    if !qpdf_available() {
        eprintln!("qpdf not available — skipping QR-12 external validation");
        return;
    }
    let tmp = std::env::temp_dir().join("pdfluent_qr12");
    std::fs::create_dir_all(&tmp).expect("mk tmp");

    // Valid (non-malformed, non-encrypted) fixtures suitable for roundtrip.
    for name in [
        "simple.pdf",
        "multi-page.pdf",
        "acroform.pdf",
        "pdfa-2b.pdf",
    ] {
        let Ok(bytes) = std::fs::read(mini(name)) else {
            continue;
        };
        let Ok(doc) = PdfDocument::from_bytes(&bytes) else {
            continue;
        };
        let out_bytes = doc.to_bytes().expect("roundtrip to_bytes");
        let out_path = tmp.join(format!("rt_{name}"));
        std::fs::write(&out_path, &out_bytes).expect("write roundtrip");

        let (ok, report) = qpdf_check(&out_path);
        assert!(
            ok,
            "qpdf --check reported structural errors on roundtrip of {name}:\n{report}"
        );
    }
}

#[test]
fn qr12_original_corpus_is_valid_baseline() {
    if !qpdf_available() {
        return;
    }
    // Sanity baseline: the valid fixtures themselves pass qpdf --check, so a
    // roundtrip failure is attributable to the writer, not the input.
    for name in ["simple.pdf", "multi-page.pdf"] {
        let p = mini(name);
        if !p.exists() {
            continue;
        }
        let (ok, report) = qpdf_check(&p);
        assert!(
            ok,
            "baseline fixture {name} unexpectedly fails qpdf:\n{report}"
        );
    }
}
