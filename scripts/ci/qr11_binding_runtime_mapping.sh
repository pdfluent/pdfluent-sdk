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
# C ABI: build the lib, then compile+link+RUN the runtime error-mapping
# harness (real typed-status assertions, not just a syntax check).
run_lane "c-abi" "command -v cc" \
  "cargo build -p pdf-capi --release >/dev/null 2>&1 && \
   LIBEXT=\$([ \"\$(uname -s)\" = Darwin ] && echo dylib || echo so) && \
   cc -Wall -Wextra -Werror -std=c11 -Icrates/pdf-capi/include \
      scripts/ci/qr11_runtime/c_abi_error_mapping.c \
      -Ltarget/release -lpdf_capi -Wl,-rpath,target/release -o /tmp/qr11_cabi && \
   /tmp/qr11_cabi"

# Node: needs the built napi binding AND a dedicated error-mapping harness.
run_lane "node" "test -f pdfluent-examples/node/strict-ts/error_mapping_smoke.mjs && test -d pdfluent-examples/node/strict-ts/node_modules/pdfluent" \
  "node pdfluent-examples/node/strict-ts/error_mapping_smoke.mjs"

# Python: needs the REAL pdfluent binding (a same-named placeholder package
# must not count). The helper itself re-checks hasattr(PdfDocument).
run_lane "python" "python3 -c 'import pdfluent,sys; sys.exit(0 if hasattr(pdfluent,\"PdfDocument\") else 1)'" \
  "python3 scripts/quality/qr11_python_error_mapping.py"

# .NET: only run when a built binding artifact exists (avoid false FAIL/pass).
run_lane "dotnet" "command -v dotnet && ls pdfluent-examples/dotnet/StrictApi/bin/*/*/StrictApi.dll >/dev/null 2>&1" \
  "(cd pdfluent-examples/dotnet/StrictApi && dotnet run --no-build >/dev/null 2>&1)"

# Java: only run a DEDICATED error-mapping test (a generic build/test is not
# an error-mapping proof). Harness absent today -> SKIP, never a false pass.
run_lane "java" "command -v mvn && test -f pdfluent-examples/java/StrictApi/src/test/java/ErrorMappingTest.java" \
  "(cd pdfluent-examples/java/StrictApi && mvn -q -o test -Dtest=ErrorMappingTest)"

exit $RESULT
