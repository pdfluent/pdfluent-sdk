use pdf_engine::PdfDocument;
use std::env;
use std::fs;
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("usage: cargo run -p pdf-engine --example batch_extract -- <pdf>");
        return ExitCode::from(2);
    };

    match run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: impl AsRef<std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = Arc::new(fs::read(path)?);
    let doc = PdfDocument::open(bytes)?;
    print!("{}", doc.extract_all_text());
    Ok(())
}
