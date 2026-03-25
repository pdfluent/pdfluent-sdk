# Cookbook

Practical, self-contained examples. Each snippet is complete enough to run with the
stated dependencies — copy, adjust paths, compile.

---

## 1. Open a PDF and extract text

```rust
use pdf_engine::PdfDocument;

fn main() -> pdf_engine::Result<()> {
    let doc = PdfDocument::open(std::fs::read("report.pdf")?)?;
    println!("{} pages", doc.page_count());

    for i in 0..doc.page_count() {
        let text = doc.extract_text(i)?;
        println!("--- page {} ---\n{text}", i + 1);
    }

    // Keyword search — returns 0-based page indices
    let pages = doc.search_text("total amount");
    println!("'total amount' found on pages: {pages:?}");

    Ok(())
}
```

**Dependencies:** `pdf-engine`

---

## 2. Check PDF/A compliance

```rust
use pdf_syntax::Pdf;
use pdf_compliance::{validate_pdfa, detect_pdfa_level, PdfALevel};

fn main() {
    let data = std::fs::read("document.pdf").unwrap();
    let pdf = Pdf::new(data).unwrap();

    let level = detect_pdfa_level(&pdf).unwrap_or(PdfALevel::A2b);
    let report = validate_pdfa(&pdf, level);

    if report.compliant {
        println!("PDF/A-{}{} ✓", level.part(), level.conformance());
    } else {
        println!("{} errors, {} warnings",
            report.error_count(), report.warning_count());
        for issue in &report.issues {
            println!("  [{}] {}", issue.rule, issue.message);
        }
    }
}
```

**Dependencies:** `pdf-syntax`, `pdf-compliance`

---

## 3. Convert a PDF to PDF/A-2b

```rust
use pdf_manip::pdfa_cleanup::cleanup_for_pdfa;
use pdf_manip::pdfa_xmp::repair_xmp;
use pdf_compliance::{validate_pdfa, PdfALevel};

fn main() -> pdf_manip::Result<()> {
    let data = std::fs::read("input.pdf")?;
    let mut doc = lopdf::Document::load_mem(&data)?;

    // Strip non-conforming constructs (JavaScript, transparency where forbidden, etc.)
    let report = cleanup_for_pdfa(&mut doc, false)?;
    println!("Cleaned: {report:?}");

    // Write/repair XMP metadata with correct PDF/A conformance identifier
    repair_xmp(&mut doc, PdfALevel::A2b)?;

    let output = doc.save_to_bytes()?;
    std::fs::write("output-pdfa.pdf", &output)?;

    // Verify the result
    let pdf = pdf_syntax::Pdf::new(output)?;
    let check = validate_pdfa(&pdf, PdfALevel::A2b);
    println!("Compliant: {}", check.compliant);

    Ok(())
}
```

**Dependencies:** `pdf-manip`, `pdf-compliance`, `pdf-syntax`, `lopdf`

---

## 4. Digitally sign a PDF

```rust
use pdf_sign::{Pkcs12Signer, sign_pdf, SignOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load PKCS#12 identity (.p12 / .pfx)
    let p12 = std::fs::read("identity.p12")?;
    let signer = Pkcs12Signer::from_pkcs12(&p12, "password")?;

    let pdf_bytes = std::fs::read("unsigned.pdf")?;

    let opts = SignOptions {
        reason: Some("Document approved".into()),
        location: Some("Amsterdam".into()),
        contact: Some("legal@example.com".into()),
        // Place a visible signature box on page 1
        visible_rect: Some((1, [400.0, 50.0, 580.0, 100.0])),
        ..Default::default()
    };

    let signed = sign_pdf(&signer, &pdf_bytes, opts)?;
    std::fs::write("signed.pdf", signed)?;
    println!("Signed PDF written.");

    Ok(())
}
```

**Dependencies:** `pdf-sign`

---

## 5. Search text and redact matches

```rust
use pdf_redact::{search_and_redact, RedactSearchOptions};

fn main() -> pdf_redact::Result<()> {
    let data = std::fs::read("contract.pdf")?;
    let mut doc = lopdf::Document::load_mem(&data)?;

    // Exact match (case-sensitive)
    let opts = RedactSearchOptions::exact("Jane Doe");
    let report = search_and_redact(&mut doc, &opts)?;
    println!("Redacted {} occurrence(s)", report.total_redacted);

    // Regex — redact all e-mail addresses
    let email_opts = RedactSearchOptions::with_regex()
        .overlay_text("[REDACTED]");
    search_and_redact(&mut doc, &email_opts)?;

    doc.save_to("redacted.pdf")?;
    Ok(())
}
```

**Dependencies:** `pdf-redact`, `lopdf`

---

## 6. Merge and split pages

```rust
use pdf_manip::pages::{merge, split_by_ranges};

fn main() -> pdf_manip::Result<()> {
    // Merge multiple files into one
    let merged = merge(&["part1.pdf", "part2.pdf", "part3.pdf"])?;
    merged.save_to("merged.pdf")?;
    println!("Merged: {} pages", merged.get_pages().len());

    // Split a document into chapters (1-based page numbers)
    let data = std::fs::read("book.pdf")?;
    let doc = lopdf::Document::load_mem(&data)?;
    let chapters = split_by_ranges(&doc, &[(1, 50), (51, 120), (121, 200)])?;
    for (i, chapter) in chapters.iter().enumerate() {
        chapter.save_to(format!("chapter_{}.pdf", i + 1))?;
    }

    Ok(())
}
```

**Dependencies:** `pdf-manip`, `lopdf`

---

## 7. Read and write form fields

```rust
use std::sync::Arc;
use pdf_syntax::Pdf;
use pdf_forms::{parse_acroform, FieldValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = Arc::new(std::fs::read("application.pdf")?);
    let pdf = Pdf::new(data.clone())?;

    let tree = parse_acroform(&pdf).expect("no AcroForm in this PDF");

    // Read all terminal fields
    for id in tree.terminal_fields() {
        let name = tree.fully_qualified_name(id);
        let value = tree.effective_value(id);
        let flags = tree.get(id).flags;
        println!("{name} = {value:?}  (read_only={})", flags.is_read_only());
    }

    // Find a specific field and inspect its properties
    if let Some(id) = tree.find_by_name("Applicant.FullName") {
        let node = tree.get(id);
        println!("max_len = {:?}", node.max_len);
    }

    Ok(())
}
```

**Dependencies:** `pdf-syntax`, `pdf-forms`

---

## 8. Flatten a form (AcroForm or XFA)

```rust
use std::sync::Arc;
use pdf_syntax::Pdf;
use pdf_forms::{parse_acroform, flatten::{flatten_form, FlattenConfig}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read("form.pdf")?;

    // Parse the field tree from pdf-syntax
    let pdf = Pdf::new(Arc::new(data.clone()))?;
    let tree = parse_acroform(&pdf).expect("no AcroForm");

    // Flatten in lopdf (supports mutation)
    let mut doc = lopdf::Document::load_mem(&data)?;
    let config = FlattenConfig {
        remove_acroform: true,
        pdfa: false,
        ..Default::default()
    };
    let result = flatten_form(&mut doc, &tree, &config);
    println!("Flattened {} field(s)", result.fields_flattened);
    if !result.skipped.is_empty() {
        println!("Skipped: {:?}", result.skipped);
    }

    doc.save_to("flattened.pdf")?;
    Ok(())
}
```

**Dependencies:** `pdf-syntax`, `pdf-forms`, `lopdf`

---

## 9. Convert PDF to DOCX

```rust
use pdf_docx::convert_pdf_bytes_to_docx;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf_bytes = std::fs::read("report.pdf")?;
    let docx_bytes = convert_pdf_bytes_to_docx(&pdf_bytes)?;
    std::fs::write("report.docx", docx_bytes)?;
    println!("Converted to DOCX.");
    Ok(())
}
```

**Dependencies:** `pdf-docx`

---

## 10. Embed a ZUGFeRD invoice

Creates a hybrid PDF/A-3 + CII XML e-invoice as required by EU e-invoicing regulations
(EN 16931 / Directive 2014/55/EU).

```rust
use chrono::NaiveDate;
use pdf_invoice::zugferd::{ZugferdInvoice, ZugferdProfile, TradeParty, Address,
                            LineItem, TaxCategory, PaymentTerms};
use pdf_invoice::embed::{embed_xml_attachment, add_zugferd_xmp, AfRelationship};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build the invoice data model
    let invoice = ZugferdInvoice {
        profile: ZugferdProfile::EN16931,
        invoice_number: "INV-2026-0042".into(),
        type_code: "380".into(), // 380 = commercial invoice
        issue_date: NaiveDate::from_ymd_opt(2026, 3, 21).unwrap(),
        seller: TradeParty {
            name: "XFA Software BV".into(),
            vat_id: Some("NL123456789B01".into()),
            address: Address {
                line1: "Herengracht 1".into(),
                city: "Amsterdam".into(),
                postcode: "1017 BZ".into(),
                country: "NL".into(),
                ..Default::default()
            },
        },
        buyer: TradeParty {
            name: "Acme Corp".into(),
            vat_id: Some("DE987654321".into()),
            address: Address {
                line1: "Unter den Linden 1".into(),
                city: "Berlin".into(),
                postcode: "10117".into(),
                country: "DE".into(),
                ..Default::default()
            },
        },
        line_items: vec![
            LineItem {
                name: "XFA SDK Enterprise licence".into(),
                quantity: 1.0,
                unit_price: 2500.00,
                tax_category: TaxCategory::StandardRate(21.0),
                ..Default::default()
            },
        ],
        payment_terms: Some(PaymentTerms {
            description: "Due within 30 days".into(),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 20).unwrap()),
        }),
        ..Default::default()
    };

    // Validate before serialising
    let issues = invoice.validate();
    if !issues.is_empty() {
        eprintln!("Invoice validation issues: {issues:?}");
    }

    // Serialise to CII XML
    let xml = invoice.to_xml()?;

    // Embed in an existing PDF/A-3 document
    let data = std::fs::read("invoice-layout.pdf")?;
    let mut doc = lopdf::Document::load_mem(&data)?;

    embed_xml_attachment(
        &mut doc,
        "factur-x.xml",
        xml.as_bytes(),
        AfRelationship::Data,
    )?;
    add_zugferd_xmp(&mut doc, ZugferdProfile::EN16931)?;

    doc.save_to("invoice-zugferd.pdf")?;
    println!("ZUGFeRD invoice written.");

    Ok(())
}
```

**Dependencies:** `pdf-invoice`, `lopdf`, `chrono`

---

## 11. Incrementally save a PDF (preserve digital signatures)

Incremental saves append a new revision to the original bytes. Any byte-range
digital signatures that covered the original body remain cryptographically
valid because the signed bytes are never rewritten.

```rust
use lopdf::{Document, IncrementalDocument, Object};

fn main() -> lopdf::Result<()> {
    // Keep the raw bytes — they become the base of the incremental chain.
    let raw = std::fs::read("signed.pdf")?;
    let prev = Document::load_mem(&raw)?;

    let mut incr = IncrementalDocument::create_from(raw, prev);

    // To modify an existing object, clone it into the new revision first.
    // Here we update the document's modification date.
    if let Ok(info_ref) = incr.get_prev_documents().trailer.get(b"Info") {
        if let Ok(&info_id) = info_ref.as_reference() {
            incr.opt_clone_object_to_new_document(info_id)?;
            if let Ok(info) = incr
                .new_document
                .get_object_mut(info_id)
                .and_then(Object::as_dict_mut)
            {
                info.set("ModDate", Object::string_literal("D:20260325000000Z"));
            }
        }
    }

    // Output = original bytes + new revision appended at the end.
    incr.save("signed-updated.pdf")?;
    println!("Incremental revision written. Prior signatures are still valid.");

    Ok(())
}
```

**Dependencies:** `lopdf`

---

## 12. Batch-process a directory of PDFs in parallel

`process_batch` runs a closure on every PDF using a fixed-size worker pool.
`PdfBatch` provides one-liner helpers for the most common tasks.

```rust
use pdf_engine::{BatchConfig, BatchResult, ErrorStrategy, PdfBatch, process_batch};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    // --- Option A: high-level helper (walks a directory tree) ---
    let compliance = PdfBatch::validate_compliance(
        "./archive",
        BatchConfig {
            workers: 8,
            timeout: Duration::from_secs(60),
            on_error: ErrorStrategy::Collect,
            ..Default::default()
        },
    );
    println!(
        "Checked {} files in {:.1}s — {} passed, {} failed",
        compliance.successes.len() + compliance.failures.len(),
        compliance.duration.as_secs_f64(),
        compliance.successes.len(),
        compliance.failures.len(),
    );

    // --- Option B: custom processor on an explicit file list ---
    let paths: Vec<PathBuf> = (1..=50)
        .map(|i| PathBuf::from(format!("pages/page-{i:03}.pdf")))
        .collect();

    let result: BatchResult<usize> = process_batch(
        &paths,
        BatchConfig {
            workers: 4,
            timeout: Duration::from_secs(30),
            on_error: ErrorStrategy::Collect,
            on_progress: Some(Box::new(|done, total| eprint!("\r{done}/{total}"))),
            ..Default::default()
        },
        |doc| Ok(doc.page_count()),
    );

    eprintln!();
    for (path, n) in &result.successes {
        println!("{}: {n} page(s)", path.display());
    }
    for (path, err) in &result.failures {
        eprintln!("FAIL {}: {err}", path.display());
    }
}
```

**Dependencies:** `pdf-engine`

---

## 13. Linearize a PDF for fast web viewing

A linearized PDF is structured so that browsers can render page 1 while the
rest of the file is still downloading ("fast web view" / "optimized" flag in
Acrobat). Use this before uploading large PDFs to a CDN or web server.

```rust
use lopdf::Document;
use std::fs::File;
use std::io::BufWriter;

fn main() -> lopdf::Result<()> {
    let data = std::fs::read("report.pdf")?;
    let doc = Document::load_mem(&data)?;

    let mut out = BufWriter::new(File::create("report-linear.pdf")?);
    doc.save_linearized(&mut out)?;

    println!("Linearized PDF written.");
    Ok(())
}
```

Alternatively, use `SaveOptions` when combining linearization with other save
settings:

```rust
use lopdf::{Document, SaveOptions};
use std::fs::File;
use std::io::BufWriter;

fn main() -> lopdf::Result<()> {
    let data = std::fs::read("report.pdf")?;
    let mut doc = Document::load_mem(&data)?;

    let options = SaveOptions::builder().linearize(true).build();
    let mut out = BufWriter::new(File::create("report-linear.pdf")?);
    doc.save_with_options(&mut out, options)?;

    Ok(())
}
```

**Dependencies:** `lopdf`

---

## 14. Repair PDF/UA accessibility (remediation)

`remediate_pdfua` applies a conservative, best-effort fix pass for common
PDF/UA-1 (ISO 14289-1) failures: sets `MarkInfo/Marked`, adds catalog `/Lang`,
fixes page tab order, tags untagged text runs as `<P>` elements, and adds
`/Alt` text to untagged figures. Manual review is still recommended for complex
documents.

```rust
use pdf_manip::pdfua::remediate_pdfua;
use pdf_compliance::validate_pdfua;
use pdf_syntax::Pdf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read("document.pdf")?;

    // Optional: inspect compliance before making any changes.
    let before_report = validate_pdfua(&Pdf::new(data.clone())?);
    println!("Before: {} error(s)", before_report.error_count());

    // Apply remediation pass.
    let mut doc = lopdf::Document::load_mem(&data)?;
    let report = remediate_pdfua(&mut doc)?;
    println!(
        "Fixed {}/{} issue(s)",
        report.issues_fixed, report.issues_found
    );
    for msg in &report.issues_unfixable {
        println!("  unfixable: {msg}");
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    std::fs::write("document-ua.pdf", &out)?;

    // Verify the result.
    let after_report = validate_pdfua(&Pdf::new(out)?);
    println!("After: {} error(s)", after_report.error_count());

    Ok(())
}
```

**Dependencies:** `pdf-manip`, `pdf-compliance`, `pdf-syntax`, `lopdf`
