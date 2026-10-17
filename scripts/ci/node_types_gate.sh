#!/usr/bin/env bash
#
# Type-check the PUBLISHED @pdfluent/node package, the way a consumer gets it.
#
# Why this exists: `pdfluent-examples/node/strict-ts` has run `tsc --strict`
# since the beta, resolved the package through a tsconfig `paths` entry
# pointing straight at `crates/pdf-node/index.d.ts`, and was never wired into
# CI. On 22-09-2026 it turned out to produce 12 type errors and to have never
# compiled at all (#585). A `paths` entry also never exercises the `exports`
# map, so a broken `types` condition would ship green.
#
# This gate packs the real tarball, installs it into throwaway projects under
# each module resolution we support, and type-checks the example against the
# package name. Then `attw` checks the conditions themselves.
#
# Usage: scripts/ci/node_types_gate.sh
# Exit:  0 all resolutions type-check; 1 any failure or a refused prerequisite.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PKG_DIR="$REPO_ROOT/crates/pdf-node"
EXAMPLE="$REPO_ROOT/pdfluent-examples/node/strict-ts/index.ts"

# Pinned so a toolchain release cannot turn this gate red or green on its own.
TS_VERSION="5.6.3"
TYPES_NODE_VERSION="20.14.10"
ATTW_VERSION="0.18.5"

skip() {
  echo "SKIPPED (not a pass): $1" >&2
  exit 1
}

command -v node >/dev/null 2>&1 || skip "node is not on PATH"
command -v npm  >/dev/null 2>&1 || skip "npm is not on PATH"
[ -f "$EXAMPLE" ] || skip "strict-ts example not found at $EXAMPLE"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "== packing @pdfluent/node =="
# `npm pack` runs prepublishOnly, which guards a real publish; not wanted here.
PACK_OUT="$WORK/pack.log"
( cd "$PKG_DIR" && npm pack --ignore-scripts --pack-destination "$WORK" ) >"$PACK_OUT" 2>&1
PACK_STATUS=$?
if [ "$PACK_STATUS" -ne 0 ]; then
  cat "$PACK_OUT" >&2
  skip "npm pack failed (status $PACK_STATUS)"
fi
TARBALL="$(find "$WORK" -maxdepth 1 -name '*.tgz' | head -1)"
[ -n "$TARBALL" ] || skip "npm pack produced no tarball"
echo "   $(basename "$TARBALL")"

# The tarball must carry both typings at all — cheap check, clear message.
# Listed into a variable first: under pipefail, `tar | grep -q` can report the
# SIGPIPE tar takes when grep stops reading early as a missing file.
LISTING="$(tar tzf "$TARBALL")" || skip "cannot list $TARBALL"
for dts in index.d.ts index.d.mts; do
  if ! grep -qx "package/$dts" <<<"$LISTING"; then
    echo "FAIL: $dts is not in the published tarball" >&2
    exit 1
  fi
done

failures=0

# name | module | moduleResolution | package type
RESOLUTIONS=(
  "node16-esm|node16|node16|module"
  "node16-cjs|node16|node16|commonjs"
  "bundler|esnext|bundler|module"
)

for entry in "${RESOLUTIONS[@]}"; do
  IFS='|' read -r name module resolution pkgtype <<<"$entry"
  proj="$WORK/$name"
  mkdir -p "$proj"

  cat >"$proj/package.json" <<EOF
{ "name": "types-gate-$name", "private": true, "version": "0.0.0", "type": "$pkgtype" }
EOF

  cat >"$proj/tsconfig.json" <<EOF
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "$module",
    "moduleResolution": "$resolution",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": false,
    "types": ["node"]
  },
  "include": ["index.ts"]
}
EOF

  cp "$EXAMPLE" "$proj/index.ts"

  echo "== $name: installing the tarball =="
  # --omit=optional: the platform binaries are not needed to type-check, and
  # pinning them here would tie the gate to whatever is on the registry.
  ( cd "$proj" && npm install --silent --no-audit --no-fund --omit=optional \
      "$TARBALL" "typescript@$TS_VERSION" "@types/node@$TYPES_NODE_VERSION" ) >"$proj/install.log" 2>&1
  if [ $? -ne 0 ]; then
    tail -20 "$proj/install.log" >&2
    skip "npm install failed for $name (no registry access?)"
  fi

  echo "== $name: tsc --strict --noEmit =="
  ( cd "$proj" && ./node_modules/.bin/tsc -p tsconfig.json ) >"$proj/tsc.log" 2>&1
  if [ $? -ne 0 ]; then
    echo "FAIL: $name (module=$module moduleResolution=$resolution type=$pkgtype)" >&2
    sed 's/^/    /' "$proj/tsc.log" >&2
    failures=$((failures + 1))
  else
    echo "   ok"
  fi
done

echo "== are-the-types-wrong =="
ATTW_LOG="$WORK/attw.log"
npx --yes "@arethetypeswrong/cli@$ATTW_VERSION" "$TARBALL" --format table >"$ATTW_LOG" 2>&1
ATTW_STATUS=$?
cat "$ATTW_LOG"
if [ "$ATTW_STATUS" -ne 0 ]; then
  echo "FAIL: attw reported problems with the published conditions" >&2
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "node types gate: $failures failure(s)" >&2
  exit 1
fi
echo "node types gate: all resolutions type-check"
