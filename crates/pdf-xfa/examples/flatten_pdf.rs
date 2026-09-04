// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: flatten_pdf <input.pdf> <output.pdf>");
        return;
    }
    let pdf_bytes = std::fs::read(&args[1]).expect("read");
    match pdf_xfa::flatten_xfa_to_pdf(&pdf_bytes) {
        Ok(result) => {
            std::fs::write(&args[2], &result).expect("write");
            eprintln!("OK: {} -> {} bytes", pdf_bytes.len(), result.len());
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}
