//! Corpus sweep for the XfaSession Phase-1 fill loop.
//!
//! For every `*.pdf` under the input directory: open a session, enumerate
//! fields, pick the first writable text field, write a marker value, save,
//! reopen, and verify the marker came back from the datasets packet.
//!
//! ```bash
//! cargo run -p pdf-xfa --release --example xfa_session_sweep -- <directory of XFA PDFs>
//! ```
//!
//! Output: one TSV line per document
//! `status<TAB>doc<TAB>pages<TAB>fields<TAB>detail` with a summary footer.
//! Statuses: `ROUNDTRIP_OK`, `ENUM_ONLY` (no writable text field),
//! `NO_PERSIST` (set ok, marker missing after reopen), `SET_FAIL`,
//! `SAVE_FAIL`, `REOPEN_FAIL`, `OPEN_FAIL`, `NOT_XFA`.

use pdf_xfa::session::{XfaFieldType, XfaSession, XfaWriteValue};

const MARKER: &str = "XFA-PHASE1-SWEEP";

fn classify(path: &std::path::Path) -> (String, usize, usize, String) {
    let Ok(bytes) = std::fs::read(path) else {
        return ("OPEN_FAIL".into(), 0, 0, "io error".into());
    };
    let mut session = match XfaSession::open(&bytes) {
        Ok(s) => s,
        Err(pdf_xfa::error::XfaError::PacketNotFound(_)) => {
            return ("NOT_XFA".into(), 0, 0, String::new());
        }
        Err(e) => return ("OPEN_FAIL".into(), 0, 0, e.to_string()),
    };
    let pages = session.page_count();
    let nfields = session.fields().len();

    // First writable, visible text-like field.
    let target = session
        .fields()
        .iter()
        .find(|f| {
            !f.read_only
                && matches!(
                    f.field_type,
                    XfaFieldType::Text | XfaFieldType::Numeric | XfaFieldType::Password
                )
                && !f.bind_none
        })
        .map(|f| f.name.clone());
    let Some(name) = target else {
        return ("ENUM_ONLY".into(), pages, nfields, String::new());
    };

    let value = if matches!(
        session.field(&name).map(|f| f.field_type),
        Some(XfaFieldType::Numeric)
    ) {
        "42".to_string()
    } else {
        MARKER.to_string()
    };

    match session.set_value(&name, XfaWriteValue::Text(&value)) {
        Ok(outcome) if !outcome.persisted_to_datasets => {
            return (
                "NO_PERSIST".into(),
                pages,
                nfields,
                format!("{name}: not bound"),
            );
        }
        Ok(_) => {}
        Err(e) => return ("SET_FAIL".into(), pages, nfields, format!("{name}: {e}")),
    }

    let saved = match session.save_to_bytes() {
        Ok(b) => b,
        Err(e) => return ("SAVE_FAIL".into(), pages, nfields, e.to_string()),
    };

    match XfaSession::open(&saved) {
        Ok(reopened) => match reopened.field(&name) {
            Some(f) if f.value == value => ("ROUNDTRIP_OK".into(), pages, nfields, name),
            Some(f) => (
                "NO_PERSIST".into(),
                pages,
                nfields,
                format!("{name}: reopened value {:?}", f.value),
            ),
            None => ("NO_PERSIST".into(), pages, nfields, format!("{name}: gone")),
        },
        Err(e) => ("REOPEN_FAIL".into(), pages, nfields, e.to_string()),
    }
}

fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("usage: xfa_session_sweep <dir-or-pdf>");
    let root = std::path::PathBuf::from(&arg);
    let mut paths: Vec<_> = if root.is_file() {
        vec![root]
    } else {
        walkdir(&root)
    };
    paths.sort();

    let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
    for path in &paths {
        let started = std::time::Instant::now();
        let (status, pages, fields, detail) = std::panic::catch_unwind(|| classify(path))
            .unwrap_or_else(|_| ("PANIC".into(), 0, 0, String::new()));
        let ms = started.elapsed().as_millis();
        println!(
            "{status}\t{}\t{pages}\t{fields}\t{ms}ms\t{detail}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        *tally.entry(status).or_default() += 1;
    }
    eprintln!("== SWEEP SUMMARY ({} docs) ==", paths.len());
    for (k, v) in tally {
        eprintln!("{k}\t{v}");
    }
}

fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(walkdir(&p));
        } else if p.extension().and_then(|e| e.to_str()) == Some("pdf") {
            out.push(p);
        }
    }
    out
}
