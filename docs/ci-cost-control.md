# CI Cost Control

GitHub Actions runs only the cheap PR sanity gate. Expensive validation
(WASM build, C bindings, desktop UI, conversion quality, corpus
regression) runs off-CI on a VPS or local box. This keeps the per-PR
GitHub spend low and predictable.

## Two-tier model

| Tier | Where | Trigger | Purpose |
|------|-------|---------|---------|
| Cheap PR gate | GitHub Actions (`.github/workflows/ci.yml`) | every PR + master push | fast sanity: compile, test, clippy, fmt |
| Expensive validation | VPS / local (`scripts/validate-expensive.sh`) | manual, before merge / release | wasm, bindings, desktop, conversion, corpus |
| Manual fallback | GitHub Actions (`.github/workflows/expensive-validation.yml`) | `workflow_dispatch` | noodknop when no VPS is available |

## Cheap PR gate (GitHub Actions)

Jobs in `ci.yml`: `check`, `test`, `clippy`, `fmt`. All run in parallel
on `ubuntu-latest`. Combined billable time is ~5–6 minutes per push.
Estimated cost: ~$0.04–0.05 per push, ~$0.20–0.25 per PR (5 pushes
average).

Not in the gate (intentionally):

- `cargo audit` — RustSec advisories on transitive deps cause baseline
  noise. A separate audit follow-up will reintroduce it once the
  baseline is green; tracked outside of #1380.
- `wasm`, `binding-tests`, `desktop-tests`, `conversion-tests`,
  `corpus-regression` — cost-prohibitive on every push. Moved to
  the off-CI script and the manual fallback workflow.

## Expensive validation (VPS / local)

Run before merging large PRs, before releases, or when changes touch:

- WASM surface (`crates/xfa-wasm` and its dep tree)
- C / PyO3 bindings (`crates/pdf-capi` and its dep tree)
- Desktop app (`crates/pdf-desktop`, `crates/pdfluent`)
- Rendering / XFA pipeline
- Conversion crates (`pdf-docx`, `pdf-xlsx`, `pdf-pptx`)

### Usage

```bash
# Validate a branch (fetches origin first by default)
./scripts/validate-expensive.sh origin/m8-feature-branch

# Validate a specific commit
./scripts/validate-expensive.sh 91b7fee30

# Run only WASM + bindings, fail fast
PHASES=A,B FAIL_FAST=1 ./scripts/validate-expensive.sh master

# Offline mode (skip git fetch)
SKIP_FETCH=1 ./scripts/validate-expensive.sh HEAD
```

Phase letters: `A=wasm`, `B=bindings`, `C=desktop`, `D=conversion`,
`E=corpus`.

Output goes to `validation-reports/<safe-ref>/<UTC-timestamp>/` with
per-phase logs and a `summary.txt`. Exit code is non-zero on any
failure.

### Posting a summary back to a PR

```bash
gh pr comment 1234 --repo jasperdew/xfa-native-rust \
  --body "$(printf 'VPS validation result\n\n```\n%s\n```\n' \
              "$(cat validation-reports/origin_my-branch/<ts>/summary.txt)")"
```

This costs zero Actions minutes.

## Manual fallback workflow

`.github/workflows/expensive-validation.yml` accepts `workflow_dispatch`
with `ref` and `phases` inputs. Use only when no VPS is available — it
incurs Actions cost.

```bash
gh workflow run expensive-validation.yml \
  --repo jasperdew/xfa-native-rust \
  -f ref=my-branch \
  -f phases=wasm,corpus
```

The fallback workflow is **not** required for merge.

## Why no path-gating?

An earlier iteration of #1380 used `dorny/paths-filter@v3` to gate the
expensive jobs by changed paths. Codex review flagged three P1
false-skip risks: the `wasm`, `capi`, and `rendering` filters omitted
transitive workspace dependencies, so a change to a transitively-used
crate would silently skip the relevant validation.

Maintaining accurate path filters means tracking the entire workspace
dep graph forever, and the failure mode is invisible (a green PR that
should not have been green). Splitting into a cheap gate plus an
explicit off-CI heavy run removes the entire class of bugs.

## Branch protection

Required checks for merge to `master`:

- `Check`
- `Test`
- `Clippy`
- `Format`

Remove from required checks (these no longer run on PR):

- `WASM Build`
- `Binding Tests (C API)`
- `Desktop App Tests`
- `Conversion Quality Tests`
- `Corpus Regression Tests`
- `Security Audit` (until audit follow-up reintroduces it)
- `Detect Changed Paths` (no longer exists)

The branch-protection update is a one-time repo-settings change after
merging #1380.

## Cost summary

| Path | Cost / PR | Cost / week (≈5 PRs) |
|------|-----------|----------------------|
| Old (10 always-on jobs) | ~$0.40–0.60 | ~$2.00–3.00 |
| **New (cheap gate)** | **~$0.20–0.25** | **~$1.00–1.25** |
| Manual `expensive-validation.yml` per run | ~$0.10–0.15 | n/a |
| Nightly (existing) | n/a | ~$0.70 |
