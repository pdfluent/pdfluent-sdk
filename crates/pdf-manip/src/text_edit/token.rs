//! MatchId token encoding and revision fingerprints.
//!
//! Wire format (design §10.2): `pdfluent-match-v1.<base64url(JSON payload)>`.
//! The token is opaque to consumers; decoding enforces version, length and
//! range limits, and every locator is fully revalidated against the live
//! document before use.

use sha2::{Digest, Sha256};

use super::{StaleReason, TextEditError};

/// Token prefix including the format version.
const TOKEN_PREFIX: &str = "pdfluent-match-v1.";
/// Hard ceiling on the encoded token length (design §10.2: strict limits).
const MAX_TOKEN_LEN: usize = 4096;

// ---------------------------------------------------------------------------
// Revision fingerprint
// ---------------------------------------------------------------------------

/// Identifies one exact revision of a document during text editing.
///
/// Composed of a SHA-256 digest of the exact source bytes plus an in-session
/// commit counter (design §10.1). The PDF `/ID` entry is never used as a
/// substitute for the byte digest. Every successful commit produces the next
/// revision (`counter + 1`), invalidating all previously issued [`super::MatchId`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DocumentRevision {
    digest: [u8; 32],
    counter: u64,
}

impl DocumentRevision {
    /// Fingerprint the exact input bytes of a document (commit counter 0).
    ///
    /// Reopening byte-identical input yields an equal revision.
    pub fn from_source_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest: [u8; 32] = hasher.finalize().into();
        Self { digest, counter: 0 }
    }

    /// The revision after one more committed transaction.
    #[must_use]
    pub fn next(&self) -> Self {
        Self {
            digest: self.digest,
            counter: self.counter + 1,
        }
    }

    /// Commit counter component (0 for freshly opened bytes).
    pub fn counter(&self) -> u64 {
        self.counter
    }

    pub(crate) fn digest_hex(&self) -> String {
        hex(&self.digest)
    }
}

// ---------------------------------------------------------------------------
// Hash helpers
// ---------------------------------------------------------------------------

/// 64-bit content hash (first 8 bytes of SHA-256), hex-encoded.
pub(crate) fn hash64_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    hex(&digest[..8])
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

// ---------------------------------------------------------------------------
// Token payload
// ---------------------------------------------------------------------------

/// Versioned JSON payload inside a MatchId token. Internal only — the schema
/// may evolve behind the version prefix.
///
/// Encoded/decoded with a hand-rolled strict codec for exactly this flat
/// schema (design §10.2), so the engine does not depend on the optional
/// `serde` feature.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TokenPayload {
    /// Payload schema version (currently 1).
    pub v: u8,
    /// Hex SHA-256 digest of the source bytes.
    pub fp: String,
    /// Commit counter of the revision that produced the match.
    pub ctr: u64,
    /// 1-based page number.
    pub page: u32,
    /// Container key: `"p"` for the page's logical stream sequence, or
    /// `"x:<name>/<name>…"` for a Form XObject resource path.
    pub ck: String,
    /// Byte range of the match inside the container's combined visual text.
    pub chr: [u64; 2],
    /// hash64 of the matched text bytes.
    pub sh: String,
    /// hash64 of the surrounding text context (±32 bytes).
    pub ch: String,
}

impl TokenPayload {
    pub(crate) fn matches_revision(&self, rev: &DocumentRevision) -> bool {
        self.ctr == rev.counter && self.fp == rev.digest_hex()
    }
}

/// Encode a payload into the opaque token string.
pub(crate) fn encode_token(payload: &TokenPayload) -> String {
    let json = format!(
        r#"{{"v":{},"fp":{},"ctr":{},"page":{},"ck":{},"chr":[{},{}],"sh":{},"ch":{}}}"#,
        payload.v,
        json_string(&payload.fp),
        payload.ctr,
        payload.page,
        json_string(&payload.ck),
        payload.chr[0],
        payload.chr[1],
        json_string(&payload.sh),
        json_string(&payload.ch),
    );
    format!("{TOKEN_PREFIX}{}", base64url_encode(json.as_bytes()))
}

/// Decode and strictly validate a token string.
pub(crate) fn decode_token(token: &str) -> Result<TokenPayload, TextEditError> {
    let invalid = |why: &str| TextEditError::InvalidMatchId {
        reason: why.to_string(),
    };
    if token.len() > MAX_TOKEN_LEN {
        return Err(invalid("token exceeds maximum length"));
    }
    let Some(body) = token.strip_prefix(TOKEN_PREFIX) else {
        return Err(invalid("unrecognized token format/version prefix"));
    };
    let bytes = base64url_decode(body).ok_or_else(|| invalid("invalid base64url body"))?;
    let json = std::str::from_utf8(&bytes).map_err(|_| invalid("payload is not UTF-8"))?;
    let payload = parse_payload(json).map_err(|e| invalid(&format!("invalid payload: {e}")))?;
    if payload.v != 1 {
        return Err(invalid("unsupported payload version"));
    }
    if payload.fp.len() != 64 || !payload.fp.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("malformed fingerprint"));
    }
    if payload.sh.len() != 16 || payload.ch.len() != 16 {
        return Err(invalid("malformed content hashes"));
    }
    if payload.page == 0 {
        return Err(invalid("page numbers are 1-based"));
    }
    if payload.chr[1] <= payload.chr[0] || payload.chr[1] > u64::from(u32::MAX) {
        return Err(invalid("character range out of bounds"));
    }
    if !(payload.ck == "p" || payload.ck.starts_with("x:")) {
        return Err(invalid("unrecognized container key"));
    }
    Ok(payload)
}

/// Check a decoded payload against the session revision.
pub(crate) fn check_revision(
    payload: &TokenPayload,
    rev: &DocumentRevision,
    id: &super::MatchId,
) -> Result<(), TextEditError> {
    if !payload.matches_revision(rev) {
        return Err(TextEditError::StaleMatch {
            match_id: id.clone(),
            reason: StaleReason::RevisionChanged,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Minimal strict JSON codec for the flat TokenPayload schema
// ---------------------------------------------------------------------------

/// Serialize a Rust string as a JSON string literal.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Strict parser for exactly the TokenPayload object: one flat JSON object,
/// known keys only, each key exactly once. Anything else is an error.
fn parse_payload(json: &str) -> Result<TokenPayload, String> {
    let mut p = Parser {
        b: json.as_bytes(),
        i: 0,
    };
    let mut v: Option<u64> = None;
    let mut fp: Option<String> = None;
    let mut ctr: Option<u64> = None;
    let mut page: Option<u64> = None;
    let mut ck: Option<String> = None;
    let mut chr: Option<[u64; 2]> = None;
    let mut sh: Option<String> = None;
    let mut ch: Option<String> = None;

    p.expect(b'{')?;
    loop {
        let key = p.parse_string()?;
        p.expect(b':')?;
        let dup = |name: &str| format!("duplicate key '{name}'");
        match key.as_str() {
            "v" => {
                if v.replace(p.parse_u64()?).is_some() {
                    return Err(dup("v"));
                }
            }
            "fp" => {
                if fp.replace(p.parse_string()?).is_some() {
                    return Err(dup("fp"));
                }
            }
            "ctr" => {
                if ctr.replace(p.parse_u64()?).is_some() {
                    return Err(dup("ctr"));
                }
            }
            "page" => {
                if page.replace(p.parse_u64()?).is_some() {
                    return Err(dup("page"));
                }
            }
            "ck" => {
                if ck.replace(p.parse_string()?).is_some() {
                    return Err(dup("ck"));
                }
            }
            "chr" => {
                p.expect(b'[')?;
                let a = p.parse_u64()?;
                p.expect(b',')?;
                let b = p.parse_u64()?;
                p.expect(b']')?;
                if chr.replace([a, b]).is_some() {
                    return Err(dup("chr"));
                }
            }
            "sh" => {
                if sh.replace(p.parse_string()?).is_some() {
                    return Err(dup("sh"));
                }
            }
            "ch" => {
                if ch.replace(p.parse_string()?).is_some() {
                    return Err(dup("ch"));
                }
            }
            other => return Err(format!("unknown key '{other}'")),
        }
        match p.next_byte()? {
            b',' => continue,
            b'}' => break,
            other => return Err(format!("unexpected byte 0x{other:02x}")),
        }
    }
    p.skip_ws();
    if p.i != p.b.len() {
        return Err("trailing data after object".to_string());
    }

    let v = v.ok_or("missing 'v'")?;
    if v > u64::from(u8::MAX) {
        return Err("'v' out of range".to_string());
    }
    let page = page.ok_or("missing 'page'")?;
    if page > u64::from(u32::MAX) {
        return Err("'page' out of range".to_string());
    }
    Ok(TokenPayload {
        v: v as u8,
        fp: fp.ok_or("missing 'fp'")?,
        ctr: ctr.ok_or("missing 'ctr'")?,
        page: page as u32,
        ck: ck.ok_or("missing 'ck'")?,
        chr: chr.ok_or("missing 'chr'")?,
        sh: sh.ok_or("missing 'sh'")?,
        ch: ch.ok_or("missing 'ch'")?,
    })
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&c) = self.b.get(self.i) {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn next_byte(&mut self) -> Result<u8, String> {
        self.skip_ws();
        let c = *self.b.get(self.i).ok_or("unexpected end of input")?;
        self.i += 1;
        Ok(c)
    }

    fn expect(&mut self, want: u8) -> Result<(), String> {
        let got = self.next_byte()?;
        if got == want {
            Ok(())
        } else {
            Err(format!("expected '{}', got 0x{got:02x}", want as char))
        }
    }

    fn parse_u64(&mut self) -> Result<u64, String> {
        self.skip_ws();
        let start = self.i;
        while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        if self.i == start {
            return Err("expected a number".to_string());
        }
        std::str::from_utf8(&self.b[start..self.i])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| "number out of range".to_string())
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.i).ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = *self.b.get(self.i).ok_or("unterminated escape")?;
                    self.i += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let cp = self.parse_hex4()?;
                            if (0xD800..=0xDBFF).contains(&cp) {
                                // Surrogate pair.
                                if self.b.get(self.i) != Some(&b'\\')
                                    || self.b.get(self.i + 1) != Some(&b'u')
                                {
                                    return Err("lone high surrogate".to_string());
                                }
                                self.i += 2;
                                let lo = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err("invalid low surrogate".to_string());
                                }
                                let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                out.push(char::from_u32(combined).ok_or("invalid surrogate pair")?);
                            } else if (0xDC00..=0xDFFF).contains(&cp) {
                                return Err("lone low surrogate".to_string());
                            } else {
                                out.push(char::from_u32(cp).ok_or("invalid codepoint")?);
                            }
                        }
                        other => return Err(format!("invalid escape 0x{other:02x}")),
                    }
                }
                c if c < 0x20 => return Err("raw control character in string".to_string()),
                _ => {
                    // Re-assemble the UTF-8 sequence starting at c.
                    let len = utf8_len(c).ok_or("invalid UTF-8 in string")?;
                    let start = self.i - 1;
                    let end = start + len;
                    if end > self.b.len() {
                        return Err("truncated UTF-8 sequence".to_string());
                    }
                    let s = std::str::from_utf8(&self.b[start..end])
                        .map_err(|_| "invalid UTF-8 in string".to_string())?;
                    out.push_str(s);
                    self.i = end;
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("truncated \\u escape".to_string());
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4])
            .map_err(|_| "invalid \\u escape".to_string())?;
        self.i += 4;
        u32::from_str_radix(s, 16).map_err(|_| "invalid \\u escape".to_string())
    }
}

fn utf8_len(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// base64url (no padding, RFC 4648 §5)
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3 + 2);
    for chunk in bytes.chunks(4) {
        let mut n: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_roundtrip() {
        for len in 0..40 {
            let data: Vec<u8> = (0..len as u8)
                .map(|i| i.wrapping_mul(37).wrapping_add(3))
                .collect();
            let enc = base64url_encode(&data);
            assert!(!enc.contains('='));
            assert_eq!(base64url_decode(&enc).unwrap(), data);
        }
    }

    #[test]
    fn token_roundtrip_and_validation() {
        let rev = DocumentRevision::from_source_bytes(b"pdf bytes");
        let payload = TokenPayload {
            v: 1,
            fp: rev.digest_hex(),
            ctr: 0,
            page: 1,
            ck: "p".into(),
            chr: [4, 9],
            sh: hash64_hex(b"match"),
            ch: hash64_hex(b"context"),
        };
        let token = encode_token(&payload);
        assert!(token.starts_with("pdfluent-match-v1."));
        assert_eq!(decode_token(&token).unwrap(), payload);

        assert!(decode_token("garbage").is_err());
        assert!(decode_token("pdfluent-match-v1.!!!").is_err());
    }

    #[test]
    fn revision_semantics() {
        let a = DocumentRevision::from_source_bytes(b"same");
        let b = DocumentRevision::from_source_bytes(b"same");
        assert_eq!(a, b, "byte-identical input yields the same fingerprint");
        assert_ne!(a, a.next(), "a commit produces a new revision");
        assert_ne!(a, DocumentRevision::from_source_bytes(b"other"));
    }
}
