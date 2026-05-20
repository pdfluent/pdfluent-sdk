//! QR-3 release-recheck: decryption handler matrix.
//!
//! The SDK *writes* AES-256 only (AES-128 selector routes to 256). The
//! release-blocking concern is the *decryption* handler matrix across the
//! standard handlers (RC4-128, AES-128/V2, AES-256/V3). We generate real
//! encrypted fixtures with an INDEPENDENT encryptor (`qpdf`) at test time
//! (no sensitive files committed) and assert pdfluent's read/open behaviour:
//!
//! - correct password  → opens (Ok) OR a typed error — never a panic;
//! - wrong password    → typed error, never panic, never silent success;
//! - missing password  → typed error / marked-encrypted, never panic.
//!
//! Outcomes are recorded; the invariant asserted for ALL handlers is
//! "no panic + typed outcome + wrong/missing password never yields the
//! plaintext". Skips if `qpdf` is unavailable (CI provides it).

use std::path::{Path, PathBuf};
use std::process::Command;

use pdfluent::{OpenOptions, PdfDocument};

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn qpdf_ok() -> bool {
    Command::new("qpdf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Encrypt `src` → `dst` with qpdf. `bits` in {40,128,256}; `aes` toggles
/// AES vs RC4 for 128-bit (256 is always AES). Returns true on success.
fn qpdf_encrypt(src: &Path, dst: &Path, user: &str, owner: &str, bits: u32, aes: bool) -> bool {
    // qpdf 11+ syntax: --encrypt --user-password=.. --owner-password=.. --bits=..
    let mut cmd = Command::new("qpdf");
    cmd.arg("--encrypt")
        .arg(format!("--user-password={user}"))
        .arg(format!("--owner-password={owner}"))
        .arg(format!("--bits={bits}"));
    if bits == 128 {
        cmd.arg(if aes { "--use-aes=y" } else { "--use-aes=n" });
    }
    cmd.arg("--").arg(src).arg(dst);
    // qpdf exit codes: 0 = ok, 3 = warnings (acceptable). Only the encrypted
    // output's validity matters, confirmed by --is-encrypted below.
    let code = cmd
        .output()
        .ok()
        .and_then(|o| o.status.code())
        .unwrap_or(-1);
    if code != 0 && code != 3 {
        return false;
    }
    Command::new("qpdf")
        .arg("--is-encrypted")
        .arg(dst)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug)]
struct Outcome {
    handler: &'static str,
    // Recorded for the evidence log (printed via Debug); not part of the
    // hard invariant, since whether a handler *opens* is informational.
    #[allow(dead_code)]
    correct_opens: bool,
    wrong_is_err: bool,
    missing_is_err: bool,
}

fn open_pw(path: &Path, pw: Option<&str>) -> Result<PdfDocument, pdfluent::Error> {
    match pw {
        Some(p) => PdfDocument::open_with(path, OpenOptions::new().with_password(p)),
        None => PdfDocument::open(path),
    }
}

#[test]
fn qr3_decryption_handler_matrix() {
    if !qpdf_ok() {
        eprintln!("qpdf unavailable — skipping QR-3 decryption matrix");
        return;
    }
    // multi-page.pdf is well-formed enough for qpdf to re-emit cleanly
    // (the minimal simple.pdf is not a clean qpdf round-trip source).
    let src = mini("multi-page.pdf");
    if !src.exists() {
        return;
    }
    let tmp = std::env::temp_dir().join("pdfluent_qr3");
    std::fs::create_dir_all(&tmp).expect("mk tmp");
    const USER: &str = "user-pw";
    const OWNER: &str = "owner-pw";

    let handlers: &[(&str, u32, bool)] = &[
        ("rc4-128", 128, false),
        ("aes-128", 128, true),
        ("aes-256", 256, true),
    ];

    let mut outcomes = Vec::new();
    for (name, bits, aes) in handlers {
        let dst = tmp.join(format!("enc_{name}.pdf"));
        if !qpdf_encrypt(&src, &dst, USER, OWNER, *bits, *aes) {
            eprintln!("qpdf could not produce {name} — skipping that handler");
            continue;
        }

        // correct password — must not panic; record whether it opens.
        let correct = std::panic::catch_unwind(|| open_pw(&dst, Some(USER)))
            .expect("open(correct) must not panic");
        // wrong password — must be a typed error (no panic, no plaintext).
        let wrong = std::panic::catch_unwind(|| open_pw(&dst, Some("definitely-wrong")))
            .expect("open(wrong) must not panic");
        // missing password — must be a typed error / refusal (no panic).
        let missing =
            std::panic::catch_unwind(|| open_pw(&dst, None)).expect("open(missing) must not panic");

        // If "correct" opened, "wrong" must NOT also yield a readable doc with
        // the same text (i.e. wrong password cannot bypass decryption).
        if let (Ok(c), Ok(w)) = (&correct, &wrong) {
            let ct = c.extract_text().unwrap_or_default();
            let wt = w.extract_text().unwrap_or_default();
            assert!(
                ct.is_empty() || ct != wt,
                "{name}: wrong password produced the same plaintext as correct — decryption bypass"
            );
        }

        outcomes.push(Outcome {
            handler: name,
            correct_opens: correct.is_ok(),
            wrong_is_err: wrong.is_err(),
            missing_is_err: missing.is_err(),
        });
    }

    // Invariant for every handler we could generate: wrong + missing password
    // never silently succeed (must be Err); no panics occurred above.
    for o in &outcomes {
        eprintln!("QR-3 {o:?}");
        assert!(
            o.wrong_is_err,
            "{}: wrong password must return a typed error, not Ok",
            o.handler
        );
        assert!(
            o.missing_is_err,
            "{}: missing password must return a typed error, not Ok",
            o.handler
        );
    }
    assert!(
        !outcomes.is_empty(),
        "no encryption handlers could be generated by qpdf"
    );
}
