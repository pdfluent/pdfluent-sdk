//! X.509 certificate parsing (RFC 5280).
//!
//! Minimal parser for extracting certificate fields relevant to
//! PDF signature validation: subject, issuer, validity, key usage.

use crate::cms::{parse_context_explicit, parse_tlv};

/// A parsed X.509 certificate.
#[derive(Debug, Clone)]
pub struct X509Certificate {
    /// DER-encoded TBSCertificate (for signature verification).
    pub tbs_raw: Vec<u8>,
    /// Subject distinguished name components.
    pub subject: Vec<RdnAttribute>,
    /// Issuer distinguished name components.
    pub issuer: Vec<RdnAttribute>,
    /// Not-before validity date string.
    pub not_before: Option<String>,
    /// Not-after validity date string.
    pub not_after: Option<String>,
    /// Whether this is a CA certificate (from BasicConstraints).
    pub is_ca: bool,
    /// Key usage bits, if present.
    pub key_usage: Option<u16>,
    /// Whether the certificate is self-signed (subject == issuer).
    pub is_self_signed: bool,
    /// The signature algorithm OID.
    pub signature_algorithm: Vec<u8>,
    /// The signature value bytes.
    pub signature_value: Vec<u8>,
    /// The raw DER of the full certificate.
    pub raw: Vec<u8>,
}

/// A relative distinguished name attribute (e.g., CN, O, OU).
#[derive(Debug, Clone, PartialEq)]
pub struct RdnAttribute {
    /// OID of the attribute type.
    pub oid: Vec<u8>,
    /// String value.
    pub value: String,
}

// Common OIDs.
const OID_CN: &[u8] = &[0x55, 0x04, 0x03]; // 2.5.4.3 — Common Name
const OID_O: &[u8] = &[0x55, 0x04, 0x0A]; // 2.5.4.10 — Organization

impl X509Certificate {
    /// Parse a DER-encoded X.509 certificate.
    pub fn from_der(data: &[u8]) -> Option<Self> {
        // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
        let (_, cert_seq) = parse_tlv(data)?;
        let mut pos = cert_seq;

        // tbsCertificate SEQUENCE
        let (rest, tbs_data) = parse_tlv_with_header(pos)?;
        let tbs_raw = tbs_data.to_vec();
        pos = rest;

        // signatureAlgorithm AlgorithmIdentifier
        let (rest, sig_algo_seq) = parse_tlv(pos)?;
        let signature_algorithm = parse_oid_from_algo_id(sig_algo_seq);
        pos = rest;

        // signatureValue BIT STRING
        let (_, sig_bits) = parse_tlv(pos)?;
        // Skip the unused-bits byte.
        let signature_value = if sig_bits.is_empty() {
            Vec::new()
        } else {
            sig_bits[1..].to_vec()
        };

        // Parse TBSCertificate fields.
        let (_, tbs_seq) = parse_tlv(&tbs_raw)?;
        let tbs = parse_tbs(tbs_seq)?;

        let is_self_signed = tbs.subject == tbs.issuer;

        Some(Self {
            tbs_raw,
            subject: tbs.subject,
            issuer: tbs.issuer,
            not_before: tbs.not_before,
            not_after: tbs.not_after,
            is_ca: tbs.is_ca,
            key_usage: tbs.key_usage,
            is_self_signed,
            signature_algorithm,
            signature_value,
            raw: data.to_vec(),
        })
    }

    /// Return the subject Common Name, if present.
    pub fn subject_common_name(&self) -> Option<String> {
        self.subject
            .iter()
            .find(|a| a.oid == OID_CN)
            .map(|a| a.value.clone())
    }

    /// Return the issuer Common Name, if present.
    pub fn issuer_common_name(&self) -> Option<String> {
        self.issuer
            .iter()
            .find(|a| a.oid == OID_CN)
            .map(|a| a.value.clone())
    }

    /// Return the subject Organization, if present.
    pub fn subject_organization(&self) -> Option<String> {
        self.subject
            .iter()
            .find(|a| a.oid == OID_O)
            .map(|a| a.value.clone())
    }

    /// Check if this certificate has the digital signature key usage bit set.
    pub fn has_digital_signature_usage(&self) -> bool {
        self.key_usage.is_none_or(|ku| ku & 0x80 != 0)
    }

    /// Check if this certificate has the key cert sign key usage bit set.
    pub fn has_key_cert_sign_usage(&self) -> bool {
        self.key_usage.is_some_and(|ku| ku & 0x04 != 0)
    }
}

/// Parse a TLV and return (rest, full_bytes_including_header).
fn parse_tlv_with_header(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let (rest, _value) = parse_tlv(data)?;
    let header_and_value_len = data.len() - rest.len();
    Some((rest, &data[..header_and_value_len]))
}

fn parse_oid_from_algo_id(data: &[u8]) -> Vec<u8> {
    parse_tlv(data)
        .map(|(_, oid)| oid.to_vec())
        .unwrap_or_default()
}

struct TbsFields {
    subject: Vec<RdnAttribute>,
    issuer: Vec<RdnAttribute>,
    not_before: Option<String>,
    not_after: Option<String>,
    is_ca: bool,
    key_usage: Option<u16>,
}

fn parse_tbs(data: &[u8]) -> Option<TbsFields> {
    let mut pos = data;

    // version [0] EXPLICIT INTEGER (optional)
    if let Some((rest, _)) = parse_context_explicit(pos, 0) {
        pos = rest;
    }

    // serialNumber INTEGER
    let (rest, _serial) = parse_tlv(pos)?;
    pos = rest;

    // signature AlgorithmIdentifier
    let (rest, _sig_algo) = parse_tlv(pos)?;
    pos = rest;

    // issuer Name
    let (rest, issuer_data) = parse_tlv(pos)?;
    let issuer = parse_name(issuer_data);
    pos = rest;

    // validity SEQUENCE { notBefore, notAfter }
    let (rest, validity) = parse_tlv(pos)?;
    let (not_before, not_after) = parse_validity(validity);
    pos = rest;

    // subject Name
    let (rest, subject_data) = parse_tlv(pos)?;
    let subject = parse_name(subject_data);
    pos = rest;

    // subjectPublicKeyInfo SEQUENCE
    let (rest, _spki) = parse_tlv(pos)?;
    pos = rest;

    // Extensions [3] EXPLICIT (optional)
    let mut is_ca = false;
    let mut key_usage = None;

    // Skip issuerUniqueID [1] and subjectUniqueID [2] if present.
    if let Some((rest2, _)) = parse_context_implicit_raw(pos, 1) {
        pos = rest2;
    }
    if let Some((rest2, _)) = parse_context_implicit_raw(pos, 2) {
        pos = rest2;
    }

    if let Some((_, exts_data)) = parse_context_explicit(pos, 3) {
        // SEQUENCE OF Extension
        if let Some((_, exts_seq)) = parse_tlv(exts_data) {
            parse_extensions(exts_seq, &mut is_ca, &mut key_usage);
        }
    }

    Some(TbsFields {
        subject,
        issuer,
        not_before,
        not_after,
        is_ca,
        key_usage,
    })
}

fn parse_name(data: &[u8]) -> Vec<RdnAttribute> {
    let mut attrs = Vec::new();
    let mut pos = data;
    while !pos.is_empty() {
        if let Some((rest, rdn_set)) = parse_tlv(pos) {
            if let Some((_, attr_seq)) = parse_tlv(rdn_set) {
                if let Some((rest2, oid)) = parse_tlv(attr_seq) {
                    if let Some((_, value_bytes)) = parse_tlv(rest2) {
                        let value = String::from_utf8_lossy(value_bytes).to_string();
                        attrs.push(RdnAttribute {
                            oid: oid.to_vec(),
                            value,
                        });
                    }
                }
            }
            pos = rest;
        } else {
            break;
        }
    }
    attrs
}

fn parse_validity(data: &[u8]) -> (Option<String>, Option<String>) {
    let mut pos = data;
    let not_before = parse_tlv(pos).and_then(|(rest, val)| {
        pos = rest;
        core::str::from_utf8(val).ok().map(String::from)
    });
    let not_after =
        parse_tlv(pos).and_then(|(_, val)| core::str::from_utf8(val).ok().map(String::from));
    (not_before, not_after)
}

// OID for BasicConstraints: 2.5.29.19
const OID_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1D, 0x13];
// OID for KeyUsage: 2.5.29.15
const OID_KEY_USAGE: &[u8] = &[0x55, 0x1D, 0x0F];

fn parse_extensions(data: &[u8], is_ca: &mut bool, key_usage: &mut Option<u16>) {
    let mut pos = data;
    while !pos.is_empty() {
        if let Some((rest, ext_seq)) = parse_tlv(pos) {
            parse_single_extension(ext_seq, is_ca, key_usage);
            pos = rest;
        } else {
            break;
        }
    }
}

fn parse_single_extension(data: &[u8], is_ca: &mut bool, key_usage: &mut Option<u16>) {
    // Extension ::= SEQUENCE { extnID OID, critical BOOLEAN (opt), extnValue OCTET STRING }
    let (rest, oid) = match parse_tlv(data) {
        Some(v) => v,
        None => return,
    };

    // Skip optional critical BOOLEAN.
    let mut pos = rest;
    if !pos.is_empty() && pos[0] == 0x01 {
        // BOOLEAN tag
        if let Some((rest2, _)) = parse_tlv(pos) {
            pos = rest2;
        }
    }

    // extnValue OCTET STRING
    let (_, ext_value) = match parse_tlv(pos) {
        Some(v) => v,
        None => return,
    };

    if oid == OID_BASIC_CONSTRAINTS {
        // BasicConstraints ::= SEQUENCE { cA BOOLEAN, pathLen INTEGER (opt) }
        if let Some((_, bc_seq)) = parse_tlv(ext_value) {
            if let Some((_, bool_val)) = parse_tlv(bc_seq) {
                if !bool_val.is_empty() && bool_val[0] != 0 {
                    *is_ca = true;
                }
            }
        }
    } else if oid == OID_KEY_USAGE {
        // KeyUsage ::= BIT STRING
        if let Some((_, bits)) = parse_tlv(ext_value) {
            if bits.len() >= 2 {
                let unused = bits[0];
                let usage = (bits[1] as u16) >> unused;
                *key_usage = Some(usage);
            } else if bits.len() == 2 {
                *key_usage = Some(bits[1] as u16);
            }
        }
    }
}

fn parse_context_implicit_raw(data: &[u8], expected_tag: u8) -> Option<(&[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    if tag != (0x80 | expected_tag) && tag != (0xA0 | expected_tag) {
        return None;
    }
    let (len, rest) = crate::cms::parse_length(&data[1..])?;
    if rest.len() < len {
        return None;
    }
    Some((&rest[len..], &rest[..len]))
}
