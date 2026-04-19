#!/usr/bin/env bash
# generate_xfa_reference.sh — Generate reference flattened PDFs for XFA oracle comparison.
#
# XFA Engine Milestone #47 — Issue #1087 (XFA-F1-04)
#
# PURPOSE
# -------
# This script documents *how* to produce reference ("oracle") output for XFA
# flattening using two external tools:
#
#   1. pdfRest API  — cloud PDF processing, uses Adobe's XFA engine under the hood.
#   2. mutool       — MuPDF's command-line tool; flat conversion of XFA PDFs.
#
# The reference output is used for visual fidelity comparison (SSIM) against
# the xfa-native-rust flatten output.  See docs/XFA_SUCCESS_CRITERIA.md for
# the target thresholds.
#
# USAGE
# -----
#   ./scripts/generate_xfa_reference.sh <input.xfa.pdf> [output_dir]
#
#   input.xfa.pdf   — Path to an XFA form PDF.
#   output_dir      — Directory to write reference files (default: ./reference_output).
#
# REQUIREMENTS
# ------------
#   - curl (for pdfRest)
#   - mutool (brew install mupdf-tools  /  apt install mupdf-tools)
#   - PDFREST_API_KEY environment variable or ~/.config/pdfluent/pdfrest-keys.json
#
# OUTPUT FILES
# ------------
#   <output_dir>/<basename>.pdfrest.pdf   — reference from pdfRest
#   <output_dir>/<basename>.mutool.pdf    — reference from mutool

set -euo pipefail

INPUT="${1:-}"
OUTPUT_DIR="${2:-./reference_output}"

if [[ -z "$INPUT" ]]; then
    echo "Usage: $0 <input.xfa.pdf> [output_dir]" >&2
    exit 1
fi

if [[ ! -f "$INPUT" ]]; then
    echo "Error: file not found: $INPUT" >&2
    exit 1
fi

BASENAME=$(basename "$INPUT" .pdf)
mkdir -p "$OUTPUT_DIR"

# ── Load API key ──────────────────────────────────────────────────────────────
KEYS_FILE="${HOME}/.config/pdfluent/pdfrest-keys.json"
if [[ -z "${PDFREST_API_KEY:-}" ]]; then
    if [[ -f "$KEYS_FILE" ]]; then
        # Keys file contains an array of objects with a "key" field.
        # Use the first key.
        PDFREST_API_KEY=$(python3 -c "import json,sys; d=json.load(open('$KEYS_FILE')); print(d[0]['key'])" 2>/dev/null || true)
    fi
fi

# ── 1. pdfRest API ────────────────────────────────────────────────────────────
#
# pdfRest /flatten-pdf endpoint removes interactive form fields and XFA.
# https://pdfrest.com/apilab/flatten-pdf/
#
# Replace PDFREST_API_KEY with your actual key (see ~/.config/pdfluent/pdfrest-keys.json).
#
# curl -X POST "https://api.pdfrest.com/flatten-pdf" \
#     -H "Api-Key: ${PDFREST_API_KEY}" \
#     -H "Accept: application/json" \
#     -H "Content-Type: multipart/form-data" \
#     --form "input=@${INPUT};type=application/pdf" \
#     -o "${OUTPUT_DIR}/${BASENAME}.pdfrest.pdf"

PDFREST_OUTPUT="${OUTPUT_DIR}/${BASENAME}.pdfrest.pdf"

if [[ -n "${PDFREST_API_KEY:-}" ]]; then
    echo "[pdfRest] Flattening ${INPUT} via pdfRest API …"
    HTTP_STATUS=$(curl -s -o "$PDFREST_OUTPUT" -w "%{http_code}" \
        -X POST "https://api.pdfrest.com/flatten-pdf" \
        -H "Api-Key: ${PDFREST_API_KEY}" \
        -H "Accept: application/json" \
        --form "input=@${INPUT};type=application/pdf")
    if [[ "$HTTP_STATUS" == "200" ]]; then
        echo "[pdfRest] OK — saved to ${PDFREST_OUTPUT}"
    else
        echo "[pdfRest] HTTP ${HTTP_STATUS} — check API key and quota." >&2
        rm -f "$PDFREST_OUTPUT"
    fi
else
    echo "[pdfRest] PDFREST_API_KEY not set — skipping pdfRest reference."
    echo "  Set PDFREST_API_KEY or populate ${KEYS_FILE} to enable."
fi

# ── 2. mutool flatten ─────────────────────────────────────────────────────────
#
# mutool convert uses MuPDF's built-in XFA/form flattening.
# Install: brew install mupdf-tools  (macOS)  or  apt install mupdf-tools
#
# mutool convert -o <output.pdf> <input.xfa.pdf>
#
# Note: mutool's XFA support is limited compared to Adobe/pdfRest.
# Use as a secondary oracle or for crash/round-trip sanity checks.

MUTOOL_OUTPUT="${OUTPUT_DIR}/${BASENAME}.mutool.pdf"

if command -v mutool &>/dev/null; then
    echo "[mutool] Flattening ${INPUT} via mutool …"
    mutool convert -o "$MUTOOL_OUTPUT" "$INPUT" 2>&1 || true
    if [[ -f "$MUTOOL_OUTPUT" ]]; then
        echo "[mutool] OK — saved to ${MUTOOL_OUTPUT}"
    else
        echo "[mutool] No output produced — mutool may not support this XFA variant." >&2
    fi
else
    echo "[mutool] mutool not found — skipping mutool reference."
    echo "  Install with: brew install mupdf-tools"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "Reference generation complete."
echo "  Input:    ${INPUT}"
echo "  pdfRest:  ${PDFREST_OUTPUT:-skipped}"
echo "  mutool:   ${MUTOOL_OUTPUT:-skipped}"
echo ""
echo "To compare with xfa-native-rust output, run:"
echo "  python3 scripts/oracle_pdfrest.py --input ${INPUT} --reference ${PDFREST_OUTPUT:-<pdfrest_output.pdf>}"
