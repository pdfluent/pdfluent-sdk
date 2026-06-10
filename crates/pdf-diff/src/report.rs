//! Structured report types for the Differential Quality Harness.
//!
//! A [`DifferentialReport`] is the top-level artifact produced by a corpus run:
//! per-document render SSIM + text similarity scores, oracle-fault bookkeeping,
//! an aggregate [`HarnessSummary`], and an optional [`BaselineDelta`] comparing
//! this run against a frozen baseline.

use crate::gate::DifferentialVerdict;

/// Verdict for the text-extraction oracle comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextVerdict {
    /// Text similarity ≥ threshold: our extraction matches the reference.
    Match,
    /// Text similarity < threshold: extraction diverges (a regression).
    Regression,
    /// The text oracle (pdftotext) was unavailable or produced no usable output.
    /// Treated as a skip — never a pass or a fail.
    OracleUnavailable,
}

/// Per-document differential result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocDifferential {
    /// Relative or absolute path to the PDF.
    pub pdf_path: String,
    /// SHA-256 hex digest of the raw PDF bytes.
    pub pdf_hash: String,
    /// Minimum SSIM across compared pages (None = oracle unavailable or engine failed).
    pub render_ssim: Option<f64>,
    /// Text similarity score from the oracle comparison (None = disabled or unavailable).
    pub text_similarity: Option<f64>,
    /// Gate verdict for the render oracle.
    pub render_verdict: DifferentialVerdict,
    /// Gate verdict for the text oracle.
    pub text_verdict: TextVerdict,
    /// True when the oracle (mutool/pdftotext) produced clearly invalid output.
    /// Oracle-fault documents are excluded from the effective pass-rate denominator.
    pub oracle_fault: bool,
    /// Human-readable reason for the oracle fault classification.
    pub oracle_fault_reason: Option<String>,
    /// Number of pages compared (0 when engine or oracle failed before any comparison).
    pub pages_compared: usize,
    /// Engine open / render error, if any.
    pub error: Option<String>,
}

/// Aggregate statistics across all documents in a corpus run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HarnessSummary {
    /// Total documents examined.
    pub total: usize,
    /// Documents where render oracle agreed (SSIM ≥ threshold).
    pub render_pass: usize,
    /// Documents where render oracle disagreed (SSIM < threshold, oracle available).
    pub render_fail: usize,
    /// Documents where text oracle agreed (similarity ≥ threshold).
    pub text_pass: usize,
    /// Documents where text oracle disagreed.
    pub text_fail: usize,
    /// Documents with confirmed oracle faults (excluded from effective denominator).
    pub oracle_faults: usize,
    /// Documents skipped due to engine failure, encryption, or both oracles absent.
    pub skipped: usize,
    /// Mean SSIM across documents where a render comparison was possible.
    pub mean_render_ssim: Option<f64>,
    /// Mean text similarity across documents where a text comparison was possible.
    pub mean_text_similarity: Option<f64>,
    /// Effective render pass rate: `render_pass / (total - oracle_faults - skipped)`.
    /// Returns 1.0 when the effective denominator is zero.
    pub effective_render_pass_rate: f64,
}

/// Comparison of this run's effective pass rate against a previously frozen baseline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineDelta {
    /// The frozen pass rate loaded from the baseline file.
    pub baseline_pass_rate: f64,
    /// The effective pass rate from this run.
    pub current_pass_rate: f64,
    /// `current_pass_rate - baseline_pass_rate` (positive = improvement).
    pub delta: f64,
    /// True when `delta < -tolerance` (this run is worse than the baseline).
    pub regressed: bool,
    /// The tolerance value used for the regression check.
    pub tolerance: f64,
}

/// Top-level artifact produced by a corpus differential run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DifferentialReport {
    /// Unique identifier for this run (e.g. `"run-20260610-143022"`).
    pub run_id: String,
    /// `git rev-parse HEAD` at the time of the run.
    pub git_commit: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Per-document results.
    pub docs: Vec<DocDifferential>,
    /// Aggregate statistics.
    pub summary: HarnessSummary,
    /// Comparison against a baseline, if one was provided.
    pub baseline_delta: Option<BaselineDelta>,
}

// ─── Helper functions ───────────────────────────────────────────────────────

/// Compute aggregate statistics from a slice of per-document results.
pub fn build_summary(docs: &[DocDifferential]) -> HarnessSummary {
    let total = docs.len();
    let mut render_pass = 0usize;
    let mut render_fail = 0usize;
    let mut text_pass = 0usize;
    let mut text_fail = 0usize;
    let mut oracle_faults = 0usize;
    let mut skipped = 0usize;
    let mut ssim_sum = 0.0f64;
    let mut ssim_count = 0usize;
    let mut text_sum = 0.0f64;
    let mut text_count = 0usize;

    for doc in docs {
        if doc.oracle_fault {
            oracle_faults += 1;
            continue;
        }
        match doc.render_verdict {
            DifferentialVerdict::Match => render_pass += 1,
            DifferentialVerdict::Regression => render_fail += 1,
            DifferentialVerdict::OracleUnavailable => skipped += 1,
        }
        if let Some(s) = doc.render_ssim {
            ssim_sum += s;
            ssim_count += 1;
        }
        match doc.text_verdict {
            TextVerdict::Match => text_pass += 1,
            TextVerdict::Regression => text_fail += 1,
            TextVerdict::OracleUnavailable => {}
        }
        if let Some(s) = doc.text_similarity {
            text_sum += s;
            text_count += 1;
        }
    }

    let effective_denom = render_pass + render_fail; // oracle_faults and skipped already excluded
    let effective_render_pass_rate = if effective_denom > 0 {
        render_pass as f64 / effective_denom as f64
    } else {
        1.0
    };

    HarnessSummary {
        total,
        render_pass,
        render_fail,
        text_pass,
        text_fail,
        oracle_faults,
        skipped,
        mean_render_ssim: if ssim_count > 0 {
            Some(ssim_sum / ssim_count as f64)
        } else {
            None
        },
        mean_text_similarity: if text_count > 0 {
            Some(text_sum / text_count as f64)
        } else {
            None
        },
        effective_render_pass_rate,
    }
}

/// Compare the summary's effective pass rate against a baseline pass rate.
///
/// Returns `None` when `baseline_pass_rate` is `None` (first-run mode).
pub fn compute_baseline_delta(
    summary: &HarnessSummary,
    baseline_pass_rate: Option<f64>,
    tolerance: f64,
) -> Option<BaselineDelta> {
    let baseline = baseline_pass_rate?;
    let current = summary.effective_render_pass_rate;
    let delta = current - baseline;
    Some(BaselineDelta {
        baseline_pass_rate: baseline,
        current_pass_rate: current,
        delta,
        regressed: delta < -tolerance,
        tolerance,
    })
}

/// Assemble a complete [`DifferentialReport`] from its parts.
pub fn build_report(
    run_id: String,
    git_commit: String,
    timestamp: String,
    docs: Vec<DocDifferential>,
    baseline_pass_rate: Option<f64>,
    baseline_tolerance: f64,
) -> DifferentialReport {
    let summary = build_summary(&docs);
    let baseline_delta = compute_baseline_delta(&summary, baseline_pass_rate, baseline_tolerance);
    DifferentialReport {
        run_id,
        git_commit,
        timestamp,
        docs,
        summary,
        baseline_delta,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn match_doc(ssim: f64, text_sim: Option<f64>) -> DocDifferential {
        DocDifferential {
            pdf_path: "a.pdf".into(),
            pdf_hash: "abc".into(),
            render_ssim: Some(ssim),
            text_similarity: text_sim,
            render_verdict: DifferentialVerdict::Match,
            text_verdict: if text_sim.is_some() {
                TextVerdict::Match
            } else {
                TextVerdict::OracleUnavailable
            },
            oracle_fault: false,
            oracle_fault_reason: None,
            pages_compared: 1,
            error: None,
        }
    }

    fn fail_doc(ssim: f64) -> DocDifferential {
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

    fn fault_doc() -> DocDifferential {
        DocDifferential {
            pdf_path: "c.pdf".into(),
            pdf_hash: "ghi".into(),
            render_ssim: None,
            text_similarity: None,
            render_verdict: DifferentialVerdict::OracleUnavailable,
            text_verdict: TextVerdict::OracleUnavailable,
            oracle_fault: true,
            oracle_fault_reason: Some("mutool output < 100 bytes".into()),
            pages_compared: 0,
            error: None,
        }
    }

    fn skip_doc() -> DocDifferential {
        DocDifferential {
            pdf_path: "d.pdf".into(),
            pdf_hash: "jkl".into(),
            render_ssim: None,
            text_similarity: None,
            render_verdict: DifferentialVerdict::OracleUnavailable,
            text_verdict: TextVerdict::OracleUnavailable,
            oracle_fault: false,
            oracle_fault_reason: None,
            pages_compared: 0,
            error: Some("mutool not found".into()),
        }
    }

    #[test]
    fn summary_counts_correct() {
        let docs = vec![match_doc(0.95, None), fail_doc(0.50), fault_doc()];
        let s = build_summary(&docs);
        assert_eq!(s.total, 3);
        assert_eq!(s.render_pass, 1);
        assert_eq!(s.render_fail, 1);
        assert_eq!(s.oracle_faults, 1);
        assert_eq!(s.skipped, 0);
    }

    #[test]
    fn oracle_fault_excluded_from_effective_denominator() {
        // 1 match + 1 oracle_fault → effective denom = 1, pass rate = 1.0
        let docs = vec![match_doc(0.90, None), fault_doc()];
        let s = build_summary(&docs);
        assert_eq!(s.oracle_faults, 1);
        assert_eq!(s.render_pass, 1);
        assert!(
            (s.effective_render_pass_rate - 1.0).abs() < 1e-9,
            "pass rate should be 1.0, got {}",
            s.effective_render_pass_rate
        );
    }

    #[test]
    fn skip_counted_separately_from_oracle_fault() {
        let docs = vec![match_doc(0.90, None), fault_doc(), skip_doc()];
        let s = build_summary(&docs);
        assert_eq!(s.skipped, 1);
        assert_eq!(s.oracle_faults, 1);
        // skip is excluded from effective denom too (not pass, not fail)
        assert!((s.effective_render_pass_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mean_ssim_computed_over_compared_docs_only() {
        let docs = vec![match_doc(0.80, None), fail_doc(0.40), fault_doc()];
        let s = build_summary(&docs);
        // mean of 0.80 and 0.40 = 0.60
        let mean = s.mean_render_ssim.unwrap();
        assert!((mean - 0.60).abs() < 1e-9, "expected 0.60 got {mean}");
    }

    #[test]
    fn mean_text_similarity_computed_correctly() {
        let docs = vec![match_doc(0.90, Some(0.80)), match_doc(0.95, Some(1.00))];
        let s = build_summary(&docs);
        let mean = s.mean_text_similarity.unwrap();
        assert!((mean - 0.90).abs() < 1e-9, "expected 0.90 got {mean}");
    }

    #[test]
    fn effective_pass_rate_all_pass() {
        let docs = vec![match_doc(0.95, None), match_doc(0.98, None)];
        let s = build_summary(&docs);
        assert!((s.effective_render_pass_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn effective_pass_rate_all_fail() {
        let docs = vec![fail_doc(0.40), fail_doc(0.30)];
        let s = build_summary(&docs);
        assert!(s.effective_render_pass_rate.abs() < 1e-9);
    }

    #[test]
    fn empty_corpus_produces_zero_counts() {
        let s = build_summary(&[]);
        assert_eq!(s.total, 0);
        assert_eq!(s.render_pass, 0);
        assert_eq!(s.render_fail, 0);
        assert_eq!(s.oracle_faults, 0);
        // effective denom 0 → pass rate 1.0 (no regression possible)
        assert!((s.effective_render_pass_rate - 1.0).abs() < 1e-9);
        assert!(s.mean_render_ssim.is_none());
    }

    #[test]
    fn baseline_delta_below_tolerance_not_regressed() {
        let docs = vec![match_doc(0.95, None), match_doc(0.98, None)];
        let s = build_summary(&docs);
        // current = 1.0, baseline = 0.997, delta = +0.003, tolerance = 0.005
        let delta = compute_baseline_delta(&s, Some(0.997), 0.005).unwrap();
        assert!(
            !delta.regressed,
            "small improvement should not be regressed"
        );
    }

    #[test]
    fn baseline_delta_above_tolerance_regressed() {
        let docs = vec![match_doc(0.95, None), fail_doc(0.40)];
        let s = build_summary(&docs);
        // current = 0.50, baseline = 0.80, delta = -0.30, tolerance = 0.005
        let delta = compute_baseline_delta(&s, Some(0.80), 0.005).unwrap();
        assert!(
            delta.regressed,
            "large drop should be flagged as regression"
        );
        assert!(delta.delta < 0.0);
    }

    #[test]
    fn first_run_without_baseline_produces_no_delta() {
        let docs = vec![match_doc(0.95, None)];
        let s = build_summary(&docs);
        let delta = compute_baseline_delta(&s, None, 0.005);
        assert!(delta.is_none(), "no baseline → no delta");
    }

    #[test]
    fn build_report_assembles_correctly() {
        let docs = vec![match_doc(0.90, None), fail_doc(0.60)];
        let report = build_report(
            "run-test".into(),
            "abc123".into(),
            "2026-06-10T00:00:00Z".into(),
            docs,
            Some(0.90),
            0.005,
        );
        assert_eq!(report.run_id, "run-test");
        assert_eq!(report.summary.render_pass, 1);
        assert_eq!(report.summary.render_fail, 1);
        assert!(report.baseline_delta.is_some());
    }

    #[test]
    fn json_roundtrip() {
        let docs = vec![match_doc(0.95, Some(0.88)), fault_doc()];
        let report = build_report(
            "run-rt".into(),
            "deadbeef".into(),
            "2026-06-10T12:00:00Z".into(),
            docs,
            Some(0.80),
            0.005,
        );
        let json = serde_json::to_string(&report).expect("serialize");
        let back: DifferentialReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.run_id, report.run_id);
        assert_eq!(back.summary.total, report.summary.total);
        assert_eq!(back.summary.oracle_faults, report.summary.oracle_faults);
        let bd = back.baseline_delta.unwrap();
        assert!(!bd.regressed);
    }
}
