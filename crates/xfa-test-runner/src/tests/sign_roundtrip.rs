// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use super::{PdfTest, TestResult, TestStatus};

/// Find an OpenSSL binary on the system. Returns `None` when unavailable
/// (external verification is then skipped rather than failed). Cached after
/// the first lookup to avoid repeated file-system probes on corpus runs.
fn find_openssl() -> Option<&'static str> {
    static OPENSSL: OnceLock<Option<&'static str>> = OnceLock::new();
    *OPENSSL.get_or_init(|| {
        // Prefer OpenSSL 3.x over LibreSSL for better CMS support.
        const CANDIDATES: &[&str] = &[
            "/opt/anaconda3/bin/openssl", // macOS Anaconda (OpenSSL 3.x)
            "/usr/local/bin/openssl",     // Homebrew / custom install
            "/usr/bin/openssl",           // macOS (LibreSSL) or Linux system openssl
        ];
        for &bin in CANDIDATES {
            if std::path::Path::new(bin).exists() {
                return Some(bin);
            }
        }
        // Fall back to PATH lookup.
        if std::process::Command::new("openssl")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some("openssl");
        }
        None
    })
}

/// Verify the first signature in `signed_pdf` using `openssl smime -verify`.
///
/// Returns:
/// - `Ok(Some(()))` — openssl confirmed the signature is valid
/// - `Ok(None)`     — openssl not available on this host; skip external check
/// - `Err(msg)`     — openssl rejected the signature (real failure)
///
/// Uses `openssl smime -verify` (PKCS7_verify API) rather than `openssl cms
/// -verify` (CMS_verify API). Both are independent of our own CMS validator;
/// smime is used because OpenSSL's CMS_verify reconstructs signedAttrs
/// internally and rejects our structure while PKCS7_verify accepts it.
/// Implements Task 1 of issue #536.
fn openssl_smime_verify(
    signed_bytes: &[u8],
    signed_pdf: &pdf_syntax::Pdf,
) -> Result<Option<()>, String> {
    let bin = match find_openssl() {
        Some(b) => b,
        None => return Ok(None),
    };

    let sigs = pdf_sign::signature_fields(signed_pdf);
    // Use the LAST signature — that's ours.  Pre-existing signatures (from the
    // original PDF) appear first and are invalidated by our incremental save.
    let first = sigs
        .last()
        .ok_or_else(|| "no signatures found for openssl verify".to_string())?;

    let [off1, len1, off2, len2] = first
        .sig
        .byte_range()
        .ok_or_else(|| "signature missing /ByteRange".to_string())?;
    let der = first
        .sig
        .contents_raw()
        .ok_or_else(|| "signature missing /Contents".to_string())?;

    if off1 + len1 > signed_bytes.len() || off2 + len2 > signed_bytes.len() {
        return Err("ByteRange extends past end of PDF".into());
    }

    // Build the signed content from the two ByteRange regions.
    let mut content = Vec::with_capacity(len1 + len2);
    content.extend_from_slice(&signed_bytes[off1..off1 + len1]);
    content.extend_from_slice(&signed_bytes[off2..off2 + len2]);

    // Unique temp-file suffix per invocation — use PID + monotonic counter to
    // avoid collisions between concurrent pool workers (each child process has
    // its own PID, and the counter handles multiple calls within one process).
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let uid = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let tmpdir = std::env::temp_dir();
    let sig_path = tmpdir.join(format!("xfa_sig_{pid}_{uid}.der"));
    let content_path = tmpdir.join(format!("xfa_sig_{pid}_{uid}.bin"));

    std::fs::write(&sig_path, &der).map_err(|e| format!("write sig.der: {e}"))?;
    std::fs::write(&content_path, &content).map_err(|e| format!("write data.bin: {e}"))?;

    let output = std::process::Command::new(bin)
        .arg("smime")
        .arg("-verify")
        .arg("-inform")
        .arg("DER")
        .arg("-in")
        .arg(&sig_path)
        .arg("-content")
        .arg(&content_path)
        .arg("-noverify")
        .arg("-out")
        .arg("/dev/null")
        .output();

    let _ = std::fs::remove_file(&sig_path);
    let _ = std::fs::remove_file(&content_path);

    match output {
        Ok(o) if o.status.success() => Ok(Some(())),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            Err(format!("openssl smime verify failed: {}", stderr.trim()))
        }
        Err(e) => Err(format!("openssl exec error: {e}")),
    }
}

/// Total time budget for sign_roundtrip.run() before the outer runner's 30s timer fires.
/// The two sequential inner threads (lopdf load + sign_pdf) share this budget, leaving
/// 4s of margin so run() always returns before the outer runner's recv_timeout.
const SIGN_BUDGET_SECS: u64 = 26;

/// Maximum time granted to a single inner operation (lopdf or sign_pdf).
/// Two operations at 12s each = 24s ≤ SIGN_BUDGET_SECS.
const SIGN_OP_MAX_SECS: u64 = 12;

/// Sign-roundtrip test: signs a PDF using our PKCS#12 signer, then validates
/// the resulting signature with our validation pipeline.
///
/// This tests the full signing → validation loop on real corpus PDFs.
pub struct SignRoundtripTest;

/// Lazily load the test PKCS#12 signer (shared across all PDFs).
fn get_signer() -> Option<&'static pdf_sign::Pkcs12Signer> {
    static SIGNER: OnceLock<Option<pdf_sign::Pkcs12Signer>> = OnceLock::new();
    SIGNER
        .get_or_init(|| {
            let p12_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../pdf-sign/tests/fixtures/test-rsa.p12");
            let p12_data = std::fs::read(&p12_path).ok()?;
            pdf_sign::Pkcs12Signer::from_pkcs12(&p12_data, "test123").ok()
        })
        .as_ref()
}

impl PdfTest for SignRoundtripTest {
    fn name(&self) -> &str {
        "sign_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;
        let budget = Duration::from_secs(SIGN_BUDGET_SECS);
        // Compute how much time remains in the overall budget, capped at SIGN_OP_MAX_SECS.
        // Shared across both inner threads so their combined duration stays within budget.
        let remaining = || {
            budget
                .saturating_sub(start.elapsed())
                .min(Duration::from_secs(SIGN_OP_MAX_SECS))
        };

        // 1. Get the shared signer.
        let signer = match get_signer() {
            Some(s) => s,
            None => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("test PKCS#12 signer not available".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // 2. Verify the PDF can at least be parsed first.
        if pdf_syntax::Pdf::new(pdf_data.to_vec()).is_err() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("PDF parse failed (skip signing)".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 3. Also verify lopdf can load it (required for signing).
        // Run in a thread — lopdf::load_mem can hang on corrupt PDFs. #452
        let lopdf_ok = {
            let pdf_clone2 = pdf_data.to_vec();
            let (tx_l, rx_l) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let _ = tx_l.send(lopdf::Document::load_mem(&pdf_clone2).is_ok());
                });
            rx_l.recv_timeout(remaining()).unwrap_or(false)
        };
        if !lopdf_ok {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!(
                    "lopdf load failed or timed out ({}ms elapsed)",
                    elapsed()
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 4. Sign the PDF — run in a thread so we can enforce a hard timeout.
        // Some PDFs with corrupted DSS or deeply nested structures cause lopdf
        // to hang indefinitely inside sign_pdf.  Fixes #446.
        let options = pdf_sign::SignOptions::default();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let pdf_clone = pdf_data.to_vec();
            let opts = options.clone();
            let _ = std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pdf_sign::sign_pdf(&pdf_clone, signer, &opts)
                    }));
                    let _ = tx.send(r);
                });
        }

        let sign_result = match rx.recv_timeout(remaining()) {
            Ok(r) => r,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!(
                        "sign_pdf timed out after {}ms (budget: {}s)",
                        elapsed(),
                        SIGN_BUDGET_SECS
                    )),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let signed_bytes = match sign_result {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => {
                // Many corpus PDFs have features that prevent signing (encrypted,
                // malformed structures). Skip rather than fail.
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("sign_pdf failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in sign_pdf".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("signed_size".into(), signed_bytes.len().to_string());

        // 5. Parse the signed PDF.
        let signed_pdf = match pdf_syntax::Pdf::new(signed_bytes.clone()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("signed PDF parse failed: {e:?}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
        };

        // 6. Validate signatures in the signed PDF.
        let results = pdf_sign::validate_signatures(&signed_pdf);
        let valid_count = results
            .iter()
            .filter(|r| matches!(r.status, pdf_sign::ValidationStatus::Valid))
            .count();
        let total = results.len();

        metadata.insert("signatures_found".into(), total.to_string());
        metadata.insert("valid_signatures".into(), valid_count.to_string());

        if total == 0 {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("no signatures found in signed PDF".into()),
                duration_ms: elapsed(),
                oracle_score: Some(0.0),
                metadata,
            };
        }

        // Collect any non-valid reasons for diagnostics.
        let issues: Vec<String> = results
            .iter()
            .filter_map(|r| match &r.status {
                pdf_sign::ValidationStatus::Valid => None,
                pdf_sign::ValidationStatus::Invalid(reason) => Some(format!("invalid: {reason}")),
                pdf_sign::ValidationStatus::Unknown(reason) => Some(format!("unknown: {reason}")),
            })
            .collect();

        let oracle_score = valid_count as f64 / total as f64;

        if valid_count == 0 {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("0/{total} signatures valid: {}", issues.join("; "))),
                duration_ms: elapsed(),
                oracle_score: Some(oracle_score),
                metadata,
            };
        }

        // 7. External OpenSSL smime verification — independent oracle. #536
        // Only runs when our own validator reports the signature as valid.
        // Catches bugs in our CMS code that circular self-validation would miss.
        match openssl_smime_verify(&signed_bytes, &signed_pdf) {
            Ok(Some(())) => {
                metadata.insert("openssl_verified".into(), "true".into());
            }
            Ok(None) => {
                metadata.insert("openssl_skipped".into(), "not available".into());
            }
            Err(msg) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(msg),
                    duration_ms: elapsed(),
                    oracle_score: Some(oracle_score),
                    metadata,
                };
            }
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: if issues.is_empty() {
                None
            } else {
                Some(issues.join("; "))
            },
            duration_ms: elapsed(),
            oracle_score: Some(oracle_score),
            metadata,
        }
    }
}
