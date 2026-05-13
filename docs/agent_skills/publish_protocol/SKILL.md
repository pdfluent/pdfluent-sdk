---
name: publish_protocol
description: Mandatory pre- and post-publish protocol for every PDFluent package publish, on every channel. Triggers whenever an agent is about to invoke `cargo publish`, `npm publish`, `twine upload`, `mvn deploy`, `gh release upload`, `glab release upload`, or any other command that pushes bytes to a package registry, release page, or distribution channel. The protocol is non-optional, channel-agnostic, and exists because a prior real-world licence-compliance incident (forked open-source crates published without `LICENSE-APACHE`/`LICENSE-MIT` files inside the tarball) had to be remediated by yanking and republishing.
---

# PDFluent publish protocol — agent instructions

You are a terminal agent about to publish a PDFluent artefact, or you are reviewing such a publish. Do not deviate from any rule below.

## 1. Hard rules — never override these

1. **Never publish from a dirty working tree.** `git status --porcelain` MUST return empty at the moment of publish. If it is non-empty, commit or stash, then re-confirm.
2. **Never use `--allow-dirty`** (cargo) or any equivalent "force" flag on any other channel.
3. **Never publish without a committed prepublish audit report** under `benchmarks/runs/prepublish_audits/<name>-<version>.md`. The report MUST exist on the publish branch tip before publish.
4. **Never trust the source tree alone.** The audit MUST inspect the actually-packaged artefact (the `.crate`, the `.tgz`, the `.whl`/`.tar.gz`, the `.jar`, the `.wasm` bundle, the installer). The PDFluent triggering incident was a case where the source tree had the licence files but they were excluded from the tarball.
5. **Never publish if the licence audit fails.** Manifest licence metadata must match what's physically inside the artefact. For `MIT OR Apache-2.0` forks: both `LICENSE-MIT` and `LICENSE-APACHE` must be present inside the artefact. For proprietary crates: the `LICENSE` file in the artefact must match the canonical sha256.
6. **Never publish if leakage patterns match.** Workstation paths (`/Users/`, `/home/`, `/opt/xfa`, `/mnt/storagebox`), corpus / oracle / VPS artefacts, secrets (`sk_live_…`, `AKIA…`, `ghp_…`, `glpat-…`, `AIza…`, `xox[abp]-…`, private-key blocks), or assignment-shaped credentials (`API_KEY="…"`, `SECRET="…"`, `TOKEN="…"`) inside the artefact tree are blockers.
7. **Never continue after a non-propagation publish failure.** A publish that errors for any reason other than "predecessor not yet on the registry index" is a stop-the-line event. Halt the train. Document. Ask the operator.
8. **Never un-yank a version that was yanked for licence non-compliance.** Un-yanking restores the original defect to the registry. Republish a fixed version instead.
9. **Never publish language bindings, npm, PyPI, Maven, WASM, or binary releases without operator authorisation for that channel.** Channel scope is per-task; do not infer authorisation across channels.
10. **Never include agent / model / vendor names** in commits, PR descriptions, audit reports, remediation reports, code comments, or crate metadata. Use neutral phrasing ("operator", "release captain").

## 2. The protocol in operational order

For every artefact you are about to publish:

### A. Confirm scope and clean state

```
git status --porcelain   # must be empty
git rev-parse HEAD       # record this in the audit report
```

If non-empty: stop, commit/stash, re-confirm.

### B. Run the channel's prepublish audit

For Rust crates:

```
scripts/release/prepublish_crate_audit.sh <crate-name>
```

The script:

- fails on dirty tree,
- runs `cargo package --list -p <crate>` and `cargo package -p <crate>`,
- unpacks the resulting `.crate`,
- verifies manifest licence metadata,
- verifies the required licence files (`LICENSE`, or `LICENSE-APACHE` + `LICENSE-MIT`, or `LICENSE` + `NOTICE`) are physically inside the tarball,
- invokes `scripts/release/audit_package_tree.py` to scan for leakage,
- writes a markdown audit report to `benchmarks/runs/prepublish_audits/<crate>-<version>.md`,
- exits non-zero on any blocker.

For other channels, follow the per-channel checklist:

- `docs/release/checklists/crates_io.md`
- `docs/release/checklists/npm.md`
- `docs/release/checklists/pypi.md`
- `docs/release/checklists/maven.md`
- `docs/release/checklists/wasm.md`
- `docs/release/checklists/binary_release.md`
- `docs/release/checklists/gitlab_package_registry.md`

For every channel, the generic helper is reusable:

```
scripts/release/audit_package_tree.py \
    --tree /tmp/unpacked-artefact \
    --out  benchmarks/runs/prepublish_audits \
    --package-name <name> \
    --package-version <version> \
    --channel <channel-tag>
```

### C. Commit the audit report

```
git add benchmarks/runs/prepublish_audits/<name>-<version>.md \
        benchmarks/runs/prepublish_audits/<name>-<version>.audit.md \
        benchmarks/runs/prepublish_audits/<name>-<version>.audit.json
git commit -m "release(audit): prepublish audit for <name> <version>"
```

### D. Run the channel dry-run

- `cargo publish -p <crate> --dry-run`
- `npm publish --dry-run --access public`
- `python -m twine check dist/*`
- `mvn -DskipTests -Pdeploy clean verify`
- WASM: `npm publish --dry-run` on the wrapper package.

Propagation-only failures (a downstream crate's dry-run fails because its upstream sibling isn't on the registry yet) are acceptable and must be documented in the audit report. Any other failure is a stop.

### E. Publish

```
cargo publish -p <crate>                      # crates.io
npm publish --access public                   # npm
python -m twine upload dist/*                 # PyPI
mvn -Pdeploy clean deploy                     # Maven
glab release upload <tag> <files>             # GitLab binary release
```

Never `--allow-dirty`.

### F. Run post-publish verification

For each artefact, before moving to the next:

1. Query the registry API for the just-published version.
2. Download the registry-distributed artefact from the public CDN.
3. Compare sha256 with the locally-built artefact.
4. Re-extract and verify the licence files are inside the downloaded artefact.
5. Run a clean consumer smoke build in `/tmp`.
6. Write a post-publish verification report using `docs/release/templates/POST_PUBLISH_VERIFY_TEMPLATE.md` and commit.

### G. On failure: remediate

Open a remediation report from `docs/release/templates/REMEDIATION_REPORT_TEMPLATE.md`. Yank only when the published artefact violates a hard rule. Prepare a fixed version. Re-run the prepublish audit on the fix. Republish. Re-verify.

If the defect class wasn't caught by the audit script, **update the audit script** in the same commit train so the same class can never slip through again.

## 3. Train discipline

When a publish train ships multiple artefacts in sequence:

- Run the audit per crate **before** any publish in the train begins, ideally via `scripts/release/release_train_guard.sh` (audit-only by default).
- Publish in declared dependency order: leaves first, downstream consumers last. Cargo will refuse to publish a crate whose immediate dependencies are not yet on the registry; treat this as the source of truth for ordering when documented order conflicts with cargo's view.
- Wait for index propagation between sequential publishes; cargo's `note: waiting for <crate> <version> to be available at registry` is acceptable, but a downstream publish that races the propagation will error.
- After each publish, run post-publish verification before starting the next. A train must not accumulate untested publishes.

## 4. What stops you

You stop, do not publish, and ask the operator if:

- The audit script returned non-zero for any reason.
- The audit report cannot be produced (e.g. cargo build error, helper script missing).
- A licence file required by §5 of `PUBLISH_PROTOCOL.md` is missing from the artefact.
- Any leakage pattern matches.
- A dependency resolves to a yanked version through any required feature path.
- The dry-run fails with anything other than a documented propagation case.
- A publish fails with anything other than a documented propagation case.
- The artefact size is unusually large (≥ 2× last published version) with no recorded waiver.
- The downloaded artefact at post-publish drift-checks against the local artefact.

## 5. Authoritative documents

If anything in this skill contradicts `docs/release/PUBLISH_PROTOCOL.md`, the protocol document wins. Update this skill to match in the same commit that updates the protocol.

- Policy: `docs/release/PUBLISH_PROTOCOL.md`
- Channel checklists: `docs/release/checklists/<channel>.md`
- Templates: `docs/release/templates/`
- Audit script: `scripts/release/prepublish_crate_audit.sh`
- Tree helper: `scripts/release/audit_package_tree.py`
- Train guard: `scripts/release/release_train_guard.sh`
