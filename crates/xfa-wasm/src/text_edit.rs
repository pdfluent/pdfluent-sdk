//! Layout-aware text replacement for the WASM binding (SDK surface).
//!
//! This handle wraps the `pdfluent::PdfDocument` facade, so licensing rides
//! along automatically: [`crate::license::activate_license_key`] sets the
//! process-global tier, Trial-tier edits stamp a "PDFluent trial" notice on
//! each modified page, and licensed tiers edit without the notice.
//!
//! All structured values cross the JS boundary as JSON strings: matches and
//! reports serialize with their canonical field names, and `MatchId` tokens
//! are opaque strings that survive `postMessage`/storage round-trips (the
//! asynchronous find → translate → apply workflow).
//!
//! This is deliberately a separate handle from `PdfEditHandle`
//! (`edit_handle.rs`): that one is the free desktop editor's edit path and
//! stays notice-free; this one is the licensed SDK surface.

use wasm_bindgen::prelude::*;

use pdfluent::text_edit::{
    CommitPolicy, FontFallback, MatchId, RegionRelation, ReplaceOptions, SignaturePolicy, TextQuery,
};

use crate::wasm_err_with_op;

/// Find-and-replace editor over a PDF document (SDK surface, licensed).
#[wasm_bindgen]
pub struct TextEditor {
    doc: pdfluent::PdfDocument,
}

#[wasm_bindgen]
impl TextEditor {
    /// Open a PDF from raw bytes.
    pub fn open(data: &[u8]) -> Result<TextEditor, JsValue> {
        let doc = pdfluent::PdfDocument::from_bytes(data)
            .map_err(|e| wasm_err_with_op("INVALID_PDF", &e.to_string(), "TextEditor.open"))?;
        Ok(TextEditor { doc })
    }

    /// Find text occurrences. `query_json` is a JSON object:
    ///
    /// ```json
    /// {
    ///   "text": "Acme B.V.",          // required
    ///   "caseInsensitive": false,      // optional
    ///   "pages": [1, 5],               // optional inclusive 1-based range
    ///   "region": {                    // optional
    ///     "page": 1,
    ///     "rect": [x0, y0, x1, y1],
    ///     "relation": "intersects"     // or "contained"
    ///   },
    ///   "limit": 100                   // optional
    /// }
    /// ```
    ///
    /// Returns a JSON array of matches; each match carries an opaque `id`
    /// token valid until the next applied edit on this document.
    #[wasm_bindgen(js_name = findText)]
    pub fn find_text(&mut self, query_json: &str) -> Result<String, JsValue> {
        let query = parse_query(query_json)
            .map_err(|e| wasm_err_with_op("INVALID_QUERY", &e, "TextEditor.findText"))?;
        let matches = self.doc.find_text(query).map_err(|e| {
            wasm_err_with_op("TEXT_EDIT_FAILED", &e.to_string(), "TextEditor.findText")
        })?;
        serde_json::to_string(&matches).map_err(|e| {
            wasm_err_with_op("SERIALIZE_FAILED", &e.to_string(), "TextEditor.findText")
        })
    }

    /// Find and replace in one call. Returns the JSON replacement report.
    ///
    /// `options_json` may be empty/`"{}"` for defaults, or:
    ///
    /// ```json
    /// {
    ///   "fontFallback": "deny" | "injectStandard" | {"explicit": "F1"},
    ///   "signaturePolicy": "reject" | "allowPostSignatureChange",
    ///   "commitPolicy": "allOrNothing" | "bestEffort"
    /// }
    /// ```
    #[wasm_bindgen(js_name = replaceText)]
    pub fn replace_text(
        &mut self,
        query_json: &str,
        replacement: &str,
        options_json: &str,
    ) -> Result<String, JsValue> {
        let query = parse_query(query_json)
            .map_err(|e| wasm_err_with_op("INVALID_QUERY", &e, "TextEditor.replaceText"))?;
        let options = parse_options(options_json)
            .map_err(|e| wasm_err_with_op("INVALID_OPTIONS", &e, "TextEditor.replaceText"))?;
        let report = self
            .doc
            .replace_text(query, replacement, options)
            .map_err(|e| {
                wasm_err_with_op("TEXT_EDIT_FAILED", &e.to_string(), "TextEditor.replaceText")
            })?;
        serde_json::to_string(&report).map_err(|e| {
            wasm_err_with_op("SERIALIZE_FAILED", &e.to_string(), "TextEditor.replaceText")
        })
    }

    /// Apply per-match replacements located earlier with `findText`.
    ///
    /// `edits_json` is a JSON array of `{"id": "<match token>", "text":
    /// "<replacement>"}`. All edits commit in one atomic transaction under
    /// `options_json` (see [`TextEditor::replace_text`]); the default is
    /// all-or-nothing. Returns the JSON replacement report.
    #[wasm_bindgen(js_name = replaceMatches)]
    pub fn replace_matches(
        &mut self,
        edits_json: &str,
        options_json: &str,
    ) -> Result<String, JsValue> {
        #[derive(serde::Deserialize)]
        struct EditIn {
            id: String,
            text: String,
        }
        let edits: Vec<EditIn> = serde_json::from_str(edits_json).map_err(|e| {
            wasm_err_with_op(
                "INVALID_EDITS",
                &format!("expected [{{id, text}}, …]: {e}"),
                "TextEditor.replaceMatches",
            )
        })?;
        let options = parse_options(options_json)
            .map_err(|e| wasm_err_with_op("INVALID_OPTIONS", &e, "TextEditor.replaceMatches"))?;
        let edits: Vec<(MatchId, String)> = edits
            .into_iter()
            .map(|e| (MatchId::from_token(e.id), e.text))
            .collect();
        let report = self
            .doc
            .replace_text_matches(&edits, options)
            .map_err(|e| {
                wasm_err_with_op(
                    "TEXT_EDIT_FAILED",
                    &e.to_string(),
                    "TextEditor.replaceMatches",
                )
            })?;
        serde_json::to_string(&report).map_err(|e| {
            wasm_err_with_op(
                "SERIALIZE_FAILED",
                &e.to_string(),
                "TextEditor.replaceMatches",
            )
        })
    }

    /// Serialize the (possibly edited) document to PDF bytes.
    pub fn save(&self) -> Result<Vec<u8>, JsValue> {
        self.doc
            .to_bytes()
            .map_err(|e| wasm_err_with_op("SAVE_FAILED", &e.to_string(), "TextEditor.save"))
    }

    /// Number of pages.
    #[wasm_bindgen(js_name = pageCount)]
    pub fn page_count(&self) -> usize {
        self.doc.page_count()
    }
}

// ---------------------------------------------------------------------------
// JSON input parsing
// ---------------------------------------------------------------------------

fn parse_query(json: &str) -> Result<TextQuery, String> {
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
        limit: Option<usize>,
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
        query = query.limit(n);
    }
    Ok(query)
}

fn parse_options(json: &str) -> Result<ReplaceOptions, String> {
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

// ---------------------------------------------------------------------------
// Native tests (run with `cargo test -p xfa-wasm --lib text_edit`)
// ---------------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream, StringFormat};

    fn pdf_with_text(lines: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }));
        let mut operations = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
            Operation::new("Td", vec![Object::Real(100.0), Object::Real(700.0)]),
        ];
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                operations.push(Operation::new(
                    "Td",
                    vec![Object::Real(0.0), Object::Real(-20.0)],
                ));
            }
            operations.push(Operation::new(
                "Tj",
                vec![Object::String(
                    line.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ));
        }
        operations.push(Operation::new("ET", vec![]));
        let content = Content { operations }.encode().unwrap();

        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => Object::Reference(font_id),
                }),
            }),
        }));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn wasm_text_editor_find_and_replace_by_id_roundtrip() {
        let bytes = pdf_with_text(&["Acme", "Acme"]);
        let mut editor = TextEditor::open(&bytes).expect("open");

        let matches_json = editor.find_text(r#"{"text": "Acme"}"#).expect("findText");
        let matches: serde_json::Value = serde_json::from_str(&matches_json).unwrap();
        let arr = matches.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let second_id = arr[1]["id"].as_str().unwrap().to_string();
        assert!(second_id.starts_with("pdfluent-match-v1."));

        // Apply by id — the token has crossed a JSON boundary, as it would
        // over postMessage in the [redacted] workflow.
        let edits = format!(
            r#"[{{"id": {}, "text": "Bqme"}}]"#,
            serde_json::to_string(&second_id).unwrap()
        );
        let report_json = editor
            .replace_matches(&edits, "{}")
            .expect("replaceMatches");
        let report: serde_json::Value = serde_json::from_str(&report_json).unwrap();
        assert_eq!(report["replacements_applied"], 1);

        // Saved output extracts the edit.
        let saved = editor.save().expect("save");
        let engine = pdf_engine::PdfDocument::open(saved).expect("reopen");
        let text = engine.extract_text(0).expect("extract");
        assert!(text.contains("Bqme"), "{text:?}");
        assert!(
            text.contains("Acme"),
            "first occurrence untouched: {text:?}"
        );

        // The SDK surface enforces the trial notice (guarded on the tier:
        // GLOBAL_TIER is process-wide, so another test may have activated a
        // license first).
        if pdfluent::license_info().tier == pdfluent::Tier::Trial {
            assert!(
                text.contains("PDFluent trial"),
                "trial edits through the WASM SDK surface are stamped: {text:?}"
            );
        }
    }

    #[test]
    fn wasm_text_editor_convenience_replace_reports() {
        let bytes = pdf_with_text(&["Hello World"]);
        let mut editor = TextEditor::open(&bytes).expect("open");
        let report_json = editor
            .replace_text(r#"{"text": "Hello"}"#, "Howdy", "{}")
            .expect("replaceText");
        let report: serde_json::Value = serde_json::from_str(&report_json).unwrap();
        assert_eq!(report["matches_found"], 1);
        assert_eq!(report["replacements_applied"], 1);
        assert_eq!(report["replacements_failed"], 0);
    }

    // JsValue construction panics on non-wasm targets, so malformed-input
    // rejection is tested against the pure parse functions directly.
    #[test]
    fn wasm_text_editor_rejects_malformed_input() {
        assert!(parse_query(r#"{"txet": "typo"}"#).is_err(), "unknown key");
        assert!(parse_query("not json").is_err());
        assert!(parse_options(r#"{"commitPolicy": "yolo"}"#).is_err());
        assert!(parse_options(r#"{"fontFallback": "bogus"}"#).is_err());
        assert!(parse_options(r#"{"signaturePolicy": 42}"#).is_err());
        // Defaults parse from empty and {}.
        assert!(parse_options("").is_ok());
        assert!(parse_options("{}").is_ok());
        // Full option surface parses.
        let o = parse_options(
            r#"{"fontFallback": {"explicit": "F7"}, "signaturePolicy": "allowPostSignatureChange", "commitPolicy": "bestEffort"}"#,
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
