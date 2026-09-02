#!/bin/sh
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
#
# Which commits the internal-matter guard vouches for, per event.
#
# Prints one git range on stdout and exits 0, or prints `SKIPPED (not a pass)`
# on stderr and exits 1. There is no third outcome: an event this script does
# not know is not a pass, because the only wrong answer here is a range that
# is empty or misses the commits the event added -- the guard then either
# refuses (empty) or approves without reading (missing), and the second one
# is invisible.
#
#   pull_request  origin/master..HEAD   the checkout sits on the merge commit,
#                                       so this is exactly the branch
#   push          $PUSH_BEFORE..HEAD    exactly the commits the push added; a
#                                       first push has no `before` (all zeros)
#                                       and that is not a range
#
# Inputs come through the environment (EVENT, PUSH_BEFORE), never as
# arguments, so the workflow can pass `${{ github.* }}` values through `env:`
# and nothing from the event reaches a shell line by interpolation.
#
# On a pull request the job runs THIS FILE from the base revision, like the
# guard itself, so a pull request cannot pick its own range. Tested by
# test_internal_matter_range.py, which runs in the same job.

set -u

case "${EVENT:-}" in
  pull_request)
    printf '%s\n' "origin/master..HEAD"
    ;;
  push)
    case "${PUSH_BEFORE:-}" in
      ""|0000000000000000000000000000000000000000)
        echo "SKIPPED (not a pass): push without a previous revision, the range it added is unknown" >&2
        exit 1
        ;;
    esac
    printf '%s\n' "${PUSH_BEFORE}..HEAD"
    ;;
  *)
    echo "SKIPPED (not a pass): no range is defined for event '${EVENT:-}', so nothing was checked" >&2
    exit 1
    ;;
esac
