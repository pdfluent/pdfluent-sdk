#!/usr/bin/env bash
# Soak test: 100K PDFs, verify no OOM/leaks
# Usage: ./scripts/soak_test.sh /path/to/corpus
# Requirements: xfa-test-runner (release build), valgrind or heaptrack optional

set -euo pipefail

CORPUS_DIR="${1:-}"
TEST_RUNNER="${TEST_RUNNER:-./target/release/xfa-test-runner}"
MAX_PDFS="${MAX_PDFS:-100000}"
MEMORY_LIMIT_MB="${MEMORY_LIMIT_MB:-2048}"
LOG_FILE="${LOG_FILE:-soak_test.log}"
CHECK_INTERVAL="${CHECK_INTERVAL:-1000}"

if [[ ! -d "$CORPUS_DIR" ]]; then
    echo "Error: Corpus directory not found: $CORPUS_DIR"
    echo "Usage: $0 /path/to/corpus"
    exit 1
fi

if [[ ! -x "$TEST_RUNNER" ]]; then
    echo "Error: xfa-test-runner not found at: $TEST_RUNNER"
    echo "Build with: cargo build --release -p xfa-test-runner"
    exit 1
fi

cd "$CORPUS_DIR"
PDFS=($(find . -maxdepth 1 -name "*.pdf" -type f | sort | head -n "$MAX_PDFS"))
TOTAL=${#PDFS[@]}

if [[ $TOTAL -eq 0 ]]; then
    echo "Error: No PDF files found in $CORPUS_DIR"
    exit 1
fi

echo "Soak Test Configuration"
echo "======================"
echo "Corpus: $CORPUS_DIR"
echo "Test runner: $TEST_RUNNER"
echo "Max PDFs: $MAX_PDFS (found: $TOTAL)"
echo "Memory limit: ${MEMORY_LIMIT_MB}MB"
echo "Check interval: every $CHECK_INTERVAL PDFs"
echo "Log file: $LOG_FILE"
echo ""

PASS=0
FAIL=0
START_TIME=$(date +%s)
MAX_RSS_KB=0
MAX_RSS_AT=0

get_rss_kb() {
    if [[ "$(uname)" == "Darwin" ]]; then
        ps -o rss= -p $$ | tr -d ' '
    else
        grepVmHWM /proc/self/status 2>/dev/null | awk '{print $2}' || echo "0"
    fi
}

echo "# Soak Test Log - $(date -Iseconds)" > "$LOG_FILE"
echo "# Corpus: $CORPUS_DIR" >> "$LOG_FILE"
echo "# Total PDFs: $TOTAL" >> "$LOG_FILE"
echo "# Timestamp,PDF_Number,RSS_KB,Exit_Code" >> "$LOG_FILE"

for i in "${!PDFS[@]}"; do
    PDF="${PDFS[$i]}"
    PDF_NUM=$((i + 1))
    PDF_NAME=$(basename "$PDF")
    
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    $TEST_RUNNER --convert-pdfa "$PDF" &>/dev/null &
    PID=$!
    
    if ! wait $PID; then
        EXIT_CODE=$?
        FAIL=$((FAIL + 1))
        echo "FAIL: $PDF_NAME (exit code: $EXIT_CODE)"
    else
        PASS=$((PASS + 1))
    fi
    
    if [[ $((PDF_NUM % CHECK_INTERVAL)) -eq 0 ]]; then
        RSS_KB=$(get_rss_kb)
        RSS_MB=$((RSS_KB / 1024))
        
        if [[ $RSS_KB -gt $MAX_RSS_KB ]]; then
            MAX_RSS_KB=$RSS_KB
            MAX_RSS_AT=$PDF_NUM
        fi
        
        echo "Progress: $PDF_NUM/$TOTAL | RSS: ${RSS_MB}MB | Pass: $PASS | Fail: $FAIL"
        echo "$(date +%s),$PDF_NUM,$RSS_KB,0" >> "$LOG_FILE"
        
        if [[ $RSS_MB -gt $MEMORY_LIMIT_MB ]]; then
            echo ""
            echo "OOM GUARD: Memory exceeded ${MEMORY_LIMIT_MB}MB at PDF $PDF_NUM (RSS: ${RSS_MB}MB)"
            echo "Stopping test to prevent OOM."
            break
        fi
    fi
done

END_TIME=$(date +%s)
TOTAL_TIME=$((END_TIME - START_TIME))

echo ""
echo "Soak Test Complete"
echo "=================="
echo "Total time: ${TOTAL_TIME}s"
echo "PDFs processed: $((PASS + FAIL))/$TOTAL"
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo "Max RSS: $((MAX_RSS_KB / 1024))MB at PDF #$MAX_RSS_AT"
echo "Log: $LOG_FILE"
echo ""
echo "Summary" >> "$LOG_FILE"
echo "Total_Time_Seconds=$TOTAL_TIME" >> "$LOG_FILE"
echo "PDFs_Processed=$((PASS + FAIL))" >> "$LOG_FILE"
echo "Passed=$PASS" >> "$LOG_FILE"
echo "Failed=$FAIL" >> "$LOG_FILE"
echo "Max_RSS_KB=$MAX_RSS_KB" >> "$LOG_FILE"
echo "Max_RSS_At_PDF=$MAX_RSS_AT" >> "$LOG_FILE"
