// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Integrity checking for files fetched over the network.
//!
//! # Why this module exists
//!
//! The PaddleOCR backend used to fetch up to ~84 MB of ONNX model weights from a
//! third-party public repository and write them straight to the cache, with no
//! integrity check of any kind. Whoever served those bytes decided what our
//! inference engine executed. For an SDK sold on privacy and on not phoning home,
//! that is the wrong default — and a warning in the README is not a fix, it is a
//! note explaining why the product is wrong.
//!
//! So fetching is now opt-in and always verified, and this module is the
//! verification. It is compiled unconditionally, without the `paddle` feature,
//! for one specific reason: tests behind a non-default feature flag do not run in
//! CI, and a security check whose tests never execute is indistinguishable from no
//! check at all. That exact mistake has cost this project three regressions.

use sha2::{Digest, Sha256};

/// Why a downloaded file was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityError {
    /// The digest of the received bytes does not match what was expected.
    DigestMismatch {
        /// What the caller said the file should hash to.
        expected: String,
        /// What it actually hashed to.
        actual: String,
    },
    /// The expected digest is not a 64-character hex SHA-256.
    MalformedDigest(String),
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DigestMismatch { expected, actual } => write!(
                f,
                "integrity check failed: expected sha256 {expected}, got {actual}. \
                 The file was NOT written. Either the source changed or the transfer \
                 was tampered with; do not retry without establishing which."
            ),
            Self::MalformedDigest(d) => write!(
                f,
                "malformed expected digest {d:?}: a sha256 is 64 hex characters"
            ),
        }
    }
}

impl std::error::Error for IntegrityError {}

/// Hex-encoded SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Check `bytes` against an expected hex SHA-256.
///
/// A malformed expected digest is an error rather than a skipped check: silently
/// accepting anything because the *expectation* was unreadable is precisely the
/// failure this module exists to prevent.
///
/// Comparison is case-insensitive on the hex, and constant-time over the digest so
/// that verification does not leak the expected value byte by byte.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), IntegrityError> {
    let expected = expected_hex.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(IntegrityError::MalformedDigest(expected_hex.to_string()));
    }

    let actual = sha256_hex(bytes);

    // Constant-time: fold every byte rather than returning on the first mismatch.
    let diff = actual
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));

    if diff == 0 {
        Ok(())
    } else {
        Err(IntegrityError::DigestMismatch { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The canonical SHA-256 of the empty input and of "abc", from FIPS 180-4.
    // Hardcoding known-answer vectors means a broken hasher fails here rather
    // than producing self-consistent nonsense that agrees with itself.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn known_answer_vectors() {
        assert_eq!(sha256_hex(b""), EMPTY);
        assert_eq!(sha256_hex(b"abc"), ABC);
    }

    #[test]
    fn accepts_matching_digest() {
        assert_eq!(verify_sha256(b"abc", ABC), Ok(()));
    }

    #[test]
    fn accepts_uppercase_and_padded_digest() {
        assert_eq!(
            verify_sha256(b"abc", &format!("  {}  ", ABC.to_uppercase())),
            Ok(())
        );
    }

    #[test]
    fn rejects_wrong_content() {
        // The scenario that matters: the URL is right, the bytes are not.
        let err = verify_sha256(b"abd", ABC).expect_err("must reject");
        match err {
            IntegrityError::DigestMismatch { expected, actual } => {
                assert_eq!(expected, ABC);
                assert_ne!(actual, ABC);
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn rejects_single_flipped_bit() {
        // 84 MB of weights differing in one byte must not pass.
        let mut a = vec![7u8; 4096];
        let good = sha256_hex(&a);
        a[2048] ^= 0x01;
        assert!(verify_sha256(&a, &good).is_err());
    }

    #[test]
    fn rejects_truncated_transfer() {
        let full = vec![3u8; 8192];
        let good = sha256_hex(&full);
        assert!(verify_sha256(&full[..4096], &good).is_err());
    }

    #[test]
    fn malformed_expectation_is_an_error_not_a_pass() {
        for bad in ["", "abc", "zz", &"f".repeat(63), &"g".repeat(64)] {
            assert!(
                matches!(
                    verify_sha256(b"anything", bad),
                    Err(IntegrityError::MalformedDigest(_))
                ),
                "expected {bad:?} to be rejected as malformed"
            );
        }
    }

    #[test]
    fn error_message_says_the_file_was_not_written() {
        // The message is part of the contract: a user who sees this must know
        // that nothing landed on disk and that retrying is not the answer.
        let msg = verify_sha256(b"abd", ABC).unwrap_err().to_string();
        assert!(msg.contains("NOT written"), "message was: {msg}");
    }
}
