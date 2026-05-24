# PDFluent Distribution & Release Systems Map

- Date: 2026-05-24 · Base: `origin/enterprise/ga-hardening` @ `47edfe4a7`
- Source of truth: the repository (manifests, scripts, `.gitlab-ci.yml`, prior reports). No assumptions.

## Version source of truth & the central blocker
- `Cargo.toml [workspace.package] version = 1.0.0-beta.3`.
- The RC line is in practice **beta.8** (17 first-party crates), but **pdf-annot=beta.4**,
  **pdf-ocr=beta.3**, **xfa-license=beta.3**, and workspace-inherited crates = beta.3.
- ⇒ **First-party version drift across 3 versions (beta.3 / beta.4 / beta.8)** — a coherent
  release train cannot proceed until aligned. (Vendored hayro forks + pdf-syntax/-interpret/-font/
  -extract/cff/lopdf/ccitt/jbig2/jpeg2000 are *intentionally* independently versioned.)

## Distribution surfaces (per-channel status)
| Channel | Surface | Build/audit tooling | CI job (manual) | Status |
|---|---|---|---|---|
| **crates.io** (Rust) | ~25 first-party crates | `crates_topo_dry_run.sh`, `release_train_guard.sh`, `prepublish_crate_audit.sh` | `package:crates-dry-run` | **PARTIAL** — tooling mature; **blocked by version drift** + 4 planned-unpublished crates |
| **CLI** (`pdfluent-cli`) | binary | `pdfluent_cli_package.sh` + `pdfluent_cli_release_gate.sh` (30 tests) | (crate dry-run) | **READY (no-publish RC)** — packaging+gate+checksums+manifest+tests all green; not yet in the unified runner |
| **WASM npm** (`@pdfluent/sdk-wasm`, `xfa-wasm`) | wasm-pack web pkg | `wasm_dry_run.sh`, `transform-wasm-pkg.sh`, `+simd128` config | `package:wasm-tarball-dry-run` | **PARTIAL** — dry-run + simd128 enabled; **SIMD-vs-scalar golden fidelity pending** (prior milestone) |
| **PyPI** (`pdf-python`) | maturin wheel | `maturin build`, `audit_package_tree.py`, `scrub-wheel.py` | `package:python-wheel-audit` | **PARTIAL** — wheel build + scrub + audit wired |
| **NuGet** (.NET, `bindings/dotnet`) | nupkg | `stage_dotnet_natives.sh`, `audit_package_tree.py` | `package:nuget-audit` | **PARTIAL** — native bundling prepared (prior milestone) |
| **Maven** (`pdf-java`) | jar (`pom.xml`) | `mvn package`, `audit_package_tree.py` | `package:maven-audit` | **PARTIAL** — pom profile fixed (prior milestone) |
| **C-ABI** (`pdf-capi`) | `pdfluent-capi-*.tar.gz` | `package_cabi.sh`, `cabi_packaging.md`, audit-all `cabi` channel | (via audit-all) | **PARTIAL** — packaging exists; **stale MIT license noted** (not publish-ready per INTERNAL_CRATES comment) |
| **Node** (`pdf-node`) | `package.json` | — | — | **EARLY** — manifest only; no release tooling |
| **Desktop/editor** (`pdf-desktop`) | `package.json` | — | — | **EARLY** — manifest only; publish=false |
| **GitLab package registry** | — | not configured | — | **N/A** — not used |

## Release orchestration (exists)
- **Protocol/runbooks:** `docs/release/PUBLISH_PROTOCOL.md`, `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`,
  `release_gate_contract.md`, `docs/release/checklists/`.
- **Unified gate runner:** `scripts/release/audit-all-packages.sh` (channels: rust, python, wasm,
  dotnet, java, cabi — **CLI not yet a channel**). P0 findings block; manual stage; never auto-publish.
- **crates.io train:** `release_train_guard.sh` (audit-first, `--execute`/`--dry-run-publish`).
- **Cross-channel version/metadata gate:** `scripts/check_release_consistency.py` —
  **was crashing on Python 3.9** (PEP-604 annotations); **fixed this milestone** + added `--offline`
  (local gate, no network) + **first-party drift detection**.
- **Artifact hygiene / leak control:** `check_no_private_paths.sh`, `audit_package_tree.py`,
  `check_release_artifact_contents.py`, `scrub-wheel.py`, remap-path-prefix in `.cargo/config.toml`.
- **CI policy:** all packaging in `package_manual`, all publish in `release_manual` — **never automatic**.
- **Rollback:** no-publish ⇒ discard staged artifacts; crates.io is yank-only (irreversible version),
  reinforcing audit-first.

## CLI in context
`pdfluent-cli` (public, `publish=false`, binary `pdfluent-cli`) is the most release-ready channel
(own packaging+gate+30 tests). It is **not blocked** by the broader package strategy except the
shared version-alignment decision. Coexists with the internal `xfa-cli` binary `pdfluent` (no
collision); program-name↔binary mismatch + takeover documented as future work.

## Signing / SBOM / reproducibility
- Reproducible-ish: remap-path-prefix everywhere; CLI uses `SOURCE_DATE_EPOCH`.
- **No code signing / notarization identity; no vendored SBOM tool** — release-time + business decisions.
