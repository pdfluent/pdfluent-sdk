# Editor release policy — DECOUPLED from SDK/WASM releases

The desktop editor (`~/Documents/PDFluent/pdfluent`, the Tauri/React app) is
NOT released on every SDK or WASM publish. This decoupling is deliberate.

## Rationale

| Concern | SDK / WASM | Editor |
|---|---|---|
| **Audience** | developers, integrators | end-users |
| **Release cadence** | per-feature (weekly / per-PR) | per-milestone (monthly / quarterly) |
| **Verification cost** | matched-runtime smokes (seconds–minutes) | manual UX + crash-reporting cycles (days) |
| **Update channel** | npm/crates.io/pypi (consumer-pulled) | Tauri auto-updater + signed installers (push to users) |
| **Rollback** | yank / version pin | re-issue installer + signing identity intervention |
| **Signing identity** | n/a (npm/crates.io don't sign client-side) | **REQUIRED** (Apple notarization, Windows code-signing) |
| **Crash-reporting branches** | none | `editor/sentry`, `editor/crash-*` are editor-specific |

## What the policy means concretely

1. **A WASM publish (`@pdfluent/sdk-wasm@1.0.0-beta.N`) does NOT trigger an editor release.**
   The website demo's vendored WASM is refreshed via
   `pdfluent-website/scripts/wasm/sync-sdk-wasm-from-npm.sh`; the editor's WASM
   handling is independent.

2. **A crates.io publish (`pdfluent@1.0.0-beta.N` etc.) does NOT trigger an editor release.**
   The editor consumes the engine via its own pipeline (currently no `@pdfluent/*`
   npm dep in `editor/package.json`).

3. **Editor releases require ALL of the following before any signed artefact ships:**
   - Explicit operator approval (per release, not blanket).
   - Tauri signing-key + Apple notarization credentials present (`~/.tauri/*.key`,
     Apple developer cert installed) — see `reference_tauri_signing` memory entry.
   - Crash-reporting infrastructure verified (Sentry/equivalent endpoint reachable).
   - End-user UX regression sweep completed on the target platforms.
   - Updater channel (Tauri `latest.json` etc.) explicitly bumped + signed.

4. **`editor/*` and `crash-*` git branches are NOT part of the SDK release train.**
   The SDK release-train hooks (pre-push gates, license registry, ledger) live
   in the SDK repo; editor branches have their own gates.

## What this policy explicitly does NOT prevent

- **Manually starting an editor release** when the operator wants a deliberate
  milestone. That uses its own runbook (separate from this SDK release train).
- **The website demo updating its embedded WASM** independently of the editor.
  The website is a separate Cloudflare Pages deployment.

## Cross-references

- SDK release train: `docs/release/RELEASE_TRAIN_MATRIX.md`, `SDK_NON_XFA_PUBLISH_TRAIN_RUNBOOK.md`
- Website-demo integration: `WEBSITE_DEMO_RELEASE_INTEGRATION.md`
- Tauri signing inventory: persistent memory `reference_tauri_signing` (key passwordless, rotated 2026-05-22)

---

**Net effect:** SDK + WASM ship continuously; website demos refresh on each
WASM publish; the editor ships on deliberate milestones with full
crash-reporting + signing + UX-regression coverage.
