//! A PAdES B-T timestamp has to stamp the signature in the same file (#515).
//!
//! The pre-supplied-token API takes the RFC 3161 token as an input parameter,
//! so the caller has to hold it *before* signing begins. At that moment the
//! signature does not exist, which means the token's `messageImprint` cannot be
//! the digest of the signature that ends up in `/Contents` -- and the token was
//! carried into the CMS as opaque bytes, unread. The result was a document that
//! advertised B-T (timestamp attribute present, right SubFilter, right OID) and
//! failed at the first verifier that recomputed the imprint. Nothing in the
//! suite noticed, because every existing test asserted the attribute was
//! *there*, never that it was *about* this signature.
//!
//! What this file pins down:
//!
//! - a token over other bytes is refused, by both the one-shot API and the
//!   two-phase one;
//! - the two-phase API hands over exactly the bytes a TSA has to stamp, and
//!   accepts the token that comes back;
//! - the accepted token really is in the file, so the refusal is not achieved
//!   by refusing everything.
//!
//! The authority is local: `local_tsa::issue` builds a real RFC 3161
//! `TimeStampToken` (`ContentInfo` → `SignedData` → `TSTInfo` with a genuine
//! `messageImprint`) over whichever bytes it is given. A network TSA would make
//! the suite depend on somebody else's uptime and would prove less: what is
//! being tested is whether *we* check the imprint, and for that the interesting
//! token is the wrong one, which no honest authority will issue.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod local_tsa;

use std::path::PathBuf;

use pdfluent_sign::{
    sign_pdf_ltv_with_token, sign_pdf_pending_timestamp, sign_pdf_timestamped, DigestAlgorithm,
    Pkcs12Signer, SignOptions,
};

/// DER body of OID 1.2.840.113549.1.9.16.2.14, id-smime-aa-timeStampToken.
const OID_TIMESTAMP_TOKEN: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x0E,
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ sits two below the repository root")
        .to_path_buf()
}

/// The signing identity and a document, or `None` outside the repository.
fn fixtures() -> Option<(Pkcs12Signer, Vec<u8>)> {
    let p12 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-rsa.p12");
    let pdf = repo_root().join("tests/corpus-mini/simple.pdf");
    if !p12.exists() || !pdf.exists() {
        return None;
    }
    let signer = Pkcs12Signer::from_pkcs12(&std::fs::read(p12).ok()?, "test123").ok()?;
    Some((signer, std::fs::read(pdf).ok()?))
}

macro_rules! fixtures_or_skip {
    () => {
        match fixtures() {
            Some(pair) => pair,
            None => {
                eprintln!("skipping: repository fixture tree not present");
                return;
            }
        }
    };
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Decode the CMS out of the last `/Contents <HEX>` in a signed file.
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

/// The reproduction: a token held before signing stamps something else.
///
/// This is the shape every caller of the one-shot API was in. The token is
/// perfectly well formed and issued by a perfectly honest authority; it simply
/// covers bytes that are not the signature this call produces, because at the
/// time it was issued that signature did not exist.
#[test]
fn a_token_obtained_before_signing_is_refused_rather_than_embedded() {
    let (signer, pdf) = fixtures_or_skip!();

    let token_over_something_else = local_tsa::issue(
        b"whatever the caller had to hand before signing",
        DigestAlgorithm::Sha256,
    );

    let err = sign_pdf_timestamped(
        &pdf,
        &signer,
        &SignOptions::default(),
        &token_over_something_else,
    )
    .map(|signed| signed.len())
    .expect_err("a token over other bytes must not become a B-T claim");

    let message = err.to_string();
    assert!(
        message.contains("timestamp") && message.contains("signature"),
        "the refusal must say the timestamp is not about this signature, got: {message}",
    );
    assert!(
        message.contains("sign_pdf_pending_timestamp"),
        "the refusal must name the API that can succeed, or the caller has been told \
         only that they are stuck, got: {message}",
    );
}

/// The same for B-LT: validation material does not rescue a wrong timestamp.
#[test]
fn the_long_term_profile_refuses_the_same_token() {
    let (signer, pdf) = fixtures_or_skip!();

    let token_over_something_else =
        local_tsa::issue(b"not this signature", DigestAlgorithm::Sha256);

    let err = sign_pdf_ltv_with_token(
        &pdf,
        &signer,
        &SignOptions::default(),
        &token_over_something_else,
        Vec::new(),
        Vec::new(),
    )
    .map(|signed| signed.len())
    .expect_err("B-LT on a mismatched timestamp is a longer-lived false claim, not a stronger one");

    assert!(
        err.to_string().contains("timestamp"),
        "the refusal must name the timestamp as the problem, got: {err}",
    );
}

/// The route that works: sign, stamp what was signed, embed.
#[test]
fn the_two_phase_api_stamps_the_signature_that_ships() {
    let (signer, pdf) = fixtures_or_skip!();

    let pending = sign_pdf_pending_timestamp(&pdf, &signer, &SignOptions::default())
        .expect("phase one signs");

    assert!(
        !pending.signature_value().is_empty(),
        "the authority has to be given something to stamp",
    );
    assert_eq!(
        pending
            .message_imprint(DigestAlgorithm::Sha256)
            .map(|h| h.len()),
        Some(32),
        "the imprint handed to a TSA client is a SHA-256 digest of the signature",
    );

    let token = local_tsa::issue(pending.signature_value(), DigestAlgorithm::Sha256);
    let signed = pending
        .timestamped(&token)
        .expect("a matching token is accepted");

    assert!(
        contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "the accepted token must actually be in the CMS; a check that refuses \
         everything would pass the two tests above and ship no timestamps at all",
    );
    assert!(
        contains(&signed, b"ETSI.CAdES.detached"),
        "and the result is still a PAdES signature",
    );
}

/// The digest the authority chose is the digest we check under.
///
/// A verifier reads the algorithm out of the token; so must we. Checking
/// everything as SHA-256 would reject every SHA-512 authority, and the failure
/// would read as "wrong signature" rather than "wrong assumption".
#[test]
fn a_token_under_another_digest_is_still_recognised_as_ours() {
    let (signer, pdf) = fixtures_or_skip!();

    let pending = sign_pdf_pending_timestamp(&pdf, &signer, &SignOptions::default())
        .expect("phase one signs");
    let token = local_tsa::issue(pending.signature_value(), DigestAlgorithm::Sha512);

    let signed = pending
        .timestamped(&token)
        .expect("a SHA-512 authority stamps the same signature");
    assert!(
        contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "a SHA-512 token belongs in the CMS just as a SHA-256 one does",
    );
}

/// Bytes that are not a token at all are refused too.
///
/// The old path accepted these: they were never parsed. A stand-in DER blob
/// produced a file with a timestamp attribute containing no timestamp.
#[test]
fn a_blob_that_is_not_a_timestamp_token_is_refused() {
    let (signer, pdf) = fixtures_or_skip!();

    // SEQUENCE { OID 1.2.840.113549.1.7.2, [0] { SEQUENCE { INTEGER 1 } } } --
    // shaped like a ContentInfo, carrying no TSTInfo. This exact blob was the
    // stand-in token the profile tests used, and it signed as B-T.
    let stand_in = vec![
        0x30, 0x12, 0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02, 0xA0, 0x05,
        0x30, 0x03, 0x02, 0x01, 0x01,
    ];

    let err = sign_pdf_timestamped(&pdf, &signer, &SignOptions::default(), &stand_in)
        .map(|signed| signed.len())
        .expect_err("a blob with no TSTInfo cannot be shown to stamp anything");

    let message = err.to_string();
    assert!(
        message.contains("not an RFC 3161 timestamp token"),
        "a caller has to be able to tell a malformed token from a mismatched one; \
         reporting this as an imprint mismatch would send them looking for a fault in \
         their signature. Got: {message}",
    );
    assert!(
        ["ContentInfo", "SignedData", "TSTInfo"]
            .iter()
            .any(|part| message.contains(part)),
        "and it must say where the structure gave out, got: {message}",
    );
}

/// A well-formed CMS that is not a timestamp is refused on its OIDs.
///
/// Nesting is not identity. Something shaped exactly like a TimeStampToken --
/// ContentInfo, SignedData, an encapsulated OCTET STRING that parses as TSTInfo
/// and whose imprint is the digest of this very signature -- is still not a
/// timestamp if its content types say it is something else, and a verifier
/// reads those content types. Walking the shape without reading them would
/// accept it, and the resulting file would advertise B-T on bytes no authority
/// ever stamped.
#[test]
fn a_structure_that_is_not_a_timestamp_token_is_refused_on_its_content_type() {
    let (signer, pdf) = fixtures_or_skip!();

    let pending = sign_pdf_pending_timestamp(&pdf, &signer, &SignOptions::default())
        .expect("phase one signs");

    for (mangled, what) in [
        (
            local_tsa::issue_with_wrong_outer_type(pending.signature_value()),
            "the outer contentType",
        ),
        (
            local_tsa::issue_with_wrong_content_type(pending.signature_value()),
            "the encapsulated eContentType",
        ),
    ] {
        let err = match sign_pdf_timestamped(&pdf, &signer, &SignOptions::default(), &mangled) {
            Ok(signed) => panic!(
                "{what} was not read: {} bytes signed on something that is not a timestamp token",
                signed.len(),
            ),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("object identifier"),
            "{what}: the refusal must say the identifier is wrong rather than blaming the \
             imprint, or the caller looks for a fault that is not there. Got: {err}",
        );
    }
}

/// A token missing the fields that say when the stamp was made is refused.
///
/// A right imprint is not enough. `serialNumber` and `genTime` are mandatory in
/// TSTInfo, and `signerInfos` is where the authority vouches for the time. Each
/// case here removes exactly one of the two, so neither check can pass for the
/// other's reason: a structure with the field we compare and nothing else would
/// be accepted by a parser that reads only as far as the comparison needs, and
/// rejected by every verifier downstream.
#[test]
fn a_token_without_its_mandatory_fields_is_refused() {
    let (signer, pdf) = fixtures_or_skip!();

    let pending = sign_pdf_pending_timestamp(&pdf, &signer, &SignOptions::default())
        .expect("phase one signs");
    let stamped = pending.signature_value();

    for (hollow, missing) in [
        (
            local_tsa::issue_without_serial_and_gen_time(stamped),
            "serialNumber",
        ),
        (
            local_tsa::issue_without_a_signer(stamped, &[]),
            "signerInfos",
        ),
        // Not empty, and not a signature: a SET is a container, so "it has
        // something in it" says nothing about whether anybody signed.
        (
            local_tsa::issue_without_a_signer(stamped, local_tsa::A_NULL_INSTEAD_OF_A_SIGNER),
            "signerInfos[0]",
        ),
        // A signer with no identity in it: the CHOICE has two arms and a NULL
        // is neither of them.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_without_an_identity(stamped),
            ),
            "SignerInfo.sid",
        ),
        // No signed attributes at all: CMS requires them whenever the
        // encapsulated content is not id-data, and a timestamp's never is.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_without_signed_attributes(),
            ),
            "signedAttrs",
        ),
        // signedAttrs present but holding a NULL: a SET with bytes in it is
        // not a SET of Attribute, and the two CMS requires are not there.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_whose_signed_attributes_are_not_attributes(),
            ),
            "Attribute",
        ),
        // Well-formed attributes, but only one of the two mandatory ones.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_without_a_message_digest(),
            ),
            "message-digest",
        ),
        // A sid that is the right tag and holds nothing: IssuerAndSerialNumber
        // is a Name and a serial number, and neither is in there.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_whose_identity_is_an_empty_shell(),
            ),
            "sid.issuerAndSerialNumber.issuer",
        ),
        // One content-type attribute with two values in its SET.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_whose_content_type_has_two_values(stamped),
            ),
            "more than one value",
        ),
        // The content type said twice. CMS gives it exactly once.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_that_says_it_twice(stamped),
            ),
            "appears more than once",
        ),
        // Both attributes there and well formed, and the digest is over
        // something other than the TSTInfo it is signed with.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_whose_message_digest_is_wrong(),
            ),
            "not the SHA-256 digest of the TSTInfo",
        ),
        // Both CMS attributes and no signing certificate: the token does not
        // say whose key made it.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_without_a_signing_certificate(stamped),
            ),
            "no signing-certificate attribute",
        ),
        // The signing-certificate attribute is a SEQUENCE around nothing.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_naming_no_certificate(stamped),
            ),
            "signing-certificate certs",
        ),
        // A NULL after the TSTInfo inside the encapsulated content.
        (
            local_tsa::issue_with_something_after_the_tst_info(stamped),
            "follow the TSTInfo",
        ),
        // digestAlgorithms names SHA-384 while its own signer used SHA-256.
        (
            local_tsa::issue_listing_another_digest(stamped),
            "does not list SHA-256",
        ),
        // The other way round: a message digest and no content type.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_without_a_content_type(stamped),
            ),
            "no content-type attribute",
        ),
        // Both attributes there, and the content type names id-data -- a
        // different signature than the SignedData around it declares.
        (
            local_tsa::issue_without_a_signer(
                stamped,
                &local_tsa::a_signer_that_signed_something_else(stamped),
            ),
            "content-type",
        ),
        // The same trick one level in: the extra TLV sits inside the outer
        // ContentInfo, where a check on what follows the token cannot see it.
        (
            local_tsa::issue_with_something_hidden_inside_it(stamped),
            "inside the ContentInfo",
        ),
        // digestAlgorithms replaced by a NULL: the right place, the wrong tag.
        (
            local_tsa::issue_without_digest_algorithms(stamped),
            "SignedData.digestAlgorithms",
        ),
        // A good token with one more TLV after it. All of the bytes handed in
        // are what gets embedded, so the extra one would ride along into the
        // attribute's SET.
        (
            local_tsa::issue_with_something_after_it(stamped),
            "follow the TimeStampToken",
        ),
    ] {
        let err = match sign_pdf_timestamped(&pdf, &signer, &SignOptions::default(), &hollow) {
            Ok(signed) => panic!(
                "{missing} was not required: {} bytes signed on a token with the right \
                 imprint and no {missing}",
                signed.len(),
            ),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains(missing),
            "the refusal must name the field that is missing ({missing}), got: {err}",
        );
    }
}

/// An authority that names itself by key, not by issuer, is accepted.
///
/// `subjectKeyIdentifier` is the other arm of the `SignerIdentifier` CHOICE,
/// and `[0] IMPLICIT OCTET STRING` is primitive -- tag `0x80`, not the
/// constructed `0xA0`. Checking for `0xA0` alone would refuse a perfectly good
/// token from a real TSA, which is the opposite fault to the one the sid check
/// exists for and just as bad: a caller who cannot sign B-T at all.
#[test]
fn an_authority_that_names_itself_by_key_is_accepted() {
    let (signer, pdf) = fixtures_or_skip!();

    let pending = sign_pdf_pending_timestamp(&pdf, &signer, &SignOptions::default())
        .expect("phase one signs");
    let token = local_tsa::issue_naming_the_signer_by_key(pending.signature_value());

    let signed = pending
        .timestamped(&token)
        .expect("a token whose signer is named by key stamps this signature too");
    assert!(
        contains(&signature_cms(&signed), OID_TIMESTAMP_TOKEN),
        "and it belongs in the CMS like any other",
    );
}
