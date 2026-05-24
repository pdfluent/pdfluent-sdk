# PDFluent Workspace Version Alignment — RC (no-publish)

- Milestone: `PDFLUENT_WORKSPACE_VERSION_ALIGNMENT_RC`
- Date: 2026-05-24 · Branch: `quality/pdfluent-workspace-version-alignment-rc` · Base: `enterprise/ga-hardening` @ `ef6f74c7f`
- **Verdict: `PDFLUENT_WORKSPACE_VERSION_ALIGNMENT_RC_PARTIAL_BLOCKERS_REMAIN`**
  - The dominant blocker — **first-party Rust RC-line version drift — is fully resolved** (crates.io train unblocked).
  - One **pre-existing structural Maven blocker** remains (see §7); per the mission it is *blocked with evidence, not force-aligned*.
- Scope honored: no publish · no `--allow-dirty` · no XFA behavior changes · no FreshMerge/parser changes · no binary rename · no secrets/private paths/artifacts · version manifests only (no source code).

## 1. What versions existed before (drift table)

| Group | Version (before) | Members |
|---|---|---|
| Workspace default `[workspace.package]` | **beta.3** | inherited by pdf-ocr, xfa-license (publishable) + pdf-invoice/pptx/xlsx/xfa-cli (unreleased) + 6 internal crates |
| Explicit RC crates | **beta.8** | 17 crates (formcalc-interpreter, pdf-compliance, pdf-docx, pdf-engine, pdf-manip, pdf-redact, pdf-render, pdf-text-format, pdf-xfa, pdfluent, pdfluent-extract, pdfluent-forms, pdfluent-sign, xfa-dom-resolver, xfa-js-sandboxed, xfa-json, xfa-layout-engine) |
| pdf-annot (explicit) | **beta.4** | pdf-annot |
| pdf-ocr, xfa-license (inherited) | **beta.3** | via workspace default |
| Stale internal dep pin | **beta.1** | `[workspace.dependencies] pdf-invoice = "=1.0.0-beta.1"` (didn't even match the crate's beta.3) |

⇒ First-party RC line spanned **3 versions (beta.3 / beta.4 / beta.8)**. A prior milestone bumped the 17 explicit crates to beta.8 but left the workspace default + pdf-annot + the inheritors + several `=` dependency pins behind.

## 2. Decision — unified target `1.0.0-beta.8`

Repo evidence (17 first-party crates + Java pom `bindings/java` + .NET csproj + `pyproject` `1.0.0b8`) already converges on **beta.8**, so beta.8 is the safe unified RC target. No evidence contradicted it.

## 3. What changed (all version-string only)

- `Cargo.toml [workspace.package] version`: **beta.3 → beta.8** (aligns pdf-ocr, xfa-license + inherited unreleased/internal crates), with an explanatory comment.
- `crates/pdf-annot/Cargo.toml version`: **beta.4 → beta.8**.
- `[workspace.dependencies]` `=` pins → beta.8: **xfa-license** (was beta.3), **pdf-annot** (was beta.4), **pdf-ocr** (was beta.3), **pdf-invoice** (was the stale beta.1).
- Per-crate intra-workspace `=` pins → beta.8 in: pdf-desktop, pdf-node, pdf-python, pdf-java, xfa-wasm (pdf-annot pin); pdf-manip, pdfluent ×2 (xfa-license pin); xfa-test-runner (pdf-ocr/pptx/xlsx/invoice/annot pins).
- `Cargo.lock`: regenerated (`cargo metadata --offline`); **26 lines = 13 crate versions × 2**, no dependency-graph churn.
- `docs/release/SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`: publish-line version annotations updated (xfa-license, pdf-annot, pdf-invoice, pdf-ocr, pdf-pptx, pdf-xlsx, xfa-cli → beta.8; pdf-font kept beta.3).

## 4. Packages intentionally NOT aligned (with reason)

| Package | Version | Why not moved |
|---|---|---|
| pdf-font | beta.3 | Independent versioning track (`INDEPENDENT_VERSION_CRATES`); its `=beta.3` dep pins were preserved |
| pdf-syntax 0.5.4, pdf-interpret 0.5.6 | own | hayro forks, open-source (MIT/Apache) semver |
| pdfluent-lopdf 0.39.2, -cff 0.2.0, -ccitt 0.2.2, -jbig2 0.2.3, -jpeg2000 0.3.3 | own | independently versioned vendored/forked crates |
| pdf-content-stream 0.1.0 | 0.1.0 | internal helper, `publish=false`, pre-1.0 |
| pdf-diff beta.1, xfa-pdfrest-compare 0.1.0 | own | internal `publish=false`, not on the RC train |
| **crates/pdf-java/pom.xml (Maven)** | beta.1 | **Structural blocker — not safely a version bump (see §7)** |

## 5. Are all first-party package versions now consistent?

**Rust crates.io RC line: YES.** The consistency checker reports:
`First-party RC-line version: 1.0.0-beta.8 (consistent across 20 crates)` — **no drift warning**.
Cross-language packaging manifests were already at beta.8 equivalents: `bindings/java/pom.xml` beta.8, `bindings/dotnet/.../PDFluent.csproj` beta.8, `pdf-python` `pyproject` `1.0.0b8`, WASM version injected at package-time. The **one exception is the Maven `crates/pdf-java/pom.xml`** (§7).

## 6. Gate results

| Gate | Result |
|---|---|
| `check_release_consistency.py --offline` | **PASS** — 0 failures, 9 warnings (pre-existing metadata only), no first-party drift |
| `cargo metadata --format-version 1 --offline` | **PASS** — full resolution, no version-requirement conflicts |
| `cargo check -p pdf-annot --offline` | **PASS** (builds at beta.8) |
| `cargo check -p pdfluent --offline` (umbrella, xfa-license pin) | **PASS** |
| `check_no_private_paths.sh` | **PASS** |
| Leak scan (changed files) | **clean** |
| No source/license-field changes | **confirmed** (manifests + lock + 1 doc only) |
| Branch & merge pipelines | run on push/merge — authoritative per-channel package/license/leak audits live in CI `package_manual` |

## 7. Remaining blocker — Maven channel (structural, not a version bump)

`crates/pdf-java/pom.xml` is `com.pdfluent:**xfa-pdf**` @ **beta.1**, **MIT** license — and it is what CI `package:maven-audit` and `scripts/publish_all.sh` actually build/deploy. Meanwhile `bindings/java/pom.xml` is the correct `com.pdfluent:**pdfluent**` @ **beta.8** under **LicenseRef-PDFluent-Commercial**, but is **not CI-wired**. Reconciling these is a *structural + licensing* decision (which pom is canonical, correct artifactId, MIT→commercial), not a safe version bump — so per the mission it is **blocked with evidence, not force-aligned**.

## 8. Does this unblock crates.io no-publish RC?

**Yes.** The crates.io first-party RC line is now a single coherent version (beta.8), resolves cleanly (`cargo metadata`), and builds. The version-drift blocker the systems map identified as dominant is removed.

## 9. What remains before actual publish

- **Maven**: reconcile the duplicate/stale pom (license + artifactId + which pom CI builds) — §7.
- **C-ABI** (`pdf-capi`): stale MIT license (flagged in prior reports) → commercial.
- **WASM**: SIMD-vs-scalar golden fidelity sign-off; the `docs/wasm-capability-matrix.md` header carries its own stale npm-track numbers (beta.11/beta.5/beta.4) — a separate doc cleanup (the npm package version is injected at package-time, not from the crate, so it does not affect crate drift).
- **Signing/notarization identity + SBOM** wiring (release-time + business decision).
- The actual publish itself (human-run, never automatic).

## 10. Categorized unresolved items

- **Business decisions:** Maven canonical pom + license; channel launch order; signing identities.
- **Future takeover work:** Maven pom reconciliation + CI rewire; WASM capability-matrix doc version cleanup; SBOM tooling.
- **Engineering blockers:** C-ABI license; WASM SIMD fidelity.

## 11. Recommended next milestone

**`PDFLUENT_MAVEN_CHANNEL_POM_RECONCILIATION`** — resolve the `crates/pdf-java` (beta.1, MIT, `xfa-pdf`) vs `bindings/java` (beta.8, commercial, `pdfluent`) duplication: pick the canonical pom, fix license + artifactId, and point CI `maven-audit` + `publish_all.sh` at it. No publish; package/license/leak audited.
