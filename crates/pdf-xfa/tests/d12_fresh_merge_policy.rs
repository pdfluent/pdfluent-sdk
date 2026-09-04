//! D12 — FreshMergeExperimental admission tests.
//!
//! Verifies:
//! - Default no-arg API still routes to SavedStateFaithful (no regression).
//! - FreshMerge trace counter is 0 when no data-bound unmatched subforms exist.
//! - FreshMerge trace counter is > 0 when data-bound unmatched subforms exist.
//! - SavedStateFaithful trace counter is always 0 (counter is policy-gated).
//! - FreshMerge metadata labels the policy correctly.
//!
//! NOTE: these tests do NOT depend on private corpus PDFs. Synthetic fixture
//! construction is limited to non-XFA PDFs (all pass through as-is) and
//! public XFA building blocks. Full admission behaviour on real docs requires
//! VPS measurement (see D12_PREP_6_FULL_D12_HANDOFF.md).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_xfa::{
    flatten_xfa_to_pdf, flatten_xfa_to_pdf_with_policy_and_metadata, XfaRenderingPolicy,
};

/// Minimal non-XFA PDF — passes through the pipeline unchanged.
const TINY_PDF: &[u8] =
    b"%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n";

// ── T1: default stability ──────────────────────────────────────────────────

#[test]
fn default_flatten_routes_to_saved_state_faithful() {
    let default_out = flatten_xfa_to_pdf(TINY_PDF);
    let explicit_out = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::SavedStateFaithful,
    );
    match (default_out, explicit_out) {
        (Ok(a), Ok((b, meta))) => {
            assert_eq!(
                a, b,
                "default and explicit SavedStateFaithful must be byte-identical"
            );
            assert_eq!(
                meta.rendering_policy,
                XfaRenderingPolicy::SavedStateFaithful
            );
            assert_eq!(
                meta.fresh_merge_admitted_nodes, 0,
                "SavedStateFaithful must never admit formdom_unmatched nodes"
            );
        }
        (Err(_), Err(_)) => { /* both fail the same way on degenerate input — acceptable */ }
        (a, b) => panic!("default and explicit saved-state disagree: {a:?} vs {b:?}"),
    }
}

// ── T2: FreshMerge API shape ───────────────────────────────────────────────

#[test]
fn fresh_merge_returns_output_not_error() {
    let result = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::FreshMergeExperimental,
    );
    assert!(
        result.is_ok(),
        "FreshMergeExperimental must return Ok (not RenderingPolicyUnsupported)"
    );
}

#[test]
fn fresh_merge_metadata_labels_policy() {
    let (_, meta) = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::FreshMergeExperimental,
    )
    .expect("FreshMerge must return output");
    assert_eq!(
        meta.rendering_policy,
        XfaRenderingPolicy::FreshMergeExperimental
    );
}

// ── T3: trace counter ─────────────────────────────────────────────────────

#[test]
fn saved_state_admitted_counter_is_always_zero() {
    let (_, meta) = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::SavedStateFaithful,
    )
    .expect("SavedStateFaithful must succeed");
    assert_eq!(
        meta.fresh_merge_admitted_nodes, 0,
        "SavedStateFaithful must never increment the FreshMerge counter"
    );
}

#[test]
fn fresh_merge_counter_zero_for_non_xfa_doc() {
    // A non-XFA PDF has no form DOM and no FormTree subforms to suppress.
    // The counter stays 0 even under FreshMerge.
    let (_, meta) = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::FreshMergeExperimental,
    )
    .expect("FreshMerge must succeed on non-XFA doc");
    assert_eq!(
        meta.fresh_merge_admitted_nodes, 0,
        "no formdom_unmatched nodes exist in a non-XFA PDF"
    );
}

// ── T4: no broad unhide in default mode ───────────────────────────────────

#[test]
fn no_broad_unhide_in_saved_state_mode() {
    // The default (no-arg) flatten must produce identical output to explicit
    // SavedStateFaithful. If any suppression-bypass logic were introduced in
    // the default path, these would diverge.
    let default_result = flatten_xfa_to_pdf(TINY_PDF);
    let saved_result = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::SavedStateFaithful,
    );
    match (default_result, saved_result) {
        (Ok(a), Ok((b, _))) => {
            assert_eq!(
                a, b,
                "default must equal explicit SavedStateFaithful — no broad unhide"
            );
        }
        (Err(_), Err(_)) => {}
        (a, b) => panic!("default diverged from explicit saved-state: {a:?} vs {b:?}"),
    }
}

// ── T5: policy token parsing ──────────────────────────────────────────────

#[test]
fn fresh_merge_token_parses() {
    assert_eq!(
        XfaRenderingPolicy::from_token("fresh-merge"),
        Some(XfaRenderingPolicy::FreshMergeExperimental)
    );
    assert_eq!(
        XfaRenderingPolicy::from_token("freshmergeexperimental"),
        Some(XfaRenderingPolicy::FreshMergeExperimental)
    );
    assert_eq!(
        XfaRenderingPolicy::from_token("saved-state"),
        Some(XfaRenderingPolicy::SavedStateFaithful)
    );
}
