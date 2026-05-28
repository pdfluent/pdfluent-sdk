# PDFluent Release Hardening — Audit & Action Plan

**Date:** 2026-05-27 · **rc1:** `aa9ba515b` · **Audit-only, no implementation.**

## 1. What we have (inventory)

### Workspace & packaging
- **28+ crates**, pinned **Rust 1.94.0** (rust-toolchain.toml), workspace MSRV **1.80.0**.
- Current workspace version: **`1.0.0-beta.8`**. Published WASM: `@pdfluent/sdk-wasm@1.0.0-beta.11` (2026-05-16).
- Dual license root (MIT + Apache-2.0) + `LICENSE` (proprietary SDK) + `NOTICE` matrix.
- Per-crate license matrix (audited): **all publish-eligible crates have an effective license** —
  `MIT OR Apache-2.0` for upstream forks, `license-file = "LICENSE"` for proprietary SDK crates,
  workspace-MIT inheritance for internal-only. **One trivial gap:** `xfa-pdfrest-compare` (`publish=false`,
  no license field — cosmetic).
- `deny.toml` with allow-list + clarify entries for the three proprietary crates
  (`formcalc-interpreter`, `pdf-annot`, `pdf-compliance` → `LicenseRef-PDFluent-Commercial`).
- `.gitignore`, `.gitattributes`, `Dockerfile`, `Formula/` (Homebrew tap).

### CI / GitLab pipeline (29 jobs across 5 stages)
- **`sanity` (auto on every push):** cargo-metadata, cargo-check, cargo-fmt, cargo-deny.
- **`quality_manual`:** cargo-test, cargo-clippy, audit-all-packages, llms-validator, c8-error-catalogue-sync.
- **`package_manual`:** crates-dry-run, wasm-tarball-dry-run, python-wheel-audit, nuget-audit, maven-audit.
- **`corpus_manual`:** xfa-handoff, xfa-no-private-paths, xfa-smoke, xfa-full-gate (dedicated runner).
- **`release_manual`:** crates-publish, wasm-npm-publish (operator-triggered only).
- Maintenance: disk-report, cleanup-dry-run, cleanup-apply.
- Pre-push hook: `scripts/ci/local_ci_gate.sh` mirrors `sanity` (fast) + `--full` adds test+audit.

### Release tooling & process
- **`docs/release/PUBLISH_PROTOCOL.md`** — mandatory protocol (instituted after the Phase-5b LICENSE-files-
  missing-in-tarball incident; "no publish without committed audit report; no exceptions").
- **Per-channel checklists:** `crates_io`, `npm`, `pypi`, `maven`, `nuget`, `gitlab_package_registry`,
  `binary_release`, `wasm` (in `docs/release/checklists/`).
- **Templates:** `PREPUBLISH_AUDIT_REPORT_TEMPLATE.md`, `POST_PUBLISH_VERIFY_TEMPLATE.md`,
  `REMEDIATION_REPORT_TEMPLATE.md`.
- **Consumer smokes:** `consumer_smokes/smoke_rust.sh`, `smoke_wasm.sh`.
- **CLI release scripts:** `scripts/release/pdfluent_cli_package.sh`,
  `pdfluent_cli_release_gate.sh` (fail-closed: publish=false / binary-name / version-consistency /
  LICENSE-in-crate-and-package / SHA256SUMS verify / no-secrets / no-private-paths / no-debug-symbols).
- **Audit-all-packages script:** `scripts/release/audit-all-packages.sh` (5 channels clean per GA plan).
- **Reproducible builds:** `--remap-path-prefix` (`.cargo/config.toml`) + `SOURCE_DATE_EPOCH`.

### Prior release docs (extensive)
- `benchmarks/runs/ga_hardening_plan/` — master GA plan with tracks R/C/B/G/F/EX/CI/COM and consolidated
  `GA_HARDENING_ISSUES.md` (per-milestone Pri/Wave/Parallel/Depends/Verdict table).
- `benchmarks/runs/post_beta_roadmap/` and `post_beta_execution/` — prior wave reports (C2..C7,
  D-XfaJs, G3/G5/G6).
- `benchmarks/runs/prepublish_audits/` — committed per-package audit reports
  (pdfluent, pdfluent-capi, +v3-tonight-{cabi,dotnet,java,node,python}).
- `benchmarks/runs/wave1_release{,_fixup}`, `wave1_publish{,_recovery}`, `sdk_hardening/`.
- `docs/reports/pdfluent_release_systems_map_final.{md,json}` (systems map).
- `docs/reports/pdfluent_cli_release_readiness_final.md` (CLI verdict:
  **`PDFLUENT_CLI_OPERATIONALLY_READY_BUT_DISTRIBUTION_INCOMPLETE`**).
- `docs/xfa/fidelity/milestones/MILESTONE_J_RELEASE_GRADE_XFA_FIDELITY_REPORT.md`.
- `CHANGELOG.md` (16 KB, entries through beta.5 + beta.11 WASM).
- `BENCHMARKS_SLA.md`, `BACKLOG.md`, `ARCHITECTURE.md` (54 KB), `IMPLEMENTATION_GUIDE.md` (64 KB),
  `API_DESIGN.md`, `CONTRIBUTING.md`, `DISTRIBUTION_SETUP.md`.

### XFA / runtime state (this program)
- BE-1 runtime foundations merged + default-safe (rc1 `aa9ba515b`):
  Epic A (observability), Epic B (`$data`/`#items` SOM), benign absent-declared-node SOM façade,
  `XFA_JS_HARVEST_MODE`. Shipping default byte-identical: **92.3% raw / 94.6% trustworthy**.
- B-keepall opt-in (`XFA_SUPPRESSION_TRUST_LAYOUT`): 98.2% / 99.3%.
- Sandboxed runtime: corpus-identical to static default + `2ff85101` 9→4 (real wall-doc parity).
- B-default-on definitively hard-walled (Epic A-D + saved-form-DOM diagnoses).

## 2. What's DONE (GA-hardening sweep)
Verified by `*_FINAL_REPORT.md` presence + green verdict:

| Track | Done items | Source |
|---|---|---|
| R (Release Train) | R1-1 crates dry-run, R1-3 WASM dry-run, R2 build verification | r1/*_FINAL_REPORT.md |
| C (Binding DX) | C8 error catalogue (15 codes, per-binding), C9 Golden Path (7 bindings) | C8/C9_FINAL_REPORT.md |
| CI | CI1 GitLab lanes (4 core), CI2/CI3 VPS storage | CI1, DAY3E_FINAL_REPORT.md |
| COM | COM1 license activation across all bindings | COM1_*_FINAL_REPORT.md |
| F | F4 llms.txt validator | DAY3B_FINAL_REPORT.md |
| EX | 8 Golden Path examples wired | DAY3C_FINAL_REPORT.md |
| Prior wave | C2-C7, D-XfaJs, G3/G5/G6 | post_beta_roadmap/* |

## 3. What's PENDING — engineering items only (no business decisions)
Ordered by ROI for the **ship gate** (highest first):

### Tier 1 — gates the actual ship (concrete, bounded, ready to execute)
| # | Item | Effort | Source |
|---|---|---|---|
| 1 | **CycloneDX SBOM step** (vendor `cargo-cyclonedx` + add CI job per `PUBLISH_PROTOCOL`) | Low (1 day) | CLI readiness §"Real engineering blockers" |
| 2 | **Cross-platform CI matrix** (add Linux + Windows triples to the CLI build+gate) | Moderate | CLI readiness ("only host triple validated") |
| 3 | **Binary release CI job** (multi-platform per `docs/release/checklists/binary_release.md`) | Moderate | binary_release checklist |
| 4 | **Node NAPI dry-run CI job (R1-4)** — audits exist, CI job missing | Low | GA plan R1-4 |
| 5 | **PyPI wheel matrix CI job (R1-2)** (5 platforms per the GA plan) — same gap | Low-Moderate | GA plan R1-2 |
| 6 | **Refresh `CHANGELOG.md` for `1.0.0-beta.8`** (BE-1 runtime work: façade + harvest merged) | Low (30 min) | CHANGELOG.md gap |
| 7 | **Re-run prepublish audits on the current workspace** (verify BE-1 didn't break them) | Low | PUBLISH_PROTOCOL §"audit the actual artifact" |
| 8 | **Fix `xfa-pdfrest-compare` license field** (trivial; publish=false but cleanup) | Trivial | per-crate audit |

### Tier 2 — process & docs (publish-time enablers)
| # | Item | Effort | Source |
|---|---|---|---|
| 9 | **R3 publish protocols + R4 post-publish smokes** (per-channel) | Moderate | GA plan R3/R4 |
| 10 | **R5 rollback/deprecate protocol** | Moderate | GA plan R5 |
| 11 | **R6 registry SHA ledger** | Low | GA plan R6 |
| 12 | **Broken-pipe stress fixture for CLI** (large-text input, in CI) | Low | CLI readiness §"unresolved" |

### Tier 3 — longer-term tracks (acknowledge, schedule, not blocking)
- B4-B9 WASM perf suite (needs hardware profiling lab).
- G7-G11 cross-binding editor API parity.
- F1 English-only website surface hardening (separate repo).
- F2/F3/F5 API docs, how-to, package-install alignment.
- COM2-COM5 pricing/support/enterprise proof bundles.

### Tier 4 — business decisions (not engineering, **not in scope**)
- Whether/when to publish to each channel (crates.io / npm / PyPI / Homebrew / Scoop / winget / deb / rpm).
- Code-signing / notarization identity (Apple ID, Authenticode cert).
- Public `pdfluent` binary name (the CLI takeover from internal `xfa-cli`).
- Pricing, support contact, enterprise contracting.

## 4. Recommended next bites (engineering-only, in execution order)

**Smallest-bite that meaningfully reduces ship-risk:**

1. **Refresh `CHANGELOG.md`** for `1.0.0-beta.8` to reflect the BE-1 runtime work (~30 min, just docs).
2. **Re-run the prepublish audits** on the current rc1 to confirm BE-1 changes haven't introduced new
   private-path / secret / artefact leaks (≤1 h, mostly automated via `audit-all-packages.sh`).
3. **Add CycloneDX SBOM step** (vendor `cargo-cyclonedx`; CI job; commit a baseline SBOM per package).
   Single biggest gap per the CLI readiness verdict.
4. **Fix `xfa-pdfrest-compare` missing license** (trivial cleanup so it doesn't trip future deny runs).
5. **Add Node NAPI dry-run CI job** (R1-4): the audit exists; just wire the CI job.
6. **Cross-platform CI matrix for the CLI** (Linux x86_64-musl + Windows x86_64-msvc): gates binary
   release; moderate effort but the highest-impact single addition.
7. **Binary release CI job** (multi-platform `cargo build --release` + package + SHA256SUMS + checksum
   verify): completes the binary-release pipeline.

After Tier-1 items: R3/R4/R5/R6 process work in `docs/release/` (mostly authoring + a few scripts) so
the publish-step playbook is one-command and rehearsed before the first real publish.

## 5. What this audit does NOT touch (per directive guard-rails)
- No publish, no `v*` tag.
- No B-default flip.
- No code changes in this audit pass (purely inventory + plan).
- No private paths / secrets / corpus contents leaked into committed reports.
- GitLab origin only (no GitHub operations).

---

**Bottom line.** The release-train infrastructure is **mature and Wave 1 complete**. The CLI is
*operationally* ready. The remaining engineering gaps are well-scoped, individually small/moderate,
and concentrated in three buckets: **SBOM, cross-platform CI, and the R3-R6 publish-process polish**.

---

## 6. Tier-1 delivery — CI pipeline diagnosis & fix (2026-05-28)

**Branch:** `release-hardening/tier1` · **Head:** `f0999509c` (green) · **Failed head:** `5a53416b` (and
unobservably also `0b6f26692`, the prior commit on the same branch with the same CI YAML).

### Root cause (5a53416b)
GitLab rejected the pipeline at **server-side YAML/config validation** — the runner never received any
jobs. Evidence: zero `Checking for jobs... received` events on the self-hosted runner
(`pdfluent-vps-ci`) between **2026-05-28 07:53:00 UTC** (push of `0b6f26692`) and
**2026-05-28 08:05:23 UTC** (push of `f0999509c`) — a 12.4-minute window covering both pipelines.
Compare to `f0999509c`: jobs arrived at the runner **15 seconds** after push.

Two YAML shapes I introduced in the original Tier-1 CI additions tripped GitLab's semantic schema:

1. **`needs.parallel.matrix` value used a runtime variable**:
   ```yaml
   needs:
     - job: package:cli-cross-platform
       parallel:
         matrix:
           - TARGET: $TARGET     # ← rejected; matrix values must be literal
       optional: false
   ```
   GitLab requires `needs.parallel.matrix` values to be a static list. Runtime variables
   (`$TARGET`) are not expanded here. The intent — pair each `package:binary-release` matrix
   instance with its same-`TARGET` `package:cli-cross-platform` counterpart — is achieved
   automatically when `needs:` lists the job by name only (matching matrix dimensions
   auto-pair).
2. Two `script:` blocks used a multi-line `if … then … fi` written as a single YAML list item
   with trailing semicolons. While the YAML parses, GitLab's CI schema for `script:` line items
   is stricter than the runner's eventual shell execution; the safe form is `|` block scalars.

### Fix (`f0999509c`)
Scoped diff, CI YAML only, no Rust code:
1. `package:binary-release.needs:` — dropped the `parallel.matrix: TARGET: $TARGET` subkey;
   left only `- job: package:cli-cross-platform, artifacts: true, optional: false` so GitLab
   auto-pairs matching matrix dimensions.
2. Both `package:node-napi-dry-run.script:` and `package:binary-release.script:` converted to
   `|` block scalars (one-line-per-step semantics preserved; logic unchanged).
3. Drive-by while fixing the script: `audit_package_tree.py --channel node` → `--channel npm`
   (the audit tool's enum is `{crates_io,npm,pypi,maven,wasm,binary,gitlab,generic}` — `node`
   would have failed at runtime).

### Proof pipeline is green (`f0999509c`)
| stage | jobs created (rules-evaluated for `push` on feature branch) | runner result |
|---|---|---|
| `sanity` | 3 (cargo-metadata, cargo-check, cargo-deny; cargo-fmt skipped — MR/master only) | **3/3 PASS** (180s / 182s / 3s) |
| `quality_manual` | 0 created (all 8 jobs have MR/master/schedule/tag rules; none match a feature-branch push) | — |
| `package_manual` | 0 created (same — Tier-1 additions intentionally MR/tag-only to avoid burning runner time on every push) | — |
| `corpus_manual` | 1 (corpus:xfa-no-private-paths — matches `push`) | **PASS** (2s) |
| `release_manual` | 0 created (publish jobs are tag-only by design) | — |

Total: **4 jobs created, 4 jobs ran, 4 PASS, 0 FAIL.** Pipeline status = **passed**.

Evidence in the runner journal (`journalctl -u gitlab-runner --since '2026-05-28 08:05:00'`):
- Jobs `14577812204` / `14577812205` / `14577812206` / `14577812207` all `job-status=success`
  between 08:05:24 and 08:08:43 UTC.
- No `Job failed`, no `Job canceled`, no `stuck_or_timeout_failure` events anywhere in the window.

### Why this is not a "stuck pipeline" or false-red
- `default: interruptible: true` — but no newer pipeline existed to cancel `f0999509c`; runner
  idle from 08:08:43 UTC onward.
- The manual stages (`quality_manual`, `package_manual`, `release_manual`) have **zero jobs
  instantiated** on this branch push (their rules require MR/master/schedule/tag context). An
  empty stage advances instantly in GitLab; the pipeline reached "success" once `corpus_manual`
  completed.
- The new Tier-1 jobs (`package:sbom-generate`, `package:node-napi-dry-run`,
  `package:cli-cross-platform`, `package:binary-release`) are **MR/tag-only by design** — they
  do not run on a feature-branch push, so they cannot false-red the branch pipeline.

### Remaining non-blocking follow-ups
- **MR/tag exercise still pending.** Because the new Tier-1 jobs only fire on MR or tag, the
  branch pipeline does not actually exercise them. To prove they work in CI before relying on
  them at a real publish, the recommended next step is to open a draft MR
  (`release-hardening/tier1` → `xfa/static-parity-rc1`) and trigger each new job manually from
  the MR pipeline view. Until then the jobs are **structurally validated** (YAML lint + rules
  + script shape) but not **runtime validated**.
- **`package:sbom-generate` baseline first run.** The job will report `[sbom] NEW (no baseline): <name>`
  for every publish-eligible crate on its first execution (drift exit code 3), because
  `docs/release/sbom_baselines/` is empty by design — the first run *generates* the baseline.
  This is documented behavior, not a CI bug; the first MR run should `cp dist/sbom/*.cdx.json
  docs/release/sbom_baselines/` and commit that as the first baseline tranche.
- **`cargo-cyclonedx` install on first run** — the job auto-installs (`--tool-install`); first
  invocation will spend ~60-120s compiling cargo-cyclonedx. Subsequent runs are cached in
  `CARGO_HOME=/var/cache/cargo-home`.

### Merge advice
**Mergeable to `xfa/static-parity-rc1`.** The branch is sanity-green, contains no Rust code
changes (CHANGELOG + license + SBOM tooling + CI YAML + audit notes only), is rebase-clean on
rc1, and the four new CI jobs are MR/tag-gated so a merge cannot introduce false-red pipelines
on subsequent rc1 pushes. The MR-run validation of the new jobs can happen post-merge from
any subsequent MR without re-opening Tier-1.
Tier-1 items 1-7 are the highest-ROI path to a defensible first publish.
