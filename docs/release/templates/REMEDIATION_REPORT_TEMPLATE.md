# Remediation report — `<incident-id>` (`<package> <version>` on `<channel>`)

> Template — copy into `benchmarks/runs/remediation/<incident-id>.md` and fill every section. Governed by `docs/release/PUBLISH_PROTOCOL.md` §12–13.
>
> Delete the blockquote helpers before committing.

## Incident summary

- Incident id: `<YYYY-MM-DD-shortslug>` (e.g. `2026-05-13-licence-missing-tarball`)
- Channel: `crates_io` | `npm` | `pypi` | `maven` | `wasm` | `binary` | `gitlab` | `generic`
- Affected package(s): `<name>` `<version>` (one row per affected artefact)
- Detected at (UTC): `<ISO-8601>`
- Detected by: `<operator role / monitor / consumer report>`
- Defect class: `<licence-file-missing | secret-leak | corpus-asset-leak | yanked-dep-pin | size-overrun | signature-failure | other>`

## Detection mechanism

- How was the defect detected? (audit script, post-publish check, downstream consumer report, security scanner, etc.)
- Was the defect class previously catchable by the audit script? yes / no
- If no: which check needs adding to prevent recurrence?

## Impact

- Number of downstream consumers known/likely affected: `<count or n/a>`
- Days exposed (defect-published timestamp → remediation timestamp): `<duration>`
- Legal / compliance dimension (e.g. Apache-2.0 §4(b) violation): `<yes/no + details>`
- Security dimension: `<yes/no + details>`
- User-visible behaviour change: `<yes/no + details>`

## Containment

- Yank executed? yes/no. If yes:
  - Versions yanked: `<list>`
  - Yank timestamp: `<UTC>`
  - Yank command: `<exact command used>`
- Take-down / deprecation executed (where yanking is unavailable)? yes/no. Details: `<…>`
- Public communication (release notes update, advisory, mailing list): `<links / drafts>`

## Replacement

- Replacement version(s) prepared: `<name> <new-version>` (one row per artefact)
- Bump rationale (semver classification): `<patch | minor | major | pre-release continuation>`
- Re-run prepublish audit on replacement: PASS — `<benchmarks/runs/prepublish_audits/<name>-<new-version>.md>`
- Re-run post-publish verification on replacement: PASS — `<benchmarks/runs/post_publish_verify/<name>-<new-version>.md>`

## Process update (mandatory if the audit script didn't catch it)

- Audit-script change applied: `<commit hash>` _or_ `<scheduled follow-up issue>`
- Test added that fails on a known-bad fixture: `<commit hash + path>`
- Protocol document update applied: `<diff to PUBLISH_PROTOCOL.md>`
- Per-channel checklist update applied: `<diff to docs/release/checklists/<channel>.md>`

## Timeline

- `<UTC ts>` — defect published / event
- `<UTC ts>` — detected
- `<UTC ts>` — train halted
- `<UTC ts>` — yank executed
- `<UTC ts>` — replacement audited
- `<UTC ts>` — replacement published
- `<UTC ts>` — replacement post-publish verified
- `<UTC ts>` — protocol / tooling update committed

## Verdict

`REMEDIATION_COMPLETE` once every row above is filled and signed off.

---

Governed by `docs/release/PUBLISH_PROTOCOL.md` §12–13.
