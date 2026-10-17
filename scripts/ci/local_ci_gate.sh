#!/usr/bin/env bash
# local_ci_gate.sh — run the GitLab CI gates locally, before pushing.
#
# Calls the SAME scripts the pipeline calls (scripts/ci/run_*.sh) so this
# gate can never drift from CI. Run it before every push:
#     bash scripts/ci/local_ci_gate.sh            # sanity + lint (fast)
#     bash scripts/ci/local_ci_gate.sh --landing  # + the crates this change touches
#     bash scripts/ci/local_ci_gate.sh --full     # + the whole workspace, and the audit
#     bash scripts/ci/local_ci_gate.sh --preflight-only [--lane]
#                                                 # just the requirements check (m482)
#
# The hook picks the lane: a branch push takes the fast one, a push to master the
# landing one. LANE=full in the environment upgrades any of them to the full
# lane. The landing lane builds and tests only the crates the landing touches
# (scripts/ci/touched_crates.py); the whole workspace is done by a wave of
# .github/workflows/ci-ephemeral.yml, on master changes, on a Hetzner instance
# bought for it and thrown away. (#343, #350)
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
FULL=0; LANDING=0; PRE_ONLY=0
# --preflight-only answers what the lane would need and runs no gate. It takes
# an optional lane behind it (--preflight-only --landing); without one it is
# the fast lane, like a bare call. It takes no lock, asks no disk and refuses
# on nothing but the requirements themselves -- it reads, it does not compile.
case "${1:-}" in
  --preflight-only) PRE_ONLY=1; shift ;;
esac
case "${1:-}" in
  --full)     FULL=1 ;;
  --landing)  LANDING=1 ;;
  --fast|"")  FULL=0 ;;
  *) echo "usage: local_ci_gate.sh [--preflight-only] [--fast|--landing|--full]" >&2; exit 2 ;;
esac
# LANE=full, from the environment, upgrades any lane to the whole workspace.
# It is the "on demand" half of the owner decision on #343 and the one escape
# that costs MORE work rather than less -- unlike PRE_PUSH_SKIP=1, which is the
# only other way to change what a push runs.
if [ "${LANE:-}" = full ]; then FULL=1; LANDING=0; fi
# EAGERLY, before anything is measured or any lock is taken. A misspelt place
# has to refuse in the first second: read lazily at the first gate it would sit
# in the lock queue for hours and then refuse, and a run that spends an hour to
# say "I did not understand you" is a run nobody reads to the end.
case "${GATE_LANE_PLACE:-}" in
  ""|remote|machine) ;;
  *) echo "LOCAL_CI_GATE: GATE_LANE_PLACE must be remote or machine, not '${GATE_LANE_PLACE}'" >&2
     echo "  NOTHING WAS CHECKED. Unset it to run every gate here." >&2
     exit 2 ;;
esac
fail=0; pass=0; uitgesteld=0; ongeraakt=0; gevallen=""
# WHERE A GATE IS JUDGED, when the lane is split across two machines (#474).
#
#   unset    every gate runs here, which is what a plain local run has always
#            been and stays
#   remote   the gates that need this operator's identity are left for the Mac
#   machine  only those, and nothing else -- the few seconds the Mac still owes
#
# ELSEWHERE is counted apart from passes and deferrals, for the reason the other
# two counters exist: "somebody else is judging this" and "this passed" are
# different sentences, and a lane that folded one into the other would read like
# a complete run to anybody who did not know the split existed.
PLAATS="${GATE_LANE_PLACE:-}"
ELDERS_LIJST=""
if [ -n "$PLAATS" ] && [ -f scripts/ci/gates_that_need_this_machine.txt ]; then
  ELDERS_LIJST=" $(sed -e 's/#.*//' scripts/ci/gates_that_need_this_machine.txt | tr -s '[:space:]' ' ') "
fi
elders=0
elders_zeg() {
  printf '=== %-9s ELSEWHERE (not a pass) — needs the machine the operator is at; it runs there\n' "$1"
  elders=$((elders+1))
}
hoort_hier() {
  case "$PLAATS" in
    "") return 0 ;;
    remote)  case "$ELDERS_LIJST" in *" $1 "*) return 1 ;; *) return 0 ;; esac ;;
    machine) case "$ELDERS_LIJST" in *" $1 "*) return 0 ;; *) return 1 ;; esac ;;
    *) echo "LOCAL_CI_GATE: GATE_LANE_PLACE must be remote or machine, not '$PLAATS'" >&2; exit 2 ;;
  esac
}

# WHERE THIS WORKTREE IS, recorded before anything is measured against it.
#
# Three terminals share this repository and its worktrees, and on 06-09-2026 a
# second terminal checked the shared landing worktree out onto master for
# twenty-six seconds while a landing gate was six minutes into compiling in it.
# The gate said nothing, because nothing asks. The tree came back to the commit
# it started on, so even a before/after comparison of `git rev-parse HEAD` would
# have passed -- see scripts/ci/_the_tree_has_not_moved.sh for why the reflog
# length is the field that remembers the trip.
#
# Checked before every expensive phase and again before the verdict, so the
# refusal names the last gate that was still measuring the right tree.
TREE_BASELINE="$(bash scripts/ci/_the_tree_has_not_moved.sh --record)"
# EXIT 4, like the disk refusal at exit 3 and unlike an ordinary failure: no
# gate here has judged anything wrong. The tree the verdict was about stopped
# being the tree on disk, which is the absence of a verdict rather than a bad
# one, and continuing would spend another half hour measuring the wrong thing.
tree_check() {
  bash scripts/ci/_the_tree_has_not_moved.sh --check "$TREE_BASELINE" && return 0
  echo "LOCAL_CI_GATE: tree moved during the run — stopping here." >&2
  echo "  NOTHING WAS CHECKED about the branch you are pushing: every gate above" >&2
  echo "  this line measured a tree that stopped being yours partway through." >&2
  echo "  Run the gate again in a worktree nobody else is standing in. Do not" >&2
  echo "  reach for PRE_PUSH_SKIP=1 — that skips every gate rather than repeating" >&2
  echo "  the one that was disturbed. The hook's next line will say the gate" >&2
  echo "  failed; it did not judge your code at all." >&2
  failed_before_the_gates tree-moved "the worktree moved during the run; nothing was judged"
  exit 4
}

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
. "$(dirname "$0")/_failed_gates_block.sh"
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
# ONE GATE ON THIS MACHINE AT A TIME, fast lane and landing lane alike (#359).
#
# Measured 07-09-2026, 01:20 to 02:10Z: three gates at once on ten cores took
# the machine to load 124, each rustc got three to ten per cent of a core, one
# landing gate sat four hours in `cargo test`, and the verified measurement
# behind it waited three hours for its turn. The landing lock serialises
# landings; nothing serialised the push gates running around them, and that is
# where the cores went.
#
# The lock is taken BEFORE the disk floor and before anything else here. Waiting
# first and measuring after is the right order twice over: the gates below are
# what has to be serialised, and the disk this run is about to be judged on is
# the disk as it will be while THIS gate runs, not as it was behind somebody
# else's half-finished build.
#
# scripts/ci/_one_gate_at_a_time.sh holds the mechanism and the reasons; the
# short of it is a FIFO queue so arrival order is service order, and a landing
# holder that is never broken on age.
#
# ANYTHING ELSE ON THIS MACHINE THAT COMPILES waits in the same line by calling
#   bash scripts/ci/with_gate_lock.sh -- <command>
# and not by polling /tmp/xfa-gate.lock itself: a waiter that does not append to
# the queue is not in the line, and loses to every FIFO waiter that arrives
# after it. The queue's file formats are written out in the PROTOCOL section of
# _one_gate_at_a_time.sh, for reading the state -- not for a second copy of it.
GATE_LOCK_HELD=0
release_the_gate_lock() {
  [ "$GATE_LOCK_HELD" = 1 ] || return 0
  GATE_LOCK_HELD=0
  bash scripts/ci/_one_gate_at_a_time.sh --release "$$" >/dev/null 2>&1
  return 0
}
# AND THE WHOLE LANE IS WATCHED, not each fixture separately (#454). Four leaks
# in two days reached the shared checkout from a fixture -- core.bare true twice,
# github/master moved, the remote at example.invalid, the identity at
# gate@invalid -- and each was found by the next person to be blocked by it,
# hours later, in another worktree. The individual fixtures record and check this
# sentinel themselves, but a lane runs 189 gates and any of them can write; one
# reading at the start and one at the end names the field for whichever gate did
# it, without asking every future gate to remember.
#
# ADVISORY, on purpose. The verdict of the lane is about the branch; a machine
# that another terminal's fixture damaged is not this branch's fault, and a red
# that makes the pusher hunt somebody else's bug is the kind of red people learn
# to ignore. It is loud, it says which field, and it says who can put it back.
LANE_CHECKOUT_BASELINE="$(bash scripts/ci/_the_checkout_is_unchanged.sh --record 2>/dev/null || echo geen-checkout)"
checkout_sentinel() {
  bash scripts/ci/_the_checkout_is_unchanged.sh --check "$LANE_CHECKOUT_BASELINE" >/dev/null 2>&1 && return 0
  echo "=== WARNING: this lane changed the shared checkout (advisory; the branch is not the suspect)" >&2
  bash scripts/ci/_the_checkout_is_unchanged.sh --check "$LANE_CHECKOUT_BASELINE" >&2 || true
  return 0
}
trap 'release_the_gate_lock; checkout_sentinel; sweep_logs' EXIT
# A shell killed by an untrapped TERM does not exit, it dies, and bash runs no
# EXIT trap for it -- which would leave the lock standing until the next waiter
# noticed the pid was gone. Both signals become an ordinary exit so the lock is
# handed back by the same path as a clean run. Terminals do kill gates: that is
# how the four-hour run on 07-09 ended.
trap 'exit 130' INT
trap 'exit 143' TERM
_baan_naam=fast; [ "$LANDING" = 1 ] && _baan_naam=landing; [ "$FULL" = 1 ] && _baan_naam=full
# THE MACHINE HALF DOES NOT QUEUE. The lock exists so that two compilations do
# not share one Mac; the five gates that need this operator's identity read a
# keyring and two text files and are over in seconds. Making them wait behind a
# landing would put back exactly the queue the remote gate was built to empty.
if [ "$PRE_ONLY" = 1 ]; then
  GATE_LOCK_HELD=0
  echo "gate lock: not taken — --preflight-only reads, it does not compile"
elif [ "$PLAATS" = machine ]; then
  GATE_LOCK_HELD=0
  echo "gate lock: not taken — this half judges only the gates that need this machine, and compiles nothing"
elif bash scripts/ci/_one_gate_at_a_time.sh --acquire "$_baan_naam" "$$"; then
  GATE_LOCK_HELD=1
else
  # EXIT 5, next to the disk refusal at 3 and the moved-tree refusal at 4. No
  # gate below has judged anything: this run never started. Saying "the gate
  # failed" here would send the reader looking for a defect that was never
  # measured.
  echo "LOCAL_CI_GATE: another gate has held this machine past the wait — stopping here." >&2
  echo "  NOTHING WAS CHECKED. Find the holder with" >&2
  echo "    bash scripts/ci/_one_gate_at_a_time.sh --holder" >&2
  echo "  and run again once it is done. Do not reach for PRE_PUSH_SKIP=1 -- that" >&2
  echo "  pushes code no gate has read." >&2
  exit 5
fi
# CARGO GETS THE CORES MINUS TWO (#359).
#
# A gate that takes every core makes the machine it runs on unusable, and the
# person at the keyboard is the reason this repository exists. Two cores is the
# difference between an editor that types and one that does not; it costs a gate
# a few per cent of its wall time now that gates no longer run three at a time.
#
# Exported rather than passed as `-j`: run_build.sh, run_clippy.sh, run_test.sh
# and run_audit.sh each call cargo themselves, and an environment variable
# reaches all of them without four edits that can drift apart.
CARGO_BUILD_JOBS="$(bash scripts/ci/_one_gate_at_a_time.sh --jobs)"
export CARGO_BUILD_JOBS
echo "cargo -j ${CARGO_BUILD_JOBS} (physical cores minus two, so the machine stays usable)"
# EVERY GATE IS TIMED, and the times survive the run (#425).
#
# The lane was nineteen minutes and nobody knew which gates were spending it.
# Finding that out took an ad-hoc patch of this function, a full run, and then
# throwing the patch away -- which is exactly the measurement that will not be
# repeated the next time the lane creeps. So the stopwatch lives here now, the
# slowest gates are printed at the end of every run, and the whole table is
# appended to TIMINGS for the budget guard to read.
#
# LOGDIR is swept on a passing run, so the table cannot live there.
TIMINGS="${XFA_GATE_TIMINGS:-${TMPDIR:-/tmp}/xfa-gate-timings.tsv}"
run() { local name="$1"; shift
  if ! hoort_hier "$name"; then
    [ "$PLAATS" = remote ] && elders_zeg "$name"
    return 0
  fi
  if [ "$FULL" != 1 ]; then
    case " $DEVELOPMENT_DEFERRED " in
      *" $name "*)
        printf '=== %-9s DEFERRED (not a pass) — infrastructure regression; runs in CI or --full\n' "$name"
        uitgesteld=$((uitgesteld+1)); return 0 ;;
    esac
  fi
  if [ "$LANDING" = 1 ] && [ "$FULL" != 1 ]; then
    case " $LANDING_OUT_OF_REACH " in
      *" $name "*)
        printf '=== %-9s SKIPPED (not a pass) — a test this landing cannot reach; runs nightly on master\n' "$name"
        ongeraakt=$((ongeraakt+1)); return 0 ;;
    esac
  fi
  local _t0=$(date +%s)
  printf '=== %-9s' "$name"
  if "$@" >"${LOGDIR}/${name}.log" 2>&1; then echo " PASS"; pass=$((pass+1))
  else echo " FAIL — see ${LOGDIR}/${name}.log"; tail -15 "${LOGDIR}/${name}.log" | sed 's/^/    /'; fail=$((fail+1)); gevallen="$gevallen $name"; fi
  printf '%s\t%s\n' "$name" "$(( $(date +%s) - _t0 ))" >> "${LOGDIR}/timings.tsv"
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
  hoort_hier "$name" || { [ "$PLAATS" = remote ] && elders_zeg "$name"; return 0; }
  if [ "$FULL" = 1 ] || [ "$LANDING" = 1 ]; then tree_check; run "$name" "$@"
  else printf '=== %-9s DEFERRED (not a pass) — runs in the landing lane, on a push to master\n' "$name"
       uitgesteld=$((uitgesteld+1)); fi
}
# A gate that only says something about ONE crate. It runs when that crate is in
# the selection, and is SKIPPED -- counted apart from both passes and deferrals --
# when it is not. Three counters and not two, because "we did not need to" and
# "it passed" are different sentences and this file has been burned by pretending
# otherwise.
crate_gate() { local name="$1" crate="$2"; shift 2
  hoort_hier "$name" || { [ "$PLAATS" = remote ] && elders_zeg "$name"; return 0; }
  if [ "$FULL" = 1 ]; then tree_check; run "$name" "$@"
  elif [ "$LANDING" = 1 ]; then
    case " $CRATES " in
      *" $crate "*) tree_check; run "$name" "$@" ;;
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
  hoort_hier "$name" || { [ "$PLAATS" = remote ] && elders_zeg "$name"; return 0; }
  if [ "$FULL" = 1 ]; then tree_check; run "$name" "$@"
  elif [ "$LANDING" = 1 ]; then
    if [ -z "$CRATES" ]; then
      printf '=== %-9s SKIPPED (not a pass) — this landing touches no crate\n' "$name"
      ongeraakt=$((ongeraakt+1))
    else
      # Word splitting on CRATES is the point: each name becomes its own
      # argument. Crate names carry no spaces -- cargo refuses them.
      tree_check
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
# THE INTERPRETER THIS LANE RUNS ON, before any gate uses it (#425).
#
# Two ways this machine has cost a terminal an afternoon, both of them a guard
# failing for a reason that has nothing to do with the code it was reading:
#
#   CERTIFICATE_VERIFY_FAILED. The framework python here has no CA bundle of its
#   own, so every guard that reaches the network dies on certificate
#   verification -- and it dies inside the guard, so the message on screen is
#   about an upstream check rather than about this machine. certifi is installed
#   and knows where the bundle is; nothing was pointing at it.
#
#   ModuleNotFoundError: yaml. every_workflow_can_start parses the workflows, and
#   without pyyaml it fails as a red gate rather than as a machine that is not
#   set up.
#
# Both are answered here, once, in the open, before anything is judged. Naming
# what is missing and how to install it is the whole point: a terminal that hits
# this at gate 90 of 145 has already paid twenty minutes for the answer.
# Neither the interpreter probe below nor the disk floor after it runs under
# --preflight-only: the preflight is stdlib-only (it needs neither yaml nor a
# bundle) and a full disk is not a missing requirement.
if [ "$PRE_ONLY" != 1 ]; then
_ssl_probe='
import sys
missing = []
for mod in ("yaml",):
    try:
        __import__(mod)
    except Exception:
        missing.append(mod)
bundle = ""
try:
    import certifi
    bundle = certifi.where()
except Exception:
    missing.append("certifi")
print(bundle)
print(",".join(missing))
'
_pre="$(python3 -c "$_ssl_probe" 2>/dev/null)"
_bundle="$(printf '%s\n' "$_pre" | sed -n 1p)"
_ontbreekt="$(printf '%s\n' "$_pre" | sed -n 2p)"
if [ -n "${_ontbreekt:-}" ]; then
  # EXIT 6, beside the disk floor at 3, the moved tree at 4 and the gate lock at
  # 5. The same reason as all three: no gate has judged anything, and calling
  # that "the gate failed" sends the reader after a defect that was never
  # measured.
  echo "LOCAL_CI_GATE: this machine is missing what the gates run on — stopping here." >&2
  echo "  missing: ${_ontbreekt}" >&2
  echo "    python3 -m pip install --user ${_ontbreekt//,/ }" >&2
  echo "  NOTHING WAS CHECKED. This is the machine, not the code." >&2
  exit 6
fi
# Only when the caller has not chosen one: a runner with its own bundle knows
# better than we do, and overriding it would be this block causing the class of
# failure it exists to prevent.
if [ -z "${SSL_CERT_FILE:-}" ] && [ -n "${_bundle:-}" ]; then
  SSL_CERT_FILE="$_bundle"; export SSL_CERT_FILE
  echo "  SSL_CERT_FILE: ${SSL_CERT_FILE} (certifi; this python has no bundle of its own)"
fi
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
  failed_before_the_gates disk-headroom "below the free-disk floor; nothing was judged"
  exit 3
fi
fi # PRE_ONLY skips the interpreter probe and the disk floor above

# THE CRATES THIS LANDING TOUCHES, and nothing else. (#343)
#
# Measured 05-09-2026 over that day's landings: 45 to 77 minutes each, one at a
# time, because the full lane compiles and tests all forty-nine crates in the
# landing worktree whatever the change was. A change to scripts/ci compiled the
# engine to prove nothing about itself.
#
# The owner decision was speed over a little certainty, and the certainty given
# up is named rather than implied: a crate outside the selection is not built or
# tested HERE. It is built and tested by the `workspace` job of
# .github/workflows/ci-ephemeral.yml, on a change-triggered wave against master, on a
# Hetzner instance bought for it and thrown away -- so a landing that broke
# something outside its own crates turns that wave red instead of being refused
# here.
#
# Two things changed on 06-09-2026 and both belong in this sentence (#350). The
# job used to be in ci.yml, on the desktop, on the push itself; it is on an
# instance now, because that desktop had twenty-nine runs queued on four cores.
# The running wave finishes; only the latest pending change is kept. That is the whole of the trade, and it holds only while that
# wave runs. `LANE=full` restores the old behaviour for one run.
#
# SELECTED ONCE, before anything compiles, so a failure to decide is a refusal
# rather than a quiet fallback: touched_crates.py exits 3 when it cannot read the
# workspace, and the answer to "I do not know" is the whole workspace.
CRATES=""; SELECTION="the whole workspace"; EXCLUDED_REACHED="xfa-wasm"
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
    # And the excluded crates this change still reaches, checked and tested
    # natively by `excldeps` below (#623).
    # FAIL CLOSED (codex, #2003): a selection that cannot answer checks every
    # excluded crate it could have reached, never none.
    if _er="$(python3 scripts/ci/touched_crates.py --excluded-reached 2>"${LOGDIR}/excluded.err")"; then
      EXCLUDED_REACHED="$(tr '\n' ' ' <<<"$_er")"
    else
      echo "  touched_crates.py --excluded-reached could not decide; checking every excluded crate it checks."
      sed 's/^/    /' "${LOGDIR}/excluded.err"
      EXCLUDED_REACHED="xfa-wasm"
    fi
    SELECTION="$(wc -w <<<"$CRATES" | tr -d ' ') crate(s): ${CRATES:-none}"
    echo "  landing lane: ${SELECTION}"
  fi
fi

# The selection is calculated once. Unknown changes keep every regression.
DEVELOPMENT_DEFERRED="$(python3 scripts/ci/development_gate_scope.py)" || exit 2

# THE TESTS OF SCRIPTS THIS LANDING CANNOT REACH (COS, 28-09-2026). The crates
# are scoped above; the tests of scripts were not, and the landing of #2019 --
# two benchmark scripts -- held the landing lock 68 minutes running tests of the
# gate pool, the public seed and the remote gate. A test whose inputs the change
# does not touch is SKIPPED here, counted with the untouched crates, and runs in
# the nightly full lane on master (scripts/cos/nightly_full_gate.sh). A change to
# this file, the scoper or the hook runs them all; so does any doubt, since the
# scoper answers "none" when it cannot read the diff.
LANDING_OUT_OF_REACH=""
if [ "$LANDING" = 1 ] && [ "$FULL" != 1 ]; then
  LANDING_OUT_OF_REACH="$(python3 scripts/ci/landing_test_scope.py)" || exit 2
  echo "landing scope: $(wc -w <<<"$LANDING_OUT_OF_REACH" | tr -d ' ') script test(s) out of this landing's reach"
fi
echo "development scope: infrastructure regressions deferred: ${DEVELOPMENT_DEFERRED:-none}"

# THE REQUIREMENTS FIRST (m482), before the first gate, in every lane.
#
# Gate runs have died an hour in on a missing machine dependency -- a venv stack
# that was never installed, a tesseract the suites silently skipped without. The
# preflight checks every requirement of every gate this lane will run, at once,
# in seconds, from scripts/ci/gate_requirements.toml, and refuses with the
# complete list. The selection above is passed in rather than recomputed, so the
# answer is about this run's gates and not about a second reading of the tree.
_preflight_lane=fast; [ "$LANDING" = 1 ] && _preflight_lane=landing; [ "$FULL" = 1 ] && _preflight_lane=full
if python3 scripts/ci/gate_preflight.py --lane "$_preflight_lane" \
    --crates "$CRATES" --deferred "$DEVELOPMENT_DEFERRED" \
    --out-of-reach "$LANDING_OUT_OF_REACH" --place "$PLAATS" \
    >"${LOGDIR}/preflight.log" 2>&1; then
  sed 's/^/  /' "${LOGDIR}/preflight.log"
else
  sed 's/^/  /' "${LOGDIR}/preflight.log"
  # EXIT 7, beside the disk floor at 3, the moved tree at 4, the gate lock at 5
  # and the interpreter at 6. The same reason as all four: no gate has judged
  # anything, and calling that "the gate failed" sends the reader after a
  # defect that was never measured.
  echo "LOCAL_CI_GATE: preflight FAILED — this machine is missing what the ${_preflight_lane} lane runs on." >&2
  echo "  The complete list is above. NOTHING WAS CHECKED: this is the machine, not the code." >&2
  failed_before_the_gates preflight "missing machine requirements; nothing was judged"
  exit 7
fi
if [ "$PRE_ONLY" = 1 ]; then
  echo "LOCAL_CI_GATE: PREFLIGHT ONLY (${_preflight_lane} lane) — every requirement present, no gate ran."
  exit 0
fi

# EVERY GENERATED REGISTER, NAMED IN ONE MESSAGE (#526).
#
# Four files are generated from the code and committed beside it, and each had
# its own gate further down with its own `--check`. On 10-09-2026 three of ten
# gate runs in a row were separate rounds of that: the NOTICE copy, then
# docs/PUBLIC_API.json, then docs/CAPABILITY_REGISTER.md twice -- each found by
# the run after the one that fixed the last, because a run stops at the first
# failure and the next stale register is invisible until then.
#
# So the question is asked once, here, before anything expensive, and the answer
# names all of them with the command that writes them. The individual gates stay
# where they are: they are milliseconds, they are what every_guard_has_a_job.py
# books, and a second opinion on a file that is already correct costs nothing.
run registers bash scripts/ci/regenerate_registers.sh --check
# Its test, next to its subject: stub generators through both modes, failing
# and succeeding, in milliseconds and without cargo.
run registerst python3 scripts/ci/test_regenerate_registers.py

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
# ADVISORY, and it reports rather than judges -- so the machine half of a split
# lane leaves it to the half that runs everything else. Two copies of an
# advisory report is two chances to read the older one. (#474)
if [ "$PLAATS" != machine ] && [ "$FULL" = 1 ]; then
echo "--- groen (advisory in both local lanes; refuses in ci.yml on master) ---"
# Kept, so the last lines of the lane can say it again. Advisory output that
# scrolls past is advisory output nobody reads: this guard has been printing
# "bindings.yml: SILENT -- no run at all since the file changed 126 days ago"
# on every push, and it took reading the file to notice. Its refusal lives in
# ci.yml, which is hosted and has not run since the budget went to EUR 1
# (#333), so this print is the only place it is said at all. (#510)
python3 scripts/ci/a_gate_that_never_went_green.py > "${LOGDIR}/groen.log" 2>&1 || true
sed 's/^/  /' "${LOGDIR}/groen.log"

fi
# NOT `zwaar`, AND THE LANE'S OWN TEST SAYS SO. #575 moved this to the landing
# lane to get the fast lane back under its budget, and
# `test_the_pre_push_gate_picks_its_lane.py` refused: the guard above is
# advisory here, and its test is what proves the guard can go red at all -- so
# advisory must not quietly become broken-and-advisory. Kept where it was; the
# budget was found elsewhere.
run groentest python3 scripts/ci/test_a_gate_that_never_went_green.py
# The visual suite renders and compares against a baseline, so it needs a
# release build. t1 added this line and took it out again in the same branch:
# local_ci_gate.sh was t2/t3's, and a t1 branch may not edit it. It comes back
# here because the crate is on loan to t3 for #326.
crate_gate visreg visual-regression cargo test -p visual-regression --release
# The other half of the same question (#293). The suite above compares the
# renderer against images of its own output, so it catches drift in what we
# draw and cannot catch drift in what it compares against: change pdf-render,
# run UPDATE_VISUAL_BASELINE=1, commit both, and every gate is green because the
# reference moved to meet the regression. Measured, not assumed -- a one-pixel
# inset of the background fill turns the suite red on its own, and green again
# once the 39 pages it moves are regenerated in the same commit. The crate's
# README already asks a reviewer to explain each changed image; since 04-09
# there is no reviewer, so the declaration has to be machine-checkable. Git
# only, no build, both lanes.
run baseline  python3 scripts/ci/a_rerendered_baseline_declares_itself.py
run baselinet python3 scripts/ci/test_a_rerendered_baseline_declares_itself.py

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
# The other side of the same coin as `kosten` above: that one watches what a
# push costs in hosted minutes, this one what it costs the one self-hosted
# runner. It is a YAML scan of 35 files, no compiler, so it belongs in the fast
# lane -- and it has to be there rather than only in the workflow, because what
# it refuses is a change to a workflow, which is exactly the kind of change that
# is easiest to make and hardest to see the effect of. On 06-09-2026 the desktop
# had twenty-nine runs queued with the oldest master commit under test two hours
# old (#350).
run paidlock python3 scripts/ci/paid_compute_policy.py
# ALSO ON LOAN, and the second half of the 32 s the lane needed (#575). The
# policy guard above runs on every push, which is where it matters: a push is
# what buys a machine. This is its test, and what a test of a guard defends is
# the guard's verdict -- so it runs where that verdict is final.
zwaar paidlocktest python3 scripts/ci/test_paid_compute_policy.py
run devscopetest python3 scripts/ci/test_development_gate_scope.py
run failnames bash scripts/ci/test_the_failed_gates_are_named.sh
run rigdatatst python3 scripts/ci/test_rig_data_stays_on_the_rig.py
run failblock bash scripts/ci/test_a_failed_run_names_its_gates.sh
run landsnaptst bash scripts/ci/test_the_landing_runs_the_snapshot_audit.sh
# A red audit names its finding on its last line, where the gate's tail and
# the failed-gates block both read. Fixture trees, seconds.
run snapcheck python3 scripts/release/test_public_snapshot_check.py
run landscopetst python3 scripts/ci/test_landing_test_scope.py
run sorttotal python3 scripts/ci/a_sort_comparator_is_a_total_order.py
run sorttottst python3 scripts/ci/test_a_sort_comparator_is_a_total_order.py
run waveneedtest python3 scripts/ci/test_wave_needs_instance.py
run desktoplast python3 scripts/ci/heavy_work_leaves_the_desktop.py
run desktoplastt python3 scripts/ci/test_heavy_work_leaves_the_desktop.py
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
# Every file under 1 MB, against master (#599). The policy stood in a script
# for months and nothing ran it; a learned model is several MB of generated
# code, and the day one was committed whole is the day the policy stops.
run largeblob env BASE_BRANCH=github/master bash scripts/ci/check_large_blobs.sh
# And the guard's own five cases, which it had none of until the rule it
# states refused a round's evidence (#574).
run largeblobt bash scripts/ci/test_check_large_blobs.sh
run crons     python3 scripts/ci/schedule_guards_match_their_cron.py
run mirror    python3 scripts/ci/test_mirror_has_not_drifted.py
# Drift is the consequence; this is the cause. mirror_has_not_drifted.py fires
# after fifty commits or two days, which is two days of a backup nobody is
# pushing. This asks whether the hourly agent is still loaded and still
# finishing runs, and it judges only the machine that has the agent (#231).
run mirroragent python3 scripts/ci/the_mirror_agent_is_running.py
run mirroragentt python3 scripts/ci/test_the_mirror_agent_is_running.py
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
# THE ONE PUBLISHED CRATE `--workspace` CANNOT REACH. crates/pdf-python is
# excluded from the workspace on purpose (PyO3 pins a Python), so cargo check,
# clippy and test all walk past it, and bindings.yml -- the only workflow that
# compiles it -- runs on tags and dispatch. On 06-09-2026 f9ae3a1d7 landed with
# four `#[pyfunction]`s defined four times each, 77 errors all E0428, and for
# three days the only symptom was that no wheel could be built (#358).
#
# In the landing lane rather than the fast one: it is a compile with its own
# lockfile and its own target directory. The two-hourly wave runs it as well,
# so a branch push is not the last chance -- but a landing is the moment master
# gains the change, and this is the cheapest place that a broken binding cannot
# survive a day.
# ONLY WHEN THE BINDING'S OWN GRAPH MOVED (#523). `pdf-python` is not a
# workspace member, so it never appears in the selection; every workspace crate
# it depends on has `pdfluent` in its dependent closure, and a change inside
# `crates/pdf-python` itself makes the selection the whole workspace. So
# `pdfluent` is the question, and a landing that does not move it is not
# answering it here.
crate_gate pybuild pdfluent cargo check --manifest-path crates/pdf-python/Cargo.toml --locked
# The cloud OCR features as a library build: tests add `image` as a
# dev-dependency and hide a feature that forgets `dep:image` (14-10-2026).
crate_gate ocrcloud pdf-engine cargo check -p pdf-engine --lib --features ocr-cloud --locked
run infra     python3 scripts/ci/test_infra_health.py
run waitfit   python3 scripts/ci/wait_fits_in_the_job_timeout.py
run onelock   python3 scripts/ci/one_lock_guards_the_instances.py
run timers    python3 scripts/ci/persistent_timers_use_a_calendar.py
run installer python3 scripts/ci/test_installer_escapes_the_path.py
run upstream  python3 scripts/ci/upstream_has_not_moved_on.py
run forklist  python3 scripts/ci/fork_lists_agree.py
# The other half of the README promise (#255). every_fork_names_its_upstream.py
# (#219) holds the fork line above the fold; this one holds the recommendation
# -- a paragraph sending a reader who does not need our changes to upstream --
# and refuses a README claiming commercial terms over a permissive manifest, as
# pdf-render's and pdf-font's did over their Apache-2.0 OR MIT.
run forkrdme  python3 scripts/ci/a_forked_readme_sends_you_upstream.py
run forkrdmet python3 scripts/ci/test_a_forked_readme_sends_you_upstream.py
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
# THE OTHER HALF OF THAT QUESTION, and it is per worktree rather than shared.
# `extensions.worktreeConfig` is on in this clone, so a worktree can carry its
# own core.hooksPath -- and two of the 72 measured on 10-09-2026 pointed at
# .git/hooks, where nothing but git's samples live. Every commit written there
# ran no identity hook, no sign-off hook and no attribution guard, and said
# nothing about it. Machine-local: the worktrees to judge are the operator's.
run wthooks   python3 scripts/ci/every_worktree_runs_the_tracked_hooks.py
run wthookstst python3 scripts/ci/test_every_worktree_runs_the_tracked_hooks.py
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
# ADVISORY, and it reports rather than judges -- so the machine half of a split
# lane leaves it to the half that runs everything else. Two copies of an
# advisory report is two chances to read the older one. (#474)
# STILL ADVISORY, and the reason it should not be has to be cleared first (#510).
#
# The trade written above was "this gives up the moment of the refusal, not the
# refusal": advisory here, and ci.yml fails the pull request before anything
# merges. That second half does not exist at the moment. ci.yml's guards job is
# hosted and hosted Actions have been stopped since the Actions budget went to
# EUR 1 on 05-09-2026 (#333), so the territory rule is a warning and nothing
# else -- the state the map's own preamble calls impossible by construction.
#
# Flipping this line to `run` was tried in this branch and reverted, because it
# refuses the branch that would flip it, and for a reason that is not this
# branch's fault: `every_guard_has_a_job.py` requires a new guard to be
# registered in .github/workflows/ci.yml, ci.yml belongs to t1 and t2, and it is
# not in `[registers].gedeeld` -- so no branch outside t1/t2 can add a guard
# without editing a file it may not touch. The register exemption exists for
# exactly that shape and does not cover this file yet. This branch adds it to
# `gedeeld`; that list is read from the BASE, on purpose, so it takes effect for
# the next branch and not for this one. Flip this line then.
if [ "$PLAATS" != machine ]; then
echo "--- territory (advisory; see #510 before making this refuse) ---"
python3 scripts/ci/territories_do_not_overlap.py 2>&1 | sed 's/^/  /' || true

fi
run regexem   python3 scripts/ci/test_register_exemption.py
# On loan (#367). This ran only in ci.yml, so a guard nobody calls passed the
# push gate green and surfaced one CI run later, on somebody else's branch --
# a push of 147 green gates carried an orphan through in exactly that way. It is
# offline and takes under two seconds; there is no reason it was not here.
run guardjobs python3 scripts/ci/every_guard_has_a_job.py
# The preflight's own proof (m482): every gate declared, every kind red when
# missing, the prepare install list equal to the apt set. Fast and offline,
# beside the guard on the guards.
run gatepreflight python3 scripts/ci/test_gate_preflight.py
# The byte-compare needs LF goldens on every checkout (build 6: CRLF on
# Windows failed serializers_over_five_documents). Fast and offline.
run goldeneol python3 scripts/ci/golden_dirs_check_out_lf.py
run prreach   python3 scripts/ci/every_gate_is_reachable_from_a_pull_request.py
run prreacht  python3 scripts/ci/test_every_gate_is_reachable_from_a_pull_request.py
# ---------------------------------------------------------------------------
# THE GUARDS THAT HAD ONE JOB, AND IT COULD NOT RUN (#333, #510)
#
# Forty-three guards ran in exactly one place: the `guards` job of ci.yml. That
# job is hosted, and hosted Actions have been stopped since the Actions budget
# went to EUR 1 on 05-09-2026. A guard whose only job cannot run has stopped
# measuring -- and six of the forty-three were red on master when they were
# measured on 10-09-2026, over days, with nothing anywhere to say so.
#
# Thirty-two of them are here. They are Python scans of files already on disk:
# forty-five seconds together, no compiler, no network. Four cost longer than
# four seconds and went to the landing lane below; five are red and run through
# `guards_red_on_master.toml`, loudly and to a date.
#
# ONE DOES NOT MOVE. `internal_matter_range.sh` reads the commit range of a
# GitHub event -- the branch on a pull request, what the push added on a push --
# and there is no such range on a machine. Run here it says
# "SKIPPED (not a pass): no range is defined for event ''", which is the honest
# answer and not a useful one. It stays in ci.yml, where the range exists; its
# test, `test_internal_matter_range.py`, is in the list below and does not need
# one.
#
# Measured, not assumed: each was run and timed on this machine before it was
# given a lane, because a first pass that grepped for the word `cargo` claimed
# ten of them needed a compiler and every one of those was a fixture string.
# ---------------------------------------------------------------------------
run baselinehw  python3 scripts/ci/a_baseline_names_the_machine.py
run manifpin    python3 scripts/ci/a_manifest_pins_every_file.py
run manifpint   python3 scripts/ci/test_a_manifest_pins_every_file.py
run forkname    python3 scripts/ci/every_fork_names_its_upstream.py
run forknamet   python3 scripts/ci/test_every_fork_names_its_upstream.py
run forkdivt    python3 scripts/ci/test_fork_divergence.py
run featprom    python3 scripts/ci/feature_promises.py
run corpuspin   python3 scripts/ci/gate_corpus_is_pinned.py
run corpuspint  python3 scripts/ci/test_gate_corpus_is_pinned.py
run ssimcorp    python3 scripts/ci/test_ssim_gate_corpus_is_present.py
run veracache   python3 scripts/ci/test_verapdf_cached.py
run ifvalid     python3 scripts/ci/if_conditions_are_valid.py
run lockagree   python3 scripts/ci/lockfile_agreement.py
run baselinehwt python3 scripts/ci/test_a_baseline_names_the_machine.py
zwaar gpurun    python3 scripts/gcp/test_gpu_run.py
run negstart    python3 scripts/gcp/test_hard_negatives_startup.py
zwaar doclstart python3 scripts/gcp/test_docling_office_startup.py
run v2cstart    python3 scripts/gcp/test_is_table_v2_clean_startup.py
run smokestart  python3 scripts/gcp/test_smoke_zeroshot_local.py
run bpbround    python3 scripts/extraction/tables_b/test_pb_round_addon.py
run dupetest    python3 scripts/ci/test_does_this_already_exist.py
# The guard (guardjobs) stays before the push; its self-test rides the
# landing lane (lanebudget, #636).
zwaar guardjobst  python3 scripts/ci/test_every_guard_has_a_job.py
run fixwell     python3 scripts/ci/test_fixtures_are_wellformed.py
# The known-images fixture is what its generator writes, twice in a row.
# Stdlib only, no engine: the four binding suites that read the fixture
# ride the landing lane below.
run genimgdet  python3 fixtures/gen/test_known_images.py
run denyinst    python3 scripts/ci/test_install_cargo_deny.py
run imranget    python3 scripts/ci/test_internal_matter_range.py
run nohosted    python3 scripts/ci/test_no_hosted_minutes_on_a_push.py
run pdfalabel   python3 scripts/ci/test_pdfa_gate_label.py
run pdfaconft   python3 scripts/ci/test_pdfa_output_conformance_gate.py
run reachchk    python3 scripts/ci/test_reachability.py --check
run reachunit2  python3 scripts/ci/test_reachability_unit.py
run retreport   python3 scripts/ci/test_retention_report.py
run testsran    python3 scripts/ci/test_tests_actually_ran.py
zwaar cliwire  python3 scripts/ci/test_cli_takeover_wiring.py
run textfid     python3 scripts/ci/test_text_fidelity.py
run libname     python3 scripts/ci/test_the_library_name_check_reads_every_caller.py
run skiplintm   python3 scripts/ci/test_the_skip_lint_reads_a_returning_macro.py
run skiplintjs  python3 scripts/ci/test_the_skip_lint_reads_javascript.py
run inputssh    python3 scripts/ci/workflow_inputs_are_not_shell.py
run inputssht   python3 scripts/ci/test_workflow_inputs_are_not_shell.py
run wasmpin     python3 scripts/ci/wasm_target_matches_the_pin.py
# The five that were red on 10-09-2026 and are not any more (#510). They spent
# a day in guards_red_on_master.toml, advisory and to a date; they are here
# because they went green, and the register refuses a row whose guard passes.
# Every one is under a second, which is why none of them went to the lane below.
run apicov      python3 scripts/ci/api_coverage.py
run apicovt     python3 scripts/ci/test_api_coverage.py
run capreg      python3 scripts/ci/capability_register.py --check
# And the half of that register nothing could check until #443: which PDF/A
# conformance levels we say we PRODUCE. The C header promised 1b and 3b, a
# published website snippet calls convert_to_pdfa(A1b), and the only flavour
# ever measured was 2b -- which turned out to score 20 of 30 at 1b. The test
# first, as everywhere on this list. Both read four small files; milliseconds.
run flavourt    python3 scripts/ci/test_a_pdfa_flavour_has_a_baseline.py
run flavour     python3 scripts/ci/a_pdfa_flavour_has_a_baseline.py
run flavgatet   python3 scripts/ci/test_pdfa_conformance_gate.py
run pubapi      python3 scripts/ci/export_public_api.py --check
run pubapit     python3 scripts/ci/test_public_api_export.py
run skiplint    python3 scripts/ci/test_skip_lint.py
# The register they came out of. Empty today; it stays because the next guard
# that goes red on master through nobody's branch needs somewhere to be loud
# and dated rather than either ignored or blocking. The test first, as
# everywhere on this list.
run knownredt python3 scripts/ci/test_a_known_red_guard_expires.py
run knownred  python3 scripts/ci/a_known_red_guard_expires.py
# One of the forty-three does compile, and went to the fast lane by mistake.
# `test_the_clippy_lane_reads_test_targets.py` builds a fixture crate with
# `cargo clippy --offline` to see which lints the shipped flags actually raise
# -- so `a_push_gate_does_not_compile_the_workspace.py` refused the whole fast
# lane the moment it was moved there, on every branch, for a reason in nobody's
# change. Corrected here rather than handed back, because until it is corrected
# no branch can push at all. (#510)
zwaar clippylane python3 scripts/ci/test_the_clippy_lane_reads_test_targets.py
# The four that cost more than four seconds. Nothing here compiles; they read
# the git history, the corpus manifest and every checkout on the machine, and
# that is time a branch push should not pay on every commit.
zwaar origincheck python3 scripts/ci/test_origin_is_the_source_in_every_checkout.py
zwaar branchmr   python3 scripts/ci/test_branches_have_a_merge_request.py
zwaar corpuspre  python3 scripts/ci/test_corpus_preflight.py
zwaar textrepl   python3 scripts/ci/test_text_replace_gate.py
# In the fast lane because the failure it catches is invisible until a wave is
# dispatched: GitHub refuses the whole workflow before a job starts, so there is
# no red job and no log, only startup_failure. Offline, milliseconds.
run callergrant python3 scripts/ci/a_caller_grants_what_it_calls.py
run callergrantt python3 scripts/ci/test_a_caller_grants_what_it_calls.py
run header    python3 scripts/ci/header_sweep.py
run sweepexem python3 scripts/ci/test_sweep_exemption.py
run notice    python3 scripts/ci/notice_names_what_exists.py
# The other half of the same question (#493): NOTICE and THIRD_PARTY_LICENSES
# name paths that exist, and every file under a crate's assets/ or resources/ is
# named in one of them. An Adobe CMap shipped to crates.io in neither for a year.
run vendored  python3 scripts/ci/every_vendored_asset_is_named.py
run vendoredt python3 scripts/ci/test_every_vendored_asset_is_named.py
run noticetst python3 scripts/ci/test_notice_names_what_exists.py
run withdrawn python3 scripts/ci/no_withdrawn_object.py
run withdrawnt python3 scripts/ci/test_no_withdrawn_object.py
# THE LANDING LANE, since 07-09-2026 (#425). 182 seconds measured, 16% of the
# fast lane on its own -- it assembles the whole published tree and greps it.
# What it watches is the tree that would be PUBLISHED, and publishing happens
# from master, so the landing is the last moment that matters and the landing
# lane is before it. Its own header already says the build half runs on a push
# to master and nowhere else, for the same reason at a larger scale.
#
# AND IT RUNS THERE (#574, 30-09-2026). From 11-09 a case in `run` deferred it on
# every lane but --full, which overrode the line below: `zwaar` handed it on and
# `run` put it down again. Two test fixtures with a machine path in them reached
# master that way, and only the nightly full lane saw it.
# scripts/ci/test_the_landing_runs_the_snapshot_audit.sh fails if it comes back.
zwaar snapaudit bash scripts/ci/public_snapshot_audit.sh
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
# The front page of the public repository (#205). Milliseconds: it reads one
# file and looks for three rows. In the fast lane because the thing it catches
# is a README rewrite, and a README rewrite is exactly the change that lands
# without anybody thinking about the seeding.
run readme    python3 scripts/ci/the_public_readme_is_a_landing_page.py
run readmetst python3 scripts/ci/test_the_public_readme_is_a_landing_page.py
# The same page is the front page of pdfluent/pdfluent-sdk, where changes land
# too (#637). One fetch; fails when the two copies differ.
run sdkreadme python3 scripts/ci/the_sdk_readme_is_the_public_one.py
run sdkrdmtst python3 scripts/ci/test_the_sdk_readme_is_the_public_one.py
# The four public repositories, read through the API (#233). This is the half
# no runner can do: `administration:read` is not a scope GITHUB_TOKEN can hold,
# so on a hosted runner the guard can only announce SKIPPED -- here `gh` has a
# keyring and it genuinely reads what GitHub reports. Same reason
# every_label_has_a_runner.py lives in this file and not in a workflow.
#
# It costs about ten seconds of API calls and it is allowed to find nothing for
# months; what it exists for is the fifth repository, created unprotected, which
# nothing else in this tree would ever look at.
run protectst python3 scripts/ci/test_public_repos_are_protected.py
run protected python3 scripts/ci/public_repos_are_protected.py
# Every repository public or private as intended (#637). Needs gh signed in,
# which only this gate has; the hosted runner cannot read private repositories.
run visibtst  python3 scripts/ci/test_the_repositories_stand_as_intended.py
run visible   python3 scripts/ci/the_repositories_stand_as_intended.py
# The other half of the same question (#233): protection decides what may be
# merged, these files decide whether an outsider ever learns what is expected of
# a change at all. Offline and instant -- it reads the tree and the list of paths
# the seeding strips, so it belongs in the fast lane and runs in the workflow too.
run contribst python3 scripts/ci/test_public_contribution_files_are_complete.py
run contribfl python3 scripts/ci/public_contribution_files_are_complete.py
run territst  python3 scripts/ci/test_territories_do_not_overlap.py
run deadhost  python3 scripts/ci/no_dead_host_in_a_connecting_script.py
run deadhosttst python3 scripts/ci/test_no_dead_host_in_a_connecting_script.py
# A Windows checkout renders every tracked symlink as a text file holding the
# link target; four fixture links did exactly that to the node smokes (#636).
# Milliseconds: one git ls-files over the index.
run symlinks  python3 scripts/ci/no_tracked_symlinks.py
run symlinkst python3 scripts/ci/test_no_tracked_symlinks.py
# Narrower and stricter than the line above: one particular address, anywhere in
# the tree, including comments. A launchd agent was still writing a Cloudflare
# token to it every hour on 07-09-2026 (#424), and a comment is where the next
# person copies an address from.
zwaar retired   python3 scripts/ci/no_retired_host.py
run retiredts python3 scripts/ci/test_no_retired_host.py
# Rig page text never goes into the repo: scores + hashed ids only.
# Milliseconds: one pass over the committed run files.
run rigtext   python3 scripts/ci/no_rig_text_in_runs.py
run rigtextts python3 scripts/ci/test_no_rig_text_in_runs.py
run snippets  python3 scripts/ci/extract_site_snippets.py --check docs/site/snippets.json
run siteblkreg python3 scripts/ci/test_site_blocks_register.py
run gatelogs   python3 scripts/ci/test_gate_logs_do_not_pile_up.py
# The lane's own cost, watched by the lane (#425). Milliseconds: it reads the
# table the run above wrote. It is deliberately the LAST cheap guard rather than
# the first -- the table it reads is the PREVIOUS run's, so putting it early
# would only make that more confusing, not less. It skips loudly on a machine
# that was busy, which on this Mac is most of the day; the answer it gives at
# night is the one that matters.
run lanebudget python3 scripts/ci/the_fast_lane_stays_under_its_budget.py \
  --lane "$(if [ "$LANDING" = 1 ]; then echo landing; elif [ "$FULL" = 1 ]; then echo full; else echo fast; fi)"
run lanebudgtst python3 scripts/ci/test_the_fast_lane_stays_under_its_budget.py
# The other half of that budget, and the half a number cannot hold (#473). What
# makes this lane expensive is not the count of gates, it is whether one of them
# reaches a compiler: two of them did, and those two were 74 s of a 395 s lane
# on a quiet cpx42 while a hundred and thirty file readers shared the rest. A
# budget in seconds says a lane got slower; this says which mistake made it
# slower, on the push that makes it, and it is a syntax scan of the gate list --
# under a second, no compiler of its own.
run pushcompile python3 scripts/ci/a_push_gate_does_not_compile_the_workspace.py
run pushcompiletst python3 scripts/ci/test_a_push_gate_does_not_compile_the_workspace.py
# What the browser is allowed to carry (#383). Reads the manifests through
# `cargo tree --target wasm32-unknown-unknown`, so it builds nothing and costs a
# couple of seconds. It belongs in the fast lane because the thing it catches --
# a dependency edge that puts 4.2 MB of fonts into a bundle claim B07 publishes
# at 9.6 MB -- is created by editing a Cargo.toml, and that is a change whose
# author should hear about it before the push, not after the wasm job.
run wasmfonts python3 scripts/ci/the_browser_bundle_carries_no_fonts.py
run wasmfontstst python3 scripts/ci/test_the_browser_bundle_carries_no_fonts.py
# The default build stays pure Rust (#641). `cargo tree` resolves without
# compiling, so this is fast-lane cheap; the manifest half answers even where
# cargo is absent.
run orttree python3 scripts/ci/the_default_tree_carries_no_ort.py
run orttreetst python3 scripts/ci/test_the_default_tree_carries_no_ort.py
# The other half of the same question: what the browser CARRIES is checked above
# without a compiler, and this compiles it (#483). It is the only wasm32 build in
# either local lane -- and until it existed, a change that broke the browser
# build passed every gate, landed, and turned master red on the next wave, up to
# two hours later. #383 did exactly that: a dependency cfg'd out on wasm32 left
# an enum variant unconstructible there, and under the `-D warnings` the wave
# sets, a dead variant is a compile error for the whole crate.
#
# IN THE LANDING LANE SINCE #473, and it was in the fast one until then, on the
# argument that a warm target directory makes it seconds and its author is at
# the keyboard now. Both halves of that were true on the Mac and neither is
# true any more. Since #474 a push does not run here: it runs on an instance
# bought for it, where "warm" means whatever last night's snapshot refresh left
# behind, and the author is not at that keyboard at all -- the finding comes
# back over ssh either way, from the push lane or from the landing lane twenty
# minutes later. Measured on a quiet cpx42: 27 s of the fast lane's 395, for a
# question the landing lane asks again before anything lands.
#
# What it catches has not changed and neither has the coverage: master takes
# only fast-forwards, so nothing reaches master without the landing lane, and
# the wave still compiles the workspace. The wasm32 std lives on the gate image
# (gate-snapshot-refresh.yml) and gate_instance_prepare.sh adds it when it does
# not.
# The command it runs IS `cargo check -p pdfluent --target wasm32-unknown-unknown`,
# so the crate this gate is about is `pdfluent` and nothing else (#523). A wasm32
# compile on a landing that touches no crate of that graph is minutes for a
# question nothing asked.
crate_gate wasmcheck pdfluent bash scripts/ci/run_wasm_check.sh
run wasmchecktst python3 scripts/ci/test_the_lane_compiles_for_the_browser.py
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
# The benchmark programme's published figures, and the guard over them. Both are
# file scanning and both belong in the fast lane: what they watch is a markdown
# table and a directory of run files, so any push can break them. The programme
# itself is not run here -- measuring is minutes of converting and validating,
# and it happens on the runner, on a calendar (#169).
run pubfig    python3 scripts/ci/a_published_number_names_its_run.py
# Its self-test is 29 s and says something only when the guard itself changes,
# which a landing re-checks: landing lane since 23-09-2026 (lanebudget, #596).
zwaar pubfigtst python3 scripts/ci/test_a_published_number_names_its_run.py
# The improvement loop (#366). The baseline guard belongs in a push above all:
# rewriting benchmarks/pdfa/baseline.json is the cheapest way to turn a red
# ratchet green, and this is the last moment before that edit is somebody
# else's history. It reads git and nothing else, so it costs a second. The
# ratchet it protects needs the corpus and runs on the wave.
run baseticket python3 scripts/ci/loop/a_baseline_only_moves_with_a_ticket.py
run basetkttst python3 scripts/ci/loop/test_a_baseline_only_moves_with_a_ticket.py
# The other way a baseline stops guarding: it does not move at all. The guard
# above reads a diff and is empty on the days that matter, so nothing noticed
# for five days that benchmarks/extraction/baseline.json still held the run of
# 09-09 while #430 and #437 had taken twenty-five documents past it -- two of
# them from nothing to a hundred, free to fall all the way back (#553). This
# reads the baseline's own commit against the commits that touched what it
# measures. Git and one JSON file; it costs a second, like its neighbour.
# IN THE LANDING LANE ONLY (#626, T2 on 25-09-2026). The harm it names lands
# when the code lands, and since #626 the landing measures a stale baseline on
# the tree it pushes; asked at a branch push as well, it refused every branch
# that had not won the race the landing lane now runs for it.
zwaar baseage python3 scripts/ci/loop/a_baseline_is_not_older_than_what_it_measures.py
zwaar baseagetst python3 scripts/ci/loop/test_a_baseline_is_not_older_than_what_it_measures.py
# And the builder that writes that baseline, which decides whether a lowered
# value can be landed at all and had no test of its own until #553.
run basebldtst python3 scripts/ci/loop/test_build_extraction_baseline.py
run ratchettst python3 scripts/ci/loop/test_pdfa_ratchet.py
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
# The other half of the same rule, and the half nobody was reading. That guard
# refuses a pull_request-reachable job on the desktop; this one refuses a job
# there in any workflow whose ref the CALLER chooses -- `gh workflow run --ref
# <branch>` runs that branch's steps on the machine holding the corpus, the warm
# cache and the runner token. It ran only in the `guards` job of ci.yml, which
# is hosted, and the Actions budget has been spent since 05-09-2026: so it was
# red on master for days with nothing to say so. A landing sees it now.
run orchhost  python3 scripts/ci/orchestration_stays_hosted.py
run prruntst  python3 scripts/ci/test_pr_code_stays_off_the_desktop.py
run licenses2 python3 scripts/ci/license_gate.py
run fetched   python3 scripts/ci/every_fetched_asset_is_registered.py
run fetchedt  python3 scripts/ci/test_every_fetched_asset_is_registered.py
run licbound  python3 scripts/ci/license_boundary.py
# The boundary guard above stays before the push; its self-test rides the
# landing lane (lanebudget, #636).
zwaar licbtest  python3 scripts/ci/test_license_boundary.py
# The document the licence hands the last word to. LICENSE-COMMERCIAL §10 says
# the agreement is that text plus a signed order form, and that the form
# prevails -- so the form and §2 drifting apart sells or withholds a right that
# neither document admits to (#220).
run orderform python3 scripts/ci/the_order_form_agrees_with_the_licence.py
run orderftst python3 scripts/ci/test_the_order_form_agrees_with_the_licence.py
# The same two documents, plus docs/licensing.md and NOTICE, read against the
# offer they all state (#227, built in #349). The order-form guard above holds
# the *rights* together; this one holds the *prices, the term and the OEM rule*
# together. They came apart before: the site sold a retired price for weeks
# after the model changed, in nineteen files, and nothing compared them.
run oneoffer  python3 scripts/ci/one_offer_in_every_document.py
run oneofftst python3 scripts/ci/test_one_offer_in_every_document.py
# The offer's second road out of here (#479): a crate README becomes the front
# page on crates.io, a pyproject becomes the PyPI page, and docs/hero becomes the
# image on the first screen of the public repository. None of them quote the
# price list, so none of them failed the pair above -- and all of them were still
# selling the retired model three days after it was withdrawn. A published
# version is immutable, so this belongs in the fast lane, before the push.
run pubprice  python3 scripts/ci/no_retired_price_in_a_published_readme.py
# The guard above stays before the push; its 21 s self-test moved to the landing
# lane on 23-09-2026 (lanebudget, #596) -- it tests the guard, not the README.
zwaar pubpricet python3 scripts/ci/test_no_retired_price_in_a_published_readme.py
# One product, six registry pages, six version numbers (#358). On 09-09-2026
# the live pages of one engine read beta.18, 1.0.0b17.post5, beta.17.5,
# beta.17.4.1, beta.17.3 and beta.17.2 -- two months apart, in six manifests no
# tool read together. Test first: a version comparison that reads five of six
# manifests looks exactly like one that reads all of them.
run regver_t  python3 scripts/ci/test_every_registry_names_the_same_release.py
run regver    python3 scripts/ci/every_registry_names_the_same_release.py
# The other half of what a registry page is: the licence document it hands the
# reader. `@pdfluent/sdk-wasm` says `SEE LICENSE IN LICENSE-OFFER`, which is a
# promise about a file, and npm packs `files` -- not the directory. Every
# published version up to 1.0.0-beta.17.5 made that promise and shipped nothing
# to keep it. No wasm build: the guard runs the real transform script over a
# reference directory. Test first, as everywhere on this list.
run wasmlic_t python3 scripts/ci/test_the_wasm_package_carries_its_licence.py
run wasmlic   python3 scripts/ci/the_wasm_package_carries_its_licence.py
# The same shape one registry over: a green check that was not about the thing
# that was wrong. The 1.0.0 Linux wheel of 10-09-2026 passed `twine check` and
# PyPI would have refused it at the door, because `twine check` reads metadata
# and the tag said `linux_x86_64` (#358). Only the test runs here: the rule
# itself needs built wheels, and the runbook calls it on the machine that has
# them, right before the upload.
run wheeltag_t python3 scripts/ci/test_wheel_platform_tag_is_publishable.py
# And the channel whose guard ran right before `mvn deploy` while it still
# demanded `1.0.0-beta.N` of the canonical pom: on the 1.0.0 publish morning it
# refused a pom the six-manifest guard had just approved, and it would have
# accepted a stale pom whose beta number sat on a plugin (#358). The guard now
# compares the pom with the release; only the test runs here, the guard itself
# is a runbook step.
# The deploy it fronts happens at release, so the test rides the landing lane
# (lanebudget, #636).
zwaar mvnchan_t python3 scripts/ci/test_maven_channel_guard.py
# The musl CRT split (P0): -crt-static rides only the cdylibs via
# musl_cdylib_rustflags.sh, never the whole musl target — the config form
# dynamically linked the musl CLI and broke the Docker image.
# The split only takes effect in release builds; it rides the landing lane
# (lanebudget, #636).
zwaar muslcrt python3 scripts/release/test_musl_crt_split.py
# The release remap block is computed from the builder's own paths: helper
# output over fake Linux, Windows and Mac envs plus the workflow shape that
# sources it. Hermetic and fast (stub rustc, no toolchain); it rides the push.
run remap python3 scripts/release/test_release_rustflags.py
# The shipped natives name no build host: binary_host_paths.py over fixtures
# (a leaking native and wheel must fail) plus the gates that run it.
# Hermetic and fast; it rides the push.
run hostpaths python3 scripts/release/test_binary_host_paths.py
# The same rules over the Rust sources of every published crate: a literal
# only one target compiles fails here, before that target's release leg.
# Hermetic and fast; it rides the push.
run srclits   python3 scripts/release/source_literals_name_no_host_path.py
run srclitst  python3 scripts/release/test_source_literals_name_no_host_path.py
# The artefact gate judges every native on its own: per-native size floor,
# kind/arch/libc off the bytes, universal2-or-both mac wheels plus abi3, one
# platform table, and the licence audit over jar/wheels/nupkg/CLI archives.
# Both judge the release gate over fixtures; they ride the landing lane
# (lanebudget, #636).
zwaar artgate python3 scripts/release/test_artifacts_cover_every_platform.py
zwaar licaudit python3 scripts/release/test_license_audit.py
# The stagers never stage a host build under a named platform (that put an
# x86 dylib in the osx-arm64 slot); the pom sizes every staged native too.
# Staging happens in the release lane; it rides the landing lane
# (lanebudget, #636).
zwaar stagefb python3 scripts/release/test_stage_natives_have_no_host_fallback.py
# The smokes test exactly the given artefact (--artifact/--platform are
# honoured; the release lane requires --artifact), and a missing smoke
# script fails the runner instead of passing it.
run smokeart python3 scripts/release/test_smoke_runner_requires_artifact.py
# The npm publish is manual (dispatch with publish_npm=true) behind the
# release gate, on all seven napi targets including aarch64 musl via zig.
# The publish it fronts happens at release; it rides the landing lane
# (lanebudget, #636).
zwaar nodepub python3 scripts/release/test_node_bindings_publish_gate.py
# The extract_office suite against this tree: a throwaway venv, maturin
# develop, pytest. Landing/full only — a debug engine build on a cold target
# dir, incremental after that.
zwaar officext bash scripts/release/run_office_tests.sh
# The balanced-tier binding parity, both channels, against the CLI references
# they pin. Unlike the office runner above, run_balanced_binding_tests.sh
# judges a prepared machine and does not prepare one, so this gate builds the
# CLI (ort-layout, for `models install`), the wheel and the addon (both ort
# features) from this tree first, in a throwaway venv. The models come from
# PDFLUENT_MODELS_BASE or the release base, and stay in the per-user dir, so
# a warm machine installs nothing. Landing/full only: two native builds plus
# a model download on a cold cache. (#661)
# Subshell body, so the EXIT trap below cleans only this gate: the file's
# own EXIT trap holds the gate lock and must survive every gate.
balbind_prepared() (
  set -euo pipefail
  cargo build -q -p pdfluent-cli --features ort-layout --locked
  BALBIND_WORK="$(mktemp -d "${TMPDIR:-/tmp}/balanced-binding-gate.XXXXXX")"
  trap 'rm -rf "$BALBIND_WORK"' EXIT
  python3 -m venv "$BALBIND_WORK/venv"
  # shellcheck disable=SC1091
  . "$BALBIND_WORK/venv/bin/activate"
  python -m pip install --quiet --upgrade pip "maturin==1.15.0" pytest
  (cd crates/pdf-python && maturin develop --features ort-layout,ort-tables)
  (cd crates/pdf-node && npm install --no-audit --no-fund && npm run build)
  PDFLUENT_CLI="$PWD/target/debug/pdfluent" \
    PDFLUENT_MODELS_BASE="${PDFLUENT_MODELS_BASE:-https://github.com/pdfluent/pdfluent-sdk/releases/download/models-v1/}" \
    bash scripts/release/run_balanced_binding_tests.sh
)
zwaar balbind balbind_prepared
# The r4 binding suites against this tree: the test targets the r4 merge
# added, each with the native build it needs. Landing/full only.
zwaar r4py     bash scripts/release/run_r4_pdfpython_tests.sh
zwaar r4node   bash scripts/release/run_r4_node_tests.sh
zwaar r4java   bash scripts/release/run_r4_java_tests.sh
zwaar r4dotnet bash scripts/release/run_r4_dotnet_tests.sh
# The semver contract against v1.0.0, before a release is cut rather than at
# publish time: until 14-10-2026 semver_gate.sh ran only in publish-crates.yml,
# and two 1.x breaks (a new pub field on BatchConfig, a new variant on
# BookmarkAction) sat on the release branch unseen until a trial run of it.
zwaar semver bash scripts/ci/semver_gate.sh
# Per defining crate, the enums sealed on 14-10-2026 stay sealed, with a
# floor on how many new enums the check must see so an empty walk cannot
# pass. Same check and feature list as the gate above.
zwaar enumseal bash scripts/ci/test_new_enums_are_sealed.sh
zwaar r4capi   bash scripts/release/run_r4_capi_tests.sh
# NativeLoader reads musl off the JVM's own interpreter (a glibc host with
# the musl package installed is still glibc): javac plus fixture binaries.
run javamusl python3 scripts/release/test_native_loader_musl.py
run javasrc11 python3 scripts/release/test_java_stays_source11.py
# The build lane gates what it built and names the rest as not built.
run lanecons python3 scripts/release/test_build_all_platforms.py
# Every `napi build` line passes only flags the pinned CLI lists (the musl
# leg's --zig, never the napi 3 --cross-compile the 2.18 CLI rejects).
run napiflags python3 scripts/release/test_napi_build_flags.py
# The linux-gnu Node bindings build against glibc 2.17 and the release gate
# reads the floor off the bytes (1.1.0 candidates needed 2.39 and 2.43).
run glibcfloor python3 scripts/release/test_node_glibc_floor.py
# The wheel scrub rewrites every build-host path in the SBOM (linux /srv,
# macOS /Volumes, Windows C:) and refuses a wheel that still carries one: the
# 1.1.0 CI wheels had 203 each, the 1.0.0 wheels none.
run scrubwhl python3 scripts/release/test_scrub_wheel.py
# No TypeScript reserved word as a parameter name in the published Node
# typings: napi copies the Rust parameter names verbatim, and `default` /
# `new` broke every consumer compiling with `skipLibCheck: false`.
run nodetyp python3 scripts/ci/node_typings_have_no_reserved_parameter.py
# The CLI channel's own tests: crates.io gate, Dockerfile, Homebrew, smoke.
# The first three judge release artefacts; they ride the landing lane.
# smokecli stays: it is the push's grip on the CLI smoke (lanebudget, #636).
zwaar cligate python3 scripts/release/test_cli_cratesio_gate.py
zwaar clidock python3 scripts/release/test_cli_dockerfile.py
zwaar brewform python3 scripts/release/test_render_homebrew_formula.py
run smokecli python3 scripts/release/test_smoke_cli.py
run smokeflags python3 scripts/release/test_smoke_cli_surface.py
run smokebin python3 scripts/release/test_smoke_binary_layout.py
# The archive builder derives its triples from the platform table and refuses
# a dynamic musl binary off the binary's own headers, not the host's loader.
run cliarch python3 scripts/release/test_cli_build_archives.py
# The sdist builder refuses the maturin that panics on the split workspace.
# The sdist is cut at release; it rides the landing lane (lanebudget, #636).
zwaar sdist python3 scripts/release/test_build_pdfluent_sdist.py
# The C API collector assembles the judged dir and the seven tarballs.
# Release assembly, and the slowest gate on a machine with a toolchain; it
# rides the landing lane (lanebudget, #636).
zwaar capicollect python3 scripts/release/test_collect_capi_release.py
# package_cabi.sh cuts one tarball per platform from a prebuilt lib.
# Cut at release; it rides the landing lane (lanebudget, #636).
zwaar cabiplat python3 scripts/release/test_package_cabi.py
# The ORT pin table stages the five runtime libraries `models install`
# fetches, verified twice (archive and member), with the MIT licence and
# notices beside each. Fixture archives, no network, seconds; fast lane.
run ortpins python3 scripts/release/test_fetch_ort_libs.py
# The GCS weights stage verified or refuse, against a stub gcloud. Fixture
# bytes, no network, seconds; fast lane.
run stagemodels python3 scripts/release/test_stage_models.py
# The release workflows ship the ort variant on all targets, and the
# default crate plus WASM stay pure Rust. Static YAML reads; fast lane.
run ortvariant python3 scripts/release/test_release_ort_variant.py
# The four release texts fetch_models_release.py pins to tree bytes check
# out LF under autocrlf (win-x64 smoke refused a correct release on CRLF).
# Throwaway local clones, no network, seconds; fast lane.
run pinnedlf python3 scripts/release/test_pinned_texts_check_out_lf.py
# release-natives.yml builds the gate's seven platforms, every channel.
run nativesmatrix python3 scripts/release/test_release_natives_matrix_matches_the_gate.py
# The release path never forgives an unbuilt platform.
run noallowmiss python3 scripts/release/test_release_path_never_allows_missing.py
# The consistency gate knows which crates are first-publishing.
run relcons python3 scripts/release/test_check_release_consistency.py
# Release-smoke packages the crate under test together with its transitive
# workspace dependencies, or a pre-publication run fails resolving =1.1.0
# from crates.io by construction (#636). Static plus `cargo metadata`; fast.
run pkgdeps python3 scripts/release/test_package_with_workspace_deps.py
# The self-hosted runner pool: routing, the publish guard, script soundness,
# the macOS runner, the Darwin setup, the Rosetta wrapper, and the dry run.
# All seven read files and print commands; nothing compiles.
run runpool python3 scripts/release/test_release_workflows_route_through_runner_pool.py
# No bash step on a Windows leg runs under the default shell (pwsh there).
# Five YAML files, milliseconds, fast lane.
run winshell python3 scripts/release/test_release_windows_steps_run_under_bash.py
# Windows ships python, not python3: a Windows-reachable bash step resolves
# its interpreter instead of calling a bare python3. Reads YAML; fast lane.
run winpy python3 scripts/release/test_release_windows_python_resolves_its_interpreter.py
# gh is absent from the self-hosted runners, and grep -q under pipefail is
# EPIPE: release steps curl the API and compare captures. Reads YAML; fast.
run nogh python3 scripts/release/test_release_steps_use_no_gh.py
# Every package name the release workflows look up exists in the workspace.
run relnames python3 scripts/release/test_release_package_names_exist.py
run selfpub python3 scripts/release/test_self_runs_never_publish.py
# A release run shows every error, not the first: validation jobs survive
# a failed leg (!cancelled) while publish jobs keep their hard gates.
run everyerr python3 scripts/release/test_release_runs_show_every_error.py
zwaar runshell python3 scripts/release/test_runner_scripts_pass_shellcheck.py
run keepcheck python3 scripts/release/test_selfhosted_setup_keep_checks.py
zwaar nodeinst python3 scripts/release/test_selfhosted_node_install.py
run pyvenv python3 scripts/release/test_selfhosted_python_venv.py
zwaar macrun python3 scripts/release/test_mac_runner.py
zwaar darwinsetup python3 scripts/release/test_selfhosted_setup_darwin.py
zwaar rosetta python3 scripts/release/test_rosetta_x64.py
zwaar gcpdry python3 scripts/release/test_gcp_runner_dry_run.py
run setupref python3 scripts/release/test_setup_action_resolves_its_scripts.py
zwaar gcpdown python3 scripts/release/test_gcp_runner_down_deregisters.py
# The Windows half of the self-hosted runners: the setup script covers a
# bare windows-2022 VM (python.org pins, machine settings, bash refusal,
# machine-wide tools root). It reads files; nothing compiles.
zwaar winps1 python3 scripts/release/test_selfhosted_setup_ps1.py
# The wasm and binary publishes are manual and gated; every runbook publish
# section requires the gate and the smoke runner first; the smokes refuse
# typos and foreign platforms; and the guard job stays free of builds.
# The three publish gates ride the landing lane; smokeargs and nobuild stay
# on the push (lanebudget, #636).
zwaar wasmpub python3 scripts/release/test_wasm_publish_gate.py
zwaar binpub python3 scripts/release/test_release_binaries_gate.py
zwaar rungates python3 scripts/release/test_runbook_publish_gates.py
run smokeargs python3 scripts/release/test_smoke_args_and_platform.py
run fetchart python3 scripts/release/test_fetch_artefact.py
run nobuild python3 scripts/ci/test_orchestration_guard_runs_no_build.py
# The extract wrappers. The PDF-header routing test is hermetic (engine
# stubbed) and runs fast; the full suites need a tree-built engine or an
# npm install, so they ride the landing/full lanes.
run xrouting python3 integrations/pdfluent-extract/tests/test_pdf_header_routing.py
# The js runner installs the tree-built binding with its own binary beside it
# (the tarball excludes the .node and the =1.1.0 platform packages are not
# published yet). Static, over the script's text, so it runs fast.
run xtjsrun python3 scripts/release/test_run_extract_js_tests.py
zwaar xtractpy bash scripts/release/run_extract_tests.sh
zwaar xtractjs bash scripts/release/run_extract_js_tests.sh
# The clean-room scanner's own tests, hermetic over scratch trees. (#643)
run cleanroom python3 scripts/ci/test_cleanroom_scan.py
# The compat tooling's unit tests: usage tiers, coverage, measurement and
# spec validation. Standalone, stdlib only. (#643)
run cpapply python3 scripts/compat/test_apply_usage.py
run cpcover python3 scripts/compat/test_coverage.py
run cpusage python3 scripts/compat/test_usage_measure.py
run cpvalid python3 scripts/compat/test_validate_spec.py
# The differential harness's own tests, over canned data. The full `check`
# against the tree wheel stays unwired: five tests/parity fixtures it reads
# were removed with the parity suite and it exits 1 without them. (#643)
run compatself python3 tests/compat/test_self.py
# The door difference self-test: the three D7 chunk tests report difference
# on base-14 boxes only, and any new divergence still fails. Needs pytest
# but no built binding and no spec checkout, so it runs in the fast lane.
run doordiff python3 -m pytest crates/pdf-python/tests/door/test_door_difference_self.py -q
run nocairoman python3 tests/compat/no_cairosvg_in_shipped_manifests.py
run nocairoimp python3 tests/compat/no_cairo_in_shipped_sources.py
# The crates.io half of the same morning: publish_ordered.sh asked `cargo info`
# from the repo root, which inside the workspace answers out of ./crates/<name>
# and never reaches the registry. All 32 crates read as already published, so
# --live skipped every one of them and exited 0 saying "done" (#358).
run cratespub_t python3 scripts/ci/test_crate_version_is_on_crates_io.py
# And the third defect the same publish turned up: the audit refuses a dirty
# tree and writes its reports into one, so the audit of crate N stopped the
# audit of crate N+1 and a 32-crate chain never got past the first (#358).
# A publish-chain self-test, 63 s on a Mac: it means something at a publication,
# not a push, so it runs in the landing lane since 23-09-2026 (lanebudget, #596).
zwaar auditdirt_t python3 scripts/ci/test_the_audit_does_not_dirty_the_next_one.py
# And that the rule it exercises is the one publish_ordered.sh uses. It was a
# second copy, and only the audit's copy ever got the repair (#358).
run treedirt_t python3 scripts/ci/test_the_dirty_tree_rule_is_one_rule.py
# And the fourth: a fork whose version was already on crates.io while its files
# had moved on. Two crates were published before the run found out (#358).
run reusedver_t python3 scripts/ci/test_published_version_matches_the_workspace.py
# The same question between releases, offline: the registry's answer is kept
# in scripts/release/published_crate_contents.json, and a commit that changes a
# recorded crate without moving its version is red here. crates/lopdf moved
# under the published 0.39.5 on 14-10-2026 and only the 1.1.0 dry run saw it.
run reusedrec python3 scripts/release/published_version_matches_the_workspace.py --against-record
run reusedrec_t python3 scripts/ci/test_published_version_matches_the_record.py
# And the fifth: a file that ships naming a path only this repository has. The
# prepublish audit refuses it, one crate at a time, during the publish -- this
# asks all 32 before one is uploaded (#358). `cargo package --list` per crate,
# so it belongs in the landing lane and not in the fast one.
zwaar pkgpaths python3 scripts/ci/a_published_file_names_no_internal_path.py
run pkgpaths_t python3 scripts/ci/test_a_published_file_names_no_internal_path.py
# And the shape that hid it: a workflow step naming a script that is not in the
# tree. The GitLab side has been checked since a release job was found calling
# one; the GitHub side was not, and had one.
run wfscripts  python3 scripts/ci/a_workflow_runs_no_script_that_is_not_there.py
run wfscripts_t python3 scripts/ci/test_a_workflow_runs_no_script_that_is_not_there.py
# The same question one step earlier: not what the package promises a reader,
# but whether the commands in it can run at all. `npm test` in the published
# Node package named two test files deleted with the licence machinery in #226
# and failed on every invocation for four days; a retroactive review found it,
# nothing that runs did (#515). Test first, as everywhere on this list.
run npmpath_t python3 scripts/ci/test_a_test_script_names_a_file_that_exists.py
run npmpath   python3 scripts/ci/a_test_script_names_a_file_that_exists.py
# The same licence, read against the binary instead of against the other
# document. LICENSE-COMMERCIAL §5 now states as a fact that nothing in the
# software reads a key, refuses a capability or marks output (#226) -- and the
# sixty-five capability gates it describes did not arrive as a decision, they
# arrived one locally-sensible edit at a time. The test first, because this
# guard's good day looks exactly like a guard that reads nothing.
run keytest   python3 scripts/ci/test_no_licence_key_in_a_binding.py
run nokey     python3 scripts/ci/no_licence_key_in_a_binding.py
run lictest   python3 scripts/ci/test_license_gate.py
# IN THE LANDING LANE SINCE #473: it regenerates the committed Java fixture with
# `cargo run -p pdf-manip --example ...` and compares, which is a compiler, and
# on a quiet cpx42 it was 47 s -- the single most expensive gate a push paid for
# that was not a guard reading a file. The fixture it watches is committed and
# changes only with the generator or the binding, both of which the landing lane
# reads before anything lands.
# The fixture lives in `pdf-java` and the generator is `pdf-manip`, and
# `pdf-java` is a dependent of `pdf-manip` -- so the one name covers both moves
# and a landing that touches neither pays nothing for it (#523).
crate_gate javafix pdf-java python3 scripts/ci/the_java_fixture_is_the_one_we_generate.py
run fixenv    python3 scripts/ci/a_fixture_cannot_touch_a_real_repo.py
run fixenvtst python3 scripts/ci/test_a_fixture_cannot_touch_a_real_repo.py
run licpol    python3 scripts/ci/the_licence_policy_says_what_it_must.py
run licpoltst python3 scripts/ci/test_the_licence_policy_says_what_it_must.py
run licregs   python3 scripts/ci/one_licence_three_registers.py
run licregtst python3 scripts/ci/test_one_licence_three_registers.py
run errdocs   python3 scripts/ci/error_codes_have_an_anchor.py
run errgen    python3 scripts/ci/error_catalogue_is_generated.py
# Learned decisions (#598): leakage, budget, and export reproducibility.
run learnleakt python3 scripts/ci/test_learned_training_uses_no_rig_document.py
run learnleak  python3 scripts/ci/learned_training_uses_no_rig_document.py
run learnbudt  python3 scripts/ci/test_a_learned_model_stays_in_budget.py
run learnbud   python3 scripts/ci/a_learned_model_stays_in_budget.py
run learnexp   python3 scripts/ci/test_learned_export_is_reproducible.py
# A lone header label spans the columns it heads (T4, #574), in both the
# Rust decoder and the Python one, under either variant of the switch.
run learnspan  python3 scripts/ci/test_lone_header_label_spans.py
run learnlines python3 scripts/extraction/learned/list_lines.py --self-test
run learnown   python3 scripts/ci/test_train_lists_refuses_its_own_output.py
run learnv2eval python3 scripts/ci/test_train_table_is_v2_eval_guard.py
run learnv2cap  python3 scripts/ci/test_train_table_is_v2_cap.py
run learnv2s3   python3 scripts/ci/test_train_table_is_v2_strat300.py
run learnv2era  python3 scripts/ci/test_train_table_is_v2_era.py
run learnv2unv  python3 scripts/ci/test_train_table_is_v2_unverifiable.py
run learnrule  python3 scripts/ci/test_training_rules_refuse.py
run pubtera    python3 scripts/ci/test_pubtables_era_draw.py
run pubfetch   python3 scripts/ci/test_fetch_training_pages_documents.py
run pubtruth   python3 scripts/ci/test_pubtables_truth.py
run pubstrat   python3 scripts/ci/test_pubtables_eval_strat300.py
run pubrefus   python3 scripts/ci/test_pubtables_strat300_refused.py
run learnstyle python3 scripts/extraction/learned/style.py --self-test
run learntree  python3 scripts/ci/test_engine_tree_scope.py
run learnstak  python3 scripts/ci/test_training_decisions_refuse.py
run learnself  python3 scripts/ci/test_self_exclusion.py
run learnhl    python3 scripts/extraction/learned/train_heading_lines.py --self-test
run learnedge  python3 scripts/extraction/learned/list_edges.py --self-test
run learndraw  python3 scripts/extraction/learned/drawings.py --self-test
run learnrast  python3 scripts/extraction/learned/raster.py --self-test
run learnfeat  python3 scripts/extraction/learned/features.py --self-test
run learnprops python3 scripts/extraction/learned/table_proposals.py --self-test
run tatrfxgen  python3 scripts/extraction/tables_b/test_make_tatr_rust_fixtures.py
# The Node column of that catalogue named five error classes nobody ever built,
# and the Node binding raised no code at all while four documents said to branch
# on one (#587). The sync check now refuses a class the package does not export;
# the contract test builds the addon and provokes every code it claims.
run errsynct  python3 scripts/ci/test_error_catalogue_sync.py
run errsync   bash scripts/release/error_catalogue_sync.sh
crate_gate nodeerr pdf-node bash scripts/ci/the_node_error_contract_holds.sh
crate_gate errtests pdfluent cargo test -q -p pdfluent --test error_codes_stable --test processing_limits
# The pdf-engine suite behind the `parallel` feature is one file, and the plain
# `cargo test -p pdf-engine` the `test` gate runs never compiles it -- without
# the feature it is zero tests, with it an abort (#636). The landing lane
# runs it whenever pdf-engine is in the selection, which a change to
# pdf-render (where the guard lives) or to Cargo.lock also brings in.
crate_gate engparallel pdf-engine cargo test -p pdf-engine --features parallel --test render_all_nested_in_a_small_pool
# The pdf-engine suite behind the `ort-layout` feature is the layout unit
# tests plus the model parity test, and the plain `cargo test -p pdf-engine`
# the `test` gate runs never compiles either -- without the feature they are
# zero tests, with it the post-processing and the PIL resampler (#641). The
# landing lane runs the unit tests (no model, no library) through the
# zero-test guard whenever pdf-engine is in the selection; the parity test
# needs the ep03 weights and the proxy pages, so it runs on demand with
# PDFLUENT_LAYOUT_PARITY_DIR and PDFLUENT_MODEL_DIR set, and skips aloud
# without them.
crate_gate englayout pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --lib layout::
# The balanced page assembly's unit tests, beside the layout ones: same
# feature, same shape, no model and no library, through the zero-test guard
# whenever pdf-engine is in the selection (#641). The adapter parity test
# needs its pages, so it runs on demand with
# PDFLUENT_BALANCED_PARITY_DIR set, and skips aloud without it.
crate_gate engbalanced pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --lib balanced::
# pdf-manip under the release cfg (`--cfg pdfluent_release`, set by
# scripts/release/release_rustflags.sh): the variant every release artefact is
# built as, with the checkout-only CMap and font-pack lookups compiled out.
# A compile plus 387 unit tests, 80 s cold on the Mac: landing lane, whenever
# pdf-manip is in the selection; the push lane never compiles. RUSTFLAGS
# replaces the config's, so -D warnings is repeated.
crate_gate manipreltest pdf-manip env RUSTFLAGS="-D warnings --cfg pdfluent_release" python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-manip --locked --lib
# The quality tier's tests, beside the balanced ones: same feature, same
# shape, no model and no library, through the zero-test guard whenever
# pdf-engine is in the selection (#641). The missing-model test points the
# tier at a scratch dir and asserts the refusal.
crate_gate engquality pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --test quality
# The balanced tier's TATR-hook tests, beside the quality ones: same feature,
# same shape, no model and no library (the fake seam), through the zero-test
# guard whenever pdf-engine is in the selection (#636). The R1 step, the
# fallback identity and the orphan guard run here; the refusal half runs
# here too, on this feature without `ort-tables`.
crate_gate engbalancedtatr pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --test balanced_tatr
# The balanced tier's region-fragment veto and rule A2 (drawing_dense) tests:
# same feature, same shape, operator-built pages and no model, through the
# zero-test guard whenever pdf-engine is in the selection (#636).
crate_gate engbalancedfragment pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --test a_region_fragment_grid_in_a_balanced_box
crate_gate engrulea pdf-engine python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdf-engine --features ort-layout --test rule_a_balanced
# The TATR parity tests behind the `ort-tables` feature are one file, and the
# plain `cargo test -p pdf-engine` never compiles them -- without the feature
# they are zero tests. The landing lane runs them whenever pdf-engine is in
# the selection; the parity runs without a dylib, the end-to-end skips loudly
# without one. The engine-hook file rides the same run for its own
# `ort-tables` half (the model-missing refusal and the three-page end-to-end,
# skipped loudly without a dylib); its fake seam runs on every build under
# the plain suite.
crate_gate engtatr pdf-engine cargo test -p pdf-engine --features ort-tables --test tatr_postprocess_matches_python --test tatr_engine_hook
# The CLI suite through the zero-test guard: when this crate is in the
# landing selection its tests run here, and a run that executes nothing is
# red rather than green (#636). Deliberately also in ci-ephemeral.yml as
# the `clitest` job: the scoped `test` gate above proves the suite passes,
# this one proves it ran, and a landing deserves both verdicts -- pre-push
# here, on a clean checkout there. This lane is the blocking one: it runs
# on every landing, while the wave runs on dispatch.
crate_gate clitest pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli
# The CLI half of the balanced tier: the models unit tests, the models
# integration tests and the extract suite, all behind `ort-layout`, through
# the zero-test guard whenever pdfluent-cli is in the selection (#641). No
# model, no library, no network: scratch dirs and fed bytes.
crate_gate climodels pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli --features ort-layout --lib cmd_models::
crate_gate climodelt pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli --features ort-layout --test cmd_models
crate_gate clibalanced pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli --features ort-layout --test cmd_extract
# The CLI half of TATR in the balanced tier: the extract suite behind
# `ort-layout,ort-tables`, through the zero-test guard whenever
# pdfluent-cli is in the selection (#636). No model, no library, no
# network: a scratch dir proves the flag reaches the TATR loader, and the
# refusal test compiles out where the feature is on.
crate_gate clibalancedtatr pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli --features ort-layout,ort-tables --test cmd_extract
# The models end-to-end behind `ort-layout,ort-tables`: an install out of
# local copies, then `balanced` (and TATR) with no flags, through the
# zero-test guard whenever pdfluent-cli is in the selection (#636). The
# end-to-end skips loudly without the local weights and library, like the
# engine's TATR end-to-end; the refusal matrix beside it needs no files.
crate_gate climodeltatr pdfluent-cli python3 scripts/ci/tests_actually_ran.py cargo -- cargo test -q -p pdfluent-cli --features ort-layout,ort-tables --test cmd_models
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
# And the same question about the tracker rather than about the code: a story is
# closed with a commit master actually has. Seven were closed on a branch that
# was later written off, each naming a real sha that was never an ancestor of
# anything, and the tracker said seven things were done that the product did not
# have (#348).
#
# HERE and not in a workflow, for the reason mirrordrift and checkouts are here:
# it needs `gh` with a token for the tracker AND this repository's history, and
# an Actions workspace has neither. It degrades to SKIPPED (not a pass) on a
# machine without access, so it can never speak about a run it did not judge.
#
# ADVISORY, and this is the same owner decision as `groen` above (#331). What it
# watches is the TRACKER, which is not in anybody's push: it fired on its own
# landing run over #227, closed by another terminal minutes earlier, and refused
# a landing that had nothing to do with it. That is the shape that closed master
# four times in 24 hours -- a queue stopped by somebody else's state.
#
# What is given up is real: a closure without a sha now goes unnoticed until
# somebody reads this output or the next run of the check. What is not given up
# is the measurement, which is the half that was missing entirely -- the seven
# closures of #348 stood for two weeks with nothing looking at them at all.
# ADVISORY, and it reports rather than judges -- so the machine half of a split
# lane leaves it to the half that runs everything else. Two copies of an
# advisory report is two chances to read the older one. (#474)
if [ "$PLAATS" != machine ] && [ "$FULL" = 1 ]; then
echo "--- closed stories (advisory; the tracker is not in this push) ---"
python3 scripts/ci/a_closed_story_names_a_landed_sha.py 2>&1 | sed 's/^/  /' || true

fi
# Its test stays a hard gate. That watches this repository's own code, which IS
# in the push, and it is the half that can be broken here.
run closedstoryt python3 scripts/ci/test_a_closed_story_names_a_landed_sha.py
run prstale   python3 scripts/ci/pr_staleness.py
run prstaletst python3 scripts/ci/test_pr_staleness.py
run sweep     python3 scripts/ci/test_sweep_spares_the_claimed_instance.py
run sweepsafe python3 scripts/ci/test_sweep_fails_safe.py
run regfetch  python3 scripts/ci/test_de_registerwachters_raken_de_echte_repo_niet.py
run reaporder python3 scripts/ci/reaping_happens_after_the_work.py
run guardrun  python3 scripts/ci/guards_do_not_hide_behind_each_other.py
run buildmnt  bash scripts/ci/test_desktop_env.sh
run buildheal bash scripts/ci/test_cargo_target_health.sh
# THE TWO SLEEPS, AND WHY THEY LEFT THE PUSH LANE (#494).
#
# The lock this gate is holding right now, and the trap that deletes the machine
# it is holding: both are driven against fakes, no token is read and no instance
# is bought, and both were in the fast lane on the argument that a lock unwired
# on a branch is discovered on master. Measured on a quiet cpx42 they are 71 s
# and 70 s -- 141 s of a 307 s lane, 46% of every push, and almost all of it is
# `sleep`: they wait out lock timeouts and boot timeouts, so the seconds do not
# shrink on a faster machine or a warmer one.
#
# What made the argument true was that the fast lane was the only place they
# ran. It is not: both stand in ci.yml's guard job, on every pull request, on
# the same code and against the same fakes -- and since #333 every change to
# this repository arrives through one. So the branch that unwires a trap is red
# before it is merged, on a hosted runner, without 141 s on every push that
# never touches either file.
#
# What is given up, precisely: a push that changes nothing about the lock now
# hears about somebody else's broken lock at the landing rather than at the
# push. That is minutes, and it is the same trade the compiling gates took in
# #473.
zwaar gatelock  bash scripts/ci/test_one_gate_at_a_time.sh
zwaar remotegate bash scripts/ci/test_remote_gate.sh
# And that the money has a ceiling that is actually read. The trap above is
# about a machine nobody deleted; this one is about a machine nobody should
# have bought. Both fail the same way -- invisibly, until a bill -- and both
# run against fakes, so neither costs anything here. (#490)
run ledger    bash scripts/ci/test_hetzner_ledger.sh
# And that no fixture can book into the operator's own ledger. Twice a test has
# spent money nobody spent -- EUR 1.33 in #454, EUR 2.2 on 10-09-2026 -- and
# both times the second symptom was worse than the first: the day's cap was
# reached, so every gate stopped buying an instance and compiled on the Mac.
# Reads the fixtures; costs milliseconds. (#517)
run ledgerfix python3 scripts/ci/a_fixture_cannot_touch_the_real_ledger.py
run ledgerfixtst python3 scripts/ci/test_a_fixture_cannot_touch_the_real_ledger.py
# And the pool, whose failure mode is the quietest of the three: gates keep
# passing and machines keep being deleted correctly, and the only symptom of a
# pool that stopped pooling is next month's bill. (#491)
#
# IN THE LANDING LANE, by the rule the budget guard states: a gate belongs there
# when what it watches only matters at a landing. A bill is judged over a day,
# and landings happen many times a day, so a pool that stopped pooling is caught
# within the hour it would first cost anything. Eighteen seconds on every push,
# against a fast lane that is already over its budget, is not what this is worth.
zwaar gatepool  bash scripts/ci/test_gate_pool.sh
# And that preparing an instance does not throw away the master ref the
# gate just pushed onto it -- the fault that made the first gate on a
# working warm image red about a merge base its own preparation had
# deleted (#482). It runs against temporary repositories and touches no
# remote.
run gateprep   bash scripts/ci/test_gate_instance_prepare.sh
run gatesweep python3 scripts/ci/test_the_orphan_sweep_reaps_a_gate_instance.py
# And the register that decides which half of a split lane judges which gate. It
# reads this file for the names it lists, so a gate that is renamed or removed
# makes it red here rather than making five gates unjudged on a machine that
# cannot judge them. (#474)
run gateplace python3 scripts/ci/test_gate_places_partition_the_lane.py
# The other half of the image contract (#495): a wave job that compiles has to
# bring its own toolchain, because "the image carries cargo on PATH" was written
# in three comments and true on none of them the morning three jobs died on
# `cargo: command not found`. Reads five workflow files and two scripts; no
# compiler, so it sits in the fast lane where the edit that breaks it is made.
run wavetoolchain python3 scripts/ci/a_wave_job_that_compiles_carries_its_toolchain.py
run wavetoolchaintst python3 scripts/ci/test_a_wave_job_that_compiles_carries_its_toolchain.py
# The release run that restored another leg's target/ and died on
# GLIBC_2.28 not found: the default rust-cache key has no target, so a cache
# step in a target-matrix job keys per matrix target (#636). Reads the
# workflows; milliseconds, so it sits in the fast lane with its test.
run cachekey python3 scripts/ci/a_rust_cache_key_names_its_target.py
run cachekeytst python3 scripts/ci/test_a_rust_cache_key_names_its_target.py
# The post step that cleaned the shared ~/.cargo/bin and deleted rustup on a
# self-hosted runner: every rust-cache step carries cache-bin: false, and this
# guard names file:line of one that does not (#636). Reads the workflows;
# milliseconds, so it sits in the fast lane with its self-test.
run cachebin python3 scripts/ci/every_rust_cache_keeps_cargo_bin.py
run cachebintst python3 scripts/ci/every_rust_cache_keeps_cargo_bin.py --self-test
# The same wall on the machine that outlives the job, and the half the guard
# above cannot see: a step that reaches cargo through the scripts it runs (#516).
# publication-guards.yml and two jobs in ci.yml said nothing about a compiler
# and died on `FileNotFoundError: 'cargo'` anyway. Reads workflows and follows
# the scripts they name; no compiler, so it belongs on the lane where the edit
# that breaks it is made.
zwaar cargopath python3 scripts/ci/a_job_that_reaches_cargo_finds_it_on_path.py
# ON LOAN (#575), the third of the moves. What it proves is about the WORKFLOW
# jobs -- that a job which reaches cargo finds it on the path -- and a workflow
# job is what a landing and master run, not a branch push.
zwaar cargopathtst python3 scripts/ci/test_a_job_that_reaches_cargo_finds_it_on_path.py
# The tag that published a release candidate (#358). `v1.0.0-rc.2` matched
# `tags: ['v*.*.*']` in five workflows and armed every publish job in them;
# @pdfluent/sdk-wasm@1.0.0 reached npm under `latest` before the run was read.
# The guard evaluates each publish condition under a pre-release tag rather
# than grepping for the clause -- the clause behind an `||` publishes anyway.
# Reads workflows, no compiler, so it belongs on the lane where the edit that
# breaks it is made.
run pretag    python3 scripts/ci/a_pre_release_tag_arms_no_publish.py
# The tag guard above stays before the push; its self-test rides the landing
# lane (lanebudget, #636).
zwaar pretagtst python3 scripts/ci/test_a_pre_release_tag_arms_no_publish.py
# The nightly warm image, and the two ways its refresh can be silently wrong:
# keeping the instance, and publishing an image of a half-built target
# directory. Both are invisible on the night they happen. (#474)
run gatesnap  python3 scripts/ci/test_refresh_the_gate_snapshot.py
# And that the same image still serves the editor. One image for both lanes is
# what lets a parked instance be handed across products (#508); nothing in this
# repository goes red when the editor's half of it rots -- an editor landing
# does, tomorrow, on a `cargo test` that cannot link. Reads seven files, costs
# milliseconds, so it sits in the fast lane where the edit that breaks it is
# made.
run sharedimage python3 scripts/ci/the_gate_image_carries_the_editor_layers.py
run sharedimagetst python3 scripts/ci/test_the_gate_image_carries_the_editor_layers.py
# The rule that a number may not depend on who is measuring. It reads the tree,
# costs milliseconds, and its subject is a defect that was green on two machines
# at once for as long as the baseline had been taken on a machine with a cache
# (#474, #476). The test first, because this guard's good day is silence.
run cachetest python3 scripts/ci/test_a_counted_compile_ignores_the_compiler_cache.py
run cachecount python3 scripts/ci/a_counted_compile_ignores_the_compiler_cache.py
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
run pkgmanif  python3 scripts/ci/every_package_manifest_names_the_same_release.py
run pkgmaniftst python3 scripts/ci/test_every_package_manifest_names_the_same_release.py
# The same five channels, one question further out (#205): the manifest above
# says which release each page describes; this says whether the page tells a
# visitor where the source is. Measured on the live pages 06-09-2026 -- all five
# named the repository in their metadata and none of the four that render a
# README named it above the fold.
run regreadme python3 scripts/ci/every_registry_readme_names_the_repository.py
run regreadmet python3 scripts/ci/test_every_registry_readme_names_the_repository.py
# And what those pages SAY (#358): a figure in a README carries a claim ID,
# no README sells the withdrawn licence model, and every one that states the
# licence states it as AGPL-3.0 or a commercial licence. Measured 07-09-2026:
# two READMEs still carried "0 fails on the 20K-PDF corpus" and the crates.io
# page opened with "a valid license is required for production use".
run readmeclaim python3 scripts/ci/a_readme_figure_carries_a_claim_id.py
run readmeclaimt python3 scripts/ci/test_a_readme_figure_carries_a_claim_id.py
# Structured extraction ships as Preview in 1.1.0 (#636): every structured
# entry point carries the label sentence, no plain-text one does.
run previewlabel python3 scripts/release/extraction_preview_label.py
run noai      python3 scripts/ci/no_ai_attribution.py
# The extraction rig's arithmetic, on fixtures, in under a second. The corpus
# run needs 500 documents, two foreign readers and a release build, so it lives
# in the wave (#367); what runs here is the half that can be wrong silently and
# still be believed.
run extract   python3 scripts/ci/test_extraction_axes.py
# The competitor half of the same rig (#390): that the published document
# list is still what its rule produces, that every column was measured on
# that list on one idle machine, and that the register and the run files
# name the same tools. No corpus and no tools -- it reads the run files
# the round left behind.
run extractcmp python3 scripts/ci/test_extraction_comparison.py
run extractrat python3 scripts/ci/loop/extraction_ratchet.py --self-test
# The paragraph axis of the structured extraction (#567), on readings written
# out by hand: the matching of a line to a block, the pair agreement and the
# summary. The corpus run needs the same 500 documents and the same two foreign
# readers as the axes above, so it is not here either.
run paraaxis  python3 scripts/extraction/paragraph_axis.py --self-test
# The split the ratchets run on (#461). Both are cheap and read no corpus: the
# first redraws all four sets from the committed populations and compares, the
# second breaks the split eight ways and requires a red each time, and the third
# is the aggregate check that fails a round which improved on the tuning set and
# lost on the test set.
run tuningsets python3 scripts/ci/the_tuning_and_test_sets_are_the_drawn_sets.py
zwaar tuningtst  python3 scripts/ci/test_the_tuning_and_test_sets_are_the_drawn_sets.py
# The thousand #365 publishes off (#365). Same footing and the same two files:
# the guard redraws the set from the committed list and the committed digests
# and refuses a manifest that is not what the rule produces, and its test breaks
# the holdout eleven ways and requires a red each time. Both read the repository
# and no corpus.
run holdout1k  python3 scripts/ci/the_holdout_1000_is_the_drawn_set.py
run holdout1kt python3 scripts/ci/test_the_holdout_1000_is_the_drawn_set.py
# The structured-extraction baseline (#566). The corpus run fetches five
# thousand annotated pages and drives two tools over them on an instance bought
# for the round; what runs here is the half that can be wrong silently: the five
# axes on fixtures, the protocol's own refusals -- a set or a tool whose licence
# says no must not be measurable by passing a flag -- the seeded split, and the
# seal over the sealed fifth.
# The wasm bundle's std rule as a source scan (#623): the Rust test lives in
# xfa-wasm and ran only where xfa-wasm was built, so a landing that touched
# pdf-engine alone let an Instant::now() onto master (f6f5f1ffc). The lists are
# read from that Rust test, so the two cannot drift. A second, both of them.
run wasmstd    python3 scripts/ci/wasm_bundle_calls_no_panicking_std.py
run wasmstdt   python3 scripts/ci/test_wasm_bundle_calls_no_panicking_std.py
run structax   python3 scripts/extraction/structured_axes.py --self-test
# The Office ground-truth reader (O2): the independent reading reproduces
# its controlled set exactly. Needs python-docx, python-pptx and openpyxl,
# which truth_gate.sh installs on a host that lacks them; the install and the
# generator cost seconds every push pays, and the tool only matters at a landing,
# so it rides the landing lane with doclstart and cliwire (lanebudget, #596).
zwaar officetruth bash scripts/office_bench/truth_gate.sh
# The Office bench runner (O4): the local columns and the scorer. Both reach
# cargo for the debug office_markdown, so they ride the landing lane.
zwaar officerun  python3 scripts/office_bench/test_run_local.py
zwaar officescore python3 scripts/office_bench/test_score.py
run structcell python3 scripts/extraction/pubtables_cells.py --self-test
run fintsplit python3 scripts/ci/test_fintabnet_split.py
run fintexcl python3 scripts/ci/test_fintabnet_eval_overlap.py
run fintarch python3 scripts/extraction/fintabnet_archive.py --self-test
run structbench python3 scripts/ci/loop/structured_bench.py --self-test
run teacherpg  python3 scripts/extraction/adapters/teacher_page.py --self-test
# The best tier's render checks, with the binary built (#641). teacherpg above skips its R-1/R-4
# render checks when no render_page_png is built -- the fast lane may not reach a compiler -- so
# this builds it and runs the self-test with --require-render at landing, where it compiles.
zwaar bestrender python3 scripts/extraction/adapters/test_best_render.py
run structbencht python3 scripts/ci/loop/test_structured_bench_order_provenance.py
run structftnpin python3 scripts/ci/loop/test_structured_bench_fintabnet_pinned.py
run structnoreal python3 scripts/ci/loop/test_structured_bench_fintabnet_norealign.py
run structftnmir python3 scripts/ci/loop/test_structured_bench_fintabnet_mirror.py
run structmisstl python3 scripts/ci/loop/test_structured_bench_missing_tools.py
run structckpt python3 scripts/ci/loop/test_structured_bench_checkpoint.py
# The post-AC round died of SIGPIPE in balanced_timing (08-10): the
# function-level red and the whole-script dry run beside it.
run structsigp python3 scripts/ci/loop/test_structured_bench_remote_sigpipe.py
run rounddry  bash scripts/ci/loop/round_dryrun.sh
# Rig page text never goes into the repo: the stripper and its test, beside
# the bench they clean up after.
run striptext python3 scripts/ci/loop/strip_run_text.py --self-test
run striptextt python3 scripts/ci/loop/test_strip_run_text.py
run structhold python3 scripts/ci/the_structured_holdout_is_the_drawn_set.py
# ParseBench, the third set (#623), scored by its own evaluator on the rig. What
# can be wrong here without the set: a sealed page in the sample, a page not
# checked against its pin, rules for pages nobody chose, page text in the run
# file (its verdict forbids that), and a headline that is not ParseBench's own.
run pbround    python3 scripts/extraction/parsebench_round.py --self-test
# The one canonical ParseBench summary (#574): the survivor-only mean the round
# headlines with, the paired comparison, and the timeout guards.
run pbsummary  python3 scripts/extraction/pb_summary.py --self-test
# The parity gate of #575: is the newest committed run at least the last one?
# Parity with Docling is the condition for the stable label and is FALSE
# today, so the lanes hold the ratchet; `--release` is what the publish paths
# run (preview) and `--stable` is what holds parity for the label.
run structpar  python3 scripts/ci/loop/structured_parity.py --check
run structpart python3 scripts/ci/test_structured_parity.py
# AND ITS OWN ARITHMETIC, which nothing ran until #580: the gate's three
# readings of a moving denominator and the resolution floor under the ratchet
# live in `--self-test`, so a change to them was only ever caught by whoever
# happened to run it by hand.
run structpars python3 scripts/ci/loop/structured_parity.py --self-test
# AND THE PREVIEW PUBLISH GATE (owner 06-10-2026, #574): `--release` exits on
# the ratchet under a PREVIEW header, and parity is `--stable`'s verdict for
# the label, which no publish path may call.
run structprev python3 scripts/ci/test_structured_preview_gate.py
# The benchmark page generator (#631): master's newest round per set, a claim and
# a run id per table, and no shipping with an empty, unclaimed or stale number.
run benchpage  python3 scripts/extraction/benchmark_page.py --self-test
run frames    python3 scripts/extraction/frames.py --self-test
# The paired numbers behind A35-A40 (bootstrap per document): the rescoring
# check recomputes the stored round(mean, 6) before its 3-decimal comparison,
# refuses a +0.0004 shift, and verdict() keeps GO above MARGIN 0.05 only.
run paired    python3 scripts/extraction/test_paired.py
run structholdt python3 scripts/ci/test_the_structured_holdout_is_the_drawn_set.py
# FinTabNet's first draw (T6, #574): fintabnet seals on the rig under its own
# seed while the guard still reads only sealed sets, and the lane's self-test
# draws before any round buys a machine.
run structfirst python3 scripts/ci/test_structured_first_draw.py
run overfit    python3 scripts/ci/loop/overfitting_gate.py --self-test
# The fidelity axis, on the same footing and for the same reason (#380). The
# corpus run converts and renders 300 documents on an ephemeral instance; what
# runs here is the arithmetic of the two thresholds and the promise that a run
# leaves a bundle behind. The Linux count of eleven survived the last round only
# as a number because that bundle did not, which is the failure this defends.
run fidelity  python3 scripts/pdfa/test_pdfa_fidelity_axis.py
# The performance axis (#384), same footing again. Five seconds here, and the
# corpus run is a thousand conversions on a machine bought for the clock. What
# this proves is the half that can be wrong silently: that a hang is bounded and
# the converter it gave up on is actually dead, that each document's peak memory
# is its own rather than a running maximum, and that a run refuses to measure
# under a machine class this machine is not. It runs the axis's own --self-test
# too, so one line covers both halves.
run perfaxis  python3 scripts/pdfa/test_pdfa_performance_axis.py
# And what that measurement renders (#454). The eval itself needs mutool,
# pdfinfo and the numpy/Pillow/scikit-image stack, which the fidelity job
# installs and no gate machine carries; what runs here is the arithmetic that
# decides how many sets a run renders, what it may borrow from a prior run's
# artifact, and whether a document the converter refused makes a completed
# measurement a failed one. All three were wrong in a way that cost instance
# hours and one red job on a measurement that had succeeded.
run evalsets  python3 crates/pdf-manip/examples/test_pdfa_round2_eval.py
# And who calls it (#547). The eval renders the source once per call, so two
# arms measured in two calls rendered the corpus four times for two candidates.
# The caller is where that came from: both calls were correct on their own.
run wavesource python3 scripts/ci/the_wave_renders_the_source_once.py
run wavesourcet python3 scripts/ci/test_the_wave_renders_the_source_once.py
run pinned300 python3 scripts/pdfa/the_300_are_the_pinned_300.py --self-test
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
# read off what the script prints.
#
# MOVED TO THE LANDING LANE 07-09-2026 (#425), and the reason it used to give for
# staying is worth answering rather than deleting. It said: this is the one
# operation that cannot be undone, and finding out ON MASTER that the filter
# broke is finding out too late. True -- and the landing lane runs BEFORE the
# push to master, not after it. Master takes only fast-forwards, so a broken
# filter is refused at the landing, before anything is on master and long before
# anything is published; seeding runs from master and never from a branch. What
# actually changes is when the AUTHOR hears about it: at the landing instead of
# at the branch push. That costs one re-run of a landing.
#
# What it bought: 257 seconds, measured, the single most expensive gate in the
# lane and 23% of it on its own.
zwaar seedtest  python3 scripts/ci/test_seed_public_repo.py
run noaitest  python3 scripts/ci/test_no_ai_attribution.py
run msgclean  python3 scripts/ci/geen_interne_zaken.py --bereik github/master..HEAD
# msgredact tests the redaction of msgclean's own report, not the commits of
# this push (msgclean above judges those, and stays in the fast lane). At 20 s
# it was the slowest gate in the lane; it rides the landing lane like
# seedtest (lanebudget, #596, train 2026-10-02e).
zwaar msgredact python3 scripts/ci/test_een_treffer_publiceert_de_term_niet.py
# The private list gained 25 lines of base64 in #222, to keep a key out of the
# published history. Every line used to be compiled as a regex, so one of them
# took `alle_regels()` down and the rest would have matched nothing: the two
# failures that make a term list worth testing separately from the guard.
run termlist  python3 scripts/ci/test_the_term_list_is_literal.py

run treetest  python3 scripts/ci/test_no_personal_address_in_the_tree.py
# MOVED TO THE LANDING LANE (#503). It walks every tracked file, so its cost
# follows the size of the tree rather than the size of the change: #503 adds
# some 39,000 lines of Adobe CMap tables in crates/pdf-manip/resources, and at
# 21 s this was the most expensive walker in a fast lane already 243 s against
# a budget of 240 s. What it watches -- a personal address anywhere in the
# tree -- only has to be true of what reaches master, and the landing lane
# still refuses it there. Its test stays here, where it costs nothing.
zwaar treeaddr  python3 scripts/ci/no_personal_address_in_the_tree.py
run oldreptest python3 scripts/ci/test_no_tracked_file_names_the_old_repository.py
run oldrepo   python3 scripts/ci/no_tracked_file_names_the_old_repository.py
# Evidence lives outside the tree: train Q carried meas/p4l-3f/ into the repo
# because meas/ was never ignored. The guard refuses any tracked path under
# meas/ or meas-out/; its test plants one in a fixture repository.
run evtest    python3 scripts/ci/test_no_tracked_evidence.py
run evidence  python3 scripts/ci/no_tracked_evidence.py
# Advisory, never blocking. It reports on live machines -- a queue, a busy
# runner, a server someone is still using -- and none of that is a reason to
# refuse a commit. The three-hourly run is where its exit code matters.
# ADVISORY, and it reports rather than judges -- so the machine half of a split
# lane leaves it to the half that runs everything else. Two copies of an
# advisory report is two chances to read the older one. (#474)
if [ "$PLAATS" != machine ] && [ "$FULL" = 1 ]; then
echo "--- infra (advisory) ---"
python3 scripts/ci/infra_health.py 2>&1 | sed 's/^/  /' || true

fi
run ci-yaml  python3 scripts/ci/ci_config_lint.py
run hangclass python3 scripts/ci/test_classify_render_outcome.py
# The golden set has one live copy and one backup, and the backup is only worth
# the check that reads it back. Hermetic and about four seconds: it builds a
# small tree, backs it up to a directory, and breaks the backup seven ways.
# In the fast lane because what it guards is the recovery path, and finding out
# on master that the restore stopped verifying is finding out on the wrong day.
# ON LOAN (#575), the second of the two moves that put the fast lane back under
# its budget. What `restore_golden.sh` protects is a baseline, and a baseline
# is read where a measurement lands rather than on a branch push -- the same
# reason its neighbours in the landing lane are there.
zwaar goldenset python3 scripts/ci/test_restore_golden.py
# The other half: the corpus comes back from a public bucket, and the machine
# that needs it has no AWS CLI. Hermetic -- stub transports on a built PATH, no
# network -- and about three seconds.
run govdocs   python3 scripts/ci/test_corpus_download_govdocs.py
run metadata cargo metadata --no-deps --format-version 1
run fmt      cargo fmt --all -- --check
scoped build  bash scripts/ci/run_build.sh
scoped clippy bash scripts/ci/run_clippy.sh
run licenses python3 scripts/release/license_registry_check.py
run advlists  python3 scripts/ci/the_advisory_lists_agree.py
run advliststst python3 scripts/ci/test_the_advisory_lists_agree.py
run advfloor  python3 scripts/ci/a_fixed_advisory_stays_fixed.py
run advfloortst python3 scripts/ci/test_a_fixed_advisory_stays_fixed.py
# apt on a machine that is still booting. In the fast lane because it is three
# subprocesses against stubs, and because the failure it guards costs a whole
# wave: three steps of 34163246411 died on one dpkg lock. (#448)
run aptlock   python3 scripts/ci/an_apt_install_waits_for_the_lock.py
run aptlocktst python3 scripts/ci/test_an_apt_install_waits_for_the_lock.py
run aptwrap   python3 scripts/ci/test_apt_get_waits_for_the_lock.py
# Two manifests that the extraction step needs to agree about, compared here in
# two seconds rather than forty minutes into a billed wave. (#448)
run extrset   python3 scripts/ci/the_extraction_set_names_the_corpus_it_is_given.py
run extrsettst python3 scripts/ci/test_the_extraction_set_names_the_corpus_it_is_given.py
run docsrefs python3 scripts/ci/docs_references_resolve.py
run docsreftst python3 scripts/ci/test_docs_references_resolve.py
run dcohook   python3 scripts/ci/test_prepare_commit_msg_signoff.py
# The guard above this file's own verdict: that the worktree it measured is
# still the worktree it was asked about. Cheap and in the fast lane, because
# a guard whose wiring only runs on a landing is a guard that gets unwired on
# a branch and discovered on master.
run treemoved python3 scripts/ci/test_the_tree_has_not_moved.py
scoped test  bash scripts/ci/run_test.sh
# THE EXCLUDED CRATES A LANDING REACHES (#623, COS 27-09-2026). xfa-wasm is out
# of the workspace build, so no landing compiled or tested it against a change
# to its dependencies -- and an `Instant::now()` landed in pdf-engine that its
# own guard test refuses. The full lane checks it always; the landing lane when
# its closure reaches it; a push defers it.
# `zwaar`: landing and full lanes. With nothing reached the script says so and
# exits 0 in a second, so an unreached landing pays nothing.
# shellcheck disable=SC2086
zwaar excldeps bash scripts/ci/run_excluded_dependents.sh $EXCLUDED_REACHED
# The publish audit, which is about the packages this repository ships rather
# than about one crate. It stays whole in both landing lanes: it packages the
# release channels and reads their manifests, so scoping it to a crate list
# would answer a different question than the one it was written for.
zwaar audit bash scripts/ci/run_audit.sh
# The last word before the verdict. Everything above has been measured; if the
# tree moved at any point after the final expensive phase, the cheap gates that
# ran afterwards judged a tree that is not this branch either.
tree_check
echo "----------------------------------------"
_lane="fast"; [ "$FULL" = 1 ] && _lane="full"; [ "$LANDING" = 1 ] && _lane="landing"
# WHERE THE LANE WENT, in every run and not only when somebody goes looking.
#
# Five names and the total, because the shape of this lane is a long tail behind
# two or three gates: on 07-09 two of a hundred and forty-six were 38% of it. The
# run is also appended to TIMINGS with the load average it ran under -- a lane
# time measured on a machine at load 100 says nothing about the lane, and the
# budget guard needs to know which runs it may judge.
if [ -f "${LOGDIR}/timings.tsv" ]; then
  _totaal=$(awk -F'\t' '{s+=$2} END{print s+0}' "${LOGDIR}/timings.tsv")
  echo "slowest gates: $(sort -k2 -rn -t"$(printf '\t')" "${LOGDIR}/timings.tsv" | head -5 |
        awk -F'\t' '{printf "%s %ss, ", $1, $2}' | sed 's/, $//')"
  echo "guard time: ${_totaal}s over $((pass+fail)) gates, ${SECONDS}s wall"
  _load=$(uptime | sed 's/.*averages*: *//' | awk '{print $1}' | tr -d ',')
  mkdir -p "$(dirname "$TIMINGS")" 2>/dev/null
  # AND WHICH LANE IT WAS, as a hash of this file (#494). Since the pool (#491)
  # one instance serves every branch in turn and the table outlives them all, so
  # a row can be a run of a lane that no longer exists: the budget guard read a
  # 308 s row written by master's lane and refused a branch whose own lane
  # measures 165 s. A time is a fact about a gate list, and the gate list is
  # this file -- so the row carries its fingerprint and the guard judges only
  # rows that match its own. A lane that has just been edited therefore has
  # nothing to judge until it has run once, which is the honest answer and the
  # one this table was built to give.
  _baanid="$( (shasum -a 256 "$0" 2>/dev/null || sha256sum "$0" 2>/dev/null) | cut -c1-12)"
  # And on which machine (#23-09): a pooled instance's table outlives the
  # branches it served, and a time is a fact about the machine that took it.
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$_lane" "$_totaal" "$SECONDS" "${_load:-unknown}" \
    "$(sort -k2 -rn -t"$(printf '\t')" "${LOGDIR}/timings.tsv" | head -8 |
       awk -F'\t' '{printf "%s=%s;", $1, $2}')" \
    "${_baanid:-unknown}" "$(python3 -c 'import socket; print(socket.gethostname())' 2>/dev/null || hostname)" \
    >> "$TIMINGS" 2>/dev/null || true
  # Keep the file a file and not a log: the guard reads the recent past, and an
  # unbounded append would outlive every question anyone asks of it.
  if [ "$(wc -l < "$TIMINGS" 2>/dev/null || echo 0)" -gt 200 ]; then
    tail -100 "$TIMINGS" > "${TIMINGS}.new" && mv -f "${TIMINGS}.new" "$TIMINGS"
  fi
fi
# THIS RUN'S OWN ROW, JUDGED (codex, #1962). `lanebudget` above reads the
# table as it stood when the lane started -- the previous run's row -- so a push
# that made a gate slower passed on the row before it, and its own row reached
# only the next run, which could be a landing, and a landing reports without
# refusing. Judged again here, once the row is written: the push that makes the
# lane slower is the push that is told.
if [ "$_lane" = fast ] && hoort_hier lanebudget; then
  printf '=== %-9s' lanebudnow
  if python3 scripts/ci/the_fast_lane_stays_under_its_budget.py --lane fast \
       > "${LOGDIR}/lanebudnow.log" 2>&1; then echo " PASS"; pass=$((pass+1))
  else echo " FAIL — see ${LOGDIR}/lanebudnow.log"; tail -15 "${LOGDIR}/lanebudnow.log" | sed 's/^/    /'
       fail=$((fail+1)); gevallen="$gevallen lanebudnow"; fi
fi
# A WORKFLOW NOBODY HAS RUN, ON THE LAST LINES INSTEAD OF THE FIRST (#510).
#
# `groen` is advisory and prints where it runs, a few hundred lines above this.
# A workflow that has not run since its file changed is not a failure of the
# branch in front of you -- but it is how bindings.yml, the only thing that
# compiled the Python binding, went 126 days without running while four
# duplicated functions sat on master. Repeated here because the tail of this
# output is the part that gets read.
#
# AND THE ONES THAT RUN AND NEVER PASS (#516). SILENT was the only class
# repeated here, and the louder failure was the quieter line: publication-guards
# .yml -- the guards for the published tree and the licence texts -- ran on
# every push for two days, 38 times, and went green on none of them, while this
# lane printed that fact several hundred lines up and nothing carried it down.
if [ -f "${LOGDIR}/groen.log" ] && grep -qE "SILENT|NEVER GREEN" "${LOGDIR}/groen.log" 2>/dev/null; then
  echo "--- workflows nothing has run, or that never went green, since their file changed (advisory; from groen) ---"
  grep -E "SILENT|NEVER GREEN" "${LOGDIR}/groen.log" | sed 's/^/  /'
fi
_uit=""; [ "$uitgesteld" -gt 0 ] && _uit=", $uitgesteld deferred to the landing lane"
# Named separately from the deferrals, and never folded into the pass count: a
# gate that was skipped because its crate is not in this landing has judged
# nothing, and the crates it would have judged are named on the line above.
[ "$ongeraakt" -gt 0 ] && _uit="$_uit, $ongeraakt skipped (crate not touched)"
# Left to the other half of a split lane, and named as such. A run that reported
# only its own passes would look complete while five gates had been judged by
# nobody at all.
[ "$elders" -gt 0 ] && _uit="$_uit, $elders left to the operator's machine"
[ "$PLAATS" = machine ] && _uit="$_uit; only the gates that need this machine"
[ "$LANDING" = 1 ] && _uit="$_uit; scope: $SELECTION"
if [ "$fail" = 0 ]; then echo "LOCAL_CI_GATE: PASS ($pass gates, $_lane lane$_uit)"; exit 0
else
  # The names again, at the bottom. They were printed when each one failed, but a
  # run prints hundreds of lines and the caller often shows only the tail -- so
  # the one thing the reader needs was the most likely thing to be cut off.
  echo "LOCAL_CI_GATE: FAIL ($fail of $((pass+fail)) gates failed, $_lane lane$_uit)"
  echo "failed: ${gevallen# }"
  # THE LAST LINES SAY WHAT FAILED (owner, 29-09-2026): each gate's first error
  # lines, and `FAILED GATES:` as the very last line, which remote_gate.sh and
  # ff3nr.sh pass on unchanged (scripts/ci/_failed_gates_block.sh).
  # shellcheck disable=SC2086
  failed_gates_block "$LOGDIR" ${gevallen# }
  exit 1
fi
