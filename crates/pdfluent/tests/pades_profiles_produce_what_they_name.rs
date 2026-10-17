//! Each PAdES profile must put in the file what its name promises (#362).
//!
//! The profile used to be a label. All four variants mapped to one SubFilter,
//! `sign_pdf` did the work for every one of them, and `sign_pdf_ltv` had no
//! caller anywhere in the workspace. #176 stopped the silent downgrade by
//! refusing the three profiles that were not wired; this checks the other half,
//! that the ones now accepted actually differ in the bytes.
//!
//! The three markers, and why each is the right thing to look for:
//!
//! - **SubFilter** `ETSI.CAdES.detached` is shared by B-B, B-T and B-LT, so it
//!   proves the signature is PAdES at all and separates nothing. Only a
//!   document timestamp uses `ETSI.RFC3161`, and we write none.
//! - **The timestamp attribute OID** `1.2.840.113549.1.9.16.2.14` appears in
//!   the CMS only when a token was embedded. B-B must not carry it. The CMS
//!   lives in `/Contents` as a hex string, so the test decodes it first;
//!   searching the raw file for the OID bytes would find nothing in any
//!   profile and pass for the wrong reason.
//! - **`/DSS`** is the Document Security Store. B-LT must carry it and B-T must
//!   not, because that is the entire difference between the two.
//!
//! Break any dispatch arm in `PdfDocument::sign` and one of these goes red:
//! route B-T through `sign_pdf` and the OID disappears, route B-LT through
//! `sign_pdf_timestamped` and the DSS disappears, accept a missing token and
//! the refusal tests fail.
//!
//! Since #515 the timestamped profiles are signed through
//! `PdfDocument::sign_timestamped`, which fetches the token *after* signing and
//! over the signature. The stand-in DER these tests used to pass in is gone:
//! it stamped nothing, and a marker that a timestamp attribute is present says
//! nothing about whether the timestamp is about this file. The tokens here come
//! from `local_tsa`, which builds a real message imprint over the bytes it is
//! given.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod local_tsa;

use std::path::PathBuf;

use pdfluent::signer::{PadesProfile, Pkcs12Signer, SignOptions};
use pdfluent::PdfDocument;

/// DER body of OID 1.2.840.113549.1.9.16.2.14, id-smime-aa-timeStampToken.
const OID_TIMESTAMP_TOKEN: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x0E,
];

fn repo_file(relative: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    path.exists().then_some(path)
}

/// The signing identity and a document to sign, or `None` off the repository.
///
/// A published crate carries neither, so the tests report the skip on stderr
/// and return rather than failing somebody else's `cargo test`. Inside the
/// repository both are present and every test below runs.
fn signer_and_document() -> Option<(Pkcs12Signer, Vec<u8>)> {
    let p12 = repo_file("../pdf-sign/tests/fixtures/test-rsa.p12")?;
    let pdf = repo_file("tests/fixtures/sample.pdf")?;
    let signer = Pkcs12Signer::from_pfx_file(&p12, "test123").expect("fixture identity loads");
    Some((signer, std::fs::read(pdf).expect("fixture document reads")))
}

macro_rules! fixtures_or_skip {
    () => {
        match signer_and_document() {
            Some(pair) => pair,
            None => {
                eprintln!("skipping: repository fixture tree not present");
                return;
            }
        }
    };
}

fn sign_to_bytes(
    pdf: &[u8],
    signer: &Pkcs12Signer,
    opts: SignOptions,
) -> pdfluent::Result<Vec<u8>> {
    let mut doc = PdfDocument::from_bytes(pdf)?;
    doc.sign(signer, opts)?;
    doc.to_bytes()
}

/// Sign through the two-phase route, with the stand-in authority as the TSA.
fn sign_timestamped_to_bytes(
    pdf: &[u8],
    signer: &Pkcs12Signer,
    opts: SignOptions,
) -> pdfluent::Result<Vec<u8>> {
    let mut doc = PdfDocument::from_bytes(pdf)?;
    doc.sign_timestamped(signer, opts, |signature| Ok(local_tsa::issue(signature)))?;
    doc.to_bytes()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Decode the CMS out of the last `/Contents <HEX>` in a signed file.
///
/// The signature is stored hex-encoded, so an assertion about DER bytes has to
/// decode before it looks. An assertion against the raw file would hold for
/// every profile and prove nothing.
fn signature_cms(pdf: &[u8]) -> Vec<u8> {
    const KEY: &[u8] = b"/Contents";
    let angle = pdf
        .windows(KEY.len())
        .enumerate()
        .rev()
        .find_map(|(i, w)| {
            if w != KEY {
                return None;
            }
            let mut j = i + KEY.len();
            while pdf.get(j).is_some_and(u8::is_ascii_whitespace) {
                j += 1;
            }
            (pdf.get(j) == Some(&b'<')).then_some(j)
        })
        .expect("a signed document has a /Contents hex string");
    let end = pdf[angle + 1..]
        .iter()
        .position(|&b| b == b'>')
        .expect("the hex string is closed")
        + angle
        + 1;

    fn nibble(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(10 + b - b'a'),
            b'A'..=b'F' => Some(10 + b - b'A'),
            _ => None,
        }
    }
    pdf[angle + 1..end]
        .chunks(2)
        .filter_map(|c| Some((nibble(*c.first()?)? << 4) | nibble(*c.get(1)?)?))
        .collect()
}

#[test]
fn basic_signature_carries_no_timestamp_and_no_dss() {
    let (signer, pdf) = fixtures_or_skip!();

    let signed = sign_to_bytes(&pdf, &signer, SignOptions::new()).expect("B-B signs");

    assert!(
        contains(&signed, b"ETSI.CAdES.detached"),
        "a PAdES signature carries the ETSI.CAdES.detached SubFilter",
    );
    assert!(
        !contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "B-B must not carry a timestamp attribute: a basic signature that looks \
         timestamped is the claim this ticket exists to remove",
    );
    assert!(
        !contains(&signed, b"/DSS"),
        "B-B must not carry a DSS; validation material belongs to B-LT",
    );
}

#[test]
fn timestamped_carries_the_timestamp_and_still_no_dss() {
    let (signer, pdf) = fixtures_or_skip!();

    let signed = sign_timestamped_to_bytes(
        &pdf,
        &signer,
        SignOptions::new().profile(PadesProfile::Timestamped),
    )
    .expect("B-T signs when the authority stamps the signature");

    assert!(
        contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "B-T must embed the timestamp token as an unsigned CMS attribute; \
         without it this is a B-B signature under another name",
    );
    assert!(
        !contains(&signed, b"/DSS"),
        "B-T stops at the timestamp. A DSS here would make it B-LT and the \
         profile would again be a label rather than a description",
    );
}

#[test]
fn long_term_carries_both_the_timestamp_and_the_validation_store() {
    let (signer, pdf) = fixtures_or_skip!();

    let signed = sign_timestamped_to_bytes(
        &pdf,
        &signer,
        SignOptions::new()
            .profile(PadesProfile::LongTerm)
            .revocation_data(vec![AN_OCSP_RESPONSE.to_vec()], Vec::new()),
    )
    .expect("B-LT signs when the authority stamps the signature");

    assert!(
        contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "B-LT is B-T plus validation material, so the timestamp must be there too",
    );
    assert!(
        contains(&signed, b"/DSS"),
        "B-LT must append a Document Security Store; that store is the whole \
         reason a verifier can still check the signature years later",
    );
    assert!(
        contains(&signed, b"/Certs"),
        "the DSS must actually hold the signer's chain, not merely exist",
    );
    assert!(
        contains(&signed, b"/OCSPs"),
        "the DSS must hold the revocation evidence too: certificates alone say \
         who signed, not that they were allowed to, which is the half of B-LT a \
         verifier offline in five years actually needs",
    );
}

/// The same refusal on the older route, which also writes the DSS.
///
/// A profile that means one thing on `sign_timestamped` and another on `sign`
/// is worse than either, because the looser entry point is the one a caller
/// finds first.
#[test]
fn long_term_without_revocation_evidence_is_refused_on_the_older_route_too() {
    let (signer, pdf) = fixtures_or_skip!();

    let err = sign_to_bytes(
        &pdf,
        &signer,
        SignOptions::new()
            .profile(PadesProfile::LongTerm)
            .timestamp_token(local_tsa::issue(b"a token the caller already held")),
    )
    .map(|signed| signed.len())
    .expect_err("B-LT without revocation evidence must be refused here as well");

    let message = err.to_string();
    assert!(
        message.contains("B-LT") && message.contains("revocation_data"),
        "the refusal must name the profile and the setter that satisfies it, got: {message}",
    );
}

/// Stand-in revocation evidence: the DSS stores these blobs, it does not read
/// them, and what a real OCSP response says is the responder's business.
const AN_OCSP_RESPONSE: &[u8] = b"a stand-in OCSP response";

/// B-LT without revocation evidence is refused, not written.
///
/// A DSS holding certificates and nothing else says who signed and not that
/// they were allowed to, so the file would carry the longer profile name on
/// the shorter profile's contents -- the same fault as a timestamp that does
/// not cover the signature it travels with (#515).
#[test]
fn long_term_without_revocation_evidence_is_refused() {
    let (signer, pdf) = fixtures_or_skip!();

    let err = sign_timestamped_to_bytes(
        &pdf,
        &signer,
        SignOptions::new().profile(PadesProfile::LongTerm),
    )
    .map(|signed| signed.len())
    .expect_err("B-LT without revocation evidence must be refused, not downgraded");

    let message = err.to_string();
    assert!(
        message.contains("B-LT") && message.contains("revocation_data"),
        "the refusal must name the profile and the setter that satisfies it, got: {message}",
    );
}

#[test]
fn a_profile_that_needs_a_timestamp_refuses_to_sign_without_one() {
    let (signer, pdf) = fixtures_or_skip!();

    for (profile, profile_name) in [
        (PadesProfile::Timestamped, "B-T"),
        (PadesProfile::LongTerm, "B-LT"),
    ] {
        // Reduced to a byte count first: on failure `expect_err` prints the `Ok`
        // side, and a whole signed PDF in the panic message helps nobody.
        let err = sign_to_bytes(&pdf, &signer, SignOptions::new().profile(profile))
            .map(|signed| signed.len())
            .expect_err("signing without a token must fail, not downgrade");
        let message = err.to_string();
        assert!(
            message.contains(profile_name) && message.contains("sign_timestamped"),
            "the error must name the profile and the route that can satisfy it. Naming \
             SignOptions::timestamp_token instead sends the caller to fetch a token now, \
             which the imprint check then refuses -- one refusal pointing at the next \
             (#515). Got: {message}",
        );
    }
}

#[test]
fn the_archive_profile_says_what_is_missing_instead_of_pretending() {
    let (signer, pdf) = fixtures_or_skip!();

    // A token does not help here: B-LTA needs a document timestamp written over
    // the whole file, which is a different object entirely. Both routes have to
    // say so; neither may quietly produce B-LT instead.
    let err = sign_timestamped_to_bytes(
        &pdf,
        &signer,
        SignOptions::new().profile(PadesProfile::LongTermArchive),
    )
    .map(|signed| signed.len())
    .expect_err("B-LTA is not implemented and must say so");

    let message = err.to_string();
    assert!(
        message.contains("DocTimeStamp"),
        "the error must name the object that is missing, so the reader can tell \
         an unimplemented feature from a misconfiguration, got: {message}",
    );
}

/// A token the caller held before signing stamps something else (#515).
///
/// This is what `SignOptions::timestamp_token` asked for: the token has to be
/// in hand before `sign` is called, so it cannot be about the signature `sign`
/// is about to make. The file used to come out carrying the timestamp attribute
/// anyway — right SubFilter, right OID, every marker above satisfied — and fail
/// the first verifier that recomputed the imprint. Refusing is the only honest
/// answer.
#[test]
fn a_timestamp_that_does_not_cover_this_signature_is_refused() {
    let (signer, pdf) = fixtures_or_skip!();

    let token_about_other_bytes = local_tsa::issue(b"whatever the caller had before signing");

    for (profile, profile_name) in [
        (PadesProfile::Timestamped, "B-T"),
        (PadesProfile::LongTerm, "B-LT"),
    ] {
        let err = match sign_to_bytes(
            &pdf,
            &signer,
            // Revocation evidence so B-LT gets as far as the token: this test
            // is about the timestamp, and a request the profile could not
            // support would be refused for the other reason first.
            SignOptions::new()
                .profile(profile)
                .timestamp_token(token_about_other_bytes.clone())
                .revocation_data(vec![AN_OCSP_RESPONSE.to_vec()], Vec::new()),
        ) {
            // Reduced to a byte count: a whole signed PDF in a panic message
            // helps nobody.
            Ok(signed) => panic!(
                "{profile_name} produced {} bytes carrying a timestamp that covers \
                 nothing in them",
                signed.len(),
            ),
            Err(e) => e,
        };
        let message = err.to_string();
        assert!(
            message.contains("imprint") || message.contains("timestamp"),
            "{profile_name}: the refusal must name the timestamp as the problem, got: {message}",
        );
        assert!(
            message.contains("sign_timestamped"),
            "{profile_name}: the refusal must name the route that works from this API, or \
             the caller has been told only that they are stuck, got: {message}",
        );
    }
}

/// The two-phase route hands the authority the signature, not the document.
///
/// What crosses the wire to a TSA is a digest of the CMS signature value. A
/// callback handed the whole file, or the byte range, or nothing at all would
/// each produce a token that our own imprint check then rejects — so this
/// asserts the shape of what the callback receives, where the failure is
/// readable, rather than leaving it to show up as a mismatch.
#[test]
fn the_authority_is_given_the_signature_to_stamp() {
    let (signer, pdf) = fixtures_or_skip!();

    let mut seen: Option<Vec<u8>> = None;
    let mut doc = PdfDocument::from_bytes(&pdf).expect("fixture opens");
    doc.sign_timestamped(
        &signer,
        SignOptions::new().profile(PadesProfile::Timestamped),
        |signature| {
            seen = Some(signature.to_vec());
            Ok(local_tsa::issue(signature))
        },
    )
    .expect("B-T signs");

    let stamped = seen.expect("the callback runs exactly once, before the token is embedded");
    assert!(
        !stamped.is_empty() && stamped.len() < pdf.len(),
        "the authority gets the signature value ({} bytes), not the document ({} bytes)",
        stamped.len(),
        pdf.len(),
    );
    assert!(
        contains(
            &signature_cms(&doc.to_bytes().expect("signed document serialises")),
            &stamped
        ),
        "and the bytes it stamped are the signature that ships",
    );
}

/// A refusal from the authority is a refusal to sign.
///
/// The alternative — write the signature and drop the timestamp — is the silent
/// downgrade to B-B that #176 removed, arriving through a different door.
#[test]
fn an_authority_that_refuses_leaves_no_document_behind() {
    let (signer, pdf) = fixtures_or_skip!();

    let mut doc = PdfDocument::from_bytes(&pdf).expect("fixture opens");
    let err = doc
        .sign_timestamped(
            &signer,
            SignOptions::new().profile(PadesProfile::Timestamped),
            |_| Err(pdfluent::Error::Unsupported("the TSA said no".into())),
        )
        .expect_err("a failed fetch must fail the signing");
    assert!(
        err.to_string().contains("the TSA said no"),
        "the caller's own error must survive, got: {err}",
    );
    assert!(
        !contains(
            &doc.to_bytes().expect("document still serialises"),
            b"ETSI.CAdES.detached"
        ),
        "and the document must be left as it was, not signed without its timestamp",
    );
}
