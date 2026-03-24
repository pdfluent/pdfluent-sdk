/// Debug example: run flatten_xfa_to_pdf and save the output.
/// Usage: cargo run --example debug_flatten -- /path/to/input.pdf /path/to/output.pdf

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: debug_flatten <input.pdf> <output.pdf>");
        std::process::exit(1);
    }
    let input = &args[1];
    let output = &args[2];

    let data = std::fs::read(input).expect("read input PDF");
    match pdf_xfa::flatten_xfa_to_pdf(&data) {
        Ok(buf) => {
            std::fs::write(output, &buf).expect("write output PDF");
            println!("Wrote {} bytes to {output}", buf.len());
        }
        Err(e) => {
            eprintln!("flatten failed: {e}");
            std::process::exit(1);
        }
    }
}
