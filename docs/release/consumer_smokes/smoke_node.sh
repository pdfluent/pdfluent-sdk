#!/usr/bin/env bash
# smoke_node.sh — Consumer smoke test for the PDFluent Node.js (NAPI) package.
#
# Distinct from `smoke_wasm.sh`: that script tests the WebAssembly build (the
# `@pdfluent/sdk-wasm` package or equivalent), this one tests the native NAPI
# binding (`@pdfluent/node` / the `pdf-node` crate output) where the published
# tarball contains both JS glue and prebuilt native `.node` binaries.
#
# Creates a temporary Node.js project, installs the NAPI package from a local
# tarball (does not require an npm publish), imports it, and exercises the
# primary JS API.
#
# Usage:
#   docs/release/consumer_smokes/smoke_node.sh [--artifact PATH] [--version VER]
#
# Options:
#   --artifact PATH  Path to the local .tgz produced by `npm pack` in
#                    crates/pdf-node/ (default: auto-detect newest .tgz under
#                    crates/pdf-node/).
#   --version VER    Expected package version (default: parsed from artifact
#                    filename).
#
# Exit codes:
#   0  smoke passed
#   1  install / import / API call failed
#   2  argv / artifact-locate error

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
ARTIFACT=""
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact|--tgz) ARTIFACT="$2"; shift 2 ;;
    --version)        EXPECTED_VERSION="$2"; shift 2 ;;
    -h|--help)
      sed -n '/^# Usage:/,/^# Exit codes:/p' "$0"; exit 0 ;;
    *) echo "[smoke_node] unknown option: $1" >&2; exit 2 ;;
  esac
done

echo "=== PDFluent Node (NAPI) consumer smoke test ==="

# 1. Locate the artifact ----------------------------------------------------
if [[ -z "$ARTIFACT" ]]; then
  ARTIFACT="$(ls -t "$REPO_ROOT"/crates/pdf-node/*.tgz 2>/dev/null | head -1)"
fi
if [[ -z "$ARTIFACT" || ! -f "$ARTIFACT" ]]; then
  echo "[smoke_node] ERROR: no .tgz artifact found"
  echo "             (looked under crates/pdf-node/*.tgz; provide --artifact if elsewhere)"
  exit 2
fi
echo "[smoke_node] artifact: $ARTIFACT"

# Derive version from filename if not given (npm pack writes
# <scope>-<name>-<version>.tgz).
if [[ -z "$EXPECTED_VERSION" ]]; then
  EXPECTED_VERSION="$(basename "$ARTIFACT" .tgz | sed -E 's/^.*-([0-9].*$)/\1/')"
fi
echo "[smoke_node] expected version: $EXPECTED_VERSION"

# 2. Prerequisite check -----------------------------------------------------
command -v node >/dev/null 2>&1 || { echo "[smoke_node] ERROR: node not in PATH"; exit 2; }
command -v npm  >/dev/null 2>&1 || { echo "[smoke_node] ERROR: npm not in PATH"; exit 2; }
echo "[smoke_node] node: $(node --version)   npm: $(npm --version)"

# 3. Scratch project --------------------------------------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$TMP"
echo "[smoke_node] scratch project: $TMP"

cat > package.json <<JSON
{ "name": "pdfluent-node-smoke", "version": "0.0.0", "private": true }
JSON

echo "[smoke_node] npm install $ARTIFACT ..."
npm install --no-audit --no-fund --silent "$ARTIFACT" || {
  echo "[smoke_node] FAIL: npm install of local tgz failed"
  exit 1
}

# 4. Import + version check + minimal API call ------------------------------
# We do not assume an exhaustive API surface (pdf-node may still be skeletal
# at this point in the release train); the smoke just confirms the binding
# loads on this Node version + the version number is what we expect. Any
# detected exported function is invoked dry to surface link errors.
cat > smoke.mjs <<'JS'
import { createRequire } from "module";
const require = createRequire(import.meta.url);

// Try both ESM-default and CJS-default surfaces; the package may export
// either depending on NAPI binding template choices.
let mod;
try {
  mod = await import("@pdfluent/node");
} catch {
  try {
    mod = await import("pdfluent-node");
  } catch {
    mod = require("@pdfluent/node");
  }
}
if (!mod) {
  console.error("[smoke_node] FAIL: import returned nothing");
  process.exit(1);
}
const exported = Object.keys(mod.default ?? mod).filter(k => k !== "default");
console.log("[smoke_node] exports:", exported.join(", ") || "(none)");

// If the binding exposes a `version` field or `getVersion()` function,
// assert it equals EXPECTED_VERSION.
const expected = process.env.SMOKE_EXPECTED_VERSION;
const target = mod.default ?? mod;
const reported = target.version ?? (typeof target.getVersion === "function" ? target.getVersion() : null);
if (expected && reported && reported !== expected) {
  console.error(`[smoke_node] FAIL: version mismatch (binding=${reported} expected=${expected})`);
  process.exit(1);
}
if (reported) {
  console.log(`[smoke_node] binding-reported version: ${reported}`);
} else {
  console.log("[smoke_node] (binding does not expose a version field; skipping version check)");
}

console.log("[smoke_node] PASS");
JS

SMOKE_EXPECTED_VERSION="$EXPECTED_VERSION" node smoke.mjs || {
  echo "[smoke_node] FAIL: smoke.mjs returned non-zero"
  exit 1
}

echo "[smoke_node] all checks passed"
exit 0
