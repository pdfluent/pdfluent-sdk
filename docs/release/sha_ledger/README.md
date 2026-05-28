# PDFluent Registry SHA Ledger

**Status:** mandatory · **Owner:** release captain · **Started:** 2026-05-28 (Tier-2 R6).

The SHA ledger is the committed, auditable record of every PDFluent artifact ever
published to any registry. Its purpose is single: **make post-publish drift
detectable forever**.

## 1. Why a ledger

A package registry is mutable from the registry's side — at minimum the
registry can return a tarball with different bytes than the one the operator
uploaded (CDN cache poisoning, upstream supply-chain attack, registry bug, or
deliberate substitution). The pre-publish audit (per `PUBLISH_PROTOCOL.md` §4)
sha256s the local artifact at the moment of publish; the post-publish verify
(per §11) sha256s the registry-downloaded artifact and compares. **The
ledger persists both, indefinitely**, so any third party can re-verify a
published version *years later* against the canonical sha256 PDFluent
recorded at publish time.

This closes the same defect class the Phase 5b licence incident exposed:
without a permanent, project-side record of "what we *intended* to publish",
"what's *now* in the registry" cannot be unambiguously compared.

## 2. File layout

Per-channel JSON files under `docs/release/sha_ledger/`:

| file | channel |
|---|---|
| `crates_io.json` | crates.io |
| `npm.json` | npm |
| `pypi.json` | PyPI |
| `maven.json` | Maven Central / GitHub Packages Maven |
| `nuget.json` | NuGet (.NET) |
| `wasm.json` | WASM tarball (if shipped stand-alone from npm) |
| `binary.json` | GitHub / GitLab Release binary artifacts |
| `gitlab.json` | GitLab Package Registry (internal mirrors) |

A per-channel layout (rather than a single combined ledger) keeps the diff
surface small per publish (only the touched channel's file changes), avoids
merge conflicts when channels publish independently, and matches the
existing per-channel checklist structure under `docs/release/checklists/`.

Each file conforms to `schema.json` in this directory.

## 3. Entry lifecycle

```
   pre-publish ──▶ "draft" entry (local sha only)
                       │
                       ▼
   publish step ──▶ "live" entry
                       │
                       ▼
   post-publish ──▶ "verified" entry (local sha == registry sha)
                       │
                       ▼ (only if needed)
   yank/deprecate ──▶ "yanked" entry (status + yanked_at + yanked_reason)
```

Entries are **append-only** for the `live` and `verified` events. Yank is a
status mutation on an existing entry (writes `yanked_at`, `yanked_reason`,
sets `status = "yanked"`); the original `published_at` / `sha256_local` /
`sha256_registry` MUST NOT be edited.

## 4. Entry fields

See `schema.json` for the full spec. Required fields per entry:

| field | type | example |
|---|---|---|
| `package` | string | `pdfluent` |
| `version` | string | `1.0.0-beta.8` |
| `artifact_filename` | string | `pdfluent-1.0.0-beta.8.crate` |
| `sha256_local` | string (hex) | `abc123…` (sha256 of the locally-built tarball at publish time) |
| `sha256_registry` | string (hex) or null | filled by post-publish verify, null until then |
| `size_bytes` | integer | `12345` |
| `published_at` | ISO-8601 UTC | `2026-05-28T12:34:56Z` |
| `verified_at` | ISO-8601 UTC or null | filled by post-publish verify |
| `audit_report` | string (repo-relative path) | `benchmarks/runs/prepublish_audits/pdfluent-1.0.0-beta.8.md` |
| `verify_report` | string (repo-relative path) or null | filled by post-publish verify |
| `status` | enum | one of `live`, `verified`, `yanked` |
| `yanked_at` | ISO-8601 UTC or null | filled on yank |
| `yanked_reason` | string or null | filled on yank |
| `registry_url` | string (URL) | canonical URL where the artifact can be re-downloaded |

## 5. Tooling

| script | what it does |
|---|---|
| `scripts/release/ledger_add_entry.py` | append a new entry. Called by the per-channel publish step right after a successful `cargo publish` / `npm publish` / etc. Computes sha256 from the local artifact, writes draft `live` entry. |
| `scripts/release/ledger_verify.py` | for one entry, downloads from `registry_url`, sha256s, compares to `sha256_local`. On success writes `sha256_registry` + `verified_at` and bumps status to `verified`. On mismatch: refuses to write, prints the diff, exits non-zero. |
| `scripts/release/ledger_mark_yanked.py` | sets `status=yanked` + `yanked_at` + `yanked_reason` on an entry. Idempotent: if entry already yanked, leaves it alone (preserves original timestamp). |

All three scripts read/write the per-channel JSON files atomically (write to
tempfile, then rename) so concurrent invocations do not corrupt the ledger.

## 6. Verification at any later time

Any third party (auditor, customer, regulator) MUST be able to re-verify a
published version years later using:

```bash
git clone <pdfluent-repo>
python3 scripts/release/ledger_verify.py --channel crates_io --package pdfluent --version 1.0.0-beta.8
```

The script downloads from the recorded `registry_url`, sha256s, and either
prints "VERIFIED MATCH" or the precise bytes-changed diff. The committed
`sha256_local` is the canonical reference.

## 7. Hard rules

> **R6-1.** Every publish step MUST call `ledger_add_entry.py` immediately
> after the successful registry upload. If the script fails, the publish is
> considered incomplete and §12 of PUBLISH_PROTOCOL.md ("Emergency
> remediation procedure") applies.
>
> **R6-2.** Every post-publish verify MUST call `ledger_verify.py` against
> the entry it just wrote. The verify report under
> `benchmarks/runs/post_publish_verify/` MUST reference the ledger entry by
> `(channel, package, version)`.
>
> **R6-3.** No entry MAY be deleted from the ledger after it has been
> written. A yanked version stays in the ledger with `status=yanked` and
> remains discoverable; the entry's purpose is exactly to document that this
> version was published, even if it should not be installed.

## 8. Cross-references

- `docs/release/PUBLISH_PROTOCOL.md` §11 — post-publish verification (writes verify_report path used here)
- `docs/release/PUBLISH_PROTOCOL.md` §12 — emergency remediation (mark_yanked.py is invoked from §12 step 2)
- `docs/release/PUBLISH_PROTOCOL.md` §13 — yanking policy (preconditions for invoking mark_yanked.py)
- `docs/release/sbom_protocol.md` — SBOM is a separate provenance artefact; the SBOM `sha256_local` is recorded for the crate's *contents*; the ledger `sha256_local` is recorded for the *artefact* (the .crate / .tgz / .whl / etc. as uploaded). They are complementary, not duplicate.
- `docs/release/release_gate_contract.md` — gate definitions cross-reference the ledger as one of the post-publish gates.
