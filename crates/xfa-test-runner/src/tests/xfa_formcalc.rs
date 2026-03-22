//! XFA FormCalc corpus evaluation test.
//!
//! Extracts all FormCalc scripts from the XFA template packet of each PDF,
//! then lexes, parses, and evaluates them through the formcalc-interpreter.
//!
//! The test passes as long as no script causes a panic or crash. Evaluation
//! errors (syntax errors, runtime errors) are counted and reported in metadata
//! but do not cause the test to fail — real-world FormCalc scripts often
//! reference SOM paths that resolve to `null` without a DOM context.
//!
//! Skip policy:
//! - No /XFA in this PDF  → Skip (not an XFA form)
//! - XFA extraction error → Skip (malformed XFA; defer to xfa_extract)
//! - No template packet   → Skip (XFA has no form template)
//! - Template has no FormCalc scripts → Skip (nothing to evaluate)
//!
//! Fail conditions:
//! - Script evaluation causes a panic (caught via catch_unwind)

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct XfaFormCalcTest;

impl PdfTest for XfaFormCalcTest {
    fn name(&self) -> &str {
        "xfa_formcalc"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("PDF could not be parsed".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        let packets = match pdf_xfa::extract::extract_xfa(&pdf) {
            Ok(p) => p,
            Err(pdf_xfa::error::XfaError::PacketNotFound(_)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("XFA extraction error: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        let template = match packets.template() {
            Some(t) => t,
            None => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("no template packet".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        let scripts = extract_formcalc_scripts(template);
        if scripts.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut eval_success = 0u32;
        let mut eval_errors = 0u32;
        let mut first_errors: Vec<String> = Vec::new();

        for script in &scripts {
            // Use catch_unwind so a panicking script causes Fail rather than crash.
            let script_owned = script.clone();
            let result = std::panic::catch_unwind(move || evaluate_formcalc(&script_owned));
            match result {
                Err(_panic) => {
                    // Script caused a panic — hard failure.
                    let mut metadata = HashMap::new();
                    metadata.insert("script_count".to_string(), scripts.len().to_string());
                    metadata.insert("eval_success".to_string(), eval_success.to_string());
                    metadata.insert("eval_errors".to_string(), (eval_errors + 1).to_string());
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some("FormCalc script caused a panic".into()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    };
                }
                Ok(Ok(())) => eval_success += 1,
                Ok(Err(e)) => {
                    eval_errors += 1;
                    if first_errors.len() < 3 {
                        first_errors.push(e);
                    }
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("script_count".to_string(), scripts.len().to_string());
        metadata.insert("eval_success".to_string(), eval_success.to_string());
        metadata.insert("eval_errors".to_string(), eval_errors.to_string());
        if !first_errors.is_empty() {
            metadata.insert("errors".to_string(), first_errors.join(" | "));
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}

/// Extract all FormCalc script bodies from a template XML string.
///
/// XFA FormCalc scripts appear as:
///   `<script contentType="application/x-formcalc">...</script>`
/// The contentType may use single or double quotes.
fn extract_formcalc_scripts(template_xml: &str) -> Vec<String> {
    let Ok(re) = regex_lite::Regex::new(
        r#"(?s)<script[^>]*contentType=["']application/x-formcalc["'][^>]*>(.*?)</script>"#,
    ) else {
        return Vec::new();
    };
    re.captures_iter(template_xml)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Lex, parse, and evaluate a FormCalc script without a DOM context.
///
/// SOM path references that can't be resolved return null values, which is
/// expected — this test only guards against crashes and panics.
fn evaluate_formcalc(script: &str) -> Result<(), String> {
    let tokens = pdf_xfa::formcalc::lexer::tokenize(script).map_err(|e| format!("lex: {e}"))?;
    let ast = pdf_xfa::formcalc::parser::parse(tokens).map_err(|e| format!("parse: {e}"))?;
    let mut interp = pdf_xfa::formcalc::interpreter::Interpreter::new();
    interp.exec(&ast).map_err(|e| format!("eval: {e}"))?;
    Ok(())
}
