#!/usr/bin/env bash
# package_cabi.sh — build and package the PDFluent C ABI distribution tarball.
#
# Produces target/release/pdfluent-capi-<version>.tar.gz containing:
#   pdfluent-capi-<version>/
#     include/         (C headers from crates/pdf-capi/include/)
#     lib/             (cdylib + staticlib from cargo build, host triple only)
#     LICENSE          (AGPL text, one half of the dual licence)
#     LICENSE-COMMERCIAL (commercial text, the other half)
#     NOTICE           (license composition notice, as the other channels ship)
#     README.md        (consumer-facing build/usage notes)
#     VERSION          (plain text version string, matches Cargo.toml)
#
# Per-platform release mode (--lib/--plat) cuts one tarball per platform
# instead: <out>/pdfluent-capi-<version>-<platform>.tar.gz, each with the
# same layout but the one lib for that platform under lib/. The release
# workflow builds the seven libs on their own machines and calls this
# seven times; the publish uploads the seven tarballs.
#
# Governed by: docs/release/cabi_packaging.md
# Usage:
#   bash scripts/release/package_cabi.sh [--skip-build]
#   bash scripts/release/package_cabi.sh --lib PATH --plat PLATFORM [--out DIR]
#
# Exit codes:
#   0  Tarball produced and SHA-256 reported.
#   1  Build or packaging failure.
#   2  Usage / configuration error.

set -Eeuo pipefail

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
CAPI_DIR="${REPO_ROOT}/crates/pdf-capi"
CARGO_TOML="${CAPI_DIR}/Cargo.toml"
TARGET_DIR="${REPO_ROOT}/target/release"

# ---------------------------------------------------------------------------
# Args
# ---------------------------------------------------------------------------
SKIP_BUILD=false
LIB_OVERRIDE=""
PLAT_OVERRIDE=""
OUT_OVERRIDE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build) SKIP_BUILD=true; shift ;;
        --lib) LIB_OVERRIDE="$2"; shift 2 ;;
        --plat) PLAT_OVERRIDE="$2"; shift 2 ;;
        --out) OUT_OVERRIDE="$2"; shift 2 ;;
        -h|--help)
            sed -n 's/^# //p' "$0" | head -30
            exit 0
            ;;
        *) echo "error: unknown flag '$1'" >&2; exit 2 ;;
    esac
done
# The tarball is cut from inside the staging dir (step 8), so a relative
# --out would land there and tar cannot open it: anchor it to the caller's
# directory now, before anything changes directory.
if [[ -n "$OUT_OVERRIDE" && "$OUT_OVERRIDE" != /* ]]; then
    OUT_OVERRIDE="$PWD/$OUT_OVERRIDE"
fi

log()  { echo "[$(date -u '+%H:%M:%S')] $*"; }
ok()   { echo "[$(date -u '+%H:%M:%S')] ✅  $*"; }
fail() { echo "[$(date -u '+%H:%M:%S')] ❌  $*" >&2; }

# Per-platform mode needs both halves, and the platform must be one of the
# seven the artefact gate knows (its PLATFORMS keys).
PLATFORMS="linux-x64 linux-arm64 linux-x64-musl linux-arm64-musl osx-x64 osx-arm64 win-x64"
if [[ -n "$LIB_OVERRIDE" || -n "$PLAT_OVERRIDE" ]]; then
    [[ -n "$LIB_OVERRIDE" && -n "$PLAT_OVERRIDE" ]] \
        || { echo "error: --lib and --plat come as a pair" >&2; exit 2; }
    [[ " $PLATFORMS " == *" $PLAT_OVERRIDE "* ]] \
        || { echo "error: unknown platform '$PLAT_OVERRIDE' (want one of: $PLATFORMS)" >&2; exit 2; }
    [[ -f "$LIB_OVERRIDE" ]] \
        || { fail "No such lib: $LIB_OVERRIDE"; exit 1; }
fi

# ---------------------------------------------------------------------------
# 1. Read version from Cargo.toml
# ---------------------------------------------------------------------------
if [[ ! -f "$CARGO_TOML" ]]; then
    fail "Missing Cargo.toml: $CARGO_TOML"
    exit 1
fi

VERSION=$(grep -m1 '^version = ' "$CARGO_TOML" | sed 's/.*"\(.*\)".*/\1/')
if [[ -z "$VERSION" ]]; then
    fail "Could not parse version from $CARGO_TOML"
    exit 1
fi
log "C ABI version: $VERSION"

if [[ -n "$PLAT_OVERRIDE" ]]; then
    PKG_NAME="pdfluent-capi-${VERSION}-${PLAT_OVERRIDE}"
else
    PKG_NAME="pdfluent-capi-${VERSION}"
fi
STAGING_PARENT="$(mktemp -d -t cabi_pkg_XXXXXX)"
STAGING="${STAGING_PARENT}/${PKG_NAME}"
mkdir -p "${STAGING}/include" "${STAGING}/lib"
trap 'rm -rf "${STAGING_PARENT}"' EXIT

# ---------------------------------------------------------------------------
# 2. Build cdylib + staticlib (unless --skip-build)
# ---------------------------------------------------------------------------
if [[ -n "$LIB_OVERRIDE" ]]; then
    log "Per-platform mode: using $LIB_OVERRIDE, no build"
elif $SKIP_BUILD; then
    log "Skipping cargo build (--skip-build given)"
else
    log "Building pdf-capi (release) ..."
    cd "$REPO_ROOT"
    if ! cargo build -p pdf-capi --release 2>&1 | tail -5; then
        fail "cargo build failed"
        exit 1
    fi
    ok "Build complete"
fi

# ---------------------------------------------------------------------------
# 3. Copy headers
# ---------------------------------------------------------------------------
log "Copying headers ..."
if [[ ! -d "${CAPI_DIR}/include" ]]; then
    fail "Missing include dir: ${CAPI_DIR}/include"
    exit 1
fi
cp "${CAPI_DIR}/include/"*.h "${STAGING}/include/"
ok "Headers: $(ls "${STAGING}/include/" | wc -l | tr -d ' ') file(s)"

# ---------------------------------------------------------------------------
# 4. Copy native libs (cdylib only)
#
# Only the shared library (cdylib) is bundled. The static archive (.a/.lib)
# is intentionally excluded: it embeds source-path string literals from
# pdf-manip resources and absolute paths from precompiled compiler_builtins,
# which leak build-environment paths. The dylib is the canonical C ABI
# distribution artefact (auditable, path-clean via --remap-path-prefix).
# ---------------------------------------------------------------------------
LIB_COUNT=0
if [[ -n "$LIB_OVERRIDE" ]]; then
    log "Staging the given lib ..."
    cp "$LIB_OVERRIDE" "${STAGING}/lib/"
    log "  Copied: $(basename "$LIB_OVERRIDE")"
    LIB_COUNT=1
else
    log "Locating built shared library in ${TARGET_DIR} ..."
    for ext in dylib so dll; do
        for prefix in lib ""; do
            for stem in pdfluent_capi pdf_capi; do
                candidate="${TARGET_DIR}/${prefix}${stem}.${ext}"
                if [[ -f "$candidate" ]]; then
                    cp "$candidate" "${STAGING}/lib/"
                    log "  Copied: $(basename "$candidate")"
                    LIB_COUNT=$((LIB_COUNT + 1))
                fi
            done
        done
    done

    if [[ $LIB_COUNT -eq 0 ]]; then
        fail "No pdf-capi shared library found in ${TARGET_DIR}. Did the build succeed?"
        fail "Looked for: libpdf_capi.{dylib,so} / pdf_capi.dll / libpdfluent_capi.*"
        exit 1
    fi
fi
ok "Libs: ${LIB_COUNT} file(s)"

# macOS: the linker stamps the dylib's LC_ID_DYLIB with the full build-output
# path (e.g. /Users/<dev>/.../target/release/deps/libpdf_capi.dylib), which
# leaks the workstation path. Rewrite to @rpath/libpdf_capi.dylib so the
# install name is portable and audit-clean.
if command -v install_name_tool >/dev/null 2>&1; then
    for f in "${STAGING}/lib/"*.dylib; do
        [[ -f "$f" ]] || continue
        base=$(basename "$f")
        if install_name_tool -id "@rpath/${base}" "$f" 2>/dev/null; then
            log "  Rewrote install_name: ${base} → @rpath/${base}"
        fi
    done
fi

# ---------------------------------------------------------------------------
# 5. Copy LICENSE, LICENSE-COMMERCIAL and NOTICE
# ---------------------------------------------------------------------------
# The product is dual-licensed: the tarball carries both halves of the
# dual text, not the AGPL LICENSE alone. NOTICE travels with the other
# channels' archives (CLI, wheels), so it travels here too.
log "Copying licence files ..."
if [[ ! -f "${CAPI_DIR}/LICENSE" ]]; then
    fail "Missing ${CAPI_DIR}/LICENSE."
    exit 1
fi
if [[ ! -f "${CAPI_DIR}/LICENSE-COMMERCIAL" ]]; then
    fail "Missing ${CAPI_DIR}/LICENSE-COMMERCIAL."
    exit 1
fi
if [[ ! -f "${REPO_ROOT}/NOTICE" ]]; then
    fail "Missing ${REPO_ROOT}/NOTICE."
    exit 1
fi
cp "${CAPI_DIR}/LICENSE" "${STAGING}/LICENSE"
cp "${CAPI_DIR}/LICENSE-COMMERCIAL" "${STAGING}/LICENSE-COMMERCIAL"
cp "${REPO_ROOT}/NOTICE" "${STAGING}/NOTICE"
ok "LICENSE, LICENSE-COMMERCIAL and NOTICE copied"

# ---------------------------------------------------------------------------
# 6. Copy README
# ---------------------------------------------------------------------------
log "Copying README ..."
if [[ -f "${CAPI_DIR}/README.md" ]]; then
    cp "${CAPI_DIR}/README.md" "${STAGING}/README.md"
    ok "README.md copied"
else
    fail "Missing ${CAPI_DIR}/README.md"
    exit 1
fi

# ---------------------------------------------------------------------------
# 7. Write VERSION file
# ---------------------------------------------------------------------------
echo "$VERSION" > "${STAGING}/VERSION"
ok "VERSION file written: $VERSION"

# ---------------------------------------------------------------------------
# 8. Create tarball
# ---------------------------------------------------------------------------
OUT_DIR="${OUT_OVERRIDE:-${TARGET_DIR}}"
mkdir -p "${OUT_DIR}"
TARBALL="${OUT_DIR}/${PKG_NAME}.tar.gz"
log "Creating tarball: ${TARBALL}"

# Tar from staging parent so archive root is PKG_NAME/.
(cd "${STAGING_PARENT}" && tar -czf "${TARBALL}" "${PKG_NAME}")

if [[ ! -f "$TARBALL" ]]; then
    fail "Tarball creation failed"
    exit 1
fi

TARBALL_SIZE=$(du -h "$TARBALL" | awk '{print $1}')
ok "Tarball created (${TARBALL_SIZE})"

# ---------------------------------------------------------------------------
# 9. SHA-256
# ---------------------------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then
    SHA=$(sha256sum "$TARBALL" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    SHA=$(shasum -a 256 "$TARBALL" | awk '{print $1}')
else
    SHA="(sha256 tool not found)"
fi

# ---------------------------------------------------------------------------
# 10. Summary
# ---------------------------------------------------------------------------
echo ""
echo "─────────────────────────────────────────────────"
echo "C ABI tarball ready:"
echo "  Path:     ${TARBALL}"
echo "  Version:  ${VERSION}"
echo "  Size:     ${TARBALL_SIZE}"
echo "  SHA-256:  ${SHA}"
echo "  Contents:"
tar -tzf "${TARBALL}" | sed 's/^/    /'
echo "─────────────────────────────────────────────────"
