//! CMS (Cryptographic Message Syntax) / PKCS#7 parsing.
//!
//! Parses DER-encoded CMS SignedData structures embedded in PDF
//! signature /Contents values per RFC 5652.

use crate::byte_range::DigestAlgorithm;
use crate::x509::X509Certificate;

/// A parsed CMS SignedData structure (RFC 5652 §5.1).
#[derive(Debug, Clone)]
pub struct CmsSignedData {
    /// The digest algorithm used.
    digest_algo: DigestAlgorithm,
    /// Embedded certificates.
    certificates: Vec<X509Certificate>,
    /// The encrypted digest (signature value).
    signature_value: Vec<u8>,
    /// The message digest from signed attributes.
    message_digest: Option<Vec<u8>>,
    /// Signing time from signed attributes, if present.
    signing_time: Option<String>,
    /// Signer common name extracted from signer info.
    signer_cn: Option<String>,
    /// The signed attributes bytes (for signature verification).
    signed_attrs_raw: Option<Vec<u8>>,
}

impl CmsSignedData {
    /// Parse a DER-encoded CMS ContentInfo / SignedData.
    pub fn from_der(data: &[u8]) -> Option<Self> {
        // ContentInfo ::= SEQUENCE { contentType, content [0] EXPLICIT }
        let (_, content_info) = parse_tlv(data)?;
        let (rest, _content_type_oid) = parse_tlv(content_info)?;
        // content [0] EXPLICIT
        let (_, explicit) = parse_context_explicit(rest, 0)?;
        // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo,
        //                           certificates [0] IMPLICIT, signerInfos }
        let (_, signed_data_seq) = parse_tlv(explicit)?;
        Self::parse_signed_data(signed_data_seq)
    }

    fn parse_signed_data(data: &[u8]) -> Option<Self> {
        let mut pos = data;

        // version INTEGER
        let (rest, _version) = parse_tlv(pos)?;
        pos = rest;

        // digestAlgorithms SET OF AlgorithmIdentifier
        let (rest, digest_algos_set) = parse_tlv(pos)?;
        let digest_algo =
            parse_algorithm_identifier(digest_algos_set).unwrap_or(DigestAlgorithm::Unknown);
        pos = rest;

        // encapContentInfo SEQUENCE
        let (rest, _encap) = parse_tlv(pos)?;
        pos = rest;

        // certificates [0] IMPLICIT SET OF Certificate (optional)
        let mut certificates = Vec::new();
        if let Some((rest2, certs_data)) = parse_context_implicit(pos, 0) {
            let mut cpos = certs_data;
            while !cpos.is_empty() {
                if let Some((next, cert_bytes)) = parse_tlv_raw(cpos) {
                    if let Some(cert) = X509Certificate::from_der(cert_bytes) {
                        certificates.push(cert);
                    }
                    cpos = next;
                } else {
                    break;
                }
            }
            pos = rest2;
        }

        // crls [1] IMPLICIT (optional, skip)
        if let Some((rest2, _)) = parse_context_implicit(pos, 1) {
            pos = rest2;
        }

        // signerInfos SET OF SignerInfo
        let (_, signer_infos_set) = parse_tlv(pos)?;
        let si = parse_signer_info(signer_infos_set, &certificates)?;

        Some(Self {
            digest_algo,
            certificates,
            signature_value: si.signature_value,
            message_digest: si.message_digest,
            signing_time: si.signing_time,
            signer_cn: si.signer_cn,
            signed_attrs_raw: si.signed_attrs_raw,
        })
    }

    /// Return the digest algorithm.
    pub fn digest_algorithm(&self) -> DigestAlgorithm {
        self.digest_algo
    }

    /// Return the embedded message digest.
    pub fn message_digest(&self) -> Option<&[u8]> {
        self.message_digest.as_deref()
    }

    /// Return embedded certificates.
    pub fn certificates(&self) -> &[X509Certificate] {
        &self.certificates
    }

    /// Return the signer's common name.
    pub fn signer_common_name(&self) -> Option<String> {
        self.signer_cn.clone()
    }

    /// Return the signing time string.
    pub fn signing_time(&self) -> Option<&str> {
        self.signing_time.as_deref()
    }

    /// Verify the integrity of the CMS signature.
    ///
    /// This checks that the signature value is structurally valid
    /// (i.e., non-empty and from a known algorithm). Full RSA/ECDSA
    /// verification requires an external crypto provider.
    pub fn verify_signature_integrity(&self) -> bool {
        // We verify structural integrity: signature bytes exist,
        // digest algorithm is known, and message digest is present.
        !self.signature_value.is_empty()
            && self.digest_algo != DigestAlgorithm::Unknown
            && self.message_digest.is_some()
    }

    /// Return the raw signature value bytes.
    pub fn signature_value(&self) -> &[u8] {
        &self.signature_value
    }

    /// Return the raw signed attributes bytes (for external verification).
    pub fn signed_attributes_raw(&self) -> Option<&[u8]> {
        self.signed_attrs_raw.as_deref()
    }
}

// ---------------------------------------------------------------------------
// Minimal BER/DER parser (just enough for CMS structures)
// ---------------------------------------------------------------------------

/// Parse a TLV (Tag-Length-Value) and return (rest, value).
pub(crate) fn parse_tlv(data: &[u8]) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let (_, rest) = parse_tag(data)?;
    let (len, rest) = parse_length(rest)?;
    if rest.len() < len {
        return None;
    }
    Some((&rest[len..], &rest[..len]))
}

/// Parse a TLV and return (rest, full_tlv_bytes_including_header).
pub(crate) fn parse_tlv_raw(data: &[u8]) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let start = data;
    let (_, rest) = parse_tag(data)?;
    let (len, rest) = parse_length(rest)?;
    if rest.len() < len {
        return None;
    }
    let header_len = start.len() - rest.len();
    let total = header_len + len;
    Some((&start[total..], &start[..total]))
}

/// Parse a context-specific EXPLICIT tagged value [tag].
pub(crate) fn parse_context_explicit(data: &[u8], expected_tag: u8) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    // Context-specific, constructed: 0xA0 | tag
    if tag != (0xA0 | expected_tag) {
        return None;
    }
    let (len, rest) = parse_length(&data[1..])?;
    if rest.len() < len {
        return None;
    }
    Some((&rest[len..], &rest[..len]))
}

/// Parse a context-specific IMPLICIT tagged value [tag].
pub(crate) fn parse_context_implicit(data: &[u8], expected_tag: u8) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    // Context-specific: 0xA0 | tag (constructed) or 0x80 | tag (primitive)
    if tag != (0xA0 | expected_tag) && tag != (0x80 | expected_tag) {
        return None;
    }
    let (len, rest) = parse_length(&data[1..])?;
    if rest.len() < len {
        return None;
    }
    Some((&rest[len..], &rest[..len]))
}

pub(crate) fn parse_tag(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    // Handle high-tag-number form (tags >= 31).
    if tag & 0x1F == 0x1F {
        let mut i = 1;
        while i < data.len() {
            if data[i] & 0x80 == 0 {
                return Some((tag, &data[i + 1..]));
            }
            i += 1;
        }
        None
    } else {
        Some((tag, &data[1..]))
    }
}

pub(crate) fn parse_length(data: &[u8]) -> Option<(usize, &[u8])> {
    if data.is_empty() {
        return None;
    }
    let first = data[0] as usize;
    if first < 0x80 {
        Some((first, &data[1..]))
    } else if first == 0x80 {
        // Indefinite length — not supported in DER.
        None
    } else {
        let num_bytes = first & 0x7F;
        if num_bytes > 4 || data.len() < 1 + num_bytes {
            return None;
        }
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | (data[1 + i] as usize);
        }
        Some((len, &data[1 + num_bytes..]))
    }
}

/// Parse an AlgorithmIdentifier SEQUENCE and return the digest algorithm.
fn parse_algorithm_identifier(data: &[u8]) -> Option<DigestAlgorithm> {
    // SET OF { SEQUENCE { OID, params } }
    // We take the first algorithm.
    let (_, algo_seq) = parse_tlv(data)?;
    let (_, oid_bytes) = parse_tlv(algo_seq)?;
    Some(DigestAlgorithm::from_oid(oid_bytes))
}

struct SignerInfoFields {
    signature_value: Vec<u8>,
    message_digest: Option<Vec<u8>>,
    signing_time: Option<String>,
    signer_cn: Option<String>,
    signed_attrs_raw: Option<Vec<u8>>,
}

/// Parse a SignerInfo and extract relevant fields.
fn parse_signer_info(data: &[u8], certs: &[X509Certificate]) -> Option<SignerInfoFields> {
    // SignerInfo ::= SEQUENCE { version, sid, digestAlgorithm,
    //                           signedAttrs [0] IMPLICIT, signatureAlgorithm,
    //                           signature, unsignedAttrs [1] IMPLICIT }
    let (_, signer_info) = parse_tlv(data)?;
    let mut pos = signer_info;

    // version INTEGER
    let (rest, _version) = parse_tlv(pos)?;
    pos = rest;

    // sid (IssuerAndSerialNumber or SubjectKeyIdentifier)
    let (rest, _sid) = parse_tlv(pos)?;
    pos = rest;

    // digestAlgorithm AlgorithmIdentifier
    let (rest, _digest_algo) = parse_tlv(pos)?;
    pos = rest;

    // signedAttrs [0] IMPLICIT SET OF Attribute (optional)
    let mut message_digest = None;
    let mut signing_time = None;
    let mut signed_attrs_raw = None;

    if let Some((rest2, attrs_data)) = parse_context_implicit(pos, 0) {
        // Save the raw signed attrs for verification (re-encode with SET OF tag).
        let attrs_start = pos;
        let attrs_total_len = pos.len() - rest2.len();
        let mut raw = Vec::with_capacity(attrs_total_len);
        // Replace the context tag [0] with SET OF (0x31).
        raw.push(0x31);
        raw.extend_from_slice(&attrs_start[1..attrs_total_len]);
        signed_attrs_raw = Some(raw);

        // Parse individual attributes.
        let mut apos = attrs_data;
        while !apos.is_empty() {
            if let Some((next, attr_seq)) = parse_tlv(apos) {
                parse_signed_attr(attr_seq, &mut message_digest, &mut signing_time);
                apos = next;
            } else {
                break;
            }
        }
        pos = rest2;
    }

    // signatureAlgorithm AlgorithmIdentifier
    let (rest, _sig_algo) = parse_tlv(pos)?;
    pos = rest;

    // signature OCTET STRING
    let (_, sig_value) = parse_tlv(pos)?;
    let signature_value = sig_value.to_vec();

    // Try to find the signer's CN from certificates.
    let signer_cn = certs.first().and_then(|c| c.subject_common_name());

    Some(SignerInfoFields {
        signature_value,
        message_digest,
        signing_time,
        signer_cn,
        signed_attrs_raw,
    })
}

// OID for messageDigest: 1.2.840.113549.1.9.4
const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];
// OID for signingTime: 1.2.840.113549.1.9.5
const OID_SIGNING_TIME: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];

fn parse_signed_attr(
    attr_data: &[u8],
    message_digest: &mut Option<Vec<u8>>,
    signing_time: &mut Option<String>,
) {
    // Attribute ::= SEQUENCE { attrType OID, attrValues SET OF }
    let (rest, oid_bytes) = match parse_tlv(attr_data) {
        Some(v) => v,
        None => return,
    };
    let (_, values_set) = match parse_tlv(rest) {
        Some(v) => v,
        None => return,
    };

    if oid_bytes == OID_MESSAGE_DIGEST {
        // SET { OCTET STRING }
        if let Some((_, digest_bytes)) = parse_tlv(values_set) {
            *message_digest = Some(digest_bytes.to_vec());
        }
    } else if oid_bytes == OID_SIGNING_TIME {
        // SET { UTCTime or GeneralizedTime }
        if let Some((_, time_bytes)) = parse_tlv(values_set) {
            if let Ok(s) = core::str::from_utf8(time_bytes) {
                *signing_time = Some(s.to_string());
            }
        }
    }
}
