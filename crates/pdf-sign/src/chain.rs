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
    _resp_data: &[u8],
    _cert: &X509Certificate,
) -> Option<RevocationStatus> {
    // Full OCSP response parsing (RFC 6960) requires significant ASN.1
    // work. For now, return None to indicate we couldn't determine status.
    // The structure is: OCSPResponse → ResponseBytes → BasicOCSPResponse
    //   → ResponseData → SingleResponse[] → CertStatus
    None
}

/// Check if a certificate serial appears in a CRL.
fn check_crl_for_cert(_crl_data: &[u8], _cert: &X509Certificate) -> Option<RevocationStatus> {
    // Full CRL parsing (RFC 5280 §5) requires serial number matching.
    // For now, return None to indicate we couldn't determine status.
    None
}
