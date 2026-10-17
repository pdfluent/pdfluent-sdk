#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# with_gate_lock.sh -- run any command under the machine-wide gate FIFO (#359).
#
# WHY THIS EXISTS
#
# local_ci_gate.sh is not the only thing on this Mac that compiles. The editor
# session runs its own builds, and on 07-09-2026 it sat forty-two minutes on
# /tmp/xfa-gate.lock with a private `mkdir` poller and lost every race: a plain
# spin has no place in the queue, so a FIFO waiter that arrives later is served
# first, every time. Two implementations of one lock is the bug -- the queue is
# a shared data structure, and a second reader that does not append to it is
# not waiting in the same line at all.
#
# So there is one implementation, _one_gate_at_a_time.sh, and this is how
# anything that is not the gate itself calls it. A caller outside this
# repository runs this script; it does not reimplement the protocol.
#
# USAGE
#   bash scripts/ci/with_gate_lock.sh [--lane fast|landing|full] [--declare D]
#                                     -- <cmd> [args…]
#   bash scripts/ci/with_gate_lock.sh -- cargo build --release
#   bash scripts/ci/with_gate_lock.sh --lane landing -- ./deploy.sh
#   bash scripts/ci/with_gate_lock.sh --declare 5m -- cargo test --lib
#
# DECLARING A DURATION (#452)
#
# `--declare` takes seconds, or minutes with an `m` suffix. A fast waiter that
# declares five minutes or less is not passed by a waiting landing: the editor
# session's two gate steps are four to six minutes on a warm target and were
# sitting two hours behind SDK gates, close enough to its own six-hour lock
# timeout to matter, and a landing that waits five minutes for them has lost
# almost nothing.
#
# It is a promise, not a hint, and it is enforced against the caller: a fast
# holder that declared D is broken once it has held D plus a minute, where an
# undeclared one would have had two hours. Declare what the command actually
# takes on a cold cache, or do not declare at all -- being passed is cheaper
# than being killed halfway.
#
# The default lane is `fast`, which is the lane a caller wants unless it is
# landing something: a fast holder can be broken once it is stuck past
# XFA_GATE_LOCK_MAX_AGE, and a caller that claims `landing` is claiming that it
# never should be.
#
# EXIT STATUS
#   the command's own status, passed through unchanged
#   5   waited past XFA_GATE_LOCK_WAIT -- the command never ran
#   6   the wrapper itself was gone before its turn came -- nothing ran
#   2   bad usage, or the queue lock is stuck
#
# Five is the gate's own "no verdict was reached" code, and it means the same
# thing here: nothing ran, so nothing was judged. A caller that treats it as a
# failure of the command will go looking for a defect that was never measured.
#
# THE ENVIRONMENT IT HANDS ON
#
# CARGO_BUILD_JOBS is set to the physical cores minus two and exported, exactly
# as the gate does, so a build run through this wrapper leaves the machine
# usable for the person at the keyboard. A caller that has already set it keeps
# its own value.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCKER="${HERE}/_one_gate_at_a_time.sh"

LANE=fast
DECLARE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --lane) [ $# -ge 2 ] || { echo "with_gate_lock: --lane needs a value" >&2; exit 2; }
            LANE="$2"; shift 2 ;;
    --declare) [ $# -ge 2 ] || { echo "with_gate_lock: --declare needs a value" >&2; exit 2; }
            DECLARE="$2"; shift 2 ;;
    --)     shift; break ;;
    *)      echo "with_gate_lock: unexpected argument '$1' (commands go after --)" >&2; exit 2 ;;
  esac
done
[ $# -ge 1 ] || { echo "usage: with_gate_lock.sh [--lane fast|landing|full] [--declare D] -- <cmd> [args…]" >&2; exit 2; }
case "$LANE" in fast|landing|full) ;; *) echo "with_gate_lock: unknown lane '$LANE'" >&2; exit 2 ;; esac

# A DECLARATION THAT DOES NOT PARSE IS REFUSED, not quietly dropped. Silently
# reading `--declare 5min` as no declaration would leave the caller believing
# it had bought an exemption it does not have, and the symptom -- being passed
# by landings -- is exactly what it was trying to avoid.
if [ -n "$DECLARE" ]; then
  # The digits are checked BEFORE any arithmetic: `$(( 5min * 60 ))` is a bash
  # error on stderr and an unset variable, which reads as "no declaration" one
  # branch later -- the quiet drop this refusal exists to prevent.
  declared_number="${DECLARE%m}"
  case "$declared_number" in ''|*[!0-9]*|0)
    echo "with_gate_lock: --declare wants seconds or minutes ('300', '5m'), not '$DECLARE'" >&2; exit 2 ;;
  esac
  case "$DECLARE" in
    *m) XFA_GATE_DECLARED_SECONDS=$(( declared_number * 60 )) ;;
    *)  XFA_GATE_DECLARED_SECONDS="$declared_number" ;;
  esac
  export XFA_GATE_DECLARED_SECONDS
fi

# RELEASING IS UNCONDITIONAL, not guarded by a "do I hold it" flag. Such a flag
# is set by the parent shell one command AFTER the helper has already made the
# lock directory and written the pid into it, and a signal in between leaves a
# lock nobody will hand back. --release is idempotent and checks the pid itself:
# it removes the lock only if this process is the holder, and drops this pid
# from the queue either way, which is exactly right for a waiter that was killed
# before its turn came.
release() {
  bash "$LOCKER" --release "$$" >/dev/null 2>&1
  return 0
}
# CMD_PID is declared here so the signal handlers below can be armed BEFORE the
# acquire: a wrapper waits in the queue for as long as the holder takes, and a
# TERM in that window found the old shape of this file with no handler at all.
# An untrapped TERM does not exit, it dies -- no EXIT trap, no release -- and
# the lock or the queue line stood until somebody noticed the pid was gone.
# Killing a waiting gate is not exotic; it is how the four-hour run of 07-09
# ended.
CMD_PID=""
kill_the_command() {
  [ -n "$CMD_PID" ] && kill -TERM "$CMD_PID" 2>/dev/null
  return 0
}
trap release EXIT
trap 'kill_the_command; exit 130' INT
trap 'kill_the_command; exit 143' TERM

# `$?` IS READ FROM THE PLAIN CALL, not from `if ! …`: the negation rewrites the
# status to 0 or 1, and the caller would be told 0 -- success -- for a wrapper
# that gave up waiting and ran nothing.
bash "$LOCKER" --acquire "$LANE" "$$"
rc=$?
if [ "$rc" -ne 0 ]; then
  [ "$rc" = 5 ] && echo "with_gate_lock: gave up waiting; the command did not run." >&2
  exit "$rc"
fi

if [ -z "${CARGO_BUILD_JOBS:-}" ]; then
  CARGO_BUILD_JOBS="$(bash "$LOCKER" --jobs)"
  export CARGO_BUILD_JOBS
fi

# THE COMMAND RUNS IN THE BACKGROUND AND THE WRAPPER WAITS ON IT.
#
# Not for concurrency -- for the signal. bash runs a trap only once the
# FOREGROUND command returns, so a wrapper that ran `"$@"` in the foreground
# would answer a TERM whenever its command happened to finish, which for a long
# build is never: the lock would stand while the terminal that killed the
# wrapper went on believing it had let go. Waiting on a background child makes
# the wait itself interruptible, so the trap runs at once.
#
# `<&0` is not a no-op: an async command gets /dev/null for stdin unless it
# redirects explicitly, and a wrapped command that reads input would see EOF.
"$@" <&0 &
CMD_PID=$!
wait "$CMD_PID"
rc=$?
exit "$rc"
