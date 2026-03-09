# XFA-Native-Rust — Technical Architecture

> Complete technical reference for the XFA PDF SDK.
> Last updated: 2026-03-09

---

## Table of Contents

1. [Project Vision](#1-project-vision)
2. [System Architecture](#2-system-architecture)
3. [Crate Map (38 crates)](#3-crate-map)
4. [Layer 1: PDF Parsing (hayro fork)](#4-layer-1-pdf-parsing)
5. [Layer 2: XFA Engine](#5-layer-2-xfa-engine)
6. [Layer 3: Document Abstraction](#6-layer-3-document-abstraction)
7. [Layer 4: Forms & Annotations](#7-layer-4-forms--annotations)
8. [Layer 5: Manipulation & Content Intelligence](#8-layer-5-manipulation--content-intelligence)
9. [Layer 6: Compliance & Standards](#9-layer-6-compliance--standards)
10. [Layer 7: Digital Signatures](#10-layer-7-digital-signatures)
11. [Layer 8: Data Exchange & Conversions](#11-layer-8-data-exchange--conversions)
12. [Layer 9: Language Bindings](#12-layer-9-language-bindings)
13. [Layer 10: Desktop Application](#13-layer-10-desktop-application)
14. [Layer 11: CLI & Server](#14-layer-11-cli--server)
15. [Testing & Quality Infrastructure](#15-testing--quality-infrastructure)
16. [Dependency Graph](#16-dependency-graph)
17. [External Dependencies](#17-external-dependencies)
18. [Security Architecture](#18-security-architecture)
19. [Performance Architecture](#19-performance-architecture)
20. [Implementation Status](#20-implementation-status)

---

## 1. Project Vision

XFA-Native-Rust is a **100% pure Rust** PDF SDK delivering full Adobe Reader parity for XFA forms, plus a complete PDF manipulation and compliance stack. The architecture replaces all C/C++ dependencies (PDFium, Poppler, MuPDF) with memory-safe Rust implementations.

**Core principles:**
- **Memory safety** — no `unsafe` in application code; all parsing is bounds-checked
- **No C dependencies** — WASM-compilable, no FFI overhead, no segfaults
- **Layered architecture** — each crate has a single responsibility with clear interfaces
- **Standards-first** — ISO 32000-2 (PDF 2.0), XFA 3.3, PDF/A (ISO 19005), PDF/UA (ISO 14289), PDF/X (ISO 15930)

**Competitive positioning:** Targets feature parity with iText, PSPDFKit, Aspose.PDF and Adobe Acrobat Pro.

---

## 2. System Architecture

### High-Level Stack

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Applications                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐  ┌───────────┐ │
│  │ pdf-desktop │  │ xfa-cli      │  │ xfa-api-    │  │ xfa-wasm  │ │
│  │ (Tauri)     │  │              │  │ server      │  │ (browser) │ │
│  └──────┬──────┘  └──────┬───────┘  └──────┬──────┘  └─────┬─────┘ │
├─────────┼────────────────┼─────────────────┼────────────────┼───────┤
│         │         Language Bindings         │                │       │
│  ┌──────┴──────┐  ┌──────┴───────┐  ┌──────┴──────┐        │       │
│  │ pdf-capi    │  │ pdf-python   │  │ pdf-node    │        │       │
│  │ (C FFI)     │  │ (PyO3)       │  │ (napi-rs)   │        │       │
│  └──────┬──────┘  └──────┬───────┘  └──────┬──────┘        │       │
├─────────┼────────────────┼─────────────────┼────────────────┼───────┤
│         └────────────────┴────────┬────────┴────────────────┘       │
│                                   ▼                                  │
│                          ┌────────────────┐                          │
│                          │   pdf-engine   │   Unified Document API   │
│                          └───────┬────────┘                          │
├──────────────────────────────────┼───────────────────────────────────┤
│              ┌───────────────────┼───────────────────┐               │
│              ▼                   ▼                   ▼               │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────────┐     │
│  │  pdf-forms     │  │  pdf-annot     │  │  pdf-xfa           │     │
│  │  (AcroForms)   │  │  (Annotations) │  │  (XFA 3.3)         │     │
│  └────────────────┘  └────────────────┘  └────────┬───────────┘     │
│                                                    │                 │
│                                          ┌─────────┼─────────┐      │
│                                          ▼         ▼         ▼      │
│                                   ┌──────────┐ ┌────────┐ ┌──────┐  │
│                                   │xfa-dom-  │ │formcalc│ │xfa-  │  │
│                                   │resolver  │ │-interp │ │layout│  │
│                                   └──────────┘ └────────┘ └──────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  Manipulation / Intelligence / Compliance / Signatures              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────┐ ┌─────────┐ │
│  │pdf-manip │ │pdf-extract│ │pdf-redact│ │pdf-       │ │pdf-sign │ │
│  │          │ │          │ │          │ │compliance │ │         │ │
│  └──────────┘ └──────────┘ └──────────┘ └───────────┘ └─────────┘ │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────┐             │
│  │pdf-docx  │ │pdf-xlsx  │ │pdf-pptx  │ │pdf-invoice│             │
│  └──────────┘ └──────────┘ └──────────┘ └───────────┘             │
├─────────────────────────────────────────────────────────────────────┤
│  PDF I/O Foundation                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │ pdf-syntax   │  │ pdf-interpret│  │ lopdf        │              │
│  │ (read-only)  │  │ (content     │  │ (read-write  │              │
│  │              │  │  streams)    │  │  mutation)   │              │
│  └──────┬───────┘  └──────┬───────┘  └──────────────┘              │
│         │                 │                                          │
│  ┌──────┴───────┐  ┌──────┴───────┐  ┌──────────────┐              │
│  │ pdf-font     │  │ pdf-render   │  │ Image codecs │              │
│  │ (CMap, CFF,  │  │ (vello_cpu   │  │ (JBIG2,      │              │
│  │  Type1)      │  │  rasterizer) │  │  JPEG2000,   │              │
│  └──────────────┘  └──────────────┘  │  CCITT)      │              │
│                                       └──────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### Dual Parser Architecture

The SDK uses two PDF parsing stacks for different purposes:

| Stack | Crate | Purpose | Mutability |
|-------|-------|---------|------------|
| **hayro fork** | `pdf-syntax` | Read-only parsing, rendering, compliance checking | Immutable |
| **lopdf** | `lopdf` (v0.39) | PDF mutation, form filling, manipulation | Mutable |

This dual-stack design allows read-only operations (rendering, text extraction, compliance) to use the zero-copy `pdf-syntax` parser, while write operations (form fill, redact, sign) go through `lopdf` which supports full PDF serialization.

---

## 3. Crate Map

### Overview: 38 crates + 1 fuzzing harness

```
crates/
├── hayro-ccitt/           # CCITT Group 3/4 image decoder
├── hayro-jbig2/           # JBIG2 image decoder
├── hayro-jpeg2000/        # JPEG2000 image decoder
├── pdf-syntax/            # Read-only PDF parser (hayro fork)
├── pdf-interpret/         # Content stream interpreter
├── pdf-font/              # Font parsing (Type1, CFF, CMap)
├── pdf-render/            # Pure Rust rasterizer (vello_cpu)
├── pdf-engine/            # Unified document API
├── pdf-forms/             # AcroForm engine
├── pdf-annot/             # Annotation engine
├── pdf-xfa/               # XFA wrapper
├── xfa-dom-resolver/      # SOM path resolution, DOM management
├── formcalc-interpreter/  # FormCalc scripting engine
├── xfa-layout-engine/     # XFA layout engine
├── xfa-json/              # JSON output for XFA layout
├── pdf-manip/             # PDF manipulation (pages, encrypt, watermark, fonts)
├── pdf-extract/           # Content extraction (text, images)
├── pdf-redact/            # GDPR-compliant redaction
├── pdf-ocr/               # OCR integration (Tesseract + PaddleOCR, feature-gated)
├── pdf-compliance/        # PDF/A, PDF/UA, PDF/X validation
├── pdf-sign/              # Digital signature validation + signing (PAdES, CMS, TSA)
├── pdf-diff/              # Visual PDF comparison (SSIM)
├── pdf-docx/              # PDF → DOCX conversion
├── pdf-xlsx/              # PDF → XLSX conversion
├── pdf-pptx/              # PDF → PPTX conversion
├── pdf-invoice/           # FDF/XFDF + ZUGFeRD/Factur-X
├── pdf-capi/              # C FFI binding
├── pdf-python/            # Python binding (PyO3) — excluded from workspace
├── pdf-node/              # Node.js binding (napi-rs)
├── xfa-wasm/              # WebAssembly binding
├── pdf-desktop/           # Tauri v2 desktop viewer
├── xfa-cli/               # CLI tool
├── xfa-api-server/        # HTTP API server (Axum)
├── xfa-test-runner/       # Corpus test runner
├── xfa-golden-tests/      # Visual regression tests
├── pdf-bench/             # Performance benchmarks (Criterion)
├── xfa-license/           # License key validation
├── pdfium-ffi-bridge/     # PDFium FFI (optional, golden tests only)
fuzz/                      # Fuzzing harness (cargo-fuzz)
```

---

## 4. Layer 1: PDF Parsing

### 4.1 pdf-syntax (hayro fork)

**Purpose:** Zero-copy, read-only PDF parsing. Forked from the hayro ecosystem (MIT/Apache-2.0).

**Key types:**
- `Pdf` — Root document handle. Lazy parsing: objects resolved on access.
- `Page` — Page access with `raw() → &Dict`, `page_stream() → Option<&[u8]>`
- `Object` — Enum: `Dict`, `Array`, `Stream`, `String`, `Name`, `Number`, `Bool`, `Null`, `Ref`
- `Rect` — Bounding box with `x0, y0, x1, y1` (f64)

**Subsystems:**
- XRef parsing (standard + cross-reference streams)
- Stream decoding: FlateDecode, LZWDecode, ASCIIHexDecode, ASCII85Decode, RunLengthDecode, CCITTFaxDecode, JBIG2Decode, JPXDecode, DCTDecode, Crypt
- Encryption: RC4, AES-128, AES-256 (password-protected PDF detection)
- Content stream tokenization

**Security hardening (SafeDocs corpus):**
- Inline nesting depth cap: 256
- XRef chain depth cap: 64
- Page count cap: 100,000
- Flate/LZW/predictor output cap: 64 MB
- File size limit: 256 MB
- Thread stack size: 8 MB

### 4.2 pdf-interpret

**Purpose:** Content stream interpretation — converts PDF operators into rendering commands.

**Key abstractions:**
- `Device` trait — rendering backend interface (rasterization, text extraction)
- Graphics state machine — tracks CTM, color space, clipping, text state
- Glyph rendering via `skrifa` (modern font parsing) + `kurbo` (2D geometry)

### 4.3 pdf-font

**Purpose:** Font parsing, CMap handling, glyph metrics.

**Key types:**
- `CMap` — CMap parser with `lookup_bf_string()` and `lookup_cid_code()`
- `BfString` — Either `Char(char)` or `String(String)` for Unicode mapping
- Font types: Type1, TrueType, CFF, OpenType
- Perfect hashing (`phf`) for built-in CMap tables

### 4.4 pdf-render

**Purpose:** Pure Rust rasterizer using `vello_cpu` (vector graphics backend).

**Pipeline:**
```
Content stream → pdf-interpret (Device) → vello_cpu scene → bitmap
```

### 4.5 Image Codecs

| Crate | Format | Notes |
|-------|--------|-------|
| `hayro-ccitt` | CCITT Group 3/4 | No dependencies, pure codec |
| `hayro-jbig2` | JBIG2 | Memory-safe decoder |
| `hayro-jpeg2000` | JPEG2000 | Optional SIMD support |

---

## 5. Layer 2: XFA Engine

The XFA engine implements the complete XFA 3.3 specification for XML Forms Architecture processing.

### 5.1 xfa-dom-resolver

**Spec:** XFA 3.3 §3 (Object Models)

**DOM hierarchy:**
```
xfa (root)
  ├── config          → Configuration DOM
  ├── datasets
  │     ├── data            → Data DOM (dataGroup / dataValue)
  │     └── dataDescription → Data Description DOM
  ├── form             → Form DOM (merge result)
  ├── layout           → Layout DOM
  └── template         → Template DOM
```

**Key capabilities:**
- SOM (Scripting Object Model) path resolution: `xfa.form.subform[3].field[*]`
- Wildcard and predicate support in SOM expressions
- Arena-allocated node trees for performance
- CRUD operations on Template and Data DOM nodes

### 5.2 formcalc-interpreter

**Spec:** XFA 3.3 §25 (FormCalc Specification)

**Pipeline:** Source → Lexer → Parser (recursive descent) → AST → Interpreter

**Built-in functions (90+):**

| Category | Functions |
|----------|-----------|
| Arithmetic | `Abs`, `Avg`, `Ceil`, `Count`, `Floor`, `Max`, `Min`, `Mod`, `Round`, `Sum` |
| Date/Time | `Date`, `Date2Num`, `Num2Date`, `Time`, `Time2Num` + 7 others |
| String | `At`, `Concat`, `Left`, `Len`, `Replace`, `Substr` + 13 others |
| Financial | `Apr`, `Pmt`, `Pv`, `Rate`, `Term` + 5 others |
| Logical | `If`, `Choose`, `Oneof`, `Within`, `Eval`, `Null` |

**SOM integration:** FormCalc scripts resolve and mutate DOM nodes via the SOM bridge.

### 5.3 xfa-layout-engine

**Spec:** XFA 3.3 §4 (Box Model), §8 (Layout for Growable Objects)

**Layout modes:**
- **Positioned** — absolute coordinates within containers
- **Flowed** — flow-based: `tb`, `lr-tb`, `rl-tb`
- **Tables** — rows, cells, column spanning

**Features:**
- Box Model (margins, borders, padding, content areas)
- Dynamic sizing (`minH`, `maxH`, `minW`, `maxW` constraints)
- Occur rules (repeating subforms based on `min`/`max`/`initial`)
- Pagination and content splitting across pages
- Leaders/trailers (headers/footers per page)
- Rich text rendering with text wrapping and font metrics
- Scripting integration (layout reacts to FormCalc calculations)

### 5.4 xfa-json

JSON serialization of XFA layout output via `serde` + `indexmap` (ordered maps).

---

## 6. Layer 3: Document Abstraction

### pdf-engine

**Purpose:** Unified high-level API for PDF documents. Central integration point for all SDK capabilities.

**Key modules:**

| Module | Purpose |
|--------|---------|
| `document` | `PdfDocument` — unified document handle |
| `render` | Page rasterization with configurable DPI, color, rotation |
| `text` | Text extraction with positioned characters, CMap-aware Unicode |
| `thumbnail` | Fast thumbnail generation |
| `geometry` | Page bounds, MediaBox/CropBox/BleedBox/TrimBox, rotation |

**Dependencies:** Integrates `pdf-syntax`, `pdf-interpret`, `pdf-render`, `pdf-forms`, `pdf-xfa`.

---

## 7. Layer 4: Forms & Annotations

### 7.1 pdf-forms

**Standard:** ISO 32000-2 §12.7 (Interactive Forms)

**Architecture:**
- `FieldTree` — Arena-allocated field hierarchy (optimized for deep nesting)
- `FormAccess` trait — unified facade over field tree
- Field types: Text, Checkbox, RadioButton, Dropdown, ListBox, PushButton, Submit, Reset

**Capabilities:**
- AcroForm dictionary parsing and field tree construction
- Default Appearance (DA) parsing and appearance stream generation
- Field validation, calculation, and format scripts
- Form flattening — converting fields to static content
- **Form value persistence** — save field values back to PDF (#300)
- **Programmatic field creation** — AcroForm field builder API (#301)

### 7.2 pdf-annot

**Standard:** ISO 32000-2 §12.5 (Annotations)

**Supported annotation types (all 12.5 types):**
- **Markup:** Text (sticky note), Highlight, Underline, StrikeOut, Squiggly
- **Geometric:** Line, Square, Circle, Polygon, PolyLine, Ink
- **Document:** Link (URI, GoTo, GoToR), FreeText, Stamp, FileAttachment, Popup

**Write support** (feature-gated via `write`):
- Annotation creation with builder pattern (#302–#305)
- Appearance stream generation for all types
- lopdf-backed serialization

---

## 8. Layer 5: Manipulation & Content Intelligence

### 8.1 pdf-manip

**Purpose:** PDF mutation operations using lopdf as the write backend.

**Modules:**

| Module | Capability | Issue |
|--------|------------|-------|
| `page_ops` | Merge, split, insert, delete, rearrange pages | #194 |
| `encrypt` | Encryption, decryption, password protection | #195 |
| `watermark` | Text and image overlay watermarks | #196 |
| `optimize` | PDF compression and optimization | #197 |
| `bookmarks` | Bookmark/outline reading and creation | #198 |
| `headers` | Headers, footers, Bates numbering | #309 |
| `content_editor` | Content stream round-trip (decode → manipulate → encode) | #310 |
| `text_run` | CMap-aware text run extraction from content streams | #311 |
| `text_replace` | Find-and-replace text in content streams | #312 |
| `image_insert` | Add JPEG/PNG images to pages (feature: `image-insert`) | #313 |
| `downsample` | Reduce DPI of embedded images (feature: `image-insert`) | #314 |
| `font_subset` | Font subsetting via `subsetter` crate (feature: `font-subset`) | #315 |
| `pdfa_xmp` | PDF/A XMP metadata repair via `xmp-writer` | #317 |
| `pdfa_fonts` | PDF/A font embedding and subsetting | #318 |
| `pdfa_colorspace` | Color space normalization, OutputIntent injection | #319 |
| `pdfa_cleanup` | Transparency flattening, JS/EmbeddedFiles removal | #320 |

**Content Editor architecture:**

```
PDF page content stream (bytes)
    ↓ lopdf::content::Content::decode()
Vec<Operation>  ← ContentEditor wraps this
    ↓ GraphicsStateTracker records state snapshots
    ↓ Manipulation: remove, replace, insert operations
    ↓ ContentEditor::encode()
Modified content stream (bytes)
    ↓ write_editor_to_page()
Updated PDF page
```

**Text Run extraction:**

```
Content stream → ContentEditor → Operations
    ↓
FontMap (per-page font resources + ToUnicode CMaps)
    ↓
extract_text_runs() → Vec<TextRun>
    ↓
Each TextRun: { text, ops_range, font_name, font_size, x, y, width }
```

**Font encoding support:**
- `Builtin` — single-byte (Latin-1 range, WinAnsiEncoding)
- `IdentityH` — 2-byte CID font (Identity-H CMap)
- `CustomCMap` — parsed ToUnicode CMap
- Reverse CMap lookup for text replacement (Unicode → character code)

### 8.2 pdf-extract

**Purpose:** Content extraction from PDF documents.

**Capabilities:**
- Image extraction (embedded images as raw bytes + metadata) (#201)
- Positioned text extraction (characters with bounding boxes) (#202)
- Full-text search with page-level results (#202)
- `PositionedChar` — character + bounding box `[x0, y0, x1, y1]`

### 8.3 pdf-redact

**Purpose:** GDPR-compliant permanent content removal.

**Architecture (dual-layer):**

```
Layer 1: Visual redaction (Redactor)
  ├── Draw colored rectangles over redacted areas
  ├── Optional overlay text (e.g., "[REDACTED]")
  └── Metadata stripping

Layer 2: Content stream surgery (search_and_redact)
  ├── Text pattern matching (exact + regex)
  ├── Positioned char → bounding rectangle computation
  ├── ContentEditor removes matching text operations
  └── Combined: overlay + removal = permanent redaction
```

**Key types:**
- `RedactionArea` — page number + bounding rect + color + overlay
- `Redactor` — applies redaction areas to a lopdf Document
- `RedactSearchOptions` — case sensitivity, regex, fill color, page filter
- `SearchRedactReport` — matches found, areas redacted, operations removed

### 8.4 pdf-ocr

**Purpose:** OCR for scanned PDFs. Pluggable engine design with multiple backends.

**Backends:**

| Backend | Feature flag | Crate | Description |
|---------|-------------|-------|-------------|
| Tesseract | `tesseract` | `leptess` | Rust bindings to Tesseract OCR engine |
| PaddleOCR | `paddle` | `ort` + `ndarray` | ONNX Runtime-based pipeline (no C++ dependencies) |

**PaddleOCR pipeline** (#399–#401):

```
Input image
    ↓ DBNet text detection model (ONNX) (#399)
    ↓   → bounding box proposals → NMS filtering
    ↓ Angle classifier (0°/180° rotation detection) (#401)
    ↓ SVTR text recognition model (ONNX) + CTC decode (#400)
    ↓   → character probabilities → greedy decode → Unicode text
    ↓
Vec<OcrWord> { text, confidence, bbox }
```

**Model management** (#401):
- Automatic model download from PaddleOCR repository
- Local cache at `~/.cache/paddle-ocr/` (via `dirs-next`)
- Three ONNX models: detection (DBNet), recognition (SVTR), angle classifier

**Key types:**
- `OcrEngine` trait — pluggable backend interface
- `PaddleOcrEngine` — PaddleOCR ONNX pipeline
- `TesseractEngine` — Tesseract wrapper
- `OcrConfig` — DPI, language, engine selection
- `make_searchable()` — converts scanned PDF to searchable PDF

---

## 9. Layer 6: Compliance & Standards

### pdf-compliance

**Purpose:** Validate and generate standards-conformant PDFs.

**Standards supported:**

| Standard | ISO | Validation | Generation |
|----------|-----|------------|------------|
| PDF/A-1a, 1b | ISO 19005-1 | Yes | Yes |
| PDF/A-2a, 2b, 2u | ISO 19005-2 | Yes | Yes |
| PDF/A-3a, 3b, 3u | ISO 19005-3 | Yes | Yes |
| PDF/A-4 | ISO 19005-4 | Yes | Yes |
| PDF/UA-1 | ISO 14289-1 | Yes | Yes (#322) |
| PDF/X-1a, 3, 4 | ISO 15930 | Yes (#321) | Yes (#321) |

**PDF/A validation architecture:**

```
Pdf (pdf-syntax)
    ↓ ObjectCache (pre-collected objects, single parse)
    ↓ detect_pdfa_level() from XMP metadata
    ↓
Per-clause checks (§6.1.x through §6.12.x):
    ├── §6.1: File structure (header, trailer, xref, streams)
    ├── §6.2: Graphics (color spaces, rendering intents, halftone)
    ├── §6.3: Fonts (embedding, encoding, metrics)
    ├── §6.4: Transparency
    ├── §6.5: Annotations (flags, appearances)
    ├── §6.6: Actions (forbidden actions)
    ├── §6.7: Metadata (XMP, document info)
    ├── §6.8: Logical structure (tagged PDF)
    ├── §6.9: Optional content
    └── §6.10-12: Additional constraints per PDF/A part
    ↓
ComplianceReport { issues: Vec<ComplianceIssue> }
```

**Performance optimizations:**
- `ObjectCache` — pre-collects all objects once to avoid repeated O(n) parsing
- `new_bounded()` — skips caching for PDFs with >20K objects
- `MaybeRef` — checks direct values without resolving indirect references
- Bounded struct tree walking (max depth 100, max nodes 10,000)
- Inline image scan capped at 1 MB content / 200 images per page

**PDF/A conversion pipeline** (non-conformant → conformant):

```
Input PDF (lopdf Document)
    ↓ pdfa_xmp: inject/repair XMP metadata (#317)
    ↓ pdfa_fonts: embed and subset all fonts (#318)
    ↓ pdfa_colorspace: normalize color spaces, inject OutputIntent (#319)
    ↓ pdfa_cleanup: remove JS, embedded files, flatten transparency (#320)
    ↓
PDF/A-conformant output
```

**PDF/UA generation** (#322):
- Structure tree generation with role mappings
- Tagged PDF creation with reading order
- Alt text and accessibility metadata

**Compliance test results (veraPDF oracle):**
- Baseline: 1,383 failures
- After 10 optimization iterations: 526 failures
- **62% reduction** in false negatives

---

## 10. Layer 7: Digital Signatures

### pdf-sign

**Standards:** PAdES (ETSI EN 319 142), CMS (RFC 5652), X.509, RFC 3161

**Validation capabilities:**
- PAdES baseline signature validation (#176)
- Certificate chain validation with OCSP/CRL revocation checking (#177)
- DocMDP + FieldMDP permission handling (#178)
- Signature appearance rendering + LTV embedding (#179)

**Signing capabilities:**
- `PdfSigner` trait + `Pkcs12Signer` — pluggable signing with PKCS#12 identity loading (#396)
- CMS SignedData DER builder — manual RFC 5652 construction with signed attributes (#396)
- Two-pass PDF signing — `sign_pdf()` / `sign_pdf_incremental()` with ByteRange placeholder injection (#398)
- DocMDP certification signatures with configurable permission levels (#398)
- Visible signature rectangles with appearance streams (#398)
- Incremental (append-only) signing preserving existing signatures (#398)
- RFC 3161 TSA timestamp embedding for PAdES-T compliance (#397)
- LTV embedding (PAdES B-LT, B-LTA) (#308)

**Signing architecture:**

```
PKCS#12 (.p12/.pfx)
    ↓ Pkcs12Signer::from_pkcs12()
    ↓ Detects key type (RSA, ECDSA P-256/P-384) from PKCS#8 AlgorithmIdentifier
    ↓
sign_pdf(pdf_bytes, signer, options)
    ↓ Pass 1: Prepare PDF with /Contents placeholder (lopdf)
    ↓          Build sig dict, AcroForm, widget annotation
    ↓          Optional: DocMDP certification, visible signature rect
    ↓ Serialize → locate placeholders in byte stream
    ↓ Pass 2: Patch /ByteRange, hash byte ranges
    ↓          CMS SignedData construction (cms_builder)
    ↓          Hex-encode and inject into /Contents
    ↓
Signed PDF (validates with pdf-syntax validation pipeline)
```

**Key modules:**

| Module | Purpose |
|--------|---------|
| `signer` | `PdfSigner` trait, `Pkcs12Signer` (RSA/ECDSA), key type detection |
| `cms_builder` | DER encoding helpers, CMS SignedData construction (RFC 5652) |
| `sign` | `sign_pdf()`, `sign_pdf_incremental()`, ByteRange injection, DocMDP |
| `tsa` | RFC 3161 timestamp request/response, TSA token embedding |
| `cms` | CMS parsing for validation |
| `chain` | Certificate chain verification |
| `docmdp` | DocMDP/FieldMDP permission handling |
| `ltv` | Long-Term Validation support |

**Crypto stack (pure Rust, RustCrypto):**

| Algorithm | Crate | Usage |
|-----------|-------|-------|
| SHA-256, SHA-384, SHA-1 | `sha2`, `sha1` | Hash computation |
| RSA PKCS#1 v1.5 | `rsa` | Signature verification/creation |
| ECDSA P-256 | `p256`, `ecdsa` | Elliptic curve signatures |
| ECDSA P-384 | `p384` | Extended elliptic curve |
| X.509 | `spki`, `der` | Certificate parsing |
| PKCS#12 | `p12` | Identity loading (.p12/.pfx) |
| PKCS#8 | `pkcs8` | Private key parsing |

---

## 11. Layer 8: Data Exchange & Conversions

### 11.1 pdf-invoice

**Standards:** FDF (PDF spec §12.7.8), XFDF (ISO 19444-1), ZUGFeRD/Factur-X (EN 16931)

**Capabilities:**
- FDF import/export (#203)
- XFDF import/export (#203)
- ZUGFeRD/Factur-X e-invoicing XML generation (#205)
- Business rule validation against EN 16931 (#226)

### 11.2 Document Conversions

| Crate | Direction | Format | Key Dependencies | Issue |
|-------|-----------|--------|------------------|-------|
| `pdf-docx` | PDF → DOCX | Office Open XML | `quick-xml`, `zip` | #323 |
| `pdf-xlsx` | PDF → XLSX | Office Open XML | `rust_xlsxwriter` | #324 |
| `pdf-pptx` | PDF → PPTX | Office Open XML | `quick-xml`, `zip` | #325 |

**Conversion pipeline:**
```
PDF → pdf-extract (text + images + positions) → layout analysis → target format
```

### 11.3 pdf-diff

**Purpose:** Visual page-level PDF comparison (#327).

**Algorithm:** SSIM (Structural Similarity Index) for perceptual image comparison. Generates diff images highlighting changes.

### 11.4 HTML → PDF (planned)

Headless Chrome wrapper for HTML-to-PDF conversion (#326).

---

## 12. Layer 9: Language Bindings

### Binding Architecture

All bindings wrap `pdf-engine` (and select crates) through language-specific FFI mechanisms:

```
                     pdf-engine (Rust core)
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
      ┌──────────┐   ┌──────────┐   ┌──────────┐
      │ pdf-capi │   │pdf-python│   │ pdf-node │
      │ extern"C"│   │  PyO3    │   │ napi-rs  │
      └────┬─────┘   └──────────┘   └──────────┘
           │
    ┌──────┼──────┐
    ▼      ▼      ▼
  .NET   Java   Swift    ← Consume C API
  P/Inv  JNI    wrapper
```

### Current Bindings

| Binding | Crate | Mechanism | Status |
|---------|-------|-----------|--------|
| **C** | `pdf-capi` | `extern "C"` + `cdylib` + `staticlib` | Done |
| **Python** | `pdf-python` | PyO3 v0.24, built via `maturin` | Done |
| **Node.js** | `pdf-node` | napi-rs, async API, TypeScript typings | Done |
| **WASM** | `xfa-wasm` | wasm-bindgen, runs in browser | Done (base) |

### Planned Bindings

| Binding | Mechanism | Issue |
|---------|-----------|-------|
| **Java** | JNI via C API | #335 |
| **.NET** | P/Invoke via C API | #336 |
| **iOS (Swift)** | Swift wrapper over C API | #337 |
| **Android (Kotlin)** | Kotlin wrapper via JNI | #338 |
| **WASM extended** | Add rendering, annotations, signing | #339 |

### API Documentation

Comprehensive documentation and example code for all bindings (#340).

---

## 13. Layer 10: Desktop Application

### pdf-desktop (Tauri v2)

**Purpose:** Native desktop PDF viewer/editor comparable to Adobe Acrobat Pro.

**Stack:** Tauri v2 (Rust backend + WebView frontend) with `pdf-engine` for rendering.

**Architecture:**

```
┌──────────────────────────────────────────┐
│              Tauri WebView               │
│  ┌────────────────────────────────────┐  │
│  │  React/Svelte Frontend            │  │
│  │  ├── PDF Viewport (canvas)        │  │
│  │  ├── Thumbnail Sidebar (#329)     │  │
│  │  ├── Annotation Toolbar (#331)    │  │
│  │  └── File Manager UI (#333)       │  │
│  └─────────────┬──────────────────────┘  │
│                │ IPC (Tauri commands)     │
│  ┌─────────────┴──────────────────────┐  │
│  │  Rust Backend                      │  │
│  │  ├── pdf-engine (render, text)     │  │
│  │  ├── pdf-annot (annotations)      │  │
│  │  ├── lopdf (mutations)            │  │
│  │  └── pdf-sign (signatures)        │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

**Features:**

| Feature | Description | Issue |
|---------|-------------|-------|
| Window shell | Window, menu bar, PDF viewport | #328 |
| Thumbnails | Sidebar with page previews | #329 |
| Text selection | Select and copy to clipboard | #330 |
| Annotations | Toolbar with property panel | #331 |
| Print | Platform-native print support | #332 |
| Undo/Redo | Operation stack + file management | #333 |
| Theming | Keyboard shortcuts + dark mode | #334 |

---

## 14. Layer 11: CLI & Server

### 14.1 xfa-cli

**Purpose:** Multi-tool CLI binary with subcommands.

**Binaries:**
- `xfa-cli` — main CLI entry point
- `xfa-collector` — corpus data collection
- `edge-case-report` / `edge-case-analyzer` — edge case analysis
- `accuracy-report` — rendering accuracy reporting
- `xfa-license-tool` — license key management
- `corpus-render` — batch rendering

### 14.2 xfa-api-server

**Purpose:** Async HTTP API server for PDF processing.

**Stack:** Axum + Tokio (async runtime).

---

## 15. Testing & Quality Infrastructure

### 15.1 Test Ecosystem

```
┌─────────────────────────────────────────────────────────┐
│                    xfa-test-runner                        │
│  ┌─────────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │ Corpus       │  │ Oracles  │  │ Results Database  │  │
│  │ Manager      │  │          │  │                   │  │
│  │              │  │ veraPDF  │  │ SQLite per run    │  │
│  │ 181K PDFs    │  │ PDFium   │  │ Trend tracking    │  │
│  │ 32K stress   │  │ Poppler  │  │ Regression detect │  │
│  └──────────────┘  └──────────┘  └───────────────────┘  │
│                                                          │
│  12 conformance tests per PDF:                           │
│  parse, metadata, text, render, forms, annotations,      │
│  signatures, compliance, manipulation, encrypt/decrypt,  │
│  round-trip, performance                                  │
└─────────────────────────────────────────────────────────┘
```

### 15.2 Test Infrastructure (VPS)

| Component | Details |
|-----------|---------|
| **Server** | Hetzner CX53 (46.225.223.175) |
| **User** | `xfa` service user |
| **Main corpus** | ~181K PDFs at `/opt/xfa-corpus` |
| **Stressful corpus** | 32,574 SafeDocs PDFs at `/opt/xfa-corpus/stressful` |
| **veraPDF suite** | `/opt/xfa-corpus/tagged/verapdf` |
| **Storage** | CIFS mount at `/mnt/storagebox` (5 TB, ~52 GB used) |
| **Skip list** | `scripts/corpus-skip.txt` (18 entries) |

### 15.3 SafeDocs Stressful Corpus Results (#273)

- 32,574 PDFs processed
- 32,190 pass / 190 fail / 191 skip / 3 timeout / **0 crashes**
- 6 crashers found and fixed: page tree bomb, xref recursion (×2), predictor OOM, crypto bounds, oversized file

### 15.4 Testing Improvements (Open Issues)

| Issue | Description |
|-------|-------------|
| #349 | `--rerun-failures` flag for targeted reruns |
| #350 | Per-test timeout instead of per-PDF |
| #351 | Adaptive timeout escalation per tier |
| #352 | Structural content stream size guard |
| #353 | Shared per-page content stream cache |
| #354 | Show applicable pass rate metric |
| #355 | Log profiling data on timeout |
| #356 | Two-phase validation (structural → content) |
| #357 | `--affected-by` filter for targeted reruns |
| #358 | Incremental results via content hashing |
| #359 | Structured crash forensics table in database |

### 15.5 Other Test Crates

| Crate | Purpose |
|-------|---------|
| `xfa-golden-tests` | Visual regression: render → PNG → pixel diff against Adobe Reader |
| `pdf-bench` | Criterion benchmarks: `pdf_parse`, `xfa_operations` |
| `pdf-diff` | SSIM-based visual PDF comparison |
| `fuzz/` | cargo-fuzz harness for parser fuzzing |

---

## 16. Dependency Graph

### Critical Path

```
pdf-syntax → pdf-interpret → pdf-render → pdf-engine → bindings/apps
                    ↓
               pdf-font
```

### XFA Path

```
xfa-dom-resolver → formcalc-interpreter → xfa-layout-engine → xfa-json
                                                    ↓
                                               pdf-xfa → pdf-engine
```

### Mutation Path

```
lopdf → pdf-forms (AcroForm write)
      → pdf-annot (annotation write)
      → pdf-manip (pages, fonts, encryption, PDF/A conversion)
      → pdf-redact (content removal)
      → pdf-sign (signature creation)
```

### Intelligence Path

```
pdf-extract (text/images) → pdf-redact (GDPR)
                           → pdf-ocr (scanned PDFs)
                           → pdf-docx / pdf-xlsx / pdf-pptx (conversions)
```

### Fan-in (most depended on)

| Crate | Depended on by |
|-------|----------------|
| `pdf-syntax` | 6+ internal crates |
| `lopdf` | 10+ internal crates |
| `pdf-engine` | 6+ internal crates + all bindings |
| `xfa-dom-resolver` | 5 internal crates |

### Fan-out (most dependencies)

| Crate | Depends on |
|-------|------------|
| `xfa-cli` | 12+ workspace crates |
| `pdf-engine` | 5 workspace crates |
| `pdfium-ffi-bridge` | 6 workspace crates |

---

## 17. External Dependencies

### Core Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `lopdf` | 0.39 | PDF read-write mutation |
| `roxmltree` | 0.20 | Read-only XML DOM (XFA, XFDF) |
| `thiserror` | 2 | Error type derivation |
| `image` | 0.25 | Bitmap I/O (PNG, JPEG) |
| `flate2` | 1 | Deflate compression |
| `regex` | 1 | Text pattern matching |

### Rendering Stack

| Crate | Version | Purpose |
|-------|---------|---------|
| `vello_cpu` | 0.0.6 | CPU vector rasterizer |
| `skrifa` | 0.40 | Modern font parsing |
| `kurbo` | 0.13 | 2D geometry primitives |
| `ttf-parser` | 0.25 | Font metrics |

### Crypto Stack

| Crate | Purpose |
|-------|---------|
| `sha2`, `sha1` | Hash computation |
| `rsa` | RSA signatures |
| `p256`, `p384`, `ecdsa` | Elliptic curve signatures |
| `spki`, `der` | X.509 certificate parsing |
| `p12` | PKCS#12 identity loading |
| `pkcs8` | Private key parsing |
| `hmac` | License key validation |

### OCR / ML Stack

| Crate | Version | Purpose |
|-------|---------|---------|
| `ort` | 2.0.0-rc.12 | ONNX Runtime bindings (PaddleOCR inference) |
| `ndarray` | 0.17 | N-dimensional arrays for model I/O |
| `leptess` | 0.14 | Tesseract OCR bindings |

### HTTP Client

| Crate | Version | Purpose |
|-------|---------|---------|
| `ureq` | 2 | TSA timestamp requests (pdf-sign) |
| `ureq` | 3 | ONNX model download (pdf-ocr) |

### PDF/A Conversion

| Crate | Version | Purpose |
|-------|---------|---------|
| `subsetter` | 0.2 | Font subsetting |
| `xmp-writer` | 0.3 | XMP metadata generation |

### Binding Frameworks

| Crate | Version | Purpose |
|-------|---------|---------|
| `pyo3` | 0.24 | Python FFI |
| `napi` | — | Node.js FFI |
| `wasm-bindgen` | — | WebAssembly FFI |
| `tauri` | 2 | Desktop app framework |

### Build & Test

| Crate | Purpose |
|-------|---------|
| `criterion` | Benchmarking |
| `pixelmatch` | Pixel-level image comparison |
| `tempfile` | Test file management |

---

## 18. Security Architecture

### Memory Safety

- No `unsafe` in application code (only in FFI boundary crates)
- All PDF parsing is bounds-checked — malformed PDFs cannot cause UB
- Arena-allocated data structures prevent use-after-free

### Resource Limits

| Resource | Limit | Reason |
|----------|-------|--------|
| File size | 256 MB | Prevent OOM on oversized files |
| Decompressed stream | 64 MB | Prevent zip bomb attacks |
| Page count | 100,000 | Prevent page tree bombs |
| XRef chain depth | 64 | Prevent xref recursion |
| Inline nesting | 256 | Prevent content stream recursion |
| Struct tree depth | 100 | Prevent compliance recursion |
| Thread stack | 8 MB | Prevent stack overflow on deep recursion |

### Encryption Support

- RC4 (40-bit, 128-bit)
- AES-128, AES-256
- Password-protected PDF detection and handling
- User/owner password distinction

### GDPR Compliance

`pdf-redact` provides permanent content removal:
1. Visual overlay (colored rectangles)
2. Content stream surgery (operation removal)
3. Metadata stripping
4. No recoverable data left in output

---

## 19. Performance Architecture

### Parsing

- **Zero-copy parsing** — `pdf-syntax` borrows from input buffer, no allocation for object access
- **Lazy object resolution** — objects parsed on demand, not upfront
- **ObjectCache** — pre-collects objects for compliance checking (single O(n) pass)
- **MaybeRef** — checks direct values without resolving indirect references

### Rendering

- **vello_cpu** — modern CPU rasterizer with SIMD support
- **Lazy page loading** — pages rendered on demand
- **Thumbnail caching** — pre-computed low-res page previews
- **Parallel rendering** — independent pages can render concurrently

### Font Processing

- **phf** (perfect hash function) — O(1) lookup for built-in CMap tables
- **Font subsetting** — reduces embedded font size to only used glyphs
- **Graceful degradation** — argstack underflow returns defaults instead of panicking

### Testing

- **Tiered execution** — fast/standard/full test tiers
- **Resume support** — `--resume` continues interrupted corpus runs
- **Content hashing** — skip unchanged PDFs in incremental runs (#358)
- **`--limit N`** — cap PDF count for quick validation

---

## 20. Implementation Status

### Completed Phases

| Phase | Description | Issues |
|-------|-------------|--------|
| **Epic 0** | Project foundation, CI/CD, test infrastructure | #1–#4 |
| **Epic 1** | XFA DOM/SOM resolution | #5–#9 |
| **Epic 2** | FormCalc interpreter (90+ built-in functions) | #10–#19 |
| **Epic 3** | XFA layout engine (Box Model, pagination, reflow) | #20–#32 |
| **Epic 4** | Native PDF I/O and rendering (pure Rust) | #33–#38 |
| **Epic 5** | Persistence and security (UR3 signatures) | #39–#42 |
| **Epic 6** | Validation, benchmarks, edge case hardening | #43–#46 |
| **Fase A** | Workspace restructure, hayro fork integration | #143–#148 |
| **Fase B** | AcroForm engine (field tree, appearance, flattening) | #149–#156 |
| **Fase C** | Annotation engine (all ISO 32000-2 types) | #157–#161 |
| **Fase D** | XFA integration (packet extraction, rendering bridge) | #162–#165 |
| **Fase E** | Test corpus and fuzzing infrastructure | #166–#169 |
| **Fase F** | Rendering pipeline (pages, text extraction, thumbnails) | #170–#175 |
| **Fase G** | Digital signatures (PAdES, certificate chains, LTV) | #176–#179 |
| **Fase H** | Compliance (PDF/A validation + conversion, PDF/UA, tagged PDF) | #180–#183 |
| **Fase I** | C API (PDFium-compatible interface) | #184–#185 |
| **Fase J** | Python bindings (PyO3 + PyPI) | #186–#187 |
| **Fase K** | WASM + CLI tools | #188–#190 |
| **Fase L** | Node.js bindings (napi-rs + npm) | #191–#192 |
| **Fase M** | PDF manipulation (merge, split, encrypt, watermark, compress) | #194–#198 |
| **Fase N** | Content intelligence (OCR, redaction, extraction, search) | #199–#202 |
| **Fase O** | Data exchange (FDF/XFDF, ZUGFeRD/Factur-X) | #203–#205 |
| **SDK Core 1** | Write APIs (forms, annotations, signatures, headers) | #300–#309 |
| **SDK Core 2** | Content engine (content editor, text runs, text replace, redact) | #310–#316 |
| **SDK Core 3** | Compliance conversion + document conversions | #317–#327 |
| **Signing** | PdfSigner + PKCS#12, CMS builder, two-pass signing, DocMDP, TSA timestamps | #396–#398 |
| **OCR** | PaddleOCR pipeline: DBNet detection, SVTR recognition, angle classifier, model management | #399–#401 |

### Open — Desktop Application (Fase 4)

| Issue | Feature | Status |
|-------|---------|--------|
| #328 | Tauri desktop shell (window, menu, viewport) | Done |
| #329 | Thumbnail sidebar | Done |
| #330 | Text selection and copy | Done |
| #331 | Annotation toolbar with property panel | Open |
| #332 | Print support (platform-native) | Open |
| #333 | Undo/Redo stack and file management | Open |
| #334 | Keyboard shortcuts and dark mode | Open |

### Open — Language Bindings (Fase 5)

| Issue | Binding | Status |
|-------|---------|--------|
| #335 | Java (JNI) | Open |
| #336 | .NET (P/Invoke) | Open |
| #337 | iOS (Swift) | Open |
| #338 | Android (Kotlin) | Open |
| #339 | WASM extended (rendering, annotations, signing) | Open |
| #340 | API documentation for all bindings | Open |

### Open — Test Infrastructure

| Issue | Feature | Status |
|-------|---------|--------|
| #239 | Corpus expansion (GovDocs1 + SafeDocs full) | Open |
| #349–#359 | Test runner improvements (11 issues) | Open |

---

*This document serves as the single source of truth for the technical architecture of XFA-Native-Rust. Update it as new features are implemented or architectural decisions change.*
