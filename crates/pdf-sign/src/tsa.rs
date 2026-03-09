//! RFC 3161 Time Stamp Authority (TSA) support.
//!
//! Provides functions to request, parse, and embed RFC 3161 timestamps
//! for PAdES-T compliant PDF signatures. The timestamp token is embedded
//! as an unsigned attribute (id-smime-aa-timeStampToken) on the CMS
//! SignerInfo.
//!
//! Gated behind the `tsa` feature flag.

use crate::byte_range::DigestAlgorithm;
use crate::cms;

/// Errors that can occur during TSA operations.
#[derive(Debug)]
pub enum TsaError {
    /// Failed to build the timestamp request.
    BuildRequest(String),
    /// HTTP request to TSA failed.
    Http(String),
    /// TSA returned an error status.
    TsaStatus(u32, String),
    /// Failed to parse the timestamp response.
    ParseResponse(String),
    /// Nonce mismatch between request and response.
    NonceMismatch,
    /// Message imprint mismatch.
    ImprintMismatch,
    /// Failed to embed timestamp in CMS.
    EmbedFailed(String),
}

impl std::fmt::Display for TsaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuildRequest(e) => write!(f, "TSA request build failed: {e}"),
            Self::Http(e) => write!(f, "TSA HTTP request failed: {e}"),
            Self::TsaStatus(code, msg) => write!(f, "TSA returned status {code}: {msg}"),
            Self::ParseResponse(e) => write!(f, "TSA response parse failed: {e}"),
            Self::NonceMismatch => write!(f, "TSA nonce mismatch"),
            Self::ImprintMismatch => write!(f, "TSA message imprint mismatch"),
            Self::EmbedFailed(e) => write!(f, "embed timestamp in CMS failed: {e}"),
        }
    }
}

impl std::error::Error for TsaError {}

/// Configuration for a Time Stamp Authority.
#[derive(Debug, Clone)]
pub struct TsaConfig {
    /// TSA endpoint URL (e.g., `http://freetsa.org/tsr`).
    pub url: String,
    /// Optional HTTP Basic Auth username.
    pub username: Option<String>,
    /// Optional HTTP Basic Auth password.
    pub password: Option<String>,
    /// Hash algorithm for the timestamp request (default: SHA-256).
    pub hash_algorithm: DigestAlgorithm,
    /// Request timeout in seconds (default: 30).
    pub timeout_secs: u64,
}

impl Default for TsaConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            hash_algorithm: DigestAlgorithm::Sha256,
            timeout_secs: 30,
            username: None,
            password: None,
        }
    }
}

// ---------------------------------------------------------------------------
// OIDs
// ---------------------------------------------------------------------------

/// OID 2.16.840.1.101.3.4.2.1 — SHA-256
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
/// OID 2.16.840.1.101.3.4.2.2 — SHA-384
const OID_SHA384: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02];
/// OID 2.16.840.1.101.3.4.2.3 — SHA-512
const OID_SHA512: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x03];
/// OID 1.3.14.3.2.26 — SHA-1
const OID_SHA1: &[u8] = &[0x2B, 0x0E, 0x03, 0x02, 0x1A];

/// OID 1.2.840.113549.1.9.16.2.14 — id-smime-aa-timeStampToken
const OID_TIMESTAMP_TOKEN: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x0E,
];

/// OID 1.2.840.113549.1.7.2 — id-signedData
#[cfg(test)]
const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];

fn digest_oid(algo: DigestAlgorithm) -> &'static [u8] {
    match algo {
        DigestAlgorithm::Sha1 => OID_SHA1,
        DigestAlgorithm::Sha256 => OID_SHA256,
        DigestAlgorithm::Sha384 => OID_SHA384,
        DigestAlgorithm::Sha512 => OID_SHA512,
        _ => OID_SHA256,
    }
}

// ---------------------------------------------------------------------------
// DER encoding helpers
// ---------------------------------------------------------------------------

fn der_tag_length(tag: u8, content_len: usize) -> Vec<u8> {
    let mut out = vec![tag];
    encode_length(content_len, &mut out);
    out
}

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

fn der_sequence(contents: &[u8]) -> Vec<u8> {
    let mut out = der_tag_length(0x30, contents.len());
    out.extend_from_slice(contents);
    out
}

fn der_oid(oid: &[u8]) -> Vec<u8> {
    let mut out = der_tag_length(0x06, oid.len());
    out.extend_from_slice(oid);
    out
}

fn der_octet_string(data: &[u8]) -> Vec<u8> {
    let mut out = der_tag_length(0x04, data.len());
    out.extend_from_slice(data);
    out
}

fn der_integer(value: u64) -> Vec<u8> {
    // Encode as minimal unsigned DER integer.
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(7);
    let significant = &bytes[start..];
    // Prepend 0x00 if high bit set (to avoid negative interpretation).
    let needs_pad = significant.first().is_some_and(|&b| b & 0x80 != 0);
    let len = significant.len() + if needs_pad { 1 } else { 0 };
    let mut out = der_tag_length(0x02, len);
    if needs_pad {
        out.push(0x00);
    }
    out.extend_from_slice(significant);
    out
}

fn der_boolean(value: bool) -> Vec<u8> {
    vec![0x01, 0x01, if value { 0xFF } else { 0x00 }]
}

fn der_null() -> Vec<u8> {
    vec![0x05, 0x00]
}

// ---------------------------------------------------------------------------
// Build TimeStampReq (RFC 3161 §2.4.1)
// ---------------------------------------------------------------------------

/// Build a DER-encoded TimeStampReq.
///
/// ```text
/// TimeStampReq ::= SEQUENCE {
///     version          INTEGER { v1(1) },
///     messageImprint   MessageImprint,
///     nonce            INTEGER OPTIONAL,
///     certReq          BOOLEAN DEFAULT FALSE
/// }
///
/// MessageImprint ::= SEQUENCE {
///     hashAlgorithm    AlgorithmIdentifier,
///     hashedMessage    OCTET STRING
/// }
/// ```
pub fn build_tsa_request(
    hash: &[u8],
    algo: DigestAlgorithm,
    nonce: Option<u64>,
) -> Result<Vec<u8>, TsaError> {
    if hash.is_empty() {
        return Err(TsaError::BuildRequest("empty hash".into()));
    }

    // AlgorithmIdentifier ::= SEQUENCE { algorithm OID, parameters NULL }
    let oid = digest_oid(algo);
    let mut algo_id_content = der_oid(oid);
    algo_id_content.extend_from_slice(&der_null());
    let algo_id = der_sequence(&algo_id_content);

    // MessageImprint ::= SEQUENCE { hashAlgorithm, hashedMessage }
    let mut mi_content = algo_id;
    mi_content.extend_from_slice(&der_octet_string(hash));
    let message_imprint = der_sequence(&mi_content);

    // TimeStampReq fields.
    let mut req_content = der_integer(1); // version = 1
    req_content.extend_from_slice(&message_imprint);

    if let Some(n) = nonce {
        req_content.extend_from_slice(&der_integer(n));
    }

    // certReq BOOLEAN TRUE
    req_content.extend_from_slice(&der_boolean(true));

    Ok(der_sequence(&req_content))
}

// ---------------------------------------------------------------------------
// Send timestamp request via HTTP
// ---------------------------------------------------------------------------

/// Send a timestamp request to a TSA endpoint and return the raw
/// DER-encoded TimeStampToken (CMS ContentInfo).
#[cfg(feature = "tsa")]
pub fn request_timestamp(config: &TsaConfig, signature_hash: &[u8]) -> Result<Vec<u8>, TsaError> {
    use sha2::Digest;
    use std::io::Read;

    // Compute the hash of the signature value.
    let hash = match config.hash_algorithm {
        DigestAlgorithm::Sha256 => sha2::Sha256::digest(signature_hash).to_vec(),
        DigestAlgorithm::Sha384 => sha2::Sha384::digest(signature_hash).to_vec(),
        DigestAlgorithm::Sha512 => sha2::Sha512::digest(signature_hash).to_vec(),
        _ => sha2::Sha256::digest(signature_hash).to_vec(),
    };

    // Generate nonce from current time.
    let nonce: u64 = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42)
    };

    let req_der = build_tsa_request(&hash, config.hash_algorithm, Some(nonce))?;

    // HTTP POST to TSA.
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build();

    let mut request = agent
        .post(&config.url)
        .set("Content-Type", "application/timestamp-query")
        .set("Accept", "application/timestamp-reply");

    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        let credentials = simple_base64(&format!("{user}:{pass}"));
        request = request.set("Authorization", &format!("Basic {credentials}"));
    }

    let response = request
        .send_bytes(&req_der)
        .map_err(|e| TsaError::Http(e.to_string()))?;

    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|e| TsaError::Http(format!("read response body: {e}")))?;

    // Parse the TimeStampResp and extract the token.
    let token = parse_tsa_response(&body)?;

    // Verify nonce matches.
    if let Some(resp_nonce) = extract_nonce_from_token(&token) {
        if resp_nonce != nonce {
            return Err(TsaError::NonceMismatch);
        }
    }

    // Verify message imprint matches.
    if let Some(resp_imprint) = extract_imprint_from_token(&token) {
        if resp_imprint != hash {
            return Err(TsaError::ImprintMismatch);
        }
    }

    Ok(token)
}

#[cfg(feature = "tsa")]
fn simple_base64(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = input.as_bytes();
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 2 < data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(CHARS[((n >> 18) & 63) as usize]);
        out.push(CHARS[((n >> 12) & 63) as usize]);
        out.push(CHARS[((n >> 6) & 63) as usize]);
        out.push(CHARS[(n & 63) as usize]);
        i += 3;
    }
    let remaining = data.len() - i;
    if remaining == 1 {
        let n = (data[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 63) as usize]);
        out.push(CHARS[((n >> 12) & 63) as usize]);
        out.push(b'=');
        out.push(b'=');
    } else if remaining == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 63) as usize]);
        out.push(CHARS[((n >> 12) & 63) as usize]);
        out.push(CHARS[((n >> 6) & 63) as usize]);
        out.push(b'=');
    }
    String::from_utf8(out).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Parse TimeStampResp (RFC 3161 §2.4.2)
// ---------------------------------------------------------------------------

/// Parse a DER-encoded TimeStampResp and extract the TimeStampToken.
///
/// ```text
/// TimeStampResp ::= SEQUENCE {
///     status          PKIStatusInfo,
///     timeStampToken  ContentInfo OPTIONAL
/// }
///
/// PKIStatusInfo ::= SEQUENCE {
///     status        PKIStatus (INTEGER),
///     statusString  PKIFreeText OPTIONAL,
///     failInfo      PKIFailureInfo OPTIONAL
/// }
/// ```
pub fn parse_tsa_response(data: &[u8]) -> Result<Vec<u8>, TsaError> {
    let (_, resp_seq) = cms::parse_tlv(data)
        .ok_or_else(|| TsaError::ParseResponse("not a valid SEQUENCE".into()))?;

    // PKIStatusInfo SEQUENCE
    let (rest, status_seq) = cms::parse_tlv(resp_seq)
        .ok_or_else(|| TsaError::ParseResponse("missing PKIStatusInfo".into()))?;

    // status INTEGER
    let (_, status_bytes) = cms::parse_tlv(status_seq)
        .ok_or_else(|| TsaError::ParseResponse("missing status INTEGER".into()))?;

    let status_value = parse_der_integer_value(status_bytes);

    // status 0 = granted, 1 = grantedWithMods
    if status_value > 1 {
        let status_str = match status_value {
            2 => "rejection",
            3 => "waiting",
            4 => "revocationWarning",
            5 => "revocationNotification",
            _ => "unknown",
        };
        return Err(TsaError::TsaStatus(
            status_value as u32,
            status_str.to_string(),
        ));
    }

    // timeStampToken ContentInfo (the rest after PKIStatusInfo)
    if rest.is_empty() {
        return Err(TsaError::ParseResponse(
            "no timeStampToken in response".into(),
        ));
    }

    // The token is the raw DER of the ContentInfo.
    let (_, token_raw) = cms::parse_tlv_raw(rest)
        .ok_or_else(|| TsaError::ParseResponse("invalid timeStampToken".into()))?;

    Ok(token_raw.to_vec())
}

fn parse_der_integer_value(data: &[u8]) -> u64 {
    let mut value = 0u64;
    for &b in data {
        value = (value << 8) | b as u64;
    }
    value
}

// ---------------------------------------------------------------------------
// Extract fields from TimeStampToken for verification
// ---------------------------------------------------------------------------

/// Extract the nonce from a TimeStampToken (TSTInfo within CMS ContentInfo).
#[cfg(feature = "tsa")]
fn extract_nonce_from_token(token_der: &[u8]) -> Option<u64> {
    let tst_info = extract_tst_info(token_der)?;
    // TSTInfo ::= SEQUENCE { version, policy, messageImprint, serialNumber,
    //                         genTime, accuracy, ordering, nonce, ... }
    let (_, tst_seq) = cms::parse_tlv(tst_info)?;
    let mut pos = tst_seq;

    // Skip: version, policy, messageImprint, serialNumber, genTime
    for _ in 0..5 {
        let (rest, _) = cms::parse_tlv(pos)?;
        pos = rest;
    }

    // Optional: accuracy (SEQUENCE), ordering (BOOLEAN), nonce (INTEGER)
    // We need to skip optional fields until we find an INTEGER.
    while !pos.is_empty() {
        let tag = pos[0];
        if tag == 0x02 {
            // INTEGER — this is the nonce
            let (_, nonce_bytes) = cms::parse_tlv(pos)?;
            return Some(parse_der_integer_value(nonce_bytes));
        }
        // Skip this TLV
        let (rest, _) = cms::parse_tlv(pos)?;
        pos = rest;
    }

    None
}

/// Extract the message imprint hash from a TimeStampToken.
#[cfg(feature = "tsa")]
fn extract_imprint_from_token(token_der: &[u8]) -> Option<Vec<u8>> {
    let tst_info = extract_tst_info(token_der)?;
    let (_, tst_seq) = cms::parse_tlv(tst_info)?;
    let mut pos = tst_seq;

    // Skip: version, policy
    let (rest, _) = cms::parse_tlv(pos)?;
    pos = rest;
    let (rest, _) = cms::parse_tlv(pos)?;
    pos = rest;

    // messageImprint SEQUENCE { hashAlgorithm, hashedMessage }
    let (_, imprint_seq) = cms::parse_tlv(pos)?;
    let (rest2, _algo) = cms::parse_tlv(imprint_seq)?;
    let (_, hash_bytes) = cms::parse_tlv(rest2)?;

    Some(hash_bytes.to_vec())
}

/// Extract the raw TSTInfo bytes from a ContentInfo wrapping SignedData.
#[cfg(feature = "tsa")]
fn extract_tst_info(token_der: &[u8]) -> Option<&[u8]> {
    // ContentInfo ::= SEQUENCE { contentType OID, content [0] EXPLICIT }
    let (_, content_info) = cms::parse_tlv(token_der)?;
    let (rest, _oid) = cms::parse_tlv(content_info)?;
    let (_, explicit) = cms::parse_context_explicit(rest, 0)?;

    // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo, ... }
    let (_, signed_data) = cms::parse_tlv(explicit)?;
    let mut pos = signed_data;

    // Skip version
    let (rest, _) = cms::parse_tlv(pos)?;
    pos = rest;
    // Skip digestAlgorithms
    let (rest, _) = cms::parse_tlv(pos)?;
    pos = rest;

    // encapContentInfo ::= SEQUENCE { contentType OID, eContent [0] EXPLICIT }
    let (_, encap) = cms::parse_tlv(pos)?;
    let (rest2, _ct_oid) = cms::parse_tlv(encap)?;
    let (_, explicit2) = cms::parse_context_explicit(rest2, 0)?;

    // eContent OCTET STRING containing DER-encoded TSTInfo
    let (_, tst_info_der) = cms::parse_tlv(explicit2)?;
    Some(tst_info_der)
}

// ---------------------------------------------------------------------------
// Embed timestamp in CMS SignedData
// ---------------------------------------------------------------------------

/// Embed a timestamp token as an unsigned attribute on CMS SignerInfo.
///
/// The timestamp token (a CMS ContentInfo) is added as the
/// id-smime-aa-timeStampToken (OID 1.2.840.113549.1.9.16.2.14)
/// unsigned attribute on the first SignerInfo in the CMS.
///
/// Returns the modified DER-encoded CMS ContentInfo.
pub fn embed_timestamp_in_cms(cms_der: &[u8], timestamp_token: &[u8]) -> Result<Vec<u8>, TsaError> {
    // We need to find the SignerInfo, and append an unsigned attribute.
    // Strategy: parse the CMS structure, locate the SignerInfo, and rebuild
    // with the unsigned attribute added.

    // ContentInfo ::= SEQUENCE { contentType OID, content [0] EXPLICIT SignedData }
    let (_, content_info) = cms::parse_tlv(cms_der)
        .ok_or_else(|| TsaError::EmbedFailed("not a valid ContentInfo".into()))?;

    let (rest, content_type_oid_raw) = cms::parse_tlv_raw(content_info)
        .ok_or_else(|| TsaError::EmbedFailed("missing contentType OID".into()))?;

    let (_, explicit_content) = cms::parse_context_explicit(rest, 0)
        .ok_or_else(|| TsaError::EmbedFailed("missing [0] EXPLICIT content".into()))?;

    let (_, signed_data_seq) = cms::parse_tlv(explicit_content)
        .ok_or_else(|| TsaError::EmbedFailed("missing SignedData SEQUENCE".into()))?;

    // Parse SignedData fields to find the SignerInfos (last SET).
    let mut pos = signed_data_seq;
    let mut prefix_parts: Vec<&[u8]> = Vec::new();

    // version
    let (rest, _) =
        cms::parse_tlv_raw(pos).ok_or_else(|| TsaError::EmbedFailed("missing version".into()))?;
    prefix_parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // digestAlgorithms
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("missing digestAlgorithms".into()))?;
    prefix_parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // encapContentInfo
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("missing encapContentInfo".into()))?;
    prefix_parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // certificates [0] IMPLICIT (optional)
    if let Some((rest2, _)) = cms::parse_context_implicit(pos, 0) {
        prefix_parts.push(&pos[..pos.len() - rest2.len()]);
        pos = rest2;
    }

    // crls [1] IMPLICIT (optional)
    if let Some((rest2, _)) = cms::parse_context_implicit(pos, 1) {
        prefix_parts.push(&pos[..pos.len() - rest2.len()]);
        pos = rest2;
    }

    // signerInfos SET — pos should now point to the SET OF SignerInfo
    let (_, signer_infos_set) = cms::parse_tlv(pos)
        .ok_or_else(|| TsaError::EmbedFailed("missing signerInfos SET".into()))?;

    // Parse the first SignerInfo SEQUENCE.
    let (si_rest, signer_info_seq) = cms::parse_tlv(signer_infos_set)
        .ok_or_else(|| TsaError::EmbedFailed("missing SignerInfo SEQUENCE".into()))?;

    // Rebuild the SignerInfo with the unsigned attribute appended.
    let modified_si = append_unsigned_attribute(signer_info_seq, timestamp_token)?;

    // Rebuild the SET OF SignerInfo.
    let modified_si_seq = der_sequence(&modified_si);
    let mut si_set_content = modified_si_seq;
    si_set_content.extend_from_slice(si_rest); // Any remaining SignerInfos

    let mut si_set = der_tag_length(0x31, si_set_content.len()); // SET OF
    si_set.extend_from_slice(&si_set_content);

    // Rebuild SignedData SEQUENCE.
    let mut sd_content = Vec::new();
    for part in &prefix_parts {
        sd_content.extend_from_slice(part);
    }
    sd_content.extend_from_slice(&si_set);
    let signed_data = der_sequence(&sd_content);

    // Wrap in [0] EXPLICIT.
    let mut explicit = der_tag_length(0xA0, signed_data.len());
    explicit.extend_from_slice(&signed_data);

    // Rebuild ContentInfo SEQUENCE.
    let mut ci_content = Vec::new();
    ci_content.extend_from_slice(content_type_oid_raw);
    ci_content.extend_from_slice(&explicit);
    let content_info_out = der_sequence(&ci_content);

    Ok(content_info_out)
}

/// Append an unsigned attribute (timeStampToken) to a SignerInfo.
fn append_unsigned_attribute(
    signer_info: &[u8],
    timestamp_token: &[u8],
) -> Result<Vec<u8>, TsaError> {
    let mut pos = signer_info;
    let mut parts: Vec<&[u8]> = Vec::new();

    // version
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("SI: missing version".into()))?;
    parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // sid
    let (rest, _) =
        cms::parse_tlv_raw(pos).ok_or_else(|| TsaError::EmbedFailed("SI: missing sid".into()))?;
    parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // digestAlgorithm
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("SI: missing digestAlgorithm".into()))?;
    parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // signedAttrs [0] IMPLICIT (optional)
    if let Some((rest2, _)) = cms::parse_context_implicit(pos, 0) {
        parts.push(&pos[..pos.len() - rest2.len()]);
        pos = rest2;
    }

    // signatureAlgorithm
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("SI: missing signatureAlgorithm".into()))?;
    parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // signature OCTET STRING
    let (rest, _) = cms::parse_tlv_raw(pos)
        .ok_or_else(|| TsaError::EmbedFailed("SI: missing signature".into()))?;
    parts.push(&pos[..pos.len() - rest.len()]);
    pos = rest;

    // Skip existing unsigned attributes [1] IMPLICIT if present.
    let existing_unsigned = if let Some((rest2, data)) = cms::parse_context_implicit(pos, 1) {
        pos = rest2;
        Some(data)
    } else {
        None
    };

    // Build the timestamp attribute.
    // Attribute ::= SEQUENCE { attrType OID, attrValues SET OF ANY }
    let attr_oid = der_oid(OID_TIMESTAMP_TOKEN);
    let mut attr_values_content = Vec::new();
    attr_values_content.extend_from_slice(timestamp_token);
    let mut attr_values = der_tag_length(0x31, attr_values_content.len()); // SET OF
    attr_values.extend_from_slice(&attr_values_content);

    let mut attr_content = attr_oid;
    attr_content.extend_from_slice(&attr_values);
    let attr = der_sequence(&attr_content);

    // Build the unsigned attributes [1] IMPLICIT SET.
    let mut unsigned_content = Vec::new();
    if let Some(existing) = existing_unsigned {
        // Merge: existing unsigned attrs + new timestamp attr.
        unsigned_content.extend_from_slice(existing);
    }
    unsigned_content.extend_from_slice(&attr);

    let mut unsigned_attrs = der_tag_length(0xA1, unsigned_content.len());
    unsigned_attrs.extend_from_slice(&unsigned_content);

    // Rebuild the SignerInfo.
    let mut result = Vec::new();
    for part in &parts {
        result.extend_from_slice(part);
    }
    result.extend_from_slice(&unsigned_attrs);

    // Include any remaining data after the SignerInfo fields (shouldn't be any).
    if !pos.is_empty() {
        result.extend_from_slice(pos);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_timestamp_request_sha256() {
        let hash = [0xAB; 32]; // Fake SHA-256 hash
        let req = build_tsa_request(&hash, DigestAlgorithm::Sha256, Some(12345)).unwrap();

        // Should be a valid DER SEQUENCE.
        assert_eq!(req[0], 0x30);
        assert!(!req.is_empty());

        // Should contain the hash bytes.
        assert!(req.windows(32).any(|w| w == &hash[..]));
    }

    #[test]
    fn build_timestamp_request_sha384() {
        let hash = [0xCD; 48];
        let req = build_tsa_request(&hash, DigestAlgorithm::Sha384, None).unwrap();
        assert_eq!(req[0], 0x30);
        assert!(req.windows(48).any(|w| w == &hash[..]));
    }

    #[test]
    fn build_timestamp_request_empty_hash_fails() {
        let result = build_tsa_request(&[], DigestAlgorithm::Sha256, None);
        assert!(result.is_err());
    }

    #[test]
    fn build_timestamp_request_with_nonce() {
        let hash = [0x42; 32];
        let req_with = build_tsa_request(&hash, DigestAlgorithm::Sha256, Some(999)).unwrap();
        let req_without = build_tsa_request(&hash, DigestAlgorithm::Sha256, None).unwrap();
        // Request with nonce should be longer.
        assert!(req_with.len() > req_without.len());
    }

    #[test]
    fn build_timestamp_request_certreq_true() {
        let hash = [0x42; 32];
        let req = build_tsa_request(&hash, DigestAlgorithm::Sha256, None).unwrap();
        // Should contain BOOLEAN TRUE (0x01, 0x01, 0xFF).
        assert!(req.windows(3).any(|w| w == [0x01, 0x01, 0xFF]));
    }

    #[test]
    fn parse_tsa_response_bad_status() {
        // Construct a minimal TimeStampResp with status = 2 (rejection).
        let status_int = vec![0x02, 0x01, 0x02]; // INTEGER 2
        let status_info = der_sequence(&status_int);
        let resp = der_sequence(&status_info);

        let result = parse_tsa_response(&resp);
        assert!(matches!(result, Err(TsaError::TsaStatus(2, _))));
    }

    #[test]
    fn parse_tsa_response_missing_token() {
        // status = 0 (granted) but no token.
        let status_int = vec![0x02, 0x01, 0x00]; // INTEGER 0
        let status_info = der_sequence(&status_int);
        let resp = der_sequence(&status_info);

        let result = parse_tsa_response(&resp);
        assert!(matches!(result, Err(TsaError::ParseResponse(_))));
    }

    #[test]
    fn der_integer_encoding() {
        // 0 → [0x02, 0x01, 0x00]
        let enc = der_integer(0);
        assert_eq!(&enc, &[0x02, 0x01, 0x00]);

        // 127 → [0x02, 0x01, 0x7F]
        let enc = der_integer(127);
        assert_eq!(&enc, &[0x02, 0x01, 0x7F]);

        // 128 → needs leading 0x00 pad: [0x02, 0x02, 0x00, 0x80]
        let enc = der_integer(128);
        assert_eq!(&enc, &[0x02, 0x02, 0x00, 0x80]);

        // 256 → [0x02, 0x02, 0x01, 0x00]
        let enc = der_integer(256);
        assert_eq!(&enc, &[0x02, 0x02, 0x01, 0x00]);
    }

    #[test]
    fn embed_timestamp_roundtrip() {
        // Build a minimal CMS ContentInfo / SignedData with one SignerInfo.
        let fake_token = build_fake_content_info();
        let cms = build_minimal_cms();

        let result = embed_timestamp_in_cms(&cms, &fake_token);
        assert!(result.is_ok(), "embed failed: {:?}", result.err());

        let modified = result.unwrap();
        // Should be a valid SEQUENCE.
        assert_eq!(modified[0], 0x30);
        // Should contain the timestamp OID.
        assert!(modified
            .windows(OID_TIMESTAMP_TOKEN.len())
            .any(|w| w == OID_TIMESTAMP_TOKEN));
    }

    #[test]
    fn embed_timestamp_preserves_signature() {
        let fake_token = build_fake_content_info();
        let cms = build_minimal_cms();
        let modified = embed_timestamp_in_cms(&cms, &fake_token).unwrap();

        // The fake signature bytes [0xSIG; 64] should still be present.
        let sig_bytes = [0xAA; 64];
        assert!(modified.windows(64).any(|w| w == &sig_bytes[..]));
    }

    // -- test helpers --

    fn build_fake_content_info() -> Vec<u8> {
        // Minimal ContentInfo wrapping empty SignedData.
        let oid = der_oid(OID_SIGNED_DATA);
        let inner = der_sequence(&[0x02, 0x01, 0x01]); // version=1
        let mut explicit = der_tag_length(0xA0, inner.len());
        explicit.extend_from_slice(&inner);
        let mut ci_content = oid;
        ci_content.extend_from_slice(&explicit);
        der_sequence(&ci_content)
    }

    fn build_minimal_cms() -> Vec<u8> {
        // Build: ContentInfo { OID signedData, [0] SignedData }
        // SignedData { version, digestAlgos, encapContent, signerInfos }

        let oid = der_oid(OID_SIGNED_DATA);

        // SignedData fields:
        let version = der_integer(1);

        // digestAlgorithms SET OF { AlgorithmIdentifier }
        let algo_oid = der_oid(OID_SHA256);
        let mut algo_content = algo_oid;
        algo_content.extend_from_slice(&der_null());
        let algo_id = der_sequence(&algo_content);
        let mut digest_algos = der_tag_length(0x31, algo_id.len());
        digest_algos.extend_from_slice(&algo_id);

        // encapContentInfo SEQUENCE { OID }
        let data_oid_bytes: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01]; // id-data
        let encap = der_sequence(&der_oid(data_oid_bytes));

        // SignerInfo SEQUENCE:
        let si_version = der_integer(1);

        // sid: IssuerAndSerialNumber (minimal)
        let issuer_name = der_sequence(&[]); // empty
        let serial = der_integer(1);
        let mut sid_content = issuer_name;
        sid_content.extend_from_slice(&serial);
        let sid = der_sequence(&sid_content);

        // digestAlgorithm
        let digest_algo = der_sequence(&{
            let mut c = der_oid(OID_SHA256);
            c.extend_from_slice(&der_null());
            c
        });

        // signatureAlgorithm (RSA with SHA-256)
        let rsa_oid: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]; // sha256WithRSAEncryption
        let sig_algo = der_sequence(&{
            let mut c = der_oid(rsa_oid);
            c.extend_from_slice(&der_null());
            c
        });

        // signature OCTET STRING (fake 64 bytes)
        let sig_value = der_octet_string(&[0xAA; 64]);

        let mut si_content = si_version;
        si_content.extend_from_slice(&sid);
        si_content.extend_from_slice(&digest_algo);
        si_content.extend_from_slice(&sig_algo);
        si_content.extend_from_slice(&sig_value);
        let signer_info = der_sequence(&si_content);

        // SET OF SignerInfo
        let mut si_set = der_tag_length(0x31, signer_info.len());
        si_set.extend_from_slice(&signer_info);

        // SignedData SEQUENCE
        let mut sd_content = version;
        sd_content.extend_from_slice(&digest_algos);
        sd_content.extend_from_slice(&encap);
        sd_content.extend_from_slice(&si_set);
        let signed_data = der_sequence(&sd_content);

        // [0] EXPLICIT
        let mut explicit = der_tag_length(0xA0, signed_data.len());
        explicit.extend_from_slice(&signed_data);

        // ContentInfo
        let mut ci_content = oid;
        ci_content.extend_from_slice(&explicit);
        der_sequence(&ci_content)
    }
}
