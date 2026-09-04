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
//! Anything that reports a number must run through here.

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(inp), Some(outp)) = (args.next(), args.next()) else {
        eprintln!("usage: pdfa_convert_real <in.pdf> <out.pdf>");
        std::process::exit(2);
    };
    let data = std::fs::read(&inp).expect("read input");
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    match pdf_manip::pdfa::convert_bytes(&data, &opts) {
        Ok(out) => {
            std::fs::write(&outp, &out).expect("write output");
            eprintln!("ok: {} -> {} ({} bytes)", inp, outp, out.len());
        }
        Err(e) => {
            eprintln!("convert failed: {e}");
            std::process::exit(1);
        }
    }
}
