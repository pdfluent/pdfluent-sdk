//! Diagnostic dump of the XfaSession field model for one PDF.
//!
//! ```bash
//! cargo run -p pdf-xfa --example xfa_session_dump -- form.pdf
//! cargo run -p pdf-xfa --example xfa_session_dump -- form.pdf \
//!     --set "form1.applicant.name=Alice" --out filled.pdf
//! ```

use pdf_xfa::session::{XfaSession, XfaWriteValue};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: xfa_session_dump <pdf> [--set name=value]... [--out out.pdf] [--all]");
        std::process::exit(2);
    };
    let mut sets: Vec<(String, String)> = Vec::new();
    let mut out_path: Option<String> = None;
    let mut show_all = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--set" => {
                let kv = args.next().expect("--set needs name=value");
                let (k, v) = kv.split_once('=').expect("--set needs name=value");
                sets.push((k.to_string(), v.to_string()));
            }
            "--out" => out_path = Some(args.next().expect("--out needs a path")),
            "--all" => show_all = true,
            other => panic!("unknown arg: {other}"),
        }
    }

    let bytes = std::fs::read(&path).expect("read input PDF");
    let t0 = std::time::Instant::now();
    let mut session = XfaSession::open(&bytes).expect("open XfaSession");
    let open_ms = t0.elapsed().as_millis();

    let fields = session.fields();
    println!("opened in {open_ms} ms");
    println!("layout pages: {}", session.page_count());
    println!("fields: {}", fields.len());

    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut counters = (0usize, 0usize, 0usize, 0usize, 0usize);
    for f in fields {
        *kinds.entry(format!("{:?}", f.field_type)).or_default() += 1;
        if f.read_only {
            counters.0 += 1;
        }
        if f.required {
            counters.1 += 1;
        }
        if f.multiline {
            counters.2 += 1;
        }
        if f.hidden {
            counters.3 += 1;
        }
        if f.bound_to_data {
            counters.4 += 1;
        }
    }
    println!("kinds: {kinds:?}");
    println!(
        "read_only={} required={} multiline={} hidden={} bound_to_data={}",
        counters.0, counters.1, counters.2, counters.3, counters.4
    );

    let limit = if show_all { usize::MAX } else { 25 };
    for f in fields.iter().take(limit) {
        println!(
            "  [{}] {:?} page={:?} ro={} value={:?}",
            f.name,
            f.field_type,
            f.page,
            f.read_only,
            if f.value.len() > 40 {
                format!("{}…", &f.value[..40.min(f.value.len())])
            } else {
                f.value.clone()
            }
        );
    }
    if fields.len() > limit {
        println!("  … {} more (use --all)", fields.len() - limit);
    }

    for (name, value) in &sets {
        match session.set_value(name, XfaWriteValue::Text(value)) {
            Ok(outcome) => println!(
                "SET {name} -> {:?} (datasets={})",
                outcome.raw_value, outcome.persisted_to_datasets
            ),
            Err(e) => println!("SET {name} FAILED: {e}"),
        }
    }

    if let Some(out) = out_path {
        let saved = session.save_to_bytes().expect("save_to_bytes");
        std::fs::write(&out, &saved).expect("write output PDF");
        println!("saved -> {out} ({} bytes)", saved.len());
    }
}
