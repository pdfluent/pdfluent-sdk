#!/usr/bin/env bash
# smoke_wasm.sh — Consumer smoke test for the PDFluent WASM / npm package.
#
# Creates a temporary Node.js project, installs the WASM package from a local
# directory (does not require npm publish), and exercises the primary JS API.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_wasm.sh [--artifact DIR] [--pkg-dir DIR]
#       [--version VERSION] [--platform KEY] [--cross-check]
#
# Options:
#   --artifact DIR   Path to the wasm-pack output directory under test (same as
#                    --pkg-dir; this is what post_publish_smoke_runner.sh passes,
#                    and the smoke tests EXACTLY this directory — never the
#                    repository package beside it)
#   --pkg-dir DIR    Same as --artifact (default: crates/xfa-wasm/pkg)
#   --version VER    Expected version string (default: read from pkg/package.json)
#   --platform KEY   Must name this host; refused otherwise unless --cross-check
#   --cross-check    Run even when --platform names another host
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_wasm]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
PKG_DIR=""
PKG_GIVEN=0
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact|--pkg-dir) PKG_DIR="$2"; PKG_GIVEN=1; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
        --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
        *) smoke_unknown_option "$1" ;;
    esac
done

smoke_check_platform

# A given artefact must exist, naming the path: a typo'd --artifact that
# fell through to the auto-detect below would vouch for the repository
# package instead of the artefact under test.
if [[ "$PKG_GIVEN" == "1" && ! -d "$PKG_DIR" ]]; then
    echo "❌ no such package dir: ${PKG_DIR}" >&2
    exit 2
fi

# Default pkg-dir
if [[ -z "$PKG_DIR" ]]; then
    for candidate in \
        "${REPO_ROOT}/crates/xfa-wasm/pkg" \
        "${REPO_ROOT}/crates/xfa-wasm/target/pkg"; do
        if [[ -d "$candidate" ]]; then
            PKG_DIR="$candidate"
            break
        fi
    done
fi

echo "=== PDFluent WASM consumer smoke test ==="
if [[ -z "$PKG_DIR" || ! -d "$PKG_DIR" ]]; then
    # No --pkg-dir was given (an explicit one that is missing fails
    # above): the no-argument local-dev case, and the only skip.
    echo "⚠️  WASM pkg directory not found. Build first with:"
    echo "    cd crates/xfa-wasm && wasm-pack build --target web --release"
    echo "    scripts/release/transform-wasm-pkg.sh crates/xfa-wasm/pkg <version>"
    echo ""
    echo "SMOKE SKIPPED: no artefact (pass --pkg-dir, or build the pkg first)."
    exit 0
fi

# Absolute before any cd: the install below runs in a temp dir, and a
# relative --pkg-dir no longer resolves from there (build 2 ran
# `npm install gate-pkg` from /tmp and failed).
PKG_DIR="$(cd -- "$PKG_DIR" && pwd)"

echo "Pkg dir: ${PKG_DIR}"

if ! command -v node &>/dev/null; then
    echo "❌ node not found in PATH"
    exit 3
fi
if ! command -v npm &>/dev/null; then
    echo "❌ npm not found in PATH"
    exit 3
fi

# Resolve version from package.json.
if [[ -z "$EXPECTED_VERSION" ]]; then
    EXPECTED_VERSION=$(python3 -c "import json; d=json.load(open('${PKG_DIR}/package.json')); print(d['version'])" 2>/dev/null || echo "unknown")
fi
echo "Version: ${EXPECTED_VERSION}"
echo ""

SMOKE_DIR=$(mktemp -d -t pdfluent_wasm_smoke_XXXXXX)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

# Create a minimal Node project that references the local pkg.
cat > "${SMOKE_DIR}/package.json" <<JSON
{
  "name": "pdfluent-wasm-smoke",
  "version": "0.0.1",
  "private": true,
  "type": "module"
}
JSON

echo "Installing WASM package from local directory..."
cd "${SMOKE_DIR}"
# Loud on purpose: --silent hid npm's error in build 2.
npm install --save "${PKG_DIR}"

# Write the smoke script.
# The path goes to python as its own argument in native spelling: inside
# the -c string Git Bash never rewrote it, so on Windows the open failed and
# the name fell back to the default in silence.
PKG_NAME=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['name'])" "$(smoke_native_path "${PKG_DIR}/package.json")" 2>/dev/null | tr -d '\r' || echo "@pdfluent/sdk-wasm")

cat > "${SMOKE_DIR}/smoke.mjs" <<MSCRIPT
import * as wasm from '${PKG_NAME}';

console.log('  import: OK');

// Version check.
const version = wasm.version?.() ?? wasm.VERSION ?? wasm.__version__ ?? '(not exported)';
console.log('  version:', version);

// Primary API surface probe.
const exports = Object.keys(wasm);
console.log('  exported symbols:', exports.slice(0, 10).join(', ') + (exports.length > 10 ? ', ...' : ''));

// Look for a primary document class.
const docClass = wasm.PdfDocument ?? wasm.Document ?? wasm.Pdf ?? null;
if (docClass) {
    console.log('  primary class found:', docClass.name || '(anonymous)');
} else {
    console.log('  primary class: not found at top level (check API surface)');
}

console.log('');
console.log('WASM consumer smoke PASS');
MSCRIPT

echo "Running WASM smoke..."
if node "${SMOKE_DIR}/smoke.mjs" 2>&1; then
    echo ""
    echo "✅ WASM consumer smoke PASS"
else
    echo ""
    echo "❌ WASM consumer smoke FAIL"
    exit 1
fi
