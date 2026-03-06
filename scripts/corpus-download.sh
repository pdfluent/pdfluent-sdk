#!/usr/bin/env bash
# Download public PDF test suites to expand the test corpus.
#
# Sources:
#   - pdf.js test PDFs (Mozilla)
#   - Apache PDFBox test documents
#   - Custom AcroForm/annotated PDFs from public repositories
#
# Usage:
#   ./scripts/corpus-download.sh [--target DIR] [--suite SUITE]
#
# Suites: pdfjs, pdfbox, govdocs, all
#
# This script only downloads freely available, public-domain or
# permissively-licensed test PDFs.

set -euo pipefail

TARGET_DIR="corpus"
SUITE="all"

while [[ $# -gt 0 ]]; do
    case $1 in
        --target) TARGET_DIR="$2"; shift 2 ;;
        --suite)  SUITE="$2"; shift 2 ;;
        -h|--help)
            head -16 "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

mkdir -p "$TARGET_DIR"

download_pdfjs() {
    echo "=== Downloading pdf.js test PDFs ==="
    local dir="$TARGET_DIR/pdfjs"
    mkdir -p "$dir"

    # pdf.js test corpus — small curated set from Mozilla
    local base="https://raw.githubusercontent.com/nicolo-ribaudo/test-fixtures/pdfs"
    local files=(
        "basicapi.pdf"
        "tracemonkey.pdf"
        "annotation-text-widget.pdf"
    )

    for f in "${files[@]}"; do
        if [[ ! -f "$dir/$f" ]]; then
            echo "  Downloading $f..."
            curl -sL -o "$dir/$f" "$base/$f" 2>/dev/null || echo "  SKIP: $f (not available)"
        else
            echo "  Already exists: $f"
        fi
    done

    echo "  pdf.js: done"
}

download_pdfbox() {
    echo "=== Downloading Apache PDFBox test PDFs ==="
    local dir="$TARGET_DIR/pdfbox"
    mkdir -p "$dir"

    echo "  PDFBox test PDFs require cloning the test repository."
    echo "  To manually download:"
    echo "    git clone --depth 1 https://github.com/apache/pdfbox.git /tmp/pdfbox"
    echo "    cp /tmp/pdfbox/pdfbox/src/test/resources/input/*.pdf $dir/"
    echo "    cp /tmp/pdfbox/pdfbox/src/test/resources/input/acroform/*.pdf $dir/"
    echo ""

    if command -v git &>/dev/null; then
        if [[ ! -d "/tmp/pdfbox-tests" ]]; then
            echo "  Cloning PDFBox (sparse, test resources only)..."
            git clone --depth 1 --filter=blob:none --sparse \
                https://github.com/apache/pdfbox.git /tmp/pdfbox-tests 2>/dev/null || {
                echo "  SKIP: Could not clone PDFBox"
                return
            }
            cd /tmp/pdfbox-tests
            git sparse-checkout set pdfbox/src/test/resources/input 2>/dev/null || true
            cd - >/dev/null
        fi

        local count=0
        find /tmp/pdfbox-tests -name "*.pdf" -exec cp {} "$dir/" \; 2>/dev/null
        count=$(ls "$dir"/*.pdf 2>/dev/null | wc -l)
        echo "  PDFBox: copied $count PDFs"
    else
        echo "  SKIP: git not available"
    fi
}

download_govdocs() {
    echo "=== govdocs1 corpus ==="
    echo "  The govdocs1 corpus (~100K documents) is too large for automatic download."
    echo "  To manually obtain a subset:"
    echo "    1. Visit https://digitalcorpora.org/corpora/files"
    echo "    2. Download govdocs1 threads containing PDFs"
    echo "    3. Extract PDFs to $TARGET_DIR/govdocs/"
    echo ""
    echo "  Recommended: download threads 0-9 (~5,000 PDFs)"
    mkdir -p "$TARGET_DIR/govdocs"
}

case "$SUITE" in
    pdfjs)   download_pdfjs ;;
    pdfbox)  download_pdfbox ;;
    govdocs) download_govdocs ;;
    all)
        download_pdfjs
        echo ""
        download_pdfbox
        echo ""
        download_govdocs
        ;;
    *)
        echo "Unknown suite: $SUITE"
        echo "Available: pdfjs, pdfbox, govdocs, all"
        exit 1
        ;;
esac

echo ""
echo "=== Corpus summary ==="
total=$(find "$TARGET_DIR" -name "*.pdf" | wc -l)
echo "Total PDFs in $TARGET_DIR: $total"
