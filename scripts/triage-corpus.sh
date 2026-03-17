#!/bin/bash
# Triage script for corpus run results
# Usage: ./triage-corpus.sh /path/to/results.sqlite

DB="${1:?Usage: $0 <sqlite-db>}"

if [ ! -f "$DB" ]; then
    echo "ERROR: Database not found: $DB"
    exit 1
fi

echo "=============================================="
echo "CORPUS TRIAGE REPORT"
echo "Database: $DB"
echo "Generated: $(date)"
echo "=============================================="

echo ""
echo "=== 1. OVERVIEW ==="
sqlite3 "$DB" "
SELECT
  COUNT(DISTINCT pdf_path) as total_pdfs,
  SUM(CASE WHEN status='pass' THEN 1 ELSE 0 END) as pass,
  SUM(CASE WHEN status='fail' THEN 1 ELSE 0 END) as fail,
  SUM(CASE WHEN status='skip' THEN 1 ELSE 0 END) as skip,
  SUM(CASE WHEN status='timeout' THEN 1 ELSE 0 END) as timeout,
  SUM(CASE WHEN status='crash' THEN 1 ELSE 0 END) as crash
FROM test_results;
" -header -column

echo ""
echo "=== 2. FAILURES BY TEST ==="
sqlite3 "$DB" "
SELECT test_name, COUNT(*) as failures
FROM test_results
WHERE status='fail'
GROUP BY test_name
ORDER BY failures DESC;
" -header -column

echo ""
echo "=== 3. TIMEOUTS BY TEST ==="
sqlite3 "$DB" "
SELECT test_name, COUNT(*) as timeouts
FROM test_results
WHERE status='timeout'
GROUP BY test_name
ORDER BY timeouts DESC;
" -header -column

echo ""
echo "=== 4. CRASHES ==="
sqlite3 "$DB" "
SELECT test_name, panic_message, COUNT(*) as count
FROM crashes
GROUP BY test_name, panic_message
ORDER BY count DESC
LIMIT 20;
" -header -column

echo ""
echo "=== 5. COMPLIANCE: FALSE NEGATIVES BY RULE ==="
sqlite3 "$DB" "
SELECT
  TRIM(rule.value) as rule_id,
  COUNT(*) as pdf_count
FROM test_results,
     json_each('[\"' || REPLACE(
       (SELECT value FROM json_each(metadata_json) WHERE key='fn_rules'),
       ',', '\",\"') || '\"]') AS rule
WHERE test_name='compliance'
  AND status='fail'
  AND json_extract(metadata_json, '$.fn_rules') IS NOT NULL
GROUP BY rule_id
ORDER BY pdf_count DESC;
" -header -column 2>/dev/null || echo "(no fn_rules metadata found — older runner?)"

echo ""
echo "=== 6. TOP FAILURE ERROR MESSAGES ==="
sqlite3 "$DB" "
SELECT test_name, error_message, COUNT(*) as count
FROM test_results
WHERE status='fail'
GROUP BY test_name, error_message
ORDER BY count DESC
LIMIT 30;
" -header -column

echo ""
echo "=== 7. ERROR CATEGORIES ==="
sqlite3 "$DB" "
SELECT error_category, COUNT(*) as count
FROM test_results
WHERE status='fail' AND error_category IS NOT NULL AND error_category != ''
GROUP BY error_category
ORDER BY count DESC;
" -header -column

echo ""
echo "=== 8. PDFA_CONVERT: FAILURE BREAKDOWN ==="
sqlite3 "$DB" "
SELECT
  CASE
    WHEN error_message LIKE '%reparse%' THEN 'reparse'
    WHEN error_message LIKE '%veraPDF%' THEN 'verapdf_reject'
    WHEN error_message LIKE '%our checker%' THEN 'our_checker'
    WHEN error_message LIKE '%convert failed%' THEN 'convert_error'
    WHEN error_message LIKE '%save failed%' THEN 'save_error'
    ELSE 'other'
  END as failure_type,
  COUNT(*) as count
FROM test_results
WHERE test_name='pdfa_convert' AND status='fail'
GROUP BY failure_type
ORDER BY count DESC;
" -header -column

echo ""
echo "=== 9. REDACT: FAILURE DETAILS ==="
sqlite3 "$DB" "
SELECT
  pdf_path,
  error_message,
  json_extract(metadata_json, '$.search_word') as word
FROM test_results
WHERE test_name='redact' AND status='fail'
ORDER BY pdf_path
LIMIT 20;
" -header -column

echo ""
echo "=== 10. MEMORY SPIKES (>1GB delta) ==="
sqlite3 "$DB" "
SELECT pdf_path, test_name, rss_delta_kb/1024 as delta_mb, rss_after_kb/1024 as after_mb
FROM memory_log
WHERE rss_delta_kb > 1048576
ORDER BY rss_delta_kb DESC
LIMIT 20;
" -header -column 2>/dev/null || echo "(no memory_log table)"

echo ""
echo "=== 11. PASS RATE BY TEST ==="
sqlite3 "$DB" "
SELECT
  test_name,
  COUNT(*) as total,
  SUM(CASE WHEN status='pass' THEN 1 ELSE 0 END) as pass,
  ROUND(100.0 * SUM(CASE WHEN status='pass' THEN 1 ELSE 0 END) / COUNT(*), 2) as pass_pct
FROM test_results
GROUP BY test_name
ORDER BY pass_pct ASC;
" -header -column

echo ""
echo "=============================================="
echo "DONE. Use the following to drill into specific failures:"
echo ""
echo "  # All failures for a specific test:"
echo "  sqlite3 $DB \"SELECT pdf_path, error_message FROM test_results WHERE test_name='compliance' AND status='fail'\""
echo ""
echo "  # Compliance FN rules for a specific PDF:"
echo "  sqlite3 $DB \"SELECT json_extract(metadata_json, '\$.fn_rules') FROM test_results WHERE pdf_path LIKE '%filename%' AND test_name='compliance'\""
echo ""
echo "  # PDFs that fail multiple tests:"
echo "  sqlite3 $DB \"SELECT pdf_path, GROUP_CONCAT(test_name) FROM test_results WHERE status='fail' GROUP BY pdf_path HAVING COUNT(*)>1\""
echo "=============================================="
