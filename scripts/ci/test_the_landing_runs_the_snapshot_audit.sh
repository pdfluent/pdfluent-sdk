#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The landing lane RUNS the snapshot path audit; it does not defer it (#574).
#
# From 11-09 to 30-09-2026 a case in `run` deferred snapaudit on every lane but
# --full, overriding the `zwaar` line that puts it in the landing lane. Two test
# fixtures with a machine path in them landed that way and turned master's full
# lane red. This drives the gate's own `run` and `zwaar`, cut out of
# local_ci_gate.sh, in each lane, and asks what they print for snapaudit.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GATE="$ROOT/scripts/ci/local_ci_gate.sh"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
fails=0; cases=0
check() { cases=$((cases+1)); if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2"; fails=$((fails+1)); fi; }

# One function of the gate, from its opening line to the first `}` in column 0.
cut_out() { awk -v f="^$2\\\\(\\\\) \\\\{" '$0 ~ f {on=1} on {print} on && /^}/ {exit}' "$1"; }

# What the lane prints for `snapaudit` when the gate itself is `true`.
lane() {  # <gate file> <FULL> <LANDING>
  {
    echo 'PLAATS=""; DEVELOPMENT_DEFERRED=""; LANDING_OUT_OF_REACH=""'
    echo "LOGDIR='$WORK/logs'; pass=0; fail=0; uitgesteld=0; ongeraakt=0; gevallen=''"
    echo "FULL=$2; LANDING=$3"
    echo 'tree_check() { :; }'
    cut_out "$1" hoort_hier
    cut_out "$1" run
    cut_out "$1" zwaar
    echo 'zwaar snapaudit true'
  } > "$WORK/harness.sh"
  mkdir -p "$WORK/logs"
  bash "$WORK/harness.sh" 2>&1
}

said="$(lane "$GATE" 0 1)"
check "$(echo "$said" | grep -q '^=== snapaudit *PASS' && echo ok)" \
  "the landing lane runs it (said: $said)"
said="$(lane "$GATE" 1 0)"
check "$(echo "$said" | grep -q '^=== snapaudit *PASS' && echo ok)" \
  "the full lane runs it"
said="$(lane "$GATE" 0 0)"
check "$(echo "$said" | grep -q '^=== snapaudit *DEFERRED' && echo ok)" \
  "the fast lane still defers it, to the landing"

# The check can see the defect it is for: the 11-09 case put back into `run`.
sed 's|^  local _t0=\$(date +%s)$|  case "$name" in snapaudit) printf "=== %-9s DEFERRED (not a pass)\\n" "$name"; return 0 ;; esac\n&|' \
  "$GATE" > "$WORK/broken_gate.sh"
said="$(lane "$WORK/broken_gate.sh" 0 1)"
check "$(echo "$said" | grep -q '^=== snapaudit *DEFERRED' && echo ok)" \
  "a run() that defers it again is caught"

# The path audit, not the build: the build half is minutes and gigabytes and
# stays on a push to master.
# Spelled in parts: the push-lane guards follow a script path named here into the
# audit's build half, and this file only reads the gate, it runs no audit.
audit="public_snapshot""_audit.sh"
check "$(grep -qx "zwaar snapaudit bash scripts/ci/$audit" "$GATE" && echo ok)" \
  "the gate calls the path audit, without --with-build"
# Neither scoper may name it, or the landing would skip it by another door.
check "$(! grep -q snapaudit "$ROOT/scripts/ci/development_gate_scope.py" "$ROOT/scripts/ci/landing_test_scope.py" && echo ok)" \
  "neither lane scoper names it"

echo "test_the_landing_runs_the_snapshot_audit: $cases case(s), $fails failure(s)"
[ "$fails" = 0 ]
