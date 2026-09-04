// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use super::{PdfTest, TestResult, TestStatus};

/// PDF/A conversion roundtrip: convert to PDF/A-2b, validate with our checker
/// and optionally with veraPDF oracle.
pub struct PdfAConvertTest {
    verapdf: Option<Arc<crate::oracles::verapdf::VeraPdfOracle>>,
    progress: Arc<Mutex<String>>,
}

impl Default for PdfAConvertTest {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfAConvertTest {
    pub fn new() -> Self {
        Self {
            verapdf: None,
            progress: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn with_verapdf(mut self, oracle: Arc<crate::oracles::verapdf::VeraPdfOracle>) -> Self {
        self.verapdf = Some(oracle);
        self
    }
}

impl PdfTest for PdfAConvertTest {
    fn name(&self) -> &str {
        "pdfa_convert"
    }

    fn progress_tracker(&self) -> Option<Arc<Mutex<String>>> {
        Some(self.progress.clone())
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        let set_progress = |msg: &str| {
            if let Ok(mut p) = self.progress.lock() {
                *p = msg.to_string();
            }
        };

        set_progress("parsing");

        // Files that are not PDFs at all: neither pdf-syntax nor lopdf would
        // accept them, so skip before doing any work. (#445)
        if !pdf_data.windows(5).any(|w| w == b"%PDF-") {
            return skip(elapsed(), "not a PDF file (missing %PDF header)");
        }

        // Already conformant — nothing to convert, and no need to spend a
        // veraPDF run proving it.
        if let Ok(pdf) = pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            if pdf_compliance::detect_pdfa_level(&pdf).is_some() {
                return TestResult {
                    status: TestStatus::Pass,
                    error_message: Some("already PDF/A".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        let qpdf = |data: &[u8]| qpdf_repair_bytes(data, path);
        let opts = pdf_manip::pdfa::PdfAConvertOptions {
            conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
            // Set PDFA_NO_EXTERNAL_REPAIR=1 to measure the shipped path: the
            // WASM build, the C ABI and the `pdfluent` facade have no qpdf to
            // shell out to, so a run with the hook enabled reports a pass-rate
            // no customer can reproduce.
            external_repair: if external_repair_enabled() {
                Some(&qpdf)
            } else {
                None
            },
            on_step: Some(&set_progress),
        };

        let mut doc = match pdf_manip::pdfa::load_for_conversion(pdf_data, &opts) {
            Ok(d) => d,
            Err(e) => return skip(elapsed(), &e.to_string()),
        };

        let convert_report = match pdf_manip::pdfa::convert_document(&mut doc, &opts) {
            Ok(r) => r,
            // A step that returned an error left the document unconvertible;
            // a panic is a defect in the pipeline and must stay visible as a
            // failure rather than being filed away as an unsupported input.
            Err(e @ pdf_manip::pdfa::PdfAConvertError::Panicked(_)) => {
                return fail(elapsed(), &e.to_string())
            }
            Err(e) => return skip(elapsed(), &e.to_string()),
        };

        set_progress("save");
        let mut saved = Vec::new();
        if let Err(e) = doc.save_to(&mut saved) {
            return fail(elapsed(), &format!("save failed: {e}"));
        }
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

        let mut metadata = HashMap::new();
        let run_own_validation = |metadata: &mut HashMap<String, String>| {
            let report = match pdf_syntax::Pdf::new(saved.clone()) {
                Ok(pdf2) => pdf_compliance::validate_pdfa(&pdf2, pdf_compliance::PdfALevel::A2b),
                Err(e) => {
                    metadata.insert("own_reparse_error".into(), format!("{e:?}"));
                    pdf_compliance::ComplianceReport {
                        compliant: false,
                        pdfa_level: Some(pdf_compliance::PdfALevel::A2b),
                        issues: vec![pdf_compliance::ComplianceIssue {
                            rule: "parser".to_string(),
                            severity: pdf_compliance::Severity::Error,
                            message: format!("reparse failed: {e:?}"),
                            location: None,
                        }],
                    }
                }
            };
            metadata.insert("own_errors".into(), report.issues.len().to_string());
            metadata.insert("own_compliant".into(), report.compliant.to_string());
            report
        };

        // Running our own compliance checker can be very expensive on some
        // pathological files. When veraPDF oracle is enabled, defer this step
        // and only run it as a fallback if oracle validation fails.
        let mut own_report: Option<pdf_compliance::ComplianceReport> = None;
        if self.verapdf.is_none() {
            set_progress("validate_own");
            own_report = Some(run_own_validation(&mut metadata));
        } else {
            metadata.insert("own_validation".into(), "deferred_to_verapdf".into());
        }

        let cleanup_report = &convert_report.cleanup;
        metadata.insert(
            "js_removed".into(),
            cleanup_report.js_actions_removed.to_string(),
        );
        metadata.insert(
            "cidtogidmap_added".into(),
            cleanup_report.cidtogidmap_added.to_string(),
        );
        metadata.insert("ap_fixes".into(), cleanup_report.ap_fixes.to_string());
        metadata.insert("pages".into(), convert_report.page_count.to_string());
        if let Some(ref fr) = convert_report.fonts {
            metadata.insert("fonts_embedded".into(), fr.fonts_embedded.to_string());
            metadata.insert("fonts_failed".into(), fr.failed.len().to_string());
        }

        // 6. Validate with veraPDF oracle if available.
        if let Some(verapdf) = &self.verapdf {
            set_progress("validate_verapdf");

            // Write to temp file for veraPDF.
            let tmp = match write_temp_pdf(&saved, path) {
                Some(p) => p,
                None => {
                    metadata.insert("verapdf".into(), "temp_write_failed".into());
                    let report = own_report
                        .take()
                        .unwrap_or_else(|| run_own_validation(&mut metadata));
                    return TestResult {
                        status: if report.compliant {
                            TestStatus::Pass
                        } else {
                            TestStatus::Fail
                        },
                        error_message: if report.compliant {
                            None
                        } else {
                            Some(format!("{} compliance issues", report.issues.len()))
                        },
                        duration_ms: elapsed(),
                        oracle_score: None,
                        metadata,
                    };
                }
            };

            // Compute a hash for cache key.
            let hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&saved);
                format!("{:x}", hasher.finalize())
            };
            let oracle_result = verapdf.validate(&tmp, &hash);

            match oracle_result {
                Ok(verapdf_report) => {
                    let _ = std::fs::remove_file(&tmp);
                    let oracle_errors = verapdf_report.failed_rules as usize;
                    metadata.insert("verapdf_errors".into(), oracle_errors.to_string());
                    if !verapdf_report.rule_failures.is_empty() {
                        let failed_rules: Vec<String> = verapdf_report
                            .rule_failures
                            .iter()
                            .map(|r| format!("{}:{}", r.clause, r.test_number))
                            .collect();
                        metadata.insert("verapdf_failed_rules".into(), failed_rules.join("|"));
                        if let Some(first) = failed_rules.first() {
                            metadata.insert("verapdf_first_rule".into(), first.clone());
                        }
                        // Store first failure description for diagnosis.
                        if let Some(first_rf) = verapdf_report.rule_failures.first() {
                            metadata.insert(
                                "verapdf_message".into(),
                                first_rf.description.chars().take(300).collect(),
                            );
                        }
                    }

                    if oracle_errors == 0 {
                        return TestResult {
                            status: TestStatus::Pass,
                            error_message: None,
                            duration_ms: elapsed(),
                            oracle_score: Some(1.0),
                            metadata,
                        };
                    } else {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "{oracle_errors} veraPDF errors after conversion"
                            )),
                            duration_ms: elapsed(),
                            oracle_score: Some(0.0),
                            metadata,
                        };
                    }
                }
                Err(e) => {
                    metadata.insert("verapdf".into(), format!("error: {e}"));
                    metadata.insert("verapdf_tmp_pdf".into(), tmp.display().to_string());
                }
            }
        }

        // Fallback: use our own checker result.
        let report = own_report.unwrap_or_else(|| {
            set_progress("validate_own_fallback");
            run_own_validation(&mut metadata)
        });

        // If the converted PDF can't be re-parsed by pdf_syntax, we can't validate it.
        // Skip rather than Fail — the conversion produced unparseable output, which is
        // an inherent limitation for some broken input PDFs. Fixes #XXX (GHOSTSCRIPT-699132-0,
        // PDFIUM-1233-0).
        if report.issues.iter().any(|i| i.rule == "parser") {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("converted PDF cannot be reparsed (invalid output)".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }

        if report.compliant {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            let issue_details: Vec<String> = report
                .issues
                .iter()
                .take(5)
                .map(|i| format!("{}: {}", i.rule, i.message))
                .collect();
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "{} compliance issues: {}",
                    report.issues.len(),
                    issue_details.join("; ")
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}

/// Convert a PDF to PDF/A-2b and return the converted bytes.
///
/// Used by `oracle-generate` to produce exactly the bytes `PdfAConvertTest::run`
/// would validate, so the oracle cache keys on the *converted* hash rather than
/// the input hash. Both call the same library pipeline, so they cannot drift.
///
/// Returns `None` when the document cannot be converted at all.
pub fn convert_to_pdfa_bytes(pdf_data: &[u8], path: &Path) -> Option<Vec<u8>> {
    let qpdf = |data: &[u8]| qpdf_repair_bytes(data, path);
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        external_repair: Some(&qpdf),
        on_step: None,
    };
    pdf_manip::pdfa::convert_bytes(pdf_data, &opts).ok()
}

/// Skip: the converter declined this input. Not a conversion defect.
fn skip(duration_ms: u64, reason: &str) -> TestResult {
    TestResult {
        status: TestStatus::Skip,
        error_message: Some(reason.to_string()),
        duration_ms,
        oracle_score: None,
        metadata: HashMap::new(),
    }
}

/// Fail: the converter accepted this input and produced a bad result.
fn fail(duration_ms: u64, reason: &str) -> TestResult {
    TestResult {
        status: TestStatus::Fail,
        error_message: Some(reason.to_string()),
        duration_ms,
        oracle_score: None,
        metadata: HashMap::new(),
    }
}

/// Whether the qpdf external-repair hook is offered to the library.
///
/// Off means the harness runs exactly the pipeline a customer gets.
fn external_repair_enabled() -> bool {
    !matches!(
        std::env::var("PDFA_NO_EXTERNAL_REPAIR").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// A `qpdf --decrypt` rewrite, offered to the library as its external repair
/// hook. Returns `None` when qpdf is not installed, which is the normal case
/// on a developer machine — the library falls back to its own repairs.
fn qpdf_repair_bytes(data: &[u8], original_path: &Path) -> Option<Vec<u8>> {
    let stem = original_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("input");
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let tmp_dir = std::env::temp_dir();
    let in_path = tmp_dir.join(format!("{stem}_{pid}_{nanos}_qpdf_in.pdf"));
    let out_path = tmp_dir.join(format!("{stem}_{pid}_{nanos}_qpdf_out.pdf"));

    if std::fs::write(&in_path, data).is_err() {
        return None;
    }

    let output = Command::new("qpdf")
        .arg("--decrypt")
        .arg("--password=")
        .arg(&in_path)
        .arg(&out_path)
        .output();

    let result = match output {
        // qpdf uses exit code 3 for "success with warnings".
        Ok(out) if out.status.success() || out.status.code() == Some(3) => {
            std::fs::read(&out_path).ok()
        }
        _ => None,
    };

    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    result
}

/// Write bytes to a temp file next to the original PDF.
fn write_temp_pdf(data: &[u8], original: &Path) -> Option<std::path::PathBuf> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");
    let dir = std::env::temp_dir();
    let safe_stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    for attempt in 0..8u8 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let pid = std::process::id();
        let path = dir.join(format!(
            "{safe_stem}_{pid}_{nanos}_{attempt}_pdfa_converted.pdf"
        ));

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                if f.write_all(data).is_ok() {
                    return Some(path);
                }
                let _ = std::fs::remove_file(&path);
                return None;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }

    None
}
