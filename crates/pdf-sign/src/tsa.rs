//! RFC 3161 Time Stamp Authority (TSA) support.
//!
//! Provides functions to request, parse, and embed RFC 3161 timestamps
//! for PAdES-T compliant PDF signatures. The timestamp token is embedded
//! as an unsigned attribute (id-smime-aa-timeStampToken) on the CMS
//! SignerInfo.
//!
//! Gated behind the `tsa` feature flag.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
    /// The token stamps something other than the signature it was handed.
    ImprintMismatch(String),
    /// The bytes handed over are not a parseable RFC 3161 TimeStampToken.
    NotATimestampToken(String),
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
            Self::ImprintMismatch(e) => write!(f, "TSA message imprint mismatch: {e}"),
            Self::NotATimestampToken(e) => write!(f, "not an RFC 3161 timestamp token: {e}"),
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
const OID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];

/// OID 1.2.840.113549.1.9.16.1.4 — id-ct-TSTInfo
const OID_CT_TST_INFO: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x01, 0x04,
];

/// OID 1.2.840.113549.1.9.3 -- id-contentType.
const OID_CONTENT_TYPE: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x03];

/// OID 1.2.840.113549.1.9.4 -- id-messageDigest.
const OID_MESSAGE_DIGEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x04];

/// OID 1.2.840.113549.1.9.16.2.12 -- id-aa-signingCertificate (RFC 2634).
const OID_SIGNING_CERTIFICATE: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x0C,
];

/// OID 1.2.840.113549.1.9.16.2.47 -- id-aa-signingCertificateV2 (RFC 5035).
const OID_SIGNING_CERTIFICATE_V2: &[u8] = &[
    0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x10, 0x02, 0x2F,
];

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
///
/// `signature_value` is the CMS `SignerInfo` signature the authority is being
/// asked to stamp -- the bytes, not their digest; the digest is taken here.
///
/// **No test reaches this, deliberately.** It posts an RFC 3161 request to a
/// timestamp authority over HTTP and returns what comes back. A test would
/// either need a real TSA -- which makes the suite depend on somebody else's
/// uptime, and fail on a train -- or a fake one, which would prove that our
/// mock answers our request and nothing about the protocol.
///
/// What is worth testing here is the parsing of the response, and that lives
/// in the functions this one calls.
#[cfg(feature = "tsa")]
pub fn request_timestamp(config: &TsaConfig, signature_value: &[u8]) -> Result<Vec<u8>, TsaError> {
    use sha2::Digest;
    use std::io::Read;

    // Compute the hash of the signature value.
    let hash = match config.hash_algorithm {
        DigestAlgorithm::Sha256 => sha2::Sha256::digest(signature_value).to_vec(),
        DigestAlgorithm::Sha384 => sha2::Sha384::digest(signature_value).to_vec(),
        DigestAlgorithm::Sha512 => sha2::Sha512::digest(signature_value).to_vec(),
        _ => sha2::Sha256::digest(signature_value).to_vec(),
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

    // Verify the token stamps what we asked to have stamped. Not "if it
    // parses": a token whose imprint cannot be read is a token that cannot be
    // shown to cover this signature, and shipping it would put the claim in
    // the file anyway.
    token_stamps(&token, signature_value)?;

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
    let tst_info = extract_tst_info(token_der).ok()?;
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

/// What a `TimeStampToken` says it stamped: which digest, over which bytes.
///
/// An RFC 3161 authority never sees the data itself, only this hash. So the
/// only question a verifier can ask of a token is whether that hash is the
/// hash of the signature the file actually carries, and this is the shape of
/// the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageImprint {
    /// The digest the authority was asked to stamp under.
    pub algorithm: DigestAlgorithm,
    /// The digest value itself.
    pub hash: Vec<u8>,
}

/// Hash `data` the way an RFC 3161 `messageImprint` does.
///
/// Returns `None` for [`DigestAlgorithm::Unknown`]: a token naming a digest we
/// cannot compute is a token we cannot check, and guessing SHA-256 would turn
/// "unverifiable" into "mismatch", which reads as a different fault.
pub fn message_imprint(data: &[u8], algorithm: DigestAlgorithm) -> Option<MessageImprint> {
    use sha2::Digest as _;
    let hash = match algorithm {
        DigestAlgorithm::Sha1 => {
            use sha1::Digest as _;
            sha1::Sha1::digest(data).to_vec()
        }
        DigestAlgorithm::Sha256 => sha2::Sha256::digest(data).to_vec(),
        DigestAlgorithm::Sha384 => sha2::Sha384::digest(data).to_vec(),
        DigestAlgorithm::Sha512 => sha2::Sha512::digest(data).to_vec(),
        DigestAlgorithm::Unknown => return None,
    };
    Some(MessageImprint { algorithm, hash })
}

/// Read the `messageImprint` out of a DER-encoded `TimeStampToken`.
///
/// The imprint is the only field that ties a token to a document. Reading it
/// is what separates "these bytes are shaped like a token" from "this token is
/// about our signature".
pub fn token_message_imprint(token_der: &[u8]) -> Result<MessageImprint, TsaError> {
    let tst_info = extract_tst_info(token_der)?;
    let (after_tst_info, tst_seq) = tlv(tst_info, TAG_SEQUENCE, "TSTInfo")?;
    // The eContent is the DER encoding of exactly one TSTInfo. A second TLV
    // inside that OCTET STRING is covered by the authority's own message digest
    // and travels with the token, so the imprint can match while the content is
    // something a conforming decoder refuses to read.
    if !after_tst_info.is_empty() {
        return Err(TsaError::NotATimestampToken(format!(
            "{} bytes follow the TSTInfo inside the encapsulated content, which holds one \
             TSTInfo and nothing else",
            after_tst_info.len(),
        )));
    }

    // TSTInfo ::= SEQUENCE { version INTEGER, policy OID, messageImprint, ... }
    let (rest, _version) = tlv(tst_seq, TAG_INTEGER, "TSTInfo.version")?;
    let (rest, _policy) = tlv(rest, TAG_OID, "TSTInfo.policy")?;

    // messageImprint ::= SEQUENCE { hashAlgorithm AlgorithmIdentifier, hashedMessage OCTET STRING }
    let (after_imprint, imprint) = tlv(rest, TAG_SEQUENCE, "TSTInfo.messageImprint")?;
    let (rest, algo_id) = tlv(imprint, TAG_SEQUENCE, "messageImprint.hashAlgorithm")?;
    let (_, algo_oid) = tlv(algo_id, TAG_OID, "hashAlgorithm.algorithm")?;
    let (_, hash) = tlv(rest, TAG_OCTET_STRING, "messageImprint.hashedMessage")?;

    // The imprint is read, but the fields after it are not optional and a
    // caller-supplied structure that stops here is not a token. Both are what
    // says WHEN the stamp was made, which is the only thing a timestamp is
    // for; a verifier reads them and rejects what does not have them.
    let (rest, _serial) = tlv(after_imprint, TAG_INTEGER, "TSTInfo.serialNumber")?;
    let (_, _gen_time) = tlv(rest, TAG_GENERALIZED_TIME, "TSTInfo.genTime")?;

    Ok(MessageImprint {
        algorithm: DigestAlgorithm::from_oid(algo_oid),
        hash: hash.to_vec(),
    })
}

const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_SET: u8 = 0x31;
const TAG_GENERALIZED_TIME: u8 = 0x18;

/// Parse one TLV and require it to carry `tag`.
///
/// The CMS parser this builds on reads any tag it is handed, which is right for
/// walking a structure whose shape is already known. It is wrong here: the
/// question these functions answer is whether arbitrary bytes ARE a
/// TimeStampToken, and a walk that accepts any tag answers "yes" for anything
/// with the same nesting. Naming the tag turns the walk into a check.
fn tlv<'a>(data: &'a [u8], tag: u8, what: &str) -> Result<(&'a [u8], &'a [u8]), TsaError> {
    match data.first() {
        Some(&found) if found == tag => cms::parse_tlv(data).ok_or_else(|| {
            TsaError::NotATimestampToken(format!("{what}: truncated or over-long length"))
        }),
        Some(&found) => Err(TsaError::NotATimestampToken(format!(
            "{what}: expected ASN.1 tag 0x{tag:02x}, found 0x{found:02x}"
        ))),
        None => Err(TsaError::NotATimestampToken(format!(
            "{what}: nothing left to read"
        ))),
    }
}

/// Walk the first `SignerInfo` in a `signerInfos` SET.
///
/// "Not empty" is not the same as "somebody signed it": a SET holding a NULL,
/// or any other bytes at all, is non-empty and carries no signature. The fields
/// are not read for their values -- verifying the authority's signature needs
/// its certificate and is the verifier's job -- but a structure that does not
/// have them is not one a verifier will accept, and embedding it would put the
/// B-T claim in the file regardless.
fn signer_info_parses(
    signer_infos: &[u8],
    tst_info_der: &[u8],
    digest_algorithms: &[u8],
) -> Result<(), TsaError> {
    let (_, si) = tlv(signer_infos, TAG_SEQUENCE, "signerInfos[0]")?;

    // SignerInfo ::= SEQUENCE { version, sid, digestAlgorithm, [0] signedAttrs,
    //                           signatureAlgorithm, signature, [1] unsignedAttrs }
    let (rest, _version) = tlv(si, TAG_INTEGER, "SignerInfo.version")?;
    // sid is a CHOICE, but a CHOICE of exactly two things: an
    // issuerAndSerialNumber SEQUENCE, or [0] subjectKeyIdentifier. Reading any
    // tag here would accept a NULL as an identity, which names no signer and
    // which a conforming verifier rejects -- so the two are named.
    let (rest, _sid) = match rest.first() {
        Some(&TAG_SEQUENCE) => {
            let (after_sid, sid) = tlv(rest, TAG_SEQUENCE, "sid.issuerAndSerialNumber")?;
            // IssuerAndSerialNumber ::= SEQUENCE { issuer Name, serialNumber
            // CertificateSerialNumber }. Name is a CHOICE of one arm, an
            // RDNSequence, so it is a SEQUENCE. A SEQUENCE holding anything
            // else names no certificate, and accepting the wrapper alone would
            // leave the arm as unchecked as reading any tag was.
            let (after_issuer, _issuer) =
                tlv(sid, TAG_SEQUENCE, "sid.issuerAndSerialNumber.issuer")?;
            let (_, serial) = tlv(
                after_issuer,
                TAG_INTEGER,
                "sid.issuerAndSerialNumber.serialNumber",
            )?;
            if serial.is_empty() {
                return Err(TsaError::NotATimestampToken(
                    "sid.issuerAndSerialNumber.serialNumber is empty, so it numbers no \
                     certificate"
                        .into(),
                ));
            }
            (after_sid, sid)
        }
        // subjectKeyIdentifier is [0] IMPLICIT OCTET STRING, so its tag is the
        // primitive 0x80; 0xA0 is accepted beside it because encoders in the
        // wild write the constructed form and the CMS parser takes either.
        // Accepting only 0xA0 would refuse a real authority's token, which is
        // the opposite fault to the one this check is here for.
        Some(0x80 | 0xA0) => cms::parse_context_implicit(rest, 0).ok_or_else(|| {
            TsaError::NotATimestampToken("sid.subjectKeyIdentifier: truncated".into())
        })?,
        Some(&found) => {
            return Err(TsaError::NotATimestampToken(format!(
                "SignerInfo.sid: expected an issuerAndSerialNumber SEQUENCE or a [0] \
                 subjectKeyIdentifier, found ASN.1 tag 0x{found:02x}"
            )))
        }
        None => {
            return Err(TsaError::NotATimestampToken(
                "SignerInfo: missing sid".into(),
            ))
        }
    };
    let (rest, digest_algorithm) = tlv(rest, TAG_SEQUENCE, "SignerInfo.digestAlgorithm")?;
    let (_, digest_oid) = tlv(digest_algorithm, TAG_OID, "SignerInfo.digestAlgorithm")?;
    let digest_algorithm = DigestAlgorithm::from_oid(digest_oid);
    // RFC 5652 §5.1: SignedData.digestAlgorithms collects the digests the
    // signers used. It may hold more than are used, so it is not compared for
    // equality -- but a SET that does not contain this signer's own digest
    // leaves a verifier free to fail the signature, and a token a verifier may
    // fail is one we should not be embedding.
    signer_digest_is_listed(digest_algorithms, digest_oid, digest_algorithm)?;
    // signedAttrs is optional in CMS only while the encapsulated content is
    // id-data. A timestamp's is id-ct-TSTInfo, so the attributes are required,
    // and a verifier that does not find them rejects the token.
    let (rest, signed_attrs) = cms::parse_context_implicit(rest, 0).ok_or_else(|| {
        TsaError::NotATimestampToken(
            "SignerInfo.signedAttrs is absent: CMS requires signed attributes when the \
             encapsulated content is id-ct-TSTInfo rather than id-data"
                .into(),
        )
    })?;
    signed_attributes_name_a_timestamp(signed_attrs, tst_info_der, digest_algorithm)?;
    let (rest, _signature_algorithm) = tlv(rest, TAG_SEQUENCE, "SignerInfo.signatureAlgorithm")?;
    let (_, signature) = tlv(rest, TAG_OCTET_STRING, "SignerInfo.signature")?;
    if signature.is_empty() {
        return Err(TsaError::NotATimestampToken(
            "SignerInfo.signature is empty: nobody signed this time".into(),
        ));
    }
    Ok(())
}

/// Require the signed attributes CMS makes mandatory, and read what they say.
///
/// `signedAttrs` is a SET OF Attribute, each a type OID and a SET of values.
/// Two of them are mandatory whenever the encapsulated content is not id-data
/// (RFC 5652 §5.3): the content type and the message digest. Without them the
/// token is not a conforming CMS signature at all, and the B-T/B-LT document it
/// would be embedded in fails validation -- so a SET that merely holds bytes is
/// not enough, and this walks it.
///
/// The content type is compared, because an attribute that names something
/// other than id-ct-TSTInfo describes a different signature than the SignedData
/// around it. The message digest is required to be a non-empty OCTET STRING and
/// no more: its value is the hash of the eContent under the authority's key, so
/// checking it means verifying the authority's signature, which needs the
/// authority's certificate and is the verifier's work, not the signer's.
///
/// RFC 3161 §2.4.1 adds a third: the ESS signing-certificate attribute, which
/// names the certificate the authority signed with. Its contents are the
/// verifier's business -- checking them needs the certificate -- but a token
/// without it does not say whose key to check, so its presence is required
/// here in either the RFC 2634 or the RFC 5035 encoding.
fn signed_attributes_name_a_timestamp(
    signed_attrs: &[u8],
    tst_info_der: &[u8],
    digest_algorithm: DigestAlgorithm,
) -> Result<(), TsaError> {
    let mut rest = signed_attrs;
    let mut content_types = 0usize;
    let mut message_digests = 0usize;
    let mut signing_certificates = 0usize;

    while !rest.is_empty() {
        let (next, attribute) = tlv(rest, TAG_SEQUENCE, "signedAttrs: Attribute")?;
        rest = next;

        let (after_type, attr_type) = tlv(attribute, TAG_OID, "signedAttrs: Attribute.attrType")?;
        let (_, values) = tlv(after_type, TAG_SET, "signedAttrs: Attribute.attrValues")?;

        if attr_type == OID_CONTENT_TYPE {
            let after_oid = oid_is(
                values,
                OID_CT_TST_INFO,
                "signedAttrs: the content-type attribute",
            )?;
            only_one_value(after_oid, "content-type")?;
            content_types += 1;
        } else if attr_type == OID_MESSAGE_DIGEST {
            let (after_digest, digest) =
                tlv(values, TAG_OCTET_STRING, "signedAttrs: the message digest")?;
            only_one_value(after_digest, "message-digest")?;
            if digest.is_empty() {
                return Err(TsaError::NotATimestampToken(
                    "signedAttrs: the message-digest attribute is empty, so it digests nothing"
                        .into(),
                ));
            }
            // The value is the digest of the encapsulated content under the
            // SignerInfo's own digest algorithm, and both of those are in hand,
            // so this one attribute can be compared rather than taken on trust.
            // A token whose message digest is over something else is signed
            // over a different TSTInfo than the one carrying our imprint --
            // which is the same fault this whole module exists for, one layer
            // in. Under a digest we do not know, the comparison is skipped
            // rather than failed: a hash added after this code was written is
            // not a reason to refuse an otherwise conforming token.
            if let Some(expected) = message_imprint(tst_info_der, digest_algorithm) {
                if digest != expected.hash {
                    return Err(TsaError::NotATimestampToken(format!(
                        "signedAttrs: the message-digest attribute is not the {} digest of the \
                         TSTInfo it is signed over, so the authority signed a different time \
                         than this token carries",
                        algorithm_name(digest_algorithm),
                    )));
                }
            }
            message_digests += 1;
        } else if attr_type == OID_SIGNING_CERTIFICATE || attr_type == OID_SIGNING_CERTIFICATE_V2 {
            // RFC 3161 §2.4.1: the TSA names the certificate that made the
            // token in a signed attribute, so a verifier can tell which key to
            // check without guessing among the certificates travelling with it.
            // Both encodings are accepted -- v2 (RFC 5035) is what an authority
            // on a SHA-2 policy writes, v1 (RFC 2634) what an older one does.
            // What is inside is the verifier's business, and it needs the
            // certificate to do anything with it; that it is there, and is a
            // structure rather than bytes, is checkable here.
            let (after_certificate, certificate) = tlv(
                values,
                TAG_SEQUENCE,
                "signedAttrs: the signing-certificate attribute",
            )?;
            only_one_value(after_certificate, "signing-certificate")?;
            // SigningCertificate[V2] ::= SEQUENCE { certs SEQUENCE OF ESSCertID
            // [v2], policies OPTIONAL }. An empty SEQUENCE is the right tag
            // around nothing, and names no certificate at all -- which is the
            // whole reason the attribute is required.
            let (_, certs) = tlv(
                certificate,
                TAG_SEQUENCE,
                "signedAttrs: signing-certificate certs",
            )?;
            let (_, _first) = tlv(
                certs,
                TAG_SEQUENCE,
                "signedAttrs: signing-certificate certs[0]",
            )?;
            signing_certificates += 1;
        }
    }

    if content_types > 1 || message_digests > 1 || signing_certificates > 1 {
        // RFC 5652 §5.3: a SignerInfo carries at most one of each of these, and
        // a verifier that sees two has no way to say which one the signature
        // was computed over.
        return Err(TsaError::NotATimestampToken(
            "signedAttrs: the content-type, message-digest and signing-certificate attributes \
             may each appear once, and one of them appears more than once here"
                .into(),
        ));
    }
    if content_types == 0 {
        return Err(TsaError::NotATimestampToken(
            "signedAttrs: no content-type attribute, which CMS requires when the encapsulated \
             content is id-ct-TSTInfo"
                .into(),
        ));
    }
    if message_digests == 0 {
        return Err(TsaError::NotATimestampToken(
            "signedAttrs: no message-digest attribute, so nothing ties this signature to the \
             time it is supposed to carry"
                .into(),
        ));
    }
    if signing_certificates == 0 {
        return Err(TsaError::NotATimestampToken(
            "signedAttrs: no signing-certificate attribute, so the token does not say which \
             certificate the authority signed it with (RFC 3161 §2.4.1, in either the RFC 2634 \
             or the RFC 5035 encoding)"
                .into(),
        ));
    }
    Ok(())
}

/// Require `SignedData.digestAlgorithms` to name the digest this signer used.
fn signer_digest_is_listed(
    digest_algorithms: &[u8],
    digest_oid: &[u8],
    algorithm: DigestAlgorithm,
) -> Result<(), TsaError> {
    let mut rest = digest_algorithms;
    while !rest.is_empty() {
        let (next, identifier) = tlv(rest, TAG_SEQUENCE, "SignedData.digestAlgorithms entry")?;
        rest = next;
        let (_, oid) = tlv(identifier, TAG_OID, "SignedData.digestAlgorithms entry")?;
        if oid == digest_oid {
            return Ok(());
        }
    }
    Err(TsaError::NotATimestampToken(format!(
        "SignedData.digestAlgorithms does not list {}, which is the digest its own SignerInfo \
         says it used",
        algorithm_name(algorithm),
    )))
}

/// Require a single-valued attribute's SET to hold nothing beyond its value.
///
/// Content type and message digest are single-valued in CMS. A SET with a
/// second value in it leaves a verifier no way to say which one the signature
/// covers, so it is refused rather than read past.
fn only_one_value(after_the_value: &[u8], attribute: &str) -> Result<(), TsaError> {
    if after_the_value.is_empty() {
        return Ok(());
    }
    Err(TsaError::NotATimestampToken(format!(
        "signedAttrs: the {attribute} attribute carries more than one value, and CMS gives it \
         exactly one"
    )))
}

/// Require an OID TLV to hold exactly `expected`.
fn oid_is<'a>(data: &'a [u8], expected: &[u8], what: &str) -> Result<&'a [u8], TsaError> {
    let (rest, oid) = tlv(data, TAG_OID, what)?;
    if oid != expected {
        return Err(TsaError::NotATimestampToken(format!(
            "{what}: not the expected object identifier"
        )));
    }
    Ok(rest)
}

/// Refuse a token that does not stamp `signature_value`.
///
/// This is the check that makes a PAdES B-T claim mean something. A timestamp
/// token is carried into the CMS as opaque bytes, so without reading its
/// imprint back nothing distinguishes a token over this signature from a token
/// over somebody else's, a token over the empty string, or a token fetched
/// before this signature existed at all -- and the last of those was the
/// documented way to use the pre-supplied-token API. The file would then
/// advertise B-T and fail at the first verifier that checked.
///
/// `signature_value` is the `signature` field of the CMS `SignerInfo` (the
/// encrypted digest), which is what CAdES §6.1.1 says a signature timestamp
/// covers.
pub fn token_stamps(token_der: &[u8], signature_value: &[u8]) -> Result<(), TsaError> {
    let stamped = token_message_imprint(token_der)?;
    let ours = message_imprint(signature_value, stamped.algorithm).ok_or_else(|| {
        TsaError::ImprintMismatch(
            "the token names a digest algorithm this build cannot compute, so what it \
             stamped cannot be compared with the signature in the file"
                .into(),
        )
    })?;
    if ours.hash == stamped.hash {
        return Ok(());
    }
    Err(TsaError::ImprintMismatch(format!(
        "the token stamps {} of some other bytes ({}), not the signature in this file ({}). \
         A token obtained before signing cannot match, because the signature does not exist \
         yet: sign first, ask the authority for a token over the signature that came out, \
         then embed it (sign_pdf_pending_timestamp)",
        algorithm_name(stamped.algorithm),
        hex_prefix(&stamped.hash),
        hex_prefix(&ours.hash),
    )))
}

/// A short, printable head of a digest, for an error a human has to read.
fn hex_prefix(hash: &[u8]) -> String {
    let mut out = String::new();
    for b in hash.iter().take(8) {
        out.push_str(&format!("{b:02x}"));
    }
    if hash.len() > 8 {
        out.push('\u{2026}');
    }
    out
}

fn algorithm_name(algorithm: DigestAlgorithm) -> &'static str {
    match algorithm {
        DigestAlgorithm::Sha1 => "SHA-1",
        DigestAlgorithm::Sha256 => "SHA-256",
        DigestAlgorithm::Sha384 => "SHA-384",
        DigestAlgorithm::Sha512 => "SHA-512",
        DigestAlgorithm::Unknown => "an unrecognised digest",
    }
}

/// Extract the raw TSTInfo bytes from a ContentInfo wrapping SignedData.
///
/// Every step is checked, tag and object identifier alike. Reading the nesting
/// alone would accept any CMS SignedData whose encapsulated content happens to
/// parse as a TSTInfo -- and accepting such bytes is how a signature comes to
/// carry a timestamp attribute that no conforming verifier will accept.
fn extract_tst_info(token_der: &[u8]) -> Result<&[u8], TsaError> {
    // ContentInfo ::= SEQUENCE { contentType OID, content [0] EXPLICIT }
    let (trailing, content_info) = tlv(token_der, TAG_SEQUENCE, "TimeStampToken")?;
    // The whole slice has to be the token. `embed_timestamp_in_cms` copies these
    // bytes verbatim into the attribute-value SET, so anything after the
    // ContentInfo becomes a second value in that SET -- checked here and
    // shipped anyway, which a conforming verifier rejects.
    if !trailing.is_empty() {
        return Err(TsaError::NotATimestampToken(format!(
            "{} bytes follow the TimeStampToken; the whole value has to be the token, \
             because all of it is what gets embedded",
            trailing.len(),
        )));
    }
    let rest = oid_is(content_info, OID_SIGNED_DATA, "ContentInfo.contentType")?;
    let (after_content, explicit) = cms::parse_context_explicit(rest, 0).ok_or_else(|| {
        TsaError::NotATimestampToken("ContentInfo: missing [0] EXPLICIT content".into())
    })?;
    // The same rule one level in. ContentInfo is contentType and [0] content
    // and nothing else, and bytes hiding after the [0] are inside the outer
    // SEQUENCE, so the check above never sees them -- while all of it is still
    // what gets embedded.
    if !after_content.is_empty() {
        return Err(TsaError::NotATimestampToken(format!(
            "{} bytes follow ContentInfo's [0] content inside the ContentInfo; it holds a \
             content type and a content and nothing else",
            after_content.len(),
        )));
    }

    // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo, ... }
    let (_, signed_data) = tlv(explicit, TAG_SEQUENCE, "SignedData")?;
    let (rest, _version) = tlv(signed_data, TAG_INTEGER, "SignedData.version")?;
    // digestAlgorithms is a SET OF AlgorithmIdentifier. Reading any tag here
    // takes a NULL for the field and walks on; naming the tag costs nothing and
    // is one more place a structure that is not a SignedData stops. The SET may
    // name more digests than are used; what it may not do is leave out the one
    // its own SignerInfo names, which `signer_digest_is_listed` checks once
    // that SignerInfo has been read.
    let (rest, digest_algorithms) = tlv(rest, TAG_SET, "SignedData.digestAlgorithms")?;
    let (_, _first_algorithm) = tlv(
        digest_algorithms,
        TAG_SEQUENCE,
        "SignedData.digestAlgorithms[0]",
    )?;

    // encapContentInfo ::= SEQUENCE { eContentType OID, eContent [0] EXPLICIT }
    let (after_encap, encap) = tlv(rest, TAG_SEQUENCE, "SignedData.encapContentInfo")?;
    let rest = oid_is(encap, OID_CT_TST_INFO, "encapContentInfo.eContentType")?;
    let (_, explicit) = cms::parse_context_explicit(rest, 0).ok_or_else(|| {
        TsaError::NotATimestampToken("encapContentInfo: missing [0] EXPLICIT eContent".into())
    })?;

    // eContent OCTET STRING containing DER-encoded TSTInfo
    let (_, tst_info_der) = tlv(explicit, TAG_OCTET_STRING, "encapContentInfo.eContent")?;

    // signerInfos, after the optional certificates [0] and crls [1]. It is the
    // authority's own signature over the TSTInfo, so a SignedData that stops
    // before it carries a time nobody vouched for -- which is not a timestamp,
    // whatever its content types say.
    let mut rest = after_encap;
    if let Some((next, _certificates)) = cms::parse_context_implicit(rest, 0) {
        rest = next;
    }
    if let Some((next, _crls)) = cms::parse_context_implicit(rest, 1) {
        rest = next;
    }
    let (_, signer_infos) = tlv(rest, TAG_SET, "SignedData.signerInfos")?;
    if signer_infos.is_empty() {
        return Err(TsaError::NotATimestampToken(
            "SignedData.signerInfos is empty: nobody signed this time".into(),
        ));
    }
    signer_info_parses(signer_infos, tst_info_der, digest_algorithms)?;

    Ok(tst_info_der)
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
