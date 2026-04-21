# RFC 0001 — SDK Core API

**Status:** DRAFT — under review for API Design Freeze (#1239)
**Milestone:** #52 — SDK Core Facade & API Ergonomics (1.0 GA Blocker)
**Covers issues:** #1214, #1215, #1216, #1217, #1218
**Authors:** PDFluent core team
**Date:** 2026-04-21

---

## 0. Summary

This RFC freezes the public API surface of the `pdfluent` crate for the 1.0 release. All design decisions in this document are binding once API Design Freeze (#1239) closes. Breaking changes after freeze require a new RFC with the freeze re-opened explicitly.

The RFC covers:
- Core object model and lifecycle
- Naming conventions
- Builder vs direct API patterns
- Sync/async strategy
- Error model
- Capability model and tier mapping
- Cross-language binding contract
- Three end-to-end flows written against the frozen API

---

## 1. Object model

### 1.1 `PdfDocument` — the central type

`PdfDocument` is the owning container for a parsed PDF. It is:

- `Send + Sync` — usable across threads and behind `Arc`.
- Not `Copy`, not `Clone`-by-default. `Clone` is implemented but documented as expensive (full-document copy, O(doc-size) allocations).
- Constructed via `open`, `from_bytes`, `from_reader`, or `create`.
- Mutated via `&mut self` methods, or via scoped `_mut` accessors (`form_mut`, `metadata_mut`, `pages_mut`, `annotations_mut`, `bookmarks_mut`, `permissions_mut`).
- Serialised via `save`, `to_bytes`, or `write_to`.
- Dropped deterministically — no hidden async cleanup.

### 1.2 Lifecycle

```
         ┌──────────────────────────────────────────────────────┐
         │                                                      │
  ::open ┤                                                      │
::from_bytes                                                    │
::from_reader ─────▶ &PdfDocument  (read-only ops)              │
  ::create │                                                    │
         └─▶ &mut PdfDocument ─▶ _mut accessors  ──▶ .save(path)│
                                                    .to_bytes() │
                                                    .write_to(W)│
                                                                │
                                                                ▼
                                                            (drop)
```

Rules:

- `_mut` accessors borrow `&mut self`. Rust's borrow checker prevents two conflicting mutations simultaneously.
- All I/O methods are explicit. No hidden writes.
- Opening a PDF does not mutate the source file. `save` writes to a new path or overwrites only when explicitly requested via `SaveOptions::overwrite(true)`.

### 1.3 `Page`, `Pages`, `PagesMut`

- `Page<'a>` — borrowed handle to a single page (`doc.page(n)?`). Lives for `'a = lifetime of &doc`.
- `Pages<'a>` — iterator + random-access over all pages.
- `PagesMut<'a>` — mutating iterator, supports `rotate`, `delete`, `crop`, `insert_blank`, `reorder`.

---

## 2. Naming conventions

Binding rules:

| Category | Convention | Example |
|---|---|---|
| Open from path | `open`, `open_with` | `PdfDocument::open("file.pdf")?` |
| Construct from memory | `from_bytes`, `from_bytes_with`, `from_reader`, `create` | `PdfDocument::from_bytes(&buf)?` |
| Persist | `save`, `save_with`, `to_bytes`, `write_to` | `doc.save("out.pdf")?` |
| Collections (read) | plural noun, no prefix | `doc.signatures()`, `doc.form_fields()` |
| Mutable sub-object | `<noun>_mut()` | `doc.form_mut()`, `doc.metadata_mut()` |
| In-place mutation | imperative verb on `&mut self` | `doc.encrypt(opts)`, `doc.flatten_forms()` |
| Transformation → new object | `to_<target>` or `into_<target>` | `doc.to_pdfa(PdfAProfile::A2b)?` |
| Options struct | `<Operation>Options` with builder methods | `SaveOptions::new().with_linearize(true)` |
| Enum variants | PascalCase nouns, no `Kind` suffix | `Tier::Developer` |
| Boolean predicates | `is_*`, `has_*`, `can_*` | `sig.is_valid()`, `doc.has_xfa()` |
| Builder types | `<Noun>Builder` only if distinct type | `PdfDocumentBuilder`; but `PdfMerger` is its own builder |

Forbidden:

- `get_*` prefixes on getters (non-idiomatic Rust).
- `set_*` on `&mut self` method on top-level `PdfDocument` (use `_mut` accessor instead).
- Suffix `_async` on method names (use submodule `pdfluent::r#async`).
- `Config` as generic suffix (use `Options`).

---

## 3. Builder vs direct

Three patterns, bound by rules:

### 3.1 Direct methods — single operation

```rust
doc.rotate_page(3, Rotation::D90)?;
doc.add_watermark("DRAFT", WatermarkOptions::centered().rotated(45.0))?;
doc.encrypt(EncryptOptions::aes256().with_permissions(Permissions::print_only()))?;
```

Each call completes or fails atomically.

### 3.2 Fluent `&mut self` chaining — pipelines on one document

```rust
doc.pages_mut()
    .rotate(3, Rotation::D90)?
    .crop(5, [0.0, 0.0, 595.0, 842.0])?
    .delete(7..=10)?;
```

Every chain step returns `Result<&mut Self, Error>`. Propagation via `?`.

### 3.3 Factory builder — multi-document assembly

```rust
let merged = PdfMerger::new()
    .add(PdfDocument::open("a.pdf")?)
    .add(PdfDocument::open("b.pdf")?)
    .with_bookmarks(BookmarkMergeStrategy::Concat)
    .build()?;
```

`PdfMerger` consumes inputs by move. `build()` produces a new `PdfDocument`. Inputs are dropped after consumption.

**Hard rule:** a method returns *exactly one* of:
- `Result<&mut Self>` — builder chain step
- `Result<(), Error>` — side-effect only
- `Result<T, Error>` — pure computation

Never a mix.

---

## 4. Sync vs async

**Default: synchronous API.** The entire `pdfluent::*` public surface is sync.

**Async is feature-gated under `async-tokio`.** Enabled via:

```toml
pdfluent = { version = "1.0", features = ["async-tokio"] }
```

Async types live under `pdfluent::r#async`:

```rust
use pdfluent::r#async::PdfDocument;

let doc = PdfDocument::open("in.pdf").await?;
```

Rules:

- Only `tokio`. No `async-std`, no `smol`.
- Types are distinct between `pdfluent::PdfDocument` (sync) and `pdfluent::r#async::PdfDocument` (async). No bridge methods.
- Async is marked **beta** in 1.0; promoted to stable in 1.1 if demand materialises.
- CPU-bound work runs via `tokio::task::spawn_blocking`. I/O uses `tokio::fs`.
- `pdfluent::r#async` is **not a 1.0 GA blocker**.

Decision D4 (RFC §6.2 in design story): **sync default, feature-gated `async-tokio` opt-in.**

---

## 5. Error model

### 5.1 The `Error` enum

One `pdfluent::Error` enum for the entire public surface.

```rust
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    // I/O
    Io { source: std::io::Error, path: Option<PathBuf> },
    FileNotFound { path: PathBuf },

    // Parsing
    InvalidPdf { byte_offset: Option<u64>, reason: String },
    UnsupportedPdfVersion { found: String, supported_up_to: String },

    // Compliance
    PdfaValidationFailed { profile: PdfAProfile, violations: Vec<Violation> },

    // Security
    DecryptionFailed { reason: DecryptionFailureReason },
    InvalidSignature { field: String, reason: String },

    // Licensing
    FeatureNotInTier {
        capability: Capability,
        current_tier: Tier,
        required_tier: Tier,
        docs_url: &'static str,
        upgrade_url: &'static str,
    },
    CapabilityNotCompiled { capability: Capability, feature_flag: &'static str, docs_url: &'static str },
    InvalidLicense { reason: String, docs_url: &'static str },

    // Environment
    UnsupportedOnWasm { operation: &'static str, docs_url: &'static str },
    MissingDependency { dep: &'static str, install_hint: &'static str, docs_url: &'static str },

    // Budget
    MemoryBudgetExceeded { requested: usize, limit: usize },

    // Internal safety-net (never fires under normal operation)
    Internal { message: String, crate_version: &'static str },
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 5.2 Per-variant guarantees

Every variant has:

1. **Stable `code: &'static str`** of format `E-<CATEGORY>-<SPECIFIC>` (e.g., `E-IO-FILE-NOT-FOUND`). Frozen per snapshot test.
2. **Clear `Display` message** in Elm style: problem → context → suggestion → see-also.
3. **`docs_url: &'static str`** deep-linking to `https://pdfluent.com/errors/<code>`.

Forbidden:

- `Error::Other(String)` or `Error::Unknown`.
- `Box<dyn std::error::Error>` in public API.
- `anyhow::Error` in public signatures.
- Implementation-detail types exposed via `source()` chain (no `lopdf::Error`, no `pdf_sign::SignError`, etc.).

### 5.3 Conversions

`pub(crate) From<internal_error>` impls map internal errors to public variants. Users never see `lopdf`, `pdf_sign`, `pdf_manip`, etc. types.

### 5.4 Display example

```
Error: FeatureNotInTier
  needed capability: DigitalSignatureSign
  your tier: Developer
  required tier: Team (€1,499/yr)

  Digital signature signing requires the Team tier or higher.
  Upgrade: https://pdfluent.com/pricing
  Docs: https://pdfluent.com/errors/E-FEATURE-NOT-IN-TIER
```

---

## 6. Capability model

### 6.1 `Capability` enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Capability {
    // Core — always available (any tier including Trial)
    PdfParse,
    PdfWrite,
    PageOps,

    // Extraction
    TextExtract,
    TextExtractWithLayout,
    ImageExtract,
    TableExtract,

    // Rendering
    RenderRaster,
    RenderThumbnail,

    // Forms
    AcroFormRead,
    AcroFormFill,
    AcroFormFlatten,
    XfaParse,
    XfaFill,
    XfaFlatten,

    // Security
    EncryptionRead,
    EncryptionWrite,
    DigitalSignatureSign,
    DigitalSignatureVerify,
    PadesLongTerm,
    PadesLongTermArchive,

    // Compliance
    PdfaValidate,
    PdfaConvertA1b,
    PdfaConvertA2b,
    PdfaConvertA3b,
    PdfuaValidate,
    PdfuaConvert,
    EInvoiceZugferd,
    EInvoiceFacturX,
    EInvoiceXRechnung,

    // Advanced
    Redaction,
    OcrTesseract,
    OcrPaddle,
    Html2Pdf,
    DocxExport,
    XlsxExport,
    PptxExport,
    PdfDiff,

    // Deployment
    WasmRuntime,
    AirGapped,
    OemRedistribution,
}
```

### 6.2 `Tier` enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Trial,
    #[serde(alias = "basic")]
    Developer,
    #[serde(alias = "professional")]
    Team,
    Business,
    #[serde(alias = "archival")]
    Enterprise,
}
```

Backward-compat aliases accept old license JSON files during 1.0.x grace period.

### 6.3 Tier × Capability matrix

Canonical mapping — this table is the source of truth and is snapshot-tested against the pricing page.

| Capability | Trial (marked) | Developer | Team | Business | Enterprise |
|---|---|---|---|---|---|
| PdfParse, PdfWrite, PageOps | ✓ | ✓ | ✓ | ✓ | ✓ |
| TextExtract, ImageExtract | ✓ | ✓ | ✓ | ✓ | ✓ |
| RenderRaster, RenderThumbnail | ✓ | ✓ | ✓ | ✓ | ✓ |
| AcroFormRead, AcroFormFill, AcroFormFlatten | ✓ | ✓ | ✓ | ✓ | ✓ |
| EncryptionRead, EncryptionWrite | ✓ | ✓ | ✓ | ✓ | ✓ |
| PdfaValidate, PdfaConvertA1b/A2b/A3b | ✓ | ✗ | ✓ | ✓ | ✓ |
| DigitalSignatureSign, Verify, PadesLongTerm, PadesLongTermArchive | ✓ | ✗ | ✓ | ✓ | ✓ |
| Redaction | ✓ | ✗ | ✓ | ✓ | ✓ |
| PdfuaValidate, PdfuaConvert | ✓ | ✗ | ✓ | ✓ | ✓ |
| EInvoiceZugferd / FacturX / XRechnung | ✓ | ✗ | ✓ | ✓ | ✓ |
| XfaFlatten | ✓ | ✗ | ✗ | ✓ | ✓ |
| OcrTesseract, OcrPaddle | ✓ | ✗ | ✗ | ✓ | ✓ |
| DocxExport, XlsxExport, PptxExport | ✓ | ✗ | ✗ | ✓ | ✓ |
| Html2Pdf | ✓ | ✗ | ✗ | ✓ | ✓ |
| PdfDiff | ✓ | ✗ | ✗ | ✓ | ✓ |
| WasmRuntime | ✓ | ✓ | ✓ | ✓ | ✓ |
| AirGapped, OemRedistribution | ✗ | ✗ | ✗ | ✗ | ✓ |

Trial tier always succeeds on `require()` but the output is marked via the `/Producer` metadata field at `save` time. Trial does not block usage — it marks output.

### 6.4 Enforcement

Every gated method calls `self.license.require(Capability::X)?` as the first line:

```rust
impl PdfDocument {
    pub fn sign(
        &mut self,
        signer: &dyn PdfSigner,
        opts: SignOptions,
    ) -> Result<()> {
        self.license.require(Capability::DigitalSignatureSign)?;
        // ... wiring to pdf_sign::sign::sign_pdf
    }
}
```

Runtime cost is <10 ns (bitset lookup). Verified via benchmark in #1234.

---

## 7. Cross-language contract

Rust is the source of truth. Each binding mirrors the API 1:1 with idiomatic adaptations:

| Aspect | Rust | Python | Node.js | Java | C |
|---|---|---|---|---|---|
| Naming | snake_case methods | snake_case methods, `@property` for zero-arg getters | camelCase | `getX`/`setX` | `pdfluent_doc_x()` |
| Type `PdfDocument` | struct | class | class | class | opaque `pdfluent_doc_t*` |
| Error | `Result<T, Error>` | exceptions (`PdfLuentError` hierarchy) | Error subclass with `.code`/`.docsUrl` | checked `PdfLuentException` | `pdfluent_error_code_t` + thread-local message |
| Capability check | `license.require(cap)` | same, raises `FeatureNotInTierError` | same, throws Error | same, throws exception | returns non-zero error code |
| Async | `pdfluent::r#async` feature | `async def` equivalents (feature-gated) | native Promise-based | `CompletableFuture` | n/a |
| Error codes | `&'static str` (e.g., `"E-IO-FILE-NOT-FOUND"`) | identical string in `.code` | identical in `.code` | identical in `getCode()` | identical in lookup table |
| API-version contract | `pdfluent::api_version()` constant | `pdfluent.api_version()` | `pdfluent.apiVersion()` | `PdfLuent.apiVersion()` | `pdfluent_api_version()` |

Each binding pins to an exact `pdfluent` crate version. Runtime `api_version()` check on load.

Binding codegen is stub-assisted via `tools/pdfluent-bindgen/` (not full codegen — produces skeletons + diff).

---

## 8. Module structure

```
pdfluent/
├── src/
│   ├── lib.rs                  # Public re-exports + prelude
│   ├── document.rs             # PdfDocument, PdfDocumentBuilder, Page, Pages, PagesMut
│   ├── merger.rs               # PdfMerger, MergeOptions, BookmarkMergeStrategy
│   ├── signer.rs               # PdfSigner trait, Pkcs12Signer, SignOptions, Signature, SignatureValidationReport, PadesProfile
│   ├── form.rs                 # PdfFormMut, FormField, FieldType
│   ├── metadata.rs             # Metadata, MetadataMut
│   ├── encrypt.rs              # EncryptOptions, Permissions, PermissionsBuilder, EncryptionAlgorithm
│   ├── watermark.rs            # WatermarkOptions, Position, Layer, Alignment, Rotation
│   ├── redact.rs               # RedactOptions
│   ├── compliance.rs           # PdfAProfile, PdfAValidationReport, Violation
│   ├── decoration.rs           # PageDecoration (watermark + header_footer + page_numbers + stamp)
│   ├── error.rs                # Error enum, Result alias, code/docs_url accessors
│   ├── capability.rs           # Capability enum
│   ├── tier.rs                 # Tier enum, tier→capability mapping
│   ├── license.rs              # License struct, require() enforcement
│   ├── prelude.rs              # Re-exports of top-15 types
│   └── r#async/                # feature "async-tokio"
│       └── mod.rs
├── tests/
│   ├── web_examples/           # scraped from pdfluent.com
│   ├── fixtures/               # shared test PDFs
│   ├── e2e/                    # end-to-end workflow tests (#1246)
│   └── migration_examples/     # before/after snippets (#1241)
└── examples/                   # runnable examples shown in rustdoc
```

Only `lib.rs` symbols are public. All module paths are internal.

---

## 9. Three end-to-end flows (binding validation)

These three flows are required to compile and run against the frozen API before `#1239` closes.

### 9.1 Flow 1 — Document load → modify → save

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    // Load
    let mut doc = PdfDocument::open("input.pdf")?;

    // Inspect
    println!("Pages: {}", doc.page_count());
    println!("PDF version: {}", doc.version());

    // Modify metadata (mutable scoped accessor)
    doc.metadata_mut()
        .set_title("Processed Invoice")
        .set_author("PDFluent SDK")
        .set_subject("Q2 2026")
        .set_keywords(&["invoice", "q2", "processed"])
        .commit()?;

    // Modify content (direct method)
    doc.add_watermark(
        "CONFIDENTIAL",
        WatermarkOptions::centered()
            .rotated(45.0)
            .opacity(0.3)
            .layer(Layer::Foreground),
    )?;

    // Persist
    doc.save_with("output.pdf", |opts| opts.with_linearize(true))?;

    Ok(())
}
```

Capabilities exercised: `PdfParse`, `PdfWrite`, (metadata is core).

### 9.2 Flow 2 — Merge multiple PDFs

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let merged = PdfMerger::new()
        .add(PdfDocument::open("cover.pdf")?)
        .add(PdfDocument::open("body.pdf")?)
        .add(PdfDocument::open("appendix.pdf")?)
        .with_bookmarks(BookmarkMergeStrategy::Concat)
        .with_page_labels(true)
        .build()?;

    println!("Merged into {} pages", merged.page_count());
    merged.save("combined.pdf")?;

    Ok(())
}
```

Capabilities exercised: `PdfParse`, `PageOps`, `PdfWrite`.

### 9.3 Flow 3 — Sign document (PAdES B-LT)

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    // Load signer from PKCS#12
    let signer = Pkcs12Signer::from_pfx_file("identity.p12", "pkcs12-password")?;

    // Load document
    let mut doc = PdfDocument::open("contract.pdf")?;

    // Sign with visible appearance on page 1
    doc.sign(
        &signer,
        SignOptions::new()
            .reason("Contractual approval")
            .location("Amsterdam, NL")
            .contact_info("legal@example.com")
            .field_name("Signature1")
            .visible_rect(1, [50.0, 50.0, 250.0, 130.0])
            .profile(PadesProfile::LongTerm),
    )?;

    // Save
    doc.save("contract-signed.pdf")?;

    // Reopen and verify
    let signed = PdfDocument::open("contract-signed.pdf")?;
    let report = signed.verify_signatures()?;
    assert!(report.all_valid(), "signature validation failed: {:?}", report.failures());

    Ok(())
}
```

Capabilities exercised: `DigitalSignatureSign`, `PadesLongTerm`, `DigitalSignatureVerify`.

These three flows compile against the `pdfluent` crate skeleton delivered alongside this RFC (see `crates/pdfluent/tests/web_examples/`). `#1240` tracks the bootstrap validation against this API.

---

## 10. Decisions (D1–D10 from design story §6.2)

| # | Decision | Answer | Rationale |
|---|---|---|---|
| D1 | Tier rename: code → website or website → code? | **Code follows website.** Rename `Basic`→`Developer`, `Professional`→`Team`, merge `Archival` into `Enterprise` as capability subset. Serde aliases for back-compat. | Marketing-copy changes are more expensive than code changes. |
| D2 | Meta-crate in monorepo or separate repo? | **Monorepo** at `crates/pdfluent/`, published as separate crates.io package. | Tooling, CI, cross-crate refactors easier in monorepo. |
| D3 | Keep `pdf_engine::api::Document`? | **Deprecate** behind `internal-legacy` feature for 1.0.x, remove in 2.0. | One blessed public API avoids support confusion. |
| D4 | Async default or opt-in? | **Sync default**, feature-gated `async-tokio` opt-in. | See §4 rationale. Async is not a 1.0 GA blocker. |
| D5 | Capability gating also on inner crates? | **Only at `pdfluent` level.** | Power users can bypass; production use via `pdfluent` is the contract. |
| D6 | Binding sync scope in milestone? | **Top-30 methods** within milestone; full parity follow-up. | Full sync is ~3 days/language; covers 80% of how-tos. |
| D7 | Web-examples CI: SDK-repo or website-repo? | **SDK-repo source of truth.** | Snippets must compile against SDK. |
| D8 | Cargo feature granularity? | **Per capability family**, not per capability. | Avoids combinatorial explosion. |
| D9 | `PdfMerger` location? | **`pdfluent::merger` module**, re-exported at crate root. | Core use-case, belongs in main crate. |
| D10 | WASM capability scope in 1.0? | **Read-only subset:** PdfParse, TextExtract, ImageExtract, RenderRaster, PdfaValidate, AcroFormRead. Write-path capabilities (encrypt, sign, redact) unsupported on WASM in 1.0. | Reproducibility and bundle-size concerns; revisit in 1.1. |

---

## 11. Stability contract preview

Post-freeze, the following changes are:

- **Allowed in 1.x patches:** bug fixes, new error variant addition (enum is `#[non_exhaustive]`), new capabilities, new methods on `PdfDocument`, new Options fields (struct is `#[non_exhaustive]`).
- **Allowed in 1.x minors:** new features (behind Cargo features), new tier capabilities, deprecation warnings on existing methods, new bindings.
- **Requires 2.0:** renaming any public symbol, changing method signature, removing a deprecated symbol, changing tier → capability mapping, changing Capability variants.

Full policy lands in `STABILITY.md` under #1232.

---

## 12. Acceptance

This RFC is considered accepted when:

1. `#1214` closes (RFC merged to `main`).
2. `#1239` closes (API Design Freeze declared).
3. `#1215`, `#1216`, `#1217`, `#1218` all close.
4. Minimum 2 of the 3-5 bootstrap examples in `#1240` compile against this API.
5. Prototype PR demonstrating Flow 1 (§9.1) end-to-end merges to `main`.

After acceptance, the API is frozen. Breaking changes require a new RFC.

---


---

## 13. Not in 1.0 (explicit)

The following capabilities are **not** part of the 1.0 public API surface. Attempting to use them results in a compile error (the symbol does not exist) rather than a runtime `FeatureNotInTier`. Each is tracked by a separate milestone.

| Capability | Why not in 1.0 | Tracked by |
|---|---|---|
| HTML → PDF (`PdfDocument::from_html`, `HtmlToPdfOptions`) | Requires headless Chromium integration, out of scope for core facade | Milestone #57 / design story #1206 (IronPDF Parity) |
| OCR direct API on `PdfDocument` (`make_searchable`, `ocr_text`) | Available via `pdf-ocr` crate; facade wrapper deferred to 1.1 | Milestone #52 Epic 2 #1224 (partial); expanded post-1.0 |
| DOCX / XLSX / PPTX conversion facades | `to_docx` lands under #1224 in 1.0; XLSX and PPTX deferred | Milestone #52 Epic 2 #1224 |
| Async API for full surface | `pdfluent::r#async` ships as beta in 1.0 with limited methods | Milestone #52 Epic 5 #1235, promote to stable in 1.1 |
| AI-based document intelligence | Separate product line, not an SDK concern | Not scheduled |
| PDF forms → web rendering | Online product, not a library concern | Not scheduled |

These are documented here so a user who tries `PdfDocument::from_html` and gets `no such method` knows where to look.

---

## 14. Validation pass revision log

| Date | Revision | Changes |
|---|---|---|
| 2026-04-21 | v1.1 | Post-validation-pass update. Applied 12 fixes: removed `PdfDocumentBuilder`; removed `permissions_mut`; split `Signature` into `SignatureInfo` + `SignatureValidation`; renamed `structured_text` → `text_with_layout`; renamed `PadesProfile` variants to descriptive names; renamed `Rotation` variants to `ClockwiseN`; made `metadata_mut`/`form_mut` both return plain handles (no `Result`); `BookmarkMergeStrategy::Concat` as `Default`; removed unused `Alignment` type; `Error::docs_url` returns `&'static str`; `PdfDocument` dropped `Clone`; added license-provisioning API (`set_license_key`, `OpenOptions::with_license_key`, env var `PDFLUENT_LICENSE_KEY`). Full report: `xfa-program-office/SDK_CORE_FACADE_VALIDATION_PASS_01.md`. |


🤖 Drafted 2026-04-21 as part of milestone #52 execution (SDK Core Facade & API Ergonomics).
