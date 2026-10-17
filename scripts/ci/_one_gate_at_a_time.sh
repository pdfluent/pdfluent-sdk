#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# _one_gate_at_a_time.sh -- one machine-wide lock for the local CI gate, so a
# landing is not starved by the pushes running around it (#359).
#
# Underscore-prefixed because it is a library and not a guard: on its own it
# takes a lock and gives it back, and would pass every time. It has no verdict
# to give a CI job, and every_guard_has_a_job.py skips this prefix for that
# reason. What runs is test_one_gate_at_a_time.sh, over dummy gates.
#
# WHAT HAPPENED
#
# 07-09-2026, 01:20 to 02:10Z. Three local gates ran at once on a ten-core Mac
# -- a landing gate, a push gate, and a third gate -- next to the GUI apps the
# owner was working in. Load went from 85 to 124. Each rustc got three to ten
# per cent of a core. The landing gate sat four hours in `cargo test`; the
# verified round-three measurement behind it waited three hours for the landing
# lock, and every other terminal's push gate crawled the same way.
#
# The landing lock (/tmp/cos-landing.lock) already serialises landings. It says
# nothing about the push gates that run around them, which is where the cores
# actually went.
#
# WHY SERIALISING IS FASTER THAN SHARING
#
# These gates compile. Three cargo builds on four to ten cores do not run in a
# third of the time each -- they thrash one core count, one page cache and one
# disk, and the sum is worse than running them in a row. Waiting costs wall time
# that is visible; thrashing costs wall time that is not, and it costs it to
# everyone including the person at the keyboard.
#
# FIFO, NOT A SCRAMBLE
#
# A plain "spin until mkdir works" loop has no order: with five terminals the
# same unlucky one can lose every race, which is how a three-hour wait happens
# while others take the lock repeatedly. So a waiter appends itself to a queue
# file and may take the lock only at the head of that queue. Arrival order is
# service order.
#
# A LANDING HOLDER IS NEVER PREEMPTED
#
# A lock nobody can break is a lock one crashed shell keeps forever, so a holder
# whose process is gone is cleared by whoever notices. Beyond that: a FAST-lane
# holder that has held past XFA_GATE_LOCK_MAX_AGE is broken (a fast lane is
# minutes; hours means stuck). A LANDING-lane holder is never broken on age. A
# landing is the expensive, serialised, already-queued thing this whole file
# exists to protect; taking the lock out from under one would put a second
# compiling gate beside it, which is the failure being fixed.
#
# THE PROTOCOL, FOR A CALLER THAT IS NOT THIS REPOSITORY'S GATE
#
# Written down because on 07-09-2026 the editor session waited forty-two minutes
# on /tmp/xfa-gate.lock with a private `mkdir` poller and lost every race: a
# spinner that does not append to the queue is not in the line at all, and every
# FIFO waiter that arrives later is served before it. Two implementations of one
# lock is the bug. Anything outside this repository should call
# scripts/ci/with_gate_lock.sh rather than reimplement what follows -- this
# section exists so that the state on disk is readable, not so it is copied.
#
#   THE QUEUE   /tmp/xfa-gate.queue, one waiter per line:
#                 "<pid> <lane> <unix seconds> <passes> <decl> <lock dir>"
#               A waiter appends itself once, before it starts polling, and
#               service order is the order of the lines: appending is atomic
#               and every rewriter here preserves relative position, so the
#               file is the arrival record. The one thing that moves a line is
#               promote_to_head, which puts whoever is next at the front so
#               that a helper from before #452 -- which acts on the first line
#               and nothing else -- reads the same answer this one computes. Field three is whole seconds and several
#               waiters share one on a busy evening; it is for reading, not for
#               sorting. The two optional fields default to 0 and are
#               written by any waiter that touches the line; a reader that
#               knows only the first three fields carries them along untouched
#               (`read -r p lane t` puts the rest in `t` and writes it back
#               verbatim), which is what lets old and new waiters share one
#               queue during a landing of this file.
#
#               THE LAST FIELD IS WHICH LOCK THIS LINE IS QUEUEING FOR, and a
#               line for a different one is skipped and passed through
#               untouched. The queue file is already per-lock -- it is derived
#               from the lock directory -- so the field is redundant to the
#               code and not to the person reading /tmp. There is more than one
#               FIFO on this machine (the gate's, and the landing script's on
#               /tmp/cos-landing2.lock), they run the same helper, and both say
#               `--acquire landing` in `ps`. The only thing that told them
#               apart was an environment variable, which `ps` does not show. On
#               08-09 that cost twenty minutes of an idle gate with ten waiters
#               when someone read one queue's waiters as the other's and
#               "repaired" the file. A queue that cannot be misread by hand is
#               worth six characters a line.
#
#   PRIORITY    A waiting LANDING goes before fast waiters that arrived earlier
#               (#452). Arrival order is fair to processes and unfair to the
#               product: a push exists to prepare a landing, so master standing
#               still while a landing waits behind six pushes is the expensive
#               outcome. Bounded three ways, because "landings first" with no
#               bound is "pushes never":
#                 * A fast waiter that has been passed XFA_GATE_MAX_PASSES
#                   times (3) goes next, whatever is waiting behind it.
#                 * A fast waiter that DECLARED a duration of at most
#                   XFA_GATE_SHORT_DECL (300s) is not passed at all. Its cost
#                   to a landing is its own declaration; the editor session's
#                   gate steps are four to six minutes and were sitting two
#                   hours behind (#452 comment, 08-09).
#                 * A landing never passes another landing.
#               No preemption in either direction: a holder finishes.
#
#   DECLARING   XFA_GATE_DECLARED_SECONDS, read at acquire time, is a waiter's
#               claim about how long it will hold. It buys the exemption above
#               and costs a shorter leash: a fast holder that declared D is
#               broken once it has held D + XFA_GATE_DECL_SLACK, instead of at
#               XFA_GATE_LOCK_MAX_AGE. A declaration you overrun is taken off
#               you; that is what keeps it from being a free pass.
#
#   ITS LOCK    /tmp/xfa-gate.queue.lock, a mkdir held for milliseconds while
#               the queue is rewritten. It stays EMPTY: the holding shell's pid
#               goes in /tmp/xfa-gate.queue.lock.pid, beside it. Broken when
#               that pid is gone, and released with `rmdir`, which takes an
#               empty directory or nothing.
#
#               Empty and rmdir together are #464. `rm -rf` clears a directory
#               and then removes it, so a `mkdir` landing between those two
#               steps succeeds and has its directory removed out from under it
#               by the releaser -- two waiters then hold this lock at once,
#               both rewrite the queue, and the loser's `mv -f` drops the
#               newest lines. A pid file INSIDE the directory is what made
#               `rmdir` impossible, so where the pid lives is not a detail.
#
#               It used to be broken on age alone -- older than a minute meant
#               a dead shell, because "nothing here holds it that long". That
#               is an assumption about timing, and it is the one that has to be
#               wrong for a queue line to go missing while every rewriter holds
#               this lock. Two waiters did go missing on 08-09. A lock dir with
#               no pid file is still broken on age -- that window is one line
#               of shell wide -- but on five minutes, not one: a holder whose
#               pid write had not landed yet was mistaken for a corpse and had
#               the directory pulled out from under it the same day.
#
#   THE LOCK    /tmp/xfa-gate.lock, a mkdir, with three files written after it:
#                 pid    the holder -- the GATE's pid, not the helper's
#                 lane   fast, landing or full
#                 since  unix seconds at acquisition
#               A directory with no pid file is a half-finished mkdir and is
#               cleared by whoever notices.
#
#   DEAD        A holder whose pid is gone is cleared, and a queue line whose
#               pid is gone is dropped, by any waiter that notices. There is no
#               reaper: the waiters are the reaper.
#
#   STUCK       A FAST holder past XFA_GATE_LOCK_MAX_AGE -- or past its own
#               declaration, if it made a short one -- is broken by a waiter,
#               and its queue line goes with it. A LANDING holder is never
#               broken on age, whatever its age.
#
#   LOST        A waiter that no longer finds its own line puts it back, with
#               the arrival time it originally had, so it keeps its place. A
#               lost line used to be fatal and silent -- you cannot be next if
#               you are not in the line, and polling a queue you are absent
#               from looks exactly like waiting your turn. On 08-09 two waiters
#               were found gone from the queue after two and three hours, and
#               master stood still for three and a half of them.
#
#   GONE        A waiter whose GATE is gone stops waiting (exit 6). The helper
#               runs as a child of the process it takes the lock for, and on
#               08-09 one orphaned helper was found waiting an hour with ppid 1
#               and its gate pid long dead. It would have taken the lock and
#               handed it to a corpse.
#
#   RELEASING   The holder removes the lock and its own queue line. It must do
#               this from an EXIT trap, and must turn INT and TERM into an
#               ordinary exit -- a shell killed by an untrapped TERM runs no
#               EXIT trap, and its lock stands until the next waiter notices.
#
# USAGE
#   bash _one_gate_at_a_time.sh --acquire <fast|landing|full> <pid>   # blocks
#     XFA_GATE_DECLARED_SECONDS=300 …  # a fast waiter's promise, see DECLARING
#   bash _one_gate_at_a_time.sh --release <pid>                       # always 0
#   bash _one_gate_at_a_time.sh --holder                              # who has it
#   bash _one_gate_at_a_time.sh --jobs                                # cargo -j
#
# The caller passes its own pid because the lock has to outlive this process:
# the holder is the gate, not the helper that took the lock for it. The caller
# is what releases it, from its EXIT trap.
#
# Exit 5 from --acquire: waited past XFA_GATE_LOCK_WAIT. That is the absence of
# a verdict, like the disk refusal at exit 3 and the moved-tree refusal at exit
# 4, and the gate reports it as such rather than as failing code. Exit 6 is the
# same kind of answer for a different reason: the gate this lock was being
# taken for is gone, so there is nothing left to run and nothing to judge.
#
# ENVIRONMENT (the test sets all of these; the gate sets none)
#   XFA_GATE_LOCK=0             turn the lock off entirely
#   XFA_GATE_LOCK_DIR           default /tmp/xfa-gate.lock
#   XFA_GATE_QUEUE              default <lock dir without .lock>.queue
#   XFA_GATE_LOCK_WAIT          seconds to wait, default 28800 (eight hours)
#   XFA_GATE_LOCK_POLL          seconds between attempts, default 2
#   XFA_GATE_LOCK_MAX_AGE       seconds before a FAST holder is broken, 7200
#   XFA_GATE_MAX_PASSES         times a fast waiter may be passed, default 3
#   XFA_GATE_QLOCK_ORPHAN_MIN   minutes before a PIDLESS queue lock is a
#                               corpse, default 5
#   XFA_GATE_SHORT_DECL         a declaration at or under this is exempt, 300
#   XFA_GATE_DECL_SLACK         grace on a declared holder's leash, default 60
#   XFA_GATE_DECLARED_SECONDS   this waiter's own claim, default 0 (none)
set -uo pipefail

LOCK="${XFA_GATE_LOCK_DIR:-/tmp/xfa-gate.lock}"
QUEUE="${XFA_GATE_QUEUE:-${LOCK%.lock}.queue}"
QLOCK="${QUEUE}.lock"
WAIT="${XFA_GATE_LOCK_WAIT:-28800}"
POLL="${XFA_GATE_LOCK_POLL:-2}"
MAX_AGE="${XFA_GATE_LOCK_MAX_AGE:-7200}"
MAX_PASSES="${XFA_GATE_MAX_PASSES:-3}"
QLOCK_ORPHAN_MIN="${XFA_GATE_QLOCK_ORPHAN_MIN:-5}"
SHORT_DECL="${XFA_GATE_SHORT_DECL:-300}"
DECL_SLACK="${XFA_GATE_DECL_SLACK:-60}"
DECLARED="${XFA_GATE_DECLARED_SECONDS:-0}"
case "$DECLARED" in ''|*[!0-9]*) DECLARED=0 ;; esac

now() { date +%s; }

# THE QUEUE IS REWRITTEN, so it needs its own lock. Held for milliseconds --
# long enough to read a handful of lines and write them back -- and taken by
# mkdir, which is atomic on every filesystem this repository runs on.
#
# One that is a minute old belongs to a shell that died between the mkdir and
# the rmdir; nothing here holds it that long, so breaking it is safe in a way
# breaking the gate lock is not.
q_lock() {
  local i hp
  for i in $(seq 1 600); do
    if mkdir "$QLOCK" 2>/dev/null; then
      # BESIDE THE DIRECTORY, NOT INSIDE IT, so the directory stays empty and
      # `rmdir` can remove it. See RELEASING below for why that matters.
      #
      # Braced, because `cmd > file 2>/dev/null` does not silence a failure to
      # OPEN the file: that message comes from the shell setting up the
      # redirection, not from the command, and redirecting the command's stderr
      # is beside the point. It printed a path that does not exist into every
      # gate log that hit the race this file is now fixing.
      #
      # Non-fatal, because the lock is the directory and the pid is only how
      # somebody else tells a holder from a corpse.
      { printf '%s\n' "$$" > "${QLOCK}.pid"; } 2>/dev/null || true
      return 0
    fi
    if [ -d "$QLOCK" ]; then
      hp="$(cat "${QLOCK}.pid" 2>/dev/null)"
      if [ -n "$hp" ]; then
        # LIVENESS, NOT AGE. A queue lock held by a living shell is held for a
        # reason, however long it has taken; one held by a dead shell is held
        # by nobody, and age could not tell those apart.
        #
        # The old rule broke any queue lock older than a minute, on the grounds
        # that nothing here holds it that long. `prune` walks the queue with a
        # `kill -0` per line, and this file now rewrites the queue more often
        # than it did -- a promotion per handover on top of that prune. On a
        # machine at load 100+ a minute stops being obviously impossible, and
        # two waiters holding this lock at once both rewrite the file: the
        # loser's `mv -f` puts back a snapshot without the newest lines, and
        # whoever was in them is gone from the queue with no way to find out.
        # That is the shape of the two waiters lost on 08-09.
        kill -0 "$hp" 2>/dev/null || { rm -f "${QLOCK}.pid" 2>/dev/null; rmdir "$QLOCK" 2>/dev/null || rm -rf "$QLOCK" 2>/dev/null; }
      elif [ -z "$(find "$QLOCK" -maxdepth 0 -mmin "-${QLOCK_ORPHAN_MIN}" 2>/dev/null)" ]; then
        # NO PID FILE YET is not the same as no owner. Between the mkdir and
        # the write one line later, a holder is indistinguishable from a shell
        # that died in between, and only time separates them.
        #
        # That window was a minute, and it cost a real one: on 08-09 a holder's
        # write failed with "No such file or directory" because somebody had
        # already removed the directory underneath it. Two adjacent shell
        # statements are microseconds apart until the machine is at load 100+
        # and swapping, and then a minute is not the safe margin it reads as.
        # Five is: no scheduler makes a process wait five minutes between two
        # lines and then lets it carry on.
        rm -f "${QLOCK}.pid" 2>/dev/null
        rmdir "$QLOCK" 2>/dev/null || rm -rf "$QLOCK" 2>/dev/null
      fi
    fi
    sleep 0.1
  done
  return 1
}
# RMDIR, NOT `rm -rf`, AND THAT IS THE WHOLE OF #464.
#
# `rm -rf` on a directory clears its contents and THEN removes the directory.
# Another waiter's `mkdir` landing between those two steps succeeds -- and this
# release then removes the directory that waiter just created. It believes it
# holds the queue lock and does not, two rewriters run over the queue at once,
# and the loser's `mv -f` puts back a snapshot without the newest lines.
#
# That is the mechanism under the two waiters found gone from the queue on
# 08-09, three and a half hours of a stopped master behind them, and under the
# `No such file or directory` and `Invalid argument` this file's own pid write
# reported on the way in: the second is the signature of the directory being
# replaced under an open, not merely missing.
#
# `rmdir` removes an EMPTY directory or nothing. It cannot take a directory
# another process has just made, because that one is not the one we are naming
# any more -- there is no window between clearing and removing, since there is
# nothing to clear. Which is why the holder's pid lives BESIDE the lock rather
# than inside it: a lock with contents cannot be released this way at all.
#
# ONLY A LOCK THAT IS STILL OURS, as before. If ours was broken and somebody
# else has taken it since, releasing theirs would recreate the same failure by
# a different route. A missing pid file is treated as ours: our own write is
# the one that can fail, and leaving it would strand the queue until the orphan
# timeout. The `rm -rf` fallback is for a lock directory that somehow has
# contents -- a copy of this file from before #464 wrote a pid into it -- and
# it keeps the transition from deadlocking.
q_unlock() {
  local hp
  hp="$(cat "${QLOCK}.pid" 2>/dev/null)"
  if [ -z "$hp" ] || [ "$hp" = "$$" ]; then
    rm -f "${QLOCK}.pid" 2>/dev/null
    rmdir "$QLOCK" 2>/dev/null || rm -rf "$QLOCK" 2>/dev/null
  fi
  return 0
}

# Every entry whose process is gone, and the lock itself if its holder is gone.
# Called with the queue lock held.
prune() {
  local tmp raw p lane t passes decl dir hp
  if [ -f "$QUEUE" ]; then
    tmp="${QUEUE}.tmp.$$"
    : > "$tmp"
    while IFS= read -r raw; do
      [ -n "$raw" ] || continue
      read -r p lane t passes decl dir <<< "$raw"
      # NOT OURS, NOT OUR BUSINESS. Dropping another lock's line because its
      # pid means nothing here is how one queue eats another.
      if [ "${dir:-$LOCK}" != "$LOCK" ]; then printf '%s\n' "$raw" >> "$tmp"; continue; fi
      if kill -0 "$p" 2>/dev/null; then
        printf '%s %s %s %s %s %s\n' "$p" "$lane" "$t" "${passes:-0}" "${decl:-0}" "$LOCK" >> "$tmp"
      fi
    done < "$QUEUE"
    mv -f "$tmp" "$QUEUE"
  fi
  if [ -f "$LOCK/pid" ]; then
    hp="$(cat "$LOCK/pid" 2>/dev/null)"
    if [ -z "$hp" ] || ! kill -0 "$hp" 2>/dev/null; then rm -rf "$LOCK"; fi
  elif [ -d "$LOCK" ]; then
    # A directory with no pid file is a half-finished mkdir from a shell that
    # died in between. Nothing owns it.
    rm -rf "$LOCK"
  fi
}

# A FAST holder that has held past the limit is stuck, and a stuck holder is
# broken by whoever is waiting -- ahead of the head-of-queue check, because the
# holder IS the head of the queue and a waiter behind it would otherwise never
# look. Its queue line goes with the lock, so the queue advances.
#
# Never a landing holder, whatever its age. Called with the queue lock held.
break_a_stuck_holder() {
  local hlane age hp decl limit
  [ -f "$LOCK/pid" ] || return 0
  hlane="$(cat "$LOCK/lane" 2>/dev/null || echo unknown)"
  [ "$hlane" = landing ] && return 0
  age=$(( $(now) - $(cat "$LOCK/since" 2>/dev/null || now) ))
  # A HOLDER THAT DECLARED IS HELD TO IT. The declaration bought it the right
  # not to be passed by a landing; the price is that its leash is its own
  # promise plus a minute, rather than the two hours a silent fast holder gets.
  # Otherwise "I will be five minutes" is a free pass with nothing behind it.
  decl="$(cat "$LOCK/decl" 2>/dev/null || echo 0)"
  case "$decl" in ''|*[!0-9]*) decl=0 ;; esac
  limit="$MAX_AGE"
  if [ "$decl" -gt 0 ] && [ "$decl" -le "$SHORT_DECL" ]; then limit=$(( decl + DECL_SLACK )); fi
  [ "$age" -ge "$limit" ] || return 0
  hp="$(cat "$LOCK/pid" 2>/dev/null || echo 0)"
  echo "gate lock: breaking a ${hlane}-lane holder (pid ${hp}) that has held ${age}s (limit ${limit}s)" >&2
  rm -rf "$LOCK"
  dequeue "$hp"
  return 0
}

# Drop one pid from the queue. Called with the queue lock held.
dequeue() {
  local who="$1" tmp raw p lane t passes decl dir
  [ -f "$QUEUE" ] || return 0
  tmp="${QUEUE}.tmp.$$"
  : > "$tmp"
  while IFS= read -r raw; do
    [ -n "$raw" ] || continue
    read -r p lane t passes decl dir <<< "$raw"
    if [ "${dir:-$LOCK}" != "$LOCK" ]; then printf '%s\n' "$raw" >> "$tmp"; continue; fi
    [ "$p" = "$who" ] && continue
    printf '%s %s %s %s %s %s\n' "$p" "$lane" "$t" "${passes:-0}" "${decl:-0}" "$LOCK" >> "$tmp"
  done < "$QUEUE"
  mv -f "$tmp" "$QUEUE"
}

# This lock's waiters, in file order, as five well-formed fields per line.
#
# FILE ORDER, NOT THE TIMESTAMP. The timestamp is whole seconds, and on a
# machine where eleven terminals queue at once several waiters share one; a
# sort on it falls back to whatever the tie-break is -- pid, which has nothing
# to do with who arrived first. Appending is atomic and every rewriter below
# preserves position, so the file itself is the arrival record and the second
# field is only ever read for reporting. An earlier draft of #452 sorted here
# and case 16 became flaky, which is the whole of the evidence for this note.
queue_entries() {
  [ -f "$QUEUE" ] || return 0
  local p lane t passes decl dir
  while read -r p lane t passes decl dir; do
    [ -n "${p:-}" ] || continue
    [ "${dir:-$LOCK}" = "$LOCK" ] || continue
    case "${t:-}" in ''|*[!0-9]*) t=0 ;; esac
    case "${passes:-}" in ''|*[!0-9]*) passes=0 ;; esac
    case "${decl:-}" in ''|*[!0-9]*) decl=0 ;; esac
    printf '%s %s %s %s %s\n' "$p" "$lane" "$t" "$passes" "$decl"
  done < "$QUEUE"
}

# May a waiting landing go in front of this one? Three ways to answer no, and
# each of them is a bound on the landing-first rule rather than an exception to
# it -- see PRIORITY above.
may_be_passed() { # lane passes decl
  [ "$1" = landing ] && return 1
  [ "$2" -ge "$MAX_PASSES" ] && return 1
  [ "$3" -gt 0 ] && [ "$3" -le "$SHORT_DECL" ] && return 1
  return 0
}

# Whose turn it is. The first waiting landing, unless something ahead of it may
# not be passed -- then the first such thing, which is the whole of the
# starvation bound. With no landing waiting this is just the first waiter,
# which is what the queue did before #452.
#
# Called with the queue lock held. Prints one pid, or fails on an empty queue.
next_in_line() {
  local -a pids lanes passes decls
  local p lane t pass decl i first_landing=-1
  while read -r p lane t pass decl; do
    pids+=("$p"); lanes+=("$lane"); passes+=("$pass"); decls+=("$decl")
  done < <(queue_entries)
  [ "${#pids[@]}" -gt 0 ] || return 1
  for i in "${!pids[@]}"; do
    if [ "${lanes[$i]}" = landing ]; then first_landing="$i"; break; fi
  done
  # No landing waiting, or the landing is already the oldest: nothing to pass.
  if [ "$first_landing" -le 0 ]; then printf '%s\n' "${pids[0]}"; return 0; fi
  for ((i = 0; i < first_landing; i++)); do
    if ! may_be_passed "${lanes[$i]}" "${passes[$i]}" "${decls[$i]}"; then
      printf '%s\n' "${pids[$i]}"; return 0
    fi
  done
  printf '%s\n' "${pids[$first_landing]}"
  return 0
}

# THE CHOICE IS WRITTEN INTO THE FILE, not only acted on.
#
# A helper from before #452 takes the lock when its line is literally the first
# in the file. This one takes it when next_in_line names it. During a landing of
# this very file the two run side by side, and on 08-09 they deadlocked: a new
# fast waiter stood at the head, deferred to a waiting landing exactly as the
# rule says, and that landing was an old helper waiting to BECOME the head.
# Neither moved. The lock stood empty for thirty-three minutes with three
# waiters behind it, and nothing in either process looked wrong -- both were
# waiting, correctly, for each other.
#
# So the winner is moved to the front of the file. Then "whose turn is it" has
# one answer that every version reads the same way, and a mix of versions cannot
# disagree. The rest of the queue keeps its relative order, so this marks who is
# next rather than reshuffling anybody.
#
# Called with the queue lock held.
promote_to_head() {
  local who="$1" tmp raw p rest first
  first="$(head -n 1 "$QUEUE" 2>/dev/null | cut -d' ' -f1)"
  # Already there: nobody is being passed, so there is nothing to count either.
  [ "${first:-}" = "$who" ] && return 0
  count_the_pass "$who"
  tmp="${QUEUE}.tmp.$$"
  : > "$tmp"
  while IFS= read -r raw; do
    [ -n "$raw" ] || continue
    read -r p rest <<< "$raw"
    [ "$p" = "$who" ] && printf '%s\n' "$raw" >> "$tmp"
  done < "$QUEUE"
  while IFS= read -r raw; do
    [ -n "$raw" ] || continue
    read -r p rest <<< "$raw"
    [ "$p" = "$who" ] && continue
    printf '%s\n' "$raw" >> "$tmp"
  done < "$QUEUE"
  mv -f "$tmp" "$QUEUE"
}

# A pass is counted ONCE PER HANDOVER, from promote_to_head, at the moment the
# winner is actually moved past somebody -- not once per poll. Polling is a
# two-second loop; counting there would put a fast waiter at the cap in six
# seconds and the rule would mean nothing. Everyone ahead of the winner and
# still waiting has been passed, which is exactly the set this walks: the lines
# before the winner's own, in the order they are in before it is moved.
#
# Called with the queue lock held.
count_the_pass() {
  local winner="$1" tmp raw p lane t passes decl dir before=1
  grep -q "^${winner} " "$QUEUE" 2>/dev/null || return 0
  tmp="${QUEUE}.tmp.$$"
  : > "$tmp"
  while IFS= read -r raw; do
    [ -n "$raw" ] || continue
    read -r p lane t passes decl dir <<< "$raw"
    if [ "${dir:-$LOCK}" != "$LOCK" ]; then printf '%s\n' "$raw" >> "$tmp"; continue; fi
    case "${passes:-}" in ''|*[!0-9]*) passes=0 ;; esac
    if [ "$p" = "$winner" ]; then
      before=0
    elif [ "$before" = 1 ] && [ "$lane" != landing ]; then
      passes=$(( passes + 1 ))
    fi
    printf '%s %s %s %s %s %s\n' "$p" "$lane" "$t" "$passes" "${decl:-0}" "$LOCK" >> "$tmp"
  done < "$QUEUE"
  mv -f "$tmp" "$QUEUE"
}

# NAMES THE LOCK, because more than one FIFO on this machine runs this script
# and `--holder` on its own read as "the" holder. Which one it is was in an
# environment variable and nowhere a reader would look.
holder_line() {
  [ -f "$LOCK/pid" ] || return 1
  printf '%s: pid %s, %s lane, held %ss' \
    "$LOCK" \
    "$(cat "$LOCK/pid" 2>/dev/null)" \
    "$(cat "$LOCK/lane" 2>/dev/null || echo unknown)" \
    "$(( $(now) - $(cat "$LOCK/since" 2>/dev/null || now) ))"
}

acquire() {
  local lane="$1" me="$2" deadline turn said=0 lost=0 my_ts
  case "$lane" in fast|landing|full) ;; *) echo "unknown lane: $lane" >&2; return 2 ;; esac
  mkdir -p "$(dirname "$QUEUE")" 2>/dev/null
  my_ts="$(now)"
  q_lock || { echo "gate lock: queue lock is stuck at $QLOCK" >&2; return 2; }
  prune
  printf '%s %s %s %s %s %s\n' "$me" "$lane" "$my_ts" 0 "$DECLARED" "$LOCK" >> "$QUEUE"
  q_unlock
  deadline=$(( my_ts + WAIT ))
  while [ "$(now)" -lt "$deadline" ]; do
    # WAITING ON BEHALF OF A CORPSE. This helper is a child of the gate it
    # takes the lock for. One orphaned helper was found on 08-09 with ppid 1
    # and a gate pid that had been dead for an hour, still holding its place
    # and still willing to take the lock for a process that could not use it.
    if ! kill -0 "$me" 2>/dev/null; then
      q_lock && { dequeue "$me"; q_unlock; }
      echo "gate lock: the gate ${LOCK} was for (pid ${me}) is gone; not waiting on its behalf" >&2
      return 6
    fi
    q_lock || { echo "gate lock: queue lock is stuck at $QLOCK" >&2; return 2; }
    prune
    break_a_stuck_holder
    # PUT MY LINE BACK IF IT IS GONE, with the arrival time it originally had.
    #
    # A waiter registers once and then polls, so until #452 a line that
    # disappeared was fatal and silent: you cannot be next_in_line if you are
    # not in the line, and from the outside -- and from the log, which says
    # "waiting" once and then nothing -- that is indistinguishable from waiting
    # your turn. It ended at XFA_GATE_LOCK_WAIT with nothing to show.
    #
    # Measured on 10-45Z 08-09: a landing gate that had waited two hours ten and
    # a fast gate that had waited three hours eight were both absent from the
    # queue, and master stood still for three and a half hours behind them.
    # Every rewriter here holds the queue lock, so a line should not be able to
    # vanish -- but the queue lock is broken on AGE, and this file now rewrites
    # the queue more often than it used to (a promotion per handover, a prune
    # per poll), which is more chances for that assumption to be wrong.
    #
    # Finding the mechanism is worth doing and is not what this line is for.
    # Whatever loses a line, losing one must cost a poll and not four hours.
    if ! grep -q "^${me} " "$QUEUE" 2>/dev/null; then
      printf '%s %s %s %s %s %s\n' "$me" "$lane" "$my_ts" 0 "$DECLARED" "$LOCK" >> "$QUEUE"
      if [ "$lost" = 0 ]; then
        echo "gate lock: my queue line was gone; put it back with its arrival time (${my_ts})" >&2
        lost=1
      fi
    fi
    turn="$(next_in_line)"
    [ -n "${turn:-}" ] && promote_to_head "$turn"
    if [ "${turn:-}" = "$me" ] && mkdir "$LOCK" 2>/dev/null; then
      printf '%s\n' "$me"        > "$LOCK/pid"
      printf '%s\n' "$lane"      > "$LOCK/lane"
      now                        > "$LOCK/since"
      printf '%s\n' "$DECLARED" > "$LOCK/decl"
      q_unlock
      return 0
    fi
    q_unlock
    if [ "$said" = 0 ]; then
      echo "gate lock: waiting for ${LOCK} ($(holder_line || echo 'no holder yet'))"
      said=1
    fi
    sleep "$POLL"
  done
  q_lock && { dequeue "$me"; q_unlock; }
  echo "gate lock: gave up on ${LOCK} after ${WAIT}s ($(holder_line || echo 'no holder'))" >&2
  return 5
}

release() {
  local me="$1"
  q_lock || return 0
  if [ -f "$LOCK/pid" ] && [ "$(cat "$LOCK/pid" 2>/dev/null)" = "$me" ]; then rm -rf "$LOCK"; fi
  dequeue "$me"
  q_unlock
  return 0
}

# PHYSICAL CORES MINUS TWO, so a gate leaves headroom for the person at the
# keyboard (#359). Physical and not logical: two hyperthreads on one core do not
# compile twice as fast, and counting them is how a "conservative" job count
# ends up oversubscribing the machine anyway. Never below one.
jobs_for_a_gate() {
  local phys=""
  if command -v sysctl >/dev/null 2>&1; then phys="$(sysctl -n hw.physicalcpu 2>/dev/null)"; fi
  if [ -z "$phys" ] && [ -r /proc/cpuinfo ]; then
    phys="$(awk -F: '/^physical id/{p=$2} /^core id/{print p"/"$2}' /proc/cpuinfo | sort -u | wc -l | tr -d ' ')"
  fi
  case "${phys:-}" in ''|*[!0-9]*) phys=0 ;; esac
  if [ "$phys" -eq 0 ] && command -v nproc >/dev/null 2>&1; then phys="$(nproc 2>/dev/null)"; fi
  case "${phys:-}" in ''|*[!0-9]*) phys=1 ;; esac
  [ "$phys" -lt 1 ] && phys=1
  local j=$(( phys - 2 ))
  [ "$j" -lt 1 ] && j=1
  printf '%s\n' "$j"
}

case "${1:-}" in
  --acquire)
    [ "${XFA_GATE_LOCK:-1}" = 0 ] && exit 0
    [ $# -eq 3 ] || { echo "usage: --acquire <fast|landing|full> <pid>" >&2; exit 2; }
    acquire "$2" "$3"; exit $?
    ;;
  --release)
    [ "${XFA_GATE_LOCK:-1}" = 0 ] && exit 0
    [ $# -eq 2 ] || { echo "usage: --release <pid>" >&2; exit 2; }
    release "$2"; exit 0
    ;;
  --holder) holder_line || { echo "${LOCK}: no holder"; exit 1; }; echo; exit 0 ;;
  --jobs)   jobs_for_a_gate; exit 0 ;;
  *) echo "usage: _one_gate_at_a_time.sh --acquire <lane> <pid> | --release <pid> | --holder | --jobs" >&2; exit 2 ;;
esac
