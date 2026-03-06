# Fase K — Design Decisions

## K1: WASM Build + TypeScript Wrapper (#188)

### PdfDoc uses pdf-syntax directly (not pdf-engine)
`pdf-engine` depends on `rayon` (for parallel rendering) and `pdf-render` (vello_cpu),
which are incompatible with WASM. `PdfDoc` uses `pdf-syntax` + `pdf-sign` +
`pdf-compliance` directly — all pure Rust, WASM-safe.

### No rendering in WASM
Page rendering requires the full pdf-render stack (vello_cpu, image), which is too
heavy for WASM. PdfDoc focuses on analysis: metadata, signatures, compliance.

### TypeScript wrapper with parsed JSON
The WASM boundary uses JSON strings. The TypeScript wrapper (`ts/index.ts`) parses
these into typed interfaces (PdfMetadata, SignatureInfo, ComplianceReport, etc.)
so consumers get full type safety.

### bytes_to_pdf_string helper
PDF strings can be UTF-8, UTF-16 BE (with BOM), or Latin-1. A shared helper handles
all three encodings, matching the implementation in pdf-engine.

## K2: CLI Render, Extract, Fill, Flatten (#189)

### clap derive for subcommands
The CLI uses `clap` derive macros with a top-level `Commands` enum. Each subcommand
dispatches to a `cmd_*.rs` module. The original demo code moved to `cmd_demo.rs`.

### Render uses pdf-engine
`cmd_render` uses `PdfDocument::render_page()` + `image::RgbaImage` to save PNGs.
Resolution is configurable via `--dpi`.

### Fill uses lopdf directly
`cmd_fill` works with `lopdf::Document` to set AcroForm field values. It walks the
/Fields tree and /Parent chain to build fully-qualified names, then matches against
the JSON input. Appearance streams (/AP) are removed to force regeneration.

### Flatten identifies Widgets in two passes
To avoid borrow checker issues, `cmd_flatten` first collects all Widget annotation
object IDs, then removes matching refs from page /Annots arrays.

### Page selection format
All page-aware commands accept `--pages "1,3-5,8"` (1-based, comma-separated ranges).
Internally converted to 0-based indices.

## K3: CLI Info, Validate, Sign (#190)

### Info combines pdf-engine and pdf-syntax
`cmd_info` uses `PdfDocument` (pdf-engine) for metadata and bookmarks, plus
`pdf_syntax::Pdf` for signature and DSS queries via pdf-sign.

### Validate exits with code 1 on non-compliance
`cmd_validate` calls `std::process::exit(1)` when the document is not compliant,
enabling use in CI/CD pipelines.

### Profile name normalization
Profile names are case-insensitive and accept multiple formats:
`pdf-a2b`, `pdfa2b`, `a2b`, `PDF/A-2b` all resolve to `PdfALevel::A2b`.

### Sign shows validation detail
`cmd_sign` validates all signatures and shows status, signer, timestamp, SubFilter,
DocMDP permissions, and DSS/LTV information.
