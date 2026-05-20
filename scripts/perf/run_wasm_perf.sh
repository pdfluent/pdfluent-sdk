#!/usr/bin/env bash
# Builds/locates the wasm-pack web package, installs the perf glue page, resolves
# the Playwright library, and runs the WASM browser perf baseline harness.
# Exit: 0 ok, 1 failure, 2 SKIP (wasm-pack/playwright/browser unavailable).
set -uo pipefail
cd "$(dirname "$0")/../.."   # repo root
PKG="${PERF_WASM_PKG:-/tmp/perf_wasm_pkg}"
OUT="${PERF_WASM_JSON:-benchmarks/runs/ga_readiness_3d/sdk_ga_performance_baseline/wasm_perf_run.json}"
VALID="${PERF_WASM_PDF:-tests/corpus-mini/multi-page.pdf}"
ITERS="${PERF_WASM_ITERS:-300}"
HARNESS="scripts/perf/run_wasm_browser_perf_baseline.mjs"
PAGE="scripts/perf/wasm/perf_page.html"
mkdir -p "$(dirname "$OUT")"
if [ ! -f "$PKG/xfa_wasm.js" ]; then
  command -v wasm-pack >/dev/null 2>&1 || { echo "SKIP reason=wasm_pack_missing"; exit 2; }
  wasm-pack build crates/xfa-wasm --target web --out-dir "$PKG" --no-typescript || { echo "BLOCKED reason=wasm_build_failed"; exit 1; }
fi
cp "$PAGE" "$PKG/index.html"
PW_NM=""
for cand in \
  "$PWD/node_modules/playwright" \
  "$(npm root -g 2>/dev/null)/playwright" \
  $(find "$HOME/.npm/_npx" -maxdepth 4 -type d -path '*/node_modules/playwright' 2>/dev/null | head -1); do
  if [ -n "$cand" ] && [ -d "$cand" ]; then PW_NM="$(dirname "$cand")"; break; fi
done
[ -z "$PW_NM" ] && { echo "SKIP reason=playwright_library_unresolved"; exit 2; }
NODE_PATH="$PW_NM" node "$HARNESS" "$PKG" "$VALID" "$OUT" "$ITERS"
