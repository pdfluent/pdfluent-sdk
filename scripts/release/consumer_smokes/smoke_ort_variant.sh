#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# smoke_ort_variant.sh — Consumer smoke for the balanced/TATR (ort) variant.
#
# The release CLI, wheel and node addon ship with the `ort-layout` and
# `ort-tables` cargo features; this smoke proves the shipped bytes run the
# balanced tier against the `models-v1` package. One script, three channels:
#
#   --channel cli     unpack the CLI archive, install layout+tables from the
#                     models dir, `models status`, then a balanced extract
#                     with TATR over the fixture.
#   --channel python  install the wheel, install layout+tables with the CLI
#                     binary into a temp model dir, then a balanced
#                     `extract_markdown` call through the binding.
#   --channel node    install the packed tarballs, install layout+tables the
#                     same way, then a balanced `extractMarkdown` call.
#
# The install never touches the network: `PDFLUENT_MODELS_BASE=file://...`
# points `models install` at the unpacked `models-v1` artefact, and
# `PDFLUENT_MODEL_DIR` plus `ORT_DYLIB_PATH` keep every installed byte
# inside a temp dir that is removed on exit.
#
# T1 owns the install surface this smoke drives (`models install layout
# tables`, `PDFLUENT_MODELS_BASE`) and the bindings' balanced knobs
# (`extract_markdown(quality="balanced")`,
# `extractMarkdown({ quality: "balanced" })`). While any of them is absent
# the smoke FAILS naming the T1 job it needs -- never a silent green, and
# never a fallthrough onto the network or the fast tier.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_ort_variant.sh --channel cli|python|node
#       --artifact PATH --models DIR [--cli-artifact PATH] [--platform-pkg PATH]
#       [--version VER] [--fixture PATH] [--sentence TEXT] [--platform KEY]
#       [--cross-check]
#
# Options:
#   --channel NAME   One of cli, python, node (required).
#   --artifact PATH  The channel artefact under test: the CLI archive, the
#                    wheel, or the main node tarball (required).
#   --models DIR     The unpacked `models-v1` artefact dir (required).
#   --cli-artifact PATH  The CLI archive, for the python and node channels'
#                    `models install` (required unless --channel cli).
#   --platform-pkg PATH  The platform node tarball (required for --channel node).
#   --version VER    Expected version string (default: any, still runs --version).
#   --fixture PATH   Fixture PDF (default: fixtures/structured.pdf).
#   --sentence TEXT  Known sentence the markdown must contain
#                    (default: Safety rules).
#   --platform KEY   Must name this host; refused otherwise unless --cross-check.
#   --cross-check    Run even when --platform names another host.
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_ort]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
CHANNEL=""; ARTIFACT=""; MODELS_DIR=""; CLI_ARTIFACT=""; PLATFORM_PKG=""
EXPECTED_VERSION=""
FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"
SENTENCE="Safety rules"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel) CHANNEL="$2"; shift 2 ;;
    --artifact) ARTIFACT="$2"; shift 2 ;;
    --models) MODELS_DIR="$2"; shift 2 ;;
    --cli-artifact) CLI_ARTIFACT="$2"; shift 2 ;;
    --platform-pkg) PLATFORM_PKG="$2"; shift 2 ;;
    --version) EXPECTED_VERSION="$2"; shift 2 ;;
    --fixture) FIXTURE="$2"; shift 2 ;;
    --sentence) SENTENCE="$2"; shift 2 ;;
    --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
    --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
    -h|--help) sed -n '/^# Usage:/,/^#   --cross-check/p' "$0"; exit 0 ;;
    *) smoke_unknown_option "$1" ;;
  esac
done

smoke_check_platform

# The shared contract first: a named artefact that is not there fails
# naming the path, before any channel check can complain instead.
if [[ -n "$ARTIFACT" && ! -f "$ARTIFACT" ]]; then
  echo "$SMOKE_TAG no such artifact: $ARTIFACT" >&2
  exit 2
fi

case "$CHANNEL" in
  cli|python|node) ;;
  *) echo "$SMOKE_TAG pass --channel cli|python|node" >&2; exit 2 ;;
esac
[[ -n "$ARTIFACT" ]] || { echo "$SMOKE_TAG no --artifact for channel $CHANNEL" >&2; exit 2; }
[[ -n "$MODELS_DIR" ]] || { echo "$SMOKE_TAG no --models dir" >&2; exit 2; }
[[ -d "$MODELS_DIR" ]] || { echo "$SMOKE_TAG no such models dir: $MODELS_DIR" >&2; exit 2; }
if [[ "$CHANNEL" != "cli" ]]; then
  [[ -n "$CLI_ARTIFACT" ]] || { echo "$SMOKE_TAG channel $CHANNEL needs --cli-artifact for models install" >&2; exit 2; }
  [[ -f "$CLI_ARTIFACT" ]] || { echo "$SMOKE_TAG no such cli artifact: $CLI_ARTIFACT" >&2; exit 2; }
fi
if [[ "$CHANNEL" == "node" ]]; then
  [[ -n "$PLATFORM_PKG" ]] || { echo "$SMOKE_TAG channel node needs --platform-pkg" >&2; exit 2; }
  [[ -f "$PLATFORM_PKG" ]] || { echo "$SMOKE_TAG no such platform package: $PLATFORM_PKG" >&2; exit 2; }
fi

echo "=== PDFluent ort-variant consumer smoke ($CHANNEL) ==="
[[ -n "$SMOKE_PLATFORM" ]] && echo "Platform: $SMOKE_PLATFORM"

fail() { echo "$SMOKE_TAG FAIL: $1" >&2; exit 1; }

# Resolved, not bare: the Windows runner ships `python`, not `python3`.
PY="$(command -v python3 || command -v python)" || fail "no python on PATH"

# The free tier is what is under test.
unset PDFLUENT_LICENSE_KEY

[[ -f "$FIXTURE" ]] || fail "fixture not found: $FIXTURE"

# This platform's runtime library, by the artefact's asset name. The musl
# legs never reach here (the workflow gates them out): the published ORT
# libraries link glibc, so there is no musl asset to name.
case "${SMOKE_PLATFORM:-}" in
  linux-x64) ORT_ASSET="onnxruntime-linux-x86_64-1.23.2.so" ;;
  linux-arm64) ORT_ASSET="onnxruntime-linux-arm64-1.23.2.so" ;;
  osx-arm64|macos-arm64|macos-aarch64) ORT_ASSET="onnxruntime-macos-arm64-1.23.2.dylib" ;;
  osx-x64|macos-x64|macos-x86_64) ORT_ASSET="onnxruntime-macos-x86_64-1.23.2.dylib" ;;
  win-x64|windows-x64) ORT_ASSET="onnxruntime-windows-x86_64-1.23.2.dll" ;;
  *) fail "no ONNX Runtime 1.23.2 library is packaged for platform '${SMOKE_PLATFORM:-<none>}'" ;;
esac

MODELS_ABS="$(cd -- "$MODELS_DIR" && pwd)"
[[ -f "$MODELS_ABS/heron_ep03.onnx" ]] || fail "models dir has no heron_ep03.onnx: $MODELS_DIR"
[[ -f "$MODELS_ABS/tatr-v1.1-all.onnx" ]] || fail "models dir has no tatr-v1.1-all.onnx: $MODELS_DIR"
[[ -f "$MODELS_ABS/$ORT_ASSET" ]] || fail "models dir has no $ORT_ASSET: $MODELS_DIR"

SMOKE_DIR="$(mktemp -d -t pdfluent_ort_smoke_XXXXXX)"
trap 'rm -rf "$SMOKE_DIR"' EXIT

# 1. The installer ---------------------------------------------------------
# Every channel installs through the release CLI: `models install` is the
# only command that ever fetches a model or a runtime library.
CLI_ARCH="$ARTIFACT"
[[ "$CHANNEL" == "cli" ]] || CLI_ARCH="$CLI_ARTIFACT"
case "$CLI_ARCH" in
  *.tar.gz|*.tgz) tar -xzf "$CLI_ARCH" -C "$SMOKE_DIR" ;;
  *.zip) "$PY" -m zipfile -e "$CLI_ARCH" "$SMOKE_DIR" ;;
  *) fail "unknown CLI archive type: $CLI_ARCH" ;;
esac
BIN="$(find "$SMOKE_DIR" \( -name pdfluent -o -name pdfluent.exe \) -type f | head -1)"
[[ -n "$BIN" ]] || fail "no pdfluent binary in $CLI_ARCH"
chmod +x "$BIN"
echo "Installer: $BIN"

echo "$SMOKE_TAG --version ..."
reported="$("$BIN" --version 2>&1)" || fail "--version exited non-zero: $reported"
echo "$SMOKE_TAG reported: $reported"
if [[ -n "$EXPECTED_VERSION" ]]; then
  echo "$reported" | grep -q "$EXPECTED_VERSION" \
    || fail "version mismatch (said '$reported', expected '$EXPECTED_VERSION')"
  echo "$SMOKE_TAG version match: OK"
fi

# 2. The T1 gate ------------------------------------------------------------
# `models install layout tables` from a `file://` models base is the T1
# install job's surface. While the binary predates it the smoke fails
# naming that job -- installing from the default release URL instead would
# vouch for the network, not for the artefact under test.
INSTALL_HELP="$("$BIN" models install --help 2>&1)" \
  || fail "'models install --help' exited non-zero"
case "$INSTALL_HELP" in
  *tables*) ;;
  *) fail "SMOKE NOT RUN: requires the T1 install job: 'models install' knows no 'tables' package" ;;
esac
# The help text never names PDFLUENT_MODELS_BASE, so probe the behaviour:
# a base with a scheme the CLI refuses must be refused before any fetch,
# naming the variable. A binary that ignores the variable would instead
# install from the default release URL; the probe's model dir is a
# throwaway and its proxy a dead port, so that path writes nothing that
# counts and reaches no network.
PROBE_DIR="$SMOKE_DIR/models-base-probe"
mkdir -p "$PROBE_DIR"
BASE_PROBE="$(PDFLUENT_MODELS_BASE="unsupported-scheme://probe/" \
  PDFLUENT_MODEL_DIR="$PROBE_DIR" ORT_DYLIB_PATH="$PROBE_DIR/ort-probe" \
  HTTPS_PROXY="http://127.0.0.1:9" https_proxy="http://127.0.0.1:9" \
  "$BIN" models install layout 2>&1)" && probe_rc=0 || probe_rc=$?
case "$probe_rc:$BASE_PROBE" in
  0:*) fail "SMOKE NOT RUN: requires the T1 install job: 'models install' accepted PDFLUENT_MODELS_BASE=unsupported-scheme://probe/, so it reads no PDFLUENT_MODELS_BASE" ;;
  *PDFLUENT_MODELS_BASE*unsupported-scheme://probe/*) ;;
  *) fail "SMOKE NOT RUN: requires the T1 install job: 'models install' reads no PDFLUENT_MODELS_BASE (probe exited $probe_rc: $BASE_PROBE)" ;;
esac
echo "$SMOKE_TAG PDFLUENT_MODELS_BASE honoured: OK (bad scheme refused naming the variable)"

# 3. Install layout+tables from the artefact --------------------------------
# A temp model dir holds every installed byte: the weights resolve through
# PDFLUENT_MODEL_DIR, and the runtime library through ORT_DYLIB_PATH,
# which the install also honours as the destination. Nothing lands in the
# per-user model dir and nothing is read from it.
INSTALL_ROOT="$SMOKE_DIR/models"
export PDFLUENT_MODEL_DIR="$INSTALL_ROOT"
export ORT_DYLIB_PATH="$INSTALL_ROOT/onnxruntime-1.23.2/$ORT_ASSET"
# The URL through smoke_file_url, not "file://" glued to the path: on Git
# Bash that gave file:///c/..., which the native CLI read as \c\... on the
# current drive (16-10 win-x64, os error 3).
MODELS_URL="$(smoke_file_url "$MODELS_ABS")" || fail "no file URL for the models dir: $MODELS_ABS"
export PDFLUENT_MODELS_BASE="$MODELS_URL/"
echo "$SMOKE_TAG PDFLUENT_MODELS_BASE=$PDFLUENT_MODELS_BASE"
echo "$SMOKE_TAG PDFLUENT_MODEL_DIR=$PDFLUENT_MODEL_DIR"
"$BIN" models install layout tables || fail "'models install layout tables' exited non-zero"
STATUS="$("$BIN" models status 2>&1)" || fail "'models status' exited non-zero"
echo "$STATUS"
echo "$STATUS" | grep -q "sha256 ok" || fail "'models status' verifies nothing"
echo "$STATUS" | grep -qi "mismatch" && fail "'models status' reports a checksum mismatch"
echo "$STATUS" | grep -q ": missing" && fail "'models status' reports a missing file"
echo "$SMOKE_TAG models install: OK"

# 4. The balanced extract ----------------------------------------------------
case "$CHANNEL" in
  cli)
    echo "$SMOKE_TAG extract --quality balanced ..."
    markdown="$("$BIN" extract "$FIXTURE" --format md --quality balanced \
      --model-dir "$INSTALL_ROOT" --table-model-dir "$INSTALL_ROOT" \
      2>"$SMOKE_DIR/extract.err")" || {
      echo "$SMOKE_TAG FAIL: balanced extract exited non-zero:" >&2
      sed 's/^/    /' "$SMOKE_DIR/extract.err" >&2
      exit 1
    }
    echo "$markdown" | grep -qF "$SENTENCE" || {
      echo "$SMOKE_TAG FAIL: sentence '$SENTENCE' not in balanced output:" >&2
      echo "$markdown" | head -20 | sed 's/^/    /' >&2
      exit 1
    }
    echo "$SMOKE_TAG balanced extract: OK (${#markdown} chars, sentence found)"
    ;;

  python)
    "$PY" -m venv "$SMOKE_DIR/venv" || fail "could not create a virtualenv"
    # The venv interpreter by path, for the install AND the check: the check
    # ran under "$PY" (resolved before the venv existed), so it never saw the
    # wheel (15-10: "No module named 'pdfluent'" with a good install). No
    # activate: on Windows it splits a drive-letter PATH in Git Bash.
    if [[ -x "$SMOKE_DIR/venv/Scripts/python.exe" ]]; then VENV_PY="$SMOKE_DIR/venv/Scripts/python.exe"; else VENV_PY="$SMOKE_DIR/venv/bin/python"; fi
    "$VENV_PY" -c "import sys; assert sys.prefix != sys.base_prefix, sys.prefix" || fail "the venv interpreter is not a venv: $VENV_PY"
    # --no-index: the wheel under test is the whole install. Without it
    # pip may reach PyPI and the smoke vouches for a mix of local and
    # remote.
    "$VENV_PY" -m pip install --no-index "$ARTIFACT" || fail "pip install of $ARTIFACT failed"
    FIXTURE="$FIXTURE" SENTENCE="$SENTENCE" EXPECTED_VERSION="$EXPECTED_VERSION" \
      PDFLUENT_MODEL_DIR="$INSTALL_ROOT" ORT_DYLIB_PATH="$ORT_DYLIB_PATH" \
      "$VENV_PY" - <<'PY' || fail "the balanced extract_markdown call failed"
import os
import sys

try:
    import pdfluent
    print("  import pdfluent: OK")
except ImportError as e:
    print(f"  import pdfluent: FAIL - {e}", file=sys.stderr)
    sys.exit(1)

expected = os.environ.get("EXPECTED_VERSION") or ""
version = getattr(pdfluent, "__version__", None)
if expected:
    if version != expected:
        print(f"  __version__: FAIL - want {expected!r}, installed {version!r}",
              file=sys.stderr)
        sys.exit(1)
    print(f"  __version__: {version} (matches the artefact)")

fixture = os.environ["FIXTURE"]
sentence = os.environ["SENTENCE"]
try:
    with pdfluent.Document(fixture) as doc:
        markdown = doc.extract_markdown(quality="balanced")
except TypeError as e:
    print("  SMOKE NOT RUN: requires the T1 bindings job: "
          f"extract_markdown has no quality knob ({e})", file=sys.stderr)
    sys.exit(1)
if sentence not in markdown:
    print(f"  extract_markdown: FAIL - {sentence!r} not in output",
          file=sys.stderr)
    print(markdown[:500], file=sys.stderr)
    sys.exit(1)
print(f"  extract_markdown: OK ({len(markdown)} chars, {sentence!r} found)")
PY
    echo "$SMOKE_TAG balanced extract_markdown: OK"
    ;;

  node)
    command -v node >/dev/null 2>&1 || fail "node not on PATH"
    command -v npm >/dev/null 2>&1 || fail "npm not on PATH"
    # Absolute before the cd: a relative tarball no longer resolves inside
    # the project, and npm reads a bare `dir/file.tgz` as a GitHub shorthand.
    ARTIFACT="$(cd -- "$(dirname -- "$ARTIFACT")" && pwd)/$(basename -- "$ARTIFACT")"
    PLATFORM_PKG="$(cd -- "$(dirname -- "$PLATFORM_PKG")" && pwd)/$(basename -- "$PLATFORM_PKG")"
    mkdir -p "$SMOKE_DIR/nodeproj" && cd "$SMOKE_DIR/nodeproj"
    cat > package.json <<'JSON'
{ "name": "pdfluent-ort-smoke", "version": "0.0.0", "private": true }
JSON
    # The platform tarball installs in the same command: it satisfies the
    # main package's optional dependency locally, so npm never reaches
    # the registry for the binary under test.
    npm install --no-audit --no-fund --loglevel=error "$ARTIFACT" "$PLATFORM_PKG" \
      || fail "npm install of the node tarballs failed"
    # The T1 bindings gate: the balanced knob must be in the shipped
    # types. napi ignores unknown option keys in silence, so calling
    # without this check would run the fast tier and pass on it.
    DTS="node_modules/@pdfluent/node/index.d.ts"
    [[ -f "$DTS" ]] || fail "no $DTS in the installed package"
    grep -q "quality" "$DTS" \
      || fail "SMOKE NOT RUN: requires the T1 bindings job: extractMarkdown has no quality knob"
    cat > smoke.mjs <<'JS'
import { createRequire } from "module";
const require = createRequire(import.meta.url);
const fs = require("fs");

const mod = require("@pdfluent/node");
const { PdfDocument } = mod;
if (!PdfDocument || typeof PdfDocument.open !== "function") {
  console.error("[smoke_ort] FAIL: no PdfDocument.open on the binding exports");
  process.exit(1);
}
const expected = process.env.SMOKE_EXPECTED_VERSION;
const reported = mod.version ?? (typeof mod.getVersion === "function" ? mod.getVersion() : null);
if (expected && reported && reported !== expected) {
  console.error(`[smoke_ort] FAIL: version mismatch (binding=${reported} expected=${expected})`);
  process.exit(1);
}
const doc = PdfDocument.open(fs.readFileSync(process.env.SMOKE_FIXTURE));
const markdown = doc.extractMarkdown({ quality: "balanced" });
const sentence = process.env.SMOKE_SENTENCE;
if (typeof markdown !== "string" || !markdown.includes(sentence)) {
  console.error(`[smoke_ort] FAIL: ${JSON.stringify(sentence)} not in balanced output:`);
  console.error(String(markdown).slice(0, 500));
  process.exit(1);
}
console.log(`[smoke_ort] extractMarkdown: OK (${markdown.length} chars, '${sentence}' found)`);
JS
    SMOKE_EXPECTED_VERSION="$EXPECTED_VERSION" SMOKE_FIXTURE="$FIXTURE" \
      SMOKE_SENTENCE="$SENTENCE" PDFLUENT_MODEL_DIR="$INSTALL_ROOT" \
      ORT_DYLIB_PATH="$ORT_DYLIB_PATH" node smoke.mjs \
      || fail "the balanced extractMarkdown call failed"
    echo "$SMOKE_TAG balanced extractMarkdown: OK"
    ;;
esac

echo ""
echo "$SMOKE_TAG PASS"
