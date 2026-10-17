//! Behaviour tests for `pdfluent protect`, `unprotect`, `redact`, `sign`.
//!
//! Passwords reach the CLI only via files, env, or piped stdin — never argv —
//! and the secrets test in `cli.rs` pins that shape of the help.

// CLI covers: protect
// CLI covers: unprotect
// CLI covers: redact
// CLI covers: sign

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{
    page_count_of, run, run_with_env, run_with_stdin, stdout_json, stdout_text, tempdir, text_of,
};

#[test]
fn protect_locks_and_names_its_algorithm() {
    let sample = crate::require_sample!();
    let dir = tempdir("protect");
    let pw = common::password_file(&dir, "pw.txt", &["s3cret"]);
    let locked = dir.join("locked.pdf");
    let arg = locked.to_string_lossy().to_string();
    let out = run(&[
        "--json",
        "protect",
        &sample.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "--algorithm",
        "aes128",
        "--permissions",
        "print-only",
        "-o",
        &arg,
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout_json(&out)["data"]["algorithm"], "aes128");
    // The output really is locked: opening it demands a password.
    let out = run(&["info", &arg]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("PASSWORD_REQUIRED"));
}

#[test]
fn protect_takes_passwords_from_env_and_pipe() {
    let sample = crate::require_sample!();
    let dir = tempdir("protect-sources");
    let via_env = dir.join("env.pdf");
    let out = run_with_env(
        &[
            "protect",
            &sample.to_string_lossy(),
            "-o",
            &via_env.to_string_lossy(),
        ],
        &[("PDFLUENT_PASSWORD", "env-secret")],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The output exists AND is locked: a missing file also exits 2, so the
    // existence check is what ties this to `protect` having done its work.
    assert!(via_env.exists());
    let probe = run(&["info", &via_env.to_string_lossy()]);
    assert_eq!(probe.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&probe.stderr).contains("PASSWORD_REQUIRED"));

    // Piped stdin answers the prompt (single line, no TTY confirm).
    let via_pipe = dir.join("pipe.pdf");
    let args = [
        "protect",
        &sample.to_string_lossy(),
        "-o",
        &via_pipe.to_string_lossy(),
    ];
    let out = run_with_stdin(&args, b"piped-secret\n");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(via_pipe.exists());
    let probe = run(&["info", &via_pipe.to_string_lossy()]);
    assert_eq!(probe.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&probe.stderr).contains("PASSWORD_REQUIRED"));
}

#[test]
fn protect_fails_without_a_password_source() {
    let sample = crate::require_sample!();
    let dir = tempdir("protect-nopw");
    let target = dir.join("x.pdf");
    // Empty piped stdin: EOF where the prompt read, no file, no env.
    let args = [
        "protect",
        &sample.to_string_lossy(),
        "-o",
        &target.to_string_lossy(),
    ];
    let out = run_with_stdin(&args, b"");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("PASSWORD_REQUIRED"));
    assert!(!target.exists());
}

#[test]
fn protect_refuses_an_empty_password_file() {
    let sample = crate::require_sample!();
    let dir = tempdir("protect-emptypw");
    // A zero-byte file and a file whose first line is empty both refuse:
    // neither may silently encrypt with an empty password and report success.
    let empty = dir.join("empty.txt");
    std::fs::write(&empty, b"").unwrap();
    let blank = common::password_file(&dir, "blank.txt", &[""]);
    for pw in [&empty, &blank] {
        let tag = pw.file_stem().unwrap().to_string_lossy().to_string();
        let target = dir.join(format!("x-{tag}.pdf"));
        let out = run(&[
            "protect",
            &sample.to_string_lossy(),
            "--password-file",
            &pw.to_string_lossy(),
            "-o",
            &target.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(2), "{tag}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("PASSWORD_REQUIRED"),
            "{tag}"
        );
        assert!(!target.exists(), "{tag}: no output may be left behind");
    }
}

#[test]
fn protect_password_file_line_two_wins_over_owner_env() {
    let sample = crate::require_sample!();
    let dir = tempdir("protect-ownerprec");
    // The explicit flag wins over the environment: line 2 of --password-file
    // is the owner password even when PDFLUENT_OWNER_PASSWORD is set.
    let pw = common::password_file(&dir, "pw.txt", &["user-secret", "file-owner"]);
    let locked = dir.join("locked.pdf");
    let out = run_with_env(
        &[
            "protect",
            &sample.to_string_lossy(),
            "--password-file",
            &pw.to_string_lossy(),
            "-o",
            &locked.to_string_lossy(),
        ],
        &[("PDFLUENT_OWNER_PASSWORD", "env-owner")],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // An owner password opens the document (removal itself is the
    // DECRYPT_NOT_SUPPORTED gap); a wrong one is BAD_PASSWORD.
    for (candidate, code) in [("file-owner", 3), ("env-owner", 2)] {
        let probe = common::password_file(&dir, &format!("probe-{candidate}.txt"), &[candidate]);
        let plain = dir.join(format!("plain-{candidate}.pdf"));
        let out = run(&[
            "unprotect",
            &locked.to_string_lossy(),
            "--password-file",
            &probe.to_string_lossy(),
            "-o",
            &plain.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(code), "{candidate}");
    }

    // And --owner-password-file still wins over both.
    let owner_file = common::password_file(&dir, "owner.txt", &["flag-owner"]);
    let locked2 = dir.join("locked2.pdf");
    let out = run_with_env(
        &[
            "protect",
            &sample.to_string_lossy(),
            "--password-file",
            &pw.to_string_lossy(),
            "--owner-password-file",
            &owner_file.to_string_lossy(),
            "-o",
            &locked2.to_string_lossy(),
        ],
        &[("PDFLUENT_OWNER_PASSWORD", "env-owner")],
    );
    assert!(out.status.success());
    for (candidate, code) in [("flag-owner", 3), ("file-owner", 2), ("env-owner", 2)] {
        let probe = common::password_file(&dir, &format!("probe2-{candidate}.txt"), &[candidate]);
        let plain = dir.join(format!("plain2-{candidate}.pdf"));
        let out = run(&[
            "unprotect",
            &locked2.to_string_lossy(),
            "--password-file",
            &probe.to_string_lossy(),
            "-o",
            &plain.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(code), "{candidate}");
    }
}

#[test]
fn unprotect_lifts_owner_restrictions() {
    let sample = crate::require_sample!();
    let dir = tempdir("unprotect-owner");
    // Empty user password + owner password: opens freely, restricted. Built
    // through the facade: the CLI refuses an empty user password outright.
    let restricted = dir.join("restricted.pdf");
    let bytes = std::fs::read(&sample).unwrap();
    let mut doc = pdfluent::PdfDocument::from_bytes(&bytes).unwrap();
    doc.encrypt(
        pdfluent::EncryptOptions::aes256()
            .with_user_password("")
            .with_owner_password("owner-pw"),
    )
    .unwrap();
    doc.save_with(
        &restricted,
        pdfluent::SaveOptions::new().with_overwrite(true),
    )
    .unwrap();
    assert_eq!(page_count_of(&restricted), 1);

    let owner = common::password_file(&dir, "owner.txt", &["owner-pw"]);
    let plain = dir.join("plain.pdf");
    let out = run(&[
        "unprotect",
        &restricted.to_string_lossy(),
        "--password-file",
        &owner.to_string_lossy(),
        "-o",
        &plain.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text_of(&plain).contains("PDFluent test fixture."));
}

#[test]
fn unprotect_passes_plain_input_through() {
    let sample = crate::require_sample!();
    let dir = tempdir("unprotect-plain");
    let plain = dir.join("plain.pdf");
    let out = run(&[
        "unprotect",
        &sample.to_string_lossy(),
        "-o",
        &plain.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(text_of(&plain).contains("PDFluent test fixture."));
}

#[test]
fn unprotect_wrong_password_is_an_input_error() {
    let sample = crate::require_sample!();
    let dir = tempdir("unprotect-wrong");
    let pw = common::password_file(&dir, "pw.txt", &["right"]);
    let locked = dir.join("locked.pdf");
    let out = run(&[
        "protect",
        &sample.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "-o",
        &locked.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let wrong = common::password_file(&dir, "wrong.txt", &["wrong"]);
    let out = run(&[
        "unprotect",
        &locked.to_string_lossy(),
        "--password-file",
        &wrong.to_string_lossy(),
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("BAD_PASSWORD"));
}

#[test]
fn unprotect_reports_user_password_removal_as_a_facade_gap() {
    let sample = crate::require_sample!();
    let dir = tempdir("unprotect-gap");
    let pw = common::password_file(&dir, "pw.txt", &["s3cret"]);
    let locked = dir.join("locked.pdf");
    let out = run(&[
        "protect",
        &sample.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "-o",
        &locked.to_string_lossy(),
    ]);
    assert!(out.status.success());
    // The password is right — removal itself is what the facade cannot do.
    let plain = dir.join("plain.pdf");
    let out = run(&[
        "unprotect",
        &locked.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "-o",
        &plain.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("DECRYPT_NOT_SUPPORTED"), "{err}");
    assert!(err.contains("facade gap"), "{err}");
    assert!(!plain.exists(), "no corrupt output may be left behind");
}

#[test]
fn redact_search_removes_text_permanently() {
    let sample = crate::require_sample!();
    let dir = tempdir("redact-search");
    let target = dir.join("clean.pdf");
    let out = run(&[
        "redact",
        &sample.to_string_lossy(),
        "--search",
        "fixture",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!text_of(&target).contains("fixture"));
}

#[test]
fn redact_regex_and_boxes_cover_regions() {
    let dir = tempdir("redact-modes");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, common::pdf_with_texts(&["call 555-0134 today"])).unwrap();

    let by_regex = dir.join("regex.pdf");
    let out = run(&[
        "redact",
        &doc.to_string_lossy(),
        "--regex",
        r"\d{3}-\d{4}",
        "-o",
        &by_regex.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!text_of(&by_regex).contains("555-0134"));

    let boxes = common::text_file(&dir, "boxes.json", r#"[{"page":1,"rect":[0,0,612,792]}]"#);
    let by_boxes = dir.join("boxed.pdf");
    let out = run(&[
        "redact",
        &doc.to_string_lossy(),
        "--boxes",
        &boxes.to_string_lossy(),
        "-o",
        &by_boxes.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!text_of(&by_boxes).contains("555-0134"));
}

#[test]
fn redact_rejects_bad_modes_and_boxes() {
    let sample = crate::require_sample!();
    // No mode at all.
    let out = run(&["redact", &sample.to_string_lossy(), "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(1));
    // Malformed boxes file.
    let dir = tempdir("redact-badboxes");
    let bad = common::text_file(&dir, "bad.json", "not json");
    let out = run(&[
        "redact",
        &sample.to_string_lossy(),
        "--boxes",
        &bad.to_string_lossy(),
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
    // Box past the last page.
    let far = common::text_file(&dir, "far.json", r#"[{"page":9,"rect":[0,0,10,10]}]"#);
    let out = run(&[
        "redact",
        &sample.to_string_lossy(),
        "--boxes",
        &far.to_string_lossy(),
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("PAGE_RANGE"));
}

#[test]
fn sign_basic_marks_and_opens() {
    let sample = crate::require_sample!();
    let (p12, password) = crate::require_p12!();
    let dir = tempdir("sign");
    let pw = common::password_file(&dir, "p12.txt", &[password]);
    let signed = dir.join("signed.pdf");
    let out = run(&[
        "sign",
        &sample.to_string_lossy(),
        "--p12",
        &p12.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "--reason",
        "approval",
        "--location",
        "Amsterdam",
        "--field",
        "Approval",
        "-o",
        &signed.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&signed), 1);
    // A signature lives in the bytes now.
    let bytes = std::fs::read(&signed).unwrap();
    assert!(bytes
        .windows(b"/ByteRange".len())
        .any(|w| w == b"/ByteRange"));
}

#[test]
fn sign_wrong_identity_password_is_an_input_error() {
    let sample = crate::require_sample!();
    let (p12, _) = crate::require_p12!();
    let dir = tempdir("sign-wrong");
    let wrong = common::password_file(&dir, "wrong.txt", &["wrong"]);
    let out = run(&[
        "sign",
        &sample.to_string_lossy(),
        "--p12",
        &p12.to_string_lossy(),
        "--password-file",
        &wrong.to_string_lossy(),
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("INVALID_IDENTITY"));
}

#[test]
fn sign_reports_timestamping_as_a_facade_gap() {
    let sample = crate::require_sample!();
    let (p12, password) = crate::require_p12!();
    let dir = tempdir("sign-tsa");
    let pw = common::password_file(&dir, "p12.txt", &[password]);
    let out = run(&[
        "sign",
        &sample.to_string_lossy(),
        "--p12",
        &p12.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "--tsa",
        "https://tsa.example.com",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("TSA_NOT_SUPPORTED"), "{err}");
    assert!(err.contains("facade gap"), "{err}");

    let out = run(&[
        "sign",
        &sample.to_string_lossy(),
        "--p12",
        &p12.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "--profile",
        "t",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("TSA_NOT_SUPPORTED"));
    // stdout stays parseable JSON even on gap errors.
    let out = run(&[
        "--json",
        "sign",
        &sample.to_string_lossy(),
        "--p12",
        &p12.to_string_lossy(),
        "--password-file",
        &pw.to_string_lossy(),
        "--profile",
        "t",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(stdout_json(&out)["error"]["code"], "TSA_NOT_SUPPORTED");
    assert_eq!(stdout_text(&out).matches("\"ok\"").count(), 1);
}
