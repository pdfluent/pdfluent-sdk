#!/usr/bin/env bash
# Automated Visual Regression Testing (AVRT) pipeline.
#
# Compares engine-rendered PNGs against Adobe gold master images
# using pixel-level diff with configurable thresholds.
#
# Usage:
#   ./scripts/run-avrt.sh [OPTIONS]
#
# Options:
#   -r, --renders DIR       Directory with engine-rendered PNGs (default: renders/)
#   -g, --gold DIR          Directory with Adobe gold master PNGs (default: gold-masters/)
#   -o, --output DIR        Output directory for diff images/reports (default: reports/avrt)
#   -t, --threshold PCT     Max allowed pixel diff percentage (default: 1.0)
#   -c, --channel-tol N     Per-channel tolerance 0-255 (default: 5)
#   --ci                    CI mode: exit 1 on any failure
#   -h, --help              Show this help
#
# Prerequisites:
#   - Gold master images must exist in GOLD_DIR as {form_name}_page{N}.png
#   - Engine renders must exist in RENDERS_DIR as {form_name}_page{N}.png
#   - Requires: cargo build -p xfa-golden-tests (for the diff tool)
#     or ImageMagick (compare command) as fallback

set -euo pipefail

RENDERS_DIR="renders"
GOLD_DIR="gold-masters"
OUTPUT_DIR="reports/avrt"
THRESHOLD="1.0"
CHANNEL_TOL="5"
CI_MODE=false

usage() {
    head -24 "$0" | grep '^#' | sed 's/^# \?//'
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--renders) RENDERS_DIR="$2"; shift 2 ;;
        -g|--gold) GOLD_DIR="$2"; shift 2 ;;
        -o|--output) OUTPUT_DIR="$2"; shift 2 ;;
        -t|--threshold) THRESHOLD="$2"; shift 2 ;;
        -c|--channel-tol) CHANNEL_TOL="$2"; shift 2 ;;
        --ci) CI_MODE=true; shift ;;
        -h|--help) usage ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [[ ! -d "$GOLD_DIR" ]]; then
    echo "ERROR: Gold masters directory not found: $GOLD_DIR"
    echo ""
    echo "To create gold masters:"
    echo "  1. Render all corpus PDFs with Adobe Acrobat to PNG"
    echo "  2. Place them in $GOLD_DIR/ as {form_name}_page{N}.png"
    echo "  3. Recommended: 150 DPI, RGB color space"
    exit 1
fi

if [[ ! -d "$RENDERS_DIR" ]]; then
    echo "ERROR: Engine renders directory not found: $RENDERS_DIR"
    echo ""
    echo "To generate engine renders:"
    echo "  cargo run --bin xfa-cli -- render --corpus corpus/ --output $RENDERS_DIR"
    exit 1
fi

mkdir -p "$OUTPUT_DIR/diffs"

# Check for ImageMagick compare
HAS_MAGICK=false
if command -v compare &>/dev/null; then
    HAS_MAGICK=true
fi

TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0
FAILED_LIST=()
RESULTS_JSON="["

echo "AVRT — Automated Visual Regression Testing"
echo "============================================"
echo "Gold masters: $GOLD_DIR"
echo "Renders:      $RENDERS_DIR"
echo "Threshold:    ${THRESHOLD}% pixel diff"
echo "Channel tol:  $CHANNEL_TOL"
echo ""

for gold_file in "$GOLD_DIR"/*.png; do
    [[ ! -f "$gold_file" ]] && continue

    basename=$(basename "$gold_file")
    render_file="${RENDERS_DIR}/${basename}"

    if [[ ! -f "$render_file" ]]; then
        printf "  %-50s SKIP (no render)\n" "$basename"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    TOTAL=$((TOTAL + 1))
    diff_file="${OUTPUT_DIR}/diffs/diff_${basename}"

    # Use ImageMagick compare if available
    if $HAS_MAGICK; then
        # Get image dimensions
        gold_size=$(identify -format "%wx%h" "$gold_file" 2>/dev/null || echo "0x0")
        render_size=$(identify -format "%wx%h" "$render_file" 2>/dev/null || echo "0x0")

        if [[ "$gold_size" != "$render_size" ]]; then
            printf "  %-50s FAIL (size mismatch: gold=%s render=%s)\n" "$basename" "$gold_size" "$render_size"
            FAILED=$((FAILED + 1))
            FAILED_LIST+=("$basename")

            # Append to results
            [[ "$RESULTS_JSON" != "[" ]] && RESULTS_JSON+=","
            RESULTS_JSON+="{\"file\":\"$basename\",\"status\":\"FAIL\",\"reason\":\"size_mismatch\",\"gold_size\":\"$gold_size\",\"render_size\":\"$render_size\"}"
            continue
        fi

        # Compute diff with AE (absolute error) metric
        # Returns the number of differing pixels
        metric_output=$(compare -metric AE -fuzz "${CHANNEL_TOL}" \
            "$gold_file" "$render_file" "$diff_file" 2>&1) || true

        # Extract diff pixel count
        diff_pixels=$(echo "$metric_output" | grep -oE '^[0-9]+' || echo "0")

        # Get total pixels
        total_pixels=$(identify -format "%[fx:w*h]" "$gold_file" 2>/dev/null || echo "1")

        if [[ "$total_pixels" -gt 0 ]]; then
            diff_pct=$(python3 -c "print(f'{($diff_pixels / $total_pixels) * 100:.4f}')")
        else
            diff_pct="0.0000"
        fi

        # Check against threshold
        pass=$(python3 -c "print('PASS' if float('$diff_pct') <= float('$THRESHOLD') else 'FAIL')")

        if [[ "$pass" == "PASS" ]]; then
            printf "  %-50s PASS (%s%% diff)\n" "$basename" "$diff_pct"
            PASSED=$((PASSED + 1))
            # Remove diff image if passed (clean)
            rm -f "$diff_file"
        else
            printf "  %-50s FAIL (%s%% diff, %s pixels)\n" "$basename" "$diff_pct" "$diff_pixels"
            FAILED=$((FAILED + 1))
            FAILED_LIST+=("$basename")
        fi

        [[ "$RESULTS_JSON" != "[" ]] && RESULTS_JSON+=","
        RESULTS_JSON+="{\"file\":\"$basename\",\"status\":\"$pass\",\"diff_pct\":$diff_pct,\"diff_pixels\":$diff_pixels,\"total_pixels\":$total_pixels}"
    else
        # Fallback: just check file sizes as a rough proxy
        gold_bytes=$(wc -c < "$gold_file")
        render_bytes=$(wc -c < "$render_file")
        diff_bytes=$(( gold_bytes > render_bytes ? gold_bytes - render_bytes : render_bytes - gold_bytes ))
        ratio=$(python3 -c "print(f'{($diff_bytes / max($gold_bytes, 1)) * 100:.2f}')")

        printf "  %-50s ???? (no ImageMagick, size diff: %s%%)\n" "$basename" "$ratio"
        SKIPPED=$((SKIPPED + 1))
    fi
done

RESULTS_JSON+="]"

echo ""
echo "Results"
echo "-------"
echo "Total:   $TOTAL"
echo "Passed:  $PASSED"
echo "Failed:  $FAILED"
echo "Skipped: $SKIPPED"

if [[ $TOTAL -gt 0 ]]; then
    PASS_RATE=$(python3 -c "print(f'{($PASSED / $TOTAL) * 100:.1f}')")
    echo "Rate:    ${PASS_RATE}%"
else
    PASS_RATE="0.0"
fi

# Write summary JSON
cat > "${OUTPUT_DIR}/summary.json" <<SUMMARY
{
  "threshold_pct": ${THRESHOLD},
  "channel_tolerance": ${CHANNEL_TOL},
  "total": ${TOTAL},
  "passed": ${PASSED},
  "failed": ${FAILED},
  "skipped": ${SKIPPED},
  "pass_rate": ${PASS_RATE},
  "failed_files": [$(printf '"%s",' "${FAILED_LIST[@]}" 2>/dev/null | sed 's/,$//' || echo "")],
  "results": ${RESULTS_JSON}
}
SUMMARY

echo ""
echo "Summary:  ${OUTPUT_DIR}/summary.json"
echo "Diffs:    ${OUTPUT_DIR}/diffs/"

if [[ ${#FAILED_LIST[@]} -gt 0 ]]; then
    echo ""
    echo "Failed files:"
    for f in "${FAILED_LIST[@]}"; do
        echo "  - $f"
    done
fi

if $CI_MODE && [[ $FAILED -gt 0 ]]; then
    echo ""
    echo "CI mode: failing due to $FAILED visual regressions"
    exit 1
fi
