//! D11/D12 — explicit XFA rendering policy API tests.
//!
//! Verifies the policy surface without changing default behaviour:
//! - default policy is `SavedStateFaithful`;
//! - explicit `SavedStateFaithful` produces byte-identical output to the no-arg
//!   entrypoint, and metadata records the policy;
//! - `FreshMergeExperimental` is plumbed (D12 draft) and returns output;
//! - token parsing round-trips.

// XfaError kept in case future tests re-add error-path assertions
#[allow(unused_imports)]
use pdf_xfa::error::XfaError;
use pdf_xfa::{
    flatten_xfa_to_pdf, flatten_xfa_to_pdf_with_policy,
    flatten_xfa_to_pdf_with_policy_and_metadata, XfaRenderingPolicy,
};

/// Minimal non-XFA PDF; flatten returns the input unchanged for both the no-arg
/// and the explicit-saved-state paths, so we can assert byte-identity without a
/// private corpus fixture.
const TINY_PDF: &[u8] =
    b"%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n";

#[test]
fn default_policy_is_saved_state_faithful() {
    assert_eq!(
        XfaRenderingPolicy::default(),
        XfaRenderingPolicy::SavedStateFaithful
    );
    // Both policies are implemented as of D12 draft.
    assert!(XfaRenderingPolicy::SavedStateFaithful.is_supported());
    assert!(XfaRenderingPolicy::FreshMergeExperimental.is_supported());
}

#[test]
fn explicit_saved_state_matches_default_output() {
    let default_out = flatten_xfa_to_pdf(TINY_PDF);
    let policy_out =
        flatten_xfa_to_pdf_with_policy(TINY_PDF, XfaRenderingPolicy::SavedStateFaithful);
    // Both paths must agree (Ok==Ok bytes, or Err==Err kind). For a non-XFA PDF
    // they return identical results; the point is the policy path adds no
    // behaviour at the default.
    match (default_out, policy_out) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "explicit saved-state must match default bytes"),
        (Err(_), Err(_)) => {}
        _ => panic!("default and explicit saved-state diverged"),
    }
}

#[test]
fn metadata_records_selected_policy() {
    // Whatever the flatten result, when it succeeds the metadata must label the
    // policy as saved_state_faithful (the only supported one).
    if let Ok((_, meta)) = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::SavedStateFaithful,
    ) {
        assert_eq!(
            meta.rendering_policy,
            XfaRenderingPolicy::SavedStateFaithful
        );
        assert_eq!(meta.rendering_policy.as_str(), "saved_state_faithful");
    }
}

#[test]
fn fresh_merge_is_plumbed_and_returns_output() {
    // D12: FreshMergeExperimental is now plumbed through the pipeline.
    // It returns output (byte-identical to SavedStateFaithful in the
    // plumbing-only phase) rather than RenderingPolicyUnsupported.
    let (_, meta) = flatten_xfa_to_pdf_with_policy_and_metadata(
        TINY_PDF,
        XfaRenderingPolicy::FreshMergeExperimental,
    )
    .expect("FreshMergeExperimental must return output after D12 plumbing");
    assert_eq!(
        meta.rendering_policy,
        XfaRenderingPolicy::FreshMergeExperimental,
        "metadata must label the FreshMerge policy"
    );
}

#[test]
fn policy_token_parsing_round_trips() {
    assert_eq!(
        XfaRenderingPolicy::from_token("saved-state"),
        Some(XfaRenderingPolicy::SavedStateFaithful)
    );
    assert_eq!(
        XfaRenderingPolicy::from_token("fresh-merge"),
        Some(XfaRenderingPolicy::FreshMergeExperimental)
    );
    assert_eq!(
        XfaRenderingPolicy::from_token("saved_state_faithful"),
        Some(XfaRenderingPolicy::SavedStateFaithful)
    );
    assert_eq!(XfaRenderingPolicy::from_token("nonsense"), None);
    // as_str <-> from_token round-trip.
    for p in [
        XfaRenderingPolicy::SavedStateFaithful,
        XfaRenderingPolicy::FreshMergeExperimental,
    ] {
        assert_eq!(XfaRenderingPolicy::from_token(p.as_str()), Some(p));
    }
}
