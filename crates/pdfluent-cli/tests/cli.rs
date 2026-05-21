//! Integration tests for the public `pdfluent` CLI scaffold.
//! Uses only the tiny shared `sample.pdf` fixture — no corpus/private files.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfluent-cli"))
}

fn sample_pdf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pdfluent/tests/fixtures/sample.pdf")
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = bin().args(args).output().expect("spawn pdfluent-cli");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn help_works() {
    let (ok, stdout, _) = run(&["--help"]);
    assert!(ok, "--help should exit 0");
    assert!(stdout.contains("PDFluent"), "help mentions product");
    // Expected public commands present.
    for c in [
        "info",
        "inspect",
        "extract-text",
        "validate",
        "doctor",
        "completions",
        "xfa",
    ] {
        assert!(stdout.contains(c), "help should list `{c}`");
    }
}

#[test]
fn version_is_beta8() {
    let (ok, stdout, _) = run(&["--version"]);
    assert!(ok);
    assert!(
        stdout.contains("1.0.0-beta.8"),
        "version should be 1.0.0-beta.8, got: {stdout}"
    );
}

#[test]
fn doctor_exits_zero() {
    let (ok, stdout, _) = run(&["doctor"]);
    assert!(ok, "doctor should exit 0");
    assert!(stdout.contains("cli_version:  1.0.0-beta.8"));
    assert!(
        stdout.contains("experimental"),
        "doctor notes XFA experimental"
    );
}

#[test]
fn internal_commands_not_exposed() {
    let (_, stdout, _) = run(&["--help"]);
    for hidden in [
        "measure",
        "debug-xfa",
        "flatten-check",
        "demo",
        "render",
        "collector",
    ] {
        assert!(
            !stdout.contains(hidden),
            "public help must not expose internal `{hidden}`"
        );
    }
}

#[test]
fn xfa_help_has_experimental_caveat() {
    let (ok, stdout, _) = run(&["xfa", "flatten", "--help"]);
    assert!(ok);
    let s = stdout.to_lowercase();
    assert!(
        s.contains("experimental"),
        "xfa flatten help must say experimental"
    );
    assert!(
        s.contains("fresh-merge"),
        "xfa flatten help mentions fresh-merge policy"
    );
    assert!(
        s.contains("saved-state"),
        "xfa flatten help mentions saved-state default"
    );
}

#[test]
fn unknown_command_fails() {
    let (ok, _, _) = run(&["definitely-not-a-command"]);
    assert!(!ok, "unknown command should exit non-zero");
}

#[test]
fn fresh_merge_requires_opt_in() {
    // fresh-merge without --experimental must be refused (non-zero), never default.
    let (ok, _, stderr) = run(&[
        "xfa",
        "flatten",
        "x.pdf",
        "-o",
        "y.pdf",
        "--policy",
        "fresh-merge",
    ]);
    assert!(!ok, "fresh-merge without --experimental must fail");
    assert!(stderr.contains("EXPERIMENTAL_OPT_IN_REQUIRED"));
}

#[test]
fn info_valid_fixture() {
    let f = sample_pdf();
    if !f.exists() {
        return; // fixture not present in this checkout; skip
    }
    let (ok, stdout, _) = run(&["info", f.to_str().unwrap()]);
    assert!(ok, "info on valid PDF should succeed");
    assert!(stdout.contains("Pages:"), "info prints page count");

    let (okj, stdoutj, _) = run(&["info", f.to_str().unwrap(), "--json"]);
    assert!(okj);
    assert!(stdoutj.contains("\"ok\": true") && stdoutj.contains("\"page_count\""));
}

#[test]
fn info_malformed_fails_cleanly() {
    let dir = std::env::temp_dir();
    let bad = dir.join("pdfluent_cli_bad_input.pdf");
    std::fs::write(&bad, b"%PDF-not-real\n").unwrap();
    let (ok, _, stderr) = run(&["info", bad.to_str().unwrap()]);
    assert!(!ok, "malformed input should fail");
    assert!(stderr.contains("INVALID_PDF") || stderr.contains("could not open"));
    let _ = std::fs::remove_file(&bad);
}

// --- core commands (extract-text, validate) ---

fn exit_code(args: &[&str]) -> i32 {
    bin()
        .args(args)
        .output()
        .expect("spawn")
        .status
        .code()
        .unwrap_or(-1)
}

#[test]
fn extract_text_valid_to_stdout() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let (ok, _stdout, _) = run(&["extract-text", f.to_str().unwrap()]);
    assert!(ok, "extract-text on valid PDF should succeed");
}

#[test]
fn extract_text_json_shape() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let (ok, stdout, _) = run(&["extract-text", f.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"ok\": true"));
    assert!(stdout.contains("\"command\": \"extract-text\""));
    assert!(stdout.contains("\"char_count\"") && stdout.contains("\"page_count\""));
    assert!(stdout.contains("\"warnings\""));
}

#[test]
fn extract_text_out_writes_file() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let out = std::env::temp_dir().join("pdfluent_cli_extract_out.txt");
    let _ = std::fs::remove_file(&out);
    let (ok, stdout, _) = run(&[
        "extract-text",
        f.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(ok);
    assert!(stdout.contains("Wrote"));
    assert!(out.exists(), "output file should be written");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn extract_text_malformed_is_exit_4() {
    let bad = std::env::temp_dir().join("pdfluent_cli_extract_bad.pdf");
    std::fs::write(&bad, b"%PDF-bogus\n").unwrap();
    assert_eq!(exit_code(&["extract-text", bad.to_str().unwrap()]), 4);
    let _ = std::fs::remove_file(&bad);
}

#[test]
fn extract_text_missing_is_exit_3() {
    assert_eq!(exit_code(&["extract-text", "/no/such/file_xyz.pdf"]), 3);
}

#[test]
fn validate_valid_is_exit_0() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    assert_eq!(exit_code(&["validate", f.to_str().unwrap()]), 0);
    let (_, stdout, _) = run(&["validate", f.to_str().unwrap()]);
    // Must not over-claim standards conformance.
    assert!(!stdout.to_lowercase().contains("pdf/a compliant"));
}

#[test]
fn validate_json_shape_no_overclaim() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let (ok, stdout, _) = run(&["validate", f.to_str().unwrap(), "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"valid\": true"));
    assert!(stdout.contains("\"checks\""));
    assert!(
        stdout.contains("\"compliance\": null"),
        "compliance must be null (not checked)"
    );
}

#[test]
fn validate_malformed_is_exit_4() {
    let bad = std::env::temp_dir().join("pdfluent_cli_validate_bad.pdf");
    std::fs::write(&bad, b"not a pdf at all").unwrap();
    assert_eq!(exit_code(&["validate", bad.to_str().unwrap()]), 4);
    let _ = std::fs::remove_file(&bad);
}

#[test]
fn validate_missing_is_exit_3() {
    assert_eq!(exit_code(&["validate", "/no/such/file_abc.pdf"]), 3);
}

#[test]
fn doctor_json_shape() {
    let (ok, stdout, _) = run(&["doctor", "--json"]);
    assert!(ok);
    assert!(stdout.contains("\"cli_version\": \"1.0.0-beta.8\""));
    assert!(stdout.contains("\"xfa\""));
}

#[test]
fn xfa_flatten_stub_exit_6() {
    // saved-state stub → NOT_IMPLEMENTED → exit 6
    assert_eq!(exit_code(&["xfa", "flatten", "in.pdf", "-o", "out.pdf"]), 6);
}

#[test]
fn no_secrets_in_doctor() {
    let (_, stdout, _) = run(&["doctor"]);
    for leak in [
        "SECRET", "TOKEN", "PASSWORD", "API_KEY", "/Users/", "/home/",
    ] {
        assert!(!stdout.contains(leak), "doctor must not leak `{leak}`");
    }
}
