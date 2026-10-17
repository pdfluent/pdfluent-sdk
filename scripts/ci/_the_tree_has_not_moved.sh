#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# _the_tree_has_not_moved.sh -- refuse a verdict that was measured on a tree
# somebody moved underneath it.
#
# Underscore-prefixed because it is a library and not a guard: run on its own
# it records where the worktree is and checks it in the same breath, and would
# pass every time. It has no verdict of its own to give a CI job, and
# every_guard_has_a_job.py skips this prefix for exactly that reason. What runs
# in CI is test_the_tree_has_not_moved.py, over a dummy gate.
#
# WHAT HAPPENED
#
# 06-09-2026, 16:55:52 CEST. A landing gate had been running for six minutes in
# the shared landing worktree when a second terminal ran `git checkout --detach
# github/master` in that same directory, then put it back twenty-six seconds
# later. Nothing failed and nothing said anything. The gate carried on
# compiling, and the branch it believed it was judging had not been on disk for
# part of that run. The push at the end would have carried the branch's commits
# to master on the strength of a verdict about a different tree.
#
# Three terminals and a shared landing worktree make this ordinary rather than
# exotic. The gate already refuses when the disk is too small to measure
# anything (exit 3); this is the same class of refusal -- the absence of a
# verdict must not wear the costume of one.
#
# WHY THE COMMIT ID IS NOT ENOUGH, AND THE REFLOG LENGTH IS THE POINT
#
# In the incident the tree came back to the very commit it started on. A gate
# comparing `git rev-parse HEAD` before and after would have seen no difference
# at all and passed, which is exactly the failure this file exists to catch.
#
# What cannot come back is the reflog. Every checkout appends to the HEAD
# reflog of that worktree, so an out-and-back leaves two entries behind and the
# count is strictly larger afterwards. Three facts are recorded together --
# where HEAD points, which ref it is on, and how many times it has moved -- and
# any of the three changing means the run judged something other than what it
# was asked to judge.
#
#     baseline="$(_the_tree_has_not_moved.sh --record)"
#     ...expensive work...
#     _the_tree_has_not_moved.sh --check "$baseline" || exit 4
#
# Exit codes:
#     0  the worktree is where it was
#     1  it moved: the caller's measurements say nothing about its own branch
#     2  used wrongly
set -uo pipefail

usage() {
  echo "usage: _the_tree_has_not_moved.sh --record" >&2
  echo "       _the_tree_has_not_moved.sh --check '<baseline line>'" >&2
  exit 2
}

# One line, three fields, in an order that reads: what HEAD is, where it points,
# how often it has moved. Kept on one line so a caller can hold it in a single
# shell variable and pass it back without quoting games.
record() {
  local ref sha moves
  # A detached HEAD has no symbolic ref, and that is a state worth recording
  # rather than an error: the landing worktree is detached for part of its life.
  ref="$(git symbolic-ref -q HEAD || echo DETACHED)"
  sha="$(git rev-parse HEAD 2>/dev/null)" || {
    echo "_the_tree_has_not_moved: not in a git worktree" >&2; exit 2; }
  # An empty reflog is 0 and not an error: a freshly added worktree has none
  # yet, and the count only has to be comparable with itself.
  moves="$(git reflog show HEAD 2>/dev/null | wc -l | tr -d ' ')"
  printf '%s %s %s\n' "$sha" "$moves" "$ref"
}

case "${1:-}" in
  --record) record ;;
  --check)
    [ $# -eq 2 ] || usage
    before="$2"
    after="$(record)"
    [ "$before" = "$after" ] && exit 0
    read -r sha_b moves_b ref_b <<<"$before"
    read -r sha_a moves_a ref_a <<<"$after"
    {
      echo "TREE MOVED DURING GATE"
      echo "  This worktree was checked out somewhere else while the gate was"
      echo "  running, so what it measured is not a statement about the branch"
      echo "  it was asked to judge. Nothing below the move can be trusted, and"
      echo "  a push on this verdict would carry unjudged commits."
      echo
      printf '  at the start:  %s on %s, %s reflog entries\n' "$sha_b" "$ref_b" "$moves_b"
      printf '  now:           %s on %s, %s reflog entries\n' "$sha_a" "$ref_a" "$moves_a"
      if [ "$sha_b" = "$sha_a" ] && [ "$ref_b" = "$ref_a" ]; then
        echo
        echo "  It came back to where it started, which is why the commit id alone"
        echo "  says nothing. The reflog is what remembers the trip."
      fi
      echo
      echo "  Find out who else is working in $(pwd), then run the gate again on a"
      echo "  worktree of your own. Do not reach for PRE_PUSH_SKIP=1: that skips"
      echo "  every gate rather than repeating the one that was disturbed."
    } >&2
    exit 1 ;;
  *) usage ;;
esac
