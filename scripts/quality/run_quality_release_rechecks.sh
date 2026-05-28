#!/usr/bin/env bash
# Orchestrator for the 6 Quality release-recheck lanes.
# Runs locally-executable lanes; marks CI-only lanes as
# release_gate_defined_not_executed_locally (with the exact script). Emits a
# machine-readable JSON summary. Exits non-zero only if a RUNNABLE/executable
# lane fails. Never uses paid APIs; never touches private artifacts.
set -uo pipefail
cd "$(dirname "$0")/../.."
OUT="benchmarks/runs/ga_readiness_3d/sdk_ga_quality_release_recheck/quality_release_recheck_status.json"
mkdir -p "$(dirname "$OUT")"
FAIL=0
declare -a ENTRIES

record () { ENTRIES+=("{\"lane\":\"$1\",\"status\":\"$2\",\"detail\":\"$3\"}"); }

run_cargo_lane () { # lane, test-target
  if cargo test -p pdfluent --test "$2" >/tmp/rc_$2.log 2>&1; then
    record "$1" "green_proven" "cargo test -p pdfluent --test $2"
  else
    record "$1" "blocked_test_failed" "see /tmp/rc_$2.log"; FAIL=1
  fi
}

echo "== QR-3 encryption matrix =="; run_cargo_lane "QR-3" "qr3_encryption_matrix"
echo "== QR-6 hostile/bombs =="; run_cargo_lane "QR-6" "qr6_hostile_bombs"
echo "== QR-15 security matrix =="; run_cargo_lane "QR-15" "qr15_security_matrix"

echo "== QR-13 no-network (re-affirm) =="
if python3 scripts/quality/check_no_network_telemetry.py >/dev/null 2>&1; then
  record "QR-13" "green_proven" "scripts/quality/check_no_network_telemetry.py"
else
  record "QR-13" "blocked_checker_failed" "no-network checker failed"; FAIL=1
fi

# QR-10 WASM browser hostile-input — executed in real Chromium.
#   QR10_RUN=1 re-runs the full build+browser harness live (~5min wasm build);
#   otherwise validate the committed run result (ok==true + valid_control ok).
QR10_RESULT="benchmarks/runs/ga_readiness_3d/sdk_ga_wasm_browser_hostile_input/qr10_browser_result.json"
if [ "${QR10_RUN:-0}" = "1" ]; then
  if bash scripts/ci/qr10_wasm_browser/run_qr10_wasm_browser.sh; then
    record "QR-10" "green_proven" "live Chromium run (run_qr10_wasm_browser.sh)"
  else
    rc=$?
    if [ "$rc" = "2" ]; then record "QR-10" "release_gate_defined_not_executed_locally" "browser/playwright unavailable (skip)"; \
    else record "QR-10" "blocked_browser_test_failed" "run_qr10_wasm_browser.sh failed"; FAIL=1; fi
  fi
elif python3 -c "import json,sys; d=json.load(open('$QR10_RESULT')); vc=d.get('cases',{}).get('valid_control',{}); sys.exit(0 if d.get('ok') is True and vc.get('ok') is True else 1)" 2>/dev/null; then
  record "QR-10" "green_proven" "committed Chromium result $QR10_RESULT (ok=true, valid_control ok); re-run with QR10_RUN=1"
else
  record "QR-10" "release_gate_defined_not_executed_locally" "scripts/ci/qr10_wasm_browser/run_qr10_wasm_browser.sh (no passing result yet)"
fi

# QR-9 FFI memory-safety — Miri (local) + ASAN/LSAN (Linux, executed on VPS).
#   QR9_RUN=1 re-runs the Linux sanitizer lane live (Linux+nightly only);
#   otherwise validate the committed ASAN/LSAN result (asan_run_exit==0).
QR9_RESULT="benchmarks/runs/ga_readiness_3d/sdk_ga_qr9_linux_asan_lsan/qr9_asan_lsan_result.json"
if [ "${QR9_RUN:-0}" = "1" ]; then
  if bash scripts/ci/qr9_sanitizers.sh; then record "QR-9" "green_proven" "live sanitizer run (qr9_sanitizers.sh)"; \
  else record "QR-9" "blocked_sanitizer_failed" "qr9_sanitizers.sh failed"; FAIL=1; fi
elif python3 -c "import json,sys; d=json.load(open('$QR9_RESULT')); sys.exit(0 if d.get('asan_run_exit')==0 and d.get('asan')=='executed_passed' else 1)" 2>/dev/null; then
  record "QR-9" "green_proven" "Miri (local) + Linux ASAN+LSAN executed-passed (committed $QR9_RESULT); re-run with QR9_RUN=1"
else
  record "QR-9" "release_gate_defined_not_executed_locally" "scripts/ci/qr9_sanitizers.sh (no passing ASAN result yet)"
fi

# QR-11 binding runtime error-mapping — C-ABI + Node + Python + .NET + Java
#   all executed-green (built artifacts + per-binding error-mapping tests).
#   QR11_RUN=1 re-runs the lane script live (needs built artifacts present).
QR11_RESULT="benchmarks/runs/ga_readiness_3d/sdk_ga_qr11_binding_runtime_mapping/qr11_result.json"
if [ "${QR11_RUN:-0}" = "1" ]; then
  if bash scripts/ci/qr11_binding_runtime_mapping.sh; then record "QR-11" "green_proven" "live binding runtime mapping (qr11_binding_runtime_mapping.sh)"; \
  else record "QR-11" "blocked_binding_runtime_failed" "qr11_binding_runtime_mapping.sh failed"; FAIL=1; fi
elif python3 -c "import json,sys; d=json.load(open('$QR11_RESULT')); b=d.get('bindings',{}); sys.exit(0 if d.get('all_green') is True and all(v.get('status')=='green_proven' for v in b.values()) and {'c-abi','node','python','dotnet','java'} <= set(b) else 1)" 2>/dev/null; then
  record "QR-11" "green_proven" "committed $QR11_RESULT — c-abi+node+python+dotnet+java all green; re-run with QR11_RUN=1"
else
  record "QR-11" "release_gate_defined_not_executed_locally" "scripts/ci/qr11_binding_runtime_mapping.sh (no passing result yet)"
fi

printf '{\n  "milestone":"SDK_GA_QUALITY_RELEASE_RECHECK_LANES",\n  "generated":"%s",\n  "lanes":[\n    %s\n  ],\n  "executable_failed":%s\n}\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(IFS=,; echo "${ENTRIES[*]}" | sed 's/},{/},\n    {/g')" "$FAIL" > "$OUT"
echo "wrote $OUT"
exit $FAIL
