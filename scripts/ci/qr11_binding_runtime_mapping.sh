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

# Node: built napi artifact (index.js + *.node) + the dedicated error-mapping
# harness. Identity-guarded inside the harness (local index.js + PdfDocument).
run_lane "node" "command -v node && ls crates/pdf-node/*.node >/dev/null 2>&1 && test -f scripts/ci/qr11_runtime/node_error_mapping.cjs" \
  "node scripts/ci/qr11_runtime/node_error_mapping.cjs"

# Python: the REAL pdfluent binding only (a same-named placeholder must not
# count). PY_BIN may point at the maturin build venv; the helper re-checks
# __file__ in pdf-python + the typed hierarchy.
run_lane "python" "\${PY_BIN:-python3} -c 'import pdfluent,sys; sys.exit(0 if hasattr(pdfluent,\"open_pdf\") and hasattr(pdfluent,\"PdfluentError\") else 1)'" \
  "\${PY_BIN:-python3} scripts/quality/qr11_python_error_mapping.py"

# .NET: build the committed console harness against the local PDFluent project
# and run it (native libpdf_capi must match the dotnet runtime arch; see report).
run_lane "dotnet" "command -v dotnet && test -f scripts/ci/qr11_runtime/dotnet/Program.cs" \
  "REPO=\"\$PWD\" dotnet run --project scripts/ci/qr11_runtime/dotnet -c Release"

# Java: the canonical com.pdfluent binding test (matches the JNI exports).
# Native libs (libpdfluent_java + libpdf_capi) must be on java.library.path.
run_lane "java" "command -v mvn && test -f bindings/java/src/test/java/com/pdfluent/Qr11ErrorMappingTest.java" \
  "(cd bindings/java && mvn test -Dtest=Qr11ErrorMappingTest -DfailIfNoTests=false)"

exit $RESULT
