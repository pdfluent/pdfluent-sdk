# Post-publish verification — `<package-name> <version>` (<channel>)

> Template — copy into `benchmarks/runs/post_publish_verify/<package-name>-<version>.md` and fill every section. Governed by `docs/release/PUBLISH_PROTOCOL.md` §11.
>
> Delete the blockquote helpers before committing.

## Inputs

- Channel: `crates_io` | `npm` | `pypi` | `maven` | `wasm` | `binary` | `gitlab` | `generic`
- Package name: `<name>`
- Published version: `<x.y.z>`
- Publish timestamp (UTC): `<ISO-8601>`
- Operator: `<role>`
- Prepublish audit report: `benchmarks/runs/prepublish_audits/<name>-<version>.md`

## Registry-side check

- Registry API endpoint queried: `<url>`
- Response status: `200`
- `newest_version` reported by API: `<x.y.z>` — matches target version
- Listed dependencies in registry match local: yes / no

## Downloaded artefact verification

- Source URL: `<static.crates.io / registry CDN / etc.>`
- Downloaded size: `<bytes>` (matches local artefact size: yes/no)
- Downloaded sha256: `<hex>` (matches local artefact sha256: yes/no)
- Licence files present in downloaded artefact: `<list>`
- Licence file sha256 matches prepublish audit: yes / no

## Consumer smoke

- Smoke project location: `/tmp/smoke-<name>-<version>/`
- Smoke command: `<channel-specific install + minimal usage>`
- Result: PASS / FAIL — `<excerpt>`
- Resolution time observed: `<seconds>`

## Drift detection

- Any difference between locally-built artefact and registry-distributed artefact: yes / no
- If yes: details of the diff and whether it is benign (e.g. metadata-only) or a defect requiring remediation.

## Verdict

`POST_PUBLISH_VERIFY_PASS` _or_ `POST_PUBLISH_VERIFY_FAIL`.

If FAIL: open a remediation report using `REMEDIATION_REPORT_TEMPLATE.md`.

---

Governed by `docs/release/PUBLISH_PROTOCOL.md` §11.
