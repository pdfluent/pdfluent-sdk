# PDFluent release train — operator workflow

This document is the operator's day-to-day reference for the release train.
It assumes the multi-channel infrastructure (canonical manifest, drift
detector, version matrix, install snippets, promotion orchestrator) is in
place. See `docs/release/PUBLISH_PROTOCOL.md` for the deeper protocol and
`docs/release/canonical_releases.toml` for the source-of-truth manifest.

## TL;DR — the four commands

```bash
# 1. What's the current state across every channel?
python3 scripts/release/release_train.py matrix

# 2. Is there drift between manifest and live?
python3 scripts/release/release_train.py drift          # exit 1 on FAIL

# 3. Per-channel health dashboard (markdown)
python3 scripts/release/release_train.py health

# 4. Generate install snippets for the website / docs
python3 scripts/release/release_train.py snippets --format json
python3 scripts/release/release_train.py snippets --format md
```

## Source-of-truth manifest

`docs/release/canonical_releases.toml` is the only authoritative answer to
"which version of X is released on channel Y". Every other tool reads from
it (drift detector, version matrix, install-snippet generator, channel
health report, website-demo sync).

Editing the manifest is a focused commit — the drift detector will validate
the change against live registry state on the next CI run.

## Daily operator workflow

| When | What | How |
|---|---|---|
| Pushing to master | 5 pre-push gates (metadata · fmt · build · clippy · licenses) | automatic |
| Scheduled (CI cron) | Drift detector | `bash scripts/ci/run_release_drift_check.sh` |
| Before a release | Per-channel pre-publish gates | `bash scripts/release/prepublish_crate_audit.sh <crate>` for Rust; channel-specific tools for others |
| During a release | Multi-channel promotion | `bash scripts/release/promote_release.sh --version 1.0.0-beta.N --channels crates_io,npm_wasm,...` |
| After a release | Verify drift is zero | `python3 scripts/release/release_train.py drift` |
| Website demo lag | Refresh vendored WASM | (operator runs sync script in `pdfluent-website` repo — see below) |

## Promotion workflow — releasing version `X` across channels

1. **Edit the manifest first.** In `docs/release/canonical_releases.toml`, bump `expected`, `pin`, `published_at`, `registry_url` for each channel you intend to release on.
2. **Commit the manifest change** with the rationale.
3. **Run the orchestrator** (skeleton today; channel branches delegate to existing tooling):
   ```bash
   bash scripts/release/promote_release.sh \
     --version 1.0.0-beta.N \
     --channels crates_io,npm_wasm,pypi,nuget,maven,gitlab_generic
   ```
4. **Verify drift is zero:** `python3 scripts/release/release_train.py drift`.
5. **Refresh the website demos** (separate repo, separate branch — see below).
6. **No git tag is created.** Tags are reserved for explicit milestone moments per the "no tags without explicit approval" policy.

## Channel-specific notes

| Channel | Publish tool | Smoke command | Ledger file |
|---|---|---|---|
| crates_io | `cargo publish -p <crate>` | `cargo new + cargo add` | `sha_ledger/crates_io.json` |
| npm_wasm | `npm publish <tgz>` | `npm install` + ESM import | `sha_ledger/wasm.json` |
| npm_napi | `npm publish` per platform sub-package + main | `npm install` per platform | `sha_ledger/npm.json` |
| pypi | `twine upload <wheel>` | `pip install` + `import` | `sha_ledger/pypi.json` |
| nuget | `dotnet nuget push <nupkg>` | `dotnet add package` | `sha_ledger/nuget.json` |
| maven | `mvn -P release deploy` (Central Publishing Plugin, autoPublish=false) → operator clicks Publish in Portal | (resolves via `mvn install`) | `sha_ledger/maven.json` |
| gitlab_generic | `curl --request PUT -H PRIVATE-TOKEN ... /packages/generic/<pkg>/<v>/<file>` | `curl GET` + sha verify | `sha_ledger/gitlab.json` |

## Website demo update workflow

The `pdfluent-website` repo vendors the WASM SDK as byte-identical
build artefacts (NOT a runtime npm dep). After every WASM publish:

```bash
cd ~/Documents/pdfluent-website

# (Reach a clean commit on the deploy branch first)

# Refresh vendored WASM to the just-published version
scripts/wasm/sync-sdk-wasm-from-npm.sh --version 1.0.0-beta.N

# Verify byte-identity (canonicalness gate)
scripts/wasm/check-sdk-wasm-canonical.sh

# Run demo smoke (Playwright)
npm run test:e2e -- --reporter=line

# Commit explicit paths (no `git add -A`)
git add public/wasm/sdk-wasm-manifest.json src/lib/wasm
git commit -m "chore(wasm): refresh manifest to 1.0.0-beta.N"
git push gitlab <branch>

# When ready: scripts/deploy/manual-cloudflare-deploy.sh   (operator approval)
```

The `scripts/release/check_website_wasm_lag.sh` (in this SDK repo) is a
read-only drift check that compares the website's pinned WASM version vs
the npm `latest` dist-tag. Wire it into a cron job.

## Recovery workflow — when drift detector flags FAIL

The drift detector ran in CI and the pipeline failed. What to do:

1. **Look at the JSON output.** `bash scripts/ci/run_release_drift_check.sh --json` shows exactly which channel drifted in which direction.
2. **Determine the cause:**
   - **Channel lags manifest** (channel < expected) → the publish didn't reach this channel. Re-run the publish (or use `promote_release.sh`).
   - **Channel ahead of manifest** (channel > expected) → someone published externally without updating the manifest. Update the manifest.
3. **Edit `docs/release/canonical_releases.toml`** to reflect reality.
4. **Re-run drift detector** to confirm 0 FAIL.
5. **Commit + push the manifest edit.**

## Rollback workflow — un-shipping a broken release

Once a version is on a registry, you generally **cannot delete it** (npm has 72h, PyPI has 0 unless admin, crates.io is append-only, etc.). The rollback path:

1. **Yank the version** on each affected channel:
   - `cargo yank --version X.Y.Z <crate>`  *(crates.io)*
   - `npm deprecate "@pdfluent/X@1.0.0-beta.N" "Deprecated: yanked due to <reason>"`  *(npm — beyond 72h)*
   - PyPI: file a yank via PyPI UI (operator only)
   - NuGet: unlisted via NuGet UI
   - Maven: cannot yank — file a `relocation.xml` and publish a replacement
   - GitLab Generic Package: `curl --request DELETE` via API
2. **Record the yank in the ledger** via `scripts/release/ledger_mark_yanked.py`.
3. **Document the yank** at `docs/release/yanks/<package>_<version>_yank_notice.md`.
4. **Publish the replacement version** (bump beta.N → beta.N+1 with the fix). See the promotion workflow.
5. **Update `docs/release/canonical_releases.toml`** to point at the replacement.
6. **Drift detector** must return 0 FAIL again before declaring rollback complete.

## Editor releases — explicitly DECOUPLED

Editor (Tauri desktop app) releases are NOT triggered by SDK/WASM
releases. See `docs/release/EDITOR_DECOUPLING_POLICY.md`. The release
train deliberately separates these two cadences.

## Future automation (recommended)

- **Scheduled drift CI**: a GitLab cron pipeline that runs `bash scripts/ci/run_release_drift_check.sh` daily, alerting on FAIL.
- **Manifest-driven snippet generation**: have the website read `release_train.py snippets --format json` directly at build time, eliminating hand-edited install commands in `docsChannels.ts`.
- **Auto-update MR**: a bot that, on a new external publish (e.g. WASM beta.N+1), opens an MR updating the manifest's `expected` field.
- **Per-channel CI orchestrator**: replace `promote_release.sh`'s stub branches with actual channel-publish logic, so the whole release becomes a single command.
