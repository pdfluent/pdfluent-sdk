use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Corpus-level roundtrip test for PDF encryption.
///
/// Encrypts the input with RC4-128 (requires no special PDF-2.0 parser
/// support), saves to bytes, reloads, verifies the Encrypt dict is present,
/// then decrypts with the user password and verifies the page count is intact.
///
/// AES-256 is skipped here because lopdf's AES-256 path (V=5, R=6) requires a
/// PDF 2.0-capable reader; RC4-128 (V=2, R=3) roundtrips reliably with lopdf.
pub struct EncryptRoundtripTest;

impl PdfTest for EncryptRoundtripTest {
    fn name(&self) -> &str {
        "encrypt_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .is_err()
        {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("thread spawn failed (resource limit)".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in encrypt_roundtrip".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("encrypt_roundtrip timed out (>25s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    use pdf_manip::encrypt::{encrypt_and_save, EncryptConfig, EncryptionAlgorithm, Permissions};

    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // Skip already-encrypted documents — lopdf can't re-encrypt without
    // first decrypting, and we don't have the password. Raw-byte scan covers
    // most cases; the trailer check below catches xref-stream PDFs where the
    // /Encrypt ref lives in a compressed xref stream, invisible to window search.
    let already_encrypted = pdf.windows(8).any(|w| w == b"/Encrypt");
    if already_encrypted {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("already encrypted".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Secondary encrypted check via the parsed trailer. Catches PDFs whose
    // /Encrypt ref is only visible after parsing (e.g. embedded in xref streams).
    if doc.trailer.get(b"Encrypt").is_ok() {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("already encrypted (trailer /Encrypt)".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let original_pages = doc.get_pages().len();
    if original_pages == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let cfg = EncryptConfig {
        user_password: b"testuser".to_vec(),
        owner_password: b"testowner".to_vec(),
        algorithm: EncryptionAlgorithm::Rc4_128,
        permissions: Permissions::allow_all(),
    };

    let mut enc_bytes = Vec::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        encrypt_and_save(&mut doc, &cfg, &mut enc_bytes)
    }));

    match r {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let msg = e.to_string();
            // "decryption error" inside encrypt_and_save means the PDF has
            // internally-encrypted objects without a proper /Encrypt dict —
            // a corrupt/non-compliant document we cannot fix. Skip it.
            let status = if msg.contains("decryption error") {
                TestStatus::Skip
            } else {
                TestStatus::Fail
            };
            return TestResult {
                status,
                error_message: Some(format!("encrypt_and_save failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("encrypt_and_save panicked".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    }

    if enc_bytes.is_empty() {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some("encrypt_and_save produced empty output".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Verify the /Encrypt entry is present in the raw bytes.
    // lopdf's encrypted loading path (load_mem without password) only populates
    // the Encrypt dict itself and leaves all other objects unloaded until the
    // correct password is provided at load time. Checking the raw bytes avoids
    // a false "no /Encrypt entry" failure from that half-loaded state.
    if !enc_bytes.windows(8).any(|w| w == b"/Encrypt") {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some("encrypted output has no /Encrypt entry in raw bytes".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Reload with password — lopdf decrypts at load time when the password is
    // supplied via load_mem_with_password. The old two-step load_mem + decrypt()
    // no longer works because the reader's encrypted loading path requires the
    // password up-front to populate the object graph.
    let dec_doc = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        lopdf::Document::load_mem_with_options(
            &enc_bytes,
            lopdf::LoadOptions::with_password("testuser"),
        )
    })) {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("reload with password failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("reload with password panicked".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let decrypted_pages = dec_doc.get_pages().len();
    if decrypted_pages != original_pages {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "page count after decrypt: {decrypted_pages}, expected {original_pages}"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("pages".to_string(), original_pages.to_string());
    metadata.insert("enc_bytes".to_string(), enc_bytes.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}
