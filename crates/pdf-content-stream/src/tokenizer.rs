//! Low-level byte scanner for PDF content streams.
//!
//! Tracks exact byte offsets so `ParsedOp` byte ranges can be used
//! for byte-identical round-trip serialization.

use crate::error::ContentStreamError;
use crate::ops::RawOperand;

/// Byte-tracking scanner over a PDF content stream slice.
pub(crate) struct Tokenizer<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub(crate) fn advance(&mut self) -> Option<u8> {
        let b = self.data.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    /// Skip PDF whitespace (`\0 \t \n \r \f \x20`) and comments (`%...`).
    pub(crate) fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(b'\0' | b'\t' | b'\n' | b'\r' | 0x0C | b' ') => {
                    self.pos += 1;
                }
                Some(b'%') => {
                    // Comment: skip until end of line.
                    self.pos += 1;
                    while let Some(b) = self.peek() {
                        self.pos += 1;
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// Read a PDF literal string `(...)`, returning raw bytes (with escapes
    /// left as-is so round-trip is byte-identical when we re-emit the raw input).
    ///
    /// The returned bytes are the DECODED content (escapes processed).
    /// The raw bytes (for round-trip) come from the byte ranges tracked by the caller.
    pub(crate) fn read_literal_string(&mut self) -> Result<Vec<u8>, ContentStreamError> {
        let start = self.pos;
        debug_assert_eq!(self.data[self.pos - 1], b'(');

        let mut result = Vec::new();
        let mut depth = 1i32;

        loop {
            match self.advance() {
                None => return Err(ContentStreamError::UnterminatedString { at: start }),
                Some(b'(') => {
                    depth += 1;
                    result.push(b'(');
                }
                Some(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    result.push(b')');
                }
                Some(b'\\') => {
                    match self.advance() {
                        None => return Err(ContentStreamError::UnterminatedString { at: start }),
                        Some(b'n') => result.push(b'\n'),
                        Some(b'r') => result.push(b'\r'),
                        Some(b't') => result.push(b'\t'),
                        Some(b'b') => result.push(0x08),
                        Some(b'f') => result.push(0x0C),
                        Some(b'(') => result.push(b'('),
                        Some(b')') => result.push(b')'),
                        Some(b'\\') => result.push(b'\\'),
                        Some(b'\r') => {
                            // Line continuation: \<CR> or \<CR><LF>
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        Some(b'\n') => {} // Line continuation
                        Some(d @ b'0'..=b'7') => {
                            // Octal escape: up to 3 digits.
                            let mut val = (d - b'0') as u32;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(c @ b'0'..=b'7') => {
                                        val = val * 8 + (c - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            result.push((val & 0xFF) as u8);
                        }
                        Some(c) => result.push(c), // Unknown escape → literal
                    }
                }
                Some(c) => result.push(c),
            }
        }
        Ok(result)
    }

    /// Read a hex string `<...>`, returning decoded bytes.
    pub(crate) fn read_hex_string(&mut self) -> Result<Vec<u8>, ContentStreamError> {
        let start = self.pos;
        debug_assert_eq!(self.data[self.pos - 1], b'<');

        let mut result = Vec::new();
        let mut hi: Option<u8> = None;

        loop {
            match self.advance() {
                None => return Err(ContentStreamError::UnterminatedHexString { at: start }),
                Some(b'>') => {
                    if let Some(h) = hi.take() {
                        result.push(h << 4); // Trailing nibble treated as 0.
                    }
                    break;
                }
                Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0C) => {} // Skip whitespace
                Some(c) => {
                    let nibble = hex_nibble(c).ok_or(ContentStreamError::UnexpectedByte {
                        byte: c,
                        at: self.pos - 1,
                    })?;
                    if let Some(h) = hi.take() {
                        result.push((h << 4) | nibble);
                    } else {
                        hi = Some(nibble);
                    }
                }
            }
        }
        Ok(result)
    }

    /// Read a name `/Name`, returning the bytes after the `/`.
    pub(crate) fn read_name(&mut self) -> Vec<u8> {
        debug_assert_eq!(self.data[self.pos - 1], b'/');
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_whitespace(c) || is_delimiter(c) {
                break;
            }
            self.pos += 1;
        }
        // Decode #XX hex sequences in name
        let raw = &self.data[start..self.pos];
        decode_name(raw)
    }

    /// Read a number starting at the current position (current byte is the first digit/sign/dot).
    pub(crate) fn read_number(&mut self) -> Result<f32, ContentStreamError> {
        let start = self.pos - 1; // caller already consumed the first byte
                                  // Consume digits, sign, dot.
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'+' || c == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let token = &self.data[start..self.pos];
        let s = std::str::from_utf8(token).map_err(|_| ContentStreamError::MalformedNumber {
            at: start,
            token: String::from_utf8_lossy(token).into_owned(),
        })?;
        s.parse::<f32>()
            .map_err(|_| ContentStreamError::MalformedNumber {
                at: start,
                token: s.to_owned(),
            })
    }

    /// Read an operator keyword (letters, digits after first, or special chars like `*`, `'`, `"`).
    pub(crate) fn read_operator(&mut self) -> Vec<u8> {
        let start = self.pos - 1;
        while let Some(c) = self.peek() {
            if is_whitespace(c) || is_delimiter(c) {
                break;
            }
            self.pos += 1;
        }
        self.data[start..self.pos].to_vec()
    }

    /// Read an array `[...]`, returning a vec of `RawOperand`.
    pub(crate) fn read_array(&mut self) -> Result<Vec<RawOperand>, ContentStreamError> {
        let start = self.pos;
        debug_assert_eq!(self.data[self.pos - 1], b'[');

        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => return Err(ContentStreamError::UnterminatedArray { at: start }),
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                Some(b'(') => {
                    self.pos += 1;
                    let s = self.read_literal_string()?;
                    items.push(RawOperand::String(s));
                }
                Some(b'<') => {
                    self.pos += 1;
                    // Could be << (dict) but arrays in content streams use <hex>
                    let s = self.read_hex_string()?;
                    items.push(RawOperand::String(s));
                }
                Some(b'/') => {
                    self.pos += 1;
                    let n = self.read_name();
                    items.push(RawOperand::Name(n));
                }
                Some(c) if c == b'+' || c == b'-' || c == b'.' || c.is_ascii_digit() => {
                    self.pos += 1;
                    let n = self.read_number()?;
                    items.push(RawOperand::Number(n));
                }
                Some(c) => {
                    return Err(ContentStreamError::UnexpectedByte {
                        byte: c,
                        at: self.pos,
                    });
                }
            }
        }
        Ok(items)
    }

    /// Read one operand from the current position.
    ///
    /// Returns `None` if the next token is an operator (not an operand).
    pub(crate) fn try_read_operand(&mut self) -> Result<Option<RawOperand>, ContentStreamError> {
        self.skip_whitespace();
        match self.peek() {
            None => Ok(None),
            Some(b'/') => {
                self.pos += 1;
                let n = self.read_name();
                Ok(Some(RawOperand::Name(n)))
            }
            Some(b'(') => {
                self.pos += 1;
                let s = self.read_literal_string()?;
                Ok(Some(RawOperand::String(s)))
            }
            Some(b'<') => {
                self.pos += 1;
                if self.peek() == Some(b'<') {
                    // << starts a dict — treat as operator territory; leave for operator reader.
                    self.pos -= 1;
                    Ok(None)
                } else {
                    let s = self.read_hex_string()?;
                    Ok(Some(RawOperand::String(s)))
                }
            }
            Some(b'[') => {
                self.pos += 1;
                let arr = self.read_array()?;
                Ok(Some(RawOperand::Array(arr)))
            }
            Some(c) if c == b'+' || c == b'-' || c == b'.' || c.is_ascii_digit() => {
                self.pos += 1;
                let n = self.read_number()?;
                Ok(Some(RawOperand::Number(n)))
            }
            // Everything else is an operator.
            _ => Ok(None),
        }
    }

    /// Read one operator token (after whitespace is already skipped).
    ///
    /// Inline-image `BI` blocks are consumed entirely including the `EI` so that
    /// the caller gets a single `Operator(b"BI")` with byte_end past `EI`.
    pub(crate) fn read_operator_keyword(&mut self) -> Vec<u8> {
        let name = self.read_operator();
        if name == b"BI" {
            self.skip_inline_image_body();
        }
        name
    }

    /// Skip past the image data + `EI` for an inline image.
    ///
    /// Scans for `EI` preceded by a whitespace-or-binary boundary.  The body
    /// is opaque to the G3 parser; capturing the raw bytes in `byte_end` is
    /// sufficient for round-trip.
    fn skip_inline_image_body(&mut self) {
        // Consume until EI.  Look for `EI` followed by whitespace or end.
        while self.pos + 2 <= self.data.len() {
            if &self.data[self.pos..self.pos + 2] == b"EI" {
                let after = self.pos + 2;
                let delimited = after >= self.data.len()
                    || is_whitespace(self.data[after])
                    || is_delimiter(self.data[after]);
                if delimited {
                    self.pos = after;
                    return;
                }
            }
            self.pos += 1;
        }
        // If EI not found, consume to end.
        self.pos = self.data.len();
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b'\0' | b'\t' | b'\n' | b'\r' | 0x0C | b' ')
}

#[inline]
fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

#[inline]
fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn decode_name(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#' && i + 2 < raw.len() {
            if let (Some(hi), Some(lo)) = (hex_nibble(raw[i + 1]), hex_nibble(raw[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    out
}
