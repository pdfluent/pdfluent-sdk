#!/usr/bin/env bash
# local_ci_gate.sh — run the GitLab CI gates locally, before pushing.
#
# Calls the SAME scripts the pipeline calls (scripts/ci/run_*.sh) so this
# gate can never drift from CI. Run it before every push:
#     bash scripts/ci/local_ci_gate.sh            # sanity + lint (fast)
#     bash scripts/ci/local_ci_gate.sh --landing  # + the crates this change touches
#     bash scripts/ci/local_ci_gate.sh --full     # + the whole workspace, and the audit
#
# The hook picks the lane: a branch push takes the fast one, a push to master the
# landing one. LANE=full in the environment upgrades any of them to the full
# lane. The landing lane builds and tests only the crates the landing touches
# (scripts/ci/touched_crates.py); ci.yml's `workspace` job does all of them, on
# master, on the runner. (#343)
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
#   test     : scripts/ci/run_test.sh    (landing lane: the touched crates)
#   audit    : scripts/ci/run_audit.sh   (landing and full lanes; needs clean tree)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
FULL=0; LANDING=0
case "${1:-}" in
  --full)     FULL=1 ;;
  --landing)  LANDING=1 ;;
  --fast|"")  FULL=0 ;;
  *) echo "usage: local_ci_gate.sh [--fast|--landing|--full]" >&2; exit 2 ;;
esac
# LANE=full, from the environment, upgrades any lane to the whole workspace.
# It is the "on demand" half of the owner decision on #343 and the one escape
# that costs MORE work rather than less -- unlike PRE_PUSH_SKIP=1, which is the
# only other way to change what a push runs.
if [ "${LANE:-}" = full ]; then FULL=1; LANDING=0; fi
fail=0; pass=0; uitgesteld=0; ongeraakt=0; gevallen=""

# A DIRECTORY PER RUN, not a fixed /tmp/lcg_<name>.log.
#
# Two gates running at once wrote to the same paths, so each overwrote the
# other's evidence -- and on 02-09-2026 that is exactly what happened: one
# terminal opened the log of its own failing gate and read another terminal's
# passing run. There is no signal when this happens. The verdict on screen is
# yours, the file it points at is whoever finished last, and both look
# entirely ordinary.
#
# Three terminals share this repository and its worktrees, so concurrent runs
# are the normal case rather than the exception. The path is printed with every
# failure so a log can be traced back to the run that produced it.
LOGDIR="$(mktemp -d "${TMPDIR:-/tmp}/lcg.XXXXXX")"
export LOGDIR
# Said once at the start, not only when something fails: a reader who wants the
# output of a check that PASSED had nowhere to look, and on a machine where two
# runs are normal "the log" is not a location. Removed on a clean exit, kept on
# a failure -- the run that failed is the one whose evidence is wanted.
echo "logs: ${LOGDIR}"
# AND THE ONES THAT WERE KEPT, which nobody was ever going to remove by hand.
#
# The line above keeps the logs of a run that failed, on purpose: that is the
# evidence somebody wants. What it never said is who throws them away
# afterwards, and the answer was nobody -- they were still standing weeks
# later, next to the two-gigabyte build directories of the site-blocks gate
# that filled this disk on 05-09-2026 (#344). These are kilobytes rather than
# gigabytes, so this is tidiness and not a disk floor; a directory nobody
# removes is still a directory nobody removes.
#
# A day old, and only when this run itself is clean. No gate here takes a day
# -- the longest lane is under an hour -- so an `lcg.` directory that old
# belongs to no live run, and this can never take the evidence of one of the
# other two terminals while it is still being read.
sweep_logs() {
  [ "$fail" -eq 0 ] || return 0
  rm -rf "$LOGDIR"
  find "${TMPDIR:-/tmp}" -maxdepth 1 -type d -name 'lcg.*' -mtime +1 \
    -exec rm -rf {} + 2>/dev/null
  return 0
}
trap sweep_logs EXIT
run() { local name="$1"; shift
  printf '=== %-9s' "$name"
  if "$@" >"${LOGDIR}/${name}.log" 2>&1; then echo " PASS"; pass=$((pass+1))
  else echo " FAIL — see ${LOGDIR}/${name}.log"; tail -15 "${LOGDIR}/${name}.log" | sed 's/^/    /'; fail=$((fail+1)); gevallen="$gevallen $name"; fi
}
# The gates that cost minutes rather than seconds and only a landing needs. They
# run in the landing lane and in the full one; an ordinary branch push defers
# them, because CI runs on that same branch minutes later. On master nothing
# duplicates them: master takes only fast-forwards, so what passes here is what
# lands.
#
# DEFERRED IS NOT PASSED, and the summary counts it separately for that reason.
# A lane that quietly folded eight gates into the pass count would read exactly
# like a full run to anyone who did not know the lanes existed -- which is the
# shape of every measurement this repository has had to withdraw.
zwaar() { local name="$1"; shift
  if [ "$FULL" = 1 ] || [ "$LANDING" = 1 ]; then run "$name" "$@"
  else printf '=== %-9s DEFERRED (not a pass) — runs in the landing lane, on a push to master\n' "$name"
       uitgesteld=$((uitgesteld+1)); fi
}
# A gate that only says something about ONE crate. It runs when that crate is in
# the selection, and is SKIPPED -- counted apart from both passes and deferrals --
# when it is not. Three counters and not two, because "we did not need to" and
# "it passed" are different sentences and this file has been burned by pretending
# otherwise.
crate_gate() { local name="$1" crate="$2"; shift 2
  if [ "$FULL" = 1 ]; then run "$name" "$@"
  elif [ "$LANDING" = 1 ]; then
    case " $CRATES " in
      *" $crate "*) run "$name" "$@" ;;
      *) printf '=== %-9s SKIPPED (not a pass) — %s is not in this landing\n' "$name" "$crate"
         ongeraakt=$((ongeraakt+1)) ;;
    esac
  else printf '=== %-9s DEFERRED (not a pass) — runs in the landing lane, on a push to master\n' "$name"
       uitgesteld=$((uitgesteld+1)); fi
}
# The three that take the selection as an argument list rather than a single
# crate. With no crates at all they have nothing to compile, and that is a real
# answer: a change to scripts/ci or docs touches no crate, and building
# forty-nine of them proves nothing about it.
scoped() { local name="$1"; shift
  if [ "$FULL" = 1 ]; then run "$name" "$@"
  elif [ "$LANDING" = 1 ]; then
    if [ -z "$CRATES" ]; then
      printf '=== %-9s SKIPPED (not a pass) — this landing touches no crate\n' "$name"
      ongeraakt=$((ongeraakt+1))
    else
      # Word splitting on CRATES is the point: each name becomes its own
      # argument. Crate names carry no spaces -- cargo refuses them.
      # shellcheck disable=SC2086
      run "$name" "$@" $CRATES
    fi
  else printf '=== %-9s DEFERRED (not a pass) — runs in the landing lane, on a push to master\n' "$name"
       uitgesteld=$((uitgesteld+1)); fi
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
  # EXIT 3, not 1, and the hook prints its own line for it. Both states used to
  # leave the same sentence on screen -- "local CI gate failed" -- so a full disk
  # read as broken code, and the reader went looking for a gate that had never
  # run. A refusal that names the wrong cause costs more than no refusal.
  echo "LOCAL_CI_GATE: disk-headroom FAILED — stopping here." >&2
  echo "  Nothing below this point can be trusted: a build the kernel kills reports" >&2
  echo "  as a failing gate while having judged nothing. Sweep what is listed above" >&2
  echo "  and run again. Do not reach for PRE_PUSH_SKIP=1 — that skips every gate," >&2
  echo "  not the ones that died." >&2
  exit 3
fi

# THE CRATES THIS LANDING TOUCHES, and nothing else. (#343)
#
# Measured 05-09-2026 over that day's landings: 45 to 77 minutes each, one at a
# time, because the full lane compiles and tests all forty-nine crates in the
# landing worktree whatever the change was. A change to scripts/ci compiled the
# engine to prove nothing about itself.
#
# The owner decision was speed over a little certainty, and the certainty given
# up is named rather than implied: a crate outside the selection is not built or
# tested HERE. It is built and tested on the push to master, by the `workspace`
# job in .github/workflows/ci.yml, on the runner -- so a landing that broke
# something outside its own crates turns master red there instead of being
# refused here. That is the whole of the trade, and it holds only while that job
# exists. `LANE=full` restores the old behaviour for one run.
#
# SELECTED ONCE, before anything compiles, so a failure to decide is a refusal
# rather than a quiet fallback: touched_crates.py exits 3 when it cannot read the
# workspace, and the answer to "I do not know" is the whole workspace.
CRATES=""; SELECTION="the whole workspace"
if [ "$LANDING" = 1 ]; then
  _tc_uit="$(python3 scripts/ci/touched_crates.py 2>&1 >"${LOGDIR}/touched.txt")"; _tc=$?
  printf '%s\n' "$_tc_uit" | sed 's/^/  /'
  if [ $_tc -ne 0 ]; then
    echo "  touched_crates.py could not decide, so this run builds everything."
    FULL=1; LANDING=0
  elif [ "$(cat "${LOGDIR}/touched.txt")" = "*" ]; then
    echo "  a workspace-wide file changed, so this run builds everything."
    FULL=1; LANDING=0
  else
    CRATES="$(tr '\n' ' ' < "${LOGDIR}/touched.txt")"
    SELECTION="$(wc -w <<<"$CRATES" | tr -d ' ') crate(s): ${CRATES:-none}"
    echo "  landing lane: ${SELECTION}"
  fi
fi

# The workflow gates run here, above the branch/merge-request check, because
# that check exits the script outright. A branch far enough ahead without a pull
# request therefore hid every one of these -- and that is exactly the branch
# most likely to have broken something. The large unreviewed change and the
# checks that would have judged it were disqualifying each other. (codex, #1610)
run startbaar python3 scripts/ci/every_workflow_can_start.py
run startbaartest python3 scripts/ci/test_every_workflow_can_start.py
# ADVISORY IN BOTH LOCAL LANES, refusing only in ci.yml on master. It reads run
# history from the API, so what it reports is the state of the runners and of
# everyone else's workflows -- and on 04-09-2026 it blocked every push from every
# terminal three times over `cargo: command not found` on a runner. Nobody being
# refused could fix that, and a refusal you cannot act on is a stopped queue.
#
# It was advisory in the fast lane and hard in the full one from that day. That
# split does not survive contact: the full lane IS the landing, so a workflow
# somebody else broke this morning still closes master for everybody, and it did
# so a fourth time within 24 hours on publication-guards.yml -- new with #1712,
# red on all four of its runs, and nothing to do with whoever was landing. The
# owner decision is on #331.
#
# What this costs, said plainly because it is the same guard that was right twice
# on the day it was written: a workflow that dies after this lands is no longer
# noticed at the moment someone pushes or lands, only in ci.yml's own run on
# master. That is one link fewer on the guard built to find exactly that kind of
# silent death, and it is the link that was refusing work nobody could act on.
echo "--- groen (advisory in both local lanes; refuses in ci.yml on master) ---"
python3 scripts/ci/a_gate_that_never_went_green.py 2>&1 | sed 's/^/  /' || true
run groentest python3 scripts/ci/test_a_gate_that_never_went_green.py
# The visual suite renders and compares against a baseline, so it needs a
# release build. t1 added this line and took it out again in the same branch:
# local_ci_gate.sh was t2/t3's, and a t1 branch may not edit it. It comes back
# here because the crate is on loan to t3 for #326.
crate_gate visreg visual-regression cargo test -p visual-regression --release

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
# Counted, not fatal. This used to `exit 1` here, which aborted the run before
# any of the 53 gates below it -- so a branch 150 commits ahead without a pull
# request got NO checking at all, and the one condition that guarantees a long
# unmerged branch also guaranteed nothing else was looked at. That is the state
# in which the other guards matter most.
#
# Codex found it through the four fork guards this branch adds (#1639), but they
# were only the newest things standing behind the exit; moving those four in
# front of it would have made them arbitrarily special and left the other
# forty-nine where they were.
if [ $_bm -ne 0 ]; then
    echo "=== branch-mr  FAIL — see the lines above"
    fail=$((fail+1)); gevallen="$gevallen branch-mr"
fi
run kosten   python3 scripts/ci/no_hosted_minutes_on_a_push.py
run instances python3 scripts/ci/one_instance_per_event.py
run jobsexist python3 scripts/ci/workflow_jobs_exist.py
run pintest   python3 scripts/ci/test_every_action_is_pinned_or_recorded.py
run pinned    python3 scripts/ci/every_action_is_pinned_or_recorded.py
run labels    python3 scripts/ci/every_label_has_a_runner.py
# Its test belongs here and not in a workflow, for the reason the guard states
# itself: it asks the GitHub API which runners carry which labels, and that
# needs a token a pull request from a fork must never have. In Actions the
# guard returns SKIPPED (not a pass) -- so the test, which asserts that the
# repository as it stands passes, cannot succeed there. #1709 wired it into
# deletions-declare-themselves.yml and made that workflow red on every run.
run labelstest python3 scripts/ci/test_every_label_has_a_runner.py
run crons     python3 scripts/ci/schedule_guards_match_their_cron.py
run mirror    python3 scripts/ci/test_mirror_has_not_drifted.py
run mirrorsync python3 scripts/ci/test_mirror_to_gitlab.py
run topology  python3 scripts/ci/the_topology_agrees_with_the_mirror_gate.py
run topologyt python3 scripts/ci/test_the_topology_agrees_with_the_mirror_gate.py
# THE MIRROR GATE ITSELF, and this is the only place in the repository where it
# can run: it compares `origin/master` with `gitlab/master`, and a checkout with
# both remotes in it is the working copy, not an Actions workspace. Registered as
# running here since 31-08-2026 and in fact called by nothing -- so between
# 02-09 and 05-09 the backup fell 348 commits behind and 8 commits ahead at the
# same time, for three days, in silence. That is the exact failure the guard was
# written to end, happening behind the guard. (#231)
#
# DEFERRED IN THE FAST LANE, refusing in the full one. The full lane is the
# landing, and a mirror that has stopped is fixable on the spot by whoever is
# landing -- `bash scripts/infra/mirror_to_gitlab.sh` -- which is what separates
# this from the refusals that closed master four times in one day over other
# people's workflows. An unreachable GitLab does not refuse anything: with
# --fetch the guard downgrades to a warning when it cannot read both sides,
# because the drift is then of unknown age.
zwaar mirrordrift python3 scripts/ci/mirror_has_not_drifted.py --fetch
# WHERE THE CODE LIVES, and the second guard that only this checkout can answer.
# `origin` pointed at the GitLab backup here while GitHub had been the source
# since 25-08-2026, so every guard falling back to `origin/master` compared
# against a repository 348 commits behind -- 62 files reported as a branch's own
# work when none of them were. And a repository with two checkouts on one disk
# has two answers to every question; a finished lopdf upgrade sat stranded in the
# second one. Both are machine states, invisible to an Actions workspace that has
# one remote and one checkout by construction. (#291)
#
# In the full lane for the reason the mirror gate is: this is the landing, and a
# duplicate checkout or a misnamed remote is fixable on the spot by whoever is
# landing. Its own test runs in the guards job on GitHub, on fixtures.
zwaar checkouts python3 scripts/ci/origin_is_the_source_in_every_checkout.py
run infra     python3 scripts/ci/test_infra_health.py
run waitfit   python3 scripts/ci/wait_fits_in_the_job_timeout.py
run onelock   python3 scripts/ci/one_lock_guards_the_instances.py
run timers    python3 scripts/ci/persistent_timers_use_a_calendar.py
run installer python3 scripts/ci/test_installer_escapes_the_path.py
run upstream  python3 scripts/ci/upstream_has_not_moved_on.py
run forklist  python3 scripts/ci/fork_lists_agree.py
run patches   python3 scripts/ci/test_upstream_gap_counts_patches.py
# The four fork-register guards. Two of them ran nowhere at all -- not in a job,
# not in this gate -- so they looked like protection and were not. The wiring was
# named as t2's part when they were written (#1609) and then lost twice: once
# when it drowned in the #1543 relay, and once when the relay's merge was rebuilt
# from scratch and only the CONFLICTED paths were carried over. This file was not
# conflicted, so the edit to it stayed behind in the discarded tree. (#296)
run forkreg   python3 scripts/ci/the_fork_register_is_verifiable.py
run forkpunt  python3 scripts/ci/een_forkpunt_wordt_op_inhoud_gecontroleerd.py
run wtconfig  python3 scripts/ci/de_gedeelde_config_breekt_geen_worktrees.py
run forkmerge python3 scripts/ci/een_fork_zonder_forkpunt_wordt_niet_gemerged.py
run featgate  python3 scripts/ci/test_feature_gated_tests_run.py
run featgatesrc python3 scripts/ci/test_coverage_reads_only_what_runs.py
run snapfifty python3 scripts/ci/test_the_snapshot_recovers_the_fifty.py
# ADVISORY here, blocking in ci.yml (line 167). It says what it sees and does not
# refuse the push.
#
# Why it moved: the map is committed, so a change that legitimately spans two
# territories cannot be pushed until the claim for it is already in the tree --
# and the claim travels in the same commit. That circle produced the loan lines,
# which are a claim written to get past a guard rather than to describe the work,
# and a register full of those describes nothing.
#
# What is given up is real and worth naming: a push can now cross a line with
# only a warning on screen, and a warning in a run that prints hundreds of lines
# is a warning that can be missed. What is not given up is the refusal -- ci.yml
# still fails the pull request, before anything merges, where a person reads it.
# This trades the moment of the refusal, not the refusal.
echo "--- territory (advisory; ci.yml refuses) ---"
python3 scripts/ci/territories_do_not_overlap.py 2>&1 | sed 's/^/  /' || true
run regexem   python3 scripts/ci/test_register_exemption.py
run prreach   python3 scripts/ci/every_gate_is_reachable_from_a_pull_request.py
run prreacht  python3 scripts/ci/test_every_gate_is_reachable_from_a_pull_request.py
run header    python3 scripts/ci/header_sweep.py
run sweepexem python3 scripts/ci/test_sweep_exemption.py
run notice    python3 scripts/ci/notice_names_what_exists.py
run noticetst python3 scripts/ci/test_notice_names_what_exists.py
run withdrawn python3 scripts/ci/no_withdrawn_object.py
run withdrawnt python3 scripts/ci/test_no_withdrawn_object.py
run snapaudit bash scripts/ci/public_snapshot_audit.sh
# Wired 05-09-2026 (#222). It answers "does the tree that would be published
# name a document we may not redistribute", it had run in no job since it was
# written, and on the day it was wired it was red -- on a doc comment in
# pdf-interpret and on a facade test that embedded a golden by path.
#
# The guard is 1.3 seconds and belongs in the fast lane: what it watches is
# ordinary source, so any push can break it. Its test builds twenty-six git
# repositories and is 31 seconds, which is the full lane's kind of cost, and
# what it watches is the guard itself -- which only a change to scripts/ci can
# break, and that change lands through the full lane on master.
run internal  python3 scripts/ci/internal_stays_internal.py
zwaar interntst python3 scripts/ci/test_internal_stays_internal.py
run territst  python3 scripts/ci/test_territories_do_not_overlap.py
run deadhost  python3 scripts/ci/no_dead_host_in_a_connecting_script.py
run snippets  python3 scripts/ci/extract_site_snippets.py --check docs/site/snippets.json
run siteblkreg python3 scripts/ci/test_site_blocks_register.py
run gatelogs   python3 scripts/ci/test_gate_logs_do_not_pile_up.py
# The cookbook and the two cross-links of #167. Same family as the two above and
# the same cost -- no compiler, so both halves sit in the fast lane. What only
# this one watches is the seam between the repositories: the site links a reader
# straight at site_snippets.rs in the PUBLISHED repository, and a path the
# seeding strips would 404 there while everything here stayed green.
run cookbook  python3 scripts/ci/cookbook_recipes.py --check
run cookbooktst python3 scripts/ci/test_cookbook_recipes.py
# The site's own Rust, compiled. Heavy, because it builds five hundred small
# programs against the real facade -- so it sits in the full lane, where it runs
# on a push to master, and not in the fast one on every branch push.
#
# It is the only thing that catches a real name in a shape that does not exist.
# `use pdfluent::Sdk;` was on the documentation page for weeks with the name
# check green: `Sdk` is not lowercase so it is not a module path, and nothing
# follows the `::` so it is not a type use. The most common way to be wrong fell
# exactly between the two patterns (#164, #247).
crate_gate siteblocks pdfluent python3 scripts/ci/site_blocks_compile.py --check
crate_gate examples pdfluent cargo build -q --examples -p pdfluent
run matrix    python3 scripts/ci/capability_matrix_matches_coverage.py
run wordsep   python3 scripts/ci/never_delete_the_word_separator.py
# The PDF/A comparison harness, which is how #189's Ghostscript numbers were
# produced. Its arithmetic is what a published comparison rests on, and every
# case in the test is a mistake that was made while measuring and came back as
# a number that looked like a result -- veraPDF's `validationResult` read as an
# object instead of a list (everything 0%), documents the converter refused
# dropped from the sample, a page rendered at another size called "not
# comparable" and skipped. It needs `node`, which this directory needs anyway:
# reproduce.mjs and compare.mjs are what it holds.
run pdfacomp  node --test benchmarks/pdfa/reproduce/compare.test.mjs
run gitenv    python3 scripts/ci/test_no_test_can_touch_the_real_repo.py
run prrunner  python3 scripts/ci/pr_code_stays_off_the_desktop.py
run prruntst  python3 scripts/ci/test_pr_code_stays_off_the_desktop.py
run licenses2 python3 scripts/ci/license_gate.py
run fetched   python3 scripts/ci/every_fetched_asset_is_registered.py
run fetchedt  python3 scripts/ci/test_every_fetched_asset_is_registered.py
run licbound  python3 scripts/ci/license_boundary.py
run licbtest  python3 scripts/ci/test_license_boundary.py
# The document the licence hands the last word to. LICENSE-COMMERCIAL §10 says
# the agreement is that text plus a signed order form, and that the form
# prevails -- so the form and §2 drifting apart sells or withholds a right that
# neither document admits to (#220).
run orderform python3 scripts/ci/the_order_form_agrees_with_the_licence.py
run orderftst python3 scripts/ci/test_the_order_form_agrees_with_the_licence.py
run lictest   python3 scripts/ci/test_license_gate.py
run javafix   python3 scripts/ci/the_java_fixture_is_the_one_we_generate.py
run fixenv    python3 scripts/ci/a_fixture_cannot_touch_a_real_repo.py
run fixenvtst python3 scripts/ci/test_a_fixture_cannot_touch_a_real_repo.py
run licpol    python3 scripts/ci/the_licence_policy_says_what_it_must.py
run licpoltst python3 scripts/ci/test_the_licence_policy_says_what_it_must.py
run licregs   python3 scripts/ci/one_licence_three_registers.py
run licregtst python3 scripts/ci/test_one_licence_three_registers.py
run errdocs   python3 scripts/ci/error_codes_have_an_anchor.py
crate_gate errtests pdfluent cargo test -q -p pdfluent --test error_codes_stable --test processing_limits
# The two lines t1 wrote and then reverted on #324, because this file was not
# theirs and they said so: "the wiring belongs in a PR by an owner of that file".
# Until it arrived the guard and its test ran nowhere, which is the state
# every_test_is_run.py is built to refuse -- and it has been refusing it on every
# pull request since, for a reason no author of one could fix. This is that PR.
run diagcat   python3 scripts/ci/every_diagnostic_code_is_documented.py
run diagcattst python3 scripts/ci/test_every_diagnostic_code_is_documented.py
# The merge point, not a formality: master only takes fast-forwards, so what
# passes here is what lands. #316.
run signoff   python3 scripts/ci/every_commit_since_the_cutoff_is_signed.py
run signofftst python3 scripts/ci/test_every_commit_since_the_cutoff_is_signed.py
run signoffcut python3 scripts/ci/test_signoff_respects_the_cutoff.py
run prstale   python3 scripts/ci/pr_staleness.py
run prstaletst python3 scripts/ci/test_pr_staleness.py
run sweep     python3 scripts/ci/test_sweep_spares_the_claimed_instance.py
run sweepsafe python3 scripts/ci/test_sweep_fails_safe.py
run regfetch  python3 scripts/ci/test_de_registerwachters_raken_de_echte_repo_niet.py
run reaporder python3 scripts/ci/reaping_happens_after_the_work.py
run guardrun  python3 scripts/ci/guards_do_not_hide_behind_each_other.py
run buildmnt  bash scripts/ci/test_desktop_env.sh
run buildheal bash scripts/ci/test_cargo_target_health.sh
run builddir  python3 scripts/ci/shared_build_dir_fails_loudly.py
run builddir2 python3 scripts/ci/test_persistent_builds_check_their_build_dir.py
run builddir3 python3 scripts/ci/persistent_builds_check_their_build_dir.py
run diskguard python3 scripts/ci/test_disk_headroom.py
# And the identity on the commits this branch adds. The pre-commit hook refuses
# one before it exists; this catches what a `--no-verify`, or a clone without
# core.hooksPath set, let through -- before it reaches a remote.
run identtest python3 scripts/ci/test_commits_use_the_noreply_alias.py
run identity  python3 scripts/ci/commits_use_the_noreply_alias.py
# The same rule where it lasts longest. Archiving a branch as a `keep/` tag
# converts commits on a name somebody will delete into commits nothing deletes,
# and the history-rewrite plan carries tags across -- so a tag over a personal
# address is a promise to keep publishing it (#314).
run tagtest   python3 scripts/ci/test_an_archive_tag_pins_no_personal_address.py
run archtags  python3 scripts/ci/an_archive_tag_pins_no_personal_address.py
# And the same address in the tree rather than in a commit field, which is the
# half a guard on author/committer cannot see.
# The message guards, and the wiring that makes them run at all.
#
#   hookwire : the commit-msg hook exists, calls both message guards, and this
#              clone's core.hooksPath actually points at it. HERKOMST.md claimed
#              that hook since 26-08-2026; on 02-09 there was no commit-msg hook
#              in the tree and the guard it named lived on an unmerged branch.
#   noai     : catches attribution the hook could not reach -- a clone that
#              never installed it, a rebase, a cherry-pick, --no-verify.
#   msgclean : the same for internal matters, over the messages on this branch.
run lanetest  python3 scripts/ci/test_the_pre_push_gate_picks_its_lane.py
# The crate selection the landing lane above is built on, and the one place
# scripts/ci/touched_crates.py is run as a check rather than as a helper: it
# answers the same question every landing asks, over fabricated workspaces
# whose answers are known. A selection that silently returned too little
# would leave a crate unbuilt on the machine AND unnamed in the summary.
run touchedtst python3 scripts/ci/test_touched_crates.py
# And the one mechanical claim the selection makes about somewhere else: the
# crates it must never name are the crates the run scripts exclude.
run touchedexcl python3 scripts/ci/touched_crates.py --check-excludes
run hookwire  python3 scripts/ci/the_commit_msg_hook_is_wired.py
run hookwiretst python3 scripts/ci/test_the_commit_msg_hook_is_wired.py
run jobimports python3 scripts/ci/a_job_has_what_its_scripts_import.py
run jobdoes   python3 scripts/ci/every_job_does_something.py
run jobdoestst python3 scripts/ci/test_every_job_does_something.py
run noai      python3 scripts/ci/no_ai_attribution.py
# 170 seconds, measured 05-09-2026, and it is the most expensive Python gate
# here. It was 76 before the replacement list (#222) added six more cases, four
# of which run the script end to end again -- the ones that prove a term is gone
# from every commit of the repository that would be published, and that the list
# cannot be used to rewrite anything a rule does not call internal.
#
# It runs `seed_public_repo.sh` end to end ten times over a four-commit fixture,
# because
# the properties that matter -- an internal path dropped, a withdrawn blob gone
# under any name, a personal address rewritten, a surviving file unchanged -- are
# properties of the repository that would be PUBLISHED, and none of them can be
# read off what the script prints. It stays in the fast lane rather than moving
# to `zwaar`: this is the one operation in this repository that cannot be undone,
# and finding out on master that its filter broke is finding out too late.
run seedtest  python3 scripts/ci/test_seed_public_repo.py
run noaitest  python3 scripts/ci/test_no_ai_attribution.py
run msgclean  python3 scripts/ci/geen_interne_zaken.py --bereik github/master..HEAD
run msgredact python3 scripts/ci/test_een_treffer_publiceert_de_term_niet.py

run treetest  python3 scripts/ci/test_no_personal_address_in_the_tree.py
run treeaddr  python3 scripts/ci/no_personal_address_in_the_tree.py
run oldreptest python3 scripts/ci/test_no_tracked_file_names_the_old_repository.py
run oldrepo   python3 scripts/ci/no_tracked_file_names_the_old_repository.py
# Advisory, never blocking. It reports on live machines -- a queue, a busy
# runner, a server someone is still using -- and none of that is a reason to
# refuse a commit. The three-hourly run is where its exit code matters.
echo "--- infra (advisory) ---"
python3 scripts/ci/infra_health.py 2>&1 | sed 's/^/  /' || true
run ci-yaml  python3 scripts/ci/ci_config_lint.py
run hangclass python3 scripts/ci/test_classify_render_outcome.py
run metadata cargo metadata --no-deps --format-version 1
run fmt      cargo fmt --all -- --check
scoped build  bash scripts/ci/run_build.sh
scoped clippy bash scripts/ci/run_clippy.sh
run licenses python3 scripts/release/license_registry_check.py
run advlists  python3 scripts/ci/the_advisory_lists_agree.py
run advliststst python3 scripts/ci/test_the_advisory_lists_agree.py
run docsrefs python3 scripts/ci/docs_references_resolve.py
run dcohook   python3 scripts/ci/test_prepare_commit_msg_signoff.py
scoped test  bash scripts/ci/run_test.sh
# The publish audit, which is about the packages this repository ships rather
# than about one crate. It stays whole in both landing lanes: it packages the
# release channels and reads their manifests, so scoping it to a crate list
# would answer a different question than the one it was written for.
zwaar audit bash scripts/ci/run_audit.sh
echo "----------------------------------------"
_lane="fast"; [ "$FULL" = 1 ] && _lane="full"; [ "$LANDING" = 1 ] && _lane="landing"
_uit=""; [ "$uitgesteld" -gt 0 ] && _uit=", $uitgesteld deferred to the landing lane"
# Named separately from the deferrals, and never folded into the pass count: a
# gate that was skipped because its crate is not in this landing has judged
# nothing, and the crates it would have judged are named on the line above.
[ "$ongeraakt" -gt 0 ] && _uit="$_uit, $ongeraakt skipped (crate not touched)"
[ "$LANDING" = 1 ] && _uit="$_uit; scope: $SELECTION"
if [ "$fail" = 0 ]; then echo "LOCAL_CI_GATE: PASS ($pass gates, $_lane lane$_uit)"; exit 0
else
  # The names again, at the bottom. They were printed when each one failed, but a
  # run prints hundreds of lines and the caller often shows only the tail -- so
  # the one thing the reader needs was the most likely thing to be cut off.
  echo "LOCAL_CI_GATE: FAIL ($fail of $((pass+fail)) gates failed, $_lane lane$_uit)"
  echo "failed: ${gevallen# }"
  exit 1
fi
