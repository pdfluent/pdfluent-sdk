# PDFluent SBOM Protocol — CycloneDX

**Status:** mandatory, applies to every publish to every channel.
**Owner:** release captain.
**Standard:** [CycloneDX](https://cyclonedx.org/) v1.5 JSON (OWASP).
**Tool:** `cargo-cyclonedx` (pinned, see version below).

---

## 1. Why
Every PDFluent publish ships a Software Bill of Materials so consumers, CVE scanners, and procurement
teams can answer one question without trusting marketing or runtime introspection: **"What is inside this
artifact, exactly?"** The SBOM lists every direct and transitive dependency with version, licence, and
hash. It exists to satisfy:

- **Supply-chain CVE matching** — Dependabot, cargo-audit, Snyk, Trivy, Grype, etc. all consume SBOMs.
- **Procurement compliance** — SOC 2, ISO 27001, EU Cyber Resilience Act, US Executive Order 14028 all
  require an SBOM at acquisition.
- **Licence transparency** — proves at point of publish that licence constraints (Apache-2.0 notice,
  GPL-incompatibility, proprietary) are satisfied. Prevents a Phase-5b-style incident.
- **Drift detection** — by comparing each new SBOM to a committed baseline we catch unintended
  dependency-graph changes (transitive bumps, new transitive licences, etc.) at the moment they happen.

This protocol is **channel-agnostic** (crates.io, npm, PyPI, Maven, NuGet, binary releases). Only the
tool changes per channel; the obligation does not.

## 2. Hard rule
> **No package may be published unless a current CycloneDX SBOM has been generated, attached to the
> publish artifact, and either matches the committed baseline or the baseline has been intentionally
> refreshed in the same change.**
> **No exceptions.**

If the SBOM cannot be generated for any reason, the publish is blocked. Stop and remediate.

## 3. Tool — pinned
- **Rust workspace:** `cargo-cyclonedx` **v0.5.7**, installed via
  `cargo install --locked --version 0.5.7 cargo-cyclonedx`. Pinned in
  `scripts/release/sbom-generate.sh` (`CYCLONEDX_VERSION`). Bumping the version requires refreshing
  every baseline in the same commit.
- **npm (sdk-wasm, node):** `@cyclonedx/cyclonedx-npm` — added with the npm publish protocol.
- **PyPI:** `cyclonedx-py`.
- **Maven:** `cyclonedx-maven-plugin`.
- **NuGet:** `cyclonedx-dotnet`.

Per-channel tooling is added as each publish protocol matures; this document covers the **Rust
workspace** path that gates every publish whose artefact contains Rust-compiled code.

## 4. Where SBOMs live
- **Generated:** `dist/sbom/{crate}.cdx.json` (one JSON per publish-eligible crate; plus optional
  `workspace.cdx.json` rollup with `--workspace`). `dist/` is in `.gitignore`.
- **Committed baselines:** `docs/release/sbom_baselines/{crate}.cdx.json`. These are the "what the
  dependency graph looked like at the last intentional refresh"; CI compares the freshly-generated SBOM
  against this baseline and fails on drift.
- **Published alongside artefact:** the per-crate SBOM is uploaded as a CI artifact and attached to
  the release tag / GitHub Release / registry metadata (per channel checklist).

## 5. Lifecycle
1. **Run locally first.** Before any publish, an engineer runs:
   ```bash
   scripts/release/sbom-generate.sh --tool-install --check
   ```
   This installs the pinned tool if missing, generates per-crate SBOMs into `dist/sbom/`, and compares
   each against the committed baseline. Drift → script exits 3 → fix or refresh baseline.
2. **CI mirrors the local step.** The `package:sbom-generate` job in `.gitlab-ci.yml` does the same
   on a clean runner. Its artifact (`dist/sbom/`) is uploaded.
3. **Refresh baseline.** Intentional changes (dependency add/remove, version bump) require the same
   commit to refresh:
   ```bash
   scripts/release/sbom-generate.sh --tool-install
   cp dist/sbom/*.cdx.json docs/release/sbom_baselines/
   git add docs/release/sbom_baselines/
   ```
   The commit message must explain *why* the baseline changed (which dependency / which version).
4. **At publish.** The artefact uploaded to crates.io / npm / etc. is accompanied by the matching
   per-crate SBOM (per channel checklist). The post-publish-verify step re-downloads the published
   artefact and re-generates the SBOM from it; the two must match.

## 6. What the SBOM contains (CycloneDX JSON shape)
- `metadata.component` — the published crate (name, version, licence, PURL).
- `components[]` — every direct + transitive dependency with version, licence (SPDX), PURL, hash.
- `dependencies[]` — the dependency graph (who depends on whom).

The drift check ignores `serialNumber` and `metadata.timestamp` (they change per generation by design)
but enforces semantic equality on `components[]` and `dependencies[]`. A diff between two SBOMs that
only differ in those metadata fields is **not** drift.

## 7. CI failure modes & remediation
| failure | meaning | fix |
|---|---|---|
| `cargo-cyclonedx not found` | tool not installed on the runner | runner image must include the pinned version; `--tool-install` is for local dev only |
| `[sbom] FAIL: <crate>` | generation errored for one crate | inspect `dist/sbom/.{crate}.err`; usually a `Cargo.toml` shape issue (missing licence, malformed dependency) |
| `[sbom] NEW (no baseline): <name>` | baseline doesn't exist for this crate | first-time crate publish — commit the generated SBOM as baseline |
| `[sbom] DRIFT: <name> differs from baseline` | dependency graph changed | inspect with `diff -u docs/release/sbom_baselines/<name>.cdx.json dist/sbom/<name>.cdx.json`; if intentional → refresh baseline in the same commit |

## 8. Cross-references
- `docs/release/PUBLISH_PROTOCOL.md` — overarching publish protocol.
- `docs/release/release_gate_contract.md` — the fail-closed gate definitions.
- `scripts/release/sbom-generate.sh` — the generator.
- `.gitlab-ci.yml` job `package:sbom-generate` — CI integration.
- `docs/release/sbom_baselines/` — committed baseline SBOMs (one per publish-eligible crate).
