//! D14 — rendering-policy guard tests.
//!
//! These pin the invariants that keep `FreshMergeExperimental` from drifting into
//! the default/production position. They test the public `pdf-xfa` policy API only
//! (no corpus, no rendering output).

use pdf_xfa::XfaRenderingPolicy;

/// Guard 1: the default rendering policy is `SavedStateFaithful`.
#[test]
fn default_policy_is_saved_state_faithful() {
    assert_eq!(
        XfaRenderingPolicy::default(),
        XfaRenderingPolicy::SavedStateFaithful,
        "the default XFA rendering policy must be SavedStateFaithful"
    );
}

/// Guard 2: `FreshMergeExperimental` must be selected by an explicit token —
/// it is never produced by the default or by the saved-state spellings.
#[test]
fn fresh_merge_requires_explicit_selection() {
    // Saved-state spellings (and case variants) never yield FreshMerge.
    for tok in [
        "saved-state",
        "saved_state",
        "saved_state_faithful",
        "SavedStateFaithful",
        "SAVED-STATE",
    ] {
        assert_eq!(
            XfaRenderingPolicy::from_token(tok),
            Some(XfaRenderingPolicy::SavedStateFaithful),
            "token {tok:?} must map to SavedStateFaithful"
        );
    }
    // FreshMerge is reachable ONLY via explicit fresh-merge spellings.
    for tok in [
        "fresh-merge",
        "fresh_merge",
        "fresh_merge_experimental",
        "FreshMergeExperimental",
    ] {
        assert_eq!(
            XfaRenderingPolicy::from_token(tok),
            Some(XfaRenderingPolicy::FreshMergeExperimental),
            "token {tok:?} must map to FreshMergeExperimental"
        );
    }
}

/// Guard 3: an unrelated/empty token never silently resolves to a policy
/// (so a typo can never accidentally select FreshMerge or a default).
#[test]
fn unknown_token_is_none() {
    for tok in [
        "",
        "fresh",
        "merge",
        "default",
        "production",
        "adobe",
        "xyz",
    ] {
        assert_eq!(
            XfaRenderingPolicy::from_token(tok),
            None,
            "token {tok:?} must not resolve to any policy"
        );
    }
}

/// Guard 4: the stable identifiers are distinct and label the policy clearly.
#[test]
fn as_str_identifiers_are_stable_and_distinct() {
    assert_eq!(
        XfaRenderingPolicy::SavedStateFaithful.as_str(),
        "saved_state_faithful"
    );
    assert_eq!(
        XfaRenderingPolicy::FreshMergeExperimental.as_str(),
        "fresh_merge_experimental"
    );
    assert_ne!(
        XfaRenderingPolicy::SavedStateFaithful.as_str(),
        XfaRenderingPolicy::FreshMergeExperimental.as_str()
    );
    // The experimental policy's identifier carries the experimental marker.
    assert!(XfaRenderingPolicy::FreshMergeExperimental
        .as_str()
        .contains("experimental"));
}
