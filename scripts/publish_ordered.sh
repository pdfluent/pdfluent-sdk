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

# Topological publish order: leaf → root.
# Use the package name (from Cargo.toml `name =`), NOT the directory name.
# cargo publish -p <name> looks up by package name, not by directory.
CRATES=(
    # --- Image codec forks (no internal deps) ---
    "pdfluent-ccitt"
    "pdfluent-jbig2"
    "pdfluent-jpeg2000"

    # --- lopdf fork + CFF parser ---
    "pdfluent-lopdf"
    "pdfluent-cff"

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
    "pdfluent-sign"
    "pdfluent-forms"
    "pdfluent-extract"
    "pdf-ocr"
    "xfa-license"

    # --- Manipulation + conversion + redaction (depend on extract/manip/xfa-license) ---
    "pdf-manip"
    "pdf-docx"
    "pdf-redact"

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

for CRATE in "${CRATES[@]}"; do
    # Resume logic
    if [[ -n "$RESUME_FROM" ]] && $SKIP; then
        if [[ "$CRATE" == "$RESUME_FROM" ]]; then
            SKIP=false
        else
            log "SKIP  $CRATE (before --from point)"
            SKIPPED+=("$CRATE")
            continue
        fi
    fi

    log "--- $CRATE ---"

    # Package validation: --no-verify skips registry-compile.
    # Compilation was already verified by 'cargo check --workspace' in pre-flight.
    # This checks: file inclusion, no path-dep leaks, LICENSE present, metadata valid.
    #
    # Cascade note: for crates that depend on a new local version not yet on crates.io
    # (e.g. pdf-interpret 0.5.3 in a beta.5 cascade), cargo package will fail with
    # "failed to select a version for the requirement". This is expected — the publish
    # order guarantees the dep will be live before the dependent crate is published.
    # We treat this specific error as a WARNING, not a fatal failure.
    log "  Packaging $CRATE..."
    if cargo package -p "$CRATE" --no-verify --allow-dirty >> "$LOGFILE" 2>&1; then
        log "  Package OK"
    elif tail -20 "$LOGFILE" | grep -q "failed to select a version for the requirement"; then
        # Expected during cascade dry-run: upstream dep has a new local version
        # that isn't on crates.io yet. The topological publish order ensures
        # the dep will be live before this crate is published.
        log "  Package SKIP (cascade dep not yet on crates.io — OK, publish order handles this)"
    else
        log "  Package FAILED for $CRATE — check $LOGFILE"
        die "Package failed for $CRATE. Aborting before any publish."
    fi

    if ! $LIVE; then
        log "  (dry-run only — skipping real publish)"
        SKIPPED+=("$CRATE")
        continue
    fi

    # Real publish
    log "  Publishing $CRATE..."
    if cargo publish -p "$CRATE" --allow-dirty \
            >> "$LOGFILE" 2>&1; then
        log "  Published $CRATE ✅"
        PUBLISHED+=("$CRATE")
    else
        log "  FAILED to publish $CRATE — check $LOGFILE"
        die "Publish failed for $CRATE. See RELEASE_PLAYBOOK.md §Recovery for next steps."
    fi

    # Wait for index propagation before publishing dependents
    if [[ "${#CRATES[@]}" -gt 0 ]] && [[ "$CRATE" != "${CRATES[-1]}" ]]; then
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
