#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Does the machine-wide gate lock actually serialise gates, and does it let go
# when the gate holding it is killed? (#359)
#
# Two dummy gates, not a mock: each one takes the lock the way
# local_ci_gate.sh takes it -- acquire with its own pid, release from an EXIT
# trap, TERM turned into an exit so the trap runs. What is asserted is
# timestamps: the second gate got the lock only after the first gave it back.
#
# A lock is the one kind of code whose failure is invisible in the ordinary
# case. Two gates that never collide look exactly like two gates a lock kept
# apart, and the day they do collide the evidence is a load average nobody
# recorded. So the cases here are the ones that fail silently in production: a
# holder that is killed, a holder that is killed uncatchably, a landing that
# must not be pushed aside, and the wiring in local_ci_gate.sh itself.
#
# Cases 15 to 28 are #452, and 29 to 30 are #464: the priority rule and its three bounds, then the two
# ways a waiter could sit in this queue and never be served from it -- a line
# belonging to a different lock of the same name, and a helper still queueing
# for a gate that had already died. Both of those fail by WAITING, which is
# indistinguishable from working, so both cases assert that somebody got the
# lock rather than that nothing went wrong.
#
# FLOOR: cases run >= 30 -- a run that executes fewer stopped short rather than
# found everything in order.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HELPER="${ROOT}/scripts/ci/_one_gate_at_a_time.sh"
GATE="${ROOT}/scripts/ci/local_ci_gate.sh"
WRAP="${ROOT}/scripts/ci/with_gate_lock.sh"
FLOOR=30
failures=0
cases=0

fail()  { echo "  FAIL: $*" >&2; failures=$((failures + 1)); }
case_() { cases=$((cases + 1)); }
now()   { python3 -c 'import time; print("%.3f" % time.time())'; }
# bc is not everywhere and awk is; both operands are seconds with milliseconds.
ge()    { awk -v a="$1" -v b="$2" 'BEGIN{exit !(a >= b)}'; }

if [ ! -f "$HELPER" ]; then
  echo "SKIPPED (not a pass): ${HELPER} is gone; nothing to test." >&2
  exit 0
fi

WORK="$(mktemp -d)"
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "${WORK}"' EXIT

# THE CALLER'S ENVIRONMENT DOES NOT GET A VOTE (#425).
#
# XFA_GATE_LOCK=0 turns the lock off, and it is a legitimate thing for somebody
# to set -- a measurement run, a machine where the queue is in the way. Inherited
# here it turns every case below into a test of nothing: acquire returns 0
# immediately, no lock is ever made, and fourteen cases fail at once with
# messages about a lock that was never asked to exist. That happened on
# 07-09-2026 during the profiling run for this ticket, and for a few minutes it
# read like a broken lock rather than a leaked variable.
#
# So the suite sets its own, over whatever it was handed. It already does this
# for the lock directory and the queue; the switch belongs in the same list.
export XFA_GATE_LOCK=1
export XFA_GATE_LOCK_DIR="${WORK}/xfa-gate.lock"
export XFA_GATE_QUEUE="${WORK}/xfa-gate.queue"
export XFA_GATE_LOCK_POLL=0.2
export XFA_GATE_LOCK_WAIT=30
export XFA_GATE_LOCK_MAX_AGE=7200

# A DUMMY GATE: the acquire/trap/release shape of local_ci_gate.sh and nothing
# else. It holds in tenth-of-a-second steps rather than one long sleep, because
# bash runs a trap only once the foreground command returns -- a gate that slept
# for its whole hold would answer a TERM a minute later and the test would be
# measuring `sleep`, not the trap.
cat > "${WORK}/dummy_gate.sh" <<'EOS'
#!/usr/bin/env bash
set -uo pipefail
HELPER="$1"; LANE="$2"; HOLD_TENTHS="$3"; TAG="$4"
stamp() { python3 -c 'import time; print("%.3f" % time.time())' > "$1"; }
release() { bash "$HELPER" --release "$$" >/dev/null 2>&1; return 0; }
trap 'release; stamp "${TAG}.released"' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
stamp "${TAG}.started"
bash "$HELPER" --acquire "$LANE" "$$" >/dev/null 2>&1 || { stamp "${TAG}.timeout"; exit 5; }
stamp "${TAG}.got"
i=0
while [ "$i" -lt "$HOLD_TENTHS" ]; do sleep 0.1; i=$((i + 1)); done
exit 0
EOS
chmod +x "${WORK}/dummy_gate.sh"

# A WAITER FROM BEFORE #452, reduced to the one behaviour that has to keep
# working while both versions of the helper run side by side: it appends a
# three-field line and takes the lock only when its line is literally the first
# in the file. Not a mock of anything -- that is what the released helper does,
# and during a landing of this file it is what half the machine is running.
cat > "${WORK}/waiter_that_acts_on_the_head.sh" <<'EOS'
#!/usr/bin/env bash
set -uo pipefail
TAG="$1"; LANE="$2"
Q="$XFA_GATE_QUEUE"; QL="${Q}.lock"; L="$XFA_GATE_LOCK_DIR"
# The queue lock, taken the way the released helper takes it: a bare mkdir,
# broken once it is a minute old. Appending without it would race the rewriters
# and drop this line -- which is a defect in a test, not in a queue.
q_lock() {
  local i
  for i in $(seq 1 600); do
    mkdir "$QL" 2>/dev/null && return 0
    [ -d "$QL" ] && [ -z "$(find "$QL" -maxdepth 0 -mmin -1 2>/dev/null)" ] && rm -rf "$QL"
    sleep 0.1
  done
  return 1
}
q_unlock() { rm -rf "$QL" 2>/dev/null; return 0; }
q_lock || exit 2
printf '%s %s %s\n' "$$" "$LANE" "$(date +%s)" >> "$Q"
q_unlock
i=0
while [ "$i" -lt 250 ]; do
  q_lock || exit 2
  head_pid="$(head -n 1 "$Q" 2>/dev/null | cut -d' ' -f1)"
  if [ "${head_pid:-}" = "$$" ] && mkdir "$L" 2>/dev/null; then
    printf '%s\n' "$$" > "${L}/pid"
    printf '%s\n' "$LANE" > "${L}/lane"
    date +%s > "${L}/since"
    q_unlock
    python3 -c 'import time; print("%.3f" % time.time())' > "${TAG}.got"
    rm -rf "$L"
    exit 0
  fi
  q_unlock
  sleep 0.1; i=$((i + 1))
done
exit 5
EOS
chmod +x "${WORK}/waiter_that_acts_on_the_head.sh"

# THE PID COMES BACK IN A VARIABLE, NOT ON STDOUT.
#
# Capturing it from a command substitution looks tidier and hangs: the
# background gate inherits the substitution's pipe, so the substitution does not
# return until the gate it just started has exited -- the exact opposite of what
# a background start is for. Starting it in this shell also makes it a real
# child, so `wait` and `kill` mean what they say.
GATE_PID=0
start_gate() { # lane hold_tenths tag -> sets GATE_PID
  bash "${WORK}/dummy_gate.sh" "$HELPER" "$1" "$2" "${WORK}/$3" >/dev/null 2>&1 &
  GATE_PID=$!
}
wait_for() { # file timeout_tenths
  local i=0
  while [ "$i" -lt "$2" ]; do [ -f "$1" ] && return 0; sleep 0.1; i=$((i + 1)); done
  return 1
}
# ARRIVAL ORDER IS WHAT CASE 6 ASSERTS, so arrival has to be observed rather
# than assumed. Sleeping a fixed moment between two starts is a guess about how
# long a shell takes to reach the queue, and on the loaded machine this ticket
# is about it is the wrong guess: at load 100 a dummy gate needs more than half
# a second to append its line, the next gate arrives first, and the test blames
# the lock for the order the test itself produced.
wait_in_queue() { # pid timeout_tenths
  local i=0
  while [ "$i" -lt "$2" ]; do
    grep -q "^$1 " "${XFA_GATE_QUEUE}" 2>/dev/null && return 0
    sleep 0.1; i=$((i + 1))
  done
  return 1
}
reset_lock() { rm -rf "${XFA_GATE_LOCK_DIR}" "${XFA_GATE_QUEUE}" "${XFA_GATE_QUEUE}.lock" "${XFA_GATE_QUEUE}.lock.pid"; }

# 1. THE SECOND GATE WAITS FOR THE FIRST.
# The one property the whole ticket is about. Without the lock the second gate
# takes off immediately and `got` lands before the first one's `released`.
case_
reset_lock
start_gate fast 25 a1; A=$GATE_PID
if ! wait_for "${WORK}/a1.got" 100; then fail "1: the first gate never took the lock"; else
  start_gate fast 1 b1; B=$GATE_PID
  wait_for "${WORK}/b1.got" 150 || fail "1: the second gate never got the lock"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null
  if [ -f "${WORK}/b1.got" ] && [ -f "${WORK}/a1.released" ]; then
    ge "$(cat "${WORK}/b1.got")" "$(cat "${WORK}/a1.released")" \
      || fail "1: the second gate held the lock while the first still had it"
    ge "$(awk -v g="$(cat "${WORK}/b1.got")" -v s="$(cat "${WORK}/b1.started")" 'BEGIN{print g-s}')" 1.0 \
      || fail "1: the second gate did not wait -- it was let straight through"
  fi
fi

# 2. KILLING THE FIRST RELEASES THE LOCK.
# A gate killed by a terminal is not exotic here: it is how the four-hour run on
# 07-09-2026 ended. If TERM did not reach the trap, the lock would stand until
# somebody noticed the pid was gone.
case_
reset_lock
start_gate fast 600 a2; A=$GATE_PID
if ! wait_for "${WORK}/a2.got" 100; then fail "2: the first gate never took the lock"; else
  start_gate fast 1 b2; B=$GATE_PID
  sleep 0.5
  [ -f "${WORK}/b2.got" ] && fail "2: the second gate got the lock while the first was alive"
  kill "$A" 2>/dev/null
  wait_for "${WORK}/b2.got" 60 || fail "2: killing the holder did not release the lock"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null
fi

# 3. AND WHEN THE TRAP CANNOT RUN AT ALL.
# SIGKILL runs nothing. The lock directory and the queue line survive their
# owner, and the next waiter has to notice the pid is gone rather than wait out
# the full four hours behind a process that no longer exists.
case_
reset_lock
start_gate fast 600 a3; A=$GATE_PID
if ! wait_for "${WORK}/a3.got" 100; then fail "3: the first gate never took the lock"; else
  kill -9 "$A" 2>/dev/null
  wait "$A" 2>/dev/null
  start_gate fast 1 b3; B=$GATE_PID
  wait_for "${WORK}/b3.got" 100 || fail "3: a hard-killed holder left the lock standing"
  wait "$B" 2>/dev/null
fi

# 4. A LANDING HOLDER IS NEVER PREEMPTED.
# MAX_AGE=0 makes every holder instantly "stuck" as far as the waiter is
# concerned. The landing holder must still be there afterwards: breaking it
# would put a second compiling gate beside a landing, which is the failure this
# file exists to prevent.
case_
reset_lock
start_gate landing 40 a4; A=$GATE_PID
if ! wait_for "${WORK}/a4.got" 100; then fail "4: the landing gate never took the lock"; else
  XFA_GATE_LOCK_MAX_AGE=0 bash "${WORK}/dummy_gate.sh" "$HELPER" fast 1 "${WORK}/b4" >/dev/null 2>&1 &
  B=$!
  sleep 1.5
  [ -f "${WORK}/b4.got" ] && fail "4: a landing holder was preempted"
  [ "$(cat "${XFA_GATE_LOCK_DIR}/pid" 2>/dev/null || echo none)" = "$A" ] \
    || fail "4: the landing holder lost the lock while it was still running"
  wait_for "${WORK}/b4.got" 150 || fail "4: the waiter never got the lock after the landing finished"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null
fi

# 5. A STUCK FAST HOLDER IS PREEMPTED.
# The other half of the same rule, and the reason the lock cannot deadlock the
# machine: a fast lane is minutes, so one that has held for hours is not coming
# back on its own.
case_
reset_lock
start_gate fast 600 a5; A=$GATE_PID
if ! wait_for "${WORK}/a5.got" 100; then fail "5: the first gate never took the lock"; else
  XFA_GATE_LOCK_MAX_AGE=0 bash "${WORK}/dummy_gate.sh" "$HELPER" fast 1 "${WORK}/b5" >/dev/null 2>&1 &
  B=$!
  wait_for "${WORK}/b5.got" 60 || fail "5: a fast holder past the age limit was not preempted"
  kill "$A" 2>/dev/null; wait "$A" 2>/dev/null; wait "$B" 2>/dev/null
fi

# 6. ARRIVAL ORDER IS SERVICE ORDER.
# The reason for a queue file rather than a scramble of mkdir calls: with five
# terminals, an unlucky one loses every race. That is what a three-hour wait
# behind a lock nobody held for three hours looks like.
case_
reset_lock
start_gate fast 60 q1; A=$GATE_PID
if ! wait_for "${WORK}/q1.got" 100; then fail "6: the first gate never took the lock"; else
  start_gate fast 3 q2; B=$GATE_PID; wait_in_queue "$B" 100 || fail "6: the second gate never reached the queue"
  start_gate fast 3 q3; C=$GATE_PID; wait_in_queue "$C" 100 || fail "6: the third gate never reached the queue"
  start_gate fast 3 q4; D=$GATE_PID; wait_in_queue "$D" 100 || fail "6: the fourth gate never reached the queue"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null; wait "$C" 2>/dev/null; wait "$D" 2>/dev/null
  if [ -f "${WORK}/q2.got" ] && [ -f "${WORK}/q3.got" ] && [ -f "${WORK}/q4.got" ]; then
    ge "$(cat "${WORK}/q3.got")" "$(cat "${WORK}/q2.got")" || fail "6: the third gate went before the second"
    ge "$(cat "${WORK}/q4.got")" "$(cat "${WORK}/q3.got")" || fail "6: the fourth gate went before the third"
  else
    fail "6: not every queued gate got the lock"
  fi
fi

# 7. CARGO IS BOUNDED, AND BOUNDED BELOW THE CORE COUNT.
# A job count equal to the cores is the state the ticket was written about: the
# machine has no headroom left for the person using it.
case_
J="$(bash "$HELPER" --jobs)"
case "$J" in ''|*[!0-9]*) fail "7: --jobs printed '$J', which is not a number" ;;
  *) [ "$J" -ge 1 ] || fail "7: --jobs printed $J; a gate that compiles with zero jobs compiles nothing"
     PHYS="$(sysctl -n hw.physicalcpu 2>/dev/null || nproc 2>/dev/null || echo 0)"
     case "$PHYS" in ''|*[!0-9]*) PHYS=0 ;; esac
     if [ "$PHYS" -gt 3 ]; then
       [ "$J" -eq $((PHYS - 2)) ] || fail "7: --jobs printed $J on $PHYS physical cores; expected $((PHYS - 2))"
     fi ;;
esac

# 8. THE GATE ITSELF TAKES THE LOCK.
# The five cases above test the mechanism. This one tests that the mechanism is
# wired to the thing it was built for -- run the real gate against a lock that
# is already held and it must refuse with exit 5 rather than start compiling
# beside the holder. A gate lock nobody calls is the same as no gate lock.
case_
reset_lock
mkdir -p "${XFA_GATE_LOCK_DIR}"
echo $$ > "${XFA_GATE_LOCK_DIR}/pid"
echo landing > "${XFA_GATE_LOCK_DIR}/lane"
python3 -c 'import time; print(int(time.time()))' > "${XFA_GATE_LOCK_DIR}/since"
if [ ! -f "$GATE" ]; then fail "8: ${GATE} is gone"; else
  # ON A WATCHDOG, because the failure mode here is not a wrong exit code -- it
  # is a gate that sails past the lock and starts the fast lane, ninety-odd
  # guards and a compile. Waiting for that to finish would turn a broken wiring
  # into a twenty-minute test run, which is how a suite stops being run.
  XFA_GATE_LOCK_WAIT=3 bash "$GATE" --fast > "${WORK}/gate.out" 2>&1 &
  G=$!
  i=0
  while kill -0 "$G" 2>/dev/null && [ "$i" -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
  if kill -0 "$G" 2>/dev/null; then
    kill -9 "$G" 2>/dev/null
    fail "8: local_ci_gate.sh went on running behind a held lock -- it did not take it"
  else
    wait "$G"; rc=$?
    [ "$rc" -eq 5 ] || fail "8: local_ci_gate.sh exited $rc behind a held lock; expected 5"
    grep -q 'NOTHING WAS CHECKED' "${WORK}/gate.out" \
      || fail "8: the refusal does not say the run was never judged"
  fi
fi
reset_lock

# 9. AND IT BOUNDS CARGO.
# Structural, because proving it by running the compiling gates costs the half
# hour this ticket is trying to give back. The line that must not quietly go
# away is the one that turns --jobs into the environment every run_*.sh reads.
case_
if ! grep -q 'CARGO_BUILD_JOBS="\$(bash scripts/ci/_one_gate_at_a_time.sh --jobs)"' "$GATE"; then
  fail "9: local_ci_gate.sh no longer sets CARGO_BUILD_JOBS from --jobs"
fi
grep -q 'export CARGO_BUILD_JOBS' "$GATE" \
  || fail "9: CARGO_BUILD_JOBS is set but not exported, so run_build.sh and run_test.sh never see it"

# 10. THE SHARED WRAPPER RUNS THE COMMAND AND HANDS BACK ITS STATUS.
# with_gate_lock.sh exists so that a caller which is not this repository's gate
# -- the editor session, on 07-09 -- waits in the same line instead of spinning
# on mkdir beside it. A wrapper that swallowed the command's exit status would
# turn every failure it wraps into a pass.
case_
reset_lock
if [ ! -f "$WRAP" ]; then fail "10: ${WRAP} is gone"; else
  bash "$WRAP" -- sh -c 'echo ran > "$0"' "${WORK}/wrap.ran" >"${WORK}/wrap.out" 2>&1
  rc=$?
  [ "$rc" -eq 0 ] || fail "10: the wrapper exited $rc for a command that succeeded"
  [ -f "${WORK}/wrap.ran" ] || fail "10: the wrapper never ran the command"
  bash "$WRAP" -- sh -c 'exit 42' >/dev/null 2>&1
  rc=$?
  [ "$rc" -eq 42 ] || fail "10: the wrapper exited $rc for a command that exited 42"
  [ -d "${XFA_GATE_LOCK_DIR}" ] && fail "10: the wrapper kept the lock after its command finished"
fi
reset_lock

# 11. AND IT WAITS IN THE SAME QUEUE AS A GATE.
# The failure this whole file is about: two implementations of one lock, so a
# waiter that is not in the queue is not in the line. A dummy gate holds; the
# wrapper must not run its command until that gate lets go.
case_
if [ ! -f "$WRAP" ]; then fail "11: ${WRAP} is gone"; else
  start_gate landing 12 A; H=$GATE_PID
  wait_for "${WORK}/A.got" 50 || fail "11: the holding gate never got the lock"
  bash "$WRAP" -- sh -c 'python3 -c "import time; print(\"%.3f\" % time.time())" > "$0"' \
       "${WORK}/wrap.when" >/dev/null 2>&1
  rc=$?
  [ "$rc" -eq 0 ] || fail "11: the wrapper exited $rc behind a holder it should have outwaited"
  if [ -f "${WORK}/wrap.when" ] && [ -f "${WORK}/A.released" ]; then
    ge "$(cat "${WORK}/wrap.when")" "$(cat "${WORK}/A.released")" \
      || fail "11: the wrapper ran its command while a gate still held the lock"
  else
    fail "11: no timestamps -- the wrapper or the gate did not finish"
  fi
  wait "$H" 2>/dev/null
fi
reset_lock

# 12. A KILLED WRAPPER LETS GO.
# Same reason the gate turns TERM into an exit: a shell killed by an untrapped
# TERM runs no EXIT trap, and the lock would stand until someone noticed the pid
# was gone. Terminals do kill long-running commands.
case_
if [ ! -f "$WRAP" ]; then fail "12: ${WRAP} is gone"; else
  bash "$WRAP" -- sh -c 'i=0; while [ $i -lt 200 ]; do sleep 0.1; i=$((i+1)); done' >/dev/null 2>&1 &
  W=$!
  wait_for "${XFA_GATE_LOCK_DIR}/pid" 50 || fail "12: the wrapper never took the lock"
  kill -TERM "$W" 2>/dev/null
  wait "$W" 2>/dev/null
  # WHAT MUST BE GONE IS THIS WRAPPER'S HOLD, not the directory: on a busy
  # machine the next waiter can take the lock in the same breath, and a test
  # that only asks whether the directory exists would call that a leak. Polled
  # rather than read once, because the release is a `bash` of its own and under
  # load it is not instant -- reading immediately measures the machine.
  i=0
  while [ "$i" -lt 300 ]; do
    h="$(cat "${XFA_GATE_LOCK_DIR}/pid" 2>/dev/null || true)"
    [ -d "${XFA_GATE_LOCK_DIR}" ] || break
    [ -n "$h" ] && [ "$h" != "$W" ] && break
    sleep 0.1; i=$((i + 1))
  done
  h="$(cat "${XFA_GATE_LOCK_DIR}/pid" 2>/dev/null || true)"
  if [ -d "${XFA_GATE_LOCK_DIR}" ] && { [ -z "$h" ] || [ "$h" = "$W" ]; }; then
    fail "12: a killed wrapper left the lock standing (holder '${h:-none}', wrapper $W)"
  fi
fi
reset_lock

# 13. AND WHEN IT GIVES UP, THE COMMAND DID NOT RUN.
# Exit 5 is the gate's "no verdict" code and means the same here. A wrapper that
# gave up and ran the command anyway would put a second build beside the holder,
# which is the failure being fixed.
case_
if [ ! -f "$WRAP" ]; then fail "13: ${WRAP} is gone"; else
  mkdir -p "${XFA_GATE_LOCK_DIR}"
  echo $$ > "${XFA_GATE_LOCK_DIR}/pid"
  echo landing > "${XFA_GATE_LOCK_DIR}/lane"
  python3 -c 'import time; print(int(time.time()))' > "${XFA_GATE_LOCK_DIR}/since"
  rm -f "${WORK}/never.ran"
  XFA_GATE_LOCK_WAIT=2 bash "$WRAP" -- sh -c 'echo ran > "$0"' "${WORK}/never.ran" >/dev/null 2>&1
  rc=$?
  [ "$rc" -eq 5 ] || fail "13: the wrapper exited $rc after giving up; expected 5"
  [ -f "${WORK}/never.ran" ] && fail "13: the wrapper ran the command after giving up waiting"
fi
reset_lock

# 14. AND A WRAPPER KILLED WHILE IT IS STILL WAITING LEAVES NOTHING BEHIND.
# The window case 12 hit by luck and this one closes on purpose: a wrapper spends
# nearly all of its life queued, not holding, and the first shape of
# with_gate_lock.sh armed its INT and TERM handlers only AFTER the acquire
# returned. A TERM in that window killed the shell untrapped -- no EXIT trap, no
# release -- and left its line in the queue, so every later waiter sat behind a
# pid that was gone. Killing a waiting gate is ordinary; that is how the
# four-hour run of 07-09 ended.
case_
reset_lock
if [ ! -f "$WRAP" ]; then fail "14: ${WRAP} is gone"; else
  start_gate landing 200 hold; H=$GATE_PID
  wait_for "${WORK}/hold.got" 100 || fail "14: the holding gate never took the lock"
  rm -f "${WORK}/queued.ran"
  bash "$WRAP" -- sh -c 'echo ran > "$0"' "${WORK}/queued.ran" >/dev/null 2>&1 &
  W2=$!
  if wait_in_queue "$W2" 100; then
    kill -TERM "$W2" 2>/dev/null
    wait "$W2" 2>/dev/null
    i=0
    while grep -q "^$W2 " "${XFA_GATE_QUEUE}" 2>/dev/null && [ "$i" -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
    grep -q "^$W2 " "${XFA_GATE_QUEUE}" 2>/dev/null \
      && fail "14: a wrapper killed while waiting left its line in the queue, so every later waiter queues behind a dead pid"
    [ "$(cat "${XFA_GATE_LOCK_DIR}/pid" 2>/dev/null)" = "$W2" ] \
      && fail "14: a wrapper killed while waiting ended up holding the lock"
  else
    kill -TERM "$W2" 2>/dev/null; wait "$W2" 2>/dev/null
    fail "14: the wrapper never reached the queue"
  fi
  [ -f "${WORK}/queued.ran" ] && fail "14: the killed wrapper ran its command anyway"
  kill "$H" 2>/dev/null; wait "$H" 2>/dev/null

  # AND THE ORDER, STRUCTURALLY, because the behaviour above cleans itself up:
  # the helper the dead wrapper left behind goes on polling, and its own prune
  # drops the line of a pid that is gone. So the queue recovers even from the
  # broken shape, and only the narrow case -- a TERM after the helper has made
  # the lock but before the parent has come back from it -- leaves a lock
  # standing. That window is real (it is what failed the landing gate on
  # 07-09-2026) and it is not reliably reachable from a test, so what is
  # asserted here is the property that closes it: the handlers are armed BEFORE
  # the wait, and the release does not ask a flag for permission.
  _t_int="$(grep -n "^trap 'kill_the_command; exit 130' INT" "$WRAP" | cut -d: -f1)"
  _t_term="$(grep -n "^trap 'kill_the_command; exit 143' TERM" "$WRAP" | cut -d: -f1)"
  _t_acq="$(grep -n '^bash "\$LOCKER" --acquire' "$WRAP" | cut -d: -f1)"
  if [ -z "$_t_int" ] || [ -z "$_t_term" ]; then
    fail "14: with_gate_lock.sh no longer traps INT and TERM; a killed waiter would die untrapped"
  elif [ -z "$_t_acq" ]; then
    fail "14: with_gate_lock.sh no longer acquires the lock on a line this test can find"
  else
    [ "$_t_int" -lt "$_t_acq" ] && [ "$_t_term" -lt "$_t_acq" ] \
      || fail "14: the INT/TERM traps are armed after the acquire (lines $_t_int/$_t_term vs $_t_acq); a wrapper killed while waiting dies untrapped and leaves the lock"
  fi
  grep -q 'HELD' "$WRAP" \
    && fail "14: the release is guarded by a held-flag again; the flag is set one command after the lock exists, and a signal in between leaves a lock nobody hands back"
fi
reset_lock

# 15. A WAITING LANDING GOES BEFORE FAST WAITERS THAT ARRIVED EARLIER (#452).
# The queue was fair to processes and unfair to the product: with eleven
# terminals pushing, master stood still for two hours twice while a landing sat
# behind three to six pushes. A push exists to prepare a landing.
case_
reset_lock
# The holder holds until all three waiters are queued and is then killed, as in
# case 2. A fixed 2.5 s hold ran out on a loaded landing machine (06-10-2026)
# before the landing had queued, so the first push took the lock legitimately
# and the test blamed the queue for the order the test itself produced.
start_gate fast 600 p1; A=$GATE_PID
if ! wait_for "${WORK}/p1.got" 100; then fail "15: the holder never took the lock"; else
  start_gate fast 3 p2; B=$GATE_PID; wait_in_queue "$B" 100 || fail "15: the first push never reached the queue"
  start_gate fast 3 p3; C=$GATE_PID; wait_in_queue "$C" 100 || fail "15: the second push never reached the queue"
  start_gate landing 3 p4; D=$GATE_PID; wait_in_queue "$D" 100 || fail "15: the landing never reached the queue"
  kill "$A" 2>/dev/null
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null; wait "$C" 2>/dev/null; wait "$D" 2>/dev/null
  if [ -f "${WORK}/p2.got" ] && [ -f "${WORK}/p3.got" ] && [ -f "${WORK}/p4.got" ]; then
    ge "$(cat "${WORK}/p2.got")" "$(cat "${WORK}/p4.got")" \
      || fail "15: a push that arrived earlier went before the waiting landing"
    ge "$(cat "${WORK}/p3.got")" "$(cat "${WORK}/p4.got")" \
      || fail "15: a push that arrived earlier went before the waiting landing"
  else
    fail "15: not every waiter got the lock"
  fi
fi

# 16. AND A PUSH IS NOT STARVED BY LANDINGS.
# "Landings first" with no bound is "pushes never", which on an evening of
# landings is a terminal that can no longer prepare one. A fast waiter that has
# been passed XFA_GATE_MAX_PASSES times goes next whatever is behind it. The
# cap is set to one here so the case costs one handover instead of three.
case_
reset_lock
export XFA_GATE_MAX_PASSES=1
start_gate fast 25 s1; A=$GATE_PID
if ! wait_for "${WORK}/s1.got" 100; then fail "16: the holder never took the lock"; else
  start_gate fast 3 s2;    B=$GATE_PID; wait_in_queue "$B" 100 || fail "16: the push never reached the queue"
  start_gate landing 3 s3; C=$GATE_PID; wait_in_queue "$C" 100 || fail "16: the first landing never reached the queue"
  start_gate landing 3 s4; D=$GATE_PID; wait_in_queue "$D" 100 || fail "16: the second landing never reached the queue"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null; wait "$C" 2>/dev/null; wait "$D" 2>/dev/null
  if [ -f "${WORK}/s2.got" ] && [ -f "${WORK}/s3.got" ] && [ -f "${WORK}/s4.got" ]; then
    ge "$(cat "${WORK}/s2.got")" "$(cat "${WORK}/s3.got")" \
      || fail "16: the first landing did not pass the push at all -- case 15 is passing for the wrong reason"
    ge "$(cat "${WORK}/s4.got")" "$(cat "${WORK}/s2.got")" \
      || fail "16: a push passed once with the cap at one was passed again; landings can starve pushes"
  else
    fail "16: not every waiter got the lock"
  fi
fi
unset XFA_GATE_MAX_PASSES

# 17. A DECLARED SHORT PUSH IS NOT PASSED AT ALL (#452, editor COS).
# The editor's two gate steps are four to six minutes on a warm target and were
# sitting two hours behind, near its own six-hour lock timeout. A landing that
# waits five minutes for them has lost almost nothing; case 15 is what happens
# to a push that declares nothing.
case_
reset_lock
start_gate fast 25 e1; A=$GATE_PID
if ! wait_for "${WORK}/e1.got" 100; then fail "17: the holder never took the lock"; else
  XFA_GATE_DECLARED_SECONDS=60 bash "${WORK}/dummy_gate.sh" "$HELPER" fast 3 "${WORK}/e2" >/dev/null 2>&1 &
  B=$!; wait_in_queue "$B" 100 || fail "17: the declared push never reached the queue"
  start_gate landing 3 e3; C=$GATE_PID; wait_in_queue "$C" 100 || fail "17: the landing never reached the queue"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null; wait "$C" 2>/dev/null
  if [ -f "${WORK}/e2.got" ] && [ -f "${WORK}/e3.got" ]; then
    ge "$(cat "${WORK}/e3.got")" "$(cat "${WORK}/e2.got")" \
      || fail "17: a landing passed a push that had declared sixty seconds"
  else
    fail "17: not every waiter got the lock"
  fi
fi

# 18. AND THE DECLARATION IS ENFORCED AGAINST THE ONE WHO MADE IT.
# Otherwise "I will be five minutes" is a free pass with nothing behind it. A
# fast holder that declared D is broken at D plus the slack, where a silent one
# would have had XFA_GATE_LOCK_MAX_AGE -- two hours, untouched here, so the
# break can only come from the declaration.
case_
reset_lock
XFA_GATE_DECLARED_SECONDS=1 bash "${WORK}/dummy_gate.sh" "$HELPER" fast 600 "${WORK}/d1" >/dev/null 2>&1 &
A=$!
if ! wait_for "${WORK}/d1.got" 100; then fail "18: the declaring holder never took the lock"; else
  XFA_GATE_DECL_SLACK=0 bash "${WORK}/dummy_gate.sh" "$HELPER" fast 1 "${WORK}/d2" >/dev/null 2>&1 &
  B=$!
  wait_for "${WORK}/d2.got" 100 \
    || fail "18: a fast holder that promised one second and held forever was never broken"
  kill "$A" 2>/dev/null; wait "$A" 2>/dev/null; wait "$B" 2>/dev/null
fi

# 19. A QUEUE LINE SAYS WHICH LOCK IT IS QUEUEING FOR.
# There is more than one FIFO on this machine running this same script, and in
# `ps` they are indistinguishable: same helper, same `--acquire landing`, and
# the lock directory only in an environment variable. A line belonging to
# another lock must never become this one's turn, and must survive being read
# by this one -- on 08-09 a hand repair moved one queue's waiters into the
# other's file, the head was a pid that could not take this gate, and the gate
# stood empty for twenty minutes with ten waiters behind it.
case_
reset_lock
printf '%s landing %s 0 0 %s\n' 999001 1 /tmp/some-other.lock > "${XFA_GATE_QUEUE}"
start_gate fast 1 f1; A=$GATE_PID
wait_for "${WORK}/f1.got" 100 \
  || fail "19: a line belonging to another lock became the head of this queue and nobody could take the gate"
wait "$A" 2>/dev/null
grep -q '^999001 ' "${XFA_GATE_QUEUE}" \
  || fail "19: another lock's queue line was eaten by this lock's waiters"
reset_lock

# 20. NOBODY WAITS ON BEHALF OF A CORPSE.
# The helper is a child of the gate it takes the lock for. An orphaned one --
# gate dead, ppid 1 -- was still holding a place in the queue and would have
# taken the lock for a process that could not use it. One such wrapper was
# found on 08-09 among the ten waiters.
case_
reset_lock
bash -c 'exit 0' & DEAD=$!; wait "$DEAD" 2>/dev/null
bash "$HELPER" --acquire fast "$DEAD" > "${WORK}/corpse.out" 2>&1
rc=$?
[ "$rc" -eq 6 ] || fail "20: acquiring for a dead gate returned ${rc}, not 6"
grep -q "^$DEAD " "${XFA_GATE_QUEUE}" 2>/dev/null \
  && fail "20: the dead gate's line was left in the queue"
[ -d "${XFA_GATE_LOCK_DIR}" ] \
  && fail "20: the lock was taken for a process that no longer exists"
reset_lock

# 21. THE WRAPPER'S DECLARATION IS PARSED OR REFUSED, NEVER DROPPED.
# `--declare` is what the editor session sets to buy the exemption in case 17,
# and it is a promise made from outside this repository. A form this script
# does not understand has to be an error: reading `5min` as no declaration at
# all would leave the caller believing it had bought something, and the symptom
# -- being passed by landings -- is the thing it was buying protection from.
case_
if [ ! -f "$WRAP" ]; then fail "21: ${WRAP} is gone"; else
  _d_out="$(bash "$WRAP" --declare 5m -- printenv XFA_GATE_DECLARED_SECONDS 2>/dev/null)"
  [ "$_d_out" = 300 ] \
    || fail "21: --declare 5m handed the helper '${_d_out}' instead of 300 seconds"
  _d_out="$(bash "$WRAP" --declare 90 -- printenv XFA_GATE_DECLARED_SECONDS 2>/dev/null)"
  [ "$_d_out" = 90 ] \
    || fail "21: --declare 90 handed the helper '${_d_out}' instead of 90 seconds"
  bash "$WRAP" --declare 5min -- true >/dev/null 2>&1
  [ "$?" -eq 2 ] \
    || fail "21: --declare 5min was accepted; a declaration that does not parse must be refused, not dropped"
  _d_out="$(bash "$WRAP" -- printenv XFA_GATE_DECLARED_SECONDS 2>/dev/null; echo "rc=$?")"
  case "$_d_out" in *rc=1*) ;; *) fail "21: a wrapper with no --declare still declared something ('${_d_out}')" ;; esac
fi
reset_lock

# 22. THE CHOICE IS WRITTEN INTO THE FILE, NOT ONLY ACTED ON.
# This file lands while waiters are already queued, so both versions run side by
# side, and on 08-09 they deadlocked: a new fast waiter stood at the head and
# deferred to a waiting landing exactly as the rule says, while that landing was
# an old helper waiting to BECOME the head. Neither moved; the lock stood empty
# for thirty-three minutes with three waiters behind it. Nothing looked wrong in
# either process -- both were waiting, correctly, for each other.
case_
reset_lock
start_gate fast 25 v1; A=$GATE_PID
if ! wait_for "${WORK}/v1.got" 100; then fail "22: the holder never took the lock"; else
  start_gate fast 3 v2; B=$GATE_PID; wait_in_queue "$B" 100 || fail "22: the new push never reached the queue"
  bash "${WORK}/waiter_that_acts_on_the_head.sh" "${WORK}/v3" landing >/dev/null 2>&1 &
  C=$!
  wait_in_queue "$C" 100 || fail "22: the old-style landing never reached the queue"
  wait "$A" 2>/dev/null; wait "$B" 2>/dev/null; wait "$C" 2>/dev/null
  # THE ASSERTION IS ON THE NEW WAITER, and that is not obvious. Without the
  # promotion the two do eventually get through -- the new waiter gives up
  # after XFA_GATE_LOCK_WAIT, drops its line, and the old-style landing finds
  # itself at the head with seconds to spare. Thirty seconds here; four to
  # eight hours in production, with the lock sitting EMPTY for all of it. So
  # what marks the deadlock is not that nobody is ever served, it is that the
  # new waiter is served only by abandoning its own wait.
  [ -f "${WORK}/v3.got" ] \
    || fail "22: the old-style landing was never served at all"
  [ -f "${WORK}/v2.got" ] \
    || fail "22: the new waiter deferred to a landing that was waiting to become the head, and gave up its whole wait to break the deadlock; the choice has to be written into the file where both versions read it"
fi
reset_lock

# 23. A WAITER THAT LOSES ITS QUEUE LINE PUTS IT BACK.
# Measured on 08-09 at 10:45Z: a landing gate two hours ten into its wait and a
# fast gate three hours eight into its were both absent from the queue, and
# master stood still for three and a half hours behind them. A waiter registers
# once and then polls; absent from the line it can never be next, and neither
# the process nor the log can tell that from waiting its turn -- the log says
# "waiting" once and then nothing for four hours.
case_
reset_lock
start_gate fast 20 g1; A=$GATE_PID
if ! wait_for "${WORK}/g1.got" 100; then fail "23: the holder never took the lock"; else
  bash "$HELPER" --acquire fast "$$" > "${WORK}/lost.out" 2>&1 &
  W=$!
  if ! wait_in_queue "$$" 100; then fail "23: the waiter never reached the queue"; else
    grep -v "^$$ " "${XFA_GATE_QUEUE}" > "${XFA_GATE_QUEUE}.cut" 2>/dev/null || true
    mv -f "${XFA_GATE_QUEUE}.cut" "${XFA_GATE_QUEUE}"
    wait "$W"; rc=$?
    [ "$rc" -eq 0 ] \
      || fail "23: a waiter whose line was dropped never got the lock (exit ${rc}); that is the four-hour wait with nothing to show for it"
    grep -q "queue line was gone" "${WORK}/lost.out" \
      || fail "23: the loss was not reported; a silent recovery cannot be told from never having lost the line, and the mechanism still needs finding"
    bash "$HELPER" --release "$$" >/dev/null 2>&1
  fi
  wait "$A" 2>/dev/null
fi
reset_lock

# 24. AN OLD QUEUE LOCK HELD BY A LIVING SHELL IS LEFT ALONE.
# This is the racing mechanism the self-heal in case 23 is a net under. Two
# waiters holding the queue lock at once both rewrite the file, and the loser's
# `mv` puts back a snapshot without the newest lines. The old rule made that
# reachable: it broke any queue lock older than a minute, on the grounds that
# nothing holds it that long -- a claim about timing on a machine that spent
# this week at load 100+, with this file rewriting the queue more often than it
# used to.
case_
reset_lock
mkdir -p "${XFA_GATE_QUEUE}.lock"
echo "$$" > "${XFA_GATE_QUEUE}.lock.pid"
touch -t 200001010000 "${XFA_GATE_QUEUE}.lock"
bash "$HELPER" --acquire fast "$$" > "${WORK}/qlock.out" 2>&1 &
W=$!
sleep 2
if [ ! -d "${XFA_GATE_QUEUE}.lock" ] || [ "$(cat "${XFA_GATE_QUEUE}.lock.pid" 2>/dev/null)" != "$$" ]; then
  fail "24: a queue lock held by a LIVE shell was broken because it was old; two waiters can now rewrite the queue at once and one of them loses lines"
fi
kill "$W" 2>/dev/null; wait "$W" 2>/dev/null
rm -rf "${XFA_GATE_QUEUE}.lock" "${XFA_GATE_QUEUE}.lock.pid"
reset_lock

# 25. AND ONE HELD BY A DEAD SHELL IS BROKEN AT ONCE.
# Liveness has to answer both ways. A rule that never breaks the queue lock
# turns one crashed shell into a machine-wide stop, which is what the age rule
# was there to prevent and what case 24 must not undo.
case_
reset_lock
bash -c 'exit 0' & DEADQ=$!; wait "$DEADQ" 2>/dev/null
mkdir -p "${XFA_GATE_QUEUE}.lock"
echo "$DEADQ" > "${XFA_GATE_QUEUE}.lock.pid"
touch "${XFA_GATE_QUEUE}.lock"
start_gate fast 1 z1; A=$GATE_PID
wait_for "${WORK}/z1.got" 100 \
  || fail "25: a queue lock left behind by a dead shell stopped every waiter on the machine"
wait "$A" 2>/dev/null
rm -rf "${XFA_GATE_QUEUE}.lock" "${XFA_GATE_QUEUE}.lock.pid"
reset_lock

# 26. A QUEUE LOCK WITH NO PID FILE YET IS A HOLDER, NOT A CORPSE.
# The pid is written one line after the mkdir, and in that window a holder
# looks exactly like a shell that died in between. On 08-09 a holder's write
# failed with "No such file or directory" because another waiter had already
# removed the directory underneath it -- two adjacent shell statements are
# microseconds apart until the machine is at load 100+ and swapping.
#
# AGED TWO MINUTES ON PURPOSE. A directory made just now survives any window,
# so a fresh one cannot tell a one-minute rule from a five-minute one -- the
# first version of this case passed with both and guarded neither. Two minutes
# is inside the window that was too short and outside the one that was not.
case_
reset_lock
mkdir -p "${XFA_GATE_QUEUE}.lock"
python3 -c 'import os, time, sys; t = time.time() - 120; os.utime(sys.argv[1], (t, t))' "${XFA_GATE_QUEUE}.lock"
bash "$HELPER" --acquire fast "$$" > "${WORK}/orphan.out" 2>&1 &
W=$!
sleep 2
[ -d "${XFA_GATE_QUEUE}.lock" ] \
  || fail "26: a queue lock two minutes pidless was taken for a corpse and removed under its holder; a holder descheduled between the mkdir and the write loses the directory it is standing on"
kill "$W" 2>/dev/null; wait "$W" 2>/dev/null
rm -rf "${XFA_GATE_QUEUE}.lock" "${XFA_GATE_QUEUE}.lock.pid"
reset_lock

# 27. BUT ONE THAT HAS SAT PIDLESS FOR LONGER IS.
# No scheduler makes a process wait five minutes between two lines and then
# lets it carry on. Without this the queue would be stuck behind a directory
# nobody owns until somebody noticed by hand.
case_
reset_lock
mkdir -p "${XFA_GATE_QUEUE}.lock"
touch -t 200001010000 "${XFA_GATE_QUEUE}.lock"
start_gate fast 1 y1; A=$GATE_PID
wait_for "${WORK}/y1.got" 100 \
  || fail "27: a queue lock left pidless by a dead shell was never broken; the queue stops for everyone"
wait "$A" 2>/dev/null
rm -rf "${XFA_GATE_QUEUE}.lock" "${XFA_GATE_QUEUE}.lock.pid"
reset_lock

# 28. RELEASING TAKES BACK ONLY A LOCK THAT IS STILL OURS.
# If ours was broken and somebody else holds it now, an unconditional remove
# destroys a live holder's lock and puts two rewriters on the queue at once --
# which is the loss this whole lock exists to prevent.
#
# STRUCTURAL, and it says so. Reaching that state for real means holding the
# queue lock with a foreign live pid while `release` runs, and `release` starts
# by waiting sixty seconds for exactly that lock -- the case would spend a
# minute of every green run racing its own fixture. Tried twice, once for #452
# and once for #464; both times the stand-in holder died inside the spin and
# the case passed for the wrong reason. Case 14 reads with_gate_lock
# .sh the same way and for the same reason. It catches the unconditional
# `rm -rf` going back in; it does not prove the timing.
case_
_q_unlock="$(sed -n '/^q_unlock()/,/^}/p' "$HELPER")"
if [ -z "$_q_unlock" ]; then
  fail "28: q_unlock is gone from ${HELPER}"
else
  # The comparison against our OWN pid is the property. Reading the pid file
  # and then removing unconditionally still reads the pid, which is how the
  # first version of this case passed a mutation that took the guard out.
  printf '%s' "$_q_unlock" | grep -q '"\$\$"' \
    || fail "28: q_unlock no longer compares the recorded holder to our own pid before removing; a broken-and-retaken lock would be destroyed under its new holder"
  printf '%s' "$_q_unlock" | grep -Eq '(if|&&|\|\|).*rm -rf|rm -rf.*QLOCK.*; fi|then.*rm -rf' \
    || printf '%s' "$_q_unlock" | grep -B1 'rm -rf' | grep -q 'if ' \
    || fail "28: q_unlock removes the queue lock unconditionally again"
  printf '%s' "$_q_unlock" | grep -Eq 'rm -rf .*QLOCK' \
    || fail "28: q_unlock no longer removes the queue lock at all"
fi
reset_lock

# 29. THE QUEUE LOCK DIRECTORY STAYS EMPTY, SO IT CAN BE RELEASED WITH RMDIR.
# #464: `rm -rf` clears a directory and then removes it, and a `mkdir` landing
# between those two steps succeeds only to have its directory taken away by the
# releaser. Two waiters then hold this lock at once and rewrite the queue
# together, and the loser's `mv -f` drops the newest lines. `rmdir` has no such
# window -- it removes an empty directory or nothing -- and it is impossible
# while a pid file sits inside the directory, which is why the pid lives beside
# it and why that is not a detail of taste.
#
# STRUCTURAL, and for a duller reason than case 28's. The queue lock exists for
# the milliseconds inside `acquire`; by the time any command here can look, it
# has been released and there is no directory to inspect. The first version of
# this case did look, found nothing, counted zero files and passed -- against a
# mutation that put the pid straight back inside.
case_
_qlock_writes="$(grep -n 'QLOCK' "$HELPER" | grep -E 'printf.*>' || true)"
printf '%s' "$_qlock_writes" | grep -q '\${QLOCK}\.pid' \
  || fail "29: the queue lock's holder is not recorded beside the directory (\${QLOCK}.pid); see the mutation note above"
printf '%s' "$_qlock_writes" | grep -q '"\$QLOCK/' \
  && fail "29: something is written INSIDE the queue lock directory again; rmdir cannot release it, so the rm -rf race is back"
# SCOPED TO q_unlock, because `rmdir "$QLOCK"` also appears in the two break
# paths: grepping the whole file passed while the release path itself had gone
# back to `rm -rf`, which is the one place the race actually bites.
_release="$(sed -n '/^q_unlock()/,/^}/p' "$HELPER")"
printf '%s' "$_release" | grep -q 'rmdir "\$QLOCK"' \
  || fail "29: q_unlock no longer releases with rmdir; rm -rf clears and then removes, and a mkdir between those two steps loses its directory to the releaser"
reset_lock

# 30. A WAITER WAITS AS LONG AS A LANDING DOES.
# The fast lane's default was four hours while the queue it waits in could be
# nine deep at twenty-odd minutes a holder. On 08-09 a push waited the full
# four and ended at exit 5 -- no verdict, nothing measured, four hours gone.
# ff3nr has run landings at eight hours for exactly this reason.
case_
_wait="$(grep -E '^WAIT=' "$HELPER" | head -1)"
case "$_wait" in
  *28800*) ;;
  *) fail "30: the fast lane's default wait is ${_wait:-missing}; a queue longer than the wait refuses pushes that were never judged" ;;
esac
reset_lock

if [ "$cases" -lt "$FLOOR" ]; then
  echo "FAIL: ran $cases cases, floor is $FLOOR -- the suite stopped short." >&2
  exit 1
fi
if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures of $cases cases failed." >&2
  exit 1
fi
echo "PASS: $cases cases."
exit 0
