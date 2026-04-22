//! Idle-RSS measurement helper.
//!
//! Opens a single PDF and sleeps so the supervising script can sample RSS
//! via `ps -o rss= -p $PID`.
//!
//! Usage: `idle-rss <pdf-path> [sleep-secs]`

use std::env;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: idle-rss <pdf-path> [sleep-secs]");
            return ExitCode::from(2);
        }
    };
    let secs: u64 = env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };

    // Hold the document open while we sleep — that's the idle footprint.
    let _doc = match pdf_engine::PdfDocument::open(bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("open {path}: {e}");
            return ExitCode::from(1);
        }
    };

    println!("{}", std::process::id());
    sleep(Duration::from_secs(secs));
    ExitCode::SUCCESS
}
