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
# DISK FIRST, before anything compiles.
#
# On 01-09-2026 a docs-only commit was refused with LOCAL_CI_GATE: FAIL (2 of
# 33). Neither failure was real: the disk had fallen to 33 GB with three
# terminals building at once and the OS killed a build script --
# `signal: 9, SIGKILL`. A gate the kernel killed has judged nothing, so those
# two "failures" were the absence of a verdict wearing the costume of one.
#
# That is also why reaching for PRE_PUSH_SKIP=1 there would have been wrong: it
# skips everything the gate would have judged, not just the two that died.
#
# scripts/ci/disk_headroom.py already answers this in a second and names what
# can be swept. Running it first turns twenty minutes ending in SIGKILL into a
# refusal you can act on. It exits rather than continuing, because every gate
# below it would be measuring the disk rather than the code.
_dh_uit="$(python3 scripts/ci/disk_headroom.py 2>&1)"; _dh=$?
printf '%s\n' "$_dh_uit" | sed 's/^/  /'
if [ $_dh -ne 0 ]; then
  echo "LOCAL_CI_GATE: disk-headroom FAILED — stopping here." >&2
  echo "  Nothing below this point can be trusted: a build the kernel kills reports" >&2
  echo "  as a failing gate while having judged nothing. Sweep what is listed above" >&2
  echo "  and run again. Do not reach for PRE_PUSH_SKIP=1 — that skips every gate," >&2
  echo "  not the ones that died." >&2
  exit 1
fi

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
run jobsexist python3 scripts/ci/workflow_jobs_exist.py
run labels    python3 scripts/ci/every_label_has_a_runner.py
run startbaar python3 scripts/ci/every_workflow_can_start.py
run startbaartest python3 scripts/ci/test_every_workflow_can_start.py
run groen     python3 scripts/ci/a_gate_that_never_went_green.py
run groentest python3 scripts/ci/test_a_gate_that_never_went_green.py
run crons     python3 scripts/ci/schedule_guards_match_their_cron.py
run mirror    python3 scripts/ci/test_mirror_has_not_drifted.py
run infra     python3 scripts/ci/test_infra_health.py
run waitfit   python3 scripts/ci/wait_fits_in_the_job_timeout.py
run onelock   python3 scripts/ci/one_lock_guards_the_instances.py
run timers    python3 scripts/ci/persistent_timers_use_a_calendar.py
run installer python3 scripts/ci/test_installer_escapes_the_path.py
run upstream  python3 scripts/ci/upstream_has_not_moved_on.py
run forklist  python3 scripts/ci/fork_lists_agree.py
run patches   python3 scripts/ci/test_upstream_gap_counts_patches.py
run featgate  python3 scripts/ci/test_feature_gated_tests_run.py
run snippets  python3 scripts/ci/extract_site_snippets.py --check docs/site/snippets.json
run examples  cargo build -q --examples -p pdfluent
run matrix    python3 scripts/ci/capability_matrix_matches_coverage.py
run wordsep   python3 scripts/ci/never_delete_the_word_separator.py
run gitenv    python3 scripts/ci/test_no_test_can_touch_the_real_repo.py
run licenses2 python3 scripts/ci/license_gate.py
run lictest   python3 scripts/ci/test_license_gate.py
run errdocs   python3 scripts/ci/error_codes_have_an_anchor.py
run errtests  cargo test -q -p pdfluent --test error_codes_stable --test processing_limits
run prstale   python3 scripts/ci/pr_staleness.py
run sweep     python3 scripts/ci/test_sweep_spares_the_claimed_instance.py
run reaporder python3 scripts/ci/reaping_happens_after_the_work.py
# Advisory, never blocking. It reports on live machines -- a queue, a busy
# runner, a server someone is still using -- and none of that is a reason to
# refuse a commit. The three-hourly run is where its exit code matters.
echo "--- infra (advisory) ---"
python3 scripts/ci/infra_health.py 2>&1 | sed 's/^/  /' || true
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
