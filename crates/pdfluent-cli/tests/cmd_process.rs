//! Behaviour tests for `pdfluent compress`, `watermark`, and `convert`.
//!
//! Office and image outputs are asserted by magic bytes plus JSON size, so
//! the tests pin "a real .docx / PNG came out" without parsing either.

// CLI covers: compress
// CLI covers: watermark
// CLI covers: convert --to docx
// CLI covers: convert --to xlsx
// CLI covers: convert --to pptx
// CLI covers: convert --to png
// CLI covers: convert --to jpeg
// CLI covers: convert --to pdfa

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{page_count_of, run, stdout_json, tempdir, text_of};

#[test]
fn compress_roundtrip_opens_and_reports() {
    let sample = crate::require_sample!();
    let dir = tempdir("compress");
    let target = dir.join("small.pdf");
    let arg = target.to_string_lossy().to_string();
    let out = run(&["--json", "compress", &sample.to_string_lossy(), "-o", &arg]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = stdout_json(&out);
    assert_eq!(value["data"]["bytes_before"], 784);
    assert!(value["data"]["bytes_after"].as_u64().unwrap() > 0);
    assert_eq!(page_count_of(&target), 1);
    assert!(text_of(&target).contains("PDFluent test fixture."));
}

#[test]
fn compress_levels_are_accepted() {
    let sample = crate::require_sample!();
    let dir = tempdir("compress-levels");
    for level in ["strict", "lossy", "archival"] {
        let target = dir.join(format!("{level}.pdf"));
        let out = run(&[
            "compress",
            &sample.to_string_lossy(),
            "--level",
            level,
            "-o",
            &target.to_string_lossy(),
        ]);
        assert!(
            out.status.success(),
            "{level}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(page_count_of(&target), 1);
    }
}

#[test]
fn compress_stdout_reports_the_real_size() {
    let sample = crate::require_sample!();
    let out = run(&["compress", &sample.to_string_lossy(), "-o", "-"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.starts_with(b"%PDF-"));
    // "compressed <before> -> <after> bytes", and <after> is what stdout got.
    let err = String::from_utf8_lossy(&out.stderr);
    let rest = err
        .split("compressed ")
        .nth(1)
        .expect("a size note on stderr")
        .to_string();
    let after: u64 = rest
        .split("->")
        .nth(1)
        .expect("before -> after")
        .split_whitespace()
        .next()
        .expect("a byte count")
        .parse()
        .expect("a numeric byte count");
    assert!(after > 0, "bytes_after must not be 0: {err}");
    assert_eq!(after, out.stdout.len() as u64, "{err}");
}

#[test]
fn compress_fails_on_missing_or_corrupt() {
    let out = run(&["compress", "no-such.pdf", "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(2));
    let dir = tempdir("compress-corrupt");
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, common::corrupt_bytes()).unwrap();
    let out = run(&["compress", &bad.to_string_lossy(), "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn watermark_text_keeps_a_readable_document() {
    let sample = crate::require_sample!();
    let dir = tempdir("watermark-text");
    let target = dir.join("marked.pdf");
    let out = run(&[
        "watermark",
        &sample.to_string_lossy(),
        "--text",
        "DRAFT",
        "--position",
        "bottom-right",
        "--opacity",
        "0.25",
        "--font-size",
        "24",
        "--color",
        "FF0000",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&target), 1);
    assert!(text_of(&target).contains("PDFluent test fixture."));
    // Watermark text joins the page content (the facade stamps real text).
    assert!(text_of(&target).contains("DRAFT"));
}

#[test]
fn watermark_image_inserts_and_opens() {
    let sample = crate::require_sample!();
    let dir = tempdir("watermark-image");
    let jpeg = dir.join("mark.jpg");
    std::fs::write(&jpeg, common::tiny_jpeg(120, 60)).unwrap();
    let target = dir.join("marked.pdf");
    let out = run(&[
        "watermark",
        &sample.to_string_lossy(),
        "--image",
        &jpeg.to_string_lossy(),
        "--width",
        "200",
        "-o",
        &target.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&target), 1);
}

#[test]
fn watermark_rejects_bad_invocations() {
    let sample = crate::require_sample!();
    // Neither --text nor --image.
    let out = run(&["watermark", &sample.to_string_lossy(), "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(1));
    // Not a colour.
    let out = run(&[
        "watermark",
        &sample.to_string_lossy(),
        "--text",
        "X",
        "--color",
        "red",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(1));
    // Not an image.
    let dir = tempdir("watermark-badimg");
    let fake = common::text_file(&dir, "fake.png", "definitely not image bytes");
    let out = run(&[
        "watermark",
        &sample.to_string_lossy(),
        "--image",
        &fake.to_string_lossy(),
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn convert_office_targets_are_zip_archives() {
    let sample = crate::require_sample!();
    let dir = tempdir("convert-office");
    // One explicit call per format, each spelled literally: the drift gate
    // (`tests/cli_drift.rs`) proves `// CLI covers: convert --to <fmt>` from
    // the argv text, which a format held in a loop variable would hide.
    for target in ["docx", "xlsx", "pptx"] {
        let out_path = dir.join(format!("out.{target}"));
        let arg = out_path.to_string_lossy().to_string();
        let out = match target {
            "docx" => run(&[
                "convert",
                &sample.to_string_lossy(),
                "--to",
                "docx",
                "-o",
                &arg,
                "--json",
            ]),
            "xlsx" => run(&[
                "convert",
                &sample.to_string_lossy(),
                "--to",
                "xlsx",
                "-o",
                &arg,
                "--json",
            ]),
            _ => run(&[
                "convert",
                &sample.to_string_lossy(),
                "--to",
                "pptx",
                "-o",
                &arg,
                "--json",
            ]),
        };
        assert!(
            out.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let bytes = std::fs::read(&out_path).unwrap();
        assert!(bytes.starts_with(b"PK"), "{target} is a zip: {bytes:?}");
        assert!(stdout_json(&out)["data"]["bytes"].as_u64().unwrap() > 0);
    }
}

#[test]
fn convert_images_render_one_file_per_page() {
    let dir = tempdir("convert-images");
    let doc = dir.join("two.pdf");
    std::fs::write(
        &doc,
        common::pdf_with_texts(&["page one here", "page two here"]),
    )
    .unwrap();
    let pngs = dir.join("pngs");
    let out = run(&[
        "convert",
        &doc.to_string_lossy(),
        "--to",
        "png",
        "--dpi",
        "72",
        "-o",
        &pngs.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for name in ["two-p1.png", "two-p2.png"] {
        let bytes = std::fs::read(pngs.join(name)).unwrap();
        assert!(
            bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "{name} is a PNG"
        );
    }
    let jpgs = dir.join("jpgs");
    let out = run(&[
        "convert",
        &doc.to_string_lossy(),
        "--to",
        "jpeg",
        "-o",
        &jpgs.to_string_lossy(),
    ]);
    assert!(out.status.success());
    let bytes = std::fs::read(jpgs.join("two-p1.jpg")).unwrap();
    assert!(bytes.starts_with(&[0xFF, 0xD8]), "two-p1.jpg is a JPEG");
}

#[test]
fn convert_pdfa_reports_its_profile() {
    let sample = crate::require_sample!();
    let dir = tempdir("convert-pdfa");
    let target = dir.join("archived.pdf");
    let arg = target.to_string_lossy().to_string();
    let out = run(&[
        "convert",
        &sample.to_string_lossy(),
        "--to",
        "pdfa",
        "--profile",
        "a1b",
        "-o",
        &arg,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = stdout_json(&out);
    assert_eq!(value["data"]["profile"], "a1b");
    assert_eq!(page_count_of(&target), 1);
}

#[test]
fn convert_rejects_bad_targets_and_outputs() {
    let sample = crate::require_sample!();
    // Unknown --to is a clap usage error.
    let out = run(&[
        "convert",
        &sample.to_string_lossy(),
        "--to",
        "gif",
        "-o",
        "x",
    ]);
    assert_eq!(out.status.code(), Some(1));
    // Images need a directory, not stdout or a file.
    let out = run(&[
        "convert",
        &sample.to_string_lossy(),
        "--to",
        "png",
        "-o",
        "-",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["convert", "no-such.pdf", "--to", "docx", "-o", "x"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn convert_raster_speed_writes_pngs() {
    let dir = tempdir("convert-raster-speed");
    let doc = dir.join("two.pdf");
    std::fs::write(
        &doc,
        common::pdf_with_texts(&["page one here", "page two here"]),
    )
    .unwrap();
    let out_dir = dir.join("pngs");
    let out = run(&[
        "convert",
        &doc.to_string_lossy(),
        "--to",
        "png",
        "--raster",
        "speed",
        "--dpi",
        "72",
        "-o",
        &out_dir.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for name in ["two-p1.png", "two-p2.png"] {
        let bytes = std::fs::read(out_dir.join(name)).unwrap();
        assert!(
            bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "{name} is a PNG"
        );
    }
}

#[test]
fn convert_rejects_unknown_raster_mode() {
    let sample = crate::require_sample!();
    let out = run(&[
        "convert",
        &sample.to_string_lossy(),
        "--to",
        "png",
        "--raster",
        "turbo",
        "-o",
        "x",
    ]);
    assert_eq!(out.status.code(), Some(1));
}
