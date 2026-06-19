#!/usr/bin/env bash
# Safe, deterministic, idempotent publish for the @pdfluent/node N-API package.
#
# Replaces the `napi prepublish` path that mutated the manifest and shipped the
# broken 3-of-6 @pdfluent/node@1.0.0-beta.17. Flow:
#   1. refuse to run on a dirty tree (never publish uncommitted state);
#   2. deterministically (re)generate all six npm/<tag>/ platform dirs;
#   3. publish each platform package — idempotent: a version already on the
#      registry is SKIPPED, so a re-run after a partial failure is safe;
#   4. validate the EXACT main tarball (6 platforms, no binary, LICENSE/README);
#   5. publish the main meta-package with --ignore-scripts (no lifecycle script
#      can mutate the manifest);
#   6. verify every package resolves on the public registry.
#
# Lifecycle scripts are disabled on every publish (--ignore-scripts). The main
# meta-package version may differ from the platform version (packaging-only
# patch, e.g. main 1.0.0-beta.17.1 over platforms 1.0.0-beta.17): set
# PDFLUENT_NODE_PLATFORM_VERSION for that case.
#
# Usage: bash scripts/release/publish.sh [--dry-run]
set -Eeuo pipefail

DRY_RUN=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
SCOPE="@pdfluent"
MAIN_VER="$(node -p "require('./package.json').version")"
PLAT_VER="${PDFLUENT_NODE_PLATFORM_VERSION:-$MAIN_VER}"
TAGS=(darwin-arm64 darwin-x64 linux-x64-gnu linux-x64-musl linux-arm64-gnu win32-x64-msvc)

log() { echo "[publish] $*"; }
die() { echo "[publish] ERROR: $*" >&2; exit 1; }

published() { npm view "$1@$2" version >/dev/null 2>&1; }

# 1. clean tree (scoped to the package) — never publish dirty
if [[ -n "$(git -C "$ROOT" status --porcelain -- . 2>/dev/null)" ]]; then
  die "working tree under crates/pdf-node is dirty — commit or stash before publishing (never --allow-dirty)"
fi

# 2. deterministic platform dirs
log "generating platform dirs (platform version $PLAT_VER) ..."
PDFLUENT_NODE_PLATFORM_VERSION="$PLAT_VER" node scripts/release/generate-npm-dirs.cjs

# 3. publish platform packages — idempotent
for t in "${TAGS[@]}"; do
  name="$SCOPE/node-$t"
  if published "$name" "$PLAT_VER"; then
    log "SKIP $name@$PLAT_VER (already on registry)"
    continue
  fi
  # per-package audit: exactly one binary + LICENSE
  bins=$(find "npm/$t" -name '*.node' | wc -l | tr -d ' ')
  [[ "$bins" == "1" ]] || die "$name must contain exactly one .node (found $bins)"
  [[ -f "npm/$t/LICENSE" ]] || die "$name missing LICENSE"
  if [[ "$DRY_RUN" == 1 ]]; then
    log "DRY-RUN would publish $name@$PLAT_VER"; ( cd "npm/$t" && npm pack --ignore-scripts --dry-run >/dev/null )
  else
    log "publish $name@$PLAT_VER"; ( cd "npm/$t" && npm publish --ignore-scripts --access public )
  fi
done

# 4. validate the exact main tarball (the gate)
log "validating main tarball ..."
node scripts/release/validate-main-tarball.cjs

# 5. publish main meta-package (no binary in root → move any out first)
shopt -s nullglob
stray=( index.*.node )
shopt -u nullglob
if (( ${#stray[@]} )); then
  mkdir -p "$ROOT/.node-stash"; mv index.*.node "$ROOT/.node-stash/"
  log "moved ${#stray[@]} root binary(ies) to .node-stash for the meta-package pack"
fi
restore_stash() { [[ -d "$ROOT/.node-stash" ]] && mv "$ROOT"/.node-stash/*.node "$ROOT"/ 2>/dev/null && rmdir "$ROOT/.node-stash" 2>/dev/null || true; }
trap restore_stash EXIT

if published "$SCOPE/node" "$MAIN_VER"; then
  log "SKIP $SCOPE/node@$MAIN_VER (already on registry)"
elif [[ "$DRY_RUN" == 1 ]]; then
  log "DRY-RUN would publish $SCOPE/node@$MAIN_VER"; npm pack --ignore-scripts --dry-run >/dev/null
else
  log "publish $SCOPE/node@$MAIN_VER (--ignore-scripts)"; npm publish --ignore-scripts --access public
fi

# 6. registry verification
[[ "$DRY_RUN" == 1 ]] && { log "dry-run complete"; exit 0; }
log "verifying on registry ..."
od_count=$(npm view "$SCOPE/node@$MAIN_VER" optionalDependencies --json | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>process.stdout.write(String(Object.keys(JSON.parse(s)).length)))")
[[ "$od_count" == "6" ]] || die "published main has $od_count optionalDependencies, expected 6"
for t in "${TAGS[@]}"; do published "$SCOPE/node-$t" "$PLAT_VER" || die "$SCOPE/node-$t@$PLAT_VER not on registry"; done
log "OK — $SCOPE/node@$MAIN_VER (6/6 platforms @ $PLAT_VER) fully published"
