//! Asking for a PAdES profile we cannot produce must fail, not downgrade (#176).
//!
//! `PadesProfile::LongTerm` was the default, and every profile mapped to the
//! same SubFilter while `sign_pdf` — not `sign_pdf_ltv` — did the work. So a
//! caller asking for long-term validity got a basic signature and no warning.
//!
//! B-T and B-LT are produced for real since #362. This file still guards the
//! default and the builder; what each profile puts in the file is checked in
//! `pades_profiles_produce_what_they_name.rs`.
//!
//! A signature that claims more than it carries is worse than one that claims
//! less: the point of B-LT is that a verifier can still check it years later,
//! and a B-B signature dressed as B-LT fails that check silently.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
fn the_builder_carries_every_profile_through_unchanged() {
    // The builder must not quietly normalise a profile away, because then
    // signing would never see the request and could neither honour nor refuse
    // it. What signing does with each one is checked against the produced bytes
    // in pades_profiles_produce_what_they_name.rs; B-T and B-LT became
    // reachable in #362 once a timestamp token could be supplied, and B-LTA is
    // still refused for want of an archive timestamp.
    for profile in [
        PadesProfile::Timestamped,
        PadesProfile::LongTerm,
        PadesProfile::LongTermArchive,
    ] {
        let opts = SignOptions::new().profile(profile);
        assert!(
            format!("{opts:?}").contains(&format!("{profile:?}")),
            "the builder must carry {profile:?} through unchanged, so that \
             signing can act on it rather than the option being dropped here",
        );
    }
}
