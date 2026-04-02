#!/usr/bin/env bash
# Soak test log analyzer
# Usage: ./scripts/soak_analyze.sh [log_file]
# Default log file: soak_test.log

set -euo pipefail

LOG_FILE="${1:-soak_test.log}"

if [[ ! -f "$LOG_FILE" ]]; then
    echo "Error: Log file not found: $LOG_FILE"
    echo "Usage: $0 [log_file]"
    exit 1
fi

DATA_FILE=$(mktemp)
trap "rm -f $DATA_FILE" EXIT

grep "^[0-9]" "$LOG_FILE" | grep -v "Summary" | grep -v "^#" > "$DATA_FILE"

if [[ ! -s "$DATA_FILE" ]]; then
    echo "Error: No data found in log file"
    exit 1
fi

TOTAL=$(wc -l < "$DATA_FILE")
PASS=$(grep ",0$" "$DATA_FILE" | wc -l)
FAIL=$(grep -v ",0$" "$DATA_FILE" | grep "^[0-9]" | wc -l)

MAX_RSS=$(sort -t',' -k3 -n "$DATA_FILE" | tail -1)
MAX_RSS_KB=$(echo "$MAX_RSS" | cut -d',' -f3)
MAX_RSS_MB=$((MAX_RSS_KB / 1024))
MAX_RSS_AT=$(echo "$MAX_RSS" | cut -d',' -f2)

AVG_RSS=$(awk -F',' '{sum+=$3; count++} END {print int(sum/count)}' "$DATA_FILE")
AVG_RSS_MB=$((AVG_RSS / 1024))

echo "Soak Test Analysis"
echo "=================="
echo "Log file: $LOG_FILE"
echo ""
echo "Overall Results"
echo "--------------"
echo "Total processed: $TOTAL"
echo "Passed: $PASS"
echo "Failed: $FAIL"
if [[ $TOTAL -gt 0 ]]; then
    FAIL_RATE=$(echo "scale=2; $FAIL * 100 / $TOTAL" | bc)
    echo "Failure rate: ${FAIL_RATE}%"
fi
echo ""
echo "Memory Statistics"
echo "----------------"
echo "Max RSS: ${MAX_RSS_MB}MB at PDF #$MAX_RSS_AT"
echo "Avg RSS: ${AVG_RSS_MB}MB"
echo ""

echo "Memory Curve (RSS over time)"
echo "----------------------------"
printf "%-10s %-10s\n" "PDF_#" "RSS_MB"
printf "%-10s %-10s\n" "------" "------"

head -20 "$DATA_FILE" | while IFS=',' read -r ts pdf rss _; do
    printf "%-10s %-10s\n" "$pdf" "$((rss / 1024))"
done

if [[ $TOTAL -gt 20 ]]; then
    echo "... (truncated)"
    tail -5 "$DATA_FILE" | while IFS=',' read -r ts pdf rss _; do
        printf "%-10s %-10s\n" "$pdf" "$((rss / 1024))"
    done
fi
echo ""

LEAK_THRESHOLD_MB=50
PREV_RSS=0
LEAK_DETECTED=false

echo "Memory Leak Detection"
echo "---------------------"
while IFS=',' read -r ts pdf rss _; do
    CURR_MB=$((rss / 1024))
    DIFF=$((CURR_MB - PREV_RSS))
    
    if [[ $pdf -gt 1000 && $DIFF -gt $LEAK_THRESHOLD_MB ]]; then
        echo "WARNING: Large RSS increase of ${DIFF}MB at PDF #$pdf (RSS: ${CURR_MB}MB)"
        LEAK_DETECTED=true
    fi
    
    PREV_RSS=$CURR_MB
done < "$DATA_FILE"

if [[ "$LEAK_DETECTED" == "false" ]]; then
    echo "No significant memory leaks detected (threshold: ${LEAK_THRESHOLD_MB}MB jump)"
fi

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "Failure Analysis"
    echo "----------------"
    grep -v ",0$" "$DATA_FILE" | grep "^[0-9]" | head -10 | while IFS=',' read -r ts pdf rss exit; do
        echo "PDF #$pdf failed with exit code $exit at RSS ${rss}KB"
    done
fi

echo ""
echo "Recommendations"
echo "---------------"
if [[ $FAIL -gt 0 ]]; then
    echo "- Investigate failed PDFs (see log for details)"
fi
if [[ $MAX_RSS_MB -gt 1800 ]]; then
    echo "- WARNING: High memory usage, consider reducing worker count"
fi
if [[ "$LEAK_DETECTED" == "true" ]]; then
    echo "- Memory leak suspected, run with valgrind/heaptrack for detailed analysis"
fi
if [[ $FAIL -eq 0 && "$LEAK_DETECTED" == "false" ]]; then
    echo "- Test passed successfully with stable memory usage"
fi
