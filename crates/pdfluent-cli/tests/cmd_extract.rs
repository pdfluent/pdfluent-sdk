//! Behaviour tests for `pdfluent extract`.
//!
//! Golden on small fixtures: the Markdown/text of the shared sample, the
//! JSON shapes, and the batch contract (dirs, globs, jobs).

// CLI covers: extract

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{run, run_with_env, run_with_stdin, stdout_json, stdout_text, tempdir, text_file};

#[test]
fn extract_md_stdout_carries_fixture_text() {
    let sample = crate::require_sample!();
    let out = run(&["extract", &sample.to_string_lossy()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout_text(&out).contains("PDFluent test fixture."));
}

#[test]
fn extract_all_formats_to_explicit_files() {
    let sample = crate::require_sample!();
    let dir = tempdir("extract-formats");
    for (format, marker) in [
        ("md", "PDFluent test fixture."),
        ("text", "PDFluent test fixture."),
    ] {
        let target = dir.join(format!("out.{format}"));
        let out = run(&[
            "extract",
            &sample.to_string_lossy(),
            "--format",
            format,
            "-o",
            &target.to_string_lossy(),
        ]);
        assert!(out.status.success());
        let text = std::fs::read_to_string(&target).expect("output written");
        assert!(text.contains(marker), "{format}: {text}");
    }
    let json_target = dir.join("out.json");
    let out = run(&[
        "extract",
        &sample.to_string_lossy(),
        "--format",
        "json",
        "-o",
        &json_target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json_target).unwrap()).unwrap();
    assert!(value.get("blocks").is_some(), "block-model JSON: {value}");
    assert!(value.to_string().contains("PDFluent test fixture."));

    let chunks_target = dir.join("out.chunks.json");
    let out = run(&[
        "extract",
        &sample.to_string_lossy(),
        "--format",
        "chunks",
        "-o",
        &chunks_target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&chunks_target).unwrap()).unwrap();
    assert_eq!(value["schema_name"], "DoclingDocument");
    assert!(value.to_string().contains("PDFluent test fixture."));
}

#[test]
fn extract_pages_selects_before_rendering() {
    let dir = tempdir("extract-pages");
    let pdf = dir.join("two.pdf");
    std::fs::write(
        &pdf,
        common::pdf_with_texts(&["Alpha on page one", "Beta on page two"]),
    )
    .unwrap();
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "--format",
        "text",
        "--pages",
        "2",
    ]);
    assert!(out.status.success());
    let text = stdout_text(&out);
    assert!(text.contains("Beta on page two"), "{text}");
    assert!(!text.contains("Alpha on page one"), "{text}");
}

#[test]
fn extract_batch_dir_glob_and_recursive() {
    let dir = tempdir("extract-batch");
    let docs = dir.join("docs");
    let nested = docs.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(docs.join("a.pdf"), common::pdf_with_texts(&["doc A"])).unwrap();
    std::fs::write(docs.join("b.pdf"), common::pdf_with_texts(&["doc B"])).unwrap();
    std::fs::write(nested.join("c.pdf"), common::pdf_with_texts(&["doc C"])).unwrap();

    // A directory without --recursive takes the top level only.
    let flat = dir.join("flat");
    let out = run(&[
        "extract",
        &docs.to_string_lossy(),
        "-o",
        &flat.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(flat.join("a.md").exists());
    assert!(flat.join("b.md").exists());
    assert!(!flat.join("c.md").exists());

    // --recursive descends.
    let deep = dir.join("deep");
    let out = run(&[
        "extract",
        &docs.to_string_lossy(),
        "--recursive",
        "-o",
        &deep.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(deep.join("c.md").exists());

    // A quoted glob expands without a shell. One match is a single input,
    // so the trailing slash is what asks for a directory.
    let globbed = dir.join("globbed");
    let pattern = format!("{}/nested/*.pdf", docs.to_string_lossy());
    let slash_arg = format!("{}/", globbed.to_string_lossy());
    let out = run(&["extract", &pattern, "-o", &slash_arg]);
    assert!(out.status.success());
    assert!(globbed.join("c.md").exists());
    assert!(!globbed.join("a.md").exists());
}

#[test]
fn extract_refuses_same_named_inputs_sharing_an_output() {
    let dir = tempdir("extract-collision");
    for (year, text) in [("2024", "last year"), ("2025", "this year")] {
        let sub = dir.join(year);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("invoice.pdf"), common::pdf_with_texts(&[text])).unwrap();
    }
    let a = dir
        .join("2024")
        .join("invoice.pdf")
        .to_string_lossy()
        .to_string();
    let b = dir
        .join("2025")
        .join("invoice.pdf")
        .to_string_lossy()
        .to_string();
    // Both sequential and parallel: the collision is refused before any
    // write, not raced between workers.
    for jobs in ["1", "4"] {
        let outdir = dir.join(format!("out-{jobs}"));
        let out = run(&[
            "extract",
            &a,
            &b,
            "--jobs",
            jobs,
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(1), "jobs={jobs}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("invoice.md"), "jobs={jobs}: {err}");
        assert!(err.contains("2024"), "jobs={jobs}: {err}");
        assert!(err.contains("2025"), "jobs={jobs}: {err}");
        let written: Vec<_> = std::fs::read_dir(&outdir)
            .map(|entries| entries.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();
        assert!(written.is_empty(), "jobs={jobs}: nothing may be written");
    }
}

#[test]
fn extract_refuses_case_insensitive_collision() {
    // A/Invoice.pdf and B/invoice.pdf map to out/Invoice.md and
    // out/invoice.md: distinct on Linux, the same file on the
    // case-insensitive filesystems of macOS and Windows. The batch is
    // refused on every platform, before any write.
    let dir = tempdir("extract-collision-ci");
    for (sub, name, text) in [("a", "Invoice.pdf", "upper"), ("b", "invoice.pdf", "lower")] {
        let folder = dir.join(sub);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), common::pdf_with_texts(&[text])).unwrap();
    }
    let upper = dir
        .join("a")
        .join("Invoice.pdf")
        .to_string_lossy()
        .to_string();
    let lower = dir
        .join("b")
        .join("invoice.pdf")
        .to_string_lossy()
        .to_string();
    for jobs in ["1", "4"] {
        let outdir = dir.join(format!("out-ci-{jobs}"));
        let out = run(&[
            "extract",
            &upper,
            &lower,
            "--jobs",
            jobs,
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(1), "jobs={jobs}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("Invoice"), "jobs={jobs}: {err}");
        assert!(err.contains("invoice"), "jobs={jobs}: {err}");
        let written: Vec<_> = std::fs::read_dir(&outdir)
            .map(|entries| entries.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();
        assert!(written.is_empty(), "jobs={jobs}: nothing may be written");
    }
}

#[test]
fn extract_streams_inputs_without_loading_all_upfront() {
    let dir = tempdir("extract-stream");
    let good = dir.join("good.pdf");
    std::fs::write(&good, common::pdf_with_texts(&["good content"])).unwrap();
    let missing = dir.join("missing.pdf").to_string_lossy().to_string();
    // A batch that fails midway keeps the outputs written so far: inputs
    // are loaded, extracted, and written one at a time, not all upfront.
    for jobs in ["1", "2"] {
        let outdir = dir.join(format!("out-{jobs}"));
        let out = run(&[
            "extract",
            &good.to_string_lossy(),
            &missing,
            "--jobs",
            jobs,
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(2), "jobs={jobs}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("FILE_NOT_FOUND"),
            "jobs={jobs}"
        );
        let text = std::fs::read_to_string(outdir.join("good.md"))
            .unwrap_or_else(|e| panic!("jobs={jobs}: the successful input is still written: {e}"));
        assert!(text.contains("good content"), "jobs={jobs}");
    }
}

#[test]
fn extract_stops_at_the_first_error_by_default() {
    // Default: the batch stops at the first failure, keeping earlier
    // outputs but never attempting later inputs. A re-run after fixing the
    // failed input needs --force only for what was written before it.
    let dir = tempdir("extract-stop");
    let good = dir.join("good.pdf");
    std::fs::write(&good, common::pdf_with_texts(&["good content"])).unwrap();
    let missing = dir.join("missing.pdf").to_string_lossy().to_string();
    let outdir = dir.join("out");
    let out = run(&[
        "extract",
        &missing,
        &good.to_string_lossy(),
        "-o",
        &outdir.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("FILE_NOT_FOUND"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !outdir.join("good.md").exists(),
        "inputs after the failure are not attempted by default"
    );
}

#[test]
fn extract_keep_going_writes_later_successes() {
    // --keep-going: every input is attempted and later successes are still
    // written; the first error in input order is reported, and a re-run
    // then needs --force for what was written.
    let dir = tempdir("extract-keep-going");
    let good = dir.join("good.pdf");
    std::fs::write(&good, common::pdf_with_texts(&["good content"])).unwrap();
    let missing = dir.join("missing.pdf").to_string_lossy().to_string();
    for jobs in ["1", "4"] {
        let outdir = dir.join(format!("out-{jobs}"));
        let out = run(&[
            "extract",
            &missing,
            &good.to_string_lossy(),
            "--keep-going",
            "--jobs",
            jobs,
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(2), "jobs={jobs}");
        let text = std::fs::read_to_string(outdir.join("good.md"))
            .unwrap_or_else(|e| panic!("jobs={jobs}: later success written: {e}"));
        assert!(text.contains("good content"), "jobs={jobs}");
        // A re-run trips --force for what --keep-going wrote.
        let rerun = run(&[
            "extract",
            &missing,
            &good.to_string_lossy(),
            "--keep-going",
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(rerun.status.code(), Some(1), "jobs={jobs}");
        assert!(
            String::from_utf8_lossy(&rerun.stderr).contains("--force"),
            "jobs={jobs}"
        );
    }
}

#[test]
fn extract_keep_going_reports_every_error_and_every_success() {
    // --keep-going with two failures and one success: both errors are
    // reported and the success is still written AND reported (human
    // `Wrote` notes, JSON `results`), instead of vanishing behind the
    // first error.
    let dir = tempdir("extract-keep-going-all");
    let good = dir.join("good.pdf");
    std::fs::write(&good, common::pdf_with_texts(&["good content"])).unwrap();
    let missing_a = dir.join("missing-a.pdf").to_string_lossy().to_string();
    let missing_b = dir.join("missing-b.pdf").to_string_lossy().to_string();
    let good_arg = good.to_string_lossy().to_string();
    for jobs in ["1", "4"] {
        let outdir = dir.join(format!("out-{jobs}"));
        let out = run(&[
            "extract",
            &missing_a,
            &good_arg,
            &missing_b,
            "--keep-going",
            "--jobs",
            jobs,
            "-o",
            &outdir.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(2), "jobs={jobs}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("missing-a.pdf"), "jobs={jobs}: {err}");
        assert!(err.contains("missing-b.pdf"), "jobs={jobs}: {err}");
        let text = std::fs::read_to_string(outdir.join("good.md"))
            .unwrap_or_else(|e| panic!("jobs={jobs}: success written: {e}"));
        assert!(text.contains("good content"), "jobs={jobs}");
        let human = stdout_text(&out);
        assert!(
            human.contains("Wrote"),
            "jobs={jobs}: success reported: {human}"
        );
        assert!(human.contains("good.md"), "jobs={jobs}: {human}");
    }
    // JSON mode: the first error stays the envelope error, with every
    // success under `results` and every failure under `errors`.
    let outdir = dir.join("out-json");
    let out = run(&[
        "--json",
        "extract",
        &missing_a,
        &good_arg,
        &missing_b,
        "--keep-going",
        "-o",
        &outdir.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let value = stdout_json(&out);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "FILE_NOT_FOUND");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing-a.pdf"));
    let results = value["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);
    assert!(results[0]["output_path"]
        .as_str()
        .unwrap()
        .ends_with("good.md"));
    let errors = value["data"]["errors"].as_array().expect("errors");
    assert_eq!(errors.len(), 2);
    assert!(errors[0]["message"]
        .as_str()
        .unwrap()
        .contains("missing-a.pdf"));
    assert!(errors[1]["message"]
        .as_str()
        .unwrap()
        .contains("missing-b.pdf"));
}

#[test]
fn extract_keep_going_human_errors_name_the_input() {
    // --keep-going in human mode: every error line names the input that
    // failed. Corrupt files carry no filename in their message, so without
    // the name the two failures below are indistinguishable.
    let dir = tempdir("extract-keep-going-names");
    let bad_a = dir.join("bad-a.pdf");
    let bad_b = dir.join("bad-b.pdf");
    std::fs::write(&bad_a, common::corrupt_bytes()).unwrap();
    std::fs::write(&bad_b, common::corrupt_bytes()).unwrap();
    let out = run(&[
        "extract",
        &bad_a.to_string_lossy(),
        &bad_b.to_string_lossy(),
        "--keep-going",
        "-o",
        &dir.join("out").to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("bad-a.pdf"), "{err}");
    assert!(err.contains("bad-b.pdf"), "{err}");
}

#[test]
fn extract_jobs_without_keep_going_stays_parallel_and_cancels() {
    // --jobs N without --keep-going is parallel (no serial warning), and
    // still stops at the first error: earlier outputs are kept, later
    // inputs are never written.
    let dir = tempdir("extract-jobs-cancel");
    let good = dir.join("good.pdf");
    std::fs::write(&good, common::pdf_with_texts(&["good content"])).unwrap();
    let missing = dir.join("missing.pdf").to_string_lossy().to_string();
    let good_arg = good.to_string_lossy().to_string();
    // A successful batch carries the proof it ran parallel: no serial
    // fallback warning (warnings only surface on success, so a failing
    // batch cannot show it either way).
    let okdir = dir.join("ok");
    let second = dir.join("second.pdf");
    std::fs::write(&second, common::pdf_with_texts(&["second content"])).unwrap();
    let out = run(&[
        "extract",
        &good_arg,
        &second.to_string_lossy(),
        "--jobs",
        "4",
        "-o",
        &okdir.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("one input at a time"),
        "no serial fallback warning: {err}"
    );
    assert!(okdir.join("good.md").exists());
    assert!(okdir.join("second.md").exists());
    let outdir = dir.join("out");
    let out = run(&[
        "extract",
        &good_arg,
        &missing,
        "--jobs",
        "4",
        "-o",
        &outdir.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("one input at a time"),
        "no serial fallback warning: {err}"
    );
    assert!(err.contains("FILE_NOT_FOUND"), "{err}");
    let text = std::fs::read_to_string(outdir.join("good.md"))
        .expect("the successful input is still written");
    assert!(text.contains("good content"));
    // The failure first: inputs after it are cancelled, never written --
    // deterministically, however the renders overlapped.
    let outdir2 = dir.join("out2");
    let out = run(&[
        "extract",
        &missing,
        &good_arg,
        "--jobs",
        "4",
        "-o",
        &outdir2.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !outdir2.join("good.md").exists(),
        "inputs after the failure are cancelled, not written"
    );
}

#[test]
fn extract_jobs_match_sequential_output() {
    let dir = tempdir("extract-jobs");
    std::fs::write(dir.join("a.pdf"), common::pdf_with_texts(&["doc A"])).unwrap();
    std::fs::write(dir.join("b.pdf"), common::pdf_with_texts(&["doc B"])).unwrap();
    let seq = dir.join("seq");
    let par = dir.join("par");
    let docs = dir.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::copy(dir.join("a.pdf"), docs.join("a.pdf")).unwrap();
    std::fs::copy(dir.join("b.pdf"), docs.join("b.pdf")).unwrap();
    let out = run(&[
        "extract",
        &docs.to_string_lossy(),
        "-o",
        &seq.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let out = run(&[
        "extract",
        &docs.to_string_lossy(),
        "--keep-going",
        "--jobs",
        "4",
        "-o",
        &par.to_string_lossy(),
    ]);
    assert!(out.status.success());
    for name in ["a.md", "b.md"] {
        assert_eq!(
            std::fs::read(seq.join(name)).unwrap(),
            std::fs::read(par.join(name)).unwrap(),
            "{name} differs between sequential and parallel"
        );
    }
}

#[test]
fn extract_json_modes_carry_results() {
    let sample = crate::require_sample!();
    // Stdout + --json inlines the text.
    let out = run(&[
        "extract",
        "--format",
        "text",
        &sample.to_string_lossy(),
        "--json",
    ]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["ok"], true);
    assert_eq!(value["command"], "extract");
    assert!(value["data"]["text"]
        .as_str()
        .unwrap()
        .contains("PDFluent test fixture."));

    // File mode + --json lists results.
    let dir = tempdir("extract-json");
    let target = dir.join("out.md");
    let out = run(&[
        "--json",
        "extract",
        &sample.to_string_lossy(),
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let value = stdout_json(&out);
    assert_eq!(value["data"]["results"][0]["page_count"], 1);
    assert!(value["data"]["results"][0]["output_path"]
        .as_str()
        .unwrap()
        .ends_with("out.md"));
}

#[test]
fn extract_stdin_reads_a_single_document() {
    let sample = crate::require_sample!();
    let bytes = std::fs::read(&sample).unwrap();
    let out = run_with_stdin(&["extract", "-", "--format", "text"], &bytes);
    assert!(out.status.success());
    assert!(stdout_text(&out).contains("PDFluent test fixture."));
}

#[test]
fn extract_refuses_nfc_nfd_collision() {
    // café in NFC (U+00E9) vs NFD (e + U+0301): distinct bytes, the same
    // file on macOS, where readdir spells every é decomposed. The batch
    // is refused on every platform, before any write.
    let dir = tempdir("extract-collision-nfc");
    for (sub, name, text) in [
        ("nfc", "caf\u{e9}.pdf", "composed"),
        ("nfd", "cafe\u{301}.pdf", "decomposed"),
    ] {
        let folder = dir.join(sub);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), common::pdf_with_texts(&[text])).unwrap();
    }
    let composed = dir
        .join("nfc")
        .join("caf\u{e9}.pdf")
        .to_string_lossy()
        .to_string();
    let decomposed = dir
        .join("nfd")
        .join("cafe\u{301}.pdf")
        .to_string_lossy()
        .to_string();
    for jobs in ["1", "4"] {
        for extra in [vec!["--keep-going"], vec![]] {
            let tag = format!("jobs-{jobs}-keep-{}", extra.len());
            let outdir = dir.join(format!("out-{tag}"));
            let outdir_arg = outdir.to_string_lossy().to_string();
            let mut argv = vec![
                "extract",
                composed.as_str(),
                decomposed.as_str(),
                "--jobs",
                jobs,
            ];
            argv.extend(extra.iter().copied());
            argv.extend(["-o", outdir_arg.as_str()]);
            let out = run(&argv);
            assert_eq!(out.status.code(), Some(1), "{tag}");
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(err.contains("caf"), "{tag}: {err}");
            let written: Vec<_> = std::fs::read_dir(&outdir)
                .map(|entries| entries.filter_map(|e| e.ok()).collect())
                .unwrap_or_default();
            assert!(written.is_empty(), "{tag}: nothing may be written");
        }
    }
}

#[test]
fn extract_refuses_to_rewrite_its_input() {
    let dir = tempdir("extract-self");
    let pdf = dir.join("a.pdf");
    let bytes = common::pdf_with_texts(&["stays a PDF"]);
    std::fs::write(&pdf, &bytes).unwrap();
    let arg = pdf.to_string_lossy().to_string();
    let out = run(&["extract", &arg, "-o", &arg]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("is the input file"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(&pdf).unwrap(), bytes);
}

#[test]
fn extract_refuses_existing_output_without_force() {
    let dir = tempdir("extract-clobber");
    let pdf = dir.join("a.pdf");
    std::fs::write(&pdf, common::pdf_with_texts(&["fresh text"])).unwrap();
    let target = dir.join("out.md");
    std::fs::write(&target, "previous run").unwrap();
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "-o",
        &target.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--force"), "{err}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "previous run");
    // ... and --force overwrites.
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "--force",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(std::fs::read_to_string(&target)
        .unwrap()
        .contains("fresh text"));
    // Atomic writes leave no temp siblings behind.
    let litter: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp."))
        .collect();
    assert!(litter.is_empty(), "temp files left behind: {litter:?}");
}

#[test]
fn extract_fails_on_corrupt_input() {
    let dir = tempdir("extract-corrupt");
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, common::corrupt_bytes()).unwrap();
    let out = run(&["extract", &bad.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("INVALID_PDF"));
}

#[test]
fn extract_fails_on_missing_input() {
    let out = run(&["extract", "no-such-file.pdf"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn extract_fails_on_unshipped_best_quality() {
    let sample = crate::require_sample!();
    let out = run(&["extract", &sample.to_string_lossy(), "--quality", "best"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_SHIPPED"), "{err}");
    assert!(err.contains("pdfluent models install layout"), "{err}");
}

#[test]
fn extract_balanced_without_the_model_fails_naming_the_install() {
    // Both builds agree on the code and the install command: the default
    // build names the `ort-layout` cargo feature, the feature build the
    // missing path. The message always names `pdfluent models install
    // layout`. The model dir is a scratch dir, so the test holds even on a
    // machine with models installed.
    let sample = crate::require_sample!();
    let dir = tempdir("extract-balanced-no-model");
    let model_dir = dir.to_string_lossy().to_string();
    let env = [("PDFLUENT_MODEL_DIR", model_dir.as_str())];
    let out = run_with_env(
        &[
            "extract",
            &sample.to_string_lossy(),
            "--quality",
            "balanced",
        ],
        &env,
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(err.contains("pdfluent models install layout"), "{err}");

    let out = run_with_env(
        &[
            "--json",
            "extract",
            &sample.to_string_lossy(),
            "--quality",
            "balanced",
        ],
        &env,
    );
    assert_eq!(out.status.code(), Some(3));
    let json = stdout_json(&out);
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "MODEL_NOT_AVAILABLE");
    assert_eq!(json["error"]["exit_code"], 3);
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("pdfluent models install layout"),
        "{json}"
    );
}

#[cfg(feature = "ort-layout")]
#[test]
fn extract_balanced_with_a_missing_model_dir_names_the_path() {
    // The feature build resolves `--model-dir` first: a scratch dir names
    // the missing file under it.
    let sample = crate::require_sample!();
    let dir = tempdir("extract-balanced-model-dir");
    let missing = dir.join("no-such-dir");
    let out = run(&[
        "extract",
        &sample.to_string_lossy(),
        "--quality",
        "balanced",
        "--model-dir",
        &missing.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(
        err.contains(&missing.to_string_lossy().to_string()),
        "the message names the path looked at: {err}"
    );
    assert!(err.contains("pdfluent models install layout"), "{err}");
}

/// Asking for TATR on a build without it refuses with the engine's own
/// `TatrNeedsOrtTables` message, before anything is planned or read -- on
/// both qualities, since the flag opts into TATR for whichever tier runs.
/// Without the refusal the default build would answer `MODEL_NOT_AVAILABLE`
/// for the layout model instead, and the flag would read as accepted.
#[cfg(not(feature = "ort-tables"))]
#[test]
fn extract_table_model_dir_without_the_feature_refuses() {
    let sample = crate::require_sample!();
    let dir = tempdir("extract-tatr-no-feature");
    let models = dir.join("models");
    for quality in ["fast", "balanced"] {
        let out = run(&[
            "extract",
            &sample.to_string_lossy(),
            "--quality",
            quality,
            "--table-model-dir",
            &models.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(3));
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("tatr table structure needs the `ort-tables` build"),
            "{quality}: {err}"
        );
    }
}

/// The flag reaches the TATR loader: with the feature, a missing TATR model
/// refuses naming the package, before any region runs. The fast tier needs
/// no other model, so this proves the wiring through the binary without a
/// model or a dylib. Without the mapping the same run succeeds.
#[cfg(feature = "ort-tables")]
#[test]
fn extract_fast_with_table_model_dir_missing_model_names_tatr() {
    let sample = crate::require_sample!();
    let dir = tempdir("extract-tatr-missing-model");
    let missing = dir.join("no-such-dir");
    let out = run(&[
        "extract",
        &sample.to_string_lossy(),
        "--table-model-dir",
        &missing.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("MODEL_NOT_AVAILABLE"), "{err}");
    assert!(
        err.contains("tatr-v1.1-all"),
        "the refusal names the package looked for: {err}"
    );
}

#[test]
fn extract_reports_office_inputs_as_a_facade_gap() {
    let dir = tempdir("extract-gap");
    let docx = text_file(&dir, "report.docx", "PK fake zip, not a PDF");
    let out = run(&["extract", &docx.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("UNSUPPORTED_INPUT"), "{err}");
    assert!(err.contains("facade"), "{err}");
}

#[test]
fn extract_refuses_multi_input_stdout() {
    let dir = tempdir("extract-multi-stdout");
    std::fs::write(dir.join("a.pdf"), common::pdf_with_texts(&["A"])).unwrap();
    std::fs::write(dir.join("b.pdf"), common::pdf_with_texts(&["B"])).unwrap();
    let a = dir.join("a.pdf").to_string_lossy().to_string();
    let b = dir.join("b.pdf").to_string_lossy().to_string();
    let out = run(&["extract", &a, &b]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn extract_rejects_bad_page_specs() {
    let sample = crate::require_sample!();
    let out = run(&["extract", &sample.to_string_lossy(), "--pages", "abc"]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["extract", &sample.to_string_lossy(), "--pages", "9-10"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("PAGE_RANGE"));
}

/// A one-page PDF drawing `text` glyph by glyph, each with its own text
/// matrix at an exact origin in Courier -- whose every glyph advances 600
/// of the font size -- so every word box below is arithmetic. Word gaps
/// are positioning, one advance each, not space glyphs.
fn courier_positioned_pdf(text: &str) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    const SIZE: f64 = 12.0;
    const ADVANCE: f64 = 0.6 * SIZE;

    let mut operations = Vec::new();
    let mut pos = 0.0;
    for ch in text.chars() {
        if ch == ' ' {
            pos += 1.0;
            continue;
        }
        let x = 72.0 + ADVANCE * pos;
        pos += 1.0;
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), SIZE.into()],
        ));
        operations.push(Operation::new(
            "Tm",
            vec![
                1.0.into(),
                0.0.into(),
                0.0.into(),
                1.0.into(),
                x.into(),
                700.0.into(),
            ],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(ch.to_string())],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations }.encode().expect("encode content");

    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

#[test]
fn extract_words_format_lists_every_word_with_its_box() {
    let dir = tempdir("extract-words");
    let pdf = dir.join("words.pdf");
    std::fs::write(&pdf, courier_positioned_pdf("AB CD")).expect("write fixture");
    let target = dir.join("out.words.jsonl");
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "--format",
        "words",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&target).expect("output written");
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line parses"))
        .collect();
    assert_eq!(rows.len(), 2, "one line per word: {text}");
    assert_eq!(rows[0]["text"], "AB");
    assert_eq!(rows[1]["text"], "CD");
    for (row, word) in rows.iter().zip([0, 1]) {
        assert_eq!(row["page"], 0);
        assert_eq!(row["block"], 0);
        assert_eq!(row["line"], 0);
        assert_eq!(row["word"], word);
        assert_eq!(row["dir"], 0.0);
    }
    // Courier at 12 points advances 7.2 a glyph from x 72: `AB` covers two
    // advances and `CD` two more past the positioned gap.
    for (row, expected) in rows
        .iter()
        .zip([[72.0, 700.0, 86.4, 712.0], [93.6, 700.0, 108.0, 712.0]])
    {
        let bbox = row["bbox"].as_array().expect("a word carries its box");
        assert_eq!(bbox.len(), 4);
        for (actual, expected) in bbox.iter().zip(expected) {
            assert!(
                (actual.as_f64().unwrap() - expected).abs() < 0.5,
                "the known word's box drifted: {bbox:?}"
            );
        }
    }
}

#[test]
fn extract_json_words_flag_adds_word_lines() {
    let dir = tempdir("extract-json-words");
    let pdf = dir.join("one.pdf");
    std::fs::write(&pdf, common::pdf_with_texts(&["Alpha on page one"])).expect("write fixture");
    let target = dir.join("out.json");
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "--format",
        "json",
        "--words",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
    let words = &value["blocks"][0]["lines"][0]["words"];
    assert!(
        words.as_array().is_some_and(|w| !w.is_empty()),
        "--words adds the word lines: {value}"
    );
    assert_eq!(words[0]["text"], "Alpha");
    // Without the flag the JSON is exactly what it was: no word lines.
    let plain_target = dir.join("plain.json");
    let out = run(&[
        "extract",
        &pdf.to_string_lossy(),
        "--format",
        "json",
        "-o",
        &plain_target.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let plain = std::fs::read_to_string(&plain_target).unwrap();
    assert!(
        !plain.contains("\"lines\""),
        "the default JSON carries word lines: {plain}"
    );
}

/// Run `extract` with the pool probe on and a hermetic thread setting: the
/// parent environment's `RAYON_NUM_THREADS`, if any, must not leak in.
fn run_extract_probed(args: &[&str], extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(common::bin());
    cmd.args(args)
        .env_remove("RAYON_NUM_THREADS")
        .env("PDFFLUENT_POOL_PROBE", "1");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.output().expect("spawn pdfluent")
}

fn eight_page_pdf(dir: &std::path::Path) -> std::path::PathBuf {
    let pdf = dir.join("eight.pdf");
    std::fs::write(
        &pdf,
        common::pdf_with_texts(&[
            "Pool page one",
            "Pool page two",
            "Pool page three",
            "Pool page four",
            "Pool page five",
            "Pool page six",
            "Pool page seven",
            "Pool page eight",
        ]),
    )
    .unwrap();
    pdf
}

#[test]
fn extract_jobs_one_runs_a_single_thread_pool() {
    // `--jobs 1` sizes the global rayon pool to one thread, so no idle
    // workers spin while pages render one at a time. The marker exists only
    // because of that sizing: without it this test is red.
    let dir = tempdir("extract-pool-one");
    let pdf = eight_page_pdf(&dir);
    let out = run_extract_probed(&["extract", &pdf.to_string_lossy(), "--jobs", "1"], &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pdfluent-pool: requested=1 effective=1"),
        "single-threaded pool: {err}"
    );
}

#[test]
fn extract_pool_probe_honours_rayon_num_threads() {
    // An explicit `RAYON_NUM_THREADS` wins over `--jobs 1`: the CLI leaves
    // the pool alone and rayon sizes it from the variable.
    let dir = tempdir("extract-pool-env");
    let pdf = eight_page_pdf(&dir);
    let out = run_extract_probed(
        &["extract", &pdf.to_string_lossy(), "--jobs", "1"],
        &[("RAYON_NUM_THREADS", "4")],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pdfluent-pool: requested=default effective=4"),
        "explicit setting honoured: {err}"
    );
}

#[test]
fn extract_jobs_four_leaves_the_default_pool() {
    // `--jobs N>1` behaviour is unchanged: the CLI does not size the pool.
    // The effective size is the machine default, so only the decision is
    // asserted here.
    let dir = tempdir("extract-pool-four");
    let pdf = eight_page_pdf(&dir);
    let out = run_extract_probed(&["extract", &pdf.to_string_lossy(), "--jobs", "4"], &[]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pdfluent-pool: requested=default"),
        "default pool kept: {err}"
    );
}

#[test]
fn extract_jobs_one_and_four_are_byte_identical() {
    // Pool sizing must not change results: one thread or the default pool,
    // the same bytes land. Markdown and block JSON both go through the
    // per-page parallel sections, so both are compared exactly.
    let dir = tempdir("extract-pool-identical");
    let pdf = eight_page_pdf(&dir);
    for format in ["md", "json"] {
        let mut outputs = Vec::new();
        for jobs in ["1", "4"] {
            let target = dir.join(format!("out-{jobs}.{format}"));
            let out = run(&[
                "extract",
                &pdf.to_string_lossy(),
                "--format",
                format,
                "--jobs",
                jobs,
                "-o",
                &target.to_string_lossy(),
            ]);
            assert!(
                out.status.success(),
                "jobs={jobs} {format}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            outputs.push(std::fs::read(&target).expect("output written"));
        }
        assert_eq!(
            outputs[0], outputs[1],
            "{format}: --jobs 1 and --jobs 4 differ"
        );
    }
}
