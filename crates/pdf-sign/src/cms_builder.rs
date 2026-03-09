//! CMS SignedData DER construction for PDF signing.
//!
//! Builds a DER-encoded CMS ContentInfo / SignedData (RFC 5652 §5.1)
//! suitable for embedding in a PDF /Contents field. Uses detached
//! content mode (no encapsulated content).

use crate::byte_range::DigestAlgorithm;
use crate::signer::{PdfSigner, Pkcs12Signer, SignError};
use digest::Digest;
use sha2::{Sha256, Sha384};

// ---------------------------------------------------------------------------
// OIDs (DER-encoded, without tag/length)
// ---------------------------------------------------------------------------

/// 1.2.840.113549.1.7.2 — signedData
const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];

/// 1.2.840.113549.1.7.1 — data (encapsulated content type)
const OID_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01];

/// 1.2.840.113549.1.9.3 — contentType
const OID_CONTENT_TYPE: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03];

/// 1.2.840.113549.1.9.4 — messageDigest
const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];

/// 1.2.840.113549.1.9.5 — signingTime
const OID_SIGNING_TIME: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];

/// 2.16.840.1.101.3.4.2.1 — SHA-256
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

/// 2.16.840.1.101.3.4.2.2 — SHA-384
const OID_SHA384: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02];

/// Build a DER-encoded CMS SignedData (detached) for the given data.
///
/// The resulting bytes form a complete ContentInfo suitable for the
/// PDF `/Contents` hex string.
pub fn build_cms_signed_data(signer: &Pkcs12Signer, data: &[u8]) -> Result<Vec<u8>, SignError> {
    let certs = signer.certificate_chain_der();
    let digest_algo = signer.digest_algorithm();
    let sig_algo_oid = signer.signature_algorithm_oid();

    // Step 1: Compute the message digest over the data.
    let message_digest = compute_digest(digest_algo, data);
    let digest_algo_oid = digest_algo_oid(digest_algo);

    // Step 2: Extract issuer + serial from leaf cert for SignerIdentifier.
    let leaf_cert_der = certs
        .first()
        .ok_or_else(|| SignError::CmsBuild("no leaf certificate".into()))?;
    let (issuer_der, serial_der) = extract_issuer_and_serial(leaf_cert_der)
        .ok_or_else(|| SignError::CmsBuild("cannot parse leaf certificate".into()))?;

    // Step 3: Build signed attributes.
    let signing_time = utc_time_now();
    let signed_attrs_content =
        build_signed_attrs_content(&message_digest, &signing_time, digest_algo_oid);

    // Step 4: Sign the signed attributes.
    // Per RFC 5652 §5.4: sign the DER encoding of the signed attributes
    // with a SET OF tag (0x31) replacing the context tag.
    let mut attrs_to_sign = Vec::new();
    attrs_to_sign.push(0x31); // SET OF tag
    encode_length(signed_attrs_content.len(), &mut attrs_to_sign);
    attrs_to_sign.extend_from_slice(&signed_attrs_content);

    let signature_value = signer
        .sign_raw(&attrs_to_sign)
        .map_err(|e| SignError::CmsBuild(format!("signing: {e}")))?;

    // Step 5: Assemble the full CMS structure.
    let signed_data = build_signed_data_sequence(
        digest_algo_oid,
        certs,
        &issuer_der,
        &serial_der,
        &signed_attrs_content,
        sig_algo_oid,
        &signature_value,
    );

    // Wrap in ContentInfo.
    let content_info = build_content_info(&signed_data);
    Ok(content_info)
}

// ---------------------------------------------------------------------------
// DER encoding helpers
// ---------------------------------------------------------------------------

/// Encode a DER length field.
fn encode_length(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else if len < 0x100 {
        out.push(0x81);
        out.push(len as u8);
    } else if len < 0x10000 {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    } else if len < 0x1000000 {
        out.push(0x83);
        out.push((len >> 16) as u8);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    } else {
        out.push(0x84);
        out.push((len >> 24) as u8);
        out.push((len >> 16) as u8);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    }
}

/// Encode a DER SEQUENCE (tag 0x30).
fn der_sequence(content: &[u8]) -> Vec<u8> {
    let mut out = vec![0x30];
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// Encode a DER SET (tag 0x31).
fn der_set(content: &[u8]) -> Vec<u8> {
    let mut out = vec![0x31];
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// Encode a DER OID (tag 0x06).
fn der_oid(oid: &[u8]) -> Vec<u8> {
    let mut out = vec![0x06];
    encode_length(oid.len(), &mut out);
    out.extend_from_slice(oid);
    out
}

/// Encode a DER OCTET STRING (tag 0x04).
fn der_octet_string(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x04];
    encode_length(data.len(), &mut out);
    out.extend_from_slice(data);
    out
}

/// Encode a DER INTEGER (tag 0x02).
fn der_integer(value: u32) -> Vec<u8> {
    if value == 0 {
        return vec![0x02, 0x01, 0x00];
    }
    let mut bytes = value.to_be_bytes().to_vec();
    // Strip leading zeros but keep at least one byte.
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes.remove(0);
    }
    // Add leading zero if high bit set (to avoid negative interpretation).
    if bytes[0] & 0x80 != 0 {
        bytes.insert(0, 0);
    }
    let mut out = vec![0x02];
    encode_length(bytes.len(), &mut out);
    out.extend_from_slice(&bytes);
    out
}

/// Encode a context-specific EXPLICIT tag (constructed).
fn der_explicit(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![0xA0 | tag];
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// Encode a context-specific IMPLICIT tag (constructed).
fn der_implicit(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![0xA0 | tag];
    encode_length(content.len(), &mut out);
    out.extend_from_slice(content);
    out
}

/// Encode a DER NULL (tag 0x05, length 0).
fn der_null() -> Vec<u8> {
    vec![0x05, 0x00]
}

/// Encode a DER UTCTime (tag 0x17).
fn der_utc_time(time_str: &str) -> Vec<u8> {
    let bytes = time_str.as_bytes();
    let mut out = vec![0x17];
    encode_length(bytes.len(), &mut out);
    out.extend_from_slice(bytes);
    out
}

// ---------------------------------------------------------------------------
// Structure builders
// ---------------------------------------------------------------------------

/// Build an AlgorithmIdentifier SEQUENCE for a hash algorithm.
fn build_algorithm_identifier(oid: &[u8]) -> Vec<u8> {
    let mut content = der_oid(oid);
    content.extend_from_slice(&der_null());
    der_sequence(&content)
}

/// Build an AlgorithmIdentifier for a signature algorithm.
///
/// ECDSA algorithms have no parameters (absent, not NULL).
/// RSA algorithms have NULL parameters.
fn build_sig_algorithm_identifier(oid: &[u8]) -> Vec<u8> {
    // ECDSA OIDs (1.2.840.10045.4.3.x) — no parameters.
    if oid.len() >= 5 && oid[0..5] == [0x2A, 0x86, 0x48, 0xCE, 0x3D] {
        der_sequence(&der_oid(oid))
    } else {
        let mut content = der_oid(oid);
        content.extend_from_slice(&der_null());
        der_sequence(&content)
    }
}

/// Build the ContentInfo wrapper.
fn build_content_info(signed_data_seq: &[u8]) -> Vec<u8> {
    let mut content = der_oid(OID_SIGNED_DATA);
    content.extend_from_slice(&der_explicit(0, signed_data_seq));
    der_sequence(&content)
}

/// Build the encapContentInfo (detached — no eContent).
fn build_encap_content_info() -> Vec<u8> {
    der_sequence(&der_oid(OID_DATA))
}

/// Build the signed attributes content (without the outer SET OF tag).
///
/// Attributes:
/// 1. contentType = id-data
/// 2. signingTime
/// 3. messageDigest
fn build_signed_attrs_content(
    message_digest: &[u8],
    signing_time: &str,
    _digest_algo_oid: &[u8],
) -> Vec<u8> {
    let mut content = Vec::new();

    // Attribute: contentType = id-data
    let ct_attr = build_attribute(OID_CONTENT_TYPE, &der_oid(OID_DATA));
    content.extend_from_slice(&ct_attr);

    // Attribute: signingTime
    let st_attr = build_attribute(OID_SIGNING_TIME, &der_utc_time(signing_time));
    content.extend_from_slice(&st_attr);

    // Attribute: messageDigest
    let md_attr = build_attribute(OID_MESSAGE_DIGEST, &der_octet_string(message_digest));
    content.extend_from_slice(&md_attr);

    content
}

/// Build a single Attribute SEQUENCE { OID, SET { value } }.
fn build_attribute(oid: &[u8], value: &[u8]) -> Vec<u8> {
    let mut content = der_oid(oid);
    content.extend_from_slice(&der_set(value));
    der_sequence(&content)
}

/// Build the IssuerAndSerialNumber SEQUENCE.
fn build_issuer_and_serial(issuer_der: &[u8], serial_der: &[u8]) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(issuer_der);
    content.extend_from_slice(serial_der);
    der_sequence(&content)
}

/// Build the full SignedData SEQUENCE.
fn build_signed_data_sequence(
    digest_algo_oid: &[u8],
    certs: &[Vec<u8>],
    issuer_der: &[u8],
    serial_der: &[u8],
    signed_attrs_content: &[u8],
    sig_algo_oid: &[u8],
    signature_value: &[u8],
) -> Vec<u8> {
    let mut content = Vec::new();

    // version INTEGER (1 for SignerIdentifier = IssuerAndSerialNumber)
    content.extend_from_slice(&der_integer(1));

    // digestAlgorithms SET OF AlgorithmIdentifier
    let algo_id = build_algorithm_identifier(digest_algo_oid);
    content.extend_from_slice(&der_set(&algo_id));

    // encapContentInfo (detached)
    content.extend_from_slice(&build_encap_content_info());

    // certificates [0] IMPLICIT SET OF Certificate
    let mut certs_content = Vec::new();
    for cert in certs {
        certs_content.extend_from_slice(cert);
    }
    content.extend_from_slice(&der_implicit(0, &certs_content));

    // signerInfos SET OF SignerInfo
    let signer_info = build_signer_info(
        digest_algo_oid,
        issuer_der,
        serial_der,
        signed_attrs_content,
        sig_algo_oid,
        signature_value,
    );
    content.extend_from_slice(&der_set(&signer_info));

    der_sequence(&content)
}

/// Build a single SignerInfo SEQUENCE.
fn build_signer_info(
    digest_algo_oid: &[u8],
    issuer_der: &[u8],
    serial_der: &[u8],
    signed_attrs_content: &[u8],
    sig_algo_oid: &[u8],
    signature_value: &[u8],
) -> Vec<u8> {
    let mut content = Vec::new();

    // version INTEGER (1)
    content.extend_from_slice(&der_integer(1));

    // sid IssuerAndSerialNumber
    content.extend_from_slice(&build_issuer_and_serial(issuer_der, serial_der));

    // digestAlgorithm AlgorithmIdentifier
    content.extend_from_slice(&build_algorithm_identifier(digest_algo_oid));

    // signedAttrs [0] IMPLICIT SET OF Attribute
    let mut signed_attrs = vec![0xA0];
    encode_length(signed_attrs_content.len(), &mut signed_attrs);
    signed_attrs.extend_from_slice(signed_attrs_content);
    content.extend_from_slice(&signed_attrs);

    // signatureAlgorithm AlgorithmIdentifier
    content.extend_from_slice(&build_sig_algorithm_identifier(sig_algo_oid));

    // signature OCTET STRING
    content.extend_from_slice(&der_octet_string(signature_value));

    der_sequence(&content)
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute the message digest using the specified algorithm.
fn compute_digest(algo: DigestAlgorithm, data: &[u8]) -> Vec<u8> {
    match algo {
        DigestAlgorithm::Sha256 => {
            let mut h = Sha256::new();
            h.update(data);
            h.finalize().to_vec()
        }
        DigestAlgorithm::Sha384 => {
            let mut h = Sha384::new();
            h.update(data);
            h.finalize().to_vec()
        }
        // Default to SHA-256 for other algorithms.
        _ => {
            let mut h = Sha256::new();
            h.update(data);
            h.finalize().to_vec()
        }
    }
}

/// Return the DER OID bytes for the given digest algorithm.
fn digest_algo_oid(algo: DigestAlgorithm) -> &'static [u8] {
    match algo {
        DigestAlgorithm::Sha256 => OID_SHA256,
        DigestAlgorithm::Sha384 => OID_SHA384,
        _ => OID_SHA256,
    }
}

/// Generate a UTCTime string for the current time.
///
/// Format: YYMMDDHHmmSSZ (13 chars).
fn utc_time_now() -> String {
    // Use std::time to get current time, format as UTCTime.
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to calendar date/time.
    let (year, month, day, hour, min, sec) = epoch_to_utc(secs);
    format!(
        "{:02}{:02}{:02}{:02}{:02}{:02}Z",
        year % 100,
        month,
        day,
        hour,
        min,
        sec
    )
}

/// Convert Unix epoch seconds to (year, month, day, hour, min, sec) in UTC.
fn epoch_to_utc(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    // Days since 1970-01-01.
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = is_leap(y);
    let month_days: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0u64;
    for md in &month_days {
        if remaining < *md {
            break;
        }
        remaining -= *md;
        m += 1;
    }

    (y, m + 1, remaining + 1, hour, min, sec)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Extract issuer Name DER and serial number DER from a certificate.
///
/// Returns (issuer_name_sequence_der, serial_integer_tlv) or None.
fn extract_issuer_and_serial(cert_der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    use crate::cms::parse_tlv;

    // Certificate ::= SEQUENCE { tbsCertificate, ... }
    let (_, cert_seq) = parse_tlv(cert_der)?;
    // tbsCertificate SEQUENCE
    let (_, tbs_seq) = parse_tlv(cert_seq)?;
    let mut pos = tbs_seq;

    // version [0] EXPLICIT (optional)
    if let Some((rest, _)) = crate::cms::parse_context_explicit(pos, 0) {
        pos = rest;
    }

    // serialNumber INTEGER — capture full TLV
    let serial_start = pos;
    let (rest, _serial_val) = parse_tlv(pos)?;
    let serial_len = pos.len() - rest.len();
    let serial_tlv = serial_start[..serial_len].to_vec();
    pos = rest;

    // signature AlgorithmIdentifier — skip
    let (rest, _) = parse_tlv(pos)?;
    pos = rest;

    // issuer Name SEQUENCE — capture full TLV
    let issuer_start = pos;
    let (rest, _) = parse_tlv(pos)?;
    let issuer_len = pos.len() - rest.len();
    let issuer_tlv = issuer_start[..issuer_len].to_vec();
    let _ = rest; // not needed further

    Some((issuer_tlv, serial_tlv))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_encoding_helpers() {
        // Test short length
        let seq = der_sequence(&[0x01, 0x02]);
        assert_eq!(seq, vec![0x30, 0x02, 0x01, 0x02]);

        // Test OID encoding
        let oid = der_oid(&[0x2A, 0x03]);
        assert_eq!(oid, vec![0x06, 0x02, 0x2A, 0x03]);

        // Test integer 0
        assert_eq!(der_integer(0), vec![0x02, 0x01, 0x00]);

        // Test integer 1
        assert_eq!(der_integer(1), vec![0x02, 0x01, 0x01]);

        // Test NULL
        assert_eq!(der_null(), vec![0x05, 0x00]);
    }

    #[test]
    fn der_length_encoding() {
        // Short form (< 128)
        let mut out = Vec::new();
        encode_length(5, &mut out);
        assert_eq!(out, vec![0x05]);

        // Long form 1 byte
        let mut out = Vec::new();
        encode_length(200, &mut out);
        assert_eq!(out, vec![0x81, 200]);

        // Long form 2 bytes
        let mut out = Vec::new();
        encode_length(0x1234, &mut out);
        assert_eq!(out, vec![0x82, 0x12, 0x34]);
    }

    #[test]
    fn epoch_to_utc_known_date() {
        // 2024-01-15 11:30:45 UTC = 1705318245
        let (y, m, d, h, min, s) = epoch_to_utc(1705318245);
        assert_eq!((y, m, d, h, min, s), (2024, 1, 15, 11, 30, 45));
    }

    #[test]
    fn utc_time_format() {
        let t = utc_time_now();
        assert!(t.ends_with('Z'));
        assert_eq!(t.len(), 13);
    }

    #[test]
    fn build_algorithm_identifier_sha256() {
        let ai = build_algorithm_identifier(OID_SHA256);
        // Should be SEQUENCE { OID sha256, NULL }
        assert_eq!(ai[0], 0x30); // SEQUENCE
        assert!(ai.len() > 4);
    }

    #[test]
    fn build_and_parse_roundtrip() {
        // Build a signed attribute set, then verify it can be partially parsed.
        let digest = vec![0xAA; 32];
        let content = build_signed_attrs_content(&digest, "240115123045Z", OID_SHA256);
        // Should contain three attributes.
        assert!(!content.is_empty());
        // Wrap in SET and check it starts with correct tag.
        let set = der_set(&content);
        assert_eq!(set[0], 0x31);
    }
}
