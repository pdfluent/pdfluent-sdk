# Fase O: Data Exchange + EU Compliance — Design Decisions

## Overview

Fase O adds form data exchange (FDF/XFDF) and ZUGFeRD/Factur-X e-invoicing
to the XFA engine. The implementation lives in the `pdf-invoice` crate.

**Crate:** `crates/pdf-invoice` (new)
**LOC:** ~2.5K
**Tests:** 24

## Modules

| Module | Purpose |
|--------|---------|
| `fdf` | FDF (Forms Data Format) binary import/export |
| `xfdf` | XFDF (XML Forms Data Format) import/export |
| `xml_form` | AcroForm XML export/import, XDP generation |
| `zugferd` | ZUGFeRD 2.3 / Factur-X CII XML generation + parsing |
| `embed` | PDF/A-3 embedding of XML as Associated Files |

## Key Decisions

### D1: FDF serialization without lopdf writer

**Decision:** Implemented a custom `write_object()` serializer for FDF output.

**Rationale:** lopdf 0.39 does not expose `Object::write_to()` publicly. Rather
than depending on lopdf internals or forking, a minimal serializer (~40 LOC)
handles the subset of PDF object types used in FDF (dicts, arrays, strings,
names, integers, references). This keeps the dependency clean.

### D2: FDF parsing via header patching

**Decision:** Replace `%FDF-1.2` header with `%PDF-1.4` to reuse lopdf's parser.

**Rationale:** FDF uses identical object/xref/trailer syntax as PDF. Patching
the 8-byte header is simpler and more maintainable than writing a separate
parser. lopdf handles the cross-reference table, object resolution, and
stream decompression — all of which FDF shares with PDF.

### D3: f64 for monetary amounts (no rust_decimal)

**Decision:** Use `f64` for all monetary amounts instead of a fixed-point
decimal type.

**Rationale:** ZUGFeRD CII XML uses string formatting with 2-4 decimal places.
f64 provides sufficient precision for invoice amounts (up to ~10^15 with
2-decimal accuracy). Adding `rust_decimal` would increase the dependency
footprint for minimal benefit in this use case. The `format_amount()` helper
ensures consistent 2+ decimal output.

### D4: Inline code lists (no zugferd-code-lists crate)

**Decision:** Tax category codes (UNTDID 5305) are implemented as a
`TaxCategory` enum directly in the crate.

**Rationale:** The `zugferd-code-lists` crate on crates.io is early-stage and
may not be actively maintained. The essential code lists (9 tax categories)
are small, stable (defined by ISO/UN standards), and unlikely to change.
Inlining them avoids an external dependency and gives full control over the
API surface.

### D5: quick-xml for CII generation, roxmltree for parsing

**Decision:** Use `quick-xml` (event-based writer) for generating CII XML,
and `roxmltree` (read-only DOM) for parsing.

**Rationale:** CII XML is deeply nested with multiple namespaces and attributes.
`quick-xml`'s `Writer` provides proper XML escaping, namespace handling, and
indentation. For parsing, `roxmltree` is already in the workspace and
provides convenient DOM traversal. This avoids pulling in a full read-write
XML library.

### D6: XFDF hand-written XML (no quick-xml dependency)

**Decision:** XFDF XML output is generated via string concatenation with
manual escaping.

**Rationale:** XFDF has a simple, flat structure (fields/field/value) that
doesn't benefit from quick-xml's complexity. A simple `xml_escape()` helper
handles the 5 XML special characters. This keeps the XFDF module self-contained.

### D7: Reuse of fdf module helpers in xfdf

**Decision:** XFDF import delegates to `fdf::find_acroform_id()` and
`fdf::set_field_value_by_name()` for PDF field manipulation.

**Rationale:** Both FDF and XFDF need to traverse the AcroForm field tree in
a lopdf `Document`. Rather than duplicating the tree-walking code, XFDF
reuses the FDF module's public(crate) helpers. Export also delegates: XFDF
export calls `FdfDocument::export_from()` and converts the result.

### D8: ZUGFeRD profile validation at serialization time

**Decision:** `ZugferdInvoice::to_xml()` calls `validate()` and returns an
error if validation fails, rather than silently generating invalid XML.

**Rationale:** Generating CII XML that doesn't match the declared profile
(e.g., EN16931 without line items) would create documents that fail
validation by receivers. Fail-fast at generation time prevents silent
non-compliance.

### D9: PDF/A-3 embedding uses FlateDecode compression

**Decision:** Embedded XML attachments are compressed with zlib (FlateDecode).

**Rationale:** PDF/A-3 allows FlateDecode for embedded files. Compression
reduces file size (typically 60-80% for XML) without requiring additional
dependencies beyond `flate2` which is already used by lopdf.

## Dependencies Added

| Crate | Version | Purpose |
|-------|---------|---------|
| `quick-xml` | 0.37 | CII XML generation (ZUGFeRD) |
| `chrono` | 0.4 | Date formatting (ISO 8601 / format 102) |
| `flate2` | 1 | Zlib compression for PDF/A-3 embedding |
| `roxmltree` | (workspace) | XML parsing (XFDF, CII) |
| `lopdf` | (workspace) | PDF object model, document I/O |

## ZUGFeRD Profiles Supported

| Profile | Line Items | Description |
|---------|-----------|-------------|
| Minimum | No | Invoice reference only |
| BasicWL | No | Structured header data |
| Basic | Yes | Header + line items |
| EN16931 | Yes | EU standard (most common) |
| Extended | Yes | Full CII feature set |

## Test Coverage

- FDF roundtrip (binary serialize + parse)
- FDF hierarchical field flattening
- XFDF roundtrip (XML serialize + parse)
- XFDF special character handling
- AcroForm XML roundtrip
- XDP generation with/without datasets
- ZUGFeRD EN16931 full XML roundtrip
- ZUGFeRD Minimum profile
- ZUGFeRD credit note (type code 381)
- Profile validation (missing required fields)
- Tax category code roundtrip
- PDF/A-3 XML embedding + extraction
- Multiple file embedding
- XMP metadata generation
