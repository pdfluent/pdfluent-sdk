#!/usr/bin/env bash
# publish_ordered.sh — Publish PDFluent SDK crates in topological order.
#
# Usage:
#   ./scripts/publish_ordered.sh                     # dry-run (default)
#   ./scripts/publish_ordered.sh --live              # real publish (requires 'cargo login')
#   ./scripts/publish_ordered.sh --live --resume     # auto-detect first unpublished, resume from there
#   ./scripts/publish_ordered.sh --live --from pdf-engine  # manual resume from a specific crate
#   ./scripts/publish_ordered.sh --wait 30           # override inter-publish delay (default 15s)
#
# Idempotent by design:
#   - Before each publish, the script queries crates.io to check if this exact version
#     already exists. If it does, it logs "ALREADY PUBLISHED" and skips — it never
#     aborts on a duplicate. This makes the script safe to re-run after partial failures.
#
#   - --resume scans crates.io at startup and sets the resume point automatically.
#     Combined with --live it makes re-running after a network error fully automatic.
#
# State file:
#   After each successful publish the crate name is appended to:
#   /tmp/publish_ordered_state_<date>.txt
#   This file is informational only. The authoritative source is crates.io itself.
#
# Recovery after partial failure:
#   1. Re-run with --live --resume  (fully automatic)
#   2. Or: --live --from <last-failed-crate>  (manual)
#   3. Already-published crates are silently skipped either way.
#
# DO NOT remove the --dry-run packaging pass. It catches version conflicts
# and missing metadata before any real publish.

set -euo pipefail

# ---------------------------------------------------------------------------
# Flags
# ---------------------------------------------------------------------------
LIVE=false
RESUME_FROM=""
AUTO_RESUME=false
WAIT_SECS=15
DATESTAMP="$(date +%Y%m%d_%H%M%S)"
LOGFILE="/tmp/publish_ordered_${DATESTAMP}.log"
STATE_FILE="/tmp/publish_ordered_state_${DATESTAMP}.txt"

while [[ $# -gt 0 ]]; do
    case $1 in
        --live)     LIVE=true; shift ;;
        --resume)   AUTO_RESUME=true; shift ;;
        --from)     RESUME_FROM="$2"; shift 2 ;;
        --wait)     WAIT_SECS="$2"; shift 2 ;;
        -h|--help)
            sed -n 's/^# //p' "$0" | head -30
            exit 0
            ;;
        *) echo "Unknown flag: $1"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo "[$(date -u '+%H:%M:%S')] $*" | tee -a "$LOGFILE"; }
die()  { log "ERROR: $*"; exit 1; }
warn() { log "WARN:  $*"; }

# Live publishing NEVER uses --allow-dirty: the real `cargo publish` below
# enforces a clean per-crate tree, and the `cargo package` inspection step is
# held to the same standard in --live mode. Only the default dry-run (which is
# meant to validate work-in-progress) permits a dirty tree for `cargo package`.
PKG_DIRTY_FLAG="--allow-dirty"
$LIVE && PKG_DIRTY_FLAG=""

# Check whether a specific crate version exists on crates.io.
# Returns 0 (true) if published, 1 (false) if not yet published.
# On network error: returns 2 and logs a warning.
crates_io_has_version() {
    local name="$1" version="$2"
    local http_code
    # -s: silent. No -f so that 4xx responses are captured correctly via -w.
    # The || true guards against curl network errors (DNS failure, timeout) under set -e.
    http_code=$(curl -s -o /dev/null -w "%{http_code}" \
        --max-time 15 \
        -H "User-Agent: pdfluent-publish/1.0 (contact: team@pdfluent.com)" \
        "https://crates.io/api/v1/crates/${name}/${version}" 2>/dev/null) || true
    case "${http_code:-000}" in
        200) return 0 ;;
        404) return 1 ;;
        *)   warn "crates.io API returned HTTP ${http_code:-000} for ${name}@${version} — treating as unpublished"; return 2 ;;
    esac
}

# Get this crate's published version from local Cargo.toml metadata.
# Uses cargo metadata (called once and cached).
_METADATA_CACHE=""
get_local_version() {
    local name="$1"
    if [[ -z "$_METADATA_CACHE" ]]; then
        _METADATA_CACHE=$(cargo metadata --no-deps --format-version 1 2>/dev/null)
    fi
    echo "$_METADATA_CACHE" | python3 -c "
import json, sys
name = '$name'
pkgs = json.load(sys.stdin)['packages']
match = next((p['version'] for p in pkgs if p['name'] == name), '')
print(match)
"
}

# ---------------------------------------------------------------------------
# Topological publish order: leaf → root.
# Use the package name (from Cargo.toml `name =`), NOT the directory name.
# cargo publish -p <name> looks up by package name, not by directory.
# ---------------------------------------------------------------------------
CRATES=(
    # Topologically sorted (deps strictly before dependents), regenerated from
    # `cargo metadata`. Encodes two ordering fixes proven necessary by an
    # earlier failed train: the `xfa-js-sandboxed` dependency must be present,
    # and `pdf-engine` must follow its dependencies `pdf-xfa` / `pdfluent-forms`.
    # Image-codec forks sit at independent versions and are skipped at publish
    # time if unchanged.
    "xfa-dom-resolver"
    "formcalc-interpreter"
    "pdfluent-ccitt"
    "pdfluent-jbig2"
    "pdfluent-jpeg2000"
    "pdf-syntax"
    "pdfluent-lopdf"
    "pdf-annot"
    "pdfluent-cff"
    "pdf-compliance"
    "pdfluent-extract"
    "pdf-docx"
    "pdf-font"
    "pdf-interpret"
    "pdf-ocr"
    "pdf-render"
    "xfa-js-sandboxed"
    "xfa-layout-engine"
    "xfa-json"
    "pdf-xfa"
    "pdfluent-forms"
    "pdf-engine"
    "xfa-license"
    "pdf-manip"
    "pdf-redact"
    "pdfluent-sign"
    "pdfluent"
)

# ---------------------------------------------------------------------------
# Auto-resume: scan crates.io to find first unpublished crate in order.
# Sets RESUME_FROM so subsequent --from logic kicks in.
# ---------------------------------------------------------------------------
if $AUTO_RESUME && [[ -z "$RESUME_FROM" ]]; then
    log "=== AUTO-RESUME: scanning crates.io to find resume point ==="
    for CRATE in "${CRATES[@]}"; do
        VER=$(get_local_version "$CRATE")
        if [[ -z "$VER" ]]; then
            warn "  Could not determine version for $CRATE — will attempt to publish"
            RESUME_FROM="$CRATE"
            break
        fi
        crates_io_check_result=0
        crates_io_has_version "$CRATE" "$VER" || crates_io_check_result=$?
        if [[ $crates_io_check_result -ne 0 ]]; then
            log "  First unpublished: $CRATE@$VER"
            RESUME_FROM="$CRATE"
            break
        fi
        log "  Already on crates.io: $CRATE@$VER"
    done
    if [[ -z "$RESUME_FROM" ]]; then
        log "  All crates already published — nothing to do."
        exit 0
    fi
    log "  Resuming from: $RESUME_FROM"
    log ""
fi

# ---------------------------------------------------------------------------
# Header
# ---------------------------------------------------------------------------
if $LIVE; then
    log "=== LIVE PUBLISH MODE ==="
    log "Will process ${#CRATES[@]} crates (already-published will be skipped automatically)."
    log "Logfile:    $LOGFILE"
    log "State file: $STATE_FILE"
    log ""
    log "CTRL-C to abort."
    log "On partial failure: re-run with --live --resume to continue safely."
    sleep 3
else
    log "=== DRY-RUN MODE (pass --live to publish) ==="
fi

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------
SKIP=true
[[ -z "$RESUME_FROM" ]] && SKIP=false

PUBLISHED=()       # published this run
ALREADY_UP=()      # already on crates.io, skipped
DRYRUN_SKIPPED=()  # dry-run skips

for CRATE in "${CRATES[@]}"; do
    # Manual / auto-resume skip
    if [[ -n "$RESUME_FROM" ]] && $SKIP; then
        if [[ "$CRATE" == "$RESUME_FROM" ]]; then
            SKIP=false
        else
            log "SKIP  $CRATE (before resume point)"
            DRYRUN_SKIPPED+=("$CRATE")
            continue
        fi
    fi

    log "--- $CRATE ---"

    # ------------------------------------------------------------------
    # Package validation
    # Cascade note: deps on new local versions not yet on crates.io will
    # cause "failed to select a version" — expected, publish order handles it.
    # ------------------------------------------------------------------
    log "  Packaging $CRATE..."
    if cargo package -p "$CRATE" --no-verify $PKG_DIRTY_FLAG >> "$LOGFILE" 2>&1; then
        log "  Package OK"
    elif tail -20 "$LOGFILE" | grep -q "failed to select a version for the requirement"; then
        log "  Package SKIP (cascade dep not yet on crates.io — OK, publish order handles this)"
    else
        log "  Package FAILED for $CRATE — check $LOGFILE"
        die "Package failed for $CRATE. Aborting before any publish."
    fi

    if ! $LIVE; then
        log "  (dry-run only — skipping real publish)"
        DRYRUN_SKIPPED+=("$CRATE")
        continue
    fi

    # ------------------------------------------------------------------
    # Idempotency check: skip if this exact version is already on crates.io.
    # This makes the script safe to re-run after partial failures or if a
    # crate was manually published.
    # ------------------------------------------------------------------
    CRATE_VER=$(get_local_version "$CRATE")
    if [[ -n "$CRATE_VER" ]]; then
        crates_io_check=0
        crates_io_has_version "$CRATE" "$CRATE_VER" || crates_io_check=$?
        if [[ $crates_io_check -eq 0 ]]; then
            log "  ALREADY PUBLISHED $CRATE@$CRATE_VER — skipping"
            ALREADY_UP+=("$CRATE")
            continue
        fi
        # rc=2 means API error: proceed with publish attempt rather than block
    fi

    # ------------------------------------------------------------------
    # Real publish
    # ------------------------------------------------------------------
    log "  Publishing $CRATE${CRATE_VER:+@$CRATE_VER}..."
    if cargo publish -p "$CRATE" >> "$LOGFILE" 2>&1; then
        log "  Published $CRATE ✅"
        PUBLISHED+=("$CRATE")
        # Record to state file for post-mortem / human review
        echo "$CRATE" >> "$STATE_FILE"
    else
        log "  FAILED to publish $CRATE — check $LOGFILE"
        log "  Recovery: re-run with --live --resume to continue from this point."
        die "Publish failed for $CRATE."
    fi

    # Wait for crates.io index propagation before publishing dependents
    if [[ "$CRATE" != "${CRATES[$((${#CRATES[@]} - 1))]}" ]]; then
        log "  Waiting ${WAIT_SECS}s for crates.io index propagation..."
        sleep "$WAIT_SECS"
    fi
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
log ""
log "=== SUMMARY ==="
log "Published this run : ${#PUBLISHED[@]}  — ${PUBLISHED[*]:-none}"
log "Already on crates.io: ${#ALREADY_UP[@]} — ${ALREADY_UP[*]:-none}"
if [[ ${#DRYRUN_SKIPPED[@]} -gt 0 ]]; then
    log "Dry-run / pre-resume skips: ${#DRYRUN_SKIPPED[@]}"
fi
log "Log:       $LOGFILE"
if [[ -f "$STATE_FILE" ]]; then
    log "State:     $STATE_FILE"
fi

# ---------------------------------------------------------------------------
# Post-publish: tagging + GitHub release (live only)
# ---------------------------------------------------------------------------
if $LIVE && [[ ${#PUBLISHED[@]} -gt 0 ]] || ( $LIVE && [[ ${#ALREADY_UP[@]} -gt 0 ]] ); then
    log ""
    log "=== POST-PUBLISH VERIFICATION ==="

    PDFLUENT_VER=$(get_local_version "pdfluent")
    if [[ -z "$PDFLUENT_VER" ]]; then
        die "Could not determine pdfluent version from cargo metadata. Tag and release manually."
    fi

    TAG="v${PDFLUENT_VER}"
    log "  pdfluent version: $PDFLUENT_VER  →  tag: $TAG"

    # Smoke-verify the umbrella crate is visible on crates.io
    log "  Verifying crates.io visibility for pdfluent@${PDFLUENT_VER}..."
    visibility_ok=0
    crates_io_has_version "pdfluent" "$PDFLUENT_VER" || visibility_ok=$?
    if [[ $visibility_ok -eq 0 ]]; then
        log "  crates.io visibility: ✅ $PDFLUENT_VER is live"
    else
        log "  WARNING: pdfluent $PDFLUENT_VER not yet visible on crates.io (index lag — retry in 60s)"
    fi

    # Git tag (idempotent).
    # NOTE: release-binaries.yml is the SOLE creator of GitHub Releases.
    # It triggers automatically when this tag is pushed.
    if git rev-parse "$TAG" >/dev/null 2>&1; then
        log "  Tag $TAG already exists — skipping"
    else
        log "  Creating git tag $TAG..."
        git tag "$TAG"
        log "  Pushing tag..."
        git push origin "$TAG"
        log "  Tag pushed: ✅ $TAG"
        log "  → release-binaries.yml will build binaries and create the GitHub Release."
    fi

    log ""
    log "=== RELEASE COMPLETE ==="
    log "  Version:  $PDFLUENT_VER"
    log "  Tag:      $TAG (pushed)"
    log "  GitHub Release: created automatically by release-binaries.yml CI workflow."
    log "  Log:      $LOGFILE"
    log ""
    log "Verify install in a clean project:"
    log "  mkdir /tmp/smoke && cd /tmp/smoke && cargo init && cargo add pdfluent && cargo check && cd - && rm -rf /tmp/smoke"
fi
