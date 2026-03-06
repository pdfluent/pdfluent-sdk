#!/usr/bin/env bash
# Generate a metadata index (JSON) for the test corpus.
#
# Scans all PDFs in the corpus directory and outputs structured metadata:
# - File size, page count category, XFA presence
# - Category classification based on filename patterns
#
# Usage:
#   ./scripts/corpus-metadata.sh [CORPUS_DIR] [OUTPUT_FILE]

set -euo pipefail

CORPUS_DIR="${1:-corpus}"
OUTPUT_FILE="${2:-corpus/metadata.json}"

if [[ ! -d "$CORPUS_DIR" ]]; then
    echo "ERROR: Corpus directory not found: $CORPUS_DIR"
    exit 1
fi

echo "Scanning corpus in $CORPUS_DIR..."

# Classify a PDF by its filename
classify() {
    local name="$1"
    case "$name" in
        f[0-9]*)    echo "irs_tax_forms" ;;
        fw[0-9]*)   echo "irs_w_forms" ;;
        i-[0-9]*)   echo "uscis_immigration" ;;
        n-[0-9]*)   echo "uscis_naturalization" ;;
        t[0-9]*)    echo "canadian_tax" ;;
        sf[0-9]*)   echo "us_standard_forms" ;;
        rc[0-9]*)   echo "canadian_benefit" ;;
        *)          echo "other" ;;
    esac
}

COUNT=0
echo "[" > "$OUTPUT_FILE.tmp"

for pdf in "$CORPUS_DIR"/*.pdf; do
    [[ ! -f "$pdf" ]] && continue

    basename=$(basename "$pdf")
    stem="${basename%.pdf}"
    size=$(wc -c < "$pdf" | tr -d ' ')
    category=$(classify "$stem")

    if [[ $COUNT -gt 0 ]]; then
        echo "," >> "$OUTPUT_FILE.tmp"
    fi

    cat >> "$OUTPUT_FILE.tmp" <<ENTRY
  {
    "file": "$basename",
    "size_bytes": $size,
    "category": "$category"
  }
ENTRY

    COUNT=$((COUNT + 1))
done

echo "" >> "$OUTPUT_FILE.tmp"
echo "]" >> "$OUTPUT_FILE.tmp"

mv "$OUTPUT_FILE.tmp" "$OUTPUT_FILE"

echo "Generated metadata for $COUNT PDFs → $OUTPUT_FILE"

# Print category summary
echo ""
echo "Category summary:"
python3 -c "
import json, collections
data = json.load(open('$OUTPUT_FILE'))
cats = collections.Counter(d['category'] for d in data)
for cat, count in sorted(cats.items(), key=lambda x: -x[1]):
    print(f'  {cat}: {count}')
print(f'  TOTAL: {len(data)}')
total_mb = sum(d['size_bytes'] for d in data) / 1024 / 1024
print(f'  Size: {total_mb:.1f} MB')
" 2>/dev/null || true
