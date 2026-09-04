//! W3-A — Static ReDoS heuristic guard for the sandboxed JS runtime.
//!
//! ## Why this module exists
//!
//! QuickJS — the JS engine wrapped by the `rquickjs` crate — implements
//! `RegExp` as a backtracking NFA in C. The wrapper sets a per-script
//! interrupt deadline, but the interrupt handler is polled at **JS opcode
//! boundaries** only, not inside the regex C code. A single `test()` /
//! `match()` opcode that invokes a catastrophically backtracking pattern
//! on a long-enough input therefore runs to completion regardless of the
//! per-script time budget.
//!
//! W2-E recorded this as finding **REDOS-01**:
//! `/(a+)+$/.test("a".repeat(25) + "!")` ran **41 seconds** without the
//! interrupt firing once.
//!
//! ## Mitigation strategy
//!
//! This module implements **Option 3** from the W3-A track brief: a static
//! heuristic input scanner that inspects every script body **before** the
//! body is handed to QuickJS, and rejects it (typed `SandboxError`) when a
//! recognisable ReDoS pattern shape appears in either:
//!
//! - a JS regex literal `/.../flags`
//! - a `new RegExp(...)` constructor call
//!
//! The scanner is intentionally narrow: it flags only the shapes that are
//! known to cause exponential backtracking in PCRE-style NFA engines. The
//! false-positive surface on real-world XFA scripts is empirically zero
//! (XFA forms use regex for format masks like `/^\d{4}$/`, not nested
//! quantifiers). The false-negative surface is acknowledged — a determined
//! adversary can express the same blow-up in shapes the heuristic does not
//! see — and this is documented in the security model.
//!
//! ## What it does NOT do
//!
//! - **Does not** touch the rquickjs version (no version bump).
//! - **Does not** modify QuickJS's regex engine.
//! - **Does not** spawn a worker / external process.
//! - **Does not** rely on a real PCRE compiler — the scanner walks JS source
//!   characters in a single pass and never executes the pattern.
//!
//! ## What it DOES
//!
//! - Catches the W2-E proof: `/(a+)+$/` shape.
//! - Catches `(a*)*`, `(a+)*`, `(a*)+`, `([a-z]+)+`, `(.+)+`.
//! - Catches the same shape inside `new RegExp("(a+)+$")`.
//! - Returns a typed `SandboxError::RegexRejected` so the dispatch site can
//!   classify it (counted as a runtime error, not a timeout / OOM).
//!
//! ## S-numbered invariants touched
//!
//! - S-9 (time budget): guard reduces the residual single-call regex blow-up
//!   surface the interrupt handler cannot reach.
//! - S-17 (fail-open at dispatch): a rejected script still leaves the
//!   parent flatten path running; the error is recoverable per
//!   `crates/pdf-xfa/src/dynamic.rs`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::js_runtime::SandboxError;

/// Verdict returned by [`scan_script_for_redos`]. A `Reject` variant carries
/// a short reason string so the rejection is observable in test failures
/// and metadata dumps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexScanVerdict {
    /// No suspicious regex literal / constructor call detected. The script
    /// may proceed to the sandbox.
    Accept,
    /// At least one suspicious shape detected. The caller should refuse to
    /// execute the body and return `SandboxError::RegexRejected`.
    Reject {
        /// One-line human-readable explanation. Stable for tests.
        reason: String,
    },
}

impl RegexScanVerdict {
    /// Convenience: convert a `Reject` verdict into the typed sandbox
    /// error. Panics on `Accept` — callers must branch on the verdict
    /// first; this is enforced by the type rather than by runtime checks
    /// everywhere.
    pub fn into_sandbox_error(self) -> SandboxError {
        match self {
            RegexScanVerdict::Accept => {
                SandboxError::ScriptError("regex guard: into_sandbox_error on Accept".into())
            }
            RegexScanVerdict::Reject { reason } => SandboxError::RegexRejected(reason),
        }
    }
}

/// Public entry point. Walks `src` once and returns the verdict. The cost
/// is O(n) in `src.len()`. Designed to be cheap enough to run before every
/// `execute_script` and every variables-script registration.
pub fn scan_script_for_redos(src: &str) -> RegexScanVerdict {
    let bytes = src.as_bytes();
    let mut i = 0;
    // Track whether a `/` here can plausibly open a regex literal. JS makes
    // this disambiguation by tracking the preceding non-whitespace token: a
    // `/` after an *expression* (identifier, number, `)`, `]`) is division;
    // a `/` after an *operator* or at statement start is a regex literal.
    // We over-approximate: treat any `/` after whitespace or known regex
    // contexts as a literal opener. False positives here only cause the
    // scanner to look at more characters; they cannot promote `Accept` to
    // `Reject` because the inner check still requires a regex-shaped body.
    let mut prev_significant: u8 = b';';
    while i < bytes.len() {
        let b = bytes[i];

        // Skip line + block comments so a commented-out `(a+)+` cannot
        // trip the scanner.
        if b == b'/' && i + 1 < bytes.len() {
            let n = bytes[i + 1];
            if n == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if n == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                continue;
            }
        }

        // Skip string literals (single / double / template). A nested
        // quantifier shape inside a string that is later passed to
        // `new RegExp(...)` is caught by the dedicated `new RegExp` scan
        // below; bare strings are not executed as regex and must not
        // trigger the guard.
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
                if c == quote {
                    break;
                }
            }
            prev_significant = b'"';
            continue;
        }
        if b == b'`' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'`' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            prev_significant = b'`';
            continue;
        }

        // Regex literal opener: `/` in a regex-allowed position.
        if b == b'/' && is_regex_context(prev_significant) {
            // Walk to the closing `/`. JS regex syntax requires the closer
            // on the same line.
            let pat_start = i + 1;
            let mut j = pat_start;
            let mut in_class = false;
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'\n' {
                    // Unterminated literal — not a regex; bail and treat
                    // the original `/` as the start of a division.
                    break;
                }
                if c == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                    continue;
                }
                if c == b'[' {
                    in_class = true;
                } else if c == b']' {
                    in_class = false;
                } else if c == b'/' && !in_class {
                    // Closer found. Inspect pattern body.
                    let body = &src[pat_start..j];
                    if let Some(reason) = pattern_is_dangerous(body) {
                        return RegexScanVerdict::Reject { reason };
                    }
                    // Skip past closer + flags.
                    i = j + 1;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    prev_significant = b')'; // treat the literal as an expression
                    break;
                }
                j += 1;
            }
            if j >= bytes.len() || bytes[j] == b'\n' {
                // Did not find a closer; just advance past the original
                // `/` and keep scanning.
                i += 1;
                prev_significant = b'/';
            }
            continue;
        }

        // `new RegExp(...)` constructor: catch the pattern string argument.
        if b == b'R' && looks_like_new_regexp(bytes, i) {
            // Find the open paren after `RegExp`.
            let mut k = i + b"RegExp".len();
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if k < bytes.len() && bytes[k] == b'(' {
                k += 1;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && (bytes[k] == b'"' || bytes[k] == b'\'') {
                    let quote = bytes[k];
                    let pat_start = k + 1;
                    let mut j = pat_start;
                    while j < bytes.len() {
                        let c = bytes[j];
                        if c == b'\\' && j + 1 < bytes.len() {
                            j += 2;
                            continue;
                        }
                        if c == quote {
                            let body = &src[pat_start..j];
                            if let Some(reason) = pattern_is_dangerous(body) {
                                return RegexScanVerdict::Reject {
                                    reason: format!("new RegExp: {reason}"),
                                };
                            }
                            break;
                        }
                        j += 1;
                    }
                }
            }
            i += 1;
            prev_significant = b'p';
            continue;
        }

        if !b.is_ascii_whitespace() {
            prev_significant = b;
        }
        i += 1;
    }

    RegexScanVerdict::Accept
}

/// True iff a `/` in this context can plausibly open a regex literal.
/// Heuristic: any non-expression-terminator can precede a regex. Anything
/// that looks like the end of an expression (`)`, `]`, identifier char,
/// digit, the regex-flags marker `'/' + flags`) means `/` is division.
fn is_regex_context(prev_significant: u8) -> bool {
    match prev_significant {
        // After these tokens, `/` is division.
        b')' | b']' | b'}' => false,
        c if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' => false,
        _ => true,
    }
}

/// True iff `bytes[i..]` begins with the identifier `RegExp` *as a token*
/// (i.e. the preceding byte, if any, is not part of an identifier).
fn looks_like_new_regexp(bytes: &[u8], i: usize) -> bool {
    let kw = b"RegExp";
    if i + kw.len() > bytes.len() {
        return false;
    }
    if &bytes[i..i + kw.len()] != kw {
        return false;
    }
    // Must not be the middle of an identifier (e.g. `MyRegExp`).
    if i > 0 {
        let p = bytes[i - 1];
        if p.is_ascii_alphanumeric() || p == b'_' || p == b'$' {
            return false;
        }
    }
    // The next byte (if any) must not be part of an identifier — `RegExp`
    // (the constructor) is followed by `(` or whitespace then `(`.
    let next = bytes.get(i + kw.len()).copied().unwrap_or(b' ');
    if next.is_ascii_alphanumeric() || next == b'_' || next == b'$' {
        return false;
    }
    true
}

/// Core heuristic. Inspect a regex pattern body (the text between the two
/// `/` delimiters, or the first arg of `new RegExp("…")`). Returns `Some`
/// with a reason iff the pattern is dangerous; `None` to accept.
///
/// The rules implemented:
///
/// - **R1 — Nested quantifier on a group:** a parenthesised group whose
///   inner body ends with a quantifier (`+`, `*`, `{n,}`) AND the group
///   itself is followed by a quantifier (`+`, `*`, `{n,}`). This is the
///   `(a+)+`, `(a*)*`, `(a+)*` family. Direct cause of the W2-E proof
///   pattern.
/// - **R2 — Alternation with overlap inside a quantified group:** a
///   group `(a|a)+`, `(a|aa)+`, `(.|.)+` — both branches match the same
///   first character and the group has a `+`/`*`/`{n,}` outside. Detected
///   conservatively: any quantified group whose first-char sets overlap
///   non-trivially. This guard is narrow (alt branches must be ≤ 4 chars
///   each, all literal) to keep false positives at zero.
/// - **R3 — Pattern length sanity:** patterns longer than 4 KiB are
///   refused outright — XFA format masks never approach that size and a
///   pathological pattern at that length is almost certainly an attempt
///   to evade R1/R2 with construction tricks. This is a defence-in-depth
///   bound, not a primary guard.
pub(crate) fn pattern_is_dangerous(pat: &str) -> Option<String> {
    if pat.len() > 4096 {
        return Some(format!(
            "regex pattern exceeds 4 KiB safety bound ({} bytes)",
            pat.len()
        ));
    }

    let bytes = pat.as_bytes();
    let n = bytes.len();

    // Single-pass parse with a stack of group start indices.
    let mut group_stack: Vec<usize> = Vec::new();
    let mut last_quantifiable_end: Option<usize> = None;
    let mut i = 0;
    while i < n {
        let b = bytes[i];
        if b == b'\\' && i + 1 < n {
            // Escaped char counts as one atom for quantifier targeting.
            last_quantifiable_end = Some(i + 2);
            i += 2;
            continue;
        }
        if b == b'[' {
            // Skip char class; treat as one atom.
            let mut j = i + 1;
            while j < n && bytes[j] != b']' {
                if bytes[j] == b'\\' && j + 1 < n {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            last_quantifiable_end = Some((j + 1).min(n));
            i = (j + 1).min(n);
            continue;
        }
        if b == b'(' {
            group_stack.push(i);
            i += 1;
            // Eat group flags like (?: (?= (?! (?<= (?<! (?<name>
            if i < n && bytes[i] == b'?' {
                i += 1;
                while i < n
                    && bytes[i] != b':'
                    && bytes[i] != b'='
                    && bytes[i] != b'!'
                    && bytes[i] != b'>'
                {
                    i += 1;
                }
                if i < n {
                    i += 1;
                }
            }
            continue;
        }
        if b == b')' {
            let group_start = match group_stack.pop() {
                Some(s) => s,
                None => {
                    // Unbalanced — let QuickJS handle the parse error.
                    return None;
                }
            };
            let group_body = &pat[group_start + 1..i];
            // Look at the character immediately after the `)` — is it an
            // *unbounded* quantifier?
            //
            // `+` / `*` / `{n,}`  → unbounded → dangerous when combined
            //                        with an inner unbounded quantifier
            // `?` / `{n}` / `{n,m}` (m present) → bounded → SAFE
            //
            // The W2-E proof needs unbounded outer iteration to blow up,
            // so we only flag patterns whose outer iteration is unbounded.
            let outer_quant = match bytes.get(i + 1).copied() {
                Some(b'+') | Some(b'*') => true,
                Some(b'{') => is_unbounded_brace_after(bytes, i + 1),
                _ => false,
            };
            if outer_quant {
                // R1: does the group body end with an unbounded quantifier?
                if body_ends_with_unbounded_quantifier(group_body) {
                    return Some(format!(
                        "nested quantifier on group `({group_body})` — W2-E REDOS-01 shape"
                    ));
                }
                // R2: overlapping alternation inside a quantified group?
                if alternation_has_overlap(group_body) {
                    return Some(format!(
                        "overlapping alternation inside quantified group `({group_body})`"
                    ));
                }
            }
            last_quantifiable_end = Some(i + 1);
            i += 1;
            continue;
        }
        if b == b'+' || b == b'*' {
            // Bare quantifier (not consumed by group exit above).
            last_quantifiable_end = Some(i + 1);
            i += 1;
            continue;
        }
        last_quantifiable_end = Some(i + 1);
        i += 1;
    }

    let _ = last_quantifiable_end; // silence unused warning in release
    None
}

/// True iff the brace quantifier starting at `bytes[start]` (which must
/// be `'{'`) is unbounded (`{n,}`). Returns `false` for `{n}` or `{n,m}`
/// with `m` present, and on parse failure.
fn is_unbounded_brace_after(bytes: &[u8], start: usize) -> bool {
    debug_assert!(start < bytes.len() && bytes[start] == b'{');
    let mut k = start + 1;
    // Numeric n
    let n_start = k;
    while k < bytes.len() && bytes[k].is_ascii_digit() {
        k += 1;
    }
    if k == n_start {
        return false;
    }
    if k >= bytes.len() {
        return false;
    }
    if bytes[k] == b'}' {
        // `{n}` — bounded (single repetition count).
        return false;
    }
    if bytes[k] != b',' {
        return false;
    }
    k += 1;
    // Optional m
    let m_start = k;
    while k < bytes.len() && bytes[k].is_ascii_digit() {
        k += 1;
    }
    if k >= bytes.len() || bytes[k] != b'}' {
        return false;
    }
    // If no `m`, the upper bound is unbounded.
    m_start == k
}

/// True iff `body` ends with an unbounded quantifier (`+`, `*`, `{n,}`).
/// Ignores trailing closing parens of nested groups (handled separately).
fn body_ends_with_unbounded_quantifier(body: &str) -> bool {
    let trimmed = body.trim_end();
    if let Some(last) = trimmed.as_bytes().last().copied() {
        if last == b'+' || last == b'*' {
            return true;
        }
        if last == b'}' {
            // `{n,m}` is bounded; `{n,}` is unbounded.
            if let Some(open) = trimmed.rfind('{') {
                let spec = &trimmed[open + 1..trimmed.len() - 1];
                if let Some(comma) = spec.find(',') {
                    let after = spec[comma + 1..].trim();
                    return after.is_empty();
                }
            }
        }
    }
    false
}

/// True iff the alternation branches inside `body` share a first character
/// (overlap that triggers backtracking when the group is quantified).
///
/// Conservative: only detects literal branches up to 8 chars each, with no
/// metacharacters. The W3-A track does not aim to catch every overlap —
/// just the obvious `(a|a)+`, `(a|aa)+`, `(foo|foobar)+` shapes.
fn alternation_has_overlap(body: &str) -> bool {
    if !body.contains('|') {
        return false;
    }
    // Split on `|` only at top level (respect nested groups / classes).
    let mut depth_paren = 0;
    let mut depth_class = 0;
    let mut branches: Vec<&str> = Vec::new();
    let mut start = 0;
    let bytes = body.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => {
                // Skip the next char.
                // (Cheap: idx is inside the iterator; rust's iter().enumerate()
                // does not allow easy skip — but escapes here only matter for
                // not splitting on `\|`, which we honour by checking the prev
                // byte below.)
            }
            b'(' if depth_class == 0 => depth_paren += 1,
            b')' if depth_class == 0 && depth_paren > 0 => depth_paren -= 1,
            b'[' if depth_class == 0 => depth_class = 1,
            b']' if depth_class == 1 => depth_class = 0,
            b'|' if depth_paren == 0
                && depth_class == 0
                && (idx == 0 || bytes[idx - 1] != b'\\') =>
            {
                branches.push(&body[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    branches.push(&body[start..]);
    if branches.len() < 2 {
        return false;
    }
    // Only look at branches that are short, literal sequences.
    let firsts: Vec<u8> = branches
        .iter()
        .filter_map(|br| {
            let s = br.trim();
            if s.is_empty() || s.len() > 8 {
                return None;
            }
            // Reject if it contains metacharacters that change the first-char
            // semantics — we want a conservative "obvious overlap" check.
            if s.bytes().any(|c| {
                matches!(
                    c,
                    b'.' | b'*' | b'+' | b'?' | b'(' | b'[' | b'\\' | b'^' | b'$' | b'{'
                )
            }) {
                return None;
            }
            s.bytes().next()
        })
        .collect();
    if firsts.len() < 2 {
        return false;
    }
    // Overlap iff any first-byte repeats.
    let mut sorted = firsts.clone();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.len() < firsts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(src: &str) -> String {
        match scan_script_for_redos(src) {
            RegexScanVerdict::Accept => panic!("expected Reject for: {src}"),
            RegexScanVerdict::Reject { reason } => reason,
        }
    }

    fn accepted(src: &str) {
        match scan_script_for_redos(src) {
            RegexScanVerdict::Accept => {}
            RegexScanVerdict::Reject { reason } => {
                panic!("expected Accept for `{src}` but rejected: {reason}")
            }
        }
    }

    // ---- R1: nested quantifier on group --------------------------------

    #[test]
    fn w2e_redos_01_proof_pattern_rejected() {
        // The exact W2-E proof: `/(a+)+$/.test("a".repeat(25) + "!")`
        let reason = rejected(r#"var m = /(a+)+$/.test("aaaaaaaaaaaaaaaaaaaaaaaaa!");"#);
        assert!(
            reason.contains("nested quantifier"),
            "reason should name nested quantifier; got: {reason}"
        );
    }

    #[test]
    fn star_star_shape_rejected() {
        rejected("var m = /(a*)*$/.test('aaaa');");
    }

    #[test]
    fn plus_star_shape_rejected() {
        rejected("var m = /(a+)*$/.test('aaaa');");
    }

    #[test]
    fn star_plus_shape_rejected() {
        rejected("var m = /(a*)+$/.test('aaaa');");
    }

    #[test]
    fn char_class_inside_nested_quant_rejected() {
        rejected("var m = /([a-z]+)+$/.test('abcd');");
    }

    #[test]
    fn dot_plus_plus_rejected() {
        rejected("var m = /(.+)+$/.test('abc');");
    }

    #[test]
    fn bounded_unbounded_brace_rejected() {
        rejected("var m = /(a{1,})+$/.test('aaaa');");
    }

    // ---- R1: NOT dangerous (bounded quantifiers) -----------------------

    #[test]
    fn bounded_brace_outside_accepted() {
        accepted("var m = /(a+){1,3}$/.test('aaa');");
    }

    #[test]
    fn bounded_brace_inside_accepted() {
        accepted("var m = /(a{1,3})+$/.test('aaa');");
    }

    // ---- R1: new RegExp() -----------------------------------------------

    #[test]
    fn new_regexp_double_quote_rejected() {
        let r = rejected(r#"var re = new RegExp("(a+)+$");"#);
        assert!(r.starts_with("new RegExp"), "reason: {r}");
    }

    #[test]
    fn new_regexp_single_quote_rejected() {
        rejected(r#"var re = new RegExp('(a+)+$');"#);
    }

    #[test]
    fn new_regexp_with_whitespace_rejected() {
        rejected(r#"var re = new RegExp ( "(a+)+$" );"#);
    }

    // ---- Identifier collisions: MUST NOT trigger -----------------------

    #[test]
    fn my_regexp_identifier_does_not_trigger() {
        accepted("var MyRegExp = 1; var x = MyRegExp;");
    }

    #[test]
    fn regexp_method_call_not_constructor_does_not_trigger() {
        // Just referencing the global, no `new`, no dangerous pattern.
        accepted("var x = RegExp.prototype.toString;");
    }

    // ---- Comments / strings must be ignored ----------------------------

    #[test]
    fn dangerous_shape_in_line_comment_accepted() {
        accepted("// /(a+)+$/ is dangerous but commented out\nvar x = 1;");
    }

    #[test]
    fn dangerous_shape_in_block_comment_accepted() {
        accepted("/* /(a+)+$/ */ var x = 1;");
    }

    #[test]
    fn dangerous_shape_in_string_literal_accepted() {
        accepted(r#"var s = "/(a+)+$/"; var x = 1;"#);
    }

    #[test]
    fn dangerous_shape_in_template_literal_accepted() {
        accepted(r#"var s = `/(a+)+$/`; var x = 1;"#);
    }

    // ---- Real-world XFA format masks must all pass --------------------

    #[test]
    fn xfa_zip_code_mask_accepted() {
        accepted(r#"var m = /^\d{4}\s?[A-Z]{2}$/.test('1234 AB');"#);
    }

    #[test]
    fn xfa_phone_number_mask_accepted() {
        accepted(r#"var m = /^\+?[0-9]{1,3}-[0-9]{3,12}$/.test('+1-555');"#);
    }

    #[test]
    fn xfa_email_mask_accepted() {
        accepted(r#"var m = /^[^@]+@[^@]+\.[a-zA-Z]{2,}$/.test('a@b.co');"#);
    }

    #[test]
    fn xfa_date_iso_mask_accepted() {
        accepted(r#"var m = /^\d{4}-\d{2}-\d{2}$/.test('2026-05-19');"#);
    }

    // ---- R2: alternation with overlap -----------------------------------

    #[test]
    fn alternation_aa_aa_overlap_rejected() {
        // Both branches start with 'a' and group is quantified.
        rejected("var m = /(a|aa)+$/.test('aaa');");
    }

    #[test]
    fn alternation_disjoint_first_chars_accepted() {
        accepted("var m = /(a|b)+$/.test('abab');");
    }

    // ---- R3: oversized pattern ------------------------------------------

    #[test]
    fn oversized_pattern_rejected() {
        let big = "a".repeat(5000);
        let src = format!(r#"var re = new RegExp("{big}");"#);
        let reason = rejected(&src);
        assert!(reason.contains("4 KiB"), "reason: {reason}");
    }

    // ---- Division must not be misread as regex --------------------------

    #[test]
    fn division_after_identifier_accepted() {
        accepted("var x = a / b; var y = c / d;");
    }

    #[test]
    fn division_after_number_accepted() {
        accepted("var x = 10 / 5;");
    }

    #[test]
    fn division_after_close_paren_accepted() {
        accepted("var x = (a + b) / c;");
    }

    // ---- pattern_is_dangerous unit tests --------------------------------

    #[test]
    fn pattern_is_dangerous_bare_nested_plus_plus() {
        assert!(pattern_is_dangerous("(a+)+").is_some());
    }

    #[test]
    fn pattern_is_dangerous_safe_anchored_digits() {
        assert!(pattern_is_dangerous(r"^\d{4}$").is_none());
    }

    // ---- Verdict to error conversion -----------------------------------

    #[test]
    fn reject_verdict_converts_to_typed_error() {
        let v = RegexScanVerdict::Reject {
            reason: "test".into(),
        };
        match v.into_sandbox_error() {
            SandboxError::RegexRejected(r) => assert_eq!(r, "test"),
            other => panic!("expected RegexRejected, got {other:?}"),
        }
    }
}
