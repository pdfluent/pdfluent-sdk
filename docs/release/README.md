# PDFluent Release Documentation — Index

**Started:** 2026-05-28 (Tier-2 R3). Single entry point for any operator
publishing PDFluent to any channel. Read top-down on a fresh-eyes day,
keyword-jump on a publish day.

The release artefacts under this directory are split into **policy** (the
rules), **procedures** (step-by-step runbooks), **per-channel material**
(checklists + smokes), **templates** (report scaffolds for audit / verify /
remediation), and **state** (committed records: SBOM baselines + SHA
ledger). Every operator artefact ultimately points back to the **global
policy** in `PUBLISH_PROTOCOL.md`, which is the single source of truth
for *what must hold* before any byte ships.

---

## 1. Where to start

| If you are about to … | Read this first |
|---|---|
| Run a publish (any channel) | `PUBLISH_PROTOCOL.md` then this README §3 for the channel |
| Run the multi-channel SDK publish train | `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` |
| Yank or deprecate a defective version | `ROLLBACK_PROCEDURE.md` |
| Refresh SBOM baselines (intentional dep change) | `sbom_protocol.md` §5 step 3 |
| Verify a previously-published version against the registry | `sha_ledger/README.md` §6 |
| Write a prepublish audit report | `templates/PREPUBLISH_AUDIT_REPORT_TEMPLATE.md` |
| Write a post-publish verify report | `templates/POST_PUBLISH_VERIFY_TEMPLATE.md` |
| Write a remediation report (post-yank) | `templates/REMEDIATION_REPORT_TEMPLATE.md` |
| Understand the release-blocking gate set | `release_gate_contract.md` |

## 2. Policy (the rules)

`PUBLISH_PROTOCOL.md` — channel-agnostic global protocol; §2 "no exceptions"
hard rule. Everything else cross-references this document.

`sbom_protocol.md` — CycloneDX SBOM is mandatory for every publish, every
channel. Baselines live under `sbom_baselines/`, generator + drift check
in `scripts/release/sbom-generate.sh`.

`release_gate_contract.md` — fail-closed gate definitions; agents and
humans both stop on any gate red.

## 3. Procedures (step-by-step)

| document | when to use |
|---|---|
| `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md` | the full multi-channel publish flow for the non-XFA SDK; lists the strict topological order |
| `ROLLBACK_PROCEDURE.md` | post-publish defect: triage → yank/deprecate per-channel command → ledger update → replacement → close |
| `pdfluent_cli_packaging.md` | host-target CLI release packaging for the `pdfluent-cli` crate |
| `pdfluent_cli_release_checklist.md` | go/no-go checklist for a CLI release |
| `pdfluent_cli_binary_takeover_strategy.md` | longer-term strategy for the `pdfluent` binary name takeover from `xfa-cli` |
| `cabi_packaging.md` | C-ABI tarball packaging |

## 4. Per-channel material

### 4.1 Prepublish checklists (`checklists/`)

Step-checkbox lists per channel. Every line in every checklist MUST be
checked AND recorded in a per-publish audit report under
`benchmarks/runs/prepublish_audits/`.

| channel | checklist |
|---|---|
| crates.io | `checklists/crates_io.md` |
| npm | `checklists/npm.md` |
| PyPI | `checklists/pypi.md` |
| Maven Central | `checklists/maven.md` |
| WASM | `checklists/wasm.md` |
| Binary release | `checklists/binary_release.md` |
| GitLab Package Registry | `checklists/gitlab_package_registry.md` |

NuGet is currently covered by per-publish runbook section §2.5 of
`SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`; a `checklists/nuget.md` is a
follow-on if NuGet publishes become routine.

### 4.2 Consumer smoke tests (`consumer_smokes/`)

Run **after** publish (or against a local pre-publish artefact) to verify
the package is installable + importable in a clean consumer environment.

| smoke | channel(s) | gates |
|---|---|---|
| `consumer_smokes/smoke_rust.sh` | crates.io | install + import + valid open |
| `consumer_smokes/smoke_python.sh` | PyPI | install + import + valid open |
| `consumer_smokes/smoke_wasm.sh` | npm (WASM) | install + import + JS API |
| `consumer_smokes/smoke_node.sh` | npm (NAPI) | install + import (ESM + CJS) + version check |
| `consumer_smokes/smoke_dotnet.sh` | NuGet | install + import + valid open |
| `consumer_smokes/smoke_java.sh` | Maven | install + import + valid open |
| `consumer_smokes/smoke_binary.sh` | Binary release | sidecar sha + bundled SHA256SUMS + LICENSE + --version (host-target only) |

Orchestrator: `../scripts/release/post_publish_smoke_runner.sh
--channel all` runs every locatable smoke + writes an aggregate
`SUMMARY.md` to `benchmarks/runs/post_publish_verify/<ts>/` linkable as
the ledger entry's `verify_report`.

## 5. State (committed records)

### 5.1 SBOM baselines (`sbom_baselines/`)

32 per-crate CycloneDX SBOMs at `1.0.0-beta.8` (post-Tier-1 merge). The
`scripts/release/sbom-generate.sh --check` job in
`.gitlab-ci.yml` compares freshly generated SBOMs against these baselines
and fails on drift. Intentional dep changes (e.g. rustls swap) require
the same commit to refresh the affected baseline(s) with a commit
message explaining the trigger.

### 5.2 SHA ledger (`sha_ledger/`)

Append-only record of every published artefact's sha256, used for
indefinite post-publish drift detection. Per-channel JSON files
(`crates_io.json`, `npm.json`, … 8 channels), schema in `schema.json`,
README in `sha_ledger/README.md`. Tools:

| script | purpose |
|---|---|
| `../scripts/release/ledger_add_entry.py` | append "live" entry after a successful publish; sha256+size from local artefact |
| `../scripts/release/ledger_verify.py` | download from `registry_url`, sha256, compare to ledger; on match `--write` mutates to `verified` |
| `../scripts/release/ledger_mark_yanked.py` | flip status to `yanked` + `yanked_at` + `yanked_reason` (irreversible, audit-trail) |

The ledger is **empty** at the post-Tier-1 commit and will be populated
by the first real publish train.

## 6. Templates (`templates/`)

| template | populated under |
|---|---|
| `templates/PREPUBLISH_AUDIT_REPORT_TEMPLATE.md` | `benchmarks/runs/prepublish_audits/<pkg>-<v>.md` |
| `templates/POST_PUBLISH_VERIFY_TEMPLATE.md` | `benchmarks/runs/post_publish_verify/<pkg>-<v>.md` (or per-run dir from the smoke runner) |
| `templates/REMEDIATION_REPORT_TEMPLATE.md` | `benchmarks/runs/remediation/<incident-id>.md` |

## 7. Operator entry-point cheat sheet

```text
                       new publish ─────────▶ PUBLISH_PROTOCOL.md
                                           ├─▶ checklists/<channel>.md
                                           ├─▶ scripts/release/sbom-generate.sh (--check)
                                           ├─▶ <publish command>
                                           ├─▶ scripts/release/ledger_add_entry.py
                                           ├─▶ scripts/release/post_publish_smoke_runner.sh
                                           └─▶ scripts/release/ledger_verify.py --write

defective publish landed ─▶ ROLLBACK_PROCEDURE.md
                              ├─▶ §1 yank-vs-deprecate decision
                              ├─▶ §2 channel-specific yank command
                              ├─▶ §3 ledger_mark_yanked.py
                              ├─▶ §5 fix + republish
                              └─▶ §7 templates/REMEDIATION_REPORT_TEMPLATE.md

third-party verifies historical publish ─▶ sha_ledger/README.md §6
                                            └─▶ scripts/release/ledger_verify.py
```

## 8. Tier history

The release directory has accreted in tiers. Each tier was authored to
close the specific gap surfaced by the previous publish (or by the GA-
hardening audit). The current contents are:

- **Pre-Tier-1**: `PUBLISH_PROTOCOL.md`, `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`,
  `release_gate_contract.md`, `checklists/`, `templates/`, `consumer_smokes/`
  (rust/python/wasm/dotnet/java), `pdfluent_cli_*`, `cabi_packaging.md`.
- **Tier-1 (2026-05-28 MR !5)**: `sbom_protocol.md`, `sbom_baselines/`,
  `RELEASE_HARDENING_AUDIT_2026_05_27.md`, plus four new CI jobs
  (`package:sbom-generate`, `package:node-napi-dry-run`,
  `package:cli-cross-platform`, `package:binary-release`).
- **Tier-2 (this MR)**: `ROLLBACK_PROCEDURE.md` (R5), `sha_ledger/` + 3
  ledger scripts (R6), `consumer_smokes/smoke_node.sh` +
  `consumer_smokes/smoke_binary.sh`, `scripts/release/post_publish_smoke_runner.sh`
  (R4), this index (R3).

The remaining work between this Tier-2 and the first real beta publish
is non-engineering: registry credentials, code-signing identity, the
topological crates.io publish chain (beta.8 propagation), and the
business decisions out of scope for any of these tiers.
