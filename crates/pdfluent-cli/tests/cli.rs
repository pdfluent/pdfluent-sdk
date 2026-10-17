//! CLI contract tests: exit codes, envelopes, help, and the kept commands.
//!
//! Behaviour tests for the new commands live in `cmd_*.rs`; this file pins
//! the cross-cutting surface (global flags, exit codes 0/1/2/3, envelope
//! shapes, and the `extract-text` compatibility the extraction rig leans on).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{alias_bin, run, sample_pdf_path, stdout_json, stdout_text, tempdir};

/// True when clap renders `flag` as its own option line. A substring or
/// token match would also accept `--owner-password-file` or a wrapped
/// doc-comment mention of the flag, neither of which means the option exists.
fn has_option_line(help: &str, flag: &str) -> bool {
    let prefix = format!("{flag} ");
    help.lines()
        .any(|line| line.trim_start().starts_with(&prefix))
}

#[test]
fn help_lists_every_command() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = stdout_text(&out);
    // Commands must appear as entries in the `Commands:` listing, not merely
    // as words in the about text (which names several of them already).
    let mut in_commands = false;
    let mut listed = Vec::new();
    for line in help.lines() {
        if line.trim() == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() || !line.starts_with(' ') {
                break;
            }
            listed.push(line.split_whitespace().next().unwrap_or("").to_string());
        }
    }
    assert!(!listed.is_empty(), "top help has a Commands section");
    for cmd in [
        "extract",
        "merge",
        "split",
        "pages",
        "compress",
        "protect",
        "unprotect",
        "watermark",
        "convert",
        "redact",
        "sign",
        "validate",
        "info",
        "inspect",
        "extract-text",
        "doctor",
        "completions",
        "xfa",
    ] {
        assert!(listed.contains(&cmd.to_string()), "help lists `{cmd}`");
    }
    for flag in ["--json", "--quiet", "--verbose"] {
        assert!(help.contains(flag), "help lists `{flag}`");
    }
}

#[test]
fn version_names_the_takeover_binary() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(stdout_text(&out).trim(), "pdfluent 1.1.0");
}

#[test]
fn the_sample_fixture_is_present() {
    // The facade's shared sample is tracked in the repo; every CLI test that
    // needs a real PDF builds on it. Red when someone deletes or moves it.
    let path = sample_pdf_path();
    assert!(path.is_file(), "fixture missing: {}", path.display());
}

#[test]
fn alias_binary_runs_the_same_program() {
    let sample = crate::require_sample!();
    let arg = sample.to_string_lossy().to_string();
    let primary = run(&["info", &arg]);
    let aliased = std::process::Command::new(alias_bin())
        .args(["info", &arg])
        .output()
        .expect("spawn pdfluent-cli");
    assert!(aliased.status.success());
    assert_eq!(primary.stdout, aliased.stdout);
    assert!(!primary.stdout.is_empty(), "alias runs the real program");
}

#[test]
fn unknown_command_is_a_usage_error() {
    let out = run(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn missing_required_argument_is_a_usage_error() {
    let out = run(&["merge", "a.pdf"]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["validate"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn info_reports_page_count_version_and_size() {
    let sample = crate::require_sample!();
    let out = run(&["info", &sample.to_string_lossy()]);
    assert!(out.status.success());
    let stdout = stdout_text(&out);
    assert!(stdout.contains("Pages:       1"));
    assert!(stdout.contains("PDF version: 1.7"));
    assert!(stdout.contains("Size:        784 bytes"));
}

#[test]
fn info_json_envelope_is_stable() {
    let sample = crate::require_sample!();
    // The global --json flag also parses after the subcommand.
    let out = run(&["info", &sample.to_string_lossy(), "--json"]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "info");
    assert_eq!(value["version"], "1.1.0");
    assert_eq!(value["data"]["page_count"], 1);
    assert_eq!(value["data"]["pdf_version"], "1.7");
    assert_eq!(value["data"]["file_size"], 784);
    assert_eq!(value["warnings"], serde_json::json!([]));
}

#[test]
fn inspect_is_info_json() {
    let sample = crate::require_sample!();
    let out = run(&["inspect", &sample.to_string_lossy()]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["command"], "info");
    assert_eq!(value["data"]["page_count"], 1);
}

#[test]
fn extract_text_ends_every_page_with_form_feed() {
    let sample = crate::require_sample!();
    let out = run(&["extract-text", &sample.to_string_lossy()]);
    assert!(out.status.success());
    let text = stdout_text(&out);
    assert_eq!(text, "PDFluent test fixture.\n\n\x0c");
}

#[test]
fn extract_text_out_writes_file_and_reports_json() {
    let sample = crate::require_sample!();
    let dir = tempdir("cli-extract-out");
    let target = dir.join("out.txt");
    let out = run(&[
        "--json",
        "extract-text",
        &sample.to_string_lossy(),
        "--out",
        &target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "PDFluent test fixture.\n\n\x0c"
    );
    let value = stdout_json(&out);
    assert_eq!(value["data"]["char_count"], 25);
    assert_eq!(value["data"]["page_count"], 1);
}

#[test]
fn extract_text_out_refuses_existing_output_without_force() {
    let sample = crate::require_sample!();
    let dir = tempdir("cli-extract-clobber");
    let target = dir.join("out.txt");
    std::fs::write(&target, "previous run").unwrap();
    let out = run(&[
        "extract-text",
        &sample.to_string_lossy(),
        "--out",
        &target.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "previous run");
    let out = run(&[
        "extract-text",
        &sample.to_string_lossy(),
        "--force",
        "--out",
        &target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "PDFluent test fixture.\n\n\x0c"
    );
}

#[test]
fn extract_text_json_carries_pages_and_caps_inline_text() {
    let sample = crate::require_sample!();
    let out = run(&["extract-text", &sample.to_string_lossy(), "--json"]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["data"]["text"], "PDFluent test fixture.\n\n\x0c");
    assert_eq!(value["data"]["pages"][0]["page"], 1);
    assert_eq!(
        value["data"]["pages"][0]["text"],
        "PDFluent test fixture.\n"
    );
}

#[test]
fn extract_text_no_page_breaks_joins_with_blank_line() {
    let sample = crate::require_sample!();
    let out = run(&[
        "extract-text",
        &sample.to_string_lossy(),
        "--no-page-breaks",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout_text(&out), "PDFluent test fixture.\n");
}

#[test]
fn extract_text_unicode_path_roundtrips() {
    let sample = crate::require_sample!();
    let dir = tempdir("cli-unicode");
    let target = dir.join("ünïcödé tëxt.pdf");
    std::fs::copy(&sample, &target).unwrap();
    let out = run(&["extract-text", &target.to_string_lossy()]);
    assert!(out.status.success());
    assert_eq!(stdout_text(&out), "PDFluent test fixture.\n\n\x0c");
}

#[test]
fn legacy_help_points_at_extract() {
    let out = run(&["extract-text", "--help"]);
    assert!(out.status.success());
    assert!(stdout_text(&out).contains("prefer `extract`"));
}

#[test]
fn validate_legacy_checks_parse_and_count() {
    let sample = crate::require_sample!();
    let out = run(&["validate", &sample.to_string_lossy()]);
    assert!(out.status.success());
    let stdout = stdout_text(&out);
    assert!(stdout.contains("valid: parses, 1 page(s), PDF 1.7"));
    assert!(stdout.contains("not a PDF/A or veraPDF conformance check"));
}

#[test]
fn corrupt_input_is_an_input_error() {
    let dir = tempdir("cli-corrupt");
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, common::corrupt_bytes()).unwrap();
    let arg = bad.to_string_lossy().to_string();
    for cmd in ["info", "extract-text", "validate"] {
        let out = run(&[cmd, &arg]);
        assert_eq!(out.status.code(), Some(2), "{cmd}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("INVALID_PDF"),
            "{cmd}"
        );
    }
}

#[test]
fn missing_input_reports_not_found() {
    let out = run(&["info", "does-not-exist.pdf"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("FILE_NOT_FOUND"));
}

/// The human error line links to the code's anchor on the error reference
/// page, `cli-` plus the code lowercased with its underscores kept.
#[test]
fn error_line_links_to_the_code_anchor_on_the_errors_page() {
    let out = run(&["info", "does-not-exist.pdf"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Docs: https://pdfluent.com/errors/#cli-file_not_found"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("pdfluent.dev"), "stderr: {stderr}");
}

#[test]
fn doctor_reports_environment() {
    let out = run(&["doctor"]);
    assert!(out.status.success());
    let stdout = stdout_text(&out);
    assert!(stdout.contains("pdfluent CLI doctor"));
    assert!(stdout.contains("cli_version:  1.1.0"));
    assert!(stdout.contains("sdk:          pdfluent facade"));
    assert!(stdout.contains("XFA is experimental"));
}

#[test]
fn doctor_file_mode_reports_leniency() {
    let sample = crate::require_sample!();
    let out = run(&["doctor", &sample.to_string_lossy()]);
    assert!(out.status.success());
    assert!(stdout_text(&out).contains("Leniency: clean"));
    let out = run(&["--json", "doctor", &sample.to_string_lossy()]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["data"]["leniency"]["clean"], true);
}

#[test]
fn xfa_flatten_stub_contract() {
    let sample = crate::require_sample!();
    let dir = tempdir("cli-xfa");
    let target = dir.join("flat.pdf");
    // saved-state: advertised stub, processing error.
    let out = run(&[
        "xfa",
        "flatten",
        &sample.to_string_lossy(),
        "-o",
        &target.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("NOT_IMPLEMENTED"), "{err}");
    assert!(err.contains("XFA is experimental"), "{err}");
    // fresh-merge without --experimental: refused before doing anything.
    let out = run(&[
        "xfa",
        "flatten",
        &sample.to_string_lossy(),
        "--policy",
        "fresh-merge",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("EXPERIMENTAL_OPT_IN_REQUIRED"));
    // Unknown policy is a usage error too.
    let out = run(&[
        "xfa",
        "flatten",
        &sample.to_string_lossy(),
        "--policy",
        "nope",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("BAD_POLICY"));
}

#[test]
fn completions_generate_for_pdfluent() {
    for (shell, marker) in [
        ("bash", "_pdfluent"),
        ("fish", "complete"),
        ("zsh", "#compdef"),
    ] {
        let out = run(&["completions", shell]);
        assert!(out.status.success(), "{shell}");
        assert!(stdout_text(&out).contains(marker), "{shell}");
    }
}

#[test]
fn no_plaintext_secrets_in_help_or_surface() {
    let out = run(&["--help"]);
    let help = stdout_text(&out);
    assert!(
        !help.contains("--password "),
        "no argv password on the top level"
    );
    for cmd in ["protect", "sign"] {
        let out = run(&[cmd, "--help"]);
        let help = stdout_text(&out);
        assert!(
            has_option_line(&help, "--password-file"),
            "{cmd} documents the file source"
        );
        assert!(
            !help.contains("--password "),
            "{cmd} takes no argv password"
        );
        assert!(
            help.contains("PDFLUENT_PASSWORD"),
            "{cmd} documents the env source"
        );
    }
}

#[test]
fn unicode_stderr_paths_stay_readable() {
    let dir = tempdir("cli-unicode-err");
    let missing = dir.join("dëfinitely-missing.pdf");
    let out = run(&["info", &missing.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("FILE_NOT_FOUND"));
    assert!(err.contains("dëfinitely-missing.pdf"));
}

#[test]
fn json_envelope_separates_stdout_and_stderr() {
    let out = run(&["info", "does-not-exist.pdf", "--json"]);
    assert_eq!(out.status.code(), Some(2));
    // stdout holds exactly one JSON document; the human error stays on stderr.
    let value = stdout_json(&out);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "FILE_NOT_FOUND");
    assert_eq!(value["error"]["exit_code"], 2);
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
}

#[test]
fn error_envelope_carries_exit_code() {
    let out = run(&["merge", "a.pdf", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("required"));
}

#[test]
fn verbose_writes_diagnostics_to_stderr() {
    let dir = tempdir("cli-verbose");
    let a = dir.join("a.pdf");
    std::fs::write(&a, common::pdf_with_texts(&["A"])).unwrap();
    let merged = dir.join("m.pdf");
    let out = run(&[
        "-v",
        "merge",
        &a.to_string_lossy(),
        "-o",
        &merged.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).is_empty());
    let out = run(&[
        "merge",
        &a.to_string_lossy(),
        "--force",
        "-o",
        &merged.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
}
