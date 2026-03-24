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
