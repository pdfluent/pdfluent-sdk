# Prepublish audit — `<package-name> <version>` (<channel>)

> Template — copy into `benchmarks/runs/prepublish_audits/<package-name>-<version>.md` and fill every section. Governed by `docs/release/PUBLISH_PROTOCOL.md`.
>
> Delete the blockquote helpers before committing.

## Inputs

- Repo: `<absolute path>`
- Channel: `crates_io` | `npm` | `pypi` | `maven` | `wasm` | `binary` | `gitlab` | `generic`
- Package name (channel-native, e.g. crates.io published name): `<name>`
- Package version: `<x.y.z>`
- Artefact path (the file that will be uploaded): `<path>`
- Artefact size: `<bytes>`
- Artefact sha256: `<hex>`
- Working tree commit: `<git rev-parse HEAD>`
- Working tree state at audit start: `git status --porcelain empty` (must be true)
- Dry-run result: PASS / N/A — `<command>` `<output excerpt>`

## Manifest licence metadata

- Field in manifest: `license = "<spdx>"` _or_ `license-file = "<filename>"`
- Required licence file(s) per channel + licence model: `<list>`
- Canonical sha256(s) for proprietary licence files: `<hex(s)>`

## Required licence files present in artefact

| File              | Size (B) | sha256 | Notes |
|-------------------|---------:|--------|-------|
| `LICENSE`         |          |        |       |
| `LICENSE-APACHE`  |          |        |       |
| `LICENSE-MIT`     |          |        |       |
| `NOTICE`          |          |        |       |

(Include only the rows that apply. Every required file must show PASS — no missing rows.)

## Package contents (`<channel-specific list command>`)

```
<paste the file-list output here, or link to a saved snapshot file>
```

## Tree audit (`scripts/release/audit_package_tree.py`)

- Helper exit code: `0` (clean) / `1` (blockers) / `2` (usage)
- JSON report: `<benchmarks/runs/prepublish_audits/<name>-<version>.audit.json>`
- Markdown report: `<benchmarks/runs/prepublish_audits/<name>-<version>.audit.md>`
- Blockers: `<count>`
- Warnings: `<count>`

## Dependency / yanked-version check

- Direct dependencies pinned non-yanked: yes / no
- Transitive `=`-pinned normal-kind deps non-yanked: yes / no
- Optional / dev-only `=`-pinned yanked deps (if any): listed below with rationale why unreachable
  - `<crate> =<version>` — reason: …

## Channel-specific checks

- [ ] Channel checklist run end-to-end: `docs/release/checklists/<channel>.md`
- [ ] Dry-run command invoked: `<command>`
- [ ] Dry-run output: PASS / propagation-only blocked / FAIL — `<excerpt>`

## Size sanity

- Compressed artefact size: `<bytes>` (cap: 50 MiB unless waived)
- Unpacked tree size: `<bytes>`
- Growth vs previous published version: `<+x.y%>` (or `n/a` for first publish)
- Waiver required? yes/no. If yes: rationale and approver.

## Verdict

`AUDIT_PASS` _or_ `AUDIT_BLOCKED` (and which §14 bullet in PUBLISH_PROTOCOL.md triggered).

If `AUDIT_PASS`: operator may proceed to publish per the channel checklist.
If `AUDIT_BLOCKED`: do not publish. Open a remediation report.

---

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Committed alongside the publish commit.
