# PDFluent Release Gate Contract

**Version:** 1.0.0
**Status:** Mandatory — no exceptions
**Owner:** release captain (or operator running publish)
**Last updated:** 2026-05-16 (E1 release gate consolidation)
**Governed by:** `docs/release/PUBLISH_PROTOCOL.md`

---

## 1. Purpose

This contract defines the mandatory gates that every package publish must pass
before bytes leave the repository. It is channel-agnostic: Rust crates, Python
wheels, WASM npm packages, .NET NuGet packages, and Java Maven artefacts all
follow the same gate sequence.

A gate is a **stop-the-line check**. If it fails, the publish step does not
start. No exceptions, no "I'll fix it in the next version."

---

## 2. Gate Sequence

Gates execute in order. A gate that emits P0 findings aborts the sequence.

### Gate 0 — Clean Tree

| Check | Stop-the-line |
|---|---|
| `git status --porcelain` is empty | yes |
| Current HEAD is on the publish branch | yes |
| `--allow-dirty` flag is **not** present in any publish command | yes |

**Script:** `prepublish_crate_audit.sh` enforces this for Rust.
`audit-all-packages.sh` enforces this globally before dispatching to per-channel audits.

---

### Gate 1 — License Metadata

| Check | Stop-the-line |
|---|---|
| SPDX expression in manifest `license` field (or `license-file` for proprietary) | yes |
| SPDX matches policy for crate type (fork → `MIT OR Apache-2.0`; ours → `AGPL-3.0-only OR LicenseRef-PDFluent-Commercial`) | yes |
| `license-file` resolves to an existing file | yes |

**Policy:**
- Forked open-source code (hayro/* crates): `MIT OR Apache-2.0`
- PDFluent-authored crates: `AGPL-3.0-only OR LicenseRef-PDFluent-Commercial`
- npm and NuGet cannot carry a LicenseRef expression, so they point at a file
  instead: `SEE LICENSE IN LICENSE-OFFER` and `<PackageLicenseFile>LICENSE-OFFER</PackageLicenseFile>`.
  `LICENSE` is the AGPL text and nothing else (#349), so it is not the pointer's
  target — a pointer to it would publish half the offer.
- Mixed: use `license-file`; never omit the actual text

---

### Gate 2 — License Files in Artefact

| Check | Stop-the-line |
|---|---|
| For `MIT OR Apache-2.0`: `LICENSE-MIT` **and** `LICENSE-APACHE` present inside tarball/zip/jar | yes |
| For `MIT` only: `LICENSE` or `LICENSE-MIT` present inside tarball | yes |
| For `Apache-2.0` only: `LICENSE` and `NOTICE` present inside tarball | yes |
| For PDFluent Commercial License: `LICENSE` with full PDFluent text present inside tarball | yes |

**Trigger incident:** Phase 5b shipped five forked crates missing `LICENSE-APACHE`. Gate 2 exists because Gate 1 passing is not sufficient.

---

### Gate 3 — Package Identity

| Check | Stop-the-line |
|---|---|
| Package name matches PDFluent naming convention (`pdfluent-*`, `@pdfluent/*`, `com.pdfluent.*`, `PDFluent`) | yes |
| Version matches the intended release version (no stale version in manifest) | yes |
| `homepage` / `url` points to `https://pdfluent.com` | yes |
| `authors` / `authors` field identifies Innovation Trigger BV | yes |
| Repository field is absent OR points to PDFluent GitLab (never GitHub) | yes |

---

### Gate 4 — No Private Paths

| Check | Stop-the-line |
|---|---|
| No `/Users/<name>/` paths inside artefact | yes |
| No `/home/<name>/` paths inside artefact | yes |
| No `/opt/xfa/` paths inside artefact | yes |
| No `/mnt/storagebox/` paths inside artefact | yes |
| No VPS-mirror paths inside artefact | yes |
| No workstation-absolute paths in SBOM entries, purl fields, or source maps | yes |

**Script:** `audit_package_tree.py` -- `LEAKAGE_PATH_PATTERNS`

---

### Gate 5 — No GitHub URLs

| Check | Stop-the-line |
|---|---|
| `github.com` absent from `package.json` / `Cargo.toml` / `pom.xml` / `.csproj` manifest | yes |
| `github.com` absent from packed artefact manifest fields | yes |
| `github.com` absent from SBOM purl / download_url fields | yes |

**Note:** `github.com` may appear in Rust source code as a documentation link inside
library code — that is acceptable. The check applies to **manifest fields** and
**SBOM metadata** only. `audit_package_tree.py` applies this check to text files
inside the artefact; manifests are checked separately by identity gate.

---

### Gate 6 — No Secrets

| Check | Stop-the-line |
|---|---|
| No Stripe live/test key pattern inside artefact | yes |
| No AWS access key ID (AKIA...) inside artefact | yes |
| No GitHub/GitLab PAT inside artefact | yes |
| No Google API key inside artefact | yes |
| No Slack token inside artefact | yes |
| No PEM private key block inside artefact | yes |
| No `API_KEY=`, `SECRET=`, `TOKEN=` assignment with a high-entropy value | yes |
| Forbidden filenames absent: `.env`, `.npmrc`, `.pypirc`, `id_rsa`, `credentials.json` | yes |

**Script:** `audit_package_tree.py` -- `LEAKAGE_SECRET_PATTERNS` + `FORBIDDEN_FILENAMES`

---

### Gate 7 — No Corpus / Oracle Paths

| Check | Stop-the-line |
|---|---|
| No `xfa-golden` path segments inside artefact | yes |
| No `oracle-baseline` / `oracle-render` / `oracle-drift` / `oracle-trace` inside artefact | yes |
| No `corpus/` path segments or corpus-extension files (`.pdf`, `.png`, `.tiff`, `.jp2`) inside non-test artefact | yes |
| No `vps-mirror` references inside artefact | yes |

**Script:** `audit_package_tree.py` -- `LEAKAGE_PATH_PATTERNS` + `CORPUS_EXTENSIONS`

---

### Gate 8 — Dry-Run Pass

| Check | Stop-the-line |
|---|---|
| `cargo package -p <crate>` (or equivalent) completes without error | yes |
| Channel dry-run (where supported) resolves cleanly: `cargo publish --dry-run`, `npm pack`, `maturin build`, `mvn package -DskipTests`, `dotnet pack` | yes |
| Dry-run artefact size ≤ 50 MiB (or explicitly waived) | yes |

---

### Gate 9 — Consumer Smoke (Post-Publish)

| Check | Stop-the-line (blocks next publish step) |
|---|---|
| After publish, install the exact published version in a clean environment | yes |
| Import / require the package and exercise the primary API surface | yes |
| Verify version string matches expected release version | yes |

**Templates:** `docs/release/consumer_smokes/` (Round 3 deliverable)

---

### Gate 10 — Registry Post-Publish Verification

| Check | Timing |
|---|---|
| Package visible on registry within expected index propagation window | within 5 min (crates.io) / immediate (npm/PyPI/NuGet/Maven) |
| Package metadata (name, version, license, description) correct as displayed on registry | before announcing release |
| Download URL resolves and checksum matches locally-built artefact | before announcing release |

**Template:** `docs/release/templates/POST_PUBLISH_VERIFY_TEMPLATE.md`

---

### Gate 11 — Audit Report Committed

| Check | Stop-the-line |
|---|---|
| Audit report written to `benchmarks/runs/prepublish_audits/<name>-<version>.<ext>` | yes |
| Report committed to the publish branch before `cargo publish` / `npm publish` / etc. | yes |
| Report contains: date, operator, channel, artefact path, gate results, verdict | yes |

**Template:** `docs/release/templates/PREPUBLISH_AUDIT_REPORT_TEMPLATE.md`

---

## 3. Per-Channel Gate Matrix

| Gate | Rust/crates.io | Python/PyPI | WASM/npm | .NET/NuGet | Java/Maven |
|---|:---:|:---:|:---:|:---:|:---:|
| 0 Clean tree | ✅ | ✅ | ✅ | ✅ | ✅ |
| 1 License metadata | ✅ | ✅ | ✅ | ✅ | ✅ |
| 2 License files in artefact | ✅ | ✅ | ✅ | ✅ | ✅ |
| 3 Package identity | ✅ | ✅ | ✅ | ✅ | ✅ |
| 4 No private paths | ✅ | ✅ | ✅ | ✅ | ✅ |
| 5 No GitHub URLs (manifests) | ✅ | ✅ | ✅ | ✅ | ✅ |
| 6 No secrets | ✅ | ✅ | ✅ | ✅ | ✅ |
| 7 No corpus/oracle paths | ✅ | ✅ | ✅ | ✅ | ✅ |
| 8 Dry-run pass | ✅ | ✅ | ✅ | ✅ | ✅ |
| 9 Consumer smoke | ✅ | ✅ | ✅ | ✅ | ✅ |
| 10 Registry verification | ✅ | ✅ | ✅ | ✅ | ✅ |
| 11 Audit report committed | ✅ | ✅ | ✅ | ✅ | ✅ |

---

## 4. Deprecated Scripts

The following scripts pre-date the gate contract and must not be used for production publish:

| Script | Status | Replacement |
|---|---|---|
| `scripts/publish_all.sh` | **Deprecated** — no artefact inspection, no audit report | `scripts/release/audit-all-packages.sh` + `scripts/release/release_train_guard.sh` |

`scripts/publish_ordered.sh` remains valid as the crates.io train executor, but must only be called **after** `release_train_guard.sh` (audit-first) passes.

---

## 5. Remediation Protocol

When a gate blocks a publish:

1. Do not skip, bypass, or comment out the gate.
2. Fix the underlying issue in the source or artefact.
3. Re-run the full gate sequence from Gate 0.
4. Write a remediation note in the audit report explaining what was found and what was fixed.
5. Only after all gates pass: proceed with publish.

If you believe a gate is producing a false positive, file an issue with evidence.
Do not publish while the investigation is open.

---

## 6. Enforcement

- CI pipeline: gates 0–8 run on every push to a publish branch.
- Pre-publish: operator runs `scripts/release/audit-all-packages.sh` for the target channel.
- Post-publish: operator runs the registry verification template.
- Audit report: committed before every real publish, no exceptions.
