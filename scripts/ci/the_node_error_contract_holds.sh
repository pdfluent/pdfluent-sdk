#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the Node addon from this tree and run its error-contract test (#587).
#
# WHY HERE
#
# crates/pdf-node/tests/error_contract.test.js provokes one failure per error
# code on a real document and requires a PdfluentError with that `.code`. The
# job that runs the package's `npm test` -- build-and-test in node-bindings.yml
# -- fires on a release tag only, and on 22-09-2026 that workflow was disabled
# outright. For as long as the contract was broken (every error a bare string,
# `.code` reading `GenericFailure`) nothing that ran would have said so. So the
# landing lane runs it whenever pdf-node is in the landing.
#
# WHY A COPY OF THE PACKAGE
#
# The test loads `index.js`, and index.js loads `./index.<platform>.node` if it
# is there and the PUBLISHED `@pdfluent/node-<platform>` package if it is not.
# `npm install` installs that package as an optional dependency, so a run that
# forgot the local binary would test the release on npm and pass. The copy gets
# the binary this tree built, and the published one is removed before jest
# starts; the checkout itself is not written to.
#
# Exit codes:
#   0  the contract test passed against the addon built from this tree
#   1  it failed, or a tool it needs is missing (that is not a pass)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

for tool in cargo node npm python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "NODE ERROR CONTRACT: FAIL (not a pass) -- '$tool' is not on PATH" >&2
        exit 1
    fi
done

cargo build -q -p pdf-node

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
case "$(uname -s)" in
    Darwin) LIB="$TARGET_DIR/debug/libpdf_node.dylib" ;;
    Linux)  LIB="$TARGET_DIR/debug/libpdf_node.so" ;;
    *) echo "NODE ERROR CONTRACT: FAIL -- unsupported host $(uname -s)" >&2; exit 1 ;;
esac
if [[ ! -f "$LIB" ]]; then
    echo "NODE ERROR CONTRACT: FAIL -- cargo built no $LIB" >&2
    exit 1
fi

# The name index.js looks for on this host: darwin-<arch>, or linux-<arch>-gnu
# (the lane's machines are glibc).
#
# On a Mac, node can run under Rosetta while cargo builds for the machine: the
# x64 node then looks for index.darwin-x64.node and cannot load an arm64 one.
# So on Darwin the binary decides, and node, npm and jest run in its
# architecture -- native dependencies of jest included.
RUN=()
if [[ "$(uname -s)" = Darwin ]]; then
    LIB_ARCH="$(lipo -archs "$LIB")"
    case "$LIB_ARCH" in
        arm64)  TAG=darwin-arm64 ;;
        x86_64) TAG=darwin-x64 ;;
        *) echo "NODE ERROR CONTRACT: FAIL -- $LIB is '$LIB_ARCH', not one architecture" >&2; exit 1 ;;
    esac
    RUN=(arch "-$LIB_ARCH")
else
    TAG="$(node -p '`linux-${process.arch}-gnu`')"
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
# The test reads ../../../fixtures and ../../../docs, so the copy keeps the
# repository's shape around the package.
mkdir -p "$WORK/crates"
ln -s "$ROOT/fixtures" "$WORK/fixtures"
ln -s "$ROOT/docs" "$WORK/docs"
mkdir "$WORK/crates/pdf-node"
# Every top-level module of the package, not a fixed list: index.js requires
# its siblings (./chunks since the page-chunk layer), and a list that forgets
# one fails this gate with "Cannot find module" instead of testing anything.
for f in "$ROOT"/crates/pdf-node/*.js "$ROOT"/crates/pdf-node/*.mjs \
         "$ROOT"/crates/pdf-node/*.d.ts "$ROOT"/crates/pdf-node/package.json; do
    [ -f "$f" ] && cp "$f" "$WORK/crates/pdf-node/"
done
cp -R "$ROOT/crates/pdf-node/tests" "$WORK/crates/pdf-node/tests"
cp "$LIB" "$WORK/crates/pdf-node/index.$TAG.node"
# THE COPY WITHOUT ITS DEBUG INFORMATION. jest's haste map reads every file of
# the package into one string, and a Linux debug build of the addon grew past
# the 512 MB a string may hold: the gate then died in `readFileSync` with no
# verdict on the code ("Cannot create a string longer than 0x1fffffe8",
# #2051). The test needs the addon's symbols, not its DWARF, and the build in
# the shared target directory is left as other gates expect it.
case "$(uname -s)" in
    Darwin) strip -S "$WORK/crates/pdf-node/index.$TAG.node" ;;
    Linux)  strip --strip-debug "$WORK/crates/pdf-node/index.$TAG.node" ;;
esac
# And a ceiling, so the day it grows back the gate says why instead of dying
# inside jest. NODE_ADDON_MAX_MB overrides it for the self-check below.
MAX_MB="${NODE_ADDON_MAX_MB:-256}"
SIZE_MB=$(( $(wc -c < "$WORK/crates/pdf-node/index.$TAG.node") / 1048576 ))
if (( SIZE_MB > MAX_MB )); then
    echo "NODE ERROR CONTRACT: FAIL -- the addon is ${SIZE_MB} MB stripped, over the ${MAX_MB} MB jest can read safely" >&2
    exit 1
fi

cd "$WORK/crates/pdf-node"
# `install`, not `ci`: the package commits no lock file.
"${RUN[@]}" npm install --no-audit --no-fund --loglevel=error
rm -rf node_modules/@pdfluent
PDFLUENT_REQUIRE_ADDON=1 "${RUN[@]}" npx jest --ci tests/error_contract.test.js tests/typed_error_layer.test.js
