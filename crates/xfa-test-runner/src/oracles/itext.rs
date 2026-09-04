//! iText 5 XFA oracle.
//!
//! Calls `/opt/itext/itext-xfa-oracle.sh` which runs `ITextXfaOracle` and
//! returns JSON:
//!   `{"has_xfa": bool, "flatten_success": bool, "page_count": N, "errors": []}`
//!
//! `ITextOracle::call` returns `None` when the script is absent, the process
//! fails to start, or the output cannot be parsed.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::Path;

/// Result returned by the iText XFA oracle script.
#[derive(Debug)]
pub struct ITextResult {
    /// Whether the PDF contains an XFA form.
    pub has_xfa: bool,
    /// Whether iText successfully flattened the XFA form.
    pub flatten_success: bool,
    /// Page count of the flattened PDF (0 on failure).
    pub page_count: u32,
    /// Any error strings reported by iText.
    pub errors: Vec<String>,
}

/// Calls the iText XFA oracle shell script.
pub struct ITextOracle {
    script_path: String,
}

impl ITextOracle {
    const DEFAULT_SCRIPT: &'static str = "/opt/itext/itext-xfa-oracle.sh";

    /// Returns `Some(ITextOracle)` when the default script is present and
    /// executable, `None` otherwise.
    pub fn new() -> Option<Self> {
        if Path::new(Self::DEFAULT_SCRIPT).exists() {
            Some(Self {
                script_path: Self::DEFAULT_SCRIPT.to_string(),
            })
        } else {
            None
        }
    }

    /// Run the oracle and optionally write the flattened PDF to `output_path`.
    ///
    /// When `output_path` is `Some`, the script receives it as a second
    /// argument and writes the iText-flattened PDF there (if flatten succeeded).
    pub fn call_with_output(
        &self,
        pdf_path: &Path,
        output_path: Option<&Path>,
    ) -> Option<ITextResult> {
        let mut cmd = std::process::Command::new(&self.script_path);
        cmd.arg(pdf_path);
        if let Some(out) = output_path {
            cmd.arg(out);
        }
        let output = cmd.output().ok()?;

        // Accept non-zero exit codes: iText may exit non-zero on XFA errors
        // while still printing valid JSON.
        if output.stdout.is_empty() {
            return None;
        }

        parse_output(&output.stdout)
    }
}

fn parse_output(data: &[u8]) -> Option<ITextResult> {
    let s = std::str::from_utf8(data).ok()?;
    // Skip any JVM startup noise before the first `{`.
    let json_start = s.find('{')?;
    let v: serde_json::Value = serde_json::from_str(&s[json_start..]).ok()?;

    Some(ITextResult {
        has_xfa: v["has_xfa"].as_bool().unwrap_or(false),
        flatten_success: v["flatten_success"].as_bool().unwrap_or(false),
        page_count: v["page_count"].as_u64().unwrap_or(0) as u32,
        errors: v["errors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}
