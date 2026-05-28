# PDFluent Release Systems — Final Readiness Report

- Milestone: `PDFLUENT_DISTRIBUTION_AND_RELEASE_SYSTEMS_MAP`
- Date: 2026-05-24 · Branch: `quality/pdfluent-distribution-systems-map` · Base: `origin/enterprise/ga-hardening` @ `47edfe4a7`
- Companion: `docs/reports/pdfluent_distribution_systems_map.md` (full per-channel detail)
- **Verdict: `PDFLUENT_RELEASE_SYSTEMS_MAPPED_AND_HARDENED`**
- Scope honored: **no publish · no binary rename · no XFA/FreshMerge/parser changes · no corpus access · no secrets/artifacts committed · no tests/gates weakened.**

## 1. Channel readiness (evidence-based)

| Channel | Verdict | Evidence | Blocker before real publish |
|---|---|---|---|
| **CLI** `pdfluent-cli` | **No-publish RC ready** | `pdfluent_cli_package.sh` + `pdfluent_cli_release_gate.sh` pass; 30 CLI tests green; checksums + `release_manifest.json`; `publish=false` | Shared version-alignment decision; not yet a channel in the unified runner; clap-name↔binary mismatch (future takeover) |
| **crates.io** (~25 crates) | **Partial — blocked** | `crates_topo_dry_run.sh`, `release_train_guard.sh`, `prepublish_crate_audit.sh`, `package:crates-dry-run` | **First-party version drift (beta.3/4/8)** + 4 planned-unpublished crates |
| **WASM npm** | **Partial** | `wasm_dry_run.sh`, `transform-wasm-pkg.sh`, `package:wasm-tarball-dry-run`, simd128 on | SIMD-vs-scalar golden fidelity (prior milestone) |
| **PyPI** `pdf-python` | **Partial** | maturin build, `scrub-wheel.py`, `audit_package_tree.py`, `package:python-wheel-audit` | Version alignment; signing/provenance |
| **NuGet** .NET | **Partial** | `stage_dotnet_natives.sh`, `package:nuget-audit` | Native matrix completeness; signing |
| **Maven** `pdf-java` | **Partial** | `mvn package`, `package:maven-audit` | Version alignment; GPG signing for Central |
| **C-ABI** `pdf-capi` | **Partial — not publish-ready** | `package_cabi.sh`, audit-all `cabi` channel | **Stale MIT license** (flagged in `INTERNAL_CRATES`) must be reconciled to commercial license |
| **Node** `pdf-node` | **Early** | manifest only | No release tooling |
| **Desktop** `pdf-desktop` | **Early** | manifest only, `publish=false` | No release tooling |
| **GitLab pkg registry** | **N/A** | not configured | — |

**Headline:** the release *machinery* (per-channel dry-run/audit jobs, unified `audit-all-packages.sh`, crates.io train guard, leak/path scanners, fail-closed gates, manual-only publish stages) is materially mature. The dominant gate to a coherent multi-crate release is **first-party version drift**, not missing tooling.

## 2. What was improved this milestone

1. **Restored a fully-broken cross-channel gate.** `scripts/check_release_consistency.py` crashed at import on Python 3.9 (the CI runner + macOS system interpreter) — PEP-604 `X | None` annotations evaluated eagerly. A release gate that crashes is false safety. Fixed with `from __future__ import annotations`. Now runs end-to-end.
2. **Added `--offline` mode** — runs local invariants (version consistency + metadata) with no crates.io network, so it is usable as a fast local/CI gate.
3. **Added first-party version-drift detection** — collects RC-line (`1.0.0-beta.N`) first-party versions, reports the spread, and emits a WARNING (rc stays 0 — alignment is a deliberate release decision, not an automatic failure). Current output: **3 versions** — beta.8 (17 crates), beta.4 (pdf-annot), beta.3 (pdf-ocr, xfa-license + workspace-inherited).
4. **Produced the grounded systems map** (`pdfluent_distribution_systems_map.md`) from repo evidence only.

Pipeline-safe: the consistency checker is **not wired into `.gitlab-ci.yml`**, so these changes cannot alter pipeline results. Validated locally: `python3 -c ast.parse` OK; `--offline` → `RESULT: 0 failures, 10 warning(s) — PASS`; `check_no_private_paths.sh` OK; leak scan on new docs clean.

## 3. What remains before any real publish (engineering)

- **Align RC versions** across the first-party train (mechanical but touches many manifests; overlaps the prior `WORKSPACE_VERSION_RECONCILIATION` territory — intentionally NOT executed here).
- **C-ABI license** reconciliation (MIT → commercial) before `pdf-capi` is publishable.
- **WASM SIMD-vs-scalar golden fidelity** sign-off.
- **Signing/notarization identity + SBOM tooling** wired into the release stage (currently absent).
- **Wire CLI as a channel** in `audit-all-packages.sh` (today it has its own gate but is not in the unified runner).

## 4. Decisions needing Jasper / business input

- **Version-alignment target & cadence:** snap everything to `beta.8`, or set a fresh unified RC? Who owns the bump?
- **Channel launch order:** which surface publishes first (crates.io vs CLI vs WASM)?
- **Code-signing identity:** Apple notarization, Windows Authenticode, Maven Central GPG — org-level credentials & ownership.
- **CLI binary takeover:** resolve clap program-name (`pdfluent`) ↔ binary (`pdfluent-cli`); the internal `xfa-cli`→`pdfluent` binary is consumed by ~10+ scripts, so any rename is a coordinated migration.

## 5. Unresolved items — categorized

- **Business decisions:** version target/cadence; launch order; signing identities; CLI naming/takeover.
- **Future takeover work:** unify CLI into the package runner; Node/Desktop release tooling; SBOM generation; per-channel signing implementation.
- **Real engineering blockers:** version drift alignment; C-ABI license; WASM SIMD fidelity.

## 6. Recommended next milestone

**`PDFLUENT_WORKSPACE_VERSION_ALIGNMENT_RC`** — execute the deliberate first-party version snap to a single RC (decision-gated by §4), validated by the now-working `check_release_consistency.py --offline`. This unblocks crates.io and removes the single dominant release-train blocker. (No publish; pure manifest alignment + gate verification.)
