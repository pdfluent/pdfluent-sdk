//! iText 5 XFA oracle.
//!
//! Calls `/opt/itext/itext-xfa-oracle.sh` which runs `ITextXfaOracle` and
//! returns JSON:
//!   `{"has_xfa": bool, "flatten_success": bool, "page_count": N, "errors": []}`
//!
//! `ITextOracle::call` returns `None` when the script is absent, the process
//! fails to start, or the output cannot be parsed.

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

    /// Run the oracle for the given PDF path.
    ///
    /// Returns `None` if the script cannot be executed or produces
    /// unparseable output.
    pub fn call(&self, pdf_path: &Path) -> Option<ITextResult> {
        let output = std::process::Command::new(&self.script_path)
            .arg(pdf_path)
            .output()
            .ok()?;

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
