#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the public SDK repository out of the private one, once (#229 layer 3,
# #222).
#
# WHY THIS IS A SEEDING STEP AND NOT A PIPELINE
#
# #231 decides that after seeding the public repository IS the working
# repository. So this runs once. A permanent export pipeline would mean two
# repositories that can drift, and drift with no one watching is the thing #231
# exists to prevent.
#
# WHAT IT REMOVES
#
#   1. the `Co-Authored-By` / `Assisted-by` / `Generated-With` trailers (#229)
#   2. every path `docs/PUBLIC_TREE.toml` calls internal, over the WHOLE history
#      and not only the tip
#   3. every personal address in an author or committer field, rewritten to the
#      noreply alias this history already carries
#
# (2) is why this file was rewritten on 05-09-2026. Until then it applied no path
# filter at all: `git push --mirror` of the complete history, with the messages
# cleaned and not one blob dropped. Two mechanisms answered "what goes public" --
# `simulate_public_tree.py` published 2315 of 5279 files, this published all of
# them plus everything ever deleted -- and the one nobody had read was the one
# that would actually have run. Every exclusion #222 rests on would have been
# undone by it, permanently, because a published object stays fetchable by id to
# anyone who has it (#260 measured that four days after a rewrite meant to end it).
#
# (3) is the same argument applied to the identity fields. 6746 commits here carry
# the owner's personal address. #261 decided to leave the addresses in the history
# that is ALREADY public and treat them as published; these are not published, and
# seeding would publish them for the first time. `docs/licensing/history-rewrite-plan.md`
# says that plan "does not touch author addresses", and it is right about the four
# public repositories it covers -- this is the other case, and it is named there as
# a separate operation.
#
# WHAT IT MUST NOT REMOVE
#
# Of the files that stay, not one byte may differ. That was checked by comparing
# every ref's tree before and after; a path filter makes trees differ on purpose,
# so the check is restated rather than dropped -- `seed_history_filter.py trees`
# lists the blob standing at every publishable path, before and after, and the two
# must be identical.
#
# IT DOES NOT PUSH BY DEFAULT
#
# Publishing is not a step in a script; it is a decision. Without --push this
# prepares the rewritten mirror, verifies it and prints what would be published.
# With --push it still refuses unless the destination is empty, because seeding
# over an existing history is a different operation that nobody asked for.
#
# IT NEEDS PYTHON, AND THAT IS NEW
#
# The previous version said it must run where only git is installed. It no longer
# can: the filter is derived from the TOML manifest by the same predicate the tree
# exporter uses, and the alternative is a second hand-kept list of paths that
# drifts from the first. Every publication guard in this repository already needs
# python3; the seeding step is not the place to be the exception.
set -euo pipefail

HIER="$(cd "$(dirname "$0")" && pwd)"
FILTER="$HIER/seed_history_filter.py"

BRON=""
DOEL=""
PUSH=0
ALIAS=""
while [ $# -gt 0 ]; do
    case "$1" in
        --push)  PUSH=1 ;;
        --alias) ALIAS="${2-}"; shift ;;
        --alias=*) ALIAS="${1#--alias=}" ;;
        -*)      echo "[seed] unknown option: $1" >&2; exit 2 ;;
        *)       if [ -z "$BRON" ]; then BRON="$1"; else DOEL="$1"; fi ;;
    esac
    shift
done

if [ -z "$BRON" ]; then
    echo "usage: seed_public_repo.sh <source-repo-or-url> [<destination-url>] [--alias <addr>] [--push]" >&2
    exit 2
fi

if [ ! -f "$FILTER" ]; then
    echo "[seed] REFUSED: $FILTER is missing, so the history would be published" >&2
    echo "  unfiltered. That is the one failure here that cannot be undone." >&2
    exit 1
fi

WERK="$(mktemp -d)"
trap 'rm -rf "$WERK"' EXIT
SPIEGEL="$WERK/mirror.git"

echo "[seed] mirroring $BRON"
git clone --quiet --mirror "$BRON" "$SPIEGEL"

# The trailers, as one alternation. `Assisted-by` is here because #229 names the
# class and not one spelling: a rule that removes exactly one line teaches people
# to write a second one.
TRAILERS='^(Co-[Aa]uthored-[Bb]y|Assisted-[Bb]y|Generated-[Ww]ith):'

VOOR="$(git -C "$SPIEGEL" log --all --format='%H %B' | grep -cE "$TRAILERS" || true)"
echo "[seed] trailer lines in the source history: $VOOR"

# Everything the verification needs, asked now rather than after the rewrite. The
# private term list is the one that is normally absent, and being told so at the
# end of a 6746-commit filter-branch is being told nothing useful.
python3 "$FILTER" preflight "$SPIEGEL"

# ------------------------------------------------------------ what must go ----
#
# Derived from docs/PUBLIC_TREE.toml over every path that has EVER existed here,
# not over `git ls-files`. A file moved into an internal directory last month is
# in the history under its old name, and a filter built from current paths would
# leave it there while reporting success.
UITSLUIT="$WERK/exclude-paths.nul"
INGETROKKEN="$WERK/withdrawn-oids.txt"
python3 "$FILTER" plan "$SPIEGEL" --paths "$UITSLUIT" --withdrawn "$INGETROKKEN"
# The list is NUL-JOINED, not NUL-terminated, so the count is separators + 1 --
# and 0 when the file is empty. Getting this wrong in the other direction would
# make the refusal below unreachable, which is the one line here that has to be.
if [ -s "$UITSLUIT" ]; then
    AANTAL_UIT=$(( $(tr -cd '\0' < "$UITSLUIT" | wc -c | tr -d ' ') + 1 ))
else
    AANTAL_UIT=0
fi

AANTAL_OBJ="$(wc -l < "$INGETROKKEN" | tr -d ' ')"
echo "[seed] $AANTAL_UIT internal path(s) to drop, carrying $AANTAL_OBJ object(s)"
echo "  that occur at no publishable path"

# A filter that matches nothing is the state this script was IN, so it is a
# refusal and not a shrug. If this repository ever genuinely has nothing to
# exclude, the manifest will say so and the line below is where to record it.
if [ "$AANTAL_UIT" = "0" ]; then
    echo "[seed] REFUSED: the manifest excludes nothing from this history." >&2
    echo "  Either docs/PUBLIC_TREE.toml was not read, or the source is not the" >&2
    echo "  repository this filter was written for. Publishing on that assumption" >&2
    echo "  is the failure that cannot be taken back." >&2
    exit 1
fi

# ------------------------------------------------------- what identity stays --
if [ -z "$ALIAS" ]; then
    ALIAS="$(python3 "$FILTER" alias "$SPIEGEL")"
fi
echo "[seed] identities that are not an alias will be rewritten to the alias this"
echo "  history already carries"

# The addresses that may stay, as one `|`-delimited string the env-filter tests
# with a `case` and no subprocess. Decided once here rather than per commit: a
# python call per identity on this history is 13 492 processes, and a rewrite
# nobody is willing to wait for is a rewrite that gets run with the filter off.
# `|a|b|c|`: a leading bar and one from `tr` after the last line. The `case`
# below tests `"|$ADDRESS|"` against it, so an address matches only as a whole
# field. Do NOT append a second bar here: that makes the string end in `||`, and
# an EMPTY address then matches -- the one value `is_allowed` refuses hardest
# would be the one value the filter let through.
TOEGESTAAN="|$(python3 "$FILTER" allowed "$SPIEGEL" | tr '\n' '|')"

echo "[seed] rewriting: messages, internal paths, identities"
# ONE filter-branch pass and not three. Each pass rewrites every commit and each
# one is minutes on this history; worse, three passes means three chances for the
# refs to end up in a state the checks below no longer describe.
#
# `--index-filter` and not `--tree-filter`: the tree filter checks every commit
# out to disk, which on 6746 commits is hours. The index one edits the index.
#
# `--tag-name-filter cat` is not decoration. Without it filter-branch rewrites
# the branches and leaves every tag pointing at the OLD commits -- so the trailer
# history stays reachable through the tags, which is exactly what was supposed to
# go. Caught by the test: a source with a tag came out with the trailer still
# there while the script reported success on the branch.
#
# Its output is kept: a filter-branch that fails half way is worth reading, and
# `>/dev/null 2>&1` on this line hid the reason the first time.
#
# Not `--prune-empty`: a commit that only touched internal paths becomes empty,
# and filter-branch's pruning is unreliable across merges. An empty commit
# publishes nothing; a mangled merge graph publishes a history nobody can read.
python3 "$FILTER" trees "$SPIEGEL" | sort > "$WERK/trees-before.txt"

FILTER_BRANCH_SQUELCH_WARNING=1 git -C "$SPIEGEL" filter-branch --force \
    --index-filter "git rm -r --cached --quiet --ignore-unmatch \
        --pathspec-from-file='$UITSLUIT' --pathspec-file-nul || true" \
    --msg-filter "grep -vE '$TRAILERS' || true" \
    --env-filter "
        case \"$TOEGESTAAN\" in
            *\"|\$GIT_AUTHOR_EMAIL|\"*) ;;
            *) GIT_AUTHOR_EMAIL='$ALIAS' ;;
        esac
        case \"$TOEGESTAAN\" in
            *\"|\$GIT_COMMITTER_EMAIL|\"*) ;;
            *) GIT_COMMITTER_EMAIL='$ALIAS' ;;
        esac
    " \
    --tag-name-filter cat \
    -- --all

# filter-branch keeps the pre-rewrite refs under refs/original/. They carry
# exactly the history that was supposed to go -- and `push --mirror` pushes every
# ref, so leaving them would publish the trailers under a different name while
# the script reported success. Drop them, then expire what the reflog still
# holds, then count.
# `refs/original` and not `refs/original/*`: git's ref patterns match whole path
# components, so the glob catches `refs/original/foo` and misses
# `refs/original/refs/heads/master`, which is the only shape that actually
# occurs. The first version of this line deleted nothing and said nothing.
git -C "$SPIEGEL" for-each-ref --format='delete %(refname)' 'refs/original' \
    | git -C "$SPIEGEL" update-ref --stdin
git -C "$SPIEGEL" reflog expire --expire=now --all >/dev/null 2>&1 || true
git -C "$SPIEGEL" gc --prune=now --quiet >/dev/null 2>&1 || true

NA="$(git -C "$SPIEGEL" log --all --format='%H %B' | grep -cE "$TRAILERS" || true)"
echo "[seed] trailer lines after the rewrite: $NA"

if [ "$NA" != "0" ]; then
    echo "[seed] FAILED: $NA trailer line(s) survived the rewrite." >&2
    exit 1
fi

# THE CHECK THAT MATTERS: of the files that stay, content is untouched.
python3 "$FILTER" trees "$SPIEGEL" | sort > "$WERK/trees-after.txt"
if ! diff -q "$WERK/trees-before.txt" "$WERK/trees-after.txt" >/dev/null; then
    echo "[seed] FAILED: the rewrite changed a file that was supposed to stay." >&2
    echo "  Lines that differ (blob id and path, per ref):" >&2
    diff "$WERK/trees-before.txt" "$WERK/trees-after.txt" | head -20 >&2
    exit 1
fi
echo "[seed] verified: every publishable path still holds the same blob"

# THE CHECK THAT CANNOT BE UNDONE IF IT IS WRONG: no internal path, no withdrawn
# object under any name, no personal address, no internal term.
if ! python3 "$FILTER" verify "$SPIEGEL" --withdrawn "$INGETROKKEN"; then
    echo "[seed] FAILED: the rewritten history is not publishable. Nothing was" >&2
    echo "  pushed. Fix the manifest or the tree, then run this again." >&2
    exit 1
fi

REFS="$(git -C "$SPIEGEL" for-each-ref --format='%(refname)' 'refs/heads/*' 'refs/tags/*' | wc -l | tr -d ' ')"
echo "[seed] $REFS ref(s) ready, $VOOR trailer line(s) and $AANTAL_UIT internal path(s) removed"

if [ "$PUSH" != "1" ]; then
    echo "[seed] not pushing. Re-run with a destination and --push to publish."
    exit 0
fi

if [ -z "$DOEL" ]; then
    echo "[seed] --push needs a destination." >&2
    exit 2
fi

# Seeding over an existing history is a different operation, and not this one.
if [ -n "$(git ls-remote --heads "$DOEL" 2>/dev/null)" ]; then
    echo "[seed] REFUSED: $DOEL already has branches." >&2
    echo "  This script seeds an empty repository. Overwriting a published" >&2
    echo "  history is a decision somebody has to make on purpose." >&2
    exit 1
fi

echo "[seed] pushing to $DOEL"
git -C "$SPIEGEL" push --mirror "$DOEL"
echo "[seed] done"
