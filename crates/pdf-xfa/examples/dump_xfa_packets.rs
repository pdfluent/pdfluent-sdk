//! Dump XFA template and datasets packets from a PDF to stdout/files.
//!
//! Used by the M3-C investigation to inspect Polish PIT and similar
//! schema-bound documents. Not part of the public CLI surface.
//!
//! # Usage
//!
//! ```text
//! cargo run --example dump_xfa_packets --features xfa-js-sandboxed -- <input.pdf> <out_prefix>
//! ```

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump_xfa_packets <input.pdf> <out_prefix>");
        std::process::exit(1);
    }
    let input = PathBuf::from(&args[1]);
    let out_prefix = &args[2];
    let pdf_bytes = std::fs::read(&input)?;
    let packets = pdf_xfa::extract::extract_xfa_from_bytes(pdf_bytes)?;
    println!("packets:");
    for (n, p) in &packets.packets {
        println!("  {} ({} bytes)", n, p.len());
    }
    if let Some(t) = packets.template() {
        let path = format!("{out_prefix}.template.xml");
        std::fs::write(&path, t)?;
        println!("wrote {path} ({} bytes)", t.len());
    }
    if let Some(d) = packets.datasets() {
        let path = format!("{out_prefix}.datasets.xml");
        std::fs::write(&path, d)?;
        println!("wrote {path} ({} bytes)", d.len());
    }
    Ok(())
}
