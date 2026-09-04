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
# WHAT IT REMOVES, AND WHAT IT MUST NOT
#
# Only commit MESSAGES change: the `Co-Authored-By` and `Assisted-by` trailers
# that #229 keeps out of what we publish. Not one byte of any file may differ.
# That is checked rather than assumed -- see the verification below -- because a
# history rewrite that quietly alters content is a far worse failure than the
# trailer it was meant to remove.
#
# IT DOES NOT PUSH BY DEFAULT
#
# Publishing is not a step in a script; it is a decision. Without --push this
# prepares the rewritten mirror, verifies it and prints what would be published.
# With --push it still refuses unless the destination is empty, because seeding
# over an existing history is a different operation that nobody asked for.
set -euo pipefail

BRON="${1-}"
DOEL="${2-}"
PUSH=0
for arg in "$@"; do [ "$arg" = "--push" ] && PUSH=1; done

if [ -z "$BRON" ]; then
    echo "usage: seed_public_repo.sh <source-repo-or-url> [<destination-url>] [--push]" >&2
    exit 2
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

# Record the tree of every ref before the rewrite, so the check afterwards is a
# comparison and not a hope.
git -C "$SPIEGEL" for-each-ref --format='%(refname) %(objecttype)' > "$WERK/refs-before.txt"
git -C "$SPIEGEL" for-each-ref --format='%(refname)' 'refs/heads/*' 'refs/tags/*' \
    | while read -r ref; do
        printf '%s %s\n' "$ref" "$(git -C "$SPIEGEL" rev-parse "$ref^{tree}" 2>/dev/null || echo NOTREE)"
      done > "$WERK/trees-before.txt"

echo "[seed] rewriting commit messages"
# `--msg-filter` touches nothing but the message. Not filter-repo: this must run
# where only git is installed, and the operation is one sed over a message.
# `--tag-name-filter cat` is not decoration. Without it filter-branch rewrites
# the branches and leaves every tag pointing at the OLD commits -- so the trailer
# history stays reachable through the tags, which is exactly what was supposed to
# go. Caught by the test: a source with a tag came out with the trailer still
# there while the script reported success on the branch.
#
# Its output is kept: a filter-branch that fails half way is worth reading, and
# `>/dev/null 2>&1` on this line hid the reason the first time.
FILTER_BRANCH_SQUELCH_WARNING=1 git -C "$SPIEGEL" filter-branch --force \
    --msg-filter "grep -vE '$TRAILERS' || true" \
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

# THE CHECK THAT MATTERS: content is untouched.
git -C "$SPIEGEL" for-each-ref --format='%(refname)' 'refs/heads/*' 'refs/tags/*' \
    | while read -r ref; do
        printf '%s %s\n' "$ref" "$(git -C "$SPIEGEL" rev-parse "$ref^{tree}" 2>/dev/null || echo NOTREE)"
      done > "$WERK/trees-after.txt"

if ! diff -q "$WERK/trees-before.txt" "$WERK/trees-after.txt" >/dev/null; then
    echo "[seed] FAILED: the rewrite changed content, not just messages." >&2
    echo "  Trees that differ:" >&2
    diff "$WERK/trees-before.txt" "$WERK/trees-after.txt" | head -20 >&2
    exit 1
fi
echo "[seed] verified: every ref points at the same tree as before"

REFS="$(git -C "$SPIEGEL" for-each-ref --format='%(refname)' 'refs/heads/*' 'refs/tags/*' | wc -l | tr -d ' ')"
echo "[seed] $REFS ref(s) ready, $VOOR trailer line(s) removed"

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
