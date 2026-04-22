# Migrating to PDFluent 1.0

**Target audience:** anyone on a `0.x` `pdfluent` snapshot, or anyone
coming from the legacy `pdf-engine::api::*` facade before the
`pdfluent` crate existed.

**TL;DR:** the 1.0 surface is frozen per [RFC 0001](docs/rfc/0001-sdk-core-api.md);
the stability rules are in [STABILITY.md](STABILITY.md); this document
tells you what moved where since the last pre-release.

---

## 0. Decide if you need to migrate

| Coming from | Migration effort |
|---|---|
| `1.0.0-alpha.*` | Low — a handful of renames (§2). |
| `1.0.0-beta.*` | Trivial — no API renames, only behaviour clarifications (§3). |
| `pdf-engine::api::*` direct usage | Medium — swap to `pdfluent::prelude::*` and update import paths (§1). |

---

## 1. From `pdf-engine::api::*` → `pdfluent::prelude::*`

### Dependency

```diff
 [dependencies]
-pdf-engine = "0.x"
+pdfluent = "1"
```

### Imports

```diff
-use pdf_engine::api;
+use pdfluent::prelude::*;
```

### Entry points

| Old | New |
|---|---|
| `pdf_engine::api::read(path)` | `pdfluent::PdfDocument::open(path)` |
| `pdf_engine::api::Document` | `pdfluent::PdfDocument` |
| `pdf_engine::api::Error` | `pdfluent::Error` |
| `doc.form_fields()` (returned `Vec`) | `doc.form_fields()?` (returns `Result<Vec<FormField>>`) |
| `doc.fill_form(&[("a", "1")])` | `doc.form_mut().set_text("a", "1")?` |
| *(no save)* | `doc.save(path)?` / `doc.to_bytes()?` |

### Error type

The old `api::Error` had variants; the new `pdfluent::Error` is
`#[non_exhaustive]` and every variant carries a stable `code()` and a
`docs_url()`:

```rust
if err.code() == "E-SECURITY-DECRYPTION-FAILED" { /* ... */ }
```

See [STABILITY.md §8](STABILITY.md#8-error-code-contract) for the
frozen codes list.

---

## 2. From `1.0.0-alpha.*` → `1.0.0`

These renames happened during the RFC v1.1 / v1.2 / v1.3 freeze.
If your snapshot is older than the listed change, apply the rename.

### 2.1 RFC v1.1 (alpha.3-ish)

| Change | Old | New |
|---|---|---|
| Builder removed | `PdfDocumentBuilder::new()` | `PdfDocument::create()` |
| Permission accessor removed | `doc.permissions_mut()` | Pass `Permissions` to `EncryptOptions`; the builder is on `Permissions::with_*()`. |
| Signature type split | `Signature { valid }` | `SignatureInfo` (metadata) + `SignatureValidationReport` (validation). |
| Structured text renamed | `doc.structured_text()` | `doc.text_with_layout()` |
| PAdES variants | `PadesProfile::B`, `T`, `LT`, `LTA` | `Basic`, `Timestamped`, `LongTerm`, `LongTermArchive` |
| Rotation variants | `Rotation::D90` | `Rotation::Clockwise90` (and friends) |
| Result-less `_mut` | `doc.form_mut()?` | `doc.form_mut()` (no `Result`) |
| Alignment type | `Alignment::*` | Removed (unused). |
| `docs_url` return type | `String` | `&'static str` |
| Clone on PdfDocument | `doc.clone()` | Removed. Use `PdfDocument::from_bytes(&doc.to_bytes()?)`. |
| License plumbing added | n/a | `OpenOptions::with_license_key`, `set_license_key`, env `PDFLUENT_LICENSE_KEY`. |

### 2.2 RFC v1.2

| Change | Old | New |
|---|---|---|
| form_fields infallible | `-> Vec<FormField>` | `-> Result<Vec<FormField>>` (regression fix). |
| Async feature | `pdfluent::r#async` module / `async-tokio` feature | Removed for 1.0. Returns in 1.1. |
| Permissions builders added | n/a | `Permissions::with_print_only()`, `with_fill_forms_only()`, `full_access()`. |
| SignOptions visible_rect type | `visible_rect(page: u32, ...)` | `visible_rect(page: usize, ...)` |
| RedactOptions wired | n/a | `doc.redact(text, RedactOptions::new().on_pages(&[1, 2]))?` |

### 2.3 RFC v1.3

| Change | Old | New |
|---|---|---|
| `pages_mut` removed | `doc.pages_mut()` | Direct methods: `rotate_page`, `split_pages`, `extract_pages`. |
| `MetadataMut::commit` | `MetadataMut::commit(self)` | `MetadataMut::commit(&mut self)` — chain pattern now compiles. |
| Permissions defaults | `aes256()` → no-permissions | `aes256()` / `aes128()` default to `Permissions::full_access()`. |
| Encryption presets | n/a | Every preset honours ISO 32000-2 §7.6.4.2 accessibility-extraction. |
| Signature reporting | `all_valid()` on unsigned docs panicked | Returns vacuous-true; pair with `is_signed()`. |
| Trial capability scope | Trial = all technical caps | Trial still marks output; `AirGapped` / `OemRedistribution` remain Enterprise-only. |

---

## 3. From `1.0.0-beta.*` → `1.0.0`

No API renames. Three behaviour clarifications:

### 3.1 `flatten_forms()` panic

In beta this method called `unimplemented!()` and aborted the
process. At 1.0 GA it returns `Error::MissingDependency` — callers
that already check the result see no observable change; callers that
ignored it get a typed error instead of an abort. See
[STABILITY.md §3.3](STABILITY.md#33-deferred-truth-gaps-in-10).

### 3.2 `linearize()` / `embed_font()`

Both return `Error::MissingDependency`. No panic.
`SaveOptions::with_linearize(true)` is still accepted; linearisation
lands in 1.1.

### 3.3 Form Unicode

`PdfFormMut::set_text` / `set_dropdown` now route through
`lopdf::text_string` which picks PDFDocEncoding for ASCII and
UTF-16BE with BOM otherwise. Non-ASCII values round-trip correctly;
previously they were corrupted by PDFDocEncoding. No API change —
just a correctness fix.

---

## 4. Tooling migration

### 4.1 Cargo features

If your `[features]` block referenced any of these, rename:

| Old | New |
|---|---|
| `async` | *(removed — see §2.2)* |
| `pdf-forms` | *(unchanged — internal crate)* |

### 4.2 Observability

1.0 adds a `tracing` feature flag (off by default). See
[OBSERVABILITY.md](OBSERVABILITY.md).

### 4.3 WASM

`wasm32-unknown-unknown` is supported for every method that doesn't
depend on native-only backends. `to_docx` and `to_images` return
`Error::UnsupportedOnWasm` on wasm32. Full matrix:
[WASM_SUPPORT.md](WASM_SUPPORT.md).

---

## 5. Bindings

The `pdf-capi` / `pdf-java` / `pdf-node` / `pdf-python` bindings
mirror the Rust facade. Per-binding migration notes live in each
binding crate's README; they are **not** covered by this document.

---

## 6. Breaking-change guarantees for 1.x

From 1.0 onwards, every change in this table is forbidden without a
2.0 MAJOR bump:

- Removing a Stable item from `pdfluent::*` or the prelude.
- Changing an `Error::code()` string.
- Changing a `docs_url()` path prefix.
- Changing an existing `#[non_exhaustive]` enum variant's shape.
- Changing a default feature flag setting.
- Dropping a supported target triple.

See [STABILITY.md §2](STABILITY.md#2-semver-policy) for the full
policy.

---

## 7. Reporting migration blockers

If you hit a case this document doesn't cover:

1. File a GitHub issue with the `migration` label.
2. Include the symbol/path you're migrating from, the Rust
   compiler error (if any), and the pre-1.0 version tag you started
   from.
3. For truth-gap questions (“is this method real or deferred?”),
   [STABILITY.md §3.3](STABILITY.md#33-deferred-truth-gaps-in-10)
   is authoritative.
