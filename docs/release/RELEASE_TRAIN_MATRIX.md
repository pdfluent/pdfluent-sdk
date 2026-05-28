# PDFluent Release Train — Per-Channel Matrix

**Status:** mandatory · **Owner:** release captain · **Started:** 2026-05-28 (Tier-3).
**Authoritative:** for any channel-level publish decision, this matrix wins until
itself updated.

Each row is one publish *target* (channel + package + version). The matrix
exists so an operator can scan one document and know: what to build, how to
audit, how to publish, how to smoke-test, how to record in the ledger, how
to roll back, and what credential is required. Every column references the
already-existing per-channel artefacts; this is a *navigation* document, not
new policy. Policy is `PUBLISH_PROTOCOL.md`.

---

## Workspace version snapshot (2026-05-28)

All publish-eligible PDFluent SDK packages are at **`1.0.0-beta.8`** (PyPI
spelling: `1.0.0b8`). 32 publish-eligible Rust crates in the workspace.
Editor/desktop binaries are out of scope for *this* repo's release train —
those live in a separate repo (`PDFluent/pdfluent`, React+Tauri) per
memory `reference_pdfluent_surfaces.md`.

## Matrix

| # | channel | package name | version | local artifact | dry-run | smoke | SBOM | ledger | rollback | credential |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | crates.io | `pdfluent` + 31 deps | `1.0.0-beta.8` | `target/package/<crate>-<v>.crate` | `scripts/release/crates_topo_dry_run.sh` (+ `topological_cratesio_dry_run.py` for full chain) | `consumer_smokes/smoke_rust.sh` | `sbom_baselines/<crate>.cdx.json` (32 committed) | `sha_ledger/crates_io.json` | `cargo yank --version <v> <crate>` + `ledger_mark_yanked.py` | crates.io API token; see `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §3 |
| 2 | npm (NAPI) | `@pdfluent/node` | `1.0.0-beta.8` | `crates/pdf-node/pdfluent-node-1.0.0-beta.8.tgz` (from `npm pack`) | `package:node-napi-dry-run` CI + `npm pack --dry-run` | `consumer_smokes/smoke_node.sh` | covered by `sbom_baselines/` rolling up pdfluent crate | `sha_ledger/npm.json` (new entry) | within 72h: `npm unpublish @pdfluent/node@<v>`; after: `npm deprecate @pdfluent/node@<v> "<reason>"` + `ledger_mark_yanked.py` | npm automation token on `@pdfluent` org |
| 3 | npm (WASM SDK) | `@pdfluent/sdk-wasm` | `1.0.0-beta.8` | `crates/xfa-wasm/pkg/<contents>` + `pdfluent-sdk-wasm-<v>.tgz` (from `npm pack`) | `scripts/release/wasm_dry_run.sh` | `consumer_smokes/smoke_wasm.sh` | covered by sbom-generate (the `xfa-wasm` baseline) | `sha_ledger/wasm.json` (new entry) | same as #2 (npm semantics) | npm automation token on `@pdfluent` org |
| 4 | PyPI | `pdfluent` | `1.0.0b8` | `crates/pdf-python/target/wheels/pdfluent-1.0.0b8-*.whl` (built by `maturin build --release`) | `python -m twine check <whl>` + `package:python-wheel-audit` CI | `consumer_smokes/smoke_python.sh` | wheel scrubbed via `scripts/release/scrub-wheel.py`; SBOM covered by sbom-generate of `pdfluent` crate | `sha_ledger/pypi.json` (new entry) | PyPI yank via project Manage UI (no CLI) + `ledger_mark_yanked.py` | PyPI API token (`__token__`) |
| 5 | Maven Central | `com.pdfluent:pdfluent` | `1.0.0-beta.8` | `bindings/java/target/pdfluent-1.0.0-beta.8.jar` + sources + javadoc | `mvn deploy -DperformRelease` to Sonatype STAGING (not release) + `scripts/release/maven_channel_guard.sh` | `consumer_smokes/smoke_java.sh` | Maven `pom.xml` + JNA classpath; SBOM via sbom-generate of underlying SDK crates | `sha_ledger/maven.json` (new entry) | drop staging repo BEFORE release; post-release: immutable, file Sonatype ticket | GPG key passphrase (in gpg-agent) + Sonatype API credentials |
| 6 | NuGet | `PDFluent` | `1.0.0-beta.8` | `bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.8.nupkg` | `dotnet pack -c Release` (`dotnet nuget push --dry-run` does not exist; use staging feed) + `package:nuget-audit` CI | `consumer_smokes/smoke_dotnet.sh` | natives staged via `scripts/release/stage_dotnet_natives.sh`; SBOM via sbom-generate | `sha_ledger/nuget.json` (new entry) | `dotnet nuget delete <pkg> <v>` (unlist, not hard delete) + `ledger_mark_yanked.py` | NuGet API key (`NUGET_API_KEY`) |
| 7 | C-ABI tarball | `pdf-capi` distributable | `1.0.0-beta.8` | `dist/pdf-capi-<v>-<target>.tar.gz` (from `scripts/release/package_cabi.sh`) | `scripts/release/package_cabi.sh --dry-run` (read-only build + audit) | manual: tiny C program against `include/` + link the lib + open a PDF | sbom-generate of `pdf-capi` crate | `sha_ledger/binary.json` (new entry under "c-abi" namespace) | replace/remove the uploaded file (reversible, dist storage) + `ledger_mark_yanked.py` | distribution storage credentials only |
| 8 | Binary release (CLI) | `pdfluent` (from `xfa-cli` crate, `[[bin]] name = "pdfluent"`) | `1.0.0-beta.8` | `dist/pdfluent-<v>-<target>.tar.gz` + `.sha256` (from `package:binary-release` CI) | `package:cli-cross-platform` + `package:binary-release` CI matrix (musl + windows-gnu — macOS in §"macOS deltas" below) | `consumer_smokes/smoke_binary.sh` | sbom-generate of `xfa-cli` crate | `sha_ledger/binary.json` (new entry per target) | GitHub/GitLab Release attachment delete; or new patch release + ledger yank | GitLab Releases API token (and registry credential for the host where Releases are mirrored); macOS Apple Developer ID + notarytool; Windows code-signing identity (see Tier-3 signing runbooks) |
| 9 | GitLab Package Registry (internal mirrors) | various | follows source channel | per channel | per channel | per channel | per channel | `sha_ledger/gitlab.json` | `glab package delete` (reversible, internal) | GitLab API token (`api` scope) |

## macOS deltas (binary release, channel #8)

Tier-1 CI cross-builds **`x86_64-unknown-linux-musl`** and
**`x86_64-pc-windows-gnu`** on the VPS runner (Ubuntu 24.04 + mingw + musl-tools).
The two macOS targets cannot be cross-compiled from Linux without `osxcross`
(heavy, license-sensitive). They are built **on a dev or operator Mac**:

| target | how |
|---|---|
| `x86_64-apple-darwin` | `cargo build --release --target x86_64-apple-darwin -p xfa-cli` on macOS-13+ |
| `aarch64-apple-darwin` | `cargo build --release --target aarch64-apple-darwin -p xfa-cli` on macOS-13+ (the architecture is the default `host` on Apple Silicon) |

Both then enter the **macOS signing runbook**
(`docs/release/SIGNING_MACOS.md`, Tier-3) — Developer ID Application
signature + notarization via `notarytool` — before being staged into
`dist/` and ledger-recorded the same way as Linux/Windows tarballs.

Windows binaries (`x86_64-pc-windows-gnu`) need the **Windows signing
runbook** (`docs/release/SIGNING_WINDOWS.md`, Tier-3) — Microsoft Trusted
Signing or the existing USB-token EV fallback — before public release.
Unsigned binaries MAY be uploaded as `*-unsigned.tar.gz` to internal
mirrors but MUST NOT be advertised as the primary download.

## Topological publish order (channel #1)

Strict order, per `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §1 and
verified by `scripts/release/topological_cratesio_dry_run.py`:

1. Pure crates (no internal deps): `pdf-syntax`, `pdf-interpret`,
   `pdf-font`, the four `pdfluent-*` upstream forks
   (`-ccitt`, `-jbig2`, `-jpeg2000`, `-lopdf`), `pdfluent-cff`.
2. Foundation: `xfa-dom-resolver`, `xfa-json`, `xfa-layout-engine`,
   `xfa-js-sandboxed`, `xfa-license`, `formcalc-interpreter`,
   `pdf-render`, `pdfluent-extract`.
3. Compliance + forms: `pdf-compliance`, `pdfluent-forms`, `pdf-annot`,
   `pdf-text-format`, `pdf-content-stream`.
4. Manipulation: `pdf-manip`, `pdf-redact`, `pdf-ocr`,
   `pdf-invoice`, `pdf-docx`, `pdf-xlsx`, `pdf-pptx`,
   `pdfluent-sign`.
5. XFA: `pdf-xfa`.
6. Engine: `pdf-engine`.
7. Umbrella: `pdfluent`.
8. Optional binary/binding adjuncts (`xfa-cli`, `pdf-capi`, etc.) — NOT
   in the crates.io chain unless the binding maintainer wants them
   published as Rust crates too.

The current crates.io state (per the Tier-1 bite-C audit re-run) only
has `formcalc-interpreter ≤ beta.5` and `pdf-compliance ≤ beta.7`. The
`=1.0.0-beta.8` chain has NOT been published; this is the
**topological-publish-train R2-1 gap** — operator-only work outside
both Tier-1 and Tier-2 scope.

## Channel-to-checklist + procedure map

For each channel above, the operator uses:

| channel | checklist | runbook section |
|---|---|---|
| crates.io | `checklists/crates_io.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.1 |
| npm (NAPI) | `checklists/npm.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.3 |
| npm (WASM) | `checklists/wasm.md` + `checklists/npm.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.7 |
| PyPI | `checklists/pypi.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.4 |
| Maven | `checklists/maven.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.6 |
| NuGet | (use `npm.md` shape; NuGet-specific checklist follow-on) | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.5 |
| C-ABI | `cabi_packaging.md` | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` §2.2 |
| Binary release | `checklists/binary_release.md` + `pdfluent_cli_release_checklist.md` | this document §"macOS deltas" + signing runbooks |
| GitLab Package Registry | `checklists/gitlab_package_registry.md` | per channel mirror |

## Stop-the-line — current blockers

| # | blocker | scope | owner |
|---|---|---|---|
| 1 | Topological crates.io publish of `=1.0.0-beta.8` chain (4 crates: `formcalc-interpreter` → `pdf-compliance` → `pdf-xfa` → `pdfluent`) | publish-train R2-1 (Wave-2) | release captain (operator) |
| 2 | crates.io API token | operator credential | release captain |
| 3 | npm automation token on `@pdfluent` org | operator credential | release captain |
| 4 | PyPI API token (`__token__`) | operator credential | release captain |
| 5 | Maven GPG key + Sonatype credentials | operator credential | release captain |
| 6 | NuGet API key | operator credential | release captain |
| 7 | macOS Apple Developer ID Application certificate + Notarytool API key | code-signing identity | business decision (jasperdew) |
| 8 | Windows code-signing identity — Microsoft Trusted Signing OR USB-token EV/OV cert | code-signing identity | business decision (jasperdew) |

Items 1 and 7–8 are the *hard* blockers; items 2–6 are credentials the
release captain provisions per channel as the train runs.

## Cross-references

- `docs/release/PUBLISH_PROTOCOL.md` — global policy (this matrix is the
  per-channel navigation aid).
- `docs/release/SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` — multi-channel
  publish flow narrative (§1 strict order, §2 per-channel procedures).
- `docs/release/ROLLBACK_PROCEDURE.md` (Tier-2 R5) — yank/deprecate
  runbook (per-channel commands in §2).
- `docs/release/sha_ledger/README.md` (Tier-2 R6) — ledger lifecycle.
- `docs/release/sbom_protocol.md` (Tier-1) — SBOM policy.
- `docs/release/SIGNING_MACOS.md` (Tier-3) — macOS Developer ID +
  notarization runbook.
- `docs/release/SIGNING_WINDOWS.md` (Tier-3) — Windows code-signing
  runbook (Microsoft Trusted Signing + USB-token EV fallback).
