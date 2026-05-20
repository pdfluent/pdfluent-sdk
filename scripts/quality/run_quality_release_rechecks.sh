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

# CI-only lanes: defined + executable in CI, not on this host.
record "QR-9"  "release_gate_defined_not_executed_locally" "scripts/ci/qr9_sanitizers.sh (Linux+nightly ASAN/LSAN/Miri)"
record "QR-10" "release_gate_defined_not_executed_locally" "scripts/ci/qr10_wasm_browser/hostile_input.spec.mjs (Playwright)"
record "QR-11" "release_gate_defined_not_executed_locally" "scripts/ci/qr11_binding_runtime_mapping.sh (needs built bindings)"

printf '{\n  "milestone":"SDK_GA_QUALITY_RELEASE_RECHECK_LANES",\n  "generated":"%s",\n  "lanes":[\n    %s\n  ],\n  "executable_failed":%s\n}\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(IFS=,; echo "${ENTRIES[*]}" | sed 's/},{/},\n    {/g')" "$FAIL" > "$OUT"
echo "wrote $OUT"
exit $FAIL
