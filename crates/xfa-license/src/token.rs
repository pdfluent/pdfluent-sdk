//! License token — HMAC-SHA256 signed JWT-like tokens.
//!
//! Token format: `base64(header).base64(claims_json).base64(hmac_sha256)`
//!
//! This is a simplified JWT compatible with offline validation.
//! The signing key is a shared secret between the license server and the engine.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::claims::LicenseClaims;
use crate::error::{LicenseError, Result};

type HmacSha256 = Hmac<Sha256>;

/// Token header (fixed for now).
const HEADER: &str = r#"{"alg":"HS256","typ":"XFA-LIC"}"#;

/// Sign license claims into a token string.
///
/// Returns a dot-separated string: `header.payload.signature`.
pub fn sign(claims: &LicenseClaims, secret: &[u8]) -> Result<String> {
    let header_b64 = URL_SAFE_NO_PAD.encode(HEADER.as_bytes());
    let payload_json = serde_json::to_vec(claims)?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_json);

    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature);

    Ok(format!("{signing_input}.{sig_b64}"))
}

/// Verify a token string and extract the claims.
///
/// Checks the HMAC signature but does NOT check expiry — the caller
/// should use [`LicenseClaims::is_expired`] for that.
pub fn verify(token: &str, secret: &[u8]) -> Result<LicenseClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(LicenseError::MalformedToken(format!(
            "expected 3 parts, got {}",
            parts.len()
        )));
    }

    let signing_input = format!("{}.{}", parts[0], parts[1]);

    // Verify signature.
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| LicenseError::MalformedToken(format!("bad signature base64: {e}")))?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&sig_bytes)
        .map_err(|_| LicenseError::InvalidSignature)?;

    // Decode payload.
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| LicenseError::MalformedToken(format!("bad payload base64: {e}")))?;

    let claims: LicenseClaims = serde_json::from_slice(&payload_bytes)?;
    Ok(claims)
}

/// Verify a token and additionally check that it has not expired.
pub fn verify_and_check_expiry(token: &str, secret: &[u8], now: u64) -> Result<LicenseClaims> {
    let claims = verify(token, secret)?;
    if claims.is_expired(now) {
        return Err(LicenseError::Expired(claims.expires_at));
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::Tier;

    const SECRET: &[u8] = b"test-secret-key-for-xfa-license";

    fn sample_claims() -> LicenseClaims {
        LicenseClaims::new("cust-42", Tier::Professional, 1700000000, 1703000000)
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let claims = sample_claims();
        let token = sign(&claims, SECRET).unwrap();
        let verified = verify(&token, SECRET).unwrap();
        assert_eq!(claims, verified);
    }

    #[test]
    fn token_has_three_parts() {
        let token = sign(&sample_claims(), SECRET).unwrap();
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn wrong_secret_fails() {
        let token = sign(&sample_claims(), SECRET).unwrap();
        let result = verify(&token, b"wrong-secret");
        assert!(matches!(result, Err(LicenseError::InvalidSignature)));
    }

    #[test]
    fn tampered_payload_fails() {
        let token = sign(&sample_claims(), SECRET).unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        // Replace one char in the payload.
        let mut payload = parts[1].to_string();
        let replacement = if payload.ends_with('A') { 'B' } else { 'A' };
        payload.pop();
        payload.push(replacement);
        let tampered = format!("{}.{}.{}", parts[0], payload, parts[2]);
        let result = verify(&tampered, SECRET);
        assert!(matches!(result, Err(LicenseError::InvalidSignature)));
    }

    #[test]
    fn malformed_token_too_few_parts() {
        let result = verify("only.two", SECRET);
        assert!(matches!(result, Err(LicenseError::MalformedToken(_))));
    }

    #[test]
    fn malformed_token_bad_base64() {
        let result = verify("a.b.!!!invalid!!!", SECRET);
        assert!(matches!(result, Err(LicenseError::MalformedToken(_))));
    }

    #[test]
    fn verify_with_expiry_check() {
        let claims = sample_claims(); // expires at 1703000000
        let token = sign(&claims, SECRET).unwrap();

        // Before expiry → OK.
        let result = verify_and_check_expiry(&token, SECRET, 1701000000);
        assert!(result.is_ok());

        // After expiry → error.
        let result = verify_and_check_expiry(&token, SECRET, 1704000000);
        assert!(matches!(result, Err(LicenseError::Expired(1703000000))));
    }

    #[test]
    fn different_tiers_produce_different_tokens() {
        let basic = LicenseClaims::new("c", Tier::Basic, 1000, 2000);
        let pro = LicenseClaims::new("c", Tier::Professional, 1000, 2000);
        let t1 = sign(&basic, SECRET).unwrap();
        let t2 = sign(&pro, SECRET).unwrap();
        assert_ne!(t1, t2);
    }
}
