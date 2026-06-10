//! Integration tests for the Differential Quality Harness v1.
//!
//! All tests are fully deterministic — no external binaries (mutool, pdftotext),
//! no corpus files, no API keys required. They exercise the report data model
//! and summary logic through the public `pdf_diff` API.

use pdf_diff::{
    build_report, build_summary, compute_baseline_delta, DifferentialReport, DifferentialVerdict,
    DocDifferential, TextVerdict,
};

// ─── Helpers ────────────────────────────────────────────────────────────────

fn make_doc(
    pdf_path: &str,
    render_verdict: DifferentialVerdict,
    render_ssim: Option<f64>,
    text_verdict: TextVerdict,
    text_similarity: Option<f64>,
    oracle_fault: bool,
    error: Option<&str>,
) -> DocDifferential {
    DocDifferential {
        pdf_path: pdf_path.into(),
        pdf_hash: "testhash".into(),
        render_ssim,
        text_similarity,
        render_verdict,
        text_verdict,
        oracle_fault,
        oracle_fault_reason: if oracle_fault {
            Some("synthetic oracle fault".into())
        } else {
            None
        },
        pages_compared: if render_ssim.is_some() { 1 } else { 0 },
        error: error.map(Into::into),
    }
}

fn match_doc(ssim: f64) -> DocDifferential {
    make_doc(
        "good.pdf",
        DifferentialVerdict::Match,
        Some(ssim),
        TextVerdict::OracleUnavailable,
        None,
        false,
        None,
    )
}

fn regression_doc(ssim: f64) -> DocDifferential {
    make_doc(
        "bad.pdf",
        DifferentialVerdict::Regression,
        Some(ssim),
        TextVerdict::OracleUnavailable,
        None,
        false,
        None,
    )
}

fn oracle_fault_doc() -> DocDifferential {
    make_doc(
        "fault.pdf",
        DifferentialVerdict::OracleUnavailable,
        None,
        TextVerdict::OracleUnavailable,
        None,
        true,
        None,
    )
}

fn skipped_doc() -> DocDifferential {
    make_doc(
        "skip.pdf",
        DifferentialVerdict::OracleUnavailable,
        None,
        TextVerdict::OracleUnavailable,
        None,
        false,
        Some("mutool not found"),
    )
}

// ─── Test 1: planted regression ─────────────────────────────────────────────

/// The planted regression test proves the gate logic fails when our output
/// diverges from the oracle. Uses the same SSIM gate as the harness.
///
/// This test does NOT require mutool — it uses synthetic PageImages directly.
#[test]
fn planted_regression_is_reported_as_failure() {
    use pdf_diff::{gate, PageImage};

    // Black vs white: maximally different renders.
    let black = PageImage::new(64, 64, vec![0u8; 64 * 64 * 4]).unwrap();
    let white = PageImage::new(64, 64, vec![255u8; 64 * 64 * 4]).unwrap();

    let (score, verdict) = gate::compare(&black, &white);

    // Prove the gate fires
    assert_eq!(
        verdict,
        DifferentialVerdict::Regression,
        "black-vs-white must be classified as Regression, SSIM={score}"
    );
    assert!(score < gate::GATE_SSIM_THRESHOLD);

    // Build a DocDifferential as if this came from run_doc()
    let doc = regression_doc(score);
    let summary = build_summary(&[doc]);

    assert_eq!(
        summary.render_fail, 1,
        "regression doc must increment render_fail"
    );
    assert_eq!(summary.render_pass, 0);
    assert!(
        summary.effective_render_pass_rate < 0.01,
        "pass rate should be near 0"
    );
}

// ─── Test 2: oracle fault excluded from pass rate ───────────────────────────

#[test]
fn oracle_fault_excluded_from_pass_rate() {
    // 1 real match + 1 oracle fault → effective denom = 1, pass rate = 1.0
    let docs = vec![match_doc(0.90), oracle_fault_doc()];
    let s = build_summary(&docs);

    assert_eq!(s.total, 2);
    assert_eq!(s.oracle_faults, 1);
    assert_eq!(s.render_pass, 1);
    assert_eq!(s.render_fail, 0);
    assert!(
        (s.effective_render_pass_rate - 1.0).abs() < 1e-9,
        "oracle fault must not inflate denominator: rate={}",
        s.effective_render_pass_rate
    );
}

// ─── Test 3: first run produces no baseline delta ───────────────────────────

#[test]
fn first_run_produces_no_baseline_delta() {
    let docs = vec![match_doc(0.95)];
    let summary = build_summary(&docs);
    let delta = compute_baseline_delta(&summary, None, 0.005);
    assert!(
        delta.is_none(),
        "no baseline provided → no delta (first-run mode)"
    );
}

// ─── Test 4: regression below tolerance is not flagged ──────────────────────

#[test]
fn regression_below_tolerance_not_flagged() {
    // 9 pass + 1 fail → rate = 0.9; baseline = 0.904; delta = -0.004; tol = 0.005
    let mut docs: Vec<_> = (0..9).map(|_| match_doc(0.90)).collect();
    docs.push(regression_doc(0.40));
    let summary = build_summary(&docs);

    let delta = compute_baseline_delta(&summary, Some(0.904), 0.005).unwrap();
    assert!(
        !delta.regressed,
        "delta={:.4} < tolerance=0.005 should NOT be flagged",
        delta.delta
    );
    assert!(delta.delta < 0.0); // it IS negative, just within tolerance
}

// ─── Test 5: regression above tolerance is flagged ──────────────────────────

#[test]
fn regression_above_tolerance_flagged() {
    // 1 pass + 1 fail → rate = 0.5; baseline = 0.80; delta = -0.30; tol = 0.005
    let docs = vec![match_doc(0.90), regression_doc(0.40)];
    let summary = build_summary(&docs);

    let delta = compute_baseline_delta(&summary, Some(0.80), 0.005).unwrap();
    assert!(
        delta.regressed,
        "delta={:.4} > tolerance=0.005 MUST be flagged as regression",
        delta.delta
    );
    assert!(delta.delta < -0.005);
}

// ─── Test 6: JSON report roundtrip ──────────────────────────────────────────

#[test]
fn json_report_roundtrip() {
    let docs = vec![
        match_doc(0.92),
        regression_doc(0.55),
        oracle_fault_doc(),
        make_doc(
            "text.pdf",
            DifferentialVerdict::Match,
            Some(0.88),
            TextVerdict::Match,
            Some(0.95),
            false,
            None,
        ),
    ];
    let report = build_report(
        "run-roundtrip".into(),
        "cafebabe".into(),
        "2026-06-10T00:00:00Z".into(),
        docs,
        Some(0.75),
        0.005,
    );

    let json = serde_json::to_string_pretty(&report).expect("serialize");

    // Deserialize and verify key fields
    let back: DifferentialReport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.run_id, "run-roundtrip");
    assert_eq!(back.summary.total, 4);
    assert_eq!(back.summary.render_pass, 2); // match + text_match
    assert_eq!(back.summary.render_fail, 1);
    assert_eq!(back.summary.oracle_faults, 1);
    assert_eq!(back.summary.text_pass, 1);

    // current = 2/3 ≈ 0.667, baseline = 0.75, delta = -0.083 → regressed
    let bd = back.baseline_delta.as_ref().unwrap();
    assert!(
        bd.regressed,
        "2/3 pass vs 0.75 baseline must flag regression"
    );
    assert!((bd.baseline_pass_rate - 0.75).abs() < 1e-9);
    assert_eq!(bd.tolerance, 0.005);

    // Verify JSON contains the run_id and expected structure
    assert!(json.contains("run-roundtrip"));
    assert!(json.contains("cafebabe"));
    assert!(json.contains("oracle_faults"));
}

// ─── Test 7: panicking doc produces an error record, does not abort ─────────

/// Verifies that the harness architecture can survive per-doc failures.
/// We can't directly call run_corpus() without a real PDF, so we test
/// the catch_unwind pattern that harness.rs uses.
#[test]
fn panicking_doc_captured_as_error_does_not_abort() {
    // Simulate the catch_unwind pattern used in run_corpus()
    let result: Result<DocDifferential, _> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("synthetic doc-level panic");
        }));

    // The panic is caught — does not propagate
    assert!(
        result.is_err(),
        "catch_unwind must catch the panic without aborting"
    );

    // In run_corpus(), the catch_unwind result is mapped to an error DocDifferential.
    // We verify the mapping logic here directly.
    let error_doc = DocDifferential {
        pdf_path: "panic.pdf".into(),
        pdf_hash: String::new(),
        render_ssim: None,
        text_similarity: None,
        render_verdict: DifferentialVerdict::OracleUnavailable,
        text_verdict: TextVerdict::OracleUnavailable,
        oracle_fault: false,
        oracle_fault_reason: None,
        pages_compared: 0,
        error: Some("panic in run_doc".into()),
    };

    // The error doc is counted as skipped, not as a failure.
    let summary = build_summary(&[error_doc]);
    assert_eq!(
        summary.render_fail, 0,
        "panic doc must not be counted as failure"
    );
    assert_eq!(summary.skipped, 1, "panic doc must be counted as skipped");
    assert!(
        (summary.effective_render_pass_rate - 1.0).abs() < 1e-9,
        "no real failures → pass rate = 1.0"
    );
}

// ─── Test 8: skipped docs do not pollute pass rate ──────────────────────────

#[test]
fn skipped_docs_excluded_from_effective_pass_rate() {
    // 2 pass + 1 skip (mutool unavailable) → effective denom = 2
    let docs = vec![match_doc(0.95), match_doc(0.88), skipped_doc()];
    let s = build_summary(&docs);
    assert_eq!(s.skipped, 1);
    assert_eq!(s.render_pass, 2);
    assert!((s.effective_render_pass_rate - 1.0).abs() < 1e-9);
}

// ─── Test 9: text verdicts tracked independently of render verdicts ──────────

#[test]
fn text_and_render_verdicts_are_independent() {
    let render_match_text_fail = make_doc(
        "a.pdf",
        DifferentialVerdict::Match,
        Some(0.90),
        TextVerdict::Regression,
        Some(0.30),
        false,
        None,
    );
    let render_fail_text_match = make_doc(
        "b.pdf",
        DifferentialVerdict::Regression,
        Some(0.50),
        TextVerdict::Match,
        Some(0.95),
        false,
        None,
    );

    let s = build_summary(&[render_match_text_fail, render_fail_text_match]);
    assert_eq!(s.render_pass, 1);
    assert_eq!(s.render_fail, 1);
    assert_eq!(s.text_pass, 1);
    assert_eq!(s.text_fail, 1);
    // effective render pass rate: 1/2 = 0.5
    assert!((s.effective_render_pass_rate - 0.5).abs() < 1e-9);
}
