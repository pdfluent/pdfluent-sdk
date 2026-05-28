#!/usr/bin/env bash
# validate_llms_local.sh — developer-loop entry point for F4-LLMS-VALIDATOR.
#
# Auto-detects the pdfluent-website checkout (default: ~/Documents/pdfluent-website/public)
# and runs the Python validator against the live llms{,-full}.txt files.
#
# Override the location:
#   LLMS_FILES_DIR=/path/to/website/public bash scripts/website/validate_llms_local.sh
#
# Pass through extra flags (e.g. --strict):
#   bash scripts/website/validate_llms_local.sh --strict

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEFAULT_DIR="${HOME}/Documents/pdfluent-website/public"
LLMS_FILES_DIR="${LLMS_FILES_DIR:-${DEFAULT_DIR}}"

if [[ ! -d "${LLMS_FILES_DIR}" ]]; then
  echo "FATAL: LLMS_FILES_DIR='${LLMS_FILES_DIR}' does not exist." >&2
  echo "Hint: clone pdfluent-website or set LLMS_FILES_DIR=/your/path/public" >&2
  exit 3
fi

LLMS="${LLMS_FILES_DIR}/llms.txt"
LLMS_FULL="${LLMS_FILES_DIR}/llms-full.txt"

if [[ ! -f "${LLMS}" || ! -f "${LLMS_FULL}" ]]; then
  echo "FATAL: missing llms.txt or llms-full.txt under ${LLMS_FILES_DIR}" >&2
  ls -la "${LLMS_FILES_DIR}" >&2 || true
  exit 3
fi

cd "${REPO_ROOT}"

JSON_OUT="${JSON_OUT:-benchmarks/runs/ga_hardening_plan/f4/LLMS_DRIFT_REPORT.json}"

python3 scripts/website/validate_llms.py \
  --llms "${LLMS}" \
  --llms-full "${LLMS_FULL}" \
  --allowlist scripts/website/llms_claim_allowlist.txt \
  --denylist scripts/website/llms_feature_denylist.txt \
  --json-out "${JSON_OUT}" \
  "$@"
