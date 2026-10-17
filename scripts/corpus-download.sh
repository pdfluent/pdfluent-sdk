#!/usr/bin/env bash
# Download public PDF test suites to expand the test corpus.
#
# Sources:
# Measured 31-08-2026: every pdf.js URL below answers 404, and the loop turned
# that into "SKIP: not available" and carried on to a summary line. So this
# script has been reporting a successful download of nothing. It says so now:
# a suite that produces no file prints `SKIPPED (not a pass): <reason>` on
# stderr and the script exits non-zero. A silent skip is indistinguishable
# from a pass, which is how four corpus gates went months without running
# (#276).
#
# The per-pull-request corpus does not come through here. It is pinned by
# checksum in corpus/GATE_CORPUS_MANIFEST.json and fetched by
# scripts/ci/fetch_gate_corpus.py. This script is for the large holdout.
#
# Sources:
#   - pdf.js test PDFs (Mozilla)
#   - Apache PDFBox test documents
#   - Custom AcroForm/annotated PDFs from public repositories
#
# Usage:
#   ./scripts/corpus-download.sh [--target DIR] [--suite SUITE] [--count N]
#
# Suites: pdfjs, pdfbox, govdocs, all. --count is the govdocs document count.
#
# This script only downloads freely available, public-domain or
# permissively-licensed test PDFs.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

TARGET_DIR="corpus"
SUITE="all"
# Measured 07-09-2026: 5000 documents is 25 zips, about 13 minutes and 3.0 GB
# on the CI runner. The same number the fetcher defaults to, named here so a
# smaller run is one flag rather than a different script.
GOVDOCS_COUNT="${GOVDOCS_COUNT:-5000}"

while [[ $# -gt 0 ]]; do
    case $1 in
        --target) TARGET_DIR="$2"; shift 2 ;;
        --suite)  SUITE="$2"; shift 2 ;;
        --count)  GOVDOCS_COUNT="$2"; shift 2 ;;
        -h|--help)
            head -16 "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

mkdir -p "$TARGET_DIR"

MISLUKT=0
niets() {
    # $1 = suite, $2 = why
    echo "SKIPPED (not a pass): $1 produced no file — $2" >&2
    MISLUKT=$((MISLUKT + 1))
}

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

    local n
    n=$(find "$dir" -name '*.pdf' | wc -l | tr -d ' ')
    if [[ "$n" -eq 0 ]]; then
        niets "pdfjs" "every URL under $base answered an error; the upstream \
layout moved and these three names no longer exist there"
    else
        echo "  pdf.js: $n file(s)"
    fi
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
        count=$(ls "$dir"/*.pdf 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$count" -eq 0 ]]; then
            niets "pdfbox" "the sparse checkout produced no PDF"
        else
            # Not pinned: this clones whatever master holds today, so two runs
            # a month apart give two different corpora. Fine for a holdout,
            # never for a gate -- which is why the gate reads a manifest.
            echo "  PDFBox: copied $count PDFs (from master, not a pinned commit)"
        fi
    else
        niets "pdfbox" "git is not installed"
    fi
}

download_govdocs() {
    echo "=== govdocs1 corpus ==="
    # This branch used to print three steps for a person to follow and then
    # report a skip, next to a script that does the same job unattended. So a
    # corpus that was "reproducible from public sources" was reproducible by
    # somebody reading instructions, which is a different claim. It now calls
    # the fetcher. (#374)
    local fetcher="$HERE/corpus-download-govdocs.sh"
    if [[ ! -x "$fetcher" ]]; then
        niets "govdocs" "$fetcher is missing or not executable"
        return
    fi
    echo "  Delegating to $(basename "$fetcher") for $GOVDOCS_COUNT documents."
    if ! bash "$fetcher" --target "$TARGET_DIR/govdocs" --count "$GOVDOCS_COUNT"; then
        niets "govdocs" "the fetcher could not produce $GOVDOCS_COUNT documents"
    fi
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
total=$(find "$TARGET_DIR" -name "*.pdf" | wc -l | tr -d ' ')
echo "Total PDFs in $TARGET_DIR: $total"

if [[ "$MISLUKT" -gt 0 ]]; then
    echo "SKIPPED (not a pass): $MISLUKT of the requested suite(s) produced \
nothing. Reported as a failure rather than a summary line, because a corpus \
you think you downloaded is worse than one you know you have not. (#276)" >&2
    exit 1
fi
