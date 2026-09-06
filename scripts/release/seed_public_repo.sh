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
#   4. every internal term named in the reviewed replacement list, in file content
#      and in commit messages alike
#   5. every workspace member in the root `Cargo.toml` whose directory does not
#      travel, because a workspace that names a crate it does not carry does not
#      parse -- see seed_history_filter.py, "A WORKSPACE MEMBER THAT IS NOT THERE"
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
# (4) is the class (2) cannot reach. Measured over `github/master` on 05-09-2026,
# after the path filter had done its work: 63 internal terms left in file content
# and 24 in commit messages -- an old `pom.xml` naming the private group URL, a
# design note naming a partner, a build note naming the desktop's hostname. There
# is no path to exclude, because those files have to go out; the string itself has
# to go. The list of what may be replaced with what is reviewed by a person and
# read from outside the tree (`seed_history_filter.py`, "A TERM IS NOT A PATH").
# Without a list nothing is rewritten and the seeding refuses on the terms, which
# is what it did before and is the behaviour that asks for eyes.
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
# Of the files that stay, not one byte may differ -- except where a reviewed
# replacement or the workspace-member edit says otherwise, and then it must be
# exactly that byte. That was checked by comparing every ref's tree before and
# after; a path filter makes trees differ on purpose, so the check is restated
# rather than dropped -- `seed_history_filter.py trees` lists the blob standing at
# every publishable path, before and after, with `--map` naming the blob the
# rewrite is EXPECTED to leave there, and the two must be identical.
#
# IT DOES NOT PUSH BY DEFAULT
#
# Publishing is not a step in a script; it is a decision. Without --push this
# prepares the rewritten mirror, verifies it and prints what would be published.
# With --push it still refuses unless the destination is empty, because seeding
# over an existing history is a different operation that nobody asked for.
#
# THE RUN THAT VERIFIES AND THE RUN THAT PUBLISHES ARE THE SAME RUN
#
# `--keep <dir>` writes the verified mirror out instead of deleting it, and
# `--publish <dir> <destination>` pushes that mirror after verifying it again.
# The two exist because #222 asks for the seeded result to be checked by hand
# before it is published, and there were only two other ways to arrange that:
# check the dry run and then push a SECOND rewrite -- which is not the artefact
# anybody looked at -- or push first and read afterwards, which is the one order
# that cannot be undone. Measured on this history the rewrite is about two and a
# half hours, so "just run it twice" is also how the checking stops happening.
#
# `--publish` re-runs the verification rather than trusting the directory. A
# mirror on disk between two commands is a mirror somebody can edit, and the
# check is cheap next to what it guards.
#
# --branch: THE SEEDED BRANCH IS NOT ALWAYS THE SOURCE'S BRANCH
#
# This history's branch is `master` and the public repository's default branch is
# `main` (#222). `git push --mirror` would create `master` there and leave `main`
# a default pointing at nothing -- a repository whose front page is empty. The
# rename is a decision, so it is an argument and not a default.
#
# --replace: A DESTINATION THAT IS NOT EMPTY
#
# The refusal above is the right default and stays it. `pdfluent/pdfluent-sdk`
# holds an eight-commit placeholder (a README, a LICENSE from before #220, and
# five scripts), and GitHub does not let its default branch be deleted, so there
# is no way to hand this script an empty destination there. #222 decided that
# placeholder is replaced by the seeding. `--replace` says so out loud, prints
# what is about to be overwritten, and is the only way past the refusal.
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
BEWAAR=""
TAK=""
PUBLICEER=0
VERVANG=0
while [ $# -gt 0 ]; do
    case "$1" in
        --push)  PUSH=1 ;;
        --publish) PUBLICEER=1 ;;
        --replace) VERVANG=1 ;;
        --alias) ALIAS="${2-}"; shift ;;
        --alias=*) ALIAS="${1#--alias=}" ;;
        --keep)  BEWAAR="${2-}"; shift ;;
        --keep=*) BEWAAR="${1#--keep=}" ;;
        --branch) TAK="${2-}"; shift ;;
        --branch=*) TAK="${1#--branch=}" ;;
        -*)      echo "[seed] unknown option: $1" >&2; exit 2 ;;
        *)       if [ -z "$BRON" ]; then BRON="$1"; else DOEL="$1"; fi ;;
    esac
    shift
done

GEBRUIK="usage: seed_public_repo.sh <source-repo-or-url> [<destination-url>] \
[--alias <addr>] [--branch <name>] [--keep <dir>] [--push]
   or: seed_public_repo.sh --publish <kept-mirror-dir> <destination-url> [--replace]"

if [ -z "$BRON" ]; then
    echo "$GEBRUIK" >&2
    exit 2
fi

# The list of object ids that must not be reachable, written beside a kept mirror
# so `--publish` verifies against the same list the rewrite was planned with.
# Regenerating it from the rewritten mirror would answer a different question:
# the rewrite removed those paths, so the plan would come back empty and the
# check would pass by having nothing to look for.
INGETROKKEN_NAAST="withdrawn-oids.txt"

if [ ! -f "$FILTER" ]; then
    echo "[seed] REFUSED: $FILTER is missing, so the history would be published" >&2
    echo "  unfiltered. That is the one failure here that cannot be undone." >&2
    exit 1
fi

# Seeding over an existing history is a different operation, and not this one.
# `--replace` is the one way past it, and it says what it is overwriting first --
# a count read off the destination and not off the operator's memory of it.
doel_is_bruikbaar() {
    _doel="$1"
    _bestaand="$(git ls-remote --heads "$_doel" 2>/dev/null || true)"
    if [ -z "$_bestaand" ]; then
        return 0
    fi
    if [ "$VERVANG" != "1" ]; then
        echo "[seed] REFUSED: $_doel already has branches." >&2
        echo "  This script seeds an empty repository. Overwriting a published" >&2
        echo "  history is a decision somebody has to make on purpose -- say so" >&2
        echo "  with --replace." >&2
        return 1
    fi
    echo "[seed] --replace: $_doel is not empty and will be overwritten. What is"
    echo "  there now:"
    printf '%s\n' "$_bestaand" | sed 's/^/    /'
    return 0
}

if [ "$PUBLICEER" = "1" ]; then
    # BRON is the kept mirror, DOEL the destination. Nothing is rewritten here:
    # this publishes bytes that already exist, and its whole job is to prove they
    # are still the bytes that were verified before it pushes them.
    if [ -z "$DOEL" ]; then
        echo "[seed] --publish needs a destination." >&2
        exit 2
    fi
    if [ ! -d "$BRON" ] || [ ! -e "$BRON/HEAD" ]; then
        echo "[seed] REFUSED: $BRON is not a bare repository, so there is nothing" >&2
        echo "  verified to publish. Run the seeding with --keep first." >&2
        exit 1
    fi
    LIJST="$BRON/$INGETROKKEN_NAAST"
    if [ ! -s "$LIJST" ]; then
        echo "[seed] REFUSED: $LIJST is missing or empty, so the withdrawn objects" >&2
        echo "  cannot be checked. A mirror kept by this script carries that list;" >&2
        echo "  one that does not is not a mirror this script verified." >&2
        exit 1
    fi
    echo "[seed] re-verifying $BRON before publishing it"
    if ! python3 "$FILTER" verify "$BRON" --withdrawn "$LIJST"; then
        echo "[seed] FAILED: the kept mirror is not publishable. Nothing was pushed." >&2
        exit 1
    fi
    doel_is_bruikbaar "$DOEL" || exit 1
    echo "[seed] pushing to $DOEL"
    git -C "$BRON" push --mirror "$DOEL"
    echo "[seed] done"
    exit 0
fi

if [ -n "$BEWAAR" ] && [ -e "$BEWAAR" ]; then
    echo "[seed] REFUSED: --keep $BEWAAR already exists. Publishing the wrong" >&2
    echo "  mirror is exactly the mistake an overwrite here would cause." >&2
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

# ------------------------------------------------------ what content changes --
#
# Done BEFORE filter-branch starts: the rewritten blobs are written into the
# mirror here, so the rewrite itself only swaps an object id in the index. A
# per-commit content rewrite over this history is hours; a table lookup is not.
KAART="$WERK/blob-map.txt"
REDACT="$WERK/redact.sed"
python3 "$FILTER" replacements "$SPIEGEL" --map "$KAART" --sed "$REDACT"

# The index filter, in a file rather than in a quoted one-liner. It runs once per
# commit, it has to contain an awk program, and a program that has been escaped
# twice through a double-quoted shell string is a program nobody can read or
# check -- which is not the property to want in the one step here that cannot be
# undone.
INDEXFILTER="$WERK/index-filter.sh"
cat > "$INDEXFILTER" <<INDEXEOF
git rm -r --cached --quiet --ignore-unmatch \
    --pathspec-from-file='$UITSLUIT' --pathspec-file-nul || true
# Swap every blob the replacement stage rewrote, wherever it stands in this
# commit's index. \`ls-files -s\` prints "<mode> <oid> <stage>\t<path>", which is
# exactly what \`update-index --index-info\` reads back, so only the id changes.
if [ -s '$KAART' ]; then
    git ls-files -s | awk -F'\t' -v kaart='$KAART' '
        BEGIN { while ((getline r < kaart) > 0) { split(r, a, " "); m[a[1]] = a[2] } }
        { split(\$1, h, " ")
          if (h[2] in m) printf "%s %s %s\t%s\n", h[1], m[h[2]], h[3], \$2 }
    ' | git update-index --index-info
fi
INDEXEOF

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
#
# `--map`: the blob a path is EXPECTED to hold afterwards. The content rewrite
# changes surviving files on purpose, so the claim is narrowed once more rather
# than dropped -- every publishable path holds either the blob it held, or that
# blob's reviewed replacement, and nothing else.
python3 "$FILTER" trees "$SPIEGEL" --map "$KAART" | sort > "$WERK/trees-before.txt"

FILTER_BRANCH_SQUELCH_WARNING=1 git -C "$SPIEGEL" filter-branch --force \
    --index-filter "sh '$INDEXFILTER'" \
    --msg-filter "grep -vE '$TRAILERS' | LC_ALL=C sed -E -f '$REDACT'" \
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
    echo "[seed] FAILED: the rewrite changed a file in a way the replacement list" >&2
    echo "  does not account for." >&2
    echo "  Lines that differ (blob id and path, per ref):" >&2
    diff "$WERK/trees-before.txt" "$WERK/trees-after.txt" | head -20 >&2
    exit 1
fi
echo "[seed] verified: every publishable path holds the blob it held, or its"
echo "  reviewed replacement, and nothing else"

# THE CHECK THAT CANNOT BE UNDONE IF IT IS WRONG: no internal path, no withdrawn
# object under any name, no personal address, no internal term -- and, because
# what a reader meets first is `cargo build`, no root manifest naming a workspace
# member the manifest keeps in-house.
if ! python3 "$FILTER" verify "$SPIEGEL" --withdrawn "$INGETROKKEN"; then
    echo "[seed] FAILED: the rewritten history is not publishable. Nothing was" >&2
    echo "  pushed. Fix the manifest or the tree, then run this again." >&2
    exit 1
fi

# The rename, and here rather than earlier: `trees` prints the ref name in every
# row, so renaming before the before/after comparison would make every row differ
# and the script would report that the rewrite changed every file it kept.
# Nothing above this line reads a branch by name.
if [ -n "$TAK" ]; then
    HUIDIG="$(git -C "$SPIEGEL" symbolic-ref --quiet --short HEAD || true)"
    if [ -z "$HUIDIG" ]; then
        echo "[seed] REFUSED: the mirror has no current branch, so there is nothing" >&2
        echo "  --branch could rename. Seed from a source whose HEAD is a branch." >&2
        exit 1
    fi
    if [ "$HUIDIG" != "$TAK" ]; then
        # `branch -m` moves HEAD with the ref, which is what makes the destination
        # default branch land on something that exists.
        git -C "$SPIEGEL" branch -m "$HUIDIG" "$TAK"
        echo "[seed] branch $HUIDIG renamed to $TAK, and HEAD points at it"
    fi
fi

REFS="$(git -C "$SPIEGEL" for-each-ref --format='%(refname)' 'refs/heads/*' 'refs/tags/*' | wc -l | tr -d ' ')"
echo "[seed] $REFS ref(s) ready, $VOOR trailer line(s) and $AANTAL_UIT internal path(s) removed"

# Kept before the push and not after it, so a mirror exists to look at even when
# the push is the thing that fails.
if [ -n "$BEWAAR" ]; then
    mkdir -p "$(dirname "$BEWAAR")"
    cp -R "$SPIEGEL" "$BEWAAR"
    cp "$INGETROKKEN" "$BEWAAR/$INGETROKKEN_NAAST"
    echo "[seed] verified mirror kept at $BEWAAR"
    echo "  publish it with: seed_public_repo.sh --publish $BEWAAR <destination>"
fi

if [ "$PUSH" != "1" ]; then
    echo "[seed] not pushing. Re-run with a destination and --push to publish."
    exit 0
fi

if [ -z "$DOEL" ]; then
    echo "[seed] --push needs a destination." >&2
    exit 2
fi

doel_is_bruikbaar "$DOEL" || exit 1

echo "[seed] pushing to $DOEL"
git -C "$SPIEGEL" push --mirror "$DOEL"
echo "[seed] done"
