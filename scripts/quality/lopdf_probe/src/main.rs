//! `lopdf_probe` — read-only strict-parse probe for the PDF input repairability
//! baseline (milestone `PDF_INPUT_REPAIRABILITY_AND_TOLERANT_PARSING_BASELINE`).
//!
//! Attempts `lopdf::Document::load(<path>)` — the exact strict parser PDFluent uses
//! in its lopdf write/validate/fallback paths (`flatten` static-fallback, `measure`
//! output-validity) — and prints a single JSON line:
//!
//! ```text
//! {"ok":true,"pages":N,"error":null}
//! {"ok":false,"pages":null,"error":"<message>"}
//! ```
//!
//! It performs **no repair**, **no mutation**, and **no write**. Exit code is 0 on
//! successful load, 1 on parse failure, 2 on usage error. This binary is not part of
//! the product workspace and changes no flatten/render/FreshMerge behavior.

use std::env;
use std::process::ExitCode;

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: lopdf_probe <pdf-path>");
            return ExitCode::from(2);
        }
    };

    match lopdf::Document::load(&path) {
        Ok(doc) => {
            let pages = doc.get_pages().len();
            println!("{{\"ok\":true,\"pages\":{pages},\"error\":null}}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!(
                "{{\"ok\":false,\"pages\":null,\"error\":\"{}\"}}",
                json_escape(&format!("{e}"))
            );
            ExitCode::FAILURE
        }
    }
}
