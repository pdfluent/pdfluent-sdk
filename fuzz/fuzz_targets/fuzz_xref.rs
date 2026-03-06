#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the xref table parser.
    // Xref tables map object IDs to byte offsets — a common attack vector.
    let mut pdf_bytes = b"%PDF-1.4\nxref\n".to_vec();
    pdf_bytes.extend_from_slice(data);
    pdf_bytes.extend_from_slice(b"\ntrailer\n<< /Size 1 >>\nstartxref\n9\n%%EOF");
    let _ = pdf_syntax::Pdf::new(pdf_bytes);
});
