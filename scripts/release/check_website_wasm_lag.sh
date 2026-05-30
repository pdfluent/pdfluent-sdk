#!/usr/bin/env bash
# check_website_wasm_lag.sh
#
# Fails (exit 1) if the website's pinned @pdfluent/sdk-wasm version
# lags behind the latest published version on the npm registry.
#
# Run as part of the SDK release-train audit cadence — e.g. cron, or
# manually before a website deploy. Pair with the website-side
# `scripts/wasm/check-sdk-wasm-canonical.sh` (byte-identity gate).
#
# Optional env:
#   WEBSITE_REPO   path to the pdfluent-website checkout
#                  (default: ~/Documents/pdfluent-website)
#   ALLOW_BETA     if set, accept dist-tag `beta` as the freshest version
#                  (default: only `latest` counts).
#
# Exit codes:
#   0   pinned version equals the latest published; up to date
#   1   pinned version lags behind the latest published
#   2   manifest missing, npm unreachable, or argv error
#
# This script does NOT mutate the website. It only reports drift.
# It does NOT publish anything. Read-only.

set -euo pipefail

WEBSITE_REPO="${WEBSITE_REPO:-$HOME/Documents/pdfluent-website}"
MANIFEST="${WEBSITE_REPO}/public/wasm/sdk-wasm-manifest.json"
PACKAGE="@pdfluent/sdk-wasm"

if [[ ! -f "${MANIFEST}" ]]; then
  echo "[lag-check] ERROR: manifest not found at ${MANIFEST}" >&2
  echo "[lag-check]        is WEBSITE_REPO=${WEBSITE_REPO} correct?" >&2
  exit 2
fi

pinned=$(python3 -c "import json; print(json.load(open('${MANIFEST}'))['version'])" 2>/dev/null || true)
if [[ -z "${pinned}" ]]; then
  echo "[lag-check] ERROR: cannot read 'version' from ${MANIFEST}" >&2
  exit 2
fi

if ! npm_meta=$(curl -sS -f "https://registry.npmjs.org/${PACKAGE}" 2>/dev/null); then
  echo "[lag-check] ERROR: cannot fetch npm metadata for ${PACKAGE}" >&2
  exit 2
fi

# Pick the "latest" semver tag (or "beta" if ALLOW_BETA is set + present)
target_tag="latest"
[[ "${ALLOW_BETA:-0}" == "1" ]] && target_tag="beta"
upstream=$(python3 -c "
import json, sys
d = json.loads('''${npm_meta}''')
tags = d.get('dist-tags', {})
v = tags.get('${target_tag}')
if not v and '${target_tag}' == 'beta':
    v = tags.get('latest')
print(v or '')
" 2>/dev/null)

if [[ -z "${upstream}" ]]; then
  echo "[lag-check] ERROR: cannot parse npm dist-tag '${target_tag}'" >&2
  exit 2
fi

echo "[lag-check] package:  ${PACKAGE}"
echo "[lag-check] pinned:   ${pinned}    (from ${MANIFEST})"
echo "[lag-check] upstream: ${upstream}  (npm dist-tag '${target_tag}')"

if [[ "${pinned}" == "${upstream}" ]]; then
  echo "[lag-check] OK: website pinned WASM equals npm '${target_tag}'."
  exit 0
fi

# Drift detected
echo "[lag-check] FAIL: website pinned WASM (${pinned}) lags behind npm '${target_tag}' (${upstream})." >&2
echo
echo "[lag-check] Recommended action (run in ${WEBSITE_REPO}):"
echo "[lag-check]   scripts/wasm/sync-sdk-wasm-from-npm.sh --version ${upstream}"
echo "[lag-check]   scripts/wasm/check-sdk-wasm-canonical.sh"
echo "[lag-check]   # then update src/config/docsChannels.ts install command if it carries the version inline"
echo "[lag-check]   # then run e2e demo smoke + deploy when the operator approves"
exit 1
