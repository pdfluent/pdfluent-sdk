//! Layout-aware text replacement for the Node binding.
//!
//! `TextEditor` owns a [`pdfluent::PdfDocument`] rather than a raw lopdf
//! document, so licensing and the Trial-tier notice apply the same way they do
//! in the Rust SDK, the WASM binding and the Python binding. The older
//! surfaces in this crate predate that rule and talk to the engine crates
//! directly; new surfaces go through the facade.
//!
//! Matches and reports cross into JavaScript as JSON strings, and match ids
//! are opaque strings that survive a queue or a file between the find and the
//! apply — the asynchronous translate-then-replace workflow.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use pdfluent::text_edit::{
    CommitPolicy, FontFallback, MatchId, RegionRelation, ReplaceOptions, SignaturePolicy, TextQuery,
};

fn err(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Find and replace PDF text while preserving fonts, positioning and the
/// surrounding page content.
#[napi]
pub struct TextEditor {
    doc: pdfluent::PdfDocument,
}

#[napi]
impl TextEditor {
    /// Open a PDF from a Buffer.
    #[napi(factory)]
    pub fn open(data: Buffer) -> Result<Self> {
        let doc = pdfluent::PdfDocument::from_bytes(&data).map_err(err)?;
        Ok(Self { doc })
    }

    /// Number of pages.
    #[napi]
    pub fn page_count(&self) -> u32 {
        self.doc.page_count() as u32
    }

    /// Find text occurrences. `queryJson` is a JSON object:
    ///
    /// ```json
    /// {
    ///   "text": "Acme B.V.",
    ///   "caseInsensitive": false,
    ///   "pages": [1, 5],
    ///   "region": { "page": 1, "rect": [x0, y0, x1, y1], "relation": "intersects" },
    ///   "limit": 100
    /// }
    /// ```
    ///
    /// Returns a JSON array of matches; each carries an opaque `id` valid
    /// until the next applied edit on this document, and an `editable` flag.
    #[napi]
    pub fn find_text(&mut self, query_json: String) -> Result<String> {
        let query = parse_query(&query_json).map_err(napi::Error::from_reason)?;
        let matches = self.doc.find_text(query).map_err(err)?;
        serde_json::to_string(&matches).map_err(err)
    }

    /// Find and replace in one call. Returns the JSON replacement report.
    ///
    /// `optionsJson` may be `"{}"`; see the README for the option surface.
    #[napi]
    pub fn replace_text(
        &mut self,
        query_json: String,
        replacement: String,
        options_json: String,
    ) -> Result<String> {
        let query = parse_query(&query_json).map_err(napi::Error::from_reason)?;
        let options = parse_options(&options_json).map_err(napi::Error::from_reason)?;
        let report = self
            .doc
            .replace_text(query, &replacement, options)
            .map_err(err)?;
        serde_json::to_string(&report).map_err(err)
    }

    /// Apply replacements to matches located earlier with `findText`.
    ///
    /// `editsJson` is a JSON array of `{"id": "<match id>", "text": "<new>"}`.
    /// They commit as one transaction; by default any invalid edit aborts the
    /// whole batch and the document stays untouched.
    #[napi]
    pub fn replace_matches(&mut self, edits_json: String, options_json: String) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct EditIn {
            id: String,
            text: String,
        }
        let edits: Vec<EditIn> = serde_json::from_str(&edits_json)
            .map_err(|e| napi::Error::from_reason(format!("expected [{{id, text}}, …]: {e}")))?;
        let options = parse_options(&options_json).map_err(napi::Error::from_reason)?;
        let edits: Vec<(MatchId, String)> = edits
            .into_iter()
            .map(|e| (MatchId::from_token(e.id), e.text))
            .collect();
        let report = self
            .doc
            .replace_text_matches(&edits, options)
            .map_err(err)?;
        serde_json::to_string(&report).map_err(err)
    }

    /// Serialize the (possibly edited) document to a Buffer.
    #[napi]
    pub fn to_buffer(&self) -> Result<Buffer> {
        let bytes = self.doc.to_bytes().map_err(err)?;
        Ok(bytes.into())
    }

    /// Write the (possibly edited) document to `path`.
    #[napi]
    pub fn save(&self, path: String) -> Result<()> {
        self.doc.save(&path).map_err(err)
    }
}

// ---------------------------------------------------------------------------
// JSON input parsing (shared shape with the WASM binding)
// ---------------------------------------------------------------------------

pub(crate) fn parse_query(json: &str) -> std::result::Result<TextQuery, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RegionIn {
        page: u32,
        rect: [f64; 4],
        #[serde(default)]
        relation: Option<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct QueryIn {
        text: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        pages: Option<[u32; 2]>,
        #[serde(default)]
        region: Option<RegionIn>,
        #[serde(default)]
        limit: Option<u32>,
    }

    let q: QueryIn = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut query = TextQuery::exact(q.text).case_insensitive(q.case_insensitive);
    if let Some([lo, hi]) = q.pages {
        query = query.pages(lo..=hi);
    }
    if let Some(region) = q.region {
        let relation = match region.relation.as_deref() {
            None | Some("intersects") => RegionRelation::Intersects,
            Some("contained") => RegionRelation::Contained,
            Some(other) => return Err(format!("unknown region relation {other:?}")),
        };
        query = query.region_with(region.page, region.rect, relation);
    }
    if let Some(n) = q.limit {
        query = query.limit(n as usize);
    }
    Ok(query)
}

pub(crate) fn parse_options(json: &str) -> std::result::Result<ReplaceOptions, String> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum FallbackIn {
        Named(String),
        Explicit { explicit: String },
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct OptionsIn {
        #[serde(default)]
        font_fallback: Option<FallbackIn>,
        #[serde(default)]
        signature_policy: Option<String>,
        #[serde(default)]
        commit_policy: Option<String>,
    }

    let trimmed = json.trim();
    let o: OptionsIn = if trimmed.is_empty() {
        OptionsIn::default()
    } else {
        serde_json::from_str(trimmed).map_err(|e| e.to_string())?
    };

    let mut options = ReplaceOptions::default();
    match o.font_fallback {
        None => {}
        Some(FallbackIn::Named(name)) => match name.as_str() {
            "deny" => options.font_fallback = FontFallback::Deny,
            "injectStandard" => options.font_fallback = FontFallback::InjectStandard,
            other => return Err(format!("unknown fontFallback {other:?}")),
        },
        Some(FallbackIn::Explicit { explicit }) => {
            options.font_fallback = FontFallback::Explicit(explicit);
        }
    }
    match o.signature_policy.as_deref() {
        None | Some("reject") => {}
        Some("allowPostSignatureChange") => {
            options.signature_policy = SignaturePolicy::AllowPostSignatureChange;
        }
        Some(other) => return Err(format!("unknown signaturePolicy {other:?}")),
    }
    match o.commit_policy.as_deref() {
        None | Some("allOrNothing") => {}
        Some("bestEffort") => options.commit_policy = CommitPolicy::BestEffort,
        Some(other) => return Err(format!("unknown commitPolicy {other:?}")),
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_option_parsing_is_strict() {
        assert!(parse_query(r#"{"text":"a"}"#).is_ok());
        assert!(parse_query(r#"{"txet":"typo"}"#).is_err());
        assert!(parse_options("").is_ok());
        assert!(parse_options("{}").is_ok());
        assert!(parse_options(r#"{"commitPolicy":"yolo"}"#).is_err());
        let o = parse_options(
            r#"{"fontFallback":{"explicit":"F7"},"signaturePolicy":"allowPostSignatureChange","commitPolicy":"bestEffort"}"#,
        )
        .unwrap();
        assert_eq!(o.font_fallback, FontFallback::Explicit("F7".into()));
        assert_eq!(
            o.signature_policy,
            SignaturePolicy::AllowPostSignatureChange
        );
        assert_eq!(o.commit_policy, CommitPolicy::BestEffort);
    }
}
