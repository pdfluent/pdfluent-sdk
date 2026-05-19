#!/usr/bin/env bash
# check_claims_export_no_promotion.sh — T9-01 CI gate
#
# Purpose:
#   Prevent silent promotion or rewording of release claims by enforcing an
#   operator-approval marker for every byte-change to
#   benchmarks/runs/xfa_enterprise_plan/XFA_RELEASE_CLAIMS_EXPORT.json.
#
# Policy reference:
#   benchmarks/runs/xfa_enterprise_plan/xfa_100_closure_v3/TRACK_XFA_CLAIMS_EXPORT_GOVERNANCE.md
#   benchmarks/runs/xfa_enterprise_plan/xfa_100_closure_v3/T9_CLAIMS_GOVERNANCE_PROPOSAL.md
#
# Logic:
#   1. Diff XFA_RELEASE_CLAIMS_EXPORT.json against the configured baseline
#      (default: origin/enterprise/ga-hardening).
#   2. If the diff is empty -> exit 0 (no promotion attempted).
#   3. If the diff is non-empty, require a matching operator-approval marker:
#         benchmarks/runs/xfa_enterprise_plan/CLAIMS_EXPORT_OPERATOR_APPROVAL_<YYYY-MM-DD>.md
#      that is tracked in HEAD and references the same date as the most recent
#      commit touching the claims export. The marker file MUST contain the
#      literal strings "Operator approval" and "XFA_RELEASE_CLAIMS_EXPORT.json".
#   4. Without a matching marker -> exit 1 with explanation.
#
# Usage:
#   scripts/check_claims_export_no_promotion.sh
#   BASELINE_REF=origin/master scripts/check_claims_export_no_promotion.sh
#   CLAIMS_OVERRIDE_MARKER=/tmp/mock.md scripts/check_claims_export_no_promotion.sh   # test hook
#
# Exit codes:
#   0  No diff, OR diff present with valid operator-approval marker.
#   1  Diff present without operator-approval marker (blocking).
#   2  Configuration or environment error (missing file, bad ref).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "${REPO_ROOT}"

CLAIMS_FILE="benchmarks/runs/xfa_enterprise_plan/XFA_RELEASE_CLAIMS_EXPORT.json"
APPROVAL_DIR="benchmarks/runs/xfa_enterprise_plan"
BASELINE_REF="${BASELINE_REF:-origin/enterprise/ga-hardening}"

log()  { printf '[claims-gate] %s\n' "$*"; }
fail() { printf '[claims-gate][BLOCK] %s\n' "$*" >&2; exit 1; }
err()  { printf '[claims-gate][ERROR] %s\n' "$*" >&2; exit 2; }

if [[ ! -f "${CLAIMS_FILE}" ]]; then
  err "Claims export not found at ${CLAIMS_FILE}"
fi

# Resolve baseline. In CI, the ref may not exist locally; fall back to a
# commit SHA via BASELINE_SHA env var when set.
if [[ -n "${BASELINE_SHA:-}" ]]; then
  BASE="${BASELINE_SHA}"
elif git rev-parse --verify --quiet "${BASELINE_REF}" >/dev/null; then
  BASE="${BASELINE_REF}"
else
  err "Cannot resolve baseline ref '${BASELINE_REF}' and BASELINE_SHA not set"
fi

# Byte-diff against baseline.
DIFF_BYTES="$(git diff "${BASE}" -- "${CLAIMS_FILE}" | wc -c | tr -d ' ')"

if [[ "${DIFF_BYTES}" == "0" ]]; then
  log "Claims export byte-identical to ${BASE}. No promotion attempted. PASS."
  exit 0
fi

log "Claims export differs from ${BASE} (${DIFF_BYTES} diff bytes). Operator-approval marker required."

# Approval marker lookup.
# 1. CLAIMS_OVERRIDE_MARKER env var (test/CI override) — explicit path.
# 2. Otherwise: search APPROVAL_DIR for CLAIMS_EXPORT_OPERATOR_APPROVAL_<YYYY-MM-DD>.md
#    matching the date of the most recent commit touching CLAIMS_FILE.

MARKER=""
if [[ -n "${CLAIMS_OVERRIDE_MARKER:-}" ]]; then
  if [[ -f "${CLAIMS_OVERRIDE_MARKER}" ]]; then
    MARKER="${CLAIMS_OVERRIDE_MARKER}"
    log "Using CLAIMS_OVERRIDE_MARKER=${MARKER}"
  else
    fail "CLAIMS_OVERRIDE_MARKER set but file not found: ${CLAIMS_OVERRIDE_MARKER}"
  fi
else
  CLAIMS_COMMIT_DATE="$(git log -1 --format=%cs -- "${CLAIMS_FILE}" || true)"
  if [[ -z "${CLAIMS_COMMIT_DATE}" ]]; then
    fail "No commit touching ${CLAIMS_FILE} found; cannot resolve approval date"
  fi
  CANDIDATE="${APPROVAL_DIR}/CLAIMS_EXPORT_OPERATOR_APPROVAL_${CLAIMS_COMMIT_DATE}.md"
  if [[ -f "${CANDIDATE}" ]]; then
    MARKER="${CANDIDATE}"
    log "Found approval marker: ${MARKER}"
  else
    fail "No approval marker at ${CANDIDATE}. Create the marker, get operator sign-off, and commit it together with the claims edit."
  fi
fi

# Content gate on the marker file.
if ! grep -q "Operator approval" "${MARKER}"; then
  fail "Marker ${MARKER} missing literal phrase 'Operator approval'"
fi
if ! grep -q "XFA_RELEASE_CLAIMS_EXPORT.json" "${MARKER}"; then
  fail "Marker ${MARKER} must reference XFA_RELEASE_CLAIMS_EXPORT.json"
fi

# Marker must be tracked (committed) in the same change set, unless override.
if [[ -z "${CLAIMS_OVERRIDE_MARKER:-}" ]]; then
  if ! git ls-files --error-unmatch "${MARKER}" >/dev/null 2>&1; then
    fail "Marker ${MARKER} is not tracked in git; commit it alongside the claims change"
  fi
fi

log "Operator-approval marker valid. Claims promotion authorised. PASS."
exit 0
