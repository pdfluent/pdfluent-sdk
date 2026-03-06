//! Display PDF document information.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_engine::PdfDocument;
use pdf_syntax::Pdf;

pub fn run(input: &Path, json: bool) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let doc = PdfDocument::open(data.clone()).context("failed to open PDF")?;
    let pdf = Pdf::new(data).map_err(|e| anyhow::anyhow!("pdf-syntax parse error: {e:?}"))?;

    let info = doc.info();
    let page_count = doc.page_count();

    // Collect page geometries.
    let mut pages_info = Vec::new();
    for i in 0..page_count {
        if let Ok(g) = doc.page_geometry(i) {
            pages_info.push(serde_json::json!({
                "page": i + 1,
                "media_box": {
                    "width": g.media_box.width(),
                    "height": g.media_box.height(),
                },
                "crop_box": {
                    "width": g.crop_box.width(),
                    "height": g.crop_box.height(),
                },
                "rotation": g.rotation.degrees(),
            }));
        }
    }

    // Signatures.
    let sigs = pdf_sign::signature_fields(&pdf);
    let sig_count = sigs.len();

    // DocMDP.
    let docmdp = pdf_sign::get_docmdp_permission(&pdf);

    // DSS.
    let dss = pdf_sign::DocumentSecurityStore::from_pdf(&pdf);

    // Bookmarks.
    let bookmarks = doc.bookmarks();

    if json {
        let result = serde_json::json!({
            "file": input.display().to_string(),
            "pages": page_count,
            "title": info.title,
            "author": info.author,
            "subject": info.subject,
            "keywords": info.keywords,
            "creator": info.creator,
            "producer": info.producer,
            "page_geometries": pages_info,
            "signatures": sig_count,
            "docmdp": docmdp.as_ref().map(|d| format!("{:?}", d.permission)),
            "dss": dss.as_ref().map(|d| serde_json::json!({
                "certificates": d.certificates.len(),
                "ocsp_responses": d.ocsp_responses.len(),
                "crls": d.crls.len(),
                "vri_entries": d.vri_entries.len(),
            })),
            "bookmarks": bookmarks.len(),
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("File: {}", input.display());
        println!("Pages: {page_count}");
        if let Some(t) = &info.title {
            println!("Title: {t}");
        }
        if let Some(a) = &info.author {
            println!("Author: {a}");
        }
        if let Some(s) = &info.subject {
            println!("Subject: {s}");
        }
        if let Some(k) = &info.keywords {
            println!("Keywords: {k}");
        }
        if let Some(c) = &info.creator {
            println!("Creator: {c}");
        }
        if let Some(p) = &info.producer {
            println!("Producer: {p}");
        }

        println!();
        for pi in &pages_info {
            println!(
                "  Page {}: {}x{} pt, rotation {}deg",
                pi["page"], pi["media_box"]["width"], pi["media_box"]["height"], pi["rotation"]
            );
        }

        if sig_count > 0 {
            println!("\nSignatures: {sig_count}");
            for sig in &sigs {
                println!("  - {}", sig.field_name);
            }
        }

        if let Some(d) = &docmdp {
            println!("DocMDP: {:?}", d.permission);
        }

        if let Some(d) = &dss {
            println!(
                "DSS: {} certs, {} OCSPs, {} CRLs, {} VRI entries",
                d.certificates.len(),
                d.ocsp_responses.len(),
                d.crls.len(),
                d.vri_entries.len()
            );
        }

        if !bookmarks.is_empty() {
            println!("\nBookmarks: {}", bookmarks.len());
            for bm in &bookmarks {
                println!("  - {}", bm.title);
            }
        }
    }

    Ok(())
}
