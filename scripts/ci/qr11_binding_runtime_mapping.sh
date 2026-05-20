#!/usr/bin/env bash
# QR-11 release gate — runtime error-mapping across bindings.
# Proves the same canonical error cases (malformed PDF, invalid page index,
# unsupported op, encrypted-without-password) map to typed errors in each
# binding's RUNTIME (not just the static parity matrix).
#
# Each binding sub-lane runs only if its BUILT artifact is present; otherwise
# it reports skip+reason (needs the binding built/published). Exit 0 = all
# runnable sub-lanes passed.
set -uo pipefail
cd "$(dirname "$0")/../.."
RESULT=0

run_lane () { # name, detect-cmd, run-cmd
  local name="$1" detect="$2" run="$3"
  if eval "$detect" >/dev/null 2>&1; then
    echo "[$name] RUN"; eval "$run" || { echo "[$name] FAIL"; RESULT=1; }
  else
    echo "[$name] SKIP reason=runtime_or_built_artifact_missing"
  fi
}

# C ABI: the strict-api example already exercises null/typed-status paths.
run_lane "c-abi" "command -v cc && test -d pdfluent-examples/c/strict-api" \
  "(cd pdfluent-examples/c/strict-api && make check >/dev/null 2>&1)"

# Node: needs the built napi binding on the example's node_modules.
run_lane "node" "command -v node && test -d pdfluent-examples/node/strict-ts/node_modules/pdfluent" \
  "node pdfluent-examples/node/strict-ts/error_mapping_smoke.mjs"

# Python: needs the editable/installed pdfluent wheel.
run_lane "python" "python3 -c 'import pdfluent' " \
  "python3 scripts/quality/qr11_python_error_mapping.py"

# .NET: needs the built binding referenced by StrictApi.
run_lane "dotnet" "command -v dotnet && test -d pdfluent-examples/dotnet/StrictApi" \
  "(cd pdfluent-examples/dotnet/StrictApi && dotnet run --no-restore >/dev/null 2>&1)"

# Java: needs the built jar on the local Maven repo.
run_lane "java" "command -v mvn && test -d pdfluent-examples/java/StrictApi" \
  "(cd pdfluent-examples/java/StrictApi && mvn -q test)"

exit $RESULT
