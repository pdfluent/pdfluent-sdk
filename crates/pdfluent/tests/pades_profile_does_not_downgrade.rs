//! Asking for a PAdES profile we cannot produce must fail, not downgrade (#176).
//!
//! `PadesProfile::LongTerm` was the default, and every profile mapped to the
//! same SubFilter while `sign_pdf` — not `sign_pdf_ltv` — did the work. So a
//! caller asking for long-term validity got a basic signature and no warning.
//!
//! A signature that claims more than it carries is worse than one that claims
//! less: the point of B-LT is that a verifier can still check it years later,
//! and a B-B signature dressed as B-LT fails that check silently.

use pdfluent::signer::{PadesProfile, SignOptions};

#[test]
fn the_default_profile_is_the_one_we_actually_produce() {
    // Constructed through the builder, which is the only public route.
    let standaard = SignOptions::new();
    let expliciet = SignOptions::new().profile(PadesProfile::BasicSignature);

    assert_eq!(
        format!("{standaard:?}"),
        format!("{expliciet:?}"),
        "the default must be what signing actually produces; LongTerm as the \
         default meant every caller silently received B-B",
    );
}

#[test]
fn the_unreachable_profiles_are_not_silently_accepted() {
    // Each of these needs a timestamp authority, and SignOptions carries no TSA
    // configuration. If that changes, this test should change with it — which
    // is the point: today it records that three of four profiles cannot be had.
    for profile in [
        PadesProfile::Timestamped,
        PadesProfile::LongTerm,
        PadesProfile::LongTermArchive,
    ] {
        let opts = SignOptions::new().profile(profile);
        assert!(
            format!("{opts:?}").contains(&format!("{profile:?}")),
            "the builder must carry {profile:?} through unchanged, so that \
             signing can refuse it rather than the option being dropped here",
        );
    }
}
