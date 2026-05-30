# Website demo ↔ SDK/WASM release integration

**Mandatory release step after every WASM publish, before deploying website.**

The website at `pdfluent.com` consumes the `@pdfluent/sdk-wasm` npm package
**as a vendored copy** (not a runtime npm dependency), kept byte-identical to
the canonical npm tarball via two existing scripts in `pdfluent-website/`:

| Script | Purpose |
|---|---|
| `scripts/wasm/sync-sdk-wasm-from-npm.sh` | Fetch `@pdfluent/sdk-wasm@<version>` from npm, copy the JS/d.ts/.wasm files into `src/lib/wasm/` and `public/wasm/`, write provenance to `public/wasm/sdk-wasm-manifest.json`. |
| `scripts/wasm/check-sdk-wasm-canonical.sh` | Fail if vendored files have drifted from the manifest. Wired into `scripts/deploy/manual-cloudflare-deploy.sh` as a hard gate. |

## Where the WASM is consumed in the website

- `src/lib/useWasm.ts` — singleton WASM loader: `import init, { PdfDoc, XfaEngine } from './wasm/xfa_wasm.js'`. Every demo (`src/components/demos/*.tsx`) imports from this.
- `src/config/docsChannels.ts` line ~69 — `installCommand: 'npm install @pdfluent/sdk-wasm@<version>'` (display-only string in the docs/install panel).

So **two places carry the version**:
1. Vendored binaries under `src/lib/wasm/` + `public/wasm/` (the actual runtime).
2. The install-command string in `docsChannels.ts` (display only).

A version bump must update **both** + the manifest sha for byte-identity.

## Canonical post-publish step (after `cargo publish` for the WASM-publishing pipeline OR after any `@pdfluent/sdk-wasm` npm publish)

```bash
# In the pdfluent-website repo:
cd ~/Documents/pdfluent-website
git checkout master   # or the deploy branch
git pull gitlab master --ff-only

# 1) Refresh vendored WASM to the just-published version
scripts/wasm/sync-sdk-wasm-from-npm.sh --version 1.0.0-beta.X

# 2) Update the install-command string (if not handled by the sync script)
#    src/config/docsChannels.ts → bump '@pdfluent/sdk-wasm@<version>' to match.

# 3) Re-run the canonicalness gate to confirm byte-identity
scripts/wasm/check-sdk-wasm-canonical.sh

# 4) Run the website demo smoke (Playwright e2e covering each demo)
npm run test:e2e -- --reporter=line

# 5) Build + deploy (only after all of the above pass)
scripts/deploy/manual-cloudflare-deploy.sh   # operator decision
```

## Drift-detection check (new automation, see §3)

`scripts/release/check_website_wasm_lag.sh` (lives in the **SDK repo**, here)
fails if the website's pinned WASM version lags behind the latest published
`@pdfluent/sdk-wasm` on the npm registry. Run as part of the SDK release-train
audit cadence (weekly cron or per-release manual).

## Known drift snapshot at goal-completion time

| Source | Version |
|---|---|
| npm `@pdfluent/sdk-wasm` dist-tag `latest` | `1.0.0-beta.11` |
| npm `@pdfluent/sdk-wasm` dist-tag `beta` | `1.0.0-beta.8` (this session's publish) |
| Website manifest (`public/wasm/sdk-wasm-manifest.json`) | `1.0.0-beta.10` *(stale by one publish)* |
| Website vendored (`src/lib/wasm/package.json`) | `1.0.0-beta.11` *(matches npm latest)* |
| Website install-command string (`docsChannels.ts`) | `1.0.0-beta.11` |

So the **manifest is stale** (claims beta.10) while the actual binaries are
beta.11 — the `check-sdk-wasm-canonical.sh` would FAIL on the next deploy
because of this drift. Operator action: re-run `sync-sdk-wasm-from-npm.sh
--version 1.0.0-beta.11` to refresh the manifest, OR `--version 1.0.0-beta.X`
when a newer version exists.

## Editor / Tauri-app demo updates

The desktop editor (`~/Documents/PDFluent/pdfluent`) currently has no
`@pdfluent/*` dependency (it consumes the engine via a different path).
**Editor releases are NOT in this integration step.** See `EDITOR_DECOUPLING_POLICY.md`.

## Future automation backlog

- A GitLab CI pipeline trigger that runs `check_website_wasm_lag.sh` on a
  schedule and opens an MR with the bumped pin if drift is found.
- A pre-deploy hook in `manual-cloudflare-deploy.sh` that exits non-zero if
  `npm view @pdfluent/sdk-wasm version > manifest.version` (i.e. there's a
  newer WASM available that the operator hasn't synced).
