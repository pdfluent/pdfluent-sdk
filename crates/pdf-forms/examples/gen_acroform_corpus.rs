//! Emit the synthetic AcroForm corpus to disk.
//!
//! Usage: `cargo run -p pdfluent-forms --example gen_acroform_corpus -- <out_dir>`
//!
//! Writes one `<category>.pdf` per support-contract category. Used to feed the
//! external-compatibility validator (`scripts/forms/verify_acroform_output.py`,
//! pikepdf-based) and the language-binding tests. The same fixtures are
//! exercised in-process by `tests/corpus_gate.rs`.

#[path = "../tests/common/acroform_fixtures.rs"]
mod fx;

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .expect("usage: gen_acroform_corpus <out_dir>");
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    for fixture in fx::all_fixtures() {
        let path = format!("{out_dir}/{}.pdf", fixture.category);
        std::fs::write(&path, &fixture.bytes).expect("write fixture");
        println!("{path}\t{} bytes", fixture.bytes.len());
    }
}
