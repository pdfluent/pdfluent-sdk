#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# mirror_to_gitlab.sh -- push the source branch onto the backup, and refuse to
# do it when that would destroy work.
#
# WHY THIS EXISTS AS A SCRIPT AND NOT AS A CRON LINE
#
# The nightly mirror ran from a cron entry on the WSL desktop. On 02-09-2026 it
# stopped, and by 05-09 the backup was 348 commits behind and 8 commits ahead --
# three days in which nothing said so, because the only thing that could have
# said so ran on the machine that had stopped. A schedule on a host nobody can
# reach is not a backup arrangement; it is a belief about one.
#
# So the schedule is not restored. What replaces it is this script, run by hand
# or from the pre-push gate's report, plus `scripts/ci/mirror_has_not_drifted.py`
# in the landing lane -- which turns a mirror nobody pushed into a red gate in
# front of the next person landing, rather than into silence. The reason is in
# docs/ci/mirror.md and the roles are in CLAUDE.md.
#
# WHAT IT REFUSES
#
# A backup does not gain commits of its own. If the mirror is ahead, someone has
# been working there and a push in the agreed direction would destroy that work
# -- which is exactly the shape that cost six days in #265. This refuses, names
# the commits, and exits 1. `--archive` is the way through: it publishes the
# mirror's current tip as a tag on the mirror first, so the commits survive the
# push and can be read back afterwards.
#
# WHY IT PUSHES WITH --no-verify
#
# The pre-push hook judges code on its way to master. What travels here is the
# source tip, which passed that gate when it landed; the destination is a copy,
# and running the twenty-minute compiling lane again to republish commits that
# already passed it would judge nothing new. The script's own precondition is
# what makes that safe: it pushes the fetched tip of the SOURCE remote and
# nothing else, so there is no path by which unreviewed work reaches the mirror
# through this file.
set -uo pipefail

SOURCE_REMOTE="${MIRROR_SOURCE_REMOTE:-github}"
TARGET_REMOTE="${MIRROR_TARGET_REMOTE:-origin}"
BRANCH="${MIRROR_BRANCH:-master}"

ARCHIVE=0
DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    --archive) ARCHIVE=1 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help)
      echo "usage: mirror_to_gitlab.sh [--archive] [--dry-run]"
      echo "  --archive  tag the mirror's current tip on the mirror before overwriting it"
      echo "  --dry-run  say what would happen; touch no remote"
      exit 0 ;;
    *) echo "mirror_to_gitlab: unknown argument '$arg'" >&2; exit 2 ;;
  esac
done

cd "$(git rev-parse --show-toplevel)" || exit 2

say() { printf '[mirror-sync] %s\n' "$*"; }
die() { printf '[mirror-sync] FATAL: %s\n' "$*" >&2; exit 1; }

# Fetch first, both sides. Comparing the refs already on disk answers a question
# about the last time somebody fetched, and that is the question this whole
# arrangement got wrong once already.
for remote in "$SOURCE_REMOTE" "$TARGET_REMOTE"; do
  if ! git fetch --quiet "$remote" "$BRANCH" 2>/dev/null; then
    die "cannot fetch ${BRANCH} from '${remote}'. Nothing was pushed. Without a
  fresh read of both sides this script would be working from whatever the refs
  on disk happened to say, which may be days old."
  fi
done

SOURCE_TIP="$(git rev-parse --verify --quiet "${SOURCE_REMOTE}/${BRANCH}" || true)"
TARGET_TIP="$(git rev-parse --verify --quiet "${TARGET_REMOTE}/${BRANCH}" || true)"
[ -n "$SOURCE_TIP" ] || die "${SOURCE_REMOTE}/${BRANCH} cannot be read after a successful fetch."
[ -n "$TARGET_TIP" ] || die "${TARGET_REMOTE}/${BRANCH} cannot be read after a successful fetch."

BEHIND="$(git rev-list --count "${TARGET_TIP}..${SOURCE_TIP}")"
AHEAD="$(git rev-list --count "${SOURCE_TIP}..${TARGET_TIP}")"
say "source ${SOURCE_REMOTE}/${BRANCH} = ${SOURCE_TIP}"
say "mirror ${TARGET_REMOTE}/${BRANCH} = ${TARGET_TIP} (${BEHIND} behind, ${AHEAD} ahead)"

if [ "$BEHIND" = 0 ] && [ "$AHEAD" = 0 ]; then
  say "already equal; nothing to push."
  exit 0
fi

ARCHIVE_TAG="archive/mirror-${BRANCH}-${TARGET_TIP:0:8}"
if [ "$AHEAD" != 0 ]; then
  say "the mirror carries ${AHEAD} commit(s) the source does not:"
  git log --format='    %h %ad %an  %s' --date=short "${SOURCE_TIP}..${TARGET_TIP}"
  if [ "$ARCHIVE" != 1 ]; then
    die "the mirror is ${AHEAD} commit(s) ahead. Pushing now would destroy those
  commits, which is the failure this arrangement exists to prevent (#265, #231).
  Read them, decide what they are, and then either bring them onto the source or
  re-run with --archive, which tags them on the mirror as ${ARCHIVE_TAG} before
  overwriting the branch."
  fi
  say "--archive given: publishing them as ${ARCHIVE_TAG} on ${TARGET_REMOTE} first."
  if [ "$DRY_RUN" = 1 ]; then
    say "dry run: would push ${TARGET_TIP} to refs/tags/${ARCHIVE_TAG} on ${TARGET_REMOTE}"
  else
    git push --no-verify "$TARGET_REMOTE" "${TARGET_TIP}:refs/tags/${ARCHIVE_TAG}" \
      || die "could not publish the archive tag. Nothing was overwritten."
    # Read it back off the remote. A push that reports success and leaves no ref
    # behind is the whole reason this file verifies instead of announcing.
    git ls-remote --exit-code --tags "$TARGET_REMOTE" "refs/tags/${ARCHIVE_TAG}" >/dev/null \
      || die "the archive tag is not on ${TARGET_REMOTE} after pushing it. Nothing
  was overwritten; the commits are still only on the mirror branch."
    say "archived: ${ARCHIVE_TAG} on ${TARGET_REMOTE} -> ${TARGET_TIP}"
  fi
fi

if [ "$DRY_RUN" = 1 ]; then
  say "dry run: would push ${SOURCE_TIP} to refs/heads/${BRANCH} on ${TARGET_REMOTE}"
  exit 0
fi

# --force-with-lease pinned to the tip we measured, so a mirror that moved
# between the fetch and this line is refused rather than overwritten. It is the
# same argument as the AHEAD check, for the seconds this script is running in.
git push --no-verify --force-with-lease="refs/heads/${BRANCH}:${TARGET_TIP}" \
  "$TARGET_REMOTE" "${SOURCE_TIP}:refs/heads/${BRANCH}" \
  || die "the push to ${TARGET_REMOTE} failed. The mirror is unchanged."

# Verified at the far side, never "the push seemed to work".
git fetch --quiet "$TARGET_REMOTE" "$BRANCH" 2>/dev/null \
  || die "pushed, but ${TARGET_REMOTE} cannot be re-read to confirm it."
NEW_TIP="$(git rev-parse --verify --quiet "${TARGET_REMOTE}/${BRANCH}" || true)"
[ "$NEW_TIP" = "$SOURCE_TIP" ] \
  || die "after the push ${TARGET_REMOTE}/${BRANCH} is ${NEW_TIP}, not ${SOURCE_TIP}."

say "OK: ${TARGET_REMOTE}/${BRANCH} is now ${SOURCE_TIP}."
exit 0
