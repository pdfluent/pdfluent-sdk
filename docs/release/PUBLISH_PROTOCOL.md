# PDFluent Publish Protocol

Status: **mandatory**, repo-rooted, applies to every publish to every channel.
Owner: release captain (or the operator running the publish train).
Last updated: 2026-05-13 (instituted after the Phase 5b post-publish licence incident).

---

## 1. Scope

This protocol governs **every** publish step to **every** package channel, including but not limited to:

- `crates.io` (Rust crates)
- `npm` (Node.js packages)
- `PyPI` (Python packages)
- Maven Central / GitHub Packages Maven (Java)
- WASM packages (npm-bundled or stand-alone)
- GitLab package registry (internal mirrors)
- binary release artifacts (GitHub Releases / GitLab Releases / signed installers)
- any future public or private package channel that distributes PDFluent code or compiled artifacts

The rule is channel-agnostic. If a publish step pushes bytes that end up in someone else's dependency graph or installer, this protocol applies.

## 2. Hard rule

> **No package may be published unless the prepublish audit passes and leaves a committed audit report.**
> **No exceptions.**

The audit is **not** "the source tree looks fine." The audit is **"the actual packaged artifact passes every check below."**

If an agent or operator cannot complete the audit, they stop. They do not publish.

## 3. The triggering incident

Phase 5b shipped seventeen Rust crates to crates.io. Five of those crates were forked open-source code under `MIT OR Apache-2.0`. Their `Cargo.toml` listed the correct SPDX identifier, but the published `.crate` tarballs did **not** contain the required `LICENSE-APACHE` and `LICENSE-MIT` files. Apache-2.0 §4(b) requires redistribution to include the licence text; the absent files put PDFluent in non-compliance the moment those artifacts went public.

Remediation cost: yank five versions, prepare five new patch versions, re-audit, re-publish, re-verify, and update consumers. Engineering cost was significant; the reputational cost would have been larger if it had stayed in the registry longer.

This protocol exists so that incident cannot repeat.

## 4. Mandatory audit requirements (every channel)

Before a single `cargo publish` / `npm publish` / `twine upload` / `mvn deploy` / `gh release upload` / etc. is invoked, the operator MUST:

| # | Step                                                                                             | Stop-the-line if not met                  |
|---|--------------------------------------------------------------------------------------------------|--------------------------------------------|
| 1 | Working tree is clean. `git status --porcelain` is empty.                                         | yes                                       |
| 2 | All publish-bound changes are committed on the publish branch.                                    | yes                                       |
| 3 | `--allow-dirty` is **not** used. This flag is banned for production publish.                     | yes                                       |
| 4 | The packaged artifact is built locally with the exact same command the publish step will run.    | yes                                       |
| 5 | The packaged artifact is unpacked into a temporary directory and **its contents** are inspected. | yes                                       |
| 6 | Licence metadata (`license` / `license-file` / SPDX) is set in the manifest and matches policy.  | yes                                       |
| 7 | Required licence text files are physically present inside the packaged artifact.                 | yes                                       |
| 8 | No corpus / oracle / VPS / workstation-absolute-path / API-key / private-data leakage.           | yes                                       |
| 9 | No yanked dependency version is reachable through the dependency graph for any feature path.     | yes                                       |
| 10| Package size is within sanity bounds (default ≤ 50 MiB per artifact unless explicitly allowed).  | yes if exceeded without written waiver    |
| 11| A `dry-run` is run where the channel supports it, and resolves cleanly.                          | yes if the channel supports a dry-run      |
| 12| An audit report is written under `benchmarks/runs/prepublish_audits/` and committed.             | yes                                       |

If any line says "yes" and the check fails, the publish step does not start.

## 5. Licence compliance requirements

### 5.1 Two-layer check

Every publish requires both layers to be true:

1. **Manifest layer.** The package manifest declares the licence:
   - SPDX expression in `[package].license` (for crates.io / npm / PyPI), or
   - `[package].license-file = "LICENSE"` for proprietary licences not in the SPDX list.
2. **Tarball layer.** The actually-distributed tarball/zip/jar contains the licence text:
   - **`MIT OR Apache-2.0`** dual-licensed forked code: both `LICENSE-MIT` and `LICENSE-APACHE` files inside the tarball, plus any required `NOTICE` files. This is mandatory; the original Phase 5b incident was a violation of this rule.
   - **MIT only**: `LICENSE` (or `LICENSE-MIT`).
   - **Apache-2.0 only**: `LICENSE` and `NOTICE`.
   - **PDFluent Commercial Licence (proprietary)**: `LICENSE` with the full licence body and `license-file = "LICENSE"` in the manifest.

A manifest declaration without the corresponding file is a compliance defect, full stop.

### 5.2 Canonical fingerprints

For licence files that are intended to be byte-identical across many crates (e.g. the PDFluent Commercial Licence), record the canonical sha256 in the audit report. Drift triggers a stop.

### 5.3 NOTICE files

If an upstream fork (or any included third-party code) ships a `NOTICE`, that `NOTICE` ships with the package. Apache-2.0 §4(d) requires it.

### 5.4 Re-licensed forks

If we re-licence a forked crate under the PDFluent Commercial Licence, the audit report must record the upstream licence(s) it derives from and confirm we have either (a) substantial original modifications meeting the threshold for relicensing, or (b) written permission. The audit is mandatory for every forked crate at every publish.

## 6. Package / tarball inspection requirements

For each artifact, the audit MUST verify:

- **Filename allow-list**: only files explicitly produced by `cargo package` / `npm pack` / `python -m build` / `mvn package` / equivalent. No stray dotfiles. No editor temp files.
- **No absolute workstation paths** anywhere inside text files: `/Users/`, `/home/`, `/opt/xfa`, `/mnt/storagebox`, `/tmp/`.
- **No oracle / corpus / VPS artefacts**: no `*.pdf` / `*.png` / `*.json` from corpus directories, oracle baseline directories, or VPS mirror caches unless explicitly allowlisted in the audit report.
- **No secrets / keys**: regex screen for `sk_live`, `sk_test`, `AKIA[A-Z0-9]{16}`, `ghp_[A-Za-z0-9]{30,}`, `AIza[A-Za-z0-9_-]{30,}`, `API_KEY`, `SECRET`, `TOKEN`, plus the explicit string patterns the helper script enumerates.
- **No private workspace members**: any workspace member with `publish = false` that would otherwise be included must not appear in the dependency closure of the published artifact.
- **No yanked dependency** is hard-pinned through any feature path the artifact exposes. Yanked optional deps are permitted only if unreachable through any consumer's feature activation; the audit must explicitly note the optional+unreachable conclusion.

## 7. Clean tree rule

`git status --porcelain` MUST be empty at the moment of the publish call. The audit script enforces this; the protocol document re-states it because it is the single rule that catches the largest class of "wait, what version did I actually publish?" mistakes.

`--allow-dirty` is banned for any publish that produces a public-channel artifact. It is permitted only for the internal `--dry-run` exploration step, and even then the operator should prefer a clean state.

## 8. Dependency / yanked-version checks

The audit MUST query the registry index (crates.io / npm / PyPI / etc.) and verify:

- Every direct dependency pinned by the artifact has a non-yanked version that satisfies the pin.
- Every transitive normal-kind dependency pinned with an exact `=` constraint resolves to a non-yanked version.
- Every optional / dev-only / feature-gated dependency pinned with an exact `=` constraint is either non-yanked or documented in the audit as "yanked but unreachable through any activated feature path".

If a yanked version is reachable for any feature, the publish stops and remediation begins.

## 9. Package size sanity

Each artifact has a default cap of **50 MiB compressed**. Exceeding the cap requires a written waiver inside the audit report explaining why the package is unusually large (e.g. bundled test corpus, embedded fonts). Binary release artifacts (installers) may exceed the cap but must still be size-bounded with a recorded value.

Unusual size growth (≥ 2× last published version) is itself a stop-the-line trigger pending operator inspection.

## 10. Dry-run requirement

Every channel that supports a dry-run MUST be exercised:

- `cargo publish --dry-run`
- `npm publish --dry-run`
- `python -m twine check dist/*` (PyPI)
- `mvn deploy -DperformRelease=true -DskipTests -Dgpg.skip=true` (with `-DaltDeploymentRepository` to a temp dir)
- WASM publishing dry-run via the npm tooling that wraps it

Channels that do not have a real dry-run (some binary release flows) still require:

- producing the artifact locally,
- unpacking it,
- running the audit script against the unpacked tree.

## 11. Post-publish verification

After publish, the operator MUST:

1. Query the registry API for the just-published version.
2. Download the actually-distributed artifact (e.g. `static.crates.io/crates/<name>/<name>-<version>.crate`) and re-verify it bit-for-bit against the local artifact and the audit report.
3. Confirm licence files are present in the downloaded artifact.
4. Run a fresh consumer smoke build in `/tmp` against the published version.
5. Write a post-publish verification report under `benchmarks/runs/post_publish_verify/` (or the phase's `benchmarks/runs/PHASE*_ROUND*_*.md` for in-train work).

Post-publish drift between the audited local artifact and the registry-distributed artifact is treated as a defect.

## 12. Emergency remediation procedure

If a publish ships a defective artifact:

1. **Stop the train.** Halt any further publishes in the same group until the defect is understood and contained.
2. **Yank the defective version** if a remediation path exists (see §13 for when yanking is appropriate).
3. **Document the defect** in `benchmarks/runs/remediation/<incident>.md` using the remediation template.
4. **Prepare the fix** in a new patch/minor version with the issue corrected.
5. **Re-run the full prepublish audit** on the fixed version — including the check that the defect class is now caught.
6. **Republish** the fixed version.
7. **Re-run post-publish verification** on the new version.
8. **Update the protocol** if the defect class was not previously caught. The next audit script run should fail before reaching publish for an analogous defect.

## 13. Yanking policy

Yank only when:

- The published artifact violates a hard rule of this protocol (e.g. missing licence file), or
- The published artifact would lead users into a known-broken state that a fixed version supersedes.

Do **not** yank to "tidy up" old pre-release versions. Yanking has a real cost for downstream consumers who have already pinned a version.

Do **not** un-yank a version that was yanked for licence non-compliance. Republish a fixed version instead. Un-yanking a non-compliant artifact re-exposes the original compliance defect.

Record every yank in the remediation report with: the version yanked, the reason, the replacement version, and the date.

## 14. Stop-the-line conditions

The audit script and the operator both stop immediately on any of:

- `git status --porcelain` non-empty.
- The packaged artifact cannot be produced.
- Any licence / NOTICE file required by §5 is missing from the packaged artifact.
- Any leakage pattern from §6 matches inside the packaged artifact.
- Any feature path resolves to a yanked dependency.
- Package size exceeds the cap without a recorded waiver.
- The dry-run fails for any reason other than a documented "predecessor not yet published" propagation case.
- A publish step fails for any reason other than a documented propagation case.

For each stop, write a `STOPPED.md` line at the bottom of the audit report stating which §14 bullet triggered and what was decided.

## 15. Where this protocol lives in the repository

- Policy: `docs/release/PUBLISH_PROTOCOL.md` (this file).
- Per-channel checklists: `docs/release/checklists/<channel>.md`.
- Templates: `docs/release/templates/`.
- Agent instructions: `docs/agent_skills/publish_protocol/SKILL.md`.
- Audit script (Rust): `scripts/release/prepublish_crate_audit.sh`.
- Generic helper: `scripts/release/audit_package_tree.py`.
- Train guard: `scripts/release/release_train_guard.sh`.
- Audit outputs: `benchmarks/runs/prepublish_audits/<crate>-<version>.md`.
- Remediation reports: `benchmarks/runs/remediation/<incident>.md`.

## 16. Authoritative source

If a procedure in any release-related document conflicts with this protocol, **this protocol wins** until it is itself updated. Updates to this protocol require a commit explaining the change and a corresponding update to the audit tooling that enforces it.
