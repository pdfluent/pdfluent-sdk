# PDFluent — Observability (1.0)

**Status:** Active for 1.0 GA. Referenced from
[`STABILITY.md`](STABILITY.md) §7.1.
**Last updated:** 2026-04-22 (Epic 4 / #1235).

---

## 1. What this is

`pdfluent` emits structured [`tracing`](https://docs.rs/tracing)
spans for every public operation on `PdfDocument` and related facade
types. Spans are zero-cost when the `tracing` Cargo feature is off
(default), and standard `tracing` when on.

## 2. How to enable

```toml
[dependencies]
pdfluent = { version = "1", features = ["tracing"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

```rust
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> pdfluent::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // RUST_LOG=pdfluent=debug cargo run
    let mut doc = pdfluent::PdfDocument::open("in.pdf")?;
    doc.compress(pdfluent::CompressOptions::strict())?;
    doc.save("out.pdf")?;
    Ok(())
}
```

## 3. Span contract

Every instrumented method follows these rules:

| Rule | Rationale |
|---|---|
| Span name is the method name, snake-case. | `cargo doc` ↔ `trace` one-to-one mapping. |
| `self` is skipped via `skip(self)`. | PDFs are large; never print them. |
| Fields carry non-secret, low-cardinality inputs. | Paths, page counts, tiers — yes. Passwords, license keys, raw bytes — no. |
| The target is `pdfluent`. | `RUST_LOG=pdfluent=debug` is the canonical toggle. |
| Level is `INFO` for operations, `DEBUG` for internals. | |
| Error results are recorded via `tracing::error!` inside the operation. | |

## 4. Instrumented methods (1.0 baseline)

Operations that emit an `INFO`-level span:

| Method | Fields |
|---|---|
| `PdfDocument::open_with` | `path` |
| `PdfDocument::from_bytes_with` | `len` (bytes) |
| `PdfDocument::from_reader` | — |
| `PdfDocument::text` | — |
| `PdfDocument::text_with_layout` | — |
| `PdfDocument::form_fields` | — |
| `PdfDocument::form_mut` | — |
| `PdfDocument::flatten_forms` | — |
| `PdfDocument::add_decoration` | `kind` |
| `PdfDocument::rotate_page` | `page`, `rotation` |
| `PdfDocument::encrypt` | `algo` |
| `PdfDocument::decrypt` | — |
| `PdfDocument::sign` | `profile` |
| `PdfDocument::signatures` | — |
| `PdfDocument::verify_signatures` | — |
| `PdfDocument::redact` | `text_len` |
| `PdfDocument::redact_region` | `page` |
| `PdfDocument::split_pages` | — |
| `PdfDocument::extract_pages` | — |
| `PdfDocument::compress` | — |
| `PdfDocument::subset_fonts` | — |
| `PdfDocument::linearize` | — |
| `PdfDocument::embed_font` | — |
| `PdfDocument::to_docx` | `path` |
| `PdfDocument::to_images` | `pages_from`, `pages_to` |
| `PdfDocument::insert_image` | `page` |
| `PdfDocument::save_with` | `path`, `overwrite` |
| `PdfDocument::to_bytes` | — |
| `PdfDocument::write_to` | — |

## 5. What's NOT instrumented (by policy)

| Not instrumented | Reason |
|---|---|
| `OpenOptions` / `SaveOptions` builder methods | Trivial field-setters; noise. |
| `PdfVersion::parse`, enum `code()` / `docs_url()` | Pure functions. |
| `Page` / `Pages` accessors | Would fire per-iteration inside `for` loops; too noisy. |
| `PdfFormMut::set_*` setters | Usually called in fast chains; instrument the enclosing `form_mut` span instead. |
| License provisioning (`set_license_key`, env read) | Touches secrets; span would need extensive redaction. |

Adding instrumentation to any of the above is **not** a breaking
change and can land in any MINOR (§7.1 policy below).

## 6. Cargo feature

- `tracing` (off by default). Enables the `dep:tracing` dependency
  and makes every `#[cfg_attr(feature = "tracing",
  tracing::instrument(..))]` attribute take effect.
- Compile-time cost when off: zero. The `cfg_attr` resolves to
  no attribute.
- Compile-time cost when on: normal `tracing` macros.
- Runtime cost when on but no subscriber installed: essentially
  zero (tracing's null-dispatcher).

## 7. Evolution policy

Per STABILITY.md §2.2, the instrumented-method set is **MINOR-expandable**:

- Adding a span to a method is a MINOR-compatible change.
- Removing a span is **MAJOR** (users may have dashboards against it).
- Renaming a span is **MAJOR**. Add a new span + emit both during
  one MINOR to give dashboards a migration window.
- Field additions are MINOR. Field removals are MAJOR.

The baseline in §4 is pinned at 1.0 GA. Future spans are tracked
per-issue and listed in CHANGELOG.md on each release.

## 8. Runbook

```bash
# Everything (verbose):
RUST_LOG=pdfluent=debug cargo run

# Only operations (omits internal DEBUG):
RUST_LOG=pdfluent=info cargo run

# Only errors from pdfluent:
RUST_LOG=pdfluent=error cargo run

# Filter to a single operation:
RUST_LOG=pdfluent[compress]=info cargo run
```

When integrating with an OTLP exporter (Jaeger, Honeycomb, Datadog)
the spans nest correctly under caller spans because we use the
standard `tracing` dispatcher.
