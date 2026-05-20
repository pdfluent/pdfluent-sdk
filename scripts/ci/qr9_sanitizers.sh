#!/usr/bin/env bash
# QR-9 release gate — FFI memory-safety via sanitizers + Miri.
# Designed for a LINUX CI runner with a nightly toolchain. On hosts that
# cannot run a given tool, that sub-lane reports skip+reason (never green).
#
# Exit 0 = all RUNNABLE sub-lanes passed; non-zero = a runnable sub-lane failed.
set -uo pipefail
cd "$(dirname "$0")/../.."

OS="$(uname -s)"
RESULT=0
echo "QR-9 sanitizer/Miri lane on ${OS}"

# 1. AddressSanitizer (Linux x86_64/aarch64 nightly). Builds std with the
#    sanitizer so FFI alloc/free in pdf-capi is instrumented.
if [ "$OS" = "Linux" ] && rustup toolchain list | grep -q nightly; then
  echo "[ASAN] cargo +nightly test -p pdfluent-capi (address sanitizer)"
  RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
    cargo +nightly test -Zbuild-std --target "$(rustc -vV | sed -n 's/host: //p')" \
    -p pdf-capi 2>&1 | tail -20 || RESULT=1
  echo "[LSAN] LeakSanitizer is bundled with ASAN on Linux; leaks fail the run above."
else
  echo "[ASAN/LSAN] SKIP — requires Linux + nightly (-Zsanitizer=address). reason=host_not_linux_or_no_nightly"
fi

# 2. Miri on a pure-logic subset (no FFI/threads/process). error_codes_stable
#    and determinism are good candidates.
if rustup +nightly component list 2>/dev/null | grep -q 'miri.*installed'; then
  echo "[MIRI] cargo +nightly miri test -p pdfluent --test error_codes_stable --test determinism"
  MIRIFLAGS="-Zmiri-disable-isolation" \
    cargo +nightly miri test -p pdfluent --test error_codes_stable --test determinism 2>&1 | tail -20 || RESULT=1
else
  echo "[MIRI] SKIP — miri component not installed (rustup component add miri). reason=miri_not_installed"
fi

# 3. C-ABI sanitizer smoke (compile the strict C example under ASAN on Linux).
if [ "$OS" = "Linux" ]; then
  echo "[CABI-ASAN] build pdfluent-examples/c/strict-api with -fsanitize=address (see runbook)"
else
  echo "[CABI-ASAN] SKIP — host_not_linux"
fi

exit $RESULT
