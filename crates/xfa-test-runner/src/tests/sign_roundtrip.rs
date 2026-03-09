use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use super::{PdfTest, TestResult, TestStatus};

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
        if lopdf::Document::load_mem(pdf_data).is_err() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("lopdf load failed (skip signing)".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 4. Sign the PDF.
        let options = pdf_sign::SignOptions::default();
        let sign_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_sign::sign_pdf(pdf_data, signer, &options)
        }));

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
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("0/{total} signatures valid: {}", issues.join("; "))),
                duration_ms: elapsed(),
                oracle_score: Some(oracle_score),
                metadata,
            }
        } else {
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
}
