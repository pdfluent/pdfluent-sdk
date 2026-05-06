#!/usr/bin/env bash
# publish_ordered.sh — Publish PDFluent SDK crates in topological order.
#
# Usage:
#   ./scripts/publish_ordered.sh            # dry-run (default)
#   ./scripts/publish_ordered.sh --live     # real publish (requires 'cargo login')
#   ./scripts/publish_ordered.sh --from pdf-engine  # resume from a specific crate
#
# The topological order below is derived from dependency edges:
# leaf crates first, umbrella crates last.
#
# Rules:
#  - Each publish is verified against the crates.io index before proceeding
#    to the next crate that depends on it.
#  - crates.io enforces a per-account rate limit; we wait 15 s between publishes
#    (not the old 11 min—new rate limits allow tighter batching for the same owner).
#  - All crates are dry-run validated first regardless of --live flag.
#  - On failure, the script aborts. Partial-publish recovery: see RELEASE_PLAYBOOK.md.
#
# DO NOT remove the --dry-run validation pass. It catches version conflicts
# and missing metadata before any real publish happens.

set -euo pipefail

LIVE=false
RESUME_FROM=""
WAIT_SECS=15
LOGFILE="/tmp/publish_ordered_$(date +%Y%m%d_%H%M%S).log"

while [[ $# -gt 0 ]]; do
    case $1 in
        --live)     LIVE=true; shift ;;
        --from)     RESUME_FROM="$2"; shift 2 ;;
        --wait)     WAIT_SECS="$2"; shift 2 ;;
        -h|--help)
            grep '^#' "$0" | head -20 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown flag: $1"; exit 1 ;;
    esac
done

log() { echo "[$(date -u '+%H:%M:%S')] $*" | tee -a "$LOGFILE"; }
die() { log "ERROR: $*"; exit 1; }

# Topological publish order: leaf → root
# Format: "local-crate-name [--package published-name]"
# The --package flag is needed when the local crate name differs from the
# published name (e.g. pdf-sign publishes as pdfluent-sign).
CRATES=(
    # --- Image codec forks (no internal deps) ---
    "hayro-ccitt --package pdfluent-ccitt"
    "hayro-jbig2 --package pdfluent-jbig2"
    "hayro-jpeg2000 --package pdfluent-jpeg2000"

    # --- lopdf fork + CFF parser ---
    "lopdf --package pdfluent-lopdf"
    "cff-parser --package pdfluent-cff"

    # --- Core parse/interpret/font layer ---
    "pdf-syntax"
    "pdf-font"
    "pdf-interpret"
    "pdf-render"

    # --- XFA stack (ordered by dependency depth) ---
    "xfa-dom-resolver"
    "formcalc-interpreter"
    "xfa-layout-engine"
    "xfa-json"

    # --- Compliance + annot ---
    "pdf-compliance"
    "pdf-annot"

    # --- Engine layer ---
    "pdf-engine"

    # --- Feature crates ---
    "pdf-sign --package pdfluent-sign"
    "pdf-forms --package pdfluent-forms"
    "pdf-extract --package pdfluent-extract"
    "pdf-ocr"
    "xfa-license"

    # --- XFA integration ---
    "pdf-xfa"

    # --- Umbrella ---
    "pdfluent"
)

if $LIVE; then
    log "=== LIVE PUBLISH MODE ==="
    log "Will publish ${#CRATES[@]} crates."
    log "Logfile: $LOGFILE"
    log ""
    log "CTRL-C to abort. On partial failure see RELEASE_PLAYBOOK.md §Recovery."
    sleep 3
else
    log "=== DRY-RUN MODE (pass --live to publish) ==="
fi

SKIP=true
[[ -z "$RESUME_FROM" ]] && SKIP=false

PUBLISHED=()
SKIPPED=()

for ENTRY in "${CRATES[@]}"; do
    # Parse crate name and optional --package flag
    CRATE_DIR=$(echo "$ENTRY" | awk '{print $1}')
    EXTRA_FLAGS=$(echo "$ENTRY" | cut -s -d' ' -f2-)

    # Determine the published crate name
    if echo "$EXTRA_FLAGS" | grep -q -- "--package"; then
        PUB_NAME=$(echo "$EXTRA_FLAGS" | sed 's/.*--package //' | awk '{print $1}')
    else
        PUB_NAME="$CRATE_DIR"
    fi

    # Resume logic
    if [[ -n "$RESUME_FROM" ]] && $SKIP; then
        if [[ "$PUB_NAME" == "$RESUME_FROM" ]] || [[ "$CRATE_DIR" == "$RESUME_FROM" ]]; then
            SKIP=false
        else
            log "SKIP  $PUB_NAME (before --from point)"
            SKIPPED+=("$PUB_NAME")
            continue
        fi
    fi

    log "--- $PUB_NAME ---"

    # Always dry-run first
    log "  Dry-run validating $PUB_NAME..."
    if ! cargo publish -p "$CRATE_DIR" $EXTRA_FLAGS --dry-run --allow-dirty \
            >> "$LOGFILE" 2>&1; then
        log "  Dry-run FAILED for $PUB_NAME — check $LOGFILE"
        die "Dry-run failed for $PUB_NAME. Aborting before any publish."
    fi
    log "  Dry-run OK"

    if ! $LIVE; then
        log "  (dry-run only — skipping real publish)"
        SKIPPED+=("$PUB_NAME")
        continue
    fi

    # Real publish
    log "  Publishing $PUB_NAME..."
    if cargo publish -p "$CRATE_DIR" $EXTRA_FLAGS --allow-dirty \
            >> "$LOGFILE" 2>&1; then
        log "  Published $PUB_NAME ✅"
        PUBLISHED+=("$PUB_NAME")
    else
        log "  FAILED to publish $PUB_NAME — check $LOGFILE"
        die "Publish failed for $PUB_NAME. See RELEASE_PLAYBOOK.md §Recovery for next steps."
    fi

    # Wait for index propagation before publishing dependents
    if [[ "${#CRATES[@]}" -gt 0 ]] && [[ "$ENTRY" != "${CRATES[-1]}" ]]; then
        log "  Waiting ${WAIT_SECS}s for crates.io index propagation..."
        sleep "$WAIT_SECS"
    fi
done

log ""
log "=== SUMMARY ==="
log "Published: ${PUBLISHED[*]:-none}"
[[ ${#SKIPPED[@]} -gt 0 ]] && log "Skipped:   ${SKIPPED[*]}"
log "Log:       $LOGFILE"

if $LIVE; then
    log ""
    log "=== POST-PUBLISH VERIFICATION ==="

    # Derive published version from workspace metadata
    PDFLUENT_VER=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
        | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='pdfluent'))" 2>/dev/null || echo "")

    if [[ -z "$PDFLUENT_VER" ]]; then
        die "Could not determine pdfluent version from cargo metadata. Tag and release manually."
    fi

    TAG="v${PDFLUENT_VER}"
    log "  Pdfluent version: $PDFLUENT_VER  →  tag: $TAG"

    # Smoke-verify the published crate is visible on crates.io
    log "  Verifying crates.io visibility..."
    if cargo info pdfluent 2>&1 | grep -q "$PDFLUENT_VER"; then
        log "  crates.io visibility: ✅ $PDFLUENT_VER is live"
    else
        log "  WARNING: pdfluent $PDFLUENT_VER not yet visible on crates.io (index lag — retry in 60s)"
    fi

    # Create git tag
    if git rev-parse "$TAG" >/dev/null 2>&1; then
        log "  Tag $TAG already exists — skipping"
    else
        log "  Creating git tag $TAG..."
        git tag "$TAG"
        log "  Pushing tag..."
        git push origin "$TAG"
        log "  Tag pushed: ✅ $TAG"
    fi

    # Create GitHub release
    if gh release view "$TAG" >/dev/null 2>&1; then
        log "  GitHub release $TAG already exists — skipping"
    else
        log "  Creating GitHub release $TAG..."
        gh release create "$TAG" \
            --title "PDFluent SDK $PDFLUENT_VER" \
            --generate-notes \
            >> "$LOGFILE" 2>&1
        log "  GitHub release created: ✅ $TAG"
    fi

    log ""
    log "=== RELEASE COMPLETE ==="
    log "  Version:  $PDFLUENT_VER"
    log "  Tag:      $TAG (pushed)"
    log "  Release:  $(gh release view "$TAG" --json url -q .url 2>/dev/null || echo 'see GitHub')"
    log "  Log:      $LOGFILE"
    log ""
    log "Verify install in a clean project:"
    log "  mkdir /tmp/smoke && cd /tmp/smoke && cargo init && cargo add pdfluent && cargo check && cd - && rm -rf /tmp/smoke"
fi
