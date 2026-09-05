#!/usr/bin/env bash
# cleanup_agent_worktrees.sh — reclaim disk by removing target/ inside
# stale agent worktrees whose branches are confirmed pushed to GitLab.
#
# Default: dry-run. Pass --apply to actually remove.
#
# Safety:
#   - Never touches the worktree directory itself, only its target/ build dir.
#   - Never touches a worktree whose branch is NOT on the gitlab remote.
#   - Never touches a worktree marked "locked" in `git worktree list`.
#   - Never touches the main worktree (the repo root).
#   - Honors infra deny-list and fail-closed unknown-path policy.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$HERE/_lib.sh"
infra_parse_args "$@"

INFRA_RECLAIM_BYTES=0
REVIEWED=0
SKIPPED_NO_REMOTE=0
SKIPPED_LOCKED=0
SKIPPED_MAIN=0
CLEANED=0

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$REPO_ROOT" ]]; then
  echo "Not inside a git repo." >&2
  exit 2
fi

# Determine the canonical repo root that owns all worktrees.
COMMON_DIR="$(git -C "$REPO_ROOT" rev-parse --git-common-dir)"
MAIN_REPO="$(dirname "$COMMON_DIR")"
# If the common dir IS .git itself, the parent is the main worktree.
[[ "$(basename "$COMMON_DIR")" == ".git" ]] || MAIN_REPO="$REPO_ROOT"

echo "Main repo: $(infra_redact_path "$MAIN_REPO")"
echo "Mode:      $([[ "$INFRA_APPLY" == "1" ]] && echo APPLY || echo DRY-RUN)"
echo

# Discover the GitLab remote by URL (could be named "gitlab" or "origin").
GITLAB_REMOTE="$(git -C "$MAIN_REPO" remote -v 2>/dev/null \
  | awk '/gitlab\.com.*\(fetch\)/{print $1; exit}')"
if [[ -z "$GITLAB_REMOTE" ]]; then
  echo "WARN: no gitlab.com remote found — no worktrees will be cleaned." >&2
fi
gitlab_branches=""
[[ -n "${GITLAB_REMOTE:-}" ]] && gitlab_branches="$(git -C "$MAIN_REPO" ls-remote --heads "${GITLAB_REMOTE}" 2>/dev/null \
  | awk '{print $2}' | sed 's@^refs/heads/@@' || true)"

# Walk every worktree EXCEPT the main one.
while IFS= read -r line; do
  # Format: <path> <sha> [<branch>] [locked]
  path="$(echo "$line" | awk '{print $1}')"
  rest="$(echo "$line" | cut -d' ' -f2-)"
  branch=""
  if [[ "$rest" =~ \[([^]]+)\] ]]; then
    branch="${BASH_REMATCH[1]}"
  fi
  locked=0
  if echo "$rest" | grep -q "locked"; then locked=1; fi

  if [[ "$path" == "$MAIN_REPO" ]]; then
    SKIPPED_MAIN=$((SKIPPED_MAIN+1))
    continue
  fi
  if (( locked )); then
    SKIPPED_LOCKED=$((SKIPPED_LOCKED+1))
    echo "  [locked] $(infra_redact_path "$path")  branch=$branch"
    continue
  fi
  if [[ -z "$branch" || "$branch" == "(detached" ]]; then
    SKIPPED_NO_REMOTE=$((SKIPPED_NO_REMOTE+1))
    echo "  [detached] $(infra_redact_path "$path")"
    continue
  fi
  if ! echo "$gitlab_branches" | grep -qx "$branch"; then
    SKIPPED_NO_REMOTE=$((SKIPPED_NO_REMOTE+1))
    echo "  [unpushed] $(infra_redact_path "$path")  branch=$branch"
    continue
  fi

  # Branch is on gitlab → target/ is reclaimable.
  if [[ -d "$path/target" ]]; then
    rc=0
    infra_safe_remove "$path/target" "branch $branch on gitlab" || rc=$?
    case "$rc" in
      0) CLEANED=$((CLEANED+1)) ;;
      2) REVIEWED=$((REVIEWED+1)) ;;
    esac
  fi
done < <(git -C "$MAIN_REPO" worktree list)

echo
echo "=== cleanup_agent_worktrees summary ==="
echo "  cleaned (target/):    $CLEANED"
echo "  skipped (main):       $SKIPPED_MAIN"
echo "  skipped (locked):     $SKIPPED_LOCKED"
echo "  skipped (unpushed):   $SKIPPED_NO_REMOTE"
echo "  review needed:        $REVIEWED"
echo "  reclaimable:          $(infra_human_bytes "$INFRA_RECLAIM_BYTES")"

if [[ -n "$INFRA_REPORT" ]]; then
  {
    echo "# cleanup_agent_worktrees report"
    echo "date: $(date -u +%FT%TZ)"
    echo "mode: $([[ "$INFRA_APPLY" == "1" ]] && echo APPLY || echo DRY-RUN)"
    echo "cleaned: $CLEANED"
    echo "skipped_main: $SKIPPED_MAIN"
    echo "skipped_locked: $SKIPPED_LOCKED"
    echo "skipped_unpushed: $SKIPPED_NO_REMOTE"
    echo "review_needed: $REVIEWED"
    echo "reclaimable_bytes: $INFRA_RECLAIM_BYTES"
    echo "reclaimable_human: $(infra_human_bytes "$INFRA_RECLAIM_BYTES")"
  } > "$INFRA_REPORT"
  echo "  report:               $INFRA_REPORT"
fi
