#!/usr/bin/env bash
# Run veraPDF conformance validation against PDF files.
#
# Usage:
#   ./scripts/run-verapdf.sh [OPTIONS] [FILE_OR_DIR...]
#
# Options:
#   -p, --profile PROFILE   PDF/A profile to validate against (default: 2b)
#                            Valid: 1a, 1b, 2a, 2b, 2u, 3a, 3b, 3u, ua1
#   -o, --output DIR        Output directory for reports (default: reports/verapdf)
#   -f, --format FORMAT     Report format: json, html, xml (default: json)
#   --ci                    CI mode: exit 1 on any failure
#   -h, --help              Show this help
#
# If no FILE_OR_DIR is given, defaults to corpus/ directory.
#
# Prerequisites:
#   - veraPDF CLI: https://verapdf.org/software/
#   - Install via: brew install verapdf (macOS) or download from verapdf.org
#   - Or use Docker: docker pull verapdf/verapdf

set -euo pipefail

PROFILE="2b"
OUTPUT_DIR="reports/verapdf"
FORMAT="json"
CI_MODE=false

usage() {
    head -20 "$0" | grep '^#' | sed 's/^# \?//'
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--profile) PROFILE="$2"; shift 2 ;;
        -o|--output) OUTPUT_DIR="$2"; shift 2 ;;
        -f|--format) FORMAT="$2"; shift 2 ;;
        --ci) CI_MODE=true; shift ;;
        -h|--help) usage ;;
        *) break ;;
    esac
done

# Default input: corpus directory
INPUTS=("${@:-corpus}")

# Locate veraPDF
VERAPDF=""
if command -v verapdf &>/dev/null; then
    VERAPDF="verapdf"
elif [[ -x "/opt/verapdf/verapdf" ]]; then
    VERAPDF="/opt/verapdf/verapdf"
elif command -v docker &>/dev/null; then
    echo "veraPDF CLI not found, falling back to Docker..."
    VERAPDF="docker"
else
    echo "ERROR: veraPDF not found. Install it:"
    echo "  macOS:  brew install verapdf"
    echo "  Linux:  https://verapdf.org/software/"
    echo "  Docker: docker pull verapdf/verapdf"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# Map format to veraPDF --format flag
case "$FORMAT" in
    json) VERA_FORMAT="json" ;;
    html) VERA_FORMAT="html" ;;
    xml)  VERA_FORMAT="mrr" ;;  # Machine-Readable Report
    *) echo "Unknown format: $FORMAT"; exit 1 ;;
esac

TOTAL=0
PASSED=0
FAILED=0
ERRORS=0
FAILED_FILES=()

# Collect all PDF files
PDF_FILES=()
for input in "${INPUTS[@]}"; do
    if [[ -f "$input" ]]; then
        PDF_FILES+=("$input")
    elif [[ -d "$input" ]]; then
        while IFS= read -r -d '' f; do
            PDF_FILES+=("$f")
        done < <(find "$input" -name '*.pdf' -type f -print0 | sort -z)
    else
        echo "WARNING: $input not found, skipping"
    fi
done

if [[ ${#PDF_FILES[@]} -eq 0 ]]; then
    echo "No PDF files found."
    exit 0
fi

echo "veraPDF Conformance Test"
echo "========================"
echo "Profile:  PDF/A-${PROFILE}"
echo "Files:    ${#PDF_FILES[@]}"
echo "Output:   ${OUTPUT_DIR}"
echo ""

for pdf in "${PDF_FILES[@]}"; do
    TOTAL=$((TOTAL + 1))
    basename=$(basename "$pdf" .pdf)
    report_file="${OUTPUT_DIR}/${basename}.${FORMAT}"

    printf "  %-50s " "$pdf"

    if [[ "$VERAPDF" == "docker" ]]; then
        # Docker mode: mount the PDF file
        abs_pdf="$(cd "$(dirname "$pdf")" && pwd)/$(basename "$pdf")"
        result=$(docker run --rm -v "$(dirname "$abs_pdf"):/data" \
            verapdf/verapdf \
            --profile "${PROFILE}" \
            --format "${VERA_FORMAT}" \
            "/data/$(basename "$pdf")" 2>&1) || true
    else
        result=$("$VERAPDF" \
            --profile "${PROFILE}" \
            --format "${VERA_FORMAT}" \
            "$pdf" 2>&1) || true
    fi

    # Save report
    echo "$result" > "$report_file"

    # Parse result for pass/fail
    if echo "$result" | grep -q '"compliant": true\|isCompliant="true"\|compliant>true'; then
        printf "PASS\n"
        PASSED=$((PASSED + 1))
    elif echo "$result" | grep -q '"compliant": false\|isCompliant="false"\|compliant>false'; then
        printf "FAIL\n"
        FAILED=$((FAILED + 1))
        FAILED_FILES+=("$pdf")
    else
        printf "ERROR\n"
        ERRORS=$((ERRORS + 1))
    fi
done

echo ""
echo "Results"
echo "-------"
echo "Total:   $TOTAL"
echo "Passed:  $PASSED"
echo "Failed:  $FAILED"
echo "Errors:  $ERRORS"

if [[ $TOTAL -gt 0 ]]; then
    PASS_RATE=$(( (PASSED * 100) / TOTAL ))
    echo "Rate:    ${PASS_RATE}%"
fi

# Write summary JSON
cat > "${OUTPUT_DIR}/summary.json" <<SUMMARY
{
  "profile": "PDF/A-${PROFILE}",
  "total": ${TOTAL},
  "passed": ${PASSED},
  "failed": ${FAILED},
  "errors": ${ERRORS},
  "pass_rate": ${PASS_RATE:-0},
  "failed_files": [$(printf '"%s",' "${FAILED_FILES[@]}" 2>/dev/null | sed 's/,$//' || echo "")]
}
SUMMARY

echo ""
echo "Summary written to ${OUTPUT_DIR}/summary.json"

if [[ ${#FAILED_FILES[@]} -gt 0 ]]; then
    echo ""
    echo "Failed files:"
    for f in "${FAILED_FILES[@]}"; do
        echo "  - $f"
    done
fi

if $CI_MODE && [[ $FAILED -gt 0 || $ERRORS -gt 0 ]]; then
    echo ""
    echo "CI mode: failing due to $FAILED failures and $ERRORS errors"
    exit 1
fi
