//! Batch mode is byte-identical to one call per PDF (#598).
//!
//! The training lanes read what the dump writes; a batch that reordered pages
//! or rounded differently would be a different dataset under the same name.
//! So the fixtures are dumped once per process and once as a parallel batch,
//! and the bytes must agree.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::process::Command;

fn fixtures() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("fixtures")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .map(|p| p.display().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn a_parallel_batch_writes_exactly_what_single_calls_write() {
    let bin = env!("CARGO_BIN_EXE_feature_dump");
    let pdfs = fixtures();
    assert!(pdfs.len() >= 3, "fixtures: {pdfs:?}");

    let mut single = Vec::new();
    for pdf in &pdfs {
        let out = Command::new(bin).arg(pdf).output().expect("run");
        assert!(
            out.status.success(),
            "{pdf}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        single.extend_from_slice(&out.stdout);
    }

    let dir = std::env::temp_dir().join(format!("feature-dump-batch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let list = dir.join("list.txt");
    std::fs::write(&list, pdfs.join("\n") + "\n").expect("list");
    let out = dir.join("chunk.jsonl");
    let status = Command::new(bin)
        .args(["--jobs", "4", "--list"])
        .arg(&list)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run batch");
    assert!(status.success());
    let batch = std::fs::read(&out).expect("chunk");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!single.is_empty());
    assert_eq!(batch, single, "the batch differs from one call per PDF");
}

#[test]
fn a_page_id_from_the_list_is_the_one_in_the_record() {
    let bin = env!("CARGO_BIN_EXE_feature_dump");
    let pdf = fixtures()
        .into_iter()
        .find(|p| p.ends_with("structured.pdf"))
        .expect("fixture");
    let dir = std::env::temp_dir().join(format!("feature-dump-id-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let list = dir.join("list.txt");
    std::fs::write(&list, format!("{pdf}\tdoclaynet-page-xyz\n")).expect("list");
    let out = Command::new(bin)
        .arg("--list")
        .arg(&list)
        .output()
        .expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        text.lines()
            .all(|l| l.contains("\"page_id\":\"doclaynet-page-xyz\"")),
        "{text}"
    );
}

/// An unreadable line in the list refuses the batch instead of dumping the
/// prefix before it as if it were all (codex, #1961).
#[test]
fn an_unreadable_list_line_is_a_refusal_not_a_shorter_batch() {
    use std::io::Write;
    let bin = env!("CARGO_BIN_EXE_feature_dump");
    let pdf = fixtures().into_iter().next().expect("fixture");
    let mut child = Command::new(bin)
        .args(["--list", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(pdf.as_bytes()).expect("write");
    stdin.write_all(b"\n\xff\xfe not utf-8\n").expect("write");
    drop(stdin);
    let out = child.wait_with_output().expect("wait");
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
