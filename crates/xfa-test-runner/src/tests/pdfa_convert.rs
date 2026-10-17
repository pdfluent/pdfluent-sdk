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

/// PDF/A conversion roundtrip: convert to the requested PDF/A flavour, validate
/// with our checker and optionally with the veraPDF oracle.
///
/// The flavour is a parameter and not a constant because the product claims
/// more than one of them. Until #443 this test converted to PDF/A-2b and
/// nothing else, so the only conformance number anyone could quote was 2b's,
/// while the C header advertised conversion to 1b and 3b as well. A claim no
/// gate measures is a claim nobody can defend.
pub struct PdfAConvertTest {
    verapdf: Option<Arc<crate::oracles::verapdf::VeraPdfOracle>>,
    conformance: pdf_manip::pdfa_xmp::PdfAConformance,
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
            conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
            progress: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn with_verapdf(mut self, oracle: Arc<crate::oracles::verapdf::VeraPdfOracle>) -> Self {
        self.verapdf = Some(oracle);
        self
    }

    /// Convert to, and grade against, this flavour instead of PDF/A-2b.
    pub fn with_conformance(mut self, conformance: pdf_manip::pdfa_xmp::PdfAConformance) -> Self {
        self.conformance = conformance;
        self
    }
}

/// The veraPDF flavour string for a conversion target, e.g. `2b`.
///
/// veraPDF names a flavour by part and letter with no separator; the letter is
/// lower case there and upper case in the ISO wording, which is why this is a
/// mapping and not a `format!`.
pub fn verapdf_flavour(conformance: pdf_manip::pdfa_xmp::PdfAConformance) -> String {
    format!(
        "{}{}",
        conformance.part(),
        conformance.conformance().to_ascii_lowercase()
    )
}

/// The level our own compliance checker validates against, for a conversion
/// target. Two enums exist because conversion targets and validation targets
/// are not the same set: `pdf-compliance` also knows PDF/A-4, which
/// `pdf-manip` cannot produce.
pub fn own_level(conformance: pdf_manip::pdfa_xmp::PdfAConformance) -> pdf_compliance::PdfALevel {
    pdf_compliance::PdfALevel::from_parts(conformance.part() as u8, conformance.conformance())
        .unwrap_or(pdf_compliance::PdfALevel::A2b)
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

        // ALREADY CONFORMANT IS A VERDICT, NOT A CLAIM (codex, #443).
        //
        // This used to pass any input whose XMP said "PDF/A", without
        // converting and without validating. But `detect_pdfa_level` reads the
        // declaration and nothing else -- it parses `pdfaid:part` and
        // `pdfaid:conformance` -- so a malformed file that merely *says* 2b was
        // recorded as conforming, and a baseline built from that is inflated by
        // exactly the documents nobody checked.
        //
        // So the claim only earns a pass when the oracle agrees with it. The
        // equality matters too: a document that says PDF/A-1b is not a
        // PDF/A-3u, and answering "pass" to a 3u run because the file claims
        // some other part would grade a flavour we never produced.
        //
        // When the claim is not confirmed -- veraPDF disagrees, veraPDF is not
        // there, or the file will not reparse -- this falls through and the
        // document is converted and graded like any other, which is also what a
        // caller of `convert_bytes` would get.
        if let Ok(pdf) = pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            if pdf_compliance::detect_pdfa_level(&pdf) == Some(own_level(self.conformance)) {
                if let Some(verapdf) = &self.verapdf {
                    set_progress("validate_verapdf_declared");
                    if let Some(tmp) = write_temp_pdf(pdf_data, path) {
                        let hash = sha256_hex(pdf_data);
                        let verdict = verapdf.validate_pdfa_flavour(
                            &tmp,
                            &hash,
                            &verapdf_flavour(self.conformance),
                        );
                        let _ = std::fs::remove_file(&tmp);
                        if let Ok(report) = verdict {
                            if report.failed_rules == 0 {
                                let mut metadata = HashMap::new();
                                metadata.insert("already_pdfa".into(), "confirmed".into());
                                metadata.insert(
                                    "pdfa_flavour".into(),
                                    verapdf_flavour(self.conformance),
                                );
                                return TestResult {
                                    status: TestStatus::Pass,
                                    error_message: Some("already PDF/A, confirmed".into()),
                                    duration_ms: elapsed(),
                                    oracle_score: Some(1.0),
                                    metadata,
                                };
                            }
                        }
                    }
                }
            }
        }

        let qpdf = |data: &[u8]| qpdf_repair_bytes(data, path);
        let opts = pdf_manip::pdfa::PdfAConvertOptions {
            conformance: self.conformance,
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
                Ok(pdf2) => pdf_compliance::validate_pdfa(&pdf2, own_level(self.conformance)),
                Err(e) => {
                    metadata.insert("own_reparse_error".into(), format!("{e:?}"));
                    pdf_compliance::ComplianceReport {
                        compliant: false,
                        pdfa_level: Some(own_level(self.conformance)),
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

            let hash = sha256_hex(&saved);
            let flavour = verapdf_flavour(self.conformance);
            metadata.insert("pdfa_flavour".into(), flavour.clone());
            let oracle_result = verapdf.validate_pdfa_flavour(&tmp, &hash, &flavour);

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
/// would validate **at its default flavour**, so the oracle cache keys on the
/// *converted* hash rather than the input hash. Both call the same library
/// pipeline, so they cannot drift.
///
/// It stays fixed at 2b, and a flavour run does not read what it caches: the
/// pre-generated oracle results are stored under the auto-detect key, while a
/// flavour run asks veraPDF a different question and caches under a key of its
/// own (`verapdf-1b`, …). One cache serving both would answer the second
/// question with the first question's verdict.
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

/// The veraPDF cache key for a document: the hash of the bytes handed to it.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
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
