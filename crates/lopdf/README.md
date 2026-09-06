# pdfluent-lopdf

> **This crate is a fork.** It began as [`lopdf`](https://github.com/J-F-Liu/lopdf) at version 0.44.0 and is
> maintained separately here. Upstream is actively developed and is the better
> choice if you do not need the changes made for PDFluent. Licensed as upstream
> (MIT); see `NOTICE` and `THIRD_PARTY_LICENSES.txt` for the full
> attribution.

Low-level PDF document object model and writer in pure Rust.

`pdfluent-lopdf` is the parser, object-model, and serializer that the
[PDFluent](https://pdfluent.com) commercial Rust PDF SDK uses for read/write
of the underlying PDF byte structure. It is published as **MIT** so it can
also be used standalone like any open-source library.

This crate is a **fork of [`lopdf`](https://crates.io/crates/lopdf)** by
Junfeng Liu, Emulator, and contributors. The fork is maintained by
PDFluent and the additions are contributed back to upstream where possible.

## What this crate gives you

- Parsing of PDF object streams, cross-reference tables (classical + xref-stream + hybrid), and incremental updates
- Typed access to PDF dictionaries, arrays, names, strings, streams, and indirect references
- Serialization back to a valid PDF (including incremental update writing)
- AES-256 encryption (V=5, R=6, /CFM /AESV3) — required by PDF 2.0 — added in this fork
- Optional `image`, `chrono`, `jiff`, `serde`, `tokio` features (see `Cargo.toml`)

What this crate is **not**:
- Not a renderer. See `pdf-render` (commercial) for rasterization.
- Not a high-level facade. See [`pdfluent`](https://crates.io/crates/pdfluent) for the SDK API.
- Not a content-stream interpreter. See `pdf-interpret` (open-source).

## Install

```toml
[dependencies]
pdfluent-lopdf = "0.39"
```

## Minimal example

```rust
use pdfluent_lopdf::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::load("input.pdf")?;
    println!("PDF version: {}", doc.version);
    println!("Pages: {}", doc.get_pages().len());
    Ok(())
}
```

For higher-level operations (rendering, signing, redaction, PDF/A,
extraction, forms) use the [`pdfluent`](https://crates.io/crates/pdfluent)
facade — it consumes this crate as the underlying object model.

## License

**MIT** — preserved from the upstream `lopdf` project.

```
Copyright (c) Junfeng Liu, Emulator, and lopdf contributors
Copyright (c) 2026 Innovation Trigger BV (PDFluent fork)
```

See [LICENSE](LICENSE).

## Why a fork

PDFluent depends on a small set of features that are either ahead of upstream
or pin-controlled for the commercial release cycle:

- **AES-256** — PDF 2.0 mandates this for new documents; not yet in upstream
- **Performance pinning** — coordinated exact-version pins across the
  PDFluent workspace prevent transitive churn for the commercial release
- **PDF/A round-trip** — small adjustments to write determinism to support
  `pdf-compliance` validation

We aim to upstream non-PDFluent-specific improvements. This crate stays MIT
to honor that path.

## Links

- PDFluent SDK: <https://pdfluent.com>
- `cargo add pdfluent` (full SDK): <https://crates.io/crates/pdfluent>
- Upstream `lopdf`: <https://github.com/J-F-Liu/lopdf>
