//! A timestamp authority that runs in this process (#515).
//!
//! Issues a real RFC 3161 `TimeStampToken` -- `ContentInfo` wrapping
//! `SignedData` whose encapsulated content is a `TSTInfo` with a genuine
//! SHA-256 `messageImprint` -- over whatever bytes it is handed, with every
//! field RFC 3161 makes mandatory. It does not *sign* the token: there is no
//! authority key, so its `SignerInfo` carries a placeholder signature.
//!
//! That is enough for the question these tests ask, which is whether the façade
//! stamps *this* signature. Answering it needs a token over the wrong bytes,
//! and no honest authority will issue one. Whether a token is trustworthy is
//! the verifier's question, needs a real authority, and is not claimed here.
//!
//! `crates/pdf-sign/tests/local_tsa/mod.rs` is the fuller copy, which also
//! stamps under SHA-384 and SHA-512. The duplication is deliberate: sharing it
//! would mean one crate's test tree reaching into another's by relative path,
//! which breaks the moment either crate is built from its own package.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// OID 1.2.840.113549.1.7.2 — id-signedData.
const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
/// OID 1.2.840.113549.1.9.16.1.4 — id-ct-TSTInfo.
const OID_TST_INFO: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x01, 0x04,
];
/// OID 1.3.6.1.4.1.53042.1 — the policy this stand-in authority stamps under.
const OID_POLICY: &[u8] = &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x83, 0x9E, 0x32, 0x01];
/// OID 2.16.840.1.101.3.4.2.1 — SHA-256.
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

/// Issue a SHA-256 timestamp token over `data`.
pub fn issue(data: &[u8]) -> Vec<u8> {
    use sha2::Digest as _;
    let imprint = sha2::Sha256::digest(data).to_vec();

    let mut mi = algorithm_identifier();
    mi.extend_from_slice(&der(0x04, &imprint));

    let mut tst = der_integer(1); // version v1
    tst.extend_from_slice(&der(0x06, OID_POLICY));
    tst.extend_from_slice(&der(0x30, &mi));
    tst.extend_from_slice(&der_integer(0x2039)); // serialNumber
    tst.extend_from_slice(&der(0x18, b"20260910120000Z")); // genTime
    let tst_info = der(0x30, &tst);

    // encapContentInfo ::= SEQUENCE { eContentType, [0] EXPLICIT OCTET STRING }
    let mut encap = der(0x06, OID_TST_INFO);
    encap.extend_from_slice(&der(0xA0, &der(0x04, &tst_info)));

    // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo, signerInfos }
    let mut signed_data = der_integer(3);
    signed_data.extend_from_slice(&der(0x31, &algorithm_identifier()));
    signed_data.extend_from_slice(&der(0x30, &encap));
    signed_data.extend_from_slice(&der(0x31, &signer_info(&tst_info)));

    // ContentInfo ::= SEQUENCE { contentType, [0] EXPLICIT SignedData }
    let mut content_info = der(0x06, OID_SIGNED_DATA);
    content_info.extend_from_slice(&der(0xA0, &der(0x30, &signed_data)));
    der(0x30, &content_info)
}

/// A structurally complete `SignerInfo` whose signature signs nothing.
fn signer_info(tst_info: &[u8]) -> Vec<u8> {
    let mut si = der_integer(1); // version
    let mut sid = der(0x30, &[]); // issuer: an empty RDNSequence
    sid.extend_from_slice(&der_integer(1)); // serialNumber
    si.extend_from_slice(&der(0x30, &sid));
    si.extend_from_slice(&algorithm_identifier());
    si.extend_from_slice(&signed_attrs(tst_info));
    si.extend_from_slice(&algorithm_identifier());
    si.extend_from_slice(&der(0x04, &[0u8; 32])); // signature: a placeholder
    der(0x30, &si)
}

/// The signed attributes CMS requires when the content is not `id-data`.
///
/// Content type and message digest. The digest is the real one over
/// `tst_info`: it is the one signed attribute a reader can check without the
/// authority's certificate, so a stand-in that faked it would be issuing
/// tokens the engine rightly refuses.
fn signed_attrs(tst_info: &[u8]) -> Vec<u8> {
    /// OID 1.2.840.113549.1.9.3 — id-contentType.
    const OID_CONTENT_TYPE: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03];
    /// OID 1.2.840.113549.1.9.4 — id-messageDigest.
    const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];
    /// OID 1.2.840.113549.1.9.16.2.47 — id-aa-signingCertificateV2.
    const OID_SIGNING_CERTIFICATE_V2: &[u8] = &[
        0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x2F,
    ];

    let mut content_type = der(0x06, OID_CONTENT_TYPE);
    content_type.extend_from_slice(&der(0x31, &der(0x06, OID_TST_INFO)));

    let mut message_digest = der(0x06, OID_MESSAGE_DIGEST);
    message_digest.extend_from_slice(&der(0x31, &der(0x04, &sha256(tst_info))));

    // RFC 3161 §2.4.1: the authority names the certificate it signed with.
    // What is in it is the verifier's business; that it is there is not.
    let mut signing_certificate = der(0x06, OID_SIGNING_CERTIFICATE_V2);
    let cert_id = der(0x30, &der(0x04, &[0x11; 32]));
    signing_certificate.extend_from_slice(&der(0x31, &der(0x30, &der(0x30, &cert_id))));

    let mut attrs = der(0x30, &content_type);
    attrs.extend_from_slice(&der(0x30, &message_digest));
    attrs.extend_from_slice(&der(0x30, &signing_certificate));
    der(0xA0, &attrs) // [0] IMPLICIT SET OF Attribute
}

fn algorithm_identifier() -> Vec<u8> {
    let mut content = der(0x06, OID_SHA256);
    content.extend_from_slice(&[0x05, 0x00]); // parameters NULL
    der(0x30, &content)
}

/// The SHA-256 of `data`.
fn sha256(data: &[u8]) -> Vec<u8> {
    use sha2::Digest as _;
    sha2::Sha256::digest(data).to_vec()
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
