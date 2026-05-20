#!/usr/bin/env bash
# QR-10 WASM browser hostile-input release gate.
# Builds (or locates) the wasm-pack `web` package, runs the real Chromium
# hostile-input harness, and emits a JSON summary. Exits non-zero on test
# failure; exits 2 (SKIP) with a machine-readable reason if the browser or
# Playwright library is unavailable. Cleans the ephemeral package unless
# QR10_KEEP=1.
set -uo pipefail
cd "$(dirname "$0")/../../.."   # repo root

PKG="${QR10_PKG_DIR:-/tmp/qr10_pkg}"
OUT="${QR10_JSON:-benchmarks/runs/ga_readiness_3d/sdk_ga_wasm_browser_hostile_input/qr10_browser_result.json}"
HARNESS="scripts/ci/qr10_wasm_browser/run_browser.mjs"
PAGE="scripts/ci/qr10_wasm_browser/test_page.html"
mkdir -p "$(dirname "$OUT")"

# 1. Build the WASM `web` package if not already present.
if [ ! -f "$PKG/xfa_wasm.js" ]; then
  if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "SKIP reason=wasm_pack_missing"; exit 2
  fi
  echo "[qr10] building wasm package -> $PKG"
  wasm-pack build crates/xfa-wasm --target web --out-dir "$PKG" --no-typescript || { echo "BLOCKED reason=wasm_build_failed"; exit 1; }
fi

# 2. Install the glue page as index.html in the package dir.
cp "$PAGE" "$PKG/index.html"

# 3. Resolve the Playwright library node_modules (local > global > npx cache).
PW_NM=""
for cand in \
  "$PWD/node_modules/playwright" \
  "$(npm root -g 2>/dev/null)/playwright" \
  $(find "$HOME/.npm/_npx" -maxdepth 4 -type d -path '*/node_modules/playwright' 2>/dev/null | head -1); do
  if [ -n "$cand" ] && [ -d "$cand" ]; then PW_NM="$(dirname "$cand")"; break; fi
done
if [ -z "$PW_NM" ]; then echo "SKIP reason=playwright_library_unresolved"; exit 2; fi

# 4. Run the harness in real Chromium.
echo "[qr10] running browser harness (NODE_PATH=$PW_NM)"
NODE_PATH="$PW_NM" node "$HARNESS" "$PKG" "$OUT"
RC=$?

# 5. Clean ephemeral package unless asked to keep it.
if [ "${QR10_KEEP:-0}" != "1" ]; then rm -rf "$PKG"; fi
exit $RC
