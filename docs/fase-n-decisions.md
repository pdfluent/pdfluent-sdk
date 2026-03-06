# Fase N -- Content Intelligence: Design Decisions

## Overview

Fase N adds content intelligence capabilities to the XFA engine: text/image extraction,
full-text search, GDPR-compliant redaction, and OCR for scanned PDFs. The implementation
spans three new crates that follow the established architectural patterns from Fase B and
the pdf-manip crate.

## Crate Structure

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `pdf-extract` | Image extraction, text extraction with positions, full-text search | lopdf, thiserror, flate2 |
| `pdf-redact` | Two-phase GDPR-compliant redaction with metadata cleaning | lopdf, thiserror, flate2 |
| `pdf-ocr` | Pluggable OCR engine trait and searchable PDF pipeline | lopdf, thiserror |

## Key Decisions

### D1: Trait-based OCR engine (`OcrEngine` trait)

**Decision:** Define `OcrEngine` as a trait with `recognize()` and `supported_languages()`,
rather than coupling to a specific OCR backend (Tesseract, Apple Vision, etc.).

**Rationale:** Different platforms have different OCR options. macOS has Vision framework,
Linux typically uses Tesseract, and cloud deployments might use Google Cloud Vision or
AWS Textract. The trait-based approach allows consumers to plug in whatever backend
is available. A `NoOpEngine` is provided for testing.

### D2: Render callback instead of hard dependency

**Decision:** `make_searchable()` takes a generic `render_fn` callback instead of
depending on pdfium-render or pdf-render directly.

**Rationale:** The OCR pipeline needs rasterized page images, but the rendering
subsystem is heavy and platform-specific. By using a callback `Fn(&Document, u32, u32) -> Result<(Vec<u8>, u32, u32), String>`,
we keep pdf-ocr lightweight and let the caller decide how to render pages.

### D3: No tantivy / no full-text index

**Decision:** Search is implemented with simple string matching on extracted text,
not with a full-text search engine like tantivy.

**Rationale:** For document-level search (typically 1-1000 pages), a full-text index
adds significant complexity and dependency weight without meaningful performance benefit.
The simple approach handles case-insensitive search, page filtering, and result limiting.
A tantivy-backed search can be added later if corpus-level search is needed.

### D4: Combined pdf-extract crate

**Decision:** Text extraction, image extraction, and search are in a single `pdf-extract`
crate rather than three separate crates.

**Rationale:** These features are closely related -- search depends on text extraction,
and users typically need both. A single crate reduces workspace complexity and makes
the API more discoverable. Internal modules keep the code organized.

### D5: Approximate character widths

**Decision:** Use a constant `APPROX_CHAR_WIDTH = 0.5` (as fraction of font size)
for character positioning instead of loading font metrics.

**Rationale:** Accurate glyph widths require loading and parsing the embedded font
program, which is complex (CIDFont, Type1, TrueType all differ). For search and
redaction purposes, approximate positions are sufficient. The positioned chars
give correct relative ordering, which is what search needs. Exact positioning
can be added later by integrating with pdf-font.

### D6: Metadata removal strategy

**Decision:** Redaction includes automatic removal of the Info dictionary, XMP metadata
stream, and page thumbnails.

**Rationale:** GDPR-compliant redaction must ensure that removed content cannot be
recovered. Document metadata (author, title, creation date) and thumbnails may contain
or reveal redacted information. Removing them is a standard practice in redaction tools
(cf. Adobe Acrobat's redaction feature).

## Test Coverage

| Crate | Module | Tests |
|-------|--------|-------|
| pdf-extract | images | 6 |
| pdf-extract | text | 7 |
| pdf-extract | search | 11 |
| pdf-redact | redact | 8 |
| pdf-ocr | engine | 2 |
| pdf-ocr | pipeline | 7 |
| **Total** | | **41** |

## Dependencies Added

- `flate2 = "1"` -- Flate compression/decompression for image extraction and content stream rewriting (already used by pdf-manip)
- No new external dependencies beyond what the workspace already provides
