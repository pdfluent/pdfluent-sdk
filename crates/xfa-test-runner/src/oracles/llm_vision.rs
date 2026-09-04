//! LLM-based visual quality oracle using OpenRouter (Gemma 3 27B).
//!
//! Sends a rendered PDF page (as a PNG) to a vision LLM and asks it to rate
//! the rendering quality on a scale of 1–10.  This catches issues that SSIM
//! misses (e.g., garbled fonts, semantic layout errors, wrong text content).
//!
//! Requires the `OPENROUTER_API_KEY` environment variable.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

const MODEL: &str = "google/gemma-3-27b-it";
const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const TIMEOUT_SECS: u64 = 30;

const PROMPT: &str = r#"Evaluate whether this PDF page was rendered correctly by a PDF engine.
Look for RENDERING DEFECTS only: blank/white areas where content should be,
missing text or images, garbled or corrupted characters, objects at completely
wrong positions, severe unintentional color distortion, layout crashes.
Do NOT penalize for intentional design choices (color schemes, fonts, photo quality).
Score 1-10: 10=rendered correctly, 1=completely blank or fatally broken.
Respond ONLY as JSON with no markdown: {"score": N, "issues": [...]}"#;

pub struct LlmVisionOracle {
    api_key: String,
    client: Client,
}

pub struct LlmReview {
    pub score: u8,
    pub issues: Vec<String>,
}

impl LlmVisionOracle {
    /// Construct oracle from `OPENROUTER_API_KEY` env var. Returns `None` when
    /// the key is absent.
    pub fn new() -> Option<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY").ok()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .ok()?;
        Some(Self { api_key, client })
    }

    /// Send `png_bytes` to the LLM and return a quality review.
    ///
    /// Retries once on HTTP 429 (rate limit) with a 2-second back-off.
    pub fn review_page(&self, png_bytes: &[u8]) -> Result<LlmReview, String> {
        let data_url = format!("data:image/png;base64,{}", BASE64.encode(png_bytes));

        let body = serde_json::json!({
            "model": MODEL,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": PROMPT},
                    {"type": "image_url", "image_url": {"url": data_url}}
                ]
            }]
        });

        let body_str = serde_json::to_string(&body).map_err(|e| format!("serialize: {e}"))?;

        for attempt in 0u8..2 {
            let resp = self
                .client
                .post(API_URL)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .body(body_str.clone())
                .send()
                .map_err(|e| format!("request: {e}"))?;

            let status = resp.status();

            // Rate limited — back off and retry once.
            if status.as_u16() == 429 && attempt == 0 {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }

            if !status.is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(format!("HTTP {status}: {body}"));
            }

            let text = resp.text().map_err(|e| format!("read response: {e}"))?;
            let json: Value =
                serde_json::from_str(&text).map_err(|e| format!("parse response JSON: {e}"))?;

            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| format!("no content in response: {text}"))?;

            return parse_review(content);
        }

        Err("max retries exceeded".into())
    }
}

/// Parse the LLM's JSON reply, stripping any markdown code fences.
fn parse_review(content: &str) -> Result<LlmReview, String> {
    // The model may wrap the JSON in ```json ... ``` fences.
    let json_str = strip_code_fences(content);

    let v: Value = serde_json::from_str(json_str.trim())
        .map_err(|e| format!("JSON parse error: {e} — raw: {content}"))?;

    let score = v["score"]
        .as_u64()
        .ok_or_else(|| format!("missing 'score' field — raw: {content}"))? as u8;

    let issues = v["issues"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(LlmReview { score, issues })
}

fn strip_code_fences(s: &str) -> &str {
    let s = s.trim();
    // ```json\n...\n``` or ```\n...\n```
    if let Some(inner) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        if let Some(end) = inner.rfind("```") {
            return inner[..end].trim();
        }
    }
    s
}
