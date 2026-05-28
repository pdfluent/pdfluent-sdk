# Quickstart Guide

> **Canonical quickstart:** for the supported public bindings (Rust, C ABI,
> WASM, Node, Python, .NET, Java) use **[quickstart-bindings.md](quickstart-bindings.md)**,
> backed by the CI-verified `pdfluent-examples/` and the frozen **`pdfluent`**
> facade. The snippets below use the lower-level internal `pdf_engine` crate
> for advanced/embedding scenarios; new integrations should prefer the
> `pdfluent` facade.

## Installation

Add the crates you need to your `Cargo.toml`:

```toml
[dependencies]
pdf-engine = { path = "crates/pdf-engine" }         # open, render, extract text
pdf-compliance = { path = "crates/pdf-compliance" }  # PDF/A validation
pdf-manip = { path = "crates/pdf-manip" }            # text replace, merge, split
lopdf = { path = "crates/lopdf" }                    # low-level PDF mutation
pdf-syntax = { path = "crates/pdf-syntax" }          # read-only PDF parsing
```

## Open a PDF and read page info

```rust
use pdf_engine::PdfDocument;

fn main() -> pdf_engine::Result<()> {
    let data = std::fs::read("invoice.pdf")?;
    let doc = PdfDocument::open(data)?;

    println!("Pages: {}", doc.page_count());

    let info = doc.info();
    if let Some(title) = &info.title {
        println!("Title: {title}");
    }

    for i in 0..doc.page_count() {
        let geo = doc.page_geometry(i)?;
        println!(
            "Page {}: {:.0} x {:.0} pt",
            i + 1,
            geo.media_box.width,
            geo.media_box.height
        );
    }

    Ok(())
}
```

## Extract text

```rust
use pdf_engine::PdfDocument;

fn main() -> pdf_engine::Result<()> {
    let doc = PdfDocument::open(std::fs::read("report.pdf")?)?;

    let text = doc.extract_all_text();
    println!("{text}");

    // Search across all pages — returns matching page indices
    let pages = doc.search_text("quarterly revenue");
    println!("Found on pages: {:?}", pages);

    Ok(())
}
```

## Validate PDF/A compliance

```rust
use pdf_syntax::Pdf;
use pdf_compliance::{validate_pdfa, detect_pdfa_level, PdfALevel};

fn main() {
    let data = std::fs::read("document.pdf").unwrap();
    let pdf = Pdf::new(data).unwrap();

    // Auto-detect the claimed PDF/A level from XMP metadata
    let level = detect_pdfa_level(&pdf).unwrap_or(PdfALevel::A2b);
    let report = validate_pdfa(&pdf, level);

    if report.compliant {
        println!("Compliant with PDF/A-{}{}", level.part(), level.conformance());
    } else {
        println!("{} errors, {} warnings", report.error_count(), report.warning_count());
        for issue in &report.issues {
            println!("  [{}] {}", issue.rule, issue.message);
        }
    }
}
```

## Replace text in a PDF

```rust
use pdf_manip::{FontMap, text_replace};

fn main() -> pdf_manip::Result<()> {
    let data = std::fs::read("template.pdf")?;
    let mut doc = lopdf::Document::load_mem(&data)?;

    // Replace on all pages at once
    let count = text_replace::replace_text_all_pages(
        &mut doc,
        "{{COMPANY}}",
        "Acme Corp",
    )?;
    println!("Replaced {count} occurrences");

    // Or target a specific page (1-based)
    let fonts = FontMap::from_page(&doc, 1)?;
    text_replace::replace_text(&mut doc, 1, "{{DATE}}", "2026-03-19", &fonts)?;

    doc.save_to("output.pdf")?;
    Ok(())
}
```

## Render a page to PNG

```rust
use pdf_engine::{PdfDocument, RenderOptions};

fn main() -> pdf_engine::Result<()> {
    let doc = PdfDocument::open(std::fs::read("brochure.pdf")?)?;

    let options = RenderOptions {
        dpi: 150.0,
        ..Default::default()
    };

    let rendered = doc.render_page(0, &options)?;
    println!("{}x{} pixels", rendered.width, rendered.height);

    // rendered.pixels contains RGBA data — encode to PNG with the `png` crate
    let file = std::fs::File::create("page1.png").unwrap();
    let mut encoder = png::Encoder::new(file, rendered.width, rendered.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rendered.pixels).unwrap();

    Ok(())
}
```

## Python and Node.js bindings

Pre-built bindings are available in `crates/pdf-python` and `crates/pdf-node`:

```bash
# Python (requires maturin)
cd crates/pdf-python && maturin develop --release

# Node.js (requires napi-rs)
cd crates/pdf-node && npm install && npm run build
```

See `crates/pdf-python/python/xfa_pdf/` and `crates/pdf-node/src/` for the binding APIs.

## Next steps

- [API Reference](api-reference.md) — full type and function documentation
- [Code Examples](examples.md) — advanced usage patterns
