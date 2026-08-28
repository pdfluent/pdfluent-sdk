#!/usr/bin/env bash
# local_ci_gate.sh — run the GitLab CI gates locally, before pushing.
#
# Calls the SAME scripts the pipeline calls (scripts/ci/run_*.sh) so this
# gate can never drift from CI. Run it before every push:
#     bash scripts/ci/local_ci_gate.sh           # sanity + lint (fast)
#     bash scripts/ci/local_ci_gate.sh --full     # + test + audit (slow)
#
# Exit 0 only if every gate passes. Mirrors:
#   ci-yaml  : scripts/ci/ci_config_lint.py
#              Shape of .gitlab-ci.yml, not just its syntax. An unquoted ": " in
#              a script line parses as valid YAML and becomes a dict, which
#              GitLab rejects outright -- the pipeline comes back `failed` with
#              zero jobs and no clue where to look.
#   metadata : cargo metadata --no-deps
#   fmt      : cargo fmt --all -- --check
#   build    : scripts/ci/run_build.sh   (cargo check, CI excludes)
#   clippy   : scripts/ci/run_clippy.sh  (-D warnings, CI excludes)
#   licenses : scripts/release/license_registry_check.py
#              Enforces docs/release/canonical_licenses.toml. Catches any
#              accidental relicensing of PDFluent IP under open source, or
#              any open-source-derivative fork being relabelled as commercial.
#   test     : scripts/ci/run_test.sh    (--full only)
#   audit    : scripts/ci/run_audit.sh   (--full only; needs clean tree)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
FULL=0
case "${1:-}" in
  --full)     FULL=1 ;;
  --fast|"")  FULL=0 ;;
  *) echo "usage: local_ci_gate.sh [--fast|--full]" >&2; exit 2 ;;
esac
fail=0; pass=0
run() { local name="$1"; shift
  printf '=== %-9s' "$name"
  if "$@" >"/tmp/lcg_${name}.log" 2>&1; then echo " PASS"; pass=$((pass+1))
  else echo " FAIL — see /tmp/lcg_${name}.log"; tail -15 "/tmp/lcg_${name}.log" | sed 's/^/    /'; fail=$((fail+1)); fi
}
# Clean-tree advisory (the CI audit job requires it; auto-generated gen/schemas
# churn is a known false-positive — see docs).
if [ -n "$(git status --porcelain | grep -vE 'gen/schemas|\.e1_gaps')" ]; then
  echo "NOTE: working tree has uncommitted changes (the CI audit job requires a clean tree)."
fi
# `run` swallows output on success, and this one says things worth reading when
# it passes: a WARNING at 50-149 commits ahead, and every announced SKIPPED.
# Run it once and show what it said either way. (Codex, #1542)
_bm_uit="$(python3 scripts/ci/branches_have_a_merge_request.py 2>&1)"; _bm=$?
printf '%s\n' "$_bm_uit" | sed 's/^/  /'
[ $_bm -eq 0 ] || { echo "LOCAL_CI_GATE: branch-mr FAILED" >&2; exit 1; }
run kosten   python3 scripts/ci/no_hosted_minutes_on_a_push.py
run instances python3 scripts/ci/one_instance_per_event.py
run infra     python3 scripts/ci/infra_health.py
run ci-yaml  python3 scripts/ci/ci_config_lint.py
run metadata cargo metadata --no-deps --format-version 1
run fmt      cargo fmt --all -- --check
run build    bash scripts/ci/run_build.sh
run clippy   bash scripts/ci/run_clippy.sh
run licenses python3 scripts/release/license_registry_check.py
if [ "$FULL" = 1 ]; then
  run test  bash scripts/ci/run_test.sh
  run audit bash scripts/ci/run_audit.sh
fi
echo "----------------------------------------"
if [ "$fail" = 0 ]; then echo "LOCAL_CI_GATE: PASS ($pass gates)"; exit 0
else echo "LOCAL_CI_GATE: FAIL ($fail of $((pass+fail)) gates failed)"; exit 1; fi
