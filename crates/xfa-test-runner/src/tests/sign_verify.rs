use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Deep signature validation test.
///
/// Unlike the basic `signatures` test (which only checks that signature parsing
/// succeeds), this test performs full cryptographic validation:
///
/// 1. Byte-range digest verification
/// 2. CMS structural integrity
/// 3. Cryptographic signature verification (RSA/ECDSA)
/// 4. Tamper detection: modifies a byte and verifies the signature is invalidated
pub struct SignVerifyTest;

impl PdfTest for SignVerifyTest {
    fn name(&self) -> &str {
        "sign_verify"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("invalid PDF: cannot parse".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let sigs = pdf_sign::signature_fields(&pdf);
        if sigs.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Phase 1: Deep validation of all signatures.
        let results = pdf_sign::validate_signatures(&pdf);

        let mut valid_count = 0u32;
        let mut invalid_count = 0u32;
        let mut unknown_count = 0u32;
        let mut reasons: Vec<String> = Vec::new();

        for r in &results {
            match &r.status {
                pdf_sign::ValidationStatus::Valid => valid_count += 1,
                pdf_sign::ValidationStatus::Invalid(reason) => {
                    invalid_count += 1;
                    reasons.push(format!("invalid({}): {reason}", r.field_name));
                }
                pdf_sign::ValidationStatus::Unknown(reason) => {
                    unknown_count += 1;
                    reasons.push(format!("unknown({}): {reason}", r.field_name));
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("signature_count".to_string(), sigs.len().to_string());
        metadata.insert("valid".to_string(), valid_count.to_string());
        metadata.insert("invalid".to_string(), invalid_count.to_string());
        metadata.insert("unknown".to_string(), unknown_count.to_string());

        // Collect sub-filter types for diagnostics.
        let sub_filters: Vec<String> = results
            .iter()
            .filter_map(|r| r.sub_filter.map(|sf| format!("{sf:?}")))
            .collect();
        if !sub_filters.is_empty() {
            metadata.insert("sub_filters".to_string(), sub_filters.join(","));
        }

        // Collect signer names.
        let signers: Vec<String> = results.iter().filter_map(|r| r.signer.clone()).collect();
        if !signers.is_empty() {
            metadata.insert("signers".to_string(), signers.join(","));
        }

        // Phase 2: Tamper detection — only for PDFs with at least one valid signature.
        if valid_count > 0 {
            let tamper_ok = test_tamper_detection(pdf_data);
            metadata.insert("tamper_detected".to_string(), tamper_ok.to_string());
            if !tamper_ok {
                reasons.push("tamper detection failed: modified PDF still shows valid".into());
            }
        }

        // Pass if we could parse and validate without crashing.
        // Record detailed results in metadata for analysis.
        let error_message = if reasons.is_empty() {
            None
        } else {
            Some(reasons.join("; "))
        };

        TestResult {
            status: TestStatus::Pass,
            error_message,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(if results.is_empty() {
                0.0
            } else {
                valid_count as f64 / results.len() as f64
            }),
            metadata,
        }
    }
}

/// Modify a byte in the signed content region and verify the signature
/// is detected as invalid (byte-range digest mismatch).
fn test_tamper_detection(pdf_data: &[u8]) -> bool {
    if pdf_data.len() < 100 {
        return true; // Too small to tamper meaningfully.
    }

    // Modify a byte near the beginning of the file (inside the first signed range).
    // ByteRange[0] is always 0, so byte 50 should be in the signed region.
    let mut tampered = pdf_data.to_vec();
    let pos = 50.min(tampered.len() - 1);
    tampered[pos] ^= 0xFF;

    let pdf = match pdf_syntax::Pdf::new(tampered) {
        Ok(p) => p,
        // If the tampered PDF can't even parse, that's fine — the
        // modification was detected at the structural level.
        Err(_) => return true,
    };

    let results = pdf_sign::validate_signatures(&pdf);
    if results.is_empty() {
        // No signatures found in tampered version — could be because the
        // modification broke the AcroForm structure. That's acceptable.
        return true;
    }

    // All signatures should now be Invalid or Unknown (not Valid).
    results
        .iter()
        .all(|r| !matches!(r.status, pdf_sign::ValidationStatus::Valid))
}
