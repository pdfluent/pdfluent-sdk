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
    /// Encapsulated content bytes (e.g. SHA-1 digest for adbe.pkcs7.sha1).
    encapsulated_content: Option<Vec<u8>>,
    /// Signing time from signed attributes, if present.
    signing_time: Option<String>,
    /// Signer common name extracted from signer info.
    signer_cn: Option<String>,
    /// The signed attributes bytes (for signature verification).
    signed_attrs_raw: Option<Vec<u8>>,
    /// The signature algorithm OID from SignerInfo.
    sig_algo_oid: Vec<u8>,
    /// The signature algorithm parameters (raw DER, needed for RSA-PSS).
    sig_algo_params: Vec<u8>,
    /// Index of the signer certificate in the certificates array.
    signer_cert_index: usize,
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

        // encapContentInfo SEQUENCE { contentType OID, content [0] EXPLICIT OCTET STRING (opt) }
        let (rest, encap_seq) = parse_tlv(pos)?;
        let encapsulated_content = parse_encap_content(encap_seq);
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
            encapsulated_content,
            signing_time: si.signing_time,
            signer_cn: si.signer_cn,
            signed_attrs_raw: si.signed_attrs_raw,
            sig_algo_oid: si.sig_algo_oid,
            sig_algo_params: si.sig_algo_params,
            signer_cert_index: si.signer_cert_index,
        })
    }

    /// Return the digest algorithm.
    pub fn digest_algorithm(&self) -> DigestAlgorithm {
        self.digest_algo
    }

    /// Return the embedded message digest from signed attributes.
    pub fn message_digest(&self) -> Option<&[u8]> {
        self.message_digest.as_deref()
    }

    /// Return the encapsulated content (e.g. SHA-1 digest for adbe.pkcs7.sha1).
    pub fn encapsulated_content(&self) -> Option<&[u8]> {
        self.encapsulated_content.as_deref()
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

    /// Check structural integrity of the CMS signature.
    ///
    /// Returns `true` if the signature bytes exist, the digest algorithm
    /// is known, and a message digest attribute is present.
    ///
    /// **Note:** This does NOT perform cryptographic verification
    /// (RSA/ECDSA over signed attributes). Use an external crypto
    /// provider for full signature verification.
    pub fn verify_structural_integrity(&self) -> bool {
        !self.signature_value.is_empty()
            && self.digest_algo != DigestAlgorithm::Unknown
            && (self.message_digest.is_some() || self.encapsulated_content.is_some())
    }

    /// Return the raw signature value bytes.
    pub fn signature_value(&self) -> &[u8] {
        &self.signature_value
    }

    /// Return the raw signed attributes bytes (for external verification).
    pub fn signed_attributes_raw(&self) -> Option<&[u8]> {
        self.signed_attrs_raw.as_deref()
    }

    /// Return the signature algorithm OID from the SignerInfo.
    pub fn signature_algorithm_oid(&self) -> &[u8] {
        &self.sig_algo_oid
    }

    /// Return the signature algorithm parameters (raw DER).
    ///
    /// For RSA-PSS this contains the RSASSA-PSS-params SEQUENCE that
    /// specifies the hash algorithm. Empty for most other algorithms.
    pub fn signature_algorithm_params(&self) -> &[u8] {
        &self.sig_algo_params
    }

    /// Return the index of the signer certificate in the certificates array.
    ///
    /// Resolved from the SignerIdentifier (SID) by matching the serial number.
    pub fn signer_cert_index(&self) -> usize {
        self.signer_cert_index
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

/// Parse encapsulated content from EncapsulatedContentInfo.
/// Returns the raw content bytes if present (used by adbe.pkcs7.sha1).
fn parse_encap_content(data: &[u8]) -> Option<Vec<u8>> {
    // EncapsulatedContentInfo ::= SEQUENCE { contentType OID, eContent [0] EXPLICIT OCTET STRING }
    let (rest, _content_type) = parse_tlv(data)?;
    // eContent [0] EXPLICIT is optional
    let (_, inner) = parse_context_explicit(rest, 0)?;
    let (_, content_bytes) = parse_tlv(inner)?;
    Some(content_bytes.to_vec())
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
    sig_algo_oid: Vec<u8>,
    sig_algo_params: Vec<u8>,
    /// Index into the certificates array identifying the signer cert.
    signer_cert_index: usize,
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

    // sid (IssuerAndSerialNumber SEQUENCE or SubjectKeyIdentifier [0])
    let (rest, sid) = parse_tlv(pos)?;
    // Try to match SID to a certificate.
    let signer_cert_index = find_cert_by_sid(sid, certs).unwrap_or(0);
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
    let (rest, sig_algo_data) = parse_tlv(pos)?;
    let (sig_algo_oid, sig_algo_params) = if let Some((params_rest, oid)) = parse_tlv(sig_algo_data)
    {
        // Remaining bytes in AlgorithmIdentifier after the OID are the parameters.
        (oid.to_vec(), params_rest.to_vec())
    } else {
        (Vec::new(), Vec::new())
    };
    pos = rest;

    // signature OCTET STRING
    let (_, sig_value) = parse_tlv(pos)?;
    let signature_value = sig_value.to_vec();

    // Resolve signer CN from the matched certificate.
    let signer_cn = certs
        .get(signer_cert_index)
        .and_then(|c| c.subject_common_name());

    Some(SignerInfoFields {
        signature_value,
        message_digest,
        signing_time,
        signer_cn,
        signed_attrs_raw,
        sig_algo_oid,
        sig_algo_params,
        signer_cert_index,
    })
}

/// Match a SignerIdentifier (SID) to a certificate in the set.
///
/// The SID is either an IssuerAndSerialNumber SEQUENCE or a
/// SubjectKeyIdentifier. We try to match by extracting the serial
/// number from IssuerAndSerialNumber and comparing to each cert's
/// serial in TBS.
fn find_cert_by_sid(sid: &[u8], certs: &[X509Certificate]) -> Option<usize> {
    if certs.len() <= 1 {
        return Some(0);
    }
    // IssuerAndSerialNumber is a SEQUENCE containing issuer Name + serial INTEGER.
    // Extract the serial (last element in the SEQUENCE).
    let mut pos = sid;
    let mut last_value = &[][..];
    while !pos.is_empty() {
        if let Some((rest, val)) = parse_tlv(pos) {
            last_value = val;
            pos = rest;
        } else {
            break;
        }
    }
    if last_value.is_empty() {
        return None;
    }
    // Compare serial to each certificate's serial number.
    for (i, cert) in certs.iter().enumerate() {
        if let Some(cert_serial) = extract_cert_serial(cert) {
            if cert_serial == last_value {
                return Some(i);
            }
        }
    }
    None
}

/// Extract serial number from a certificate's TBS for SID matching.
fn extract_cert_serial(cert: &X509Certificate) -> Option<&[u8]> {
    let (_, tbs_seq) = parse_tlv(&cert.tbs_raw)?;
    let mut pos = tbs_seq;
    // Skip optional version [0] EXPLICIT.
    if let Some((rest, _)) = parse_context_explicit(pos, 0) {
        pos = rest;
    }
    // Next is the serial number INTEGER.
    let (_, serial) = parse_tlv(pos)?;
    Some(serial)
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
