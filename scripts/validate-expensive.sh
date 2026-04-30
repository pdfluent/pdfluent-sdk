#!/usr/bin/env bash
#
# validate-expensive.sh — VPS/local expensive validation runner.
#
# Runs the heavy validation suite that is intentionally NOT part of
# the GitHub Actions PR gate (cost reasons; see
# docs/ci-cost-control.md). Use this on a VPS or local box before
# merging large PRs, before releases, or when changes touch wasm /
# bindings / desktop / rendering / XFA / conversion code.
#
# Usage:
#   scripts/validate-expensive.sh <branch-or-commit>
#
# Env vars:
#   FAIL_FAST=1     Stop at first failed phase (default: 0, run all).
#   SKIP_FETCH=1    Skip `git fetch` (offline mode).
#   PHASES=A,B,...  Restrict phases (default: A,B,C,D,E).
#                   A=wasm  B=bindings  C=desktop  D=conversion  E=corpus
#
# Output:
#   validation-reports/<safe-ref>/<UTC-timestamp>/
#     summary.txt
#     fetch.log, checkout.log
#     phase-<X>-<name>.log
#     corpus-results.sqlite (phase E only, when corpus available)
#
# Exit codes:
#   0  all selected phases passed
#   1  one or more phases failed
#   2  usage / setup error

set -uo pipefail

REF="${1:-}"
if [[ -z "$REF" ]]; then
  echo "Usage: $0 <branch-or-commit>" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Use the default ONLY when PHASES is unset; when PHASES is set but
# empty (e.g. `PHASES= scripts/validate-expensive.sh ...`) treat it as
# an error rather than silently substituting the default. Use the
# `${VAR-default}` form (no colon) so empty stays empty.
PHASES_ENABLED="${PHASES-A,B,C,D,E}"

# Validate PHASES before any side effects (mkdir / fetch / checkout) so
# a typo or empty value aborts cleanly without producing a misleading
# report directory or git activity. Allowed letters: A B C D E.
if [[ -z "$PHASES_ENABLED" ]]; then
  echo "[FATAL] PHASES is empty. Set PHASES to a non-empty comma-separated subset of A,B,C,D,E (e.g. PHASES=A,B)" >&2
  exit 2
fi
PHASES_ARRAY=()
IFS=',' read -ra _phase_tokens <<< "$PHASES_ENABLED"
for _token in "${_phase_tokens[@]}"; do
  case "$_token" in
    A|B|C|D|E)
      PHASES_ARRAY+=("$_token")
      ;;
    "")
      echo "[FATAL] Empty phase letter in PHASES='$PHASES_ENABLED' (allowed: A,B,C,D,E)" >&2
      exit 2
      ;;
    *)
      echo "[FATAL] Unknown phase letter '$_token' in PHASES='$PHASES_ENABLED' (allowed: A,B,C,D,E)" >&2
      exit 2
      ;;
  esac
done
if [[ ${#PHASES_ARRAY[@]} -eq 0 ]]; then
  echo "[FATAL] No valid phases parsed from PHASES='$PHASES_ENABLED'" >&2
  exit 2
fi

TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
SAFE_REF="${REF//\//_}"
REPORT_DIR="validation-reports/${SAFE_REF}/${TIMESTAMP}"
mkdir -p "$REPORT_DIR"

SUMMARY="$REPORT_DIR/summary.txt"

{
  echo "VPS Expensive Validation"
  echo "Ref:        $REF"
  echo "Timestamp:  $TIMESTAMP"
  echo "Phases:     $PHASES_ENABLED"
  echo "Fail-fast:  ${FAIL_FAST:-0}"
  echo "Report dir: $REPORT_DIR"
  echo "=========================================="
} | tee "$SUMMARY"

if [[ "${SKIP_FETCH:-0}" != "1" ]]; then
  if ! git fetch origin --tags --prune 2>&1 | tee "$REPORT_DIR/fetch.log"; then
    echo "[FATAL] git fetch failed -- aborting (no phases run)" | tee -a "$SUMMARY"
    exit 2
  fi
fi
if ! git checkout "$REF" 2>&1 | tee "$REPORT_DIR/checkout.log"; then
  echo "[FATAL] git checkout '$REF' failed -- aborting (no phases run)" | tee -a "$SUMMARY"
  exit 2
fi

FAILURES=()
RAN_COUNT=0

run_phase() {
  local letter="$1" name="$2"
  shift 2
  if [[ ",${PHASES_ENABLED}," != *",${letter},"* ]]; then
    echo "[skip] Phase ${letter} ${name}" | tee -a "$SUMMARY"
    return 0
  fi
  echo "[run]  Phase ${letter} ${name}" | tee -a "$SUMMARY"
  RAN_COUNT=$((RAN_COUNT + 1))
  local logfile="$REPORT_DIR/phase-${letter}-${name}.log"
  local start
  start=$(date +%s)
  if "$@" >"$logfile" 2>&1; then
    local elapsed=$(( $(date +%s) - start ))
    echo "[pass] Phase ${letter} (${elapsed}s)" | tee -a "$SUMMARY"
    return 0
  else
    local elapsed=$(( $(date +%s) - start ))
    echo "[FAIL] Phase ${letter} (${elapsed}s) -- see ${logfile}" | tee -a "$SUMMARY"
    FAILURES+=("${letter}:${name}")
    return 1
  fi
}

# Phase functions are explicitly fail-fast: each command must succeed or
# the function returns non-zero. Without this, bash (with errexit off)
# returns only the LAST command's status, which can mask earlier
# failures when a phase chains multiple commands.

phase_a_wasm() {
  cargo build --target wasm32-unknown-unknown -p xfa-wasm || return 1
}

phase_b_bindings() {
  cargo build --release -p pdf-capi || return 1
  if [[ -f crates/pdf-capi/tests/Makefile ]]; then
    (cd crates/pdf-capi/tests && make test CAPI_LIB=../../../target/release) || return 1
  else
    echo "No crates/pdf-capi/tests/Makefile, skipping C smoke tests"
  fi
}

phase_c_desktop() {
  cargo test --package pdf-desktop || return 1
  if [[ -f crates/pdf-desktop/package.json ]]; then
    (cd crates/pdf-desktop && npm install && npx vitest run) || return 1
  else
    echo "No crates/pdf-desktop/package.json, skipping frontend tests"
  fi
}

phase_d_conversion() {
  cargo test --package pdf-docx || return 1
  cargo test --package pdf-xlsx || return 1
  cargo test --package pdf-pptx || return 1
}

phase_e_corpus() {
  cargo build --release -p xfa-test-runner || return 1
  local corpus_dir="tests/corpus-subset"
  if [[ -d "$corpus_dir" ]] && [[ $(find "$corpus_dir" -name '*.pdf' 2>/dev/null | wc -l) -gt 0 ]]; then
    ./target/release/xfa-test-runner run \
      --corpus "$corpus_dir" \
      --db "$REPORT_DIR/corpus-results.sqlite" \
      --workers 2 --timeout 10 --no-verapdf || return 1
    local panics
    panics=$(sqlite3 "$REPORT_DIR/corpus-results.sqlite" \
      "SELECT COUNT(*) FROM test_results WHERE status='crash'") || return 1
    echo "Panics: $panics"
    if [[ "$panics" -gt 0 ]]; then
      ./target/release/xfa-test-runner clusters \
        --db "$REPORT_DIR/corpus-results.sqlite" || true
      return 1
    fi
  else
    echo "No corpus PDFs in $corpus_dir, skipping panic check"
  fi
}

dispatch_phase() {
  local letter="$1"
  case "$letter" in
    A) run_phase A wasm       phase_a_wasm ;;
    B) run_phase B bindings   phase_b_bindings ;;
    C) run_phase C desktop    phase_c_desktop ;;
    D) run_phase D conversion phase_d_conversion ;;
    E) run_phase E corpus     phase_e_corpus ;;
    *) echo "Unknown phase: $letter" >&2; return 2 ;;
  esac
}

for phase in A B C D E; do
  if ! dispatch_phase "$phase"; then
    if [[ "${FAIL_FAST:-0}" == "1" ]]; then
      echo "[abort] FAIL_FAST=1 -- stopping after first failure" | tee -a "$SUMMARY"
      break
    fi
  fi
done

echo "==========================================" | tee -a "$SUMMARY"
# Defense in depth: PHASES validation above guarantees PHASES_ARRAY has
# at least one valid letter, but if some future change to dispatch_phase
# / FAIL_FAST drops every phase before it runs, refuse to declare PASS.
if [[ $RAN_COUNT -eq 0 ]]; then
  echo "Result: INVALID (no phases ran -- check PHASES='$PHASES_ENABLED')" | tee -a "$SUMMARY"
  exit 2
fi
if [[ ${#FAILURES[@]} -eq 0 ]]; then
  echo "Result: PASS ($RAN_COUNT phase(s) ran)" | tee -a "$SUMMARY"
  exit 0
else
  echo "Result: FAIL (${#FAILURES[@]} of $RAN_COUNT phase(s) failed): ${FAILURES[*]}" | tee -a "$SUMMARY"
  exit 1
fi
