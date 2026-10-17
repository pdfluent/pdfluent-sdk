#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# _a_fixture_inherits_no_git_env.sh -- sourced FIRST by every shell fixture that
# runs git.
#
#     . scripts/ci/_a_fixture_inherits_no_git_env.sh
#
# WHAT HAPPENED, FOUR TIMES IN TWO DAYS, AND IT WAS ONE CAUSE
#
# The shared checkout kept acquiring things no one had typed: `core.bare = true`
# (twice, breaking `git rev-parse` in all seventy-odd worktrees and pruning the
# landing worktree's registration), `remote.github.url =
# https://example.invalid/other.git`, `user.email = gate@invalid`, and
# `refs/remotes/github/master` moved to a fixture's commit -- which made
# `github/master..HEAD` read as zero commits and took a landing's message guard
# down with it. Every value is one a fixture in scripts/ci sets on a repository
# of its own, and none could be reproduced by running that fixture by hand.
#
# THE DOOR. A git hook exports GIT_DIR. The gate runs from the pre-push hook, so
# every fixture it starts inherits `GIT_DIR=<the real repository>/.git` -- and
# GIT_DIR outranks `-C`: `git -C /tmp/fixture config …` reads and writes the
# repository GIT_DIR names, not the one in /tmp/fixture. Worse, `git -C
# /tmp/fixture init` under an inherited GIT_DIR and no work tree RE-INITIALISES
# that repository, and a repository initialised without a work tree is written
# down as `core.bare = true`. That is the whole sequence, in one line of
# environment nobody set on purpose:
#
#   git -C "$pad" init      -> real repo re-inited bare  (core.bare = true)
#   git -C "$pad" config …  -> real repo's config        (gate@invalid)
#   git -C "$pad" update-ref -> real repo's refs         (github/master moved)
#   git -C "$pad" remote set-url -> real repo's remote   (example.invalid)
#
# The python fixtures have been sealed against exactly this since 02-09-2026
# (scripts/ci/fixture_env.py drops every GIT_* and pins the config files), and
# a_fixture_cannot_touch_a_real_repo.py holds them to it -- but that guard globs
# `*.py`. The shell fixtures were never in it, and they are the ones the gate
# runs from inside a hook.
#
# THE SECOND DOOR, SAME SHAPE: EDITOR_LAND* (15-09-2026)
#
# scripts/cos/editor_land.sh names its own log file and then EXPORTS
# EDITOR_LAND_LOG, so that the re-exec of itself under `tee` writes to the log
# it already announced instead of opening a second one. It reads
# EDITOR_LAND_LOG_DIR, EDITOR_LAND_ISSUE and EDITOR_LANDING_LOCK from the
# environment for the same reason.
#
# A fixture whose subject IS a landing therefore starts one from inside a
# landing, and the inner landing sees the outer one's log path already set --
# so it takes the "the caller named the log" branch and never runs the block
# under test. By hand there is no outer landing and the fixture is green; in
# the gate there always is one, and it was red every time. Green by hand and red
# in the gate is the signature of an inherited variable, and this file is where
# that class of variable is dropped.
#
# EDITOR_LAND* rather than EDITOR_LAND_*: EDITOR_LANDING_LOCK steers which lock
# an inner landing waits on, which is the same kind of mistake with a longer
# name. EDITOR_GATE and CARGO_TARGET_DIR are deliberately NOT dropped -- those
# say which gate and which build directory to use, a fixture sets them on the
# call it makes, and a gate lane that sets one means it.
#
# WHAT THIS DOES. Unsets every GIT_* and every EDITOR_LAND* variable, so `-C`
# means what it says, a fixture's git calls reach the fixture's repository, and
# a landing a fixture starts is that fixture's own. A fixture that wants its
# config sealed sets GIT_CONFIG_SYSTEM and GIT_CONFIG_GLOBAL itself, AFTER this
# -- deliberately, at the call that needs it, which is what
# test_gate_instance_prepare.sh does around the script under test; a fixture
# that wants a landing pointed at its own log or lock passes those on the call,
# after this, for the same reason.
#
# Two `sed` expressions rather than one alternation: `\|` is a GNU extension and
# this file runs on the Mac's BSD sed as well as the runner's.
#
# Not `set -u`-hostile and not a subshell: it is sourced, and it must work in a
# fixture that has already set its own options.

for _var in $(env | sed -n -e 's/^\(GIT_[A-Za-z0-9_]*\)=.*/\1/p' \
                           -e 's/^\(EDITOR_LAND[A-Za-z0-9_]*\)=.*/\1/p'); do
  unset "$_var"
done
unset _var
