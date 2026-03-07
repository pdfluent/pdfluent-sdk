//! veraPDF compliance oracle — calls the veraPDF CLI and parses JSON output.
//!
//! veraPDF is the EU-funded reference validator for PDF/A (ISO 19005).
//! We use it as a ground truth oracle to detect false negatives in our
//! pdf-compliance crate.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::Deserialize;

use crate::db::Database;

/// Result of a veraPDF validation run.
#[derive(Debug, Clone)]
pub struct VeraPdfResult {
    pub is_compliant: bool,
    pub profile_name: String,
    pub passed_rules: u32,
    pub failed_rules: u32,
    pub passed_checks: u32,
    pub failed_checks: u32,
    pub rule_failures: Vec<RuleFailure>,
    pub duration_ms: u64,
}

/// A single failed rule from veraPDF output.
#[derive(Debug, Clone)]
pub struct RuleFailure {
    pub specification: String,
    pub clause: String,
    pub test_number: u32,
    pub description: String,
}

/// Comparison between our engine and veraPDF.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ComplianceComparison {
    pub our_compliant: bool,
    pub verapdf_compliant: bool,
    /// Rules veraPDF flags that we miss — these are bugs.
    pub false_negatives: Vec<String>,
    /// Rules we flag that veraPDF does not — acceptable (we're stricter).
    pub false_positives: Vec<String>,
    pub agreement_rate: f64,
}

/// The veraPDF oracle. Wraps the CLI binary and optional result caching.
pub struct VeraPdfOracle {
    binary_path: PathBuf,
    db: Option<std::sync::Arc<Database>>,
}

impl std::fmt::Debug for VeraPdfOracle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VeraPdfOracle")
            .field("binary_path", &self.binary_path)
            .finish()
    }
}

impl VeraPdfOracle {
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            binary_path,
            db: None,
        }
    }

    pub fn with_cache(mut self, db: std::sync::Arc<Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// Check if the veraPDF binary is available.
    pub fn is_available(&self) -> bool {
        Command::new(&self.binary_path)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Validate a PDF with veraPDF. Returns cached result if available.
    pub fn validate(&self, pdf_path: &Path, pdf_hash: &str) -> Result<VeraPdfResult, String> {
        // Check cache first
        if let Some(db) = &self.db {
            if let Some(cached) = db.get_oracle_cache("verapdf", pdf_hash) {
                return serde_json::from_str(&cached)
                    .map_err(|e| format!("cache deserialize error: {e}"));
            }
        }

        let result = self.run_verapdf(pdf_path)?;

        // Store in cache
        if let Some(db) = &self.db {
            if let Ok(json) = serde_json::to_string(&result) {
                let _ = db.set_oracle_cache("verapdf", pdf_hash, &json);
            }
        }

        Ok(result)
    }

    fn run_verapdf(&self, pdf_path: &Path) -> Result<VeraPdfResult, String> {
        let start = Instant::now();

        let output = Command::new(&self.binary_path)
            .args([
                "--format",
                "json",
                "--flavour",
                "0", // Auto-detect profile
            ])
            .arg(pdf_path)
            .output()
            .map_err(|e| format!("failed to run veraPDF: {e}"))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !output.status.success() && output.stdout.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("veraPDF failed: {stderr}"));
        }

        let report: VeraPdfReport = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("failed to parse veraPDF JSON: {e}"))?;

        let job = report
            .report
            .jobs
            .into_iter()
            .next()
            .ok_or("veraPDF returned no jobs")?;

        let vr = match job.validation_result {
            Some(ValidationResultWrapper::Array(mut arr)) => {
                if arr.is_empty() {
                    return Err("veraPDF returned empty validation result array".to_string());
                }
                arr.remove(0)
            }
            Some(ValidationResultWrapper::Single(vr)) => vr,
            None => return Err("veraPDF returned no validation result".to_string()),
        };

        let rule_failures: Vec<RuleFailure> = vr
            .details
            .rule_summaries
            .into_iter()
            .filter(|r| r.status == "failed")
            .map(|r| RuleFailure {
                specification: r.specification,
                clause: r.clause,
                test_number: r.test_number,
                description: r.description,
            })
            .collect();

        Ok(VeraPdfResult {
            is_compliant: vr.compliant,
            profile_name: vr.profile_name,
            passed_rules: vr.details.passed_rules,
            failed_rules: vr.details.failed_rules,
            passed_checks: vr.details.passed_checks,
            failed_checks: vr.details.failed_checks,
            rule_failures,
            duration_ms,
        })
    }
}

/// Compare our compliance report with veraPDF's result.
pub fn compare_compliance(
    our_report: &pdf_compliance::ComplianceReport,
    verapdf_result: &VeraPdfResult,
) -> ComplianceComparison {
    let our_rules: std::collections::HashSet<&str> = our_report
        .issues
        .iter()
        .filter(|i| i.severity == pdf_compliance::Severity::Error)
        .map(|i| i.rule.as_str())
        .collect();

    let verapdf_rules: std::collections::HashSet<&str> = verapdf_result
        .rule_failures
        .iter()
        .map(|r| r.clause.as_str())
        .collect();

    // False negatives: veraPDF says FAIL but we don't flag it
    let false_negatives: Vec<String> = verapdf_rules
        .iter()
        .filter(|r| !our_rules.contains(*r))
        .map(|r| r.to_string())
        .collect();

    // False positives: we say FAIL but veraPDF doesn't flag it
    let false_positives: Vec<String> = our_rules
        .iter()
        .filter(|r| !verapdf_rules.contains(*r))
        .map(|r| r.to_string())
        .collect();

    let total_verapdf = verapdf_result.rule_failures.len().max(1) as f64;
    let agreement_rate = 1.0 - (false_negatives.len() as f64 / total_verapdf);

    ComplianceComparison {
        our_compliant: our_report.compliant,
        verapdf_compliant: verapdf_result.is_compliant,
        false_negatives,
        false_positives,
        agreement_rate,
    }
}

/// Check if a PDF claims to be PDF/A by examining XMP metadata.
pub fn claims_pdfa(pdf_data: &[u8]) -> bool {
    // Fast path: search raw bytes for pdfaid:part
    if let Ok(text) = std::str::from_utf8(pdf_data) {
        if text.contains("pdfaid:part") || text.contains("pdfaSchema") {
            return true;
        }
    }
    // Binary search for the XMP marker
    pdf_data.windows(11).any(|w| w == b"pdfaid:part")
}

// ─── veraPDF JSON schema ───────────────────────────────────────────

#[derive(Deserialize)]
struct VeraPdfReport {
    report: ReportBody,
}

#[derive(Deserialize)]
struct ReportBody {
    jobs: Vec<VeraPdfJob>,
}

#[derive(Deserialize)]
struct VeraPdfJob {
    /// veraPDF 1.28+ wraps validationResult in an array.
    #[serde(rename = "validationResult")]
    validation_result: Option<ValidationResultWrapper>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ValidationResultWrapper {
    Array(Vec<ValidationResult>),
    Single(ValidationResult),
}

#[derive(Deserialize)]
struct ValidationResult {
    /// veraPDF <1.26: "isCompliant", veraPDF 1.28+: "compliant"
    #[serde(alias = "isCompliant")]
    compliant: bool,
    #[serde(alias = "profileName", rename = "profileName", default)]
    profile_name: String,
    details: ValidationDetails,
}

#[derive(Deserialize)]
struct ValidationDetails {
    #[serde(rename = "passedRules")]
    passed_rules: u32,
    #[serde(rename = "failedRules")]
    failed_rules: u32,
    #[serde(rename = "passedChecks")]
    passed_checks: u32,
    #[serde(rename = "failedChecks")]
    failed_checks: u32,
    #[serde(rename = "ruleSummaries")]
    #[serde(default)]
    rule_summaries: Vec<RuleSummaryJson>,
}

#[derive(Deserialize)]
struct RuleSummaryJson {
    #[serde(default)]
    specification: String,
    #[serde(default)]
    clause: String,
    #[serde(rename = "testNumber", default)]
    test_number: u32,
    #[serde(default)]
    status: String,
    #[serde(default)]
    description: String,
}

// Serialization for cache

impl serde::Serialize for VeraPdfResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("VeraPdfResult", 8)?;
        s.serialize_field("is_compliant", &self.is_compliant)?;
        s.serialize_field("profile_name", &self.profile_name)?;
        s.serialize_field("passed_rules", &self.passed_rules)?;
        s.serialize_field("failed_rules", &self.failed_rules)?;
        s.serialize_field("passed_checks", &self.passed_checks)?;
        s.serialize_field("failed_checks", &self.failed_checks)?;
        s.serialize_field("rule_failures", &self.rule_failures)?;
        s.serialize_field("duration_ms", &self.duration_ms)?;
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for VeraPdfResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Helper {
            is_compliant: bool,
            profile_name: String,
            passed_rules: u32,
            failed_rules: u32,
            passed_checks: u32,
            failed_checks: u32,
            rule_failures: Vec<RuleFailureHelper>,
            duration_ms: u64,
        }
        #[derive(Deserialize)]
        struct RuleFailureHelper {
            specification: String,
            clause: String,
            test_number: u32,
            description: String,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(VeraPdfResult {
            is_compliant: h.is_compliant,
            profile_name: h.profile_name,
            passed_rules: h.passed_rules,
            failed_rules: h.failed_rules,
            passed_checks: h.passed_checks,
            failed_checks: h.failed_checks,
            rule_failures: h
                .rule_failures
                .into_iter()
                .map(|r| RuleFailure {
                    specification: r.specification,
                    clause: r.clause,
                    test_number: r.test_number,
                    description: r.description,
                })
                .collect(),
            duration_ms: h.duration_ms,
        })
    }
}

impl serde::Serialize for RuleFailure {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("RuleFailure", 4)?;
        s.serialize_field("specification", &self.specification)?;
        s.serialize_field("clause", &self.clause)?;
        s.serialize_field("test_number", &self.test_number)?;
        s.serialize_field("description", &self.description)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_pdfa_detects_xmp() {
        // Minimal XMP with pdfaid:part
        let data = b"<rdf:RDF><pdfaid:part>2</pdfaid:part></rdf:RDF>";
        assert!(claims_pdfa(data));
    }

    #[test]
    fn claims_pdfa_rejects_plain_pdf() {
        let data = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj";
        assert!(!claims_pdfa(data));
    }

    #[test]
    fn compare_detects_false_negatives() {
        let our_report = pdf_compliance::ComplianceReport {
            compliant: true,
            issues: vec![],
            pdfa_level: None,
        };
        let verapdf = VeraPdfResult {
            is_compliant: false,
            profile_name: "PDF/A-2B".to_string(),
            passed_rules: 100,
            failed_rules: 2,
            passed_checks: 500,
            failed_checks: 3,
            rule_failures: vec![
                RuleFailure {
                    specification: "ISO 19005-2".to_string(),
                    clause: "6.1.2".to_string(),
                    test_number: 1,
                    description: "Missing OutputIntent".to_string(),
                },
                RuleFailure {
                    specification: "ISO 19005-2".to_string(),
                    clause: "6.2.3".to_string(),
                    test_number: 1,
                    description: "Font not embedded".to_string(),
                },
            ],
            duration_ms: 1000,
        };

        let cmp = compare_compliance(&our_report, &verapdf);
        assert!(!cmp.verapdf_compliant);
        assert!(cmp.our_compliant);
        assert_eq!(cmp.false_negatives.len(), 2);
        assert!(cmp.false_negatives.contains(&"6.1.2".to_string()));
        assert!(cmp.false_negatives.contains(&"6.2.3".to_string()));
        assert!(cmp.false_positives.is_empty());
    }

    #[test]
    fn compare_accepts_false_positives() {
        use pdf_compliance::{ComplianceIssue, Severity};

        let our_report = pdf_compliance::ComplianceReport {
            compliant: false,
            pdfa_level: None,
            issues: vec![ComplianceIssue {
                rule: "6.7.1".to_string(),
                severity: Severity::Error,
                message: "Extra strict check".to_string(),
                location: None,
            }],
        };
        let verapdf = VeraPdfResult {
            is_compliant: true,
            profile_name: "PDF/A-1B".to_string(),
            passed_rules: 100,
            failed_rules: 0,
            passed_checks: 500,
            failed_checks: 0,
            rule_failures: vec![],
            duration_ms: 500,
        };

        let cmp = compare_compliance(&our_report, &verapdf);
        assert!(cmp.false_negatives.is_empty());
        assert_eq!(cmp.false_positives.len(), 1);
        assert_eq!(cmp.agreement_rate, 1.0);
    }

    #[test]
    fn parse_verapdf_json() {
        let json = r#"{
            "report": {
                "jobs": [{
                    "validationResult": {
                        "isCompliant": false,
                        "profileName": "PDF/A-2B",
                        "details": {
                            "passedRules": 95,
                            "failedRules": 2,
                            "passedChecks": 480,
                            "failedChecks": 5,
                            "ruleSummaries": [
                                {
                                    "specification": "ISO 19005-2",
                                    "clause": "6.1.2",
                                    "testNumber": 1,
                                    "status": "failed",
                                    "description": "test failure"
                                },
                                {
                                    "specification": "ISO 19005-2",
                                    "clause": "6.3.1",
                                    "testNumber": 2,
                                    "status": "passed",
                                    "description": "passed rule"
                                }
                            ]
                        }
                    }
                }]
            }
        }"#;

        let report: VeraPdfReport = serde_json::from_str(json).unwrap();
        let job = &report.report.jobs[0];
        let wrapper = job.validation_result.as_ref().unwrap();
        let vr = match wrapper {
            ValidationResultWrapper::Single(v) => v,
            ValidationResultWrapper::Array(arr) => &arr[0],
        };
        assert!(!vr.compliant);
        assert_eq!(vr.profile_name, "PDF/A-2B");
        assert_eq!(vr.details.failed_rules, 2);
        // Only "failed" rules should be captured
        let failures: Vec<_> = vr
            .details
            .rule_summaries
            .iter()
            .filter(|r| r.status == "failed")
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].clause, "6.1.2");
    }
}
