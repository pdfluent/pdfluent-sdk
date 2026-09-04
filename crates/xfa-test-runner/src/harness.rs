//! Differential Quality Harness v1.
//!
//! Drives render-oracle and optional text-oracle comparisons for a corpus of
//! PDFs, producing a [`DifferentialReport`] that can be compared against a
//! frozen baseline.
//!
//! Uses the same oracle patterns as [`crate::tests::render_mupdf_oracle`] and
//! [`crate::tests::text_oracle`] but aggregates results into the shared
//! [`pdf_diff`] report types rather than the internal [`TestResult`] type.
//! The existing test modules are unchanged.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use pdf_diff::{
    build_report, DifferentialReport, DifferentialVerdict, DocDifferential, TextVerdict,
    GATE_SSIM_THRESHOLD,
};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::oracle_fault::check_mutool_output;
use crate::oracles::poppler::{self, PopplerOracle};
use crate::oracles::ssim;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Configuration for a differential harness run.
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Render DPI for mutool (default: 150).
    pub dpi: f64,
    /// SSIM threshold for the render oracle gate (default: `GATE_SSIM_THRESHOLD` = 0.75).
    pub render_ssim_threshold: f64,
    /// Text-similarity threshold for the text oracle gate (default: 0.50).
    pub text_sim_threshold: f64,
    /// Maximum pages to render per document (default: 5).
    pub max_pages: usize,
    /// Enable text oracle comparison via pdftotext (default: false).
    pub with_text: bool,
    /// Path to a JSON baseline file (`corpus/CI_BASELINE.json`-compatible).
    /// If `None`, first-run mode: no `BaselineDelta` is produced.
    pub baseline_path: Option<PathBuf>,
    /// Tolerance for the regression check (default: 0.005).
    pub baseline_tolerance: f64,
    /// Cap the number of documents processed (useful for quick spot checks).
    pub limit: Option<usize>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            dpi: 150.0,
            render_ssim_threshold: GATE_SSIM_THRESHOLD,
            text_sim_threshold: 0.50,
            max_pages: 5,
            with_text: false,
            baseline_path: None,
            baseline_tolerance: 0.005,
            limit: None,
        }
    }
}

/// Run the differential harness on a single PDF document.
///
/// Never panics: all errors are captured in the returned [`DocDifferential`].
pub fn run_doc(pdf_data: &[u8], path: &Path, cfg: &HarnessConfig) -> DocDifferential {
    let pdf_path = path.to_string_lossy().into_owned();
    let pdf_hash = hex_sha256(pdf_data);

    // Baseline skeleton for early-return cases
    let mut doc = DocDifferential {
        pdf_path: pdf_path.clone(),
        pdf_hash,
        render_ssim: None,
        text_similarity: None,
        render_verdict: DifferentialVerdict::OracleUnavailable,
        text_verdict: TextVerdict::OracleUnavailable,
        oracle_fault: false,
        oracle_fault_reason: None,
        pages_compared: 0,
        error: None,
    };

    // Check mutool availability
    if Command::new("mutool").arg("-v").output().is_err() {
        doc.error = Some("mutool not found in PATH".into());
        return doc;
    }

    // Open with our engine
    let engine_doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
        Ok(d) => d,
        Err(e) => {
            let msg = e.to_string();
            let is_skip = msg.contains("Decryption(")
                || msg.contains("PasswordProtected")
                || msg.contains("UnsupportedAlgorithm")
                || msg.contains("invalid PDF");
            doc.error = Some(if is_skip {
                format!("skip: {msg}")
            } else {
                format!("engine open: {msg}")
            });
            return doc;
        }
    };

    // Write PDF to temp file for mutool
    let uid = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp = std::env::temp_dir();
    let pdf_tmp = tmp.join(format!("xfa-harness-{pid}-{uid}.pdf"));
    if let Err(e) = std::fs::write(&pdf_tmp, pdf_data) {
        doc.error = Some(format!("write temp PDF: {e}"));
        return doc;
    }

    let page_count = engine_doc.page_count().min(cfg.max_pages);
    let render_opts = pdf_engine::RenderOptions {
        dpi: cfg.dpi,
        ..Default::default()
    };

    let mut min_ssim = 1.0_f64;
    let mut pages_compared = 0usize;
    let mut oracle_fault_detected = false;
    let mut oracle_fault_reason: Option<String> = None;

    for i in 0..page_count {
        // Render with our engine
        let our_render = match engine_doc.render_page(i, &render_opts) {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_file(&pdf_tmp);
                doc.error = Some(format!("our render page {i}: {e}"));
                return doc;
            }
        };

        // Render with mutool
        let png_tmp = tmp.join(format!("xfa-harness-{pid}-{uid}-p{i}.png"));
        let dpi_s = format!("{}", cfg.dpi as u32);
        let ok = Command::new("mutool")
            .args([
                "draw",
                "-q",
                "-r",
                &dpi_s,
                "-o",
                png_tmp.to_str().unwrap_or(""),
                pdf_tmp.to_str().unwrap_or(""),
                &format!("{}", i + 1),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ok {
            let _ = std::fs::remove_file(&png_tmp);
            // mutool render failed — skip oracle for this doc, not our bug
            let _ = std::fs::remove_file(&pdf_tmp);
            doc.render_verdict = DifferentialVerdict::OracleUnavailable;
            doc.error = Some(format!("mutool draw failed on page {}", i + 1));
            return doc;
        }

        // Check for oracle fault
        let fault = check_mutool_output(&png_tmp, pdf_data.len());
        if fault.is_fault {
            oracle_fault_detected = true;
            oracle_fault_reason = fault.reason;
            let _ = std::fs::remove_file(&png_tmp);
            break;
        }

        // Load the PNG
        let mutool_img = match image::open(&png_tmp) {
            Ok(img) => img.into_rgba8(),
            Err(e) => {
                let _ = std::fs::remove_file(&png_tmp);
                oracle_fault_detected = true;
                oracle_fault_reason = Some(format!("PNG load failed: {e}"));
                break;
            }
        };
        let _ = std::fs::remove_file(&png_tmp);

        let page_ssim = ssim::compute_ssim(
            &our_render.pixels,
            our_render.width,
            our_render.height,
            mutool_img.as_raw(),
            mutool_img.width(),
            mutool_img.height(),
        );

        if page_ssim < min_ssim {
            min_ssim = page_ssim;
        }
        pages_compared += 1;
    }

    let _ = std::fs::remove_file(&pdf_tmp);

    if oracle_fault_detected {
        doc.oracle_fault = true;
        doc.oracle_fault_reason = oracle_fault_reason;
        doc.render_verdict = DifferentialVerdict::OracleUnavailable;
        return doc;
    }

    if pages_compared == 0 {
        doc.render_verdict = DifferentialVerdict::OracleUnavailable;
        doc.error = doc.error.or(Some("no pages compared".into()));
        return doc;
    }

    doc.render_ssim = Some(min_ssim);
    doc.pages_compared = pages_compared;
    doc.render_verdict = if min_ssim >= cfg.render_ssim_threshold {
        DifferentialVerdict::Match
    } else {
        DifferentialVerdict::Regression
    };

    // Optional text oracle
    if cfg.with_text {
        doc.text_verdict = run_text_oracle(path, &engine_doc, cfg.text_sim_threshold, &mut doc);
    }

    doc
}

fn run_text_oracle(
    path: &Path,
    engine_doc: &pdf_engine::PdfDocument,
    threshold: f64,
    doc: &mut DocDifferential,
) -> TextVerdict {
    if !PopplerOracle::is_available() {
        return TextVerdict::OracleUnavailable;
    }

    let poppler_text = match PopplerOracle::extract_all_text(path) {
        Ok(t) => t,
        Err(_) => return TextVerdict::OracleUnavailable,
    };

    let our_text = engine_doc.extract_all_text();
    let our_norm = poppler::normalize_text(&our_text);
    let pop_norm = poppler::normalize_text(&poppler_text);

    if our_norm.is_empty() && pop_norm.is_empty() {
        return TextVerdict::OracleUnavailable;
    }

    let sim = poppler::text_similarity(&our_norm, &pop_norm);
    doc.text_similarity = Some(sim);

    // Require at least 50 poppler chars to avoid false failures on scanned PDFs
    if pop_norm.len() < 50 {
        return TextVerdict::OracleUnavailable;
    }

    if sim >= threshold {
        TextVerdict::Match
    } else {
        TextVerdict::Regression
    }
}

/// Run the differential harness over an entire corpus directory.
///
/// Walks `corpus_dir` for `*.pdf` files, processes up to `cfg.limit` documents
/// in parallel, and assembles a [`DifferentialReport`].
pub fn run_corpus(
    corpus_dir: &Path,
    cfg: &HarnessConfig,
    workers: usize,
    run_id: &str,
    git_commit: &str,
) -> DifferentialReport {
    use walkdir::WalkDir;

    let mut pdf_paths: Vec<PathBuf> = WalkDir::new(corpus_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    pdf_paths.sort();

    if let Some(limit) = cfg.limit {
        pdf_paths.truncate(limit);
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers.max(1))
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    let docs: Vec<DocDifferential> = pool.install(|| {
        pdf_paths
            .par_iter()
            .map(|path| {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let data = match std::fs::read(path) {
                        Ok(d) => d,
                        Err(e) => {
                            return DocDifferential {
                                pdf_path: path.to_string_lossy().into_owned(),
                                pdf_hash: String::new(),
                                render_ssim: None,
                                text_similarity: None,
                                render_verdict: DifferentialVerdict::OracleUnavailable,
                                text_verdict: TextVerdict::OracleUnavailable,
                                oracle_fault: false,
                                oracle_fault_reason: None,
                                pages_compared: 0,
                                error: Some(format!("read error: {e}")),
                            };
                        }
                    };
                    run_doc(&data, path, cfg)
                }));

                match result {
                    Ok(doc) => doc,
                    Err(_) => DocDifferential {
                        pdf_path: path.to_string_lossy().into_owned(),
                        pdf_hash: String::new(),
                        render_ssim: None,
                        text_similarity: None,
                        render_verdict: DifferentialVerdict::OracleUnavailable,
                        text_verdict: TextVerdict::OracleUnavailable,
                        oracle_fault: false,
                        oracle_fault_reason: None,
                        pages_compared: 0,
                        error: Some("panic in run_doc".into()),
                    },
                }
            })
            .collect()
    });

    let baseline_pass_rate = load_baseline_pass_rate(cfg.baseline_path.as_deref());

    let timestamp = {
        // Use simple formatting without Date::now() (deterministic per-environment)
        // Callers that need wall-clock time should pass the run_id with a timestamp in it.
        chrono::Utc::now().to_rfc3339()
    };

    build_report(
        run_id.to_string(),
        git_commit.to_string(),
        timestamp,
        docs,
        baseline_pass_rate,
        cfg.baseline_tolerance,
    )
}

/// Load the `pass_rate` field from a `CI_BASELINE.json`-compatible file.
///
/// Returns `None` on any error (missing file, malformed JSON, missing field).
fn load_baseline_pass_rate(path: Option<&Path>) -> Option<f64> {
    let path = path?;
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("pass_rate")?.as_f64()
}

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_diff::{build_summary, DifferentialVerdict, TextVerdict};

    fn make_match(ssim: f64) -> DocDifferential {
        DocDifferential {
            pdf_path: "a.pdf".into(),
            pdf_hash: "abc".into(),
            render_ssim: Some(ssim),
            text_similarity: None,
            render_verdict: DifferentialVerdict::Match,
            text_verdict: TextVerdict::OracleUnavailable,
            oracle_fault: false,
            oracle_fault_reason: None,
            pages_compared: 1,
            error: None,
        }
    }

    fn make_regression(ssim: f64) -> DocDifferential {
        DocDifferential {
            pdf_path: "b.pdf".into(),
            pdf_hash: "def".into(),
            render_ssim: Some(ssim),
            text_similarity: None,
            render_verdict: DifferentialVerdict::Regression,
            text_verdict: TextVerdict::OracleUnavailable,
            oracle_fault: false,
            oracle_fault_reason: None,
            pages_compared: 1,
            error: None,
        }
    }

    fn make_oracle_fault() -> DocDifferential {
        DocDifferential {
            pdf_path: "c.pdf".into(),
            pdf_hash: "ghi".into(),
            render_ssim: None,
            text_similarity: None,
            render_verdict: DifferentialVerdict::OracleUnavailable,
            text_verdict: TextVerdict::OracleUnavailable,
            oracle_fault: true,
            oracle_fault_reason: Some("mutool PNG too small".into()),
            pages_compared: 0,
            error: None,
        }
    }

    #[test]
    fn harness_config_defaults() {
        let cfg = HarnessConfig::default();
        assert_eq!(cfg.dpi, 150.0);
        assert_eq!(cfg.render_ssim_threshold, GATE_SSIM_THRESHOLD);
        assert_eq!(cfg.text_sim_threshold, 0.50);
        assert_eq!(cfg.max_pages, 5);
        assert!(!cfg.with_text);
        assert!(cfg.baseline_path.is_none());
        assert_eq!(cfg.baseline_tolerance, 0.005);
        assert!(cfg.limit.is_none());
    }

    #[test]
    fn oracle_fault_not_counted_in_pass_rate() {
        let docs = vec![make_match(0.90), make_oracle_fault()];
        let s = build_summary(&docs);
        assert_eq!(s.oracle_faults, 1);
        assert_eq!(s.render_pass, 1);
        assert!((s.effective_render_pass_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn regression_counted_correctly() {
        let docs = vec![make_match(0.90), make_regression(0.50)];
        let s = build_summary(&docs);
        assert_eq!(s.render_pass, 1);
        assert_eq!(s.render_fail, 1);
        assert!((s.effective_render_pass_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn load_baseline_pass_rate_missing_file() {
        let r = load_baseline_pass_rate(Some(Path::new("/nonexistent.json")));
        assert!(r.is_none());
    }

    #[test]
    fn load_baseline_pass_rate_valid_json() {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), r#"{"pass_rate": 0.8173}"#).unwrap();
        let r = load_baseline_pass_rate(Some(f.path()));
        assert!((r.unwrap() - 0.8173).abs() < 1e-6);
    }

    #[test]
    fn load_baseline_pass_rate_none_path() {
        assert!(load_baseline_pass_rate(None).is_none());
    }
}
