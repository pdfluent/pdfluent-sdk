#!/usr/bin/env bash
# run_extract_js_tests.sh — the @pdfluent/extract suite against THIS tree.
#
# @pdfluent/node@^1.1.0 is not on the registry yet (the tree is 1.1.0), so a
# plain `npm install` cannot resolve the engine — and resolving it from the
# registry would test the registry, not the tree. This builds the binding
# from crates/pdf-node and installs that tarball as the engine, then builds
# and tests the wrapper. Slow on a cold target dir; incremental after that.
# Used by the landing/full lanes and the scoped CI step, never the fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in node npm cargo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_extract_js_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

# A UNIVERSAL NODE UNDER ROSETTA RUNS AS x64 while rustc (arm64-only) builds
# arm64, so the binding is built for one arch and looked for under the other.
# That happens whenever this script descends from an x86_64 shell, such as an
# Intel bash 5 on an Apple-silicon Mac. Run node and npm natively instead.
if [ "$(uname -s)" = Darwin ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
  node() { arch -arm64 "$(type -P node)" "$@"; }
  npm()  { arch -arm64 "$(type -P npm)" "$@"; }
fi

cd crates/pdf-node
npm install --no-audit --no-fund
npm run build
# The platform file index.js loads beside itself (./index.<triple>.node),
# derived the way it derives it: platform and arch off the process, the linux
# flavour off the libc the process runs under.
TRIPLE="$(node -p "process.platform + '-' + process.arch + (process.platform === 'linux' ? (process.report.getReport().header.glibcVersionRuntime ? '-gnu' : '-musl') : process.platform === 'win32' ? '-msvc' : '')")"
BUILT="$REPO_ROOT/crates/pdf-node/index.${TRIPLE}.node"
if [ ! -f "$BUILT" ]; then
  echo "run_extract_js_tests.sh: $BUILT not found after npm run build; the binding did not produce this platform's binary" >&2
  # A sibling for another arch means the node runtime and the Rust target
  # disagree (seen: node under Rosetta while rustc targets arm64): name it.
  for _sibling in "$REPO_ROOT/crates/pdf-node"/index.*.node; do
    [ -f "$_sibling" ] && echo "run_extract_js_tests.sh: but this sibling was produced: $_sibling" >&2
  done
  exit 1
fi
TARBALL="$REPO_ROOT/crates/pdf-node/$(npm pack 2>/dev/null | tail -n 1)"
trap 'rm -f "$TARBALL"' EXIT

cd "$REPO_ROOT/integrations/pdfluent-extract-js"
# The tarball argument wins over the registry range: the tree build lands in
# node_modules as @pdfluent/node, and --no-save keeps the manifest's ^1.1.0.
# A plain install, deliberately not --omit=optional: vitest's own toolchain
# needs ITS optional natives (rollup's), and omitting them breaks the run.
# The unpublished =1.1.0 platform packages warn and skip, and whatever the
# registry holds is never loaded -- index.js requires the local
# ./index.<triple>.node first, which the next line is.
npm install --no-audit --no-fund --no-save "$TARBALL"
cp "$BUILT" node_modules/@pdfluent/node/
npm run build
npm test
