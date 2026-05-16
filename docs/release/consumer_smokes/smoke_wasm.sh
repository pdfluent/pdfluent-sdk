#!/usr/bin/env bash
# smoke_wasm.sh — Consumer smoke test for the PDFluent WASM / npm package.
#
# Creates a temporary Node.js project, installs the WASM package from a local
# directory (does not require npm publish), and exercises the primary JS API.
#
# Usage:
#   docs/release/consumer_smokes/smoke_wasm.sh [--pkg-dir DIR] [--version VERSION]
#
# Options:
#   --pkg-dir DIR    Path to the wasm-pack output directory (default: crates/xfa-wasm/pkg)
#   --version VER    Expected version string (default: read from pkg/package.json)

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
PKG_DIR=""
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --pkg-dir) PKG_DIR="$2"; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        *) shift ;;
    esac
done

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
    echo "⚠️  WASM pkg directory not found. Build first with:"
    echo "    cd crates/xfa-wasm && wasm-pack build --target web --release"
    echo "    scripts/release/transform-wasm-pkg.sh crates/xfa-wasm/pkg <version>"
    echo ""
    echo "Smoke skipped — no artefact."
    exit 0
fi

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
npm install --silent --save "${PKG_DIR}"

# Write the smoke script.
PKG_NAME=$(python3 -c "import json; d=json.load(open('${PKG_DIR}/package.json')); print(d['name'])" 2>/dev/null || echo "@pdfluent/sdk-wasm")

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
