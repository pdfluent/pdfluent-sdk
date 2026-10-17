#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# _the_checkout_is_unchanged.sh -- a shell fixture must leave the repository it
# was started from exactly as it found it.
#
#     BASELINE="$(bash scripts/ci/_the_checkout_is_unchanged.sh --record)"
#     ... the fixture runs ...
#     bash scripts/ci/_the_checkout_is_unchanged.sh --check "$BASELINE"
#
# Underscore-prefixed because it is a library and not a guard: run on its own it
# records and checks in the same breath and would pass every time. What runs in
# CI is the fixture that uses it, and a_fixture_cannot_touch_a_real_repo.py is
# what makes sure the shell fixtures do.
#
# WHAT HAPPENED, TWICE IN ONE AFTERNOON
#
# 09-09-2026. `core.bare = true` appeared in the .git/config of the main
# checkout, and `git rev-parse` then failed in every one of its worktrees --
# there are around thirty, one per terminal, all sharing that file. Some hours
# later the `github` remote of the same checkout was pointing at
# https://example.invalid/other.git, which is a URL that exists in exactly one
# place in this repository: a fixture in test_gate_instance_prepare.sh that
# means to set it on a throwaway repository of its own. Every fetch and every
# push from every worktree failed with "Could not resolve host".
#
# Neither could be reproduced by running the suspected fixture on its own
# afterwards, which is the whole argument for this file. The python fixtures
# have been sealed against this since 02-09 (scripts/ci/fixture_env.py:
# GIT_CONFIG_GLOBAL, GIT_CONFIG_NOSYSTEM, no GIT_* inherited) and
# a_fixture_cannot_touch_a_real_repo.py holds them to it -- but that guard globs
# `*.py`, and the shell fixtures were never in it. A fixture written in bash
# reaches the real checkout through the same two doors as one written in python:
# a `cd` that failed and a git call with no `-C`.
#
# WHY A SENTINEL AND NOT A SEAL. A shell fixture cannot be sealed the way a
# python one is: it drives scripts that take their repository from the current
# directory, so the environment has to stay real. What it can do is say, at the
# end, whether the repository it started in still has the configuration it had
# at the start -- and name itself when it does not, which is the thing that was
# missing when this took two people an afternoon.
#
# TWO MORE FIELDS SINCE 10-09-2026 (#454), and both were leaks this file watched
# happen without a word. `refs/remotes/github/master`: a fixture's `update-ref`
# moved it to its own commit, so `github/master..HEAD` read as zero commits and
# a landing's message guard failed about a branch that was fine. And the count
# of PRUNABLE worktrees: with core.bare briefly true, `git worktree list` called
# all seventy of them prunable, and the registration of the shared landing
# worktree was pruned away -- the config was back to false by the time anybody
# looked, and the damage was in a file the ten fields did not name.
#
# The fields are the ones that broke, each of them at least once on 09-09-2026:
# whether the checkout thinks it is bare, where its main remote points, where
# `origin` points -- a dozen guards fall back on origin -- and the identity a
# commit from this checkout would carry. The last one is not the checkout's own
# file: `git config --system` writes to the machine, which no `cd` protects, and
# a fixture that ran the gate's preparation script here left
# `gate@pdfluent.invalid` in /usr/local/etc/gitconfig as the fallback identity
# of every repository on the machine. Reading the EFFECTIVE value catches it
# wherever it was written.
set -uo pipefail

wortel="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -z "$wortel" ]; then
  # Not a refusal: a fixture may run somewhere that is not a checkout at all,
  # and this file has nothing to say about that. It must not be the reason a
  # test fails.
  echo "geen-checkout"
  exit 0
fi

# THE MACHINE'S FILES ARE READ BY NAME, not through the effective value. A
# `git config --system` write is invisible to `git config --get` for as long as
# the operator has a global identity that outranks it -- which is how
# `gate@pdfluent.invalid` sat in /usr/local/etc/gitconfig unnoticed while every
# commit still carried the right address. It becomes the identity of every
# repository on the machine that has no other, which is every fresh clone and
# every fixture.
lees() {
  printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s' \
    "$wortel" \
    "$(git -C "$wortel" config --get core.bare 2>/dev/null)" \
    "$(git -C "$wortel" config --get remote.github.url 2>/dev/null)" \
    "$(git -C "$wortel" config --get remote.origin.url 2>/dev/null)" \
    "$(git -C "$wortel" config --get user.email 2>/dev/null)" \
    "$(git -C "$wortel" config --get user.name 2>/dev/null)" \
    "$(git config --system --get user.email 2>/dev/null)" \
    "$(git config --system --get user.name 2>/dev/null)" \
    "$(git config --global --get user.email 2>/dev/null)" \
    "$(git config --global --get user.name 2>/dev/null)" \
    "$(git -C "$wortel" rev-parse --verify --quiet refs/remotes/github/master 2>/dev/null)" \
    "$(git -C "$wortel" worktree list --porcelain 2>/dev/null | grep -c '^prunable ')"
}

case "${1:---record}" in
  --record) lees; echo ;;
  --check)
    was="${2:-}"
    nu="$(lees)"
    [ "$was" = "geen-checkout" ] && exit 0
    if [ "$was" = "$nu" ]; then exit 0; fi
    # Loud, and on stderr, with both readings: the reader is looking at a test
    # that passed its own cases and left the machine broken for every other
    # terminal, and the difference is the only thing that says which field.
    echo "THE CHECKOUT WAS CHANGED BY THIS TEST." >&2
    echo "  before: $was" >&2
    echo "  after:  $nu" >&2
    echo "  (fields: toplevel|core.bare|remote.github.url|remote.origin.url|user.email|user.name|system user.email|system user.name|global user.email|global user.name|github/master|prunable worktrees)" >&2
    echo "  Every worktree of that checkout shares this file, and the system and" >&2
    echo "  global files are the whole machine's. Put the fixture's git calls" >&2
    echo "  behind 'git -C \$fixture', make the directory it works in a hard" >&2
    echo "  failure rather than a fall-through to the current one, and run any" >&2
    echo "  script that writes config with GIT_CONFIG_SYSTEM and GIT_CONFIG_GLOBAL" >&2
    echo "  pointed at throwaway files." >&2
    exit 1 ;;
  *) echo "usage: _the_checkout_is_unchanged.sh --record | --check <baseline>" >&2; exit 2 ;;
esac
