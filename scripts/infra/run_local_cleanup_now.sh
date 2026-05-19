#!/usr/bin/env bash
# run_local_cleanup_now.sh — Operator-safe manual Mac cleanup runner.
#
# Drop-in replacement for the LaunchAgent
# ~/Library/LaunchAgents/com.pdfluent.cleanup.plist when launchd cannot
# be made to read ~/Documents/ under macOS Sequoia (TCC + launchd
# sandbox layer blocks execve/read of scripts in ~/Documents/ even
# when /bin/bash has Full Disk Access — see
# benchmarks/runs/ga_100_closure_v3/toolchain_ci_hygiene/
# MAC_LAUNCHD_SANDBOX_DIAGNOSIS_REPORT.md for the full evidence).
#
# This wrapper invokes the underlying cleanup pipeline from an
# interactive shell context (no launchd sandbox in the call chain),
# defaults to a dry-run, and requires an explicit --apply for any
# destructive operation.
#
# Usage:
#
#   # Dry run (default — read-only; reports what WOULD be removed):
#   bash scripts/infra/run_local_cleanup_now.sh
#
#   # Destructive apply (explicit opt-in; preceded by a dry-run):
#   bash scripts/infra/run_local_cleanup_now.sh --apply
#
#   # Custom report dir (defaults to ~/Library/Logs/PDFluent/):
#   bash scripts/infra/run_local_cleanup_now.sh --report-dir <dir>
#
# Hard guarantees:
#
#   - Default mode is DRY-RUN.  No file is removed unless --apply is
#     passed.
#   - The underlying scripts honour the denylist in
#     `scripts/infra/_lib.sh` (corpus / oracle / safedocs / results
#     paths permanently fail-closed).  Even with --apply, denylisted
#     paths are skipped.
#   - Unpushed branches are skipped (their target/ dirs would lose
#     their build cache and force a re-clone-and-rebuild flow).
#   - Locked worktrees are skipped.
#   - The main worktree is skipped.
#   - A timestamped report is written to the report dir so this
#     command is auditable after the fact.
#
# Why this wrapper exists:
#
#   macOS Sequoia (26.x) blocks launchd-spawned processes from
#   executing scripts under ~/Documents/, even when /bin/bash has
#   Full Disk Access in System Settings.  The TCC AUTHREQ_RESULT
#   reports authValue=2 (allowed) for the Documents service, but a
#   separate App Sandbox layer rejects open()/execve() inside that
#   sandbox context.  The only known programmatic workaround is to
#   add the LaunchAgent's plist itself to the FDA list, which the
#   System Settings file picker does not accept for plists inside
#   ~/Library/LaunchAgents/.  Until Apple changes that UX (or PDFluent
#   ships a signed app-bundle wrapper for the cleanup), the safe
#   path is: don't rely on launchd; run this script manually or via
#   a Cron / Hammerspoon / Automator trigger that runs in a normal
#   user-shell context.

set -Eeuo pipefail

# ---------------------------------------------------------------------------
# Resolve the repo root regardless of where this script is invoked from.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"

# ---------------------------------------------------------------------------
# Flags.
# ---------------------------------------------------------------------------
APPLY=0
REPORT_DIR="${HOME}/Library/Logs/PDFluent"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)      APPLY=1; shift ;;
        --report-dir) REPORT_DIR="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,60p' "${BASH_SOURCE[0]}"
            exit 0 ;;
        *)
            echo "Unknown arg: $1" >&2
            echo "Run with -h for usage." >&2
            exit 2 ;;
    esac
done

mkdir -p "${REPORT_DIR}"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT="${REPORT_DIR}/cleanup-${TS}.log"

# Tee everything we print into the timestamped report.
exec > >(tee -a "${REPORT}") 2>&1

echo "============================================================"
echo "PDFluent local cleanup runner"
echo "============================================================"
echo "Started:    $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Repo root:  ${REPO_ROOT}"
echo "Report:     ${REPORT}"
echo "Mode:       $([[ "$APPLY" -eq 1 ]] && echo APPLY || echo DRY-RUN)"
echo ""

# ---------------------------------------------------------------------------
# Stage 1 — Disk snapshot (read-only, always runs).
# ---------------------------------------------------------------------------
echo "--- Stage 1: storage snapshot ---"
if [[ -x "${REPO_ROOT}/scripts/infra/storage_report.sh" ]]; then
    "${REPO_ROOT}/scripts/infra/storage_report.sh"
else
    echo "WARN: storage_report.sh missing or not executable; skipping snapshot."
fi
echo ""

# ---------------------------------------------------------------------------
# Stage 2 — Cleanup dry-run (always runs first, even in --apply mode).
# This makes the destructive plan visible BEFORE anything is removed.
# ---------------------------------------------------------------------------
echo "--- Stage 2: cleanup dry-run (would-remove plan) ---"
"${REPO_ROOT}/scripts/infra/cleanup_agent_worktrees.sh"
echo ""
"${REPO_ROOT}/scripts/infra/cleanup_local_artifacts.sh"
echo ""

# ---------------------------------------------------------------------------
# Stage 3 — Apply (only if --apply explicitly given).
# ---------------------------------------------------------------------------
if [[ "$APPLY" -eq 1 ]]; then
    echo "--- Stage 3: APPLY (destructive) ---"
    echo "About to apply the plan above.  Underlying scripts enforce"
    echo "the denylist in scripts/infra/_lib.sh (corpus / oracle / "
    echo "safedocs / results paths are permanently fail-closed)."
    echo ""
    "${REPO_ROOT}/scripts/infra/cleanup_agent_worktrees.sh" --apply
    echo ""
    "${REPO_ROOT}/scripts/infra/cleanup_local_artifacts.sh" --apply
    echo ""
    echo "--- Stage 3 done ---"
else
    echo "--- Stage 3: SKIPPED (dry-run only; pass --apply to actually clean) ---"
fi

echo ""
echo "Finished:   $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Report:     ${REPORT}"
echo "============================================================"
