use std::fs;

fn main() {
    let paths = [
        ("/tmp/gen-776_776397.pdf", "/tmp/gen-776-xfa.pdf"),
        ("/tmp/gen-778_778456.pdf", "/tmp/gen-778-xfa.pdf"),
        ("/tmp/r3-PDFBOX-2755-0.pdf", "/tmp/pdfbox-xfa.pdf"),
    ];

    for (input, output) in &paths {
        let data = fs::read(input).expect("read failed");
        match pdf_xfa::flatten_xfa_to_pdf(&data) {
            Ok(out) => {
                fs::write(output, &out).expect("write failed");
                println!("OK {} -> {} ({} bytes)", input, output, out.len());
            }
            Err(e) => {
                println!("ERR {} -> {:?}", input, e);
            }
        }
    }
}
