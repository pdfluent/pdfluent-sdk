#!/usr/bin/env bash
# smoke_node.sh — Consumer smoke test for the PDFluent Node.js (NAPI) package.
#
# Distinct from `smoke_wasm.sh`: that script tests the WebAssembly build (the
# `@pdfluent/sdk-wasm` package or equivalent), this one tests the native NAPI
# binding (`@pdfluent/node` / the `pdf-node` crate output) where the published
# tarball contains both JS glue and prebuilt native `.node` binaries.
#
# Creates a temporary Node.js project, installs the NAPI package from local
# tarballs (does not require an npm publish), and extracts a fixture PDF to
# markdown through `PdfDocument`: an import alone never loads the platform
# `.node` binary the way a real consumer does.
#
# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_node.sh [--artifact PATH] [--version VER]
#       [--platform KEY] [--cross-check] [--platform-pkg PATH]
#       [--extract-tgz PATH] [--fixture PATH] [--sentence TEXT]
#
# Options:
#   --artifact PATH  Path to the local .tgz produced by `npm pack` in
#                    crates/pdf-node/ (default: auto-detect newest .tgz under
#                    crates/pdf-node/).
#   --platform-pkg PATH  The packed platform sub-package (npm/<tag>) for this
#                    host, installed alongside --artifact so the loader finds
#                    the local binary instead of reaching for the registry.
#   --extract-tgz PATH   @pdfluent/extract tarball: installed on top (the npx-
#                    style leg), then its `pdfluent-extract` bin extracts the
#                    fixture. Skipped when absent.
#   --version VER    Expected package version (default: parsed from artifact
#                    filename).
#   --platform KEY   Must name this host; refused otherwise unless --cross-check
#   --cross-check    Run even when --platform names another host
#   --fixture PATH   Fixture PDF to extract (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain (default: Safety rules)
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Exit codes:
#   0  smoke passed
#   1  install / import / API call failed
#   2  argv / artifact-locate error

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_node]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
ARTIFACT=""
PLATFORM_PKG=""
EXTRACT_TGZ=""
EXPECTED_VERSION=""
FIXTURE=""
SENTENCE="Safety rules"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact|--tgz) ARTIFACT="$2"; shift 2 ;;
    --platform-pkg)   PLATFORM_PKG="$2"; shift 2 ;;
    --extract-tgz)    EXTRACT_TGZ="$2"; shift 2 ;;
    --version)        EXPECTED_VERSION="$2"; shift 2 ;;
    --platform)       SMOKE_PLATFORM="$2"; shift 2 ;;
    --cross-check)    SMOKE_CROSS_CHECK=1; shift ;;
    --fixture)        FIXTURE="$2"; shift 2 ;;
    --sentence)       SENTENCE="$2"; shift 2 ;;
    -h|--help)
      sed -n '/^# Usage:/,/^# Exit codes:/p' "$0"; exit 0 ;;
    *) smoke_unknown_option "$1" ;;
  esac
done

smoke_check_platform

[[ -z "$FIXTURE" ]] && FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"

echo "=== PDFluent Node (NAPI) consumer smoke test ==="

# 1. Locate the artifact ----------------------------------------------------
if [[ -z "$ARTIFACT" ]]; then
  ARTIFACT="$(ls -t "$REPO_ROOT"/crates/pdf-node/*.tgz 2>/dev/null | head -1)"
fi
if [[ -z "$ARTIFACT" || ! -f "$ARTIFACT" ]]; then
  echo "[smoke_node] ERROR: no such artifact: ${ARTIFACT:-<none found>}"
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

for extra in "$PLATFORM_PKG" "$EXTRACT_TGZ"; do
  if [[ -n "$extra" && ! -f "$extra" ]]; then
    echo "[smoke_node] ERROR: no such tarball: $extra" >&2
    exit 2
  fi
done
[[ -n "$PLATFORM_PKG" ]] && echo "[smoke_node] platform package: $PLATFORM_PKG"
[[ -n "$EXTRACT_TGZ" ]] && echo "[smoke_node] extract tarball: $EXTRACT_TGZ"

if [[ ! -f "$FIXTURE" ]]; then
  echo "[smoke_node] ERROR: fixture not found: $FIXTURE" >&2
  exit 2
fi
echo "[smoke_node] fixture: $FIXTURE"

# The scratch project below is entered with `cd`: a relative tarball path
# would no longer resolve there, and npm reads a bare `dir/file.tgz` as a
# GitHub shorthand (git ls-remote ssh://git@github.com/dir/file.tgz.git).
abs_path() { echo "$(cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")"; }
ARTIFACT="$(abs_path "$ARTIFACT")"; FIXTURE="$(abs_path "$FIXTURE")"
[[ -n "$PLATFORM_PKG" ]] && PLATFORM_PKG="$(abs_path "$PLATFORM_PKG")"
[[ -n "$EXTRACT_TGZ" ]] && EXTRACT_TGZ="$(abs_path "$EXTRACT_TGZ")"

# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises.
unset PDFLUENT_LICENSE_KEY

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

# The platform tarball installs in the same command: it satisfies the main
# package's optional dependency locally, so npm never reaches the registry
# for the binary under test. (An unsatisfied optional dep would only warn,
# and the loader would then fail on a missing .node — loud, but naming the
# wrong cause.)
INSTALL_TGZS=("$ARTIFACT")
[[ -n "$PLATFORM_PKG" ]] && INSTALL_TGZS+=("$PLATFORM_PKG")
echo "[smoke_node] npm install ${INSTALL_TGZS[*]} ..."
npm install --no-audit --no-fund --loglevel=error "${INSTALL_TGZS[@]}" || {
  echo "[smoke_node] FAIL: npm install of local tgz failed"
  exit 1
}

# 4. Import + extraction ----------------------------------------------------
# The smoke opens a real PDF and extracts markdown through the binding's
# public surface. An import-only check passes on a package whose platform
# `.node` binary is broken or missing — the loader defers that failure to
# the first call.
cat > smoke.mjs <<'JS'
import { createRequire } from "module";
const require = createRequire(import.meta.url);
const fs = require("fs");

let mod;
try {
  mod = require("@pdfluent/node");
} catch (e) {
  console.error(`[smoke_node] FAIL: require("@pdfluent/node") threw: ${e.message}`);
  process.exit(1);
}
const { PdfDocument } = mod;
if (!PdfDocument || typeof PdfDocument.open !== "function") {
  console.error("[smoke_node] FAIL: no PdfDocument.open on the binding exports");
  process.exit(1);
}

const fixture = process.env.SMOKE_FIXTURE;
const sentence = process.env.SMOKE_SENTENCE;
let doc;
try {
  doc = PdfDocument.open(fs.readFileSync(fixture));
} catch (e) {
  console.error(`[smoke_node] FAIL: PdfDocument.open threw: ${e.message}`);
  process.exit(1);
}
let markdown;
try {
  markdown = doc.extractMarkdown();
} catch (e) {
  console.error(`[smoke_node] FAIL: extractMarkdown threw: ${e.message}`);
  process.exit(1);
}
if (typeof markdown !== "string" || !markdown.includes(sentence)) {
  console.error(`[smoke_node] FAIL: ${JSON.stringify(sentence)} not in markdown output:`);
  console.error(String(markdown).slice(0, 500));
  process.exit(1);
}
console.log(`[smoke_node] extractMarkdown: OK (${markdown.length} chars, '${sentence}' found)`);
console.log(`[smoke_node] first line: ${markdown.split("\n")[0] ?? ""}`);

// If the binding exposes a `version` field or `getVersion()` function,
// assert it equals EXPECTED_VERSION.
const expected = process.env.SMOKE_EXPECTED_VERSION;
const reported = mod.version ?? (typeof mod.getVersion === "function" ? mod.getVersion() : null);
if (expected && reported && reported !== expected) {
  console.error(`[smoke_node] FAIL: version mismatch (binding=${reported} expected=${expected})`);
  process.exit(1);
}
if (reported) {
  console.log(`[smoke_node] binding-reported version: ${reported}`);
}

console.log("[smoke_node] PASS");
JS

SMOKE_EXPECTED_VERSION="$EXPECTED_VERSION" SMOKE_FIXTURE="$FIXTURE" SMOKE_SENTENCE="$SENTENCE" node smoke.mjs || {
  echo "[smoke_node] FAIL: smoke.mjs returned non-zero"
  exit 1
}

# 5. The npx-style leg: @pdfluent/extract installed on top of the binding
# under test, its `pdfluent-extract` bin extracting the same fixture. The
# `@pdfluent/node` dependency resolves to the tarball already installed
# above, so the bin runs against the artefact under test.
if [[ -n "$EXTRACT_TGZ" ]]; then
  echo "[smoke_node] npm install $EXTRACT_TGZ ..."
  npm install --no-audit --no-fund --loglevel=error "$EXTRACT_TGZ" || {
    echo "[smoke_node] FAIL: npm install of the extract tarball failed"
    exit 1
  }
  # npm may satisfy the extract package's @pdfluent/node range from the
  # registry instead of the tarball installed above (pip --no-index cannot
  # do that, which is why only the node leg needs this check): the bin
  # would then test the registry binding, not the artefact under test.
  INSTALLED_NODE_VER="$(node -p "require('./node_modules/@pdfluent/node/package.json').version" 2>/dev/null || echo unknown)"
  if [[ "$INSTALLED_NODE_VER" != "$EXPECTED_VERSION" ]]; then
    echo "[smoke_node] FAIL: @pdfluent/node resolved to $INSTALLED_NODE_VER, not the artefact under test ($EXPECTED_VERSION)"
    exit 1
  fi
  echo "[smoke_node] @pdfluent/node still $INSTALLED_NODE_VER (the artefact under test)"
  echo "[smoke_node] npx pdfluent-extract ..."
  EXTRACT_OUT="$(npx --no-install pdfluent-extract "$FIXTURE" 2>extract.err)" || {
    echo "[smoke_node] FAIL: pdfluent-extract exited non-zero:"
    sed 's/^/    /' extract.err
    exit 1
  }
  case "$EXTRACT_OUT" in
    *"$SENTENCE"*)
      echo "[smoke_node] pdfluent-extract: OK (${#EXTRACT_OUT} chars, '$SENTENCE' found)"
      echo "[smoke_node] first line: $(printf '%s' "$EXTRACT_OUT" | head -1)"
      ;;
    *)
      echo "[smoke_node] FAIL: '$SENTENCE' not in pdfluent-extract output:"
      printf '%s' "$EXTRACT_OUT" | head -20 | sed 's/^/    /'
      exit 1
      ;;
  esac
fi

echo "[smoke_node] all checks passed"
exit 0
