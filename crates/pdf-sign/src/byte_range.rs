//! Byte range digest verification (ISO 32000-2 §12.8.1).
//!
//! Computes the message digest over the signed byte ranges of the PDF
//! and compares it against the digest embedded in the CMS signature.

use digest::Digest;
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

use crate::sig_dict::SigDict;
use crate::types::DigestVerification;

/// Verify the byte-range digest of a signature.
///
/// Extracts the signed byte ranges from the PDF, computes the hash,
/// and compares it against the message digest in the CMS SignedData.
pub fn verify_byte_range_digest(
    pdf_data: &[u8],
    byte_range: &[usize; 4],
    sig: &SigDict<'_>,
) -> DigestVerification {
    let (off1, len1, off2, len2) = (byte_range[0], byte_range[1], byte_range[2], byte_range[3]);

    // Bounds check (use checked_add to prevent overflow with crafted values).
    let end1 = match off1.checked_add(len1) {
        Some(e) if e <= pdf_data.len() => e,
        _ => return DigestVerification::Error("byte range exceeds PDF size".into()),
    };
    let end2 = match off2.checked_add(len2) {
        Some(e) if e <= pdf_data.len() => e,
        _ => return DigestVerification::Error("byte range exceeds PDF size".into()),
    };

    let range1 = &pdf_data[off1..end1];
    let range2 = &pdf_data[off2..end2];

    // The gap between range1 and range2 should be the hex-encoded /Contents.
    // Verify that range2 starts right after the /Contents hex string.
    let gap_start = off1 + len1;
    if off2 < gap_start {
        return DigestVerification::Error("overlapping byte ranges".into());
    }

    // Determine which digest algorithm was used from the CMS data.
    let signed_data = match sig.cms_signed_data() {
        Some(sd) => sd,
        None => return DigestVerification::Error("cannot parse CMS SignedData".into()),
    };

    let algo = signed_data.digest_algorithm();
    let embedded_digest = match signed_data.message_digest() {
        Some(d) => d,
        None => return DigestVerification::Error("no message digest in CMS".into()),
    };

    let computed = match algo {
        DigestAlgorithm::Sha1 => {
            let mut h = Sha1::new();
            h.update(range1);
            h.update(range2);
            h.finalize().to_vec()
        }
        DigestAlgorithm::Sha256 => {
            let mut h = Sha256::new();
            h.update(range1);
            h.update(range2);
            h.finalize().to_vec()
        }
        DigestAlgorithm::Sha384 => {
            let mut h = Sha384::new();
            h.update(range1);
            h.update(range2);
            h.finalize().to_vec()
        }
        DigestAlgorithm::Sha512 => {
            let mut h = Sha512::new();
            h.update(range1);
            h.update(range2);
            h.finalize().to_vec()
        }
        DigestAlgorithm::Unknown => {
            return DigestVerification::Error("unknown digest algorithm".into());
        }
    };

    if computed == embedded_digest {
        DigestVerification::Ok
    } else {
        DigestVerification::Mismatch
    }
}

/// Supported digest algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    /// SHA-1 (legacy, not recommended).
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-384.
    Sha384,
    /// SHA-512.
    Sha512,
    /// Unrecognized algorithm.
    Unknown,
}

impl DigestAlgorithm {
    /// Parse from an ASN.1 OID byte sequence.
    pub fn from_oid(oid: &[u8]) -> Self {
        // Common OIDs for digest algorithms.
        match oid {
            // 1.3.14.3.2.26 — SHA-1
            [0x2B, 0x0E, 0x03, 0x02, 0x1A] => Self::Sha1,
            // 2.16.840.1.101.3.4.2.1 — SHA-256
            [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01] => Self::Sha256,
            // 2.16.840.1.101.3.4.2.2 — SHA-384
            [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02] => Self::Sha384,
            // 2.16.840.1.101.3.4.2.3 — SHA-512
            [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03] => Self::Sha512,
            _ => Self::Unknown,
        }
    }
}
