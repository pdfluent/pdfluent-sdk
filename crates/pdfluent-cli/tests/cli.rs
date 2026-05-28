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
fn inspect_json_invocation_works() {
    // The spec lists `pdfluent inspect <input> --json`; the --json flag is
    // accepted for compatibility and inspect always emits JSON.
    let f = sample_pdf();
    if !f.exists() {
        return; // fixture not present in this checkout; skip
    }
    let (ok, stdout, _) = run(&["inspect", f.to_str().unwrap(), "--json"]);
    assert!(ok, "inspect --json should succeed");
    assert!(
        stdout.contains("\"page_count\""),
        "inspect emits JSON with page_count"
    );
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

// --- enterprise readiness additions (PDFLUENT_CLI_ENTERPRISE_PRODUCTION_READINESS) ---

/// `completions` emits non-empty, shell-appropriate output and exits 0 for every
/// supported shell. Guards the documented completion-install workflow.
///
/// NOTE: generated completions use the clap program name `pdfluent` (not the
/// binary `pdfluent-cli`) — a known mismatch tracked as the top item in
/// `docs/reports/pdfluent_cli_enterprise_readiness_final.md` (entangled with the
/// deferred `pdfluent` binary-takeover decision; intentionally not changed here).
#[test]
fn completions_all_shells_smoke() {
    for (shell, marker) in [
        ("bash", "pdfluent"),
        ("zsh", "#compdef"),
        ("fish", "pdfluent"),
        ("powershell", "pdfluent"),
    ] {
        let (ok, stdout, _) = run(&["completions", shell]);
        assert!(ok, "completions {shell} should exit 0");
        assert!(!stdout.trim().is_empty(), "completions {shell} non-empty");
        assert!(
            stdout.contains(marker),
            "completions {shell} should contain `{marker}`"
        );
    }
}

/// Every `--json` surface must emit a single valid JSON document with the stable
/// envelope keys. Parses (not substring-matches) to lock the machine contract.
#[test]
fn json_outputs_are_valid_documents() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let fp = f.to_str().unwrap();
    let cases: &[&[&str]] = &[
        &["info", fp, "--json"],
        &["inspect", fp, "--json"],
        &["validate", fp, "--json"],
        &["extract-text", fp, "--json"],
        &["doctor", "--json"],
    ];
    for args in cases {
        let (ok, stdout, _) = run(args);
        assert!(ok, "{args:?} should succeed");
        let v: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{args:?} invalid JSON: {e}"));
        assert_eq!(v["ok"], serde_json::Value::Bool(true), "{args:?} ok=true");
        assert_eq!(v["version"], "1.0.0-beta.8", "{args:?} stable version");
        assert!(v.get("command").is_some(), "{args:?} has command");
        assert!(v.get("data").is_some(), "{args:?} has data");
    }
}

/// The error envelope is also valid JSON with the documented error shape + exit code.
#[test]
fn json_error_envelope_is_valid() {
    let bad = std::env::temp_dir().join("pdfluent_cli_json_err.pdf");
    std::fs::write(&bad, b"%PDF-nope\n").unwrap();
    let (ok, stdout, _) = run(&["info", bad.to_str().unwrap(), "--json"]);
    assert!(!ok, "malformed --json should be non-zero");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("error envelope is valid JSON");
    assert_eq!(v["ok"], serde_json::Value::Bool(false));
    assert_eq!(v["error"]["code"], "INVALID_PDF");
    assert_eq!(v["error"]["exit_code"], 4);
    let _ = std::fs::remove_file(&bad);
}

/// `extract-text --out` writes exactly the same text that the stdout path prints
/// (round-trip integrity), and stdout stays clean of the text in `--out` mode.
#[test]
fn extract_text_out_content_matches_stdout() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let fp = f.to_str().unwrap();
    let (_, stdout_text, _) = run(&["extract-text", fp]);
    let out = std::env::temp_dir().join("pdfluent_cli_roundtrip.txt");
    let _ = std::fs::remove_file(&out);
    let (ok, stdout_out, _) = run(&["extract-text", fp, "--out", out.to_str().unwrap()]);
    assert!(ok);
    let file_text = std::fs::read_to_string(&out).expect("out file readable");
    assert_eq!(
        file_text, stdout_text,
        "--out content must equal stdout text"
    );
    assert!(
        !stdout_out.contains(&file_text) || file_text.is_empty(),
        "--out mode must not also dump text to stdout"
    );
    let _ = std::fs::remove_file(&out);
}

/// Usage errors (missing required arg) exit with code 2 (USAGE), not a panic.
#[test]
fn missing_required_arg_is_usage_error() {
    assert_eq!(exit_code(&["info"]), 2, "missing <INPUT> is a usage error");
}

// --- release/distribution contract hardening (PDFLUENT_CLI_RELEASE_AND_DISTRIBUTION_READINESS) ---

/// Paths with spaces and non-ASCII (unicode) characters are handled correctly.
#[test]
fn unicode_and_space_path_handling() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let dir = std::env::temp_dir().join("pdfluent cli ünïcode dir");
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join("héllo wörld (1).pdf");
    std::fs::copy(&f, &p).unwrap();
    let (ok, stdout, _) = run(&["info", p.to_str().unwrap()]);
    assert!(ok, "info on unicode/space path should succeed");
    assert!(stdout.contains("Pages:"));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_dir(&dir);
}

/// `extract-text --out` overwrites an existing file deterministically (documented
/// overwrite semantics: the target is replaced with exactly the extracted text).
#[test]
fn extract_text_out_overwrites_existing() {
    let f = sample_pdf();
    if !f.exists() {
        return;
    }
    let out = std::env::temp_dir().join("pdfluent_cli_overwrite.txt");
    std::fs::write(&out, b"PREEXISTING-CONTENT-SHOULD-BE-REPLACED").unwrap();
    let (ok, _, _) = run(&[
        "extract-text",
        f.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(ok);
    let after = std::fs::read_to_string(&out).unwrap();
    assert!(
        !after.contains("PREEXISTING-CONTENT"),
        "--out must overwrite, not append"
    );
    let _ = std::fs::remove_file(&out);
}

/// On error, human output goes to stderr and stdout stays clean (operational
/// contract: pipelines can trust stdout for data only in non-JSON mode).
#[test]
fn error_human_output_goes_to_stderr_not_stdout() {
    let bad = std::env::temp_dir().join("pdfluent_cli_stderr_sep.pdf");
    std::fs::write(&bad, b"%PDF-broken\n").unwrap();
    let (ok, stdout, stderr) = run(&["info", bad.to_str().unwrap()]);
    assert!(!ok);
    assert!(
        stdout.trim().is_empty(),
        "stdout must be clean on error (non-JSON)"
    );
    assert!(stderr.contains("INVALID_PDF"), "human error code on stderr");
    let _ = std::fs::remove_file(&bad);
}
