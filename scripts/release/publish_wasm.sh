#!/usr/bin/env bash
# publish_wasm.sh — publish @pdfluent/sdk-wasm to npm.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md.
#
# Why this exists: `.gitlab-ci.yml` has called this file since the release jobs
# were written and it was never in the repository (found 22-08-2026). The dry
# run beside it, scripts/release/wasm_dry_run.sh, has always existed and does
# every step except the last one — so this script runs that same gate and then,
# only if it passes, publishes.
#
# Usage:
#   scripts/release/publish_wasm.sh          # gate only, publishes nothing
#   scripts/release/publish_wasm.sh --live   # gate, then npm publish
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

LIVE=false
[ "${1:-}" = "--live" ] && LIVE=true
[ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ] && { sed -n '2,14p' "$0"; exit 0; }

if [ -n "$(git status --porcelain)" ]; then
  echo "publish_wasm: working tree is not clean — PUBLISH_PROTOCOL.md §4.1" >&2
  git status --short >&2
  exit 1
fi

# The gate builds, packs, extracts and audits the actual tarball. Running it
# here rather than trusting an earlier pipeline stage keeps the thing that is
# published and the thing that was audited the same bytes.
echo "[publish-wasm] running the packaging gate (build + pack + audit)"
bash "${SCRIPT_DIR}/wasm_dry_run.sh"

PKG_DIR="${REPO_ROOT}/crates/xfa-wasm/pkg"
VERSION="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['version'])" "${PKG_DIR}/package.json")"
NAME="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['name'])" "${PKG_DIR}/package.json")"

if npm view "${NAME}@${VERSION}" version >/dev/null 2>&1; then
  echo "[publish-wasm] ${NAME}@${VERSION} is already on npm — nothing to do"
  exit 0
fi

if ! $LIVE; then
  echo "[publish-wasm] gate passed for ${NAME}@${VERSION} — pass --live to publish"
  exit 0
fi

echo "[publish-wasm] PUBLISHING ${NAME}@${VERSION}"
npm publish --access public "${PKG_DIR}"

for _ in $(seq 1 30); do
  npm view "${NAME}@${VERSION}" version >/dev/null 2>&1 && break
  sleep 5
done
echo "[publish-wasm] ${NAME}@${VERSION} is live"
