#!/usr/bin/env bash
# QR-9 release gate — Linux ASAN+LSAN over the memory-safety subset, plus Miri.
#
# Proven 2026-05-20 on the Hetzner VPS (Linux x86_64, nightly + rust-src):
#   ga_quality (QR-1 no-panic x3 + QR-8 Send/Sync + concurrency) passed under
#   -Zsanitizer=address -Zbuild-std with ASAN_OPTIONS=detect_leaks=1 (LSAN on),
#   0 sanitizer findings, 0 leaks.
#
# On a non-Linux / no-nightly host the ASAN/LSAN sub-lane reports
# skipped_with_reason (never green). Emits a JSON summary. Exit 0 = all
# RUNNABLE sub-lanes passed; non-zero = a runnable sub-lane failed.
set -uo pipefail
cd "$(dirname "$0")/../.."

OS="$(uname -s)"
OUT="${QR9_JSON:-benchmarks/runs/ga_readiness_3d/sdk_ga_qr9_linux_asan_lsan/qr9_sanitizers_run.json}"
mkdir -p "$(dirname "$OUT")"
ASAN_STATUS="skipped"; ASAN_REASON=""; MIRI_STATUS="skipped"; MIRI_REASON=""
RESULT=0

have_nightly() { rustup toolchain list 2>/dev/null | grep -q nightly; }

# ---- ASAN + LSAN (Linux only) ----
if [ "$OS" = "Linux" ] && have_nightly; then
  echo "[ASAN+LSAN] cargo +nightly test -Zbuild-std -p pdfluent --test ga_quality (detect_leaks=1)"
  TGT="$(rustc -vV | sed -n 's/host: //p')"
  if RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
     ASAN_OPTIONS="detect_leaks=1:abort_on_error=1" \
     cargo +nightly test -Zbuild-std --target "$TGT" -p pdfluent --test ga_quality -- --test-threads=1; then
    ASAN_STATUS="passed"
  else
    ASAN_STATUS="failed"; RESULT=1
  fi
else
  ASAN_STATUS="skipped"; ASAN_REASON="host_not_linux_or_no_nightly (OS=$OS)"
  echo "[ASAN+LSAN] SKIP reason=$ASAN_REASON"
fi

# ---- Miri (pure-logic subset; runs anywhere miri is installed) ----
if rustup +nightly component list 2>/dev/null | grep -q 'miri.*(installed)'; then
  echo "[MIRI] cargo +nightly miri test -p pdfluent --test error_codes_stable"
  if MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p pdfluent --test error_codes_stable; then
    MIRI_STATUS="passed"
  else
    MIRI_STATUS="failed"; RESULT=1
  fi
else
  MIRI_STATUS="skipped"; MIRI_REASON="miri_component_not_installed"
  echo "[MIRI] SKIP reason=$MIRI_REASON"
fi

printf '{\n  "lane":"QR-9","os":"%s","generated":"%s",\n  "asan_lsan":{"status":"%s","reason":"%s"},\n  "miri":{"status":"%s","reason":"%s"},\n  "result":%s\n}\n' \
  "$OS" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$ASAN_STATUS" "$ASAN_REASON" "$MIRI_STATUS" "$MIRI_REASON" "$RESULT" > "$OUT"
echo "wrote $OUT"
exit $RESULT
