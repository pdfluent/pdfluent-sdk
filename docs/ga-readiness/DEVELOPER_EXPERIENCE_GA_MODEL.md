# PDFluent SDK — Developer Experience GA Model

Defines what **DX 100%** means for the non-XFA SDK. "DX 100%" = a
developer in any supported language can install, run a first example, and
recover from errors within minutes, using docs that are provably accurate
and CI-enforced against drift.

Status vocabulary: see [SDK_GA_READINESS_TAXONOMY.md](SDK_GA_READINESS_TAXONOMY.md).
Evidence cited here is from the current `enterprise/ga-hardening` tree;
anything not directly verified in this audit is labelled `missing_evidence`
or `partial_evidence` — not green.

---

## DX-1 · 5-minute quickstart per language

- **Current evidence:** `pdfluent-examples/{rust,c,wasm,node,python,dotnet,java}` exist and are exercised by the Golden Path revalidation (`benchmarks/runs/ga_100_closure_v3/cookbook_examples/GOLDEN_PATH_REVALIDATION_REPORT.md`, 7/7).
- **Likely gaps:** No single "5-minute quickstart" page per language proven to take a cold developer from zero→first output; golden path proves *example compiles/runs*, not *time-to-first-success* or doc-page existence.
- **Acceptance criteria:** One quickstart page per language; each ends in a verified output; each step is copy-pasteable; total ≤ 5 min on a clean machine.
- **Required gates:** docs example checker (`scripts/docs/check_examples_and_snippets.py`) covering quickstart snippets; per-language first-run smoke (DX-11).
- **Release-blocking:** YES (claim-blocking for "easy to adopt").

## DX-2 · Install instructions per package/channel

- **Current evidence:** `scripts/release/audit-all-packages.sh --dry-run` PASS across 6 channels; channels report `NO_ARTIFACT` when not built.
- **Likely gaps:** Install docs not proven against *published* packages (nothing is published yet — correct for pre-GA). crates.io/npm/PyPI/NuGet/Maven install lines are unverified end-to-end.
- **Acceptance criteria:** Each channel has an install command that resolves the package name used in audit; package names consistent with taxonomy (no stale npm names — see GitLab-migration claim risk).
- **Required gates:** audit-all-packages dry-run + package-name consistency check; at GA, install-from-registry smoke.
- **Release-blocking:** YES (`green_but_needs_release_recheck` until published).

## DX-3 · Copy-paste examples that compile/run in CI

- **Current evidence:** Golden Path 7/7; cookbook drift matrix (`COOKBOOK_EXAMPLES_DRIFT_MATRIX.json`).
- **Likely gaps:** Coverage breadth — which public methods are exemplified vs. only smoke'd; whether *every* docs snippet is CI-compiled (vs. a curated subset).
- **Acceptance criteria:** Every fenced code block in quickstart + cookbook is compiled/run in CI or explicitly marked `no-run` with reason.
- **Required gates:** `scripts/docs/check_examples_and_snippets.py` (must cover all snippets, not a subset).
- **Release-blocking:** YES.

## DX-4 · API consistency across bindings

- **Current evidence:** `scripts/bindings/check_binding_api_parity.py` → `BINDING_API_PARITY_TRUE_100_PERCENT_GREEN` (147 total, 128 supported, 19 intentionally_unsupported).
- **Likely gaps:** Parity proves *presence/intent*, not *naming/ergonomic consistency* (camelCase vs snake_case conventions per language, argument order, return shapes).
- **Acceptance criteria:** Documented naming convention per language; parity matrix + a consistency lint that the 19 `intentionally_unsupported` are documented per binding.
- **Required gates:** binding API parity checker (green now); add convention doc.
- **Release-blocking:** NO for binary; YES as claim-blocking for "consistent API".

## DX-5 · Error messages and typed codes

- **Current evidence:** `crates/pdfluent/tests/error_codes_stable.rs` (13 tests); `Error` enum (18 variants + ResourceLimitKind); `CORE_PDF_ERROR_DX_REPORT.md`; `docs/error_catalogue.md`.
- **Likely gaps:** Whether every binding surfaces the *same* stable codes (mapping proof is a Quality category, QR-11); message quality/actionability not audited.
- **Acceptance criteria:** Stable code per error; catalogue complete and drift-checked; each binding maps to the same codes.
- **Required gates:** `error_codes_stable.rs`; `scripts/release/error_catalogue_sync.sh`.
- **Release-blocking:** YES.

## DX-6 · License activation / deactivation / status UX

- **Current evidence:** `scripts/bindings/check_license_e2e_parity.py` → `COMMERCIAL_LICENSE_E2E_TRUE_100_PERCENT_GREEN` (140 total, 124 supported).
- **Likely gaps:** UX flow docs (how a developer activates/deactivates/checks status per language) vs. the E2E parity which proves API presence.
- **Acceptance criteria:** Documented activate/deactivate/status flow per binding with a runnable example; offline/expired/invalid-key messaging documented.
- **Required gates:** license E2E parity (green now) + a license UX doc snippet in the example checker.
- **Release-blocking:** YES (commercial product core).

## DX-7 · Troubleshooting guide

- **Current evidence:** `docs/error_catalogue.md`; scattered fase decision docs.
- **Likely gaps:** No consolidated troubleshooting guide (common failures: missing native lib, wrong feature flags, encrypted-PDF password, WASM init, license errors).
- **Acceptance criteria:** One troubleshooting page mapping symptom → cause → fix, cross-linked to error codes.
- **Required gates:** docs presence check; links resolve.
- **Release-blocking:** NO (post-GA acceptable) but **strongly recommended** for enterprise-ready.

## DX-8 · API reference completeness

- **Current evidence:** `docs/en/api-reference.md`; rustdoc on facade; RFC 0001.
- **Likely gaps:** Whether every public item has docs (rustdoc `missing_docs` not proven enforced); per-binding reference completeness.
- **Acceptance criteria:** `#![deny(missing_docs)]` (or equivalent) on public crates; generated reference per binding.
- **Required gates:** `cargo doc` with missing-docs lint; per-binding doc presence.
- **Release-blocking:** NO for binary; YES claim-blocking for "documented SDK".

## DX-9 · Feature matrix accuracy

- **Current evidence:** `CORE_PDF_SDK_CAPABILITY_MATRIX.{json,md}` (27 supported, 2 split, checker-enforced); binding smoke matrix.
- **Likely gaps:** Public-facing feature matrix (website/README) accuracy vs. the internal capability matrix; XFA naming/claims overlap.
- **Acceptance criteria:** Public feature matrix generated from / cross-checked against the internal capability matrix; no claim without a matrix-backed row.
- **Required gates:** `check_core_pdf_sdk_quality.py`; a public-matrix ↔ internal-matrix consistency check.
- **Release-blocking:** YES (claim-blocking).

## DX-10 · Migration / release notes

- **Current evidence:** fase-*-decisions docs; per-milestone closure reports.
- **Likely gaps:** No consolidated CHANGELOG / migration notes for the v1 line (beta.N → GA), no documented breaking-change policy.
- **Acceptance criteria:** CHANGELOG with semver policy; migration notes for any pre-GA API moves.
- **Required gates:** changelog presence + version policy doc.
- **Release-blocking:** NO for binary; YES at GA cut.

## DX-11 · First-run smoke per binding

- **Current evidence:** capability matrix `bindings_non_xfa_smoke` (rust/c/wasm/node/python/dotnet/java each GREEN at build/compile level); editor runtime proof for WASM.
- **Likely gaps:** "First-run" = a brand-new consumer project that depends on the package and runs one call. Current smokes compile in-repo examples, not consume-as-dependency (except WASM Node runtime proof).
- **Acceptance criteria:** Per binding, a consumer-style smoke (depend on artifact, run one op, assert output).
- **Required gates:** per-binding consumer smoke in CI.
- **Release-blocking:** YES (`green_but_needs_release_recheck`).

## DX-12 · Docs drift checker

- **Current evidence:** `scripts/docs/check_examples_and_snippets.py` (docs/examples drift OK); cookbook drift matrix.
- **Likely gaps:** Scope of the checker (snippets only? all pages? feature matrix? install lines?).
- **Acceptance criteria:** Drift checker covers snippets + feature matrix + install commands + error catalogue; runs in CI; fails on drift.
- **Required gates:** the checker itself, expanded to the above scope.
- **Release-blocking:** YES (it is the enforcement mechanism for most other DX rows).

---

## DX axis summary

Release-blocking DX categories: DX-1, DX-2, DX-3, DX-5, DX-6, DX-9, DX-11, DX-12 (and DX-4/DX-8/DX-10 as claim-blocking at GA cut). Most have *partial* evidence today (compile/parity-level), but lack **consumer-perspective** and **doc-accuracy-at-scale** proof. The DX milestone (A) closes the gap between "examples compile in-repo" and "a new developer succeeds from published packages with accurate docs".
