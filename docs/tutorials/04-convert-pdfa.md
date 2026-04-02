# Converting to PDF/A Standards

PDFluent provides full support for archiving standards like PDF/A-1, PDF/A-2, and PDF/A-3. This tutorial covers how to check for compliance and how to save a PDF in a specific format.

## PDF/A Compliance Check

You can verify if a document already complies with PDF/A standards.

```rust
use pdf_engine::api;

fn main() -> Result<(), api::Error> {
    let doc = api::read("document.pdf")?;

    if doc.is_pdfa_compliant()? {
        println!("The document is already PDF/A compliant.");
    } else {
        println!("The document is not PDF/A compliant.");
    }

    Ok(())
}
```

## Converting to PDF/A

To save a document as a specific PDF/A version, use the `save_with` method and specify the desired format in the options.

```rust
use pdf_engine::api::PdfFormat;

// Save as PDF/A-2b for long-term archiving
doc.save_with("archived_v2.pdf", |opts| {
    opts.format(PdfFormat::PdfA2b)
})?;

// Save as PDF/A-3b (often used for ZUGFeRD invoices)
doc.save_with("archived_v3.pdf", |opts| {
    opts.format(PdfFormat::PdfA3b)
})?;
```

## Linearization (Fast Web View)

PDFluent also supports linearization, which optimizes a PDF for web viewing by allowing it to be displayed page-by-page as it downloads.

```rust
doc.save_with("linearized.pdf", |opts| {
    opts.linearize(true)
})?;
```

## Summary

In this tutorial, you learned how to:
1. Verify PDF/A compliance.
2. Save a document as PDF/A-1b, PDF/A-2b, or PDF/A-3b.
3. Enable linearization for fast web viewing.
