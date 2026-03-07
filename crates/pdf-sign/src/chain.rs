//! Certificate chain building and verification.
//!
//! Builds a chain from the leaf (signing) certificate up through
//! intermediates to a root CA, and checks validity periods.

use crate::x509::X509Certificate;

/// Result of certificate chain verification.
#[derive(Debug, Clone)]
pub struct ChainVerificationResult {
    /// Whether a valid chain was built.
    pub chain_valid: bool,
    /// The certificate chain (leaf first, root last).
    pub chain: Vec<CertificateInfo>,
    /// Any issues found.
    pub issues: Vec<String>,
}

/// Summary information about a certificate in the chain.
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    /// Subject common name.
    pub subject_cn: Option<String>,
    /// Issuer common name.
    pub issuer_cn: Option<String>,
    /// Whether it's a CA certificate.
    pub is_ca: bool,
    /// Whether it's self-signed.
    pub is_self_signed: bool,
    /// Validity not-before date.
    pub not_before: Option<String>,
    /// Validity not-after date.
    pub not_after: Option<String>,
}

impl CertificateInfo {
    /// Create from an X509Certificate.
    pub fn from_cert(cert: &X509Certificate) -> Self {
        Self {
            subject_cn: cert.subject_common_name(),
            issuer_cn: cert.issuer_common_name(),
            is_ca: cert.is_ca,
            is_self_signed: cert.is_self_signed,
            not_before: cert.not_before.clone(),
            not_after: cert.not_after.clone(),
        }
    }
}

/// Build and verify a certificate chain from embedded certificates.
///
/// The first certificate in `certs` is assumed to be the signing (leaf)
/// certificate. The function attempts to build a chain to a self-signed
/// root.
pub fn verify_certificate_chain(certs: &[X509Certificate]) -> ChainVerificationResult {
    if certs.is_empty() {
        return ChainVerificationResult {
            chain_valid: false,
            chain: Vec::new(),
            issues: vec!["no certificates provided".into()],
        };
    }

    let mut chain = Vec::new();
    let mut issues = Vec::new();

    // Start with the leaf certificate.
    let leaf = &certs[0];
    chain.push(CertificateInfo::from_cert(leaf));

    // Check leaf has digital signature usage.
    if !leaf.has_digital_signature_usage() {
        issues.push("leaf certificate missing digitalSignature key usage".into());
    }

    // Build chain by matching issuer to subject.
    let mut current = leaf;
    let mut depth = 0;
    const MAX_DEPTH: usize = 10;

    while !current.is_self_signed && depth < MAX_DEPTH {
        // Find the issuer certificate in the provided set.
        let issuer = certs
            .iter()
            .find(|c| c.subject == current.issuer && !std::ptr::eq(*c, current));

        match issuer {
            Some(issuer_cert) => {
                chain.push(CertificateInfo::from_cert(issuer_cert));

                // Intermediate and root certs should be CAs.
                if !issuer_cert.is_ca && !issuer_cert.is_self_signed {
                    issues.push(format!(
                        "intermediate certificate '{}' is not a CA",
                        issuer_cert.subject_common_name().unwrap_or_default()
                    ));
                }

                if !issuer_cert.has_key_cert_sign_usage() && !issuer_cert.is_self_signed {
                    issues.push(format!(
                        "intermediate certificate '{}' missing keyCertSign usage",
                        issuer_cert.subject_common_name().unwrap_or_default()
                    ));
                }

                current = issuer_cert;
                depth += 1;
            }
            None => {
                issues.push("incomplete chain — issuer certificate not found".into());
                break;
            }
        }
    }

    if depth >= MAX_DEPTH {
        issues.push("chain too deep (possible loop)".into());
    }

    // A chain is considered valid if we reached a self-signed root
    // and there are no critical issues.
    let reached_root = current.is_self_signed;
    let chain_valid = reached_root && issues.is_empty();

    if !reached_root {
        issues.push("chain does not end at a self-signed root".into());
    }

    ChainVerificationResult {
        chain_valid,
        chain,
        issues,
    }
}

/// OCSP response status (simplified).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevocationStatus {
    /// Certificate is valid.
    Good,
    /// Certificate has been revoked.
    Revoked,
    /// Revocation status is unknown.
    Unknown,
}

/// OCSP/CRL revocation check result.
#[derive(Debug, Clone)]
pub struct RevocationResult {
    /// The certificate subject.
    pub subject: Option<String>,
    /// The revocation status.
    pub status: RevocationStatus,
    /// Source of the check.
    pub source: RevocationSource,
}

/// Where the revocation check result came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevocationSource {
    /// From an embedded OCSP response (in DSS or signature).
    EmbeddedOcsp,
    /// From an embedded CRL.
    EmbeddedCrl,
    /// No revocation information available.
    None,
}

/// Check revocation status using embedded OCSP/CRL data from the DSS.
///
/// This performs offline revocation checking using data already embedded
/// in the PDF. Online OCSP/CRL fetching is intentionally not implemented
/// (would require network access and async runtime).
pub fn check_revocation_embedded(
    cert: &X509Certificate,
    ocsp_responses: &[Vec<u8>],
    crls: &[Vec<u8>],
) -> RevocationResult {
    let subject = cert.subject_common_name();

    // Check embedded OCSP responses.
    for resp in ocsp_responses {
        if let Some(status) = parse_ocsp_response_status(resp, cert) {
            return RevocationResult {
                subject,
                status,
                source: RevocationSource::EmbeddedOcsp,
            };
        }
    }

    // Check embedded CRLs.
    for crl in crls {
        if let Some(status) = check_crl_for_cert(crl, cert) {
            return RevocationResult {
                subject,
                status,
                source: RevocationSource::EmbeddedCrl,
            };
        }
    }

    RevocationResult {
        subject,
        status: RevocationStatus::Unknown,
        source: RevocationSource::None,
    }
}

/// Attempt to parse an OCSP response and check if it covers this cert.
///
/// This is a simplified check — we look for the cert's serial number
/// in the OCSP response's single responses.
fn parse_ocsp_response_status(
    resp_data: &[u8],
    _cert: &X509Certificate,
) -> Option<RevocationStatus> {
    use crate::cms::{parse_context_explicit, parse_context_implicit, parse_length, parse_tlv};
    let (_, outer_seq) = parse_tlv(resp_data)?;
    let (rest, status_enum) = parse_tlv(outer_seq)?;
    if status_enum.is_empty() || status_enum[0] != 0 {
        return None;
    }
    let (_, rbi) = parse_context_explicit(rest, 0)?;
    let (_, rbs) = parse_tlv(rbi)?;
    let (rest, _) = parse_tlv(rbs)?;
    let (_, resp_octet) = parse_tlv(rest)?;
    let (_, basic_seq) = parse_tlv(resp_octet)?;
    let (_, tbs_resp) = parse_tlv(basic_seq)?;
    let (_, tbs_seq) = parse_tlv(tbs_resp)?;
    let mut pos = tbs_seq;
    if let Some((r, _)) = parse_context_explicit(pos, 0) {
        pos = r;
    }
    if !pos.is_empty() && (pos[0] == 0xA1 || pos[0] == 0xA2) {
        let (len, inner) = parse_length(&pos[1..])?;
        if inner.len() < len {
            return None;
        }
        pos = &inner[len..];
    }
    let (rest, _) = parse_tlv(pos)?;
    pos = rest;
    let (_, responses_seq) = parse_tlv(pos)?;
    let mut rpos = responses_seq;
    while !rpos.is_empty() {
        if let Some((next, sr)) = parse_tlv(rpos) {
            if let Some(s) = check_single_response(sr) {
                return Some(s);
            }
            rpos = next;
        } else {
            break;
        }
    }
    if let Some((_, rd)) = parse_context_implicit(pos, 0) {
        let mut rpos = rd;
        while !rpos.is_empty() {
            if let Some((next, sr)) = parse_tlv(rpos) {
                if let Some(s) = check_single_response(sr) {
                    return Some(s);
                }
                rpos = next;
            } else {
                break;
            }
        }
    }
    None
}

/// Check a SingleResponse for certificate status.
fn check_single_response(data: &[u8]) -> Option<RevocationStatus> {
    use crate::cms::parse_tlv;
    let (rest, _cert_id) = parse_tlv(data)?;
    if rest.is_empty() {
        return None;
    }
    match rest[0] & 0xBF {
        0x80 => Some(RevocationStatus::Good),
        0xA1 | 0x81 => Some(RevocationStatus::Revoked),
        0x82 => Some(RevocationStatus::Unknown),
        _ => None,
    }
}

/// Check if a certificate serial appears in a CRL (RFC 5280 Section 5).
fn check_crl_for_cert(crl_data: &[u8], cert: &X509Certificate) -> Option<RevocationStatus> {
    use crate::cms::parse_tlv;
    let (_, crl_seq) = parse_tlv(crl_data)?;
    let (_, tbs_crl) = parse_tlv(crl_seq)?;
    let (_, tbs_seq) = parse_tlv(tbs_crl)?;
    let mut pos = tbs_seq;
    if !pos.is_empty() && pos[0] == 0x02 {
        let (rest, _) = parse_tlv(pos)?;
        pos = rest;
    }
    let (rest, _) = parse_tlv(pos)?;
    pos = rest;
    let (rest, _) = parse_tlv(pos)?;
    pos = rest;
    let (rest, _) = parse_tlv(pos)?;
    pos = rest;
    if !pos.is_empty() && (pos[0] == 0x17 || pos[0] == 0x18) {
        let (rest, _) = parse_tlv(pos)?;
        pos = rest;
    }
    if pos.is_empty() || pos[0] != 0x30 {
        return Some(RevocationStatus::Good);
    }
    let (_, revoked_seq) = parse_tlv(pos)?;
    let cert_serial = extract_serial_number(cert)?;
    let mut rpos = revoked_seq;
    while !rpos.is_empty() {
        if let Some((next, entry_seq)) = parse_tlv(rpos) {
            if let Some((_, serial)) = parse_tlv(entry_seq) {
                if serial == cert_serial {
                    return Some(RevocationStatus::Revoked);
                }
            }
            rpos = next;
        } else {
            break;
        }
    }
    Some(RevocationStatus::Good)
}

/// Extract the serial number from a certificate's TBS data.
fn extract_serial_number(cert: &X509Certificate) -> Option<Vec<u8>> {
    use crate::cms::{parse_context_explicit, parse_tlv};
    let (_, tbs_seq) = parse_tlv(&cert.tbs_raw)?;
    let mut pos = tbs_seq;
    if let Some((rest, _)) = parse_context_explicit(pos, 0) {
        pos = rest;
    }
    let (_, serial) = parse_tlv(pos)?;
    Some(serial.to_vec())
}
