# PDFluent SDK — Quality / Reliability GA Model

Defines what **Quality 100%** means for the non-XFA SDK. "Quality 100%" =
every public entrypoint is safe on adversarial input, errors are
deterministic and typed, memory is sound across FFI, and there is a
regression corpus that gates future changes — all proven, not asserted.

Status vocabulary: see [SDK_GA_READINESS_TAXONOMY.md](SDK_GA_READINESS_TAXONOMY.md).

---

## QR-1 · No-panic public entrypoints

- **Current evidence:** `crates/pdfluent/tests/core_pdf_quality.rs` (catch_unwind no-panic guards on empty/garbage/truncated/header-only/bogus-xref); `security.rs`; `processing_limits.rs`.
- **Missing proof:** Systematic coverage that **every** public facade method (not just open/parse) is panic-guarded; binding-level no-panic (a Rust panic across FFI is UB unless caught).
- **Candidate tests:** Per-public-method hostile-input matrix; FFI catch_unwind boundary test per C-ABI/Node/Python/WASM export.
- **Acceptance criteria:** No public entrypoint panics on any corpus/fuzz input; FFI boundaries convert panic→typed error.
- **Release-blocking:** YES.

## QR-2 · Malformed / corrupt PDF handling

- **Current evidence:** `core_pdf_quality.rs` typed-error-not-panic on malformed classes; fixture manifest.
- **Missing proof:** Breadth — a labelled malformed corpus with expected typed outcomes; coverage of partially-valid (recoverable) vs. fatal.
- **Candidate tests:** Malformed corpus runner asserting `Error` variant per fixture.
- **Acceptance criteria:** Every malformed class maps to a documented typed error; no hang, no panic, bounded time.
- **Release-blocking:** YES.

## QR-3 · Encrypted PDFs

- **Current evidence:** `security.rs` (`decrypt_with_wrong_password_returns_decryption_failed`); `core_pdf_quality.rs` encrypted-fixture typed behaviour; facade `encrypt`/`decrypt` + AES-128/256.
- **Missing proof:** Coverage across RC4/AESV2/AESV3, owner-vs-user password, permissions enforcement; empty/garbage password edge cases per binding.
- **Candidate tests:** Encryption matrix fixture set; per-binding decrypt smoke.
- **Acceptance criteria:** All standard handlers open/refuse correctly with typed errors; permissions honored.
- **Release-blocking:** YES.

## QR-4 · Object streams / xref streams

- **Current evidence:** `core_pdf_quality.rs` objstream/xrefstream load + roundtrip page-count; `crates/lopdf/src/object_stream.rs`, `crates/pdf-syntax/src/xref.rs`.
- **Missing proof:** Hybrid-reference files, broken/cyclic xref-stream recovery breadth.
- **Candidate tests:** Hybrid + corrupt-xref-stream fixtures with expected outcomes.
- **Acceptance criteria:** ObjStm/XRef-stream parse + roundtrip preserve structure; corrupt variants → typed error.
- **Release-blocking:** YES.

## QR-5 · Resource limits

- **Current evidence:** `processing_limits.rs` (file size, object depth, operator count, image pixels, stream size → typed `ResourceLimitExceeded`).
- **Missing proof:** Per-binding exposure of limit config; default-limit documentation; limits applied on the render/extract paths too.
- **Candidate tests:** Per-binding limit-config smoke; render-path limit test.
- **Acceptance criteria:** Documented default limits; configurable per surface; enforced on all heavy paths.
- **Release-blocking:** YES.

## QR-6 · Hostile input / decompression bombs

- **Current evidence:** `processing_limits.rs` stream-size + image-pixel caps; `CORE_PDF_SECURITY_HARDENING_REPORT.md`.
- **Missing proof:** Explicit decompression-ratio (zip-bomb) guard test; nested-filter bomb; recursive object reference bomb.
- **Candidate tests:** Decompression-bomb fixtures (Flate ratio, nested filters), recursion-depth bomb.
- **Acceptance criteria:** Bounded memory + time on all bomb classes; typed limit error.
- **Release-blocking:** YES.

## QR-7 · Deterministic typed errors

- **Current evidence:** `error_codes_stable.rs` (13 tests); `Error` enum (18 variants + ResourceLimitKind); `docs/error_catalogue.md`; `error_catalogue_sync.sh`.
- **Missing proof:** Determinism across repeated runs and across platforms (same input → same code); stability across the release.
- **Candidate tests:** Repeat-run determinism test; cross-platform code-stability check in CI.
- **Acceptance criteria:** Stable codes, catalogue in sync, no nondeterministic error text in codes.
- **Release-blocking:** YES.

## QR-8 · Thread / concurrency safety

- **Current evidence:** None identified specific to concurrency.
- **Missing proof:** `Send`/`Sync` guarantees of public types; parallel open/extract/render safety; documented threading model.
- **Candidate tests:** Multi-thread stress (N threads × open/extract/render distinct + shared docs); `Send`/`Sync` static assertions.
- **Acceptance criteria:** Documented threading model; no data races (loom/ASAN where applicable); static Send/Sync assertions.
- **Release-blocking:** YES (enterprise concurrency expectation).

## QR-9 · Memory-safety at FFI boundaries

- **Current evidence:** C ABI strict-api build (`-Wall -Wextra -Werror`) in capability matrix; binding smokes.
- **Missing proof:** No ASAN/Valgrind/LeakSanitizer run over C-ABI/Node/Python; no double-free/use-after-free/leak proof; ownership/lifetime contract doc.
- **Candidate tests:** ASAN+LSAN over C-ABI test harness; Miri over unsafe Rust in `pdf-capi`; alloc/free balance test.
- **Acceptance criteria:** Zero leaks/UB under sanitizers on the FFI surface; documented ownership contract.
- **Release-blocking:** YES (this is a core enterprise-grade assurance).

## QR-10 · WASM runtime safety

- **Current evidence:** Editor WASM Node runtime proof (`EDITOR_HANDOFF_SDK_RUNTIME_PROOF.md`); wasm strict-ts-edit.
- **Missing proof:** OOM/trap behaviour on hostile input in-browser; memory growth bounds; panic→JS-exception mapping.
- **Candidate tests:** WASM hostile-input harness (Node + headless browser) asserting trap→catchable error, bounded memory.
- **Acceptance criteria:** No uncatchable traps on hostile input; documented memory model; panics surface as JS exceptions.
- **Release-blocking:** YES.

## QR-11 · Binding exception mapping

- **Current evidence:** Binding API parity green; per-binding strict builds.
- **Missing proof:** That each binding maps Rust `Error` variants → idiomatic exceptions with the **same stable codes** (Node throws, Python raises, .NET/Java exceptions, C-ABI status codes).
- **Candidate tests:** Per-binding error-mapping test: trigger each error class, assert mapped type + code.
- **Acceptance criteria:** 1:1 documented mapping; codes stable; no swallowed/silent errors.
- **Release-blocking:** YES.

## QR-12 · Structural PDF correctness

- **Current evidence:** roundtrip page-count preservation (`core_pdf_quality.rs`, `merge.rs`); PDF/A validate (`validate_pdfa`).
- **Missing proof:** Output validity against an external validator (e.g. veraPDF/qpdf `--check`) for save/merge/split/flatten outputs.
- **Candidate tests:** Output-validity gate: run qpdf/veraPDF on representative outputs; assert no structural errors.
- **Acceptance criteria:** Outputs pass an independent structural validator.
- **Release-blocking:** YES (correctness claim).

## QR-13 · Privacy / no network / no telemetry

- **Current evidence:** Project rule (no telemetry); no network in core paths (asserted, not proven).
- **Missing proof:** A test that asserts **zero network egress** during open/parse/save/extract/render across bindings; dependency audit for phone-home.
- **Candidate tests:** Sandbox/no-network test harness; static scan for socket/http usage in runtime crates.
- **Acceptance criteria:** Provably no network/telemetry on any non-license path; license path documented + offline-capable.
- **Release-blocking:** YES (enterprise/gov requirement; also claim-blocking).

## QR-14 · Regression corpus

- **Current evidence:** `CORE_PDF_FIXTURE_MANIFEST`; VPS corpus (216 GB, ~1M PDFs) referenced for XFA; corpus-mini in-repo.
- **Missing proof:** A **non-XFA** regression corpus wired to a gate (open/extract/render/save over a labelled set with expected outcomes + perf budget), runnable in CI.
- **Candidate tests:** Curated non-XFA corpus subset + golden-outcome runner as a CI gate.
- **Acceptance criteria:** Corpus gate runs in CI; regressions fail the build; corpus documented.
- **Release-blocking:** YES (this is the durability mechanism for all other QR rows).

## QR-15 · Security-sensitive workflows

- **Current evidence:** `CORE_PDF_SECURITY_HARDENING_REPORT.md`; redaction crate (`pdf-redact`); encryption.
- **Missing proof:** Redaction *content-removal* proof (text truly removed, not just covered); signature verify correctness matrix; sanitization of JS/launch actions on flatten (some evidence in pdf-forms).
- **Candidate tests:** Redaction extraction-after-redact test; signature verify matrix; active-content sanitization test.
- **Acceptance criteria:** Redaction removes underlying content; signatures verified correctly; dangerous actions stripped where claimed.
- **Release-blocking:** YES for any advertised security feature.

---

## Quality axis summary

Core-SDK safety primitives (no-panic, typed errors, resource limits,
malformed handling) are the **strongest** area with real tests. The
**release-blocking gaps** cluster in: FFI memory-safety under sanitizers
(QR-9), cross-binding exception mapping (QR-11), concurrency model (QR-8),
no-network proof (QR-13), independent structural validation (QR-12), and a
CI-wired non-XFA regression corpus (QR-14). These are the substance of the
Quality/Reliability milestone (B).
