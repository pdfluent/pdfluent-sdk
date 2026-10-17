//! A timestamp authority that runs in this process (#515).
//!
//! It issues real RFC 3161 `TimeStampToken` DER: a `ContentInfo` wrapping
//! `SignedData` whose encapsulated content is a `TSTInfo` with a genuine
//! `messageImprint` over whatever bytes it is handed, and every field RFC 3161
//! makes mandatory. What it does not do is *sign* the token -- there is no
//! authority key and no chain, so the `SignerInfo` it writes carries a
//! placeholder signature over nothing.
//!
//! That is the right shape for what is under test. The question is whether the
//! signing path checks *which bytes* a token stamps, and answering it needs a
//! token over the wrong bytes -- which is exactly the token a real authority
//! will never issue. A network TSA would also make the suite depend on somebody
//! else's uptime; this one runs on a train.
//!
//! Trusting the token (is the authority's own signature valid, is its
//! certificate in a trust list) is the verifier's question and needs a real
//! authority. Nothing here claims to answer it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent_sign::DigestAlgorithm;

/// OID 1.2.840.113549.1.7.2 — id-signedData.
const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
/// OID 1.2.840.113549.1.9.16.1.4 — id-ct-TSTInfo.
const OID_TST_INFO: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x01, 0x04,
];
/// OID 1.3.6.1.4.1.53042.1 — the policy this stand-in authority stamps under.
const OID_POLICY: &[u8] = &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x83, 0x9E, 0x32, 0x01];

/// Issue a timestamp token over `data`, hashed with `algorithm`.
pub fn issue(data: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    token_over_imprint(&digest(data, algorithm), algorithm)
}

/// Issue a token whose imprint is `imprint` verbatim, whatever that covers.
pub fn token_over_imprint(imprint: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    let mut tst = der_integer(1); // version v1
    tst.extend_from_slice(&der_oid(OID_POLICY));
    tst.extend_from_slice(&message_imprint(imprint, algorithm));
    tst.extend_from_slice(&der_integer(0x2039)); // serialNumber
    tst.extend_from_slice(&generalized_time("20260910120000Z"));
    let tst_info = der(0x30, &tst);

    // encapContentInfo ::= SEQUENCE { eContentType, [0] EXPLICIT OCTET STRING }
    let mut encap = der_oid(OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, &tst_info)));

    // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo,
    //                           [certificates], signerInfos }
    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&der(0x31, &algorithm_identifier(algorithm))); // digestAlgorithms
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(&der(0x31, &signer_info(&tst_info, algorithm))); // signerInfos

    // ContentInfo ::= SEQUENCE { contentType, [0] EXPLICIT SignedData }
    let mut content_info = der_oid(OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}

/// The digest `algorithm` takes of `data`.
pub fn digest(data: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    use sha2::Digest as _;
    match algorithm {
        DigestAlgorithm::Sha256 => sha2::Sha256::digest(data).to_vec(),
        DigestAlgorithm::Sha384 => sha2::Sha384::digest(data).to_vec(),
        DigestAlgorithm::Sha512 => sha2::Sha512::digest(data).to_vec(),
        other => panic!("the stand-in authority does not stamp under {other:?}"),
    }
}

fn message_imprint(hash: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    let mut content = algorithm_identifier(algorithm);
    content.extend_from_slice(&der(0x04, hash));
    der(0x30, &content)
}

/// A structurally complete `SignerInfo` whose signature signs nothing.
///
/// A `SignedData` with no signer carries a time nobody vouched for, so the
/// parser under test refuses one -- rightly. What is in the signature is not
/// what these tests are about; that it is there, and shaped like one, is. The
/// *digest* is real, because that one the parser can check without a key.
fn signer_info(tst_info: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    let mut si = der_integer(1); // version
    let mut sid = der(0x30, &[]); // issuer: an empty RDNSequence
    sid.extend_from_slice(&der_integer(1)); // serialNumber
    si.extend_from_slice(&der(0x30, &sid));
    si.extend_from_slice(&algorithm_identifier(algorithm));
    si.extend_from_slice(&signed_attrs(tst_info, algorithm));
    si.extend_from_slice(&algorithm_identifier(algorithm));
    si.extend_from_slice(&der(0x04, &[0u8; 32])); // signature: a placeholder
    der(0x30, &si)
}

/// The signed attributes a timestamp's `SignerInfo` must carry.
///
/// Content type and message digest, which CMS requires whenever the
/// encapsulated content is something other than `id-data`. The digest is the
/// real one over `tst_info`: it is the one signed attribute a reader can check
/// without the authority's certificate, so a stand-in that faked it would be
/// issuing tokens no verifier accepts.
/// OID 1.2.840.113549.1.9.3 — id-contentType.
const OID_CONTENT_TYPE: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03];
/// OID 1.2.840.113549.1.9.4 — id-messageDigest.
const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];
/// OID 1.2.840.113549.1.9.16.2.47 — id-aa-signingCertificateV2.
const OID_SIGNING_CERTIFICATE_V2: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x2F,
];

/// The ESS signing-certificate attribute, in its RFC 5035 encoding.
///
/// `SigningCertificateV2 ::= SEQUENCE { certs SEQUENCE OF ESSCertIDv2, ... }`.
/// What is in it identifies the authority's certificate, which this stand-in
/// does not have; the structure is what the parser under test reads.
fn signing_certificate() -> Vec<u8> {
    let mut cert_id = der(0x04, &[0x11; 32]); // ESSCertIDv2.certHash
    cert_id = der(0x30, &cert_id);
    let certs = der(0x30, &cert_id);

    let mut attribute = der_oid(OID_SIGNING_CERTIFICATE_V2);
    attribute.extend_from_slice(&der(0x31, &der(0x30, &certs)));
    attribute
}

fn signed_attrs(tst_info: &[u8], algorithm: DigestAlgorithm) -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &digest(tst_info, algorithm))));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    attrs.extend_from_slice(&der(0x30, &signing_certificate()));
    der(0xA0, &attrs) // [0] IMPLICIT SET OF Attribute
}

fn algorithm_identifier(algorithm: DigestAlgorithm) -> Vec<u8> {
    let oid: &[u8] = match algorithm {
        DigestAlgorithm::Sha256 => &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
        DigestAlgorithm::Sha384 => &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02],
        DigestAlgorithm::Sha512 => &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03],
        other => panic!("the stand-in authority does not stamp under {other:?}"),
    };
    let mut content = der_oid(oid);
    content.extend_from_slice(&[0x05, 0x00]); // parameters NULL
    der(0x30, &content)
}

/// A DER TLV: `tag`, a definite length, and `content`.
fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let first = bytes
            .iter()
            .position(|&b| b != 0)
            .unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - first) as u8);
        out.extend_from_slice(&bytes[first..]);
    }
    out.extend_from_slice(content);
    out
}

fn der_oid(body: &[u8]) -> Vec<u8> {
    der(0x06, body)
}

fn der_integer(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    let mut body = Vec::new();
    if bytes[first] & 0x80 != 0 {
        body.push(0x00); // keep it positive
    }
    body.extend_from_slice(&bytes[first..]);
    der(0x02, &body)
}

fn generalized_time(value: &str) -> Vec<u8> {
    der(0x18, value.as_bytes())
}

/// A token whose outer `ContentInfo.contentType` is not `id-signedData`.
///
/// Everything else is a valid token over `data`. Only the identifier that says
/// what this structure is has been changed, which is precisely the field a
/// shape-only walk never reads.
pub fn issue_with_wrong_outer_type(data: &[u8]) -> Vec<u8> {
    replace_first(
        &issue(data, DigestAlgorithm::Sha256),
        OID_SIGNED_DATA,
        OID_DATA,
    )
}

/// A token whose encapsulated `eContentType` is not `id-ct-TSTInfo`.
pub fn issue_with_wrong_content_type(data: &[u8]) -> Vec<u8> {
    replace_first(
        &issue(data, DigestAlgorithm::Sha256),
        OID_TST_INFO,
        OID_CT_DATA,
    )
}

/// OID 1.2.840.113549.1.7.1 — id-data, the same length as id-signedData.
const OID_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];
/// OID 1.2.840.113549.1.9.16.1.9 — id-ct-compressedData, as long as id-ct-TSTInfo.
const OID_CT_DATA: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x01, 0x09,
];

/// Overwrite the first occurrence of `needle` in place; lengths must match, so
/// every DER length around it stays correct and only the identity changes.
fn replace_first(der: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    assert_eq!(
        needle.len(),
        replacement.len(),
        "a swap must not move any length"
    );
    let at = der
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the OID this token was built with is in it");
    let mut out = der.to_vec();
    out[at..at + needle.len()].copy_from_slice(replacement);
    out
}

/// A token whose `TSTInfo` stops after `messageImprint`.
///
/// `serialNumber` and `genTime` are gone; everything else, the authority's own
/// `SignerInfo` included, is intact. Built by re-encoding rather than by
/// truncating bytes, so every DER length stays right and the only thing wrong
/// with the structure is what is absent from it.
pub fn issue_without_serial_and_gen_time(data: &[u8]) -> Vec<u8> {
    let mut tst = der_integer(1);
    tst.extend_from_slice(&der_oid(OID_POLICY));
    tst.extend_from_slice(&message_imprint(
        &digest(data, DigestAlgorithm::Sha256),
        DigestAlgorithm::Sha256,
    ));
    let tst_info = der(0x30, &tst);
    let signer = signer_info(&tst_info, DigestAlgorithm::Sha256);
    wrap(&tst_info, &der(0x31, &signer))
}

/// A token nobody signed: a complete `TSTInfo` and no `SignerInfo`.
///
/// `signer_infos` decides what stands in the SET: nothing at all, or a NULL,
/// which is not empty and is not a signature either.
pub fn issue_without_a_signer(data: &[u8], signer_infos: &[u8]) -> Vec<u8> {
    wrap(&tst_info_over(data), &der(0x31, signer_infos))
}

/// The complete `TSTInfo` this stand-in authority writes over `data`.
///
/// The fixtures that hand a hand-built `SignerInfo` to `issue_without_a_signer`
/// need the same bytes, because the message-digest attribute has to be the
/// digest of exactly this structure.
fn tst_info_over(data: &[u8]) -> Vec<u8> {
    let mut tst = der_integer(1);
    tst.extend_from_slice(&der_oid(OID_POLICY));
    tst.extend_from_slice(&message_imprint(
        &digest(data, DigestAlgorithm::Sha256),
        DigestAlgorithm::Sha256,
    ));
    tst.extend_from_slice(&der_integer(0x2039));
    tst.extend_from_slice(&generalized_time("20260910120000Z"));
    der(0x30, &tst)
}

/// A `SignerInfo` with no `signedAttrs` at all.
///
/// CMS requires them whenever the encapsulated content is not `id-data`, and a
/// timestamp's is `id-ct-TSTInfo`. Everything else is in order.
pub fn a_signer_without_signed_attributes() -> Vec<u8> {
    let mut si = der_integer(1);
    let mut sid = der(0x30, &[]);
    sid.extend_from_slice(&der_integer(1));
    si.extend_from_slice(&der(0x30, &sid));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0x04, &[0u8; 32]));
    der(0x30, &si)
}

/// A `SignerInfo` whose `signedAttrs` holds bytes that are not attributes.
///
/// The field is there and non-empty, but a NULL is not an Attribute, so the
/// content-type and message-digest attributes CMS requires are not in it.
pub fn a_signer_whose_signed_attributes_are_not_attributes() -> Vec<u8> {
    a_signer_with_these_signed_attributes(&der(0xA0, &[0x05, 0x00]))
}

/// A `SignerInfo` carrying only the content-type attribute.
///
/// CMS requires the message digest beside it; without one nothing ties the
/// authority's signature to the time it encapsulates.
pub fn a_signer_without_a_message_digest() -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));
    a_signer_with_these_signed_attributes(&der(0xA0, &der(0x30, &content_type)))
}

/// A good token with an extra TLV inside its outer `ContentInfo`.
///
/// The bytes sit after the `[0] content` and inside the outer SEQUENCE, so a
/// check on what follows the token never sees them -- and all of the bytes
/// handed in are what gets embedded.
pub fn issue_with_something_hidden_inside_it(data: &[u8]) -> Vec<u8> {
    let token = issue(data, DigestAlgorithm::Sha256);
    let (_, content_info) = split_tlv(&token);
    let mut body = content_info.to_vec();
    body.extend_from_slice(&[0x05, 0x00]); // an ASN.1 NULL, tucked in
    der(0x30, &body)
}

/// A token whose `SignedData.digestAlgorithms` is a NULL rather than a SET.
pub fn issue_without_digest_algorithms(data: &[u8]) -> Vec<u8> {
    let tst_info = tst_info_over(data);

    let mut encap = der_oid(OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, &tst_info)));

    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&[0x05, 0x00]); // digestAlgorithms: a NULL
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(&der(0x31, &signer_info(&tst_info, DigestAlgorithm::Sha256)));

    let mut content_info = der_oid(OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}

/// The content of the first TLV in `der`, and what follows it.
fn split_tlv(der: &[u8]) -> (&[u8], &[u8]) {
    let first = der[1];
    let (len, at) = if first & 0x80 == 0 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7F) as usize;
        let mut len = 0usize;
        for b in &der[2..2 + n] {
            len = (len << 8) | *b as usize;
        }
        (len, 2 + n)
    };
    (&der[at + len..], &der[at..at + len])
}

/// A `SignerInfo` whose `sid` is a SEQUENCE with nothing usable in it.
///
/// `IssuerAndSerialNumber` is a Name followed by a serial number. The outer
/// SEQUENCE is the right tag, so an arm that stops at the wrapper accepts a
/// signer identified by nothing at all.
pub fn a_signer_whose_identity_is_an_empty_shell() -> Vec<u8> {
    let mut si = der_integer(1);
    si.extend_from_slice(&der(0x30, &[0x05, 0x00])); // sid: SEQUENCE { NULL }
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0xA0, &[0x05, 0x00]));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0x04, &[0u8; 32]));
    der(0x30, &si)
}

/// A `SignerInfo` whose content-type attribute holds two values.
///
/// The attribute appears once and its SET has two OIDs in it, the right one
/// first. A reader that stops after the value it wanted never sees the second,
/// and a verifier has no way to say which the signature covers.
pub fn a_signer_whose_content_type_has_two_values(data: &[u8]) -> Vec<u8> {
    /// OID 1.2.840.113549.1.7.1 -- id-data.
    const OID_DATA_TOO: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];

    let mut values = der_oid(OID_TST_INFO);
    values.extend_from_slice(&der_oid(OID_DATA_TOO));

    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &values));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A `SignerInfo` carrying the content-type attribute twice.
///
/// Both copies are well formed and say the same thing. CMS gives a SignerInfo
/// one of each of these attributes, because a verifier reading two has no way
/// to say which the signature was computed over.
pub fn a_signer_that_says_it_twice(data: &[u8]) -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &content_type));
    attrs.extend_from_slice(&der(0x30, &message_digest));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A `SignerInfo` whose signing-certificate attribute names no certificate.
///
/// The attribute is there and its value is a SEQUENCE, which is the right tag
/// around nothing: no `certs`, no ESSCertID, nothing that says which key made
/// the token.
pub fn a_signer_naming_no_certificate(data: &[u8]) -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));

    let mut hollow = der_oid(OID_SIGNING_CERTIFICATE_V2);
    hollow.extend_from_slice(&der(0x31, &der(0x30, &[])));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    attrs.extend_from_slice(&der(0x30, &hollow));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A token with a second TLV inside the encapsulated content.
///
/// The eContent OCTET STRING holds one TSTInfo and a NULL after it. The
/// authority's own message digest covers all of it, so every later check can
/// pass on a content a conforming decoder refuses to read.
pub fn issue_with_something_after_the_tst_info(data: &[u8]) -> Vec<u8> {
    let mut content = tst_info_over(data);
    content.extend_from_slice(&[0x05, 0x00]);

    let mut encap = der_oid(OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, &content)));

    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&der(0x31, &algorithm_identifier(DigestAlgorithm::Sha256)));
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(&der(0x31, &signer_info(&content, DigestAlgorithm::Sha256)));

    let mut content_info = der_oid(OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}

/// A `SignerInfo` with both CMS attributes and no signing-certificate.
///
/// RFC 3161 §2.4.1 asks the authority to name the certificate it signed with;
/// a token without it does not say whose key a verifier should check.
pub fn a_signer_without_a_signing_certificate(data: &[u8]) -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A token whose `digestAlgorithms` names a digest its own signer did not use.
///
/// The SET is well formed and holds SHA-384; the `SignerInfo` signs under
/// SHA-256. CMS lets the SET be a superset, not a different set.
pub fn issue_listing_another_digest(data: &[u8]) -> Vec<u8> {
    let tst_info = tst_info_over(data);

    let mut encap = der_oid(OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, &tst_info)));

    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&der(0x31, &algorithm_identifier(DigestAlgorithm::Sha384)));
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(&der(0x31, &signer_info(&tst_info, DigestAlgorithm::Sha256)));

    let mut content_info = der_oid(OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}

/// A `SignerInfo` whose message-digest attribute digests something else.
///
/// Both mandatory attributes are there and well formed. The digest is the
/// right length and the wrong value, which is what a stale or corrupt token
/// looks like -- and the one signed attribute a reader can catch on its own,
/// because the content it covers travels in the same structure.
pub fn a_signer_whose_message_digest_is_wrong() -> Vec<u8> {
    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_TST_INFO)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &[0u8; 32])));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A `SignerInfo` carrying only the message-digest attribute.
///
/// The other mandatory one is missing, so nothing in the signature says what
/// kind of content it covers.
pub fn a_signer_without_a_content_type(data: &[u8]) -> Vec<u8> {
    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));
    a_signer_with_these_signed_attributes(&der(0xA0, &der(0x30, &message_digest)))
}

/// A `SignerInfo` whose content-type attribute names `id-data`.
///
/// The attribute is well formed and says this signature is over plain data --
/// which contradicts the `id-ct-TSTInfo` the SignedData around it declares.
pub fn a_signer_that_signed_something_else(data: &[u8]) -> Vec<u8> {
    /// OID 1.2.840.113549.1.7.1 -- id-data.
    const OID_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];

    let mut content_type = der_oid(OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der_oid(OID_DATA)));

    let mut message_digest = der_oid(OID_MESSAGE_DIGEST);
    let hash = digest(&tst_info_over(data), DigestAlgorithm::Sha256);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &hash)));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    a_signer_with_these_signed_attributes(&der(0xA0, &attrs))
}

/// A `SignerInfo` that is in order apart from the `signedAttrs` handed in.
fn a_signer_with_these_signed_attributes(signed_attrs: &[u8]) -> Vec<u8> {
    let mut si = der_integer(1);
    let mut sid = der(0x30, &[]);
    sid.extend_from_slice(&der_integer(1));
    si.extend_from_slice(&der(0x30, &sid));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(signed_attrs);
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0x04, &[0u8; 32]));
    der(0x30, &si)
}

/// A good token with one more TLV stuck on the end.
///
/// All of these bytes are what gets embedded, so the extra one would become a
/// second value in the timestamp attribute's SET.
pub fn issue_with_something_after_it(data: &[u8]) -> Vec<u8> {
    let mut out = issue(data, DigestAlgorithm::Sha256);
    out.extend_from_slice(&[0x05, 0x00]); // an ASN.1 NULL, along for the ride
    out
}

/// An ASN.1 NULL: non-empty bytes that sign nothing.
pub const A_NULL_INSTEAD_OF_A_SIGNER: &[u8] = &[0x05, 0x00];

/// A token whose signer is named by `subjectKeyIdentifier` rather than by issuer.
///
/// `[0] IMPLICIT OCTET STRING` is primitive, so the tag is `0x80` -- not the
/// constructed `0xA0` a reader might expect from a context-specific field.
/// Authorities do use this arm, so it has to be accepted.
pub fn issue_naming_the_signer_by_key(data: &[u8]) -> Vec<u8> {
    let mut tst = der_integer(1);
    tst.extend_from_slice(&der_oid(OID_POLICY));
    tst.extend_from_slice(&message_imprint(
        &digest(data, DigestAlgorithm::Sha256),
        DigestAlgorithm::Sha256,
    ));
    tst.extend_from_slice(&der_integer(0x2039));
    tst.extend_from_slice(&generalized_time("20260910120000Z"));

    let tst_info = der(0x30, &tst);

    let mut si = der_integer(3); // version 3 goes with subjectKeyIdentifier
    si.extend_from_slice(&der(0x80, &[0xAB; 20])); // sid: [0] IMPLICIT, primitive
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&signed_attrs(&tst_info, DigestAlgorithm::Sha256));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0x04, &[0u8; 32]));

    wrap(&tst_info, &der(0x31, &der(0x30, &si)))
}

/// A `SignerInfo` whose `sid` is an ASN.1 NULL rather than an identity.
///
/// `SignerIdentifier` is a CHOICE of two things -- an issuerAndSerialNumber
/// SEQUENCE or a [0] subjectKeyIdentifier -- and a NULL is neither. Everything
/// else about the SignerInfo is in order.
pub fn a_signer_without_an_identity(data: &[u8]) -> Vec<u8> {
    let mut si = der_integer(1); // version
    si.extend_from_slice(&[0x05, 0x00]); // sid: a NULL, which identifies nobody
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&signed_attrs(&tst_info_over(data), DigestAlgorithm::Sha256));
    si.extend_from_slice(&algorithm_identifier(DigestAlgorithm::Sha256));
    si.extend_from_slice(&der(0x04, &[0u8; 32]));
    der(0x30, &si)
}

/// `tst_info` and `signer_infos` as a ContentInfo/SignedData token.
fn wrap(tst_info: &[u8], signer_infos: &[u8]) -> Vec<u8> {
    let mut encap = der_oid(OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, tst_info)));

    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&der(0x31, &algorithm_identifier(DigestAlgorithm::Sha256)));
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(signer_infos);

    let mut content_info = der_oid(OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}
