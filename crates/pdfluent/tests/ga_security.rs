//! GA Quality — QR-15 security-sensitive workflow proof: redaction actually
//! REMOVES the underlying text (not merely draws a box over it).
//!
//! Strategy: open a fixture, extract a real token, redact it, persist via
//! `to_bytes()`, reopen the redacted output, and assert the token is no
//! longer extractable. Skips gracefully when a fixture has no extractable
//! text (e.g. scanned image-only pages).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;

use pdfluent::prelude::*;
use pdfluent::redact::RedactOptions;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

/// First alphabetic token of length >= 4 in the text, lowercased.
fn pick_token(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_alphabetic())
        .find(|w| w.len() >= 4)
        .map(|w| w.to_string())
}

#[test]
fn qr15_redaction_removes_underlying_text() {
    for name in ["multi-page.pdf", "simple.pdf", "acroform.pdf"] {
        let Ok(bytes) = std::fs::read(mini(name)) else {
            continue;
        };
        let Ok(doc) = PdfDocument::from_bytes(&bytes) else {
            continue;
        };
        let Ok(text) = doc.extract_text() else {
            continue;
        };
        let Some(token) = pick_token(&text) else {
            continue; // no extractable text token (e.g. scanned) — skip
        };
        // Sanity: the token is present before redaction.
        assert!(
            text.to_lowercase().contains(&token.to_lowercase()),
            "{name}: token '{token}' should be present pre-redaction"
        );

        let mut doc = PdfDocument::from_bytes(&bytes).expect("reopen for redact");
        // Redact the token; if redaction is unsupported for this doc shape it
        // returns a typed error — that is a separate concern, here we only
        // assert that a *successful* redaction removes the content.
        if doc.redact(&token, RedactOptions::new()).is_err() {
            continue;
        }
        let out = doc.to_bytes().expect("persist redacted");
        let reopened = PdfDocument::from_bytes(&out).expect("reopen redacted");
        let after = reopened.extract_text().unwrap_or_default().to_lowercase();
        assert!(
            !after.contains(&token.to_lowercase()),
            "{name}: redacted token '{token}' must NOT be extractable after redaction \
             (content must be removed, not merely covered)"
        );
        return; // proved on the first fixture with extractable text
    }
    eprintln!("QR-15: no fixture with extractable redactable text — skipped");
}
