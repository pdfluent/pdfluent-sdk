// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Convert a PDF to PDF/A through the SHIPPING pipeline (`pdfa::convert_bytes`).
//!
//! Why this exists next to `convert_pdfa.rs`: that example hand-rolls its own
//! sequence of fixup calls, and the sequence has drifted from the real one — it
//! still calls the superseded `strip_control_chars_from_streams`, which blanks
//! every character code below 32 because it passes an empty preserve set. On a
//! TeX subset font (codes 1..31 are ordinary letters) that erases the page, so
//! measurements taken through that example do not describe what we ship.
//! Anything that reports a number must run through here. The optional JSON
//! diagnostics use `convert_bytes_with_report`, the shared shipping implementation.

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(inp), Some(outp)) = (args.next(), args.next()) else {
        eprintln!("usage: pdfa_convert_real <in.pdf> <out.pdf> [diagnostics.json]");
        std::process::exit(2);
    };
    let diagnostics = args.next();
    #[cfg(not(feature = "serde"))]
    if diagnostics.is_some() {
        eprintln!("JSON diagnostics require the serde feature");
        std::process::exit(2);
    }
    let data = std::fs::read(&inp).expect("read input");
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    // Both APIs share the same shipping implementation; retain diagnostics
    // alongside measured outputs when the caller requests them.
    match pdf_manip::pdfa::convert_bytes_with_report(&data, &opts) {
        Ok((out, _report)) => {
            #[cfg(feature = "serde")]
            let report = _report;
            #[cfg(feature = "serde")]
            if let Some(path) = diagnostics {
                let value = serde_json::json!({
                    "pages": report.page_count,
                    "warnings": report.warnings,
                    "page_tree_repaired": report.page_tree_repaired,
                    "output_intent_added": report.output_intent_added,
                    "font_embedding": report.fonts.as_ref().map(|fonts| serde_json::json!({
                        "inspected": fonts.fonts_inspected,
                        "requiring_embedding": fonts.non_embedded_found,
                        "embedded": fonts.fonts_embedded,
                        "failed": fonts.failed,
                    })),
                });
                #[cfg(feature = "font-subset")]
                let value = {
                    let mut value = value;
                    value["font_subsetting"] = serde_json::json!({
                        "programs_subsetted": report.subsets.programs_subsetted,
                        "encoded_bytes_saved": report.subsets.bytes_saved,
                        "skipped": report.subsets.skipped.iter().map(|(id, reason)|
                            serde_json::json!({"object": id.0, "generation": id.1, "reason": reason})
                        ).collect::<Vec<_>>()
                    });
                    value
                };
                std::fs::write(
                    path,
                    serde_json::to_vec_pretty(&value).expect("encode diagnostics"),
                )
                .expect("write diagnostics");
            }
            std::fs::write(&outp, &out).expect("write output");
            eprintln!("ok: {} -> {} ({} bytes)", inp, outp, out.len());
        }
        Err(e) => {
            eprintln!("convert failed: {e}");
            std::process::exit(1);
        }
    }
}
