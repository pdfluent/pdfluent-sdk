//! `validate`: parse checks and PDF/A conformance, thin over
//! [`validate_pdfa`](pdfluent::PdfDocument::validate_pdfa).
//!
//! PDF/UA validation exists in `pdf-compliance` but is not exposed by the
//! facade — `--pdfua` reports that gap instead of reaching past the facade.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Validate PDF/A conformance (default profile a2b, see --profile).
    #[arg(long)]
    pub pdfa: bool,

    /// Validate PDF/UA conformance. Accepted and reported: the facade does
    /// not expose it.
    #[arg(long)]
    pub pdfua: bool,

    /// PDF/A profile for --pdfa.
    #[arg(long, value_enum, default_value = "a2b")]
    pub profile: crate::cmd_convert::Profile,
}

pub(crate) fn run(ctx: &Ctx, args: &ValidateArgs) -> u8 {
    if args.pdfua {
        return output::error(
            "validate",
            ctx,
            "PDFUA_NOT_IN_FACADE",
            "PDF/UA validation exists in `pdf-compliance` but is not exposed by the `pdfluent` facade (facade gap, reported rather than re-implemented)",
        );
    }
    let loaded = match input::load("validate", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("validate", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    if !args.pdfa {
        // Legacy parse + page-count check. NOT a conformance verdict.
        let page_count = doc.page_count();
        let version = doc.version().to_string();
        return output::success(
            "validate",
            ctx,
            json!({
                "valid": true,
                "page_count": page_count,
                "pdf_version": version,
                "checks": ["parse", "page_count"],
                "compliance": serde_json::Value::Null,
            }),
            vec![],
            |_| {
                println!("valid: parses, {page_count} page(s), PDF {version}");
                println!("note: this is a parse + page-count check, not a PDF/A or veraPDF conformance check.");
            },
        );
    }
    let profile = args.profile;
    let report = match doc.validate_pdfa(profile.into()) {
        Ok(report) => report,
        Err(e) => return output::facade_error("validate", ctx, &e),
    };
    let violations: Vec<serde_json::Value> = report
        .violations
        .iter()
        .map(|v| {
            json!({
                "rule": v.rule,
                "message": v.message,
                "severity": format!("{:?}", v.severity),
                "page": v.page,
            })
        })
        .collect();
    if !report.is_compliant() {
        let detail: Vec<(String, String)> = report
            .violations
            .iter()
            .map(|v| (v.rule.clone(), v.message.clone()))
            .collect();
        return output::error_with_data(
            "validate",
            ctx,
            "NOT_COMPLIANT",
            &format!(
                "not compliant with PDF/A-{} ({} violation(s))",
                profile_name(profile),
                violations.len()
            ),
            Some(json!({
                "compliant": false,
                "profile": profile_name(profile),
                "violations": violations,
            })),
            || {
                for (rule, message) in &detail {
                    eprintln!("  [{rule}] {message}");
                }
            },
        );
    }
    output::success(
        "validate",
        ctx,
        json!({
            "compliant": true,
            "profile": profile_name(profile),
            "violations": violations,
        }),
        vec![],
        |_| {
            println!("compliant: PDF/A-{}", profile_name(profile));
        },
    )
}

fn profile_name(profile: crate::cmd_convert::Profile) -> &'static str {
    match profile {
        crate::cmd_convert::Profile::A1b => "a1b",
        crate::cmd_convert::Profile::A2b => "a2b",
        crate::cmd_convert::Profile::A3b => "a3b",
    }
}
