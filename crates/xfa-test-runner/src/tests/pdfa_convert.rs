use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{PdfTest, TestResult, TestStatus};

/// PDF/A conversion roundtrip: convert to PDF/A-2b, validate with our checker
/// and optionally with veraPDF oracle.
pub struct PdfAConvertTest {
    verapdf: Option<Arc<crate::oracles::verapdf::VeraPdfOracle>>,
    progress: Arc<Mutex<String>>,
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

        // 1. Check if already PDF/A-compliant — skip.
        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("pdf-syntax parse failed".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        if pdf_compliance::detect_pdfa_level(&pdf).is_some() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("already PDF/A".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 2. Load via lopdf for mutation.
        set_progress("lopdf_load");
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
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

        if doc.get_pages().is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("0 pages".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 3. Run PDF/A conversion pipeline.
        set_progress("cleanup");
        let cleanup_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false)
        }));
        let cleanup_report = match cleanup_result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("cleanup failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in cleanup_for_pdfa".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // 3a. Embed non-embedded fonts.
        set_progress("font_embed");
        let font_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::embed_fonts(&mut doc)
        }));
        let font_report = match font_result {
            Ok(Ok(r)) => Some(r),
            _ => None,
        };

        // 3a-2. Fix metrics and CIDSet on already-embedded fonts.
        set_progress("font_metrics");
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_fonts::fix_embedded_font_metrics(&mut doc);
            pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
        }));

        // 3b. Normalize color spaces: add sRGB OutputIntent if missing.
        set_progress("colorspace");
        let colorspace_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc)
        }));
        match colorspace_result {
            Ok(Ok(cs_report)) => {
                if cs_report.output_intent_added {
                    // OutputIntent was added — good.
                    let _ = cs_report;
                }
            }
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("colorspace normalization failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in normalize_colorspaces".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        set_progress("xmp_repair");
        let xmp_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_xmp::repair_xmp_metadata(
                &mut doc,
                pdf_manip::pdfa_xmp::PdfAConformance::A2b,
                None,
            )
        }));
        match xmp_result {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("xmp repair failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in repair_xmp_metadata".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }

        // 4. Save.
        set_progress("save");
        let mut saved = Vec::new();
        if let Err(e) = doc.save_to(&mut saved) {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("save failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 4b. Fix PDF header for PDF/A compliance (binary comment).
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);

        // 5. Validate with our own checker.
        set_progress("validate_own");
        let pdf2 = match pdf_syntax::Pdf::new(saved.clone()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("reparse failed: {e:?}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let report = pdf_compliance::validate_pdfa(&pdf2, pdf_compliance::PdfALevel::A2b);

        let mut metadata = HashMap::new();
        metadata.insert("own_errors".into(), report.issues.len().to_string());
        metadata.insert("own_compliant".into(), report.compliant.to_string());
        metadata.insert(
            "js_removed".into(),
            cleanup_report.js_actions_removed.to_string(),
        );
        metadata.insert(
            "cidtogidmap_added".into(),
            cleanup_report.cidtogidmap_added.to_string(),
        );
        metadata.insert("ap_fixes".into(), cleanup_report.ap_fixes.to_string());
        if let Some(ref fr) = font_report {
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
            let _ = std::fs::remove_file(&tmp);

            match oracle_result {
                Ok(verapdf_report) => {
                    let oracle_errors = verapdf_report.failed_rules as usize;
                    metadata.insert("verapdf_errors".into(), oracle_errors.to_string());

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
                }
            }
        }

        // Fallback: use our own checker result.
        if report.compliant {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "{} compliance issues after conversion",
                    report.issues.len()
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}

/// Write bytes to a temp file next to the original PDF.
fn write_temp_pdf(data: &[u8], original: &Path) -> Option<std::path::PathBuf> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");
    let dir = std::env::temp_dir();
    let path = dir.join(format!("{stem}_pdfa_converted.pdf"));
    std::fs::write(&path, data).ok()?;
    Some(path)
}
