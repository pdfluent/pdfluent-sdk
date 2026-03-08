use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::verapdf::{self, VeraPdfOracle};

pub struct ComplianceTest {
    pub verapdf_oracle: Option<Arc<VeraPdfOracle>>,
}

impl ComplianceTest {
    pub fn new() -> Self {
        Self {
            verapdf_oracle: None,
        }
    }

    pub fn with_verapdf(mut self, oracle: Arc<VeraPdfOracle>) -> Self {
        self.verapdf_oracle = Some(oracle);
        self
    }
}

impl PdfTest for ComplianceTest {
    fn name(&self) -> &str {
        "compliance"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // Check if PDF claims to be PDF/A
        let is_pdfa = verapdf::claims_pdfa(pdf_data);

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e:?}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // Auto-detect PDF/A level from XMP, fall back to A1b
        let level =
            pdf_compliance::detect_pdfa_level(&pdf).unwrap_or(pdf_compliance::PdfALevel::A1b);

        let report = pdf_compliance::validate_pdfa(&pdf, level);

        let mut metadata = HashMap::new();
        metadata.insert("compliant".to_string(), report.compliant.to_string());
        metadata.insert("error_count".to_string(), report.error_count().to_string());
        metadata.insert(
            "warning_count".to_string(),
            report.warning_count().to_string(),
        );
        metadata.insert("claims_pdfa".to_string(), is_pdfa.to_string());

        // If PDF claims PDF/A and we have a veraPDF oracle, compare results
        if is_pdfa {
            if let Some(oracle) = &self.verapdf_oracle {
                let pdf_hash = sha2_hex(pdf_data);
                match oracle.validate(path, &pdf_hash) {
                    Ok(verapdf_result) => {
                        let comparison = verapdf::compare_compliance(&report, &verapdf_result);

                        metadata.insert(
                            "verapdf_compliant".to_string(),
                            comparison.verapdf_compliant.to_string(),
                        );
                        metadata.insert(
                            "verapdf_profile".to_string(),
                            verapdf_result.profile_name.clone(),
                        );
                        metadata.insert(
                            "false_negatives".to_string(),
                            comparison.false_negatives.len().to_string(),
                        );
                        metadata.insert(
                            "false_positives".to_string(),
                            comparison.false_positives.len().to_string(),
                        );
                        metadata.insert(
                            "verapdf_duration_ms".to_string(),
                            verapdf_result.duration_ms.to_string(),
                        );

                        // False negatives are bugs — we miss something veraPDF catches
                        if !comparison.false_negatives.is_empty() {
                            return TestResult {
                                status: TestStatus::Fail,
                                error_message: Some(format!(
                                    "False negatives vs veraPDF: {:?}",
                                    comparison.false_negatives
                                )),
                                duration_ms: start.elapsed().as_millis() as u64,
                                oracle_score: Some(comparison.agreement_rate),
                                metadata,
                            };
                        }

                        return TestResult {
                            status: TestStatus::Pass,
                            error_message: None,
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: Some(comparison.agreement_rate),
                            metadata,
                        };
                    }
                    Err(e) => {
                        metadata.insert("verapdf_error".to_string(), e);
                    }
                }
            }
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}

fn sha2_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
