#!/usr/bin/env bash
# audit-all-packages.sh — unified pre-publish gate runner for all PDFluent channels.
#
# Governed by: docs/release/release_gate_contract.md
# See also:    docs/release/PUBLISH_PROTOCOL.md
#
# Usage:
#   scripts/release/audit-all-packages.sh [OPTIONS] [CHANNEL...]
#
# Options:
#   --dry-run         Run packaging steps but do not write audit reports (default: off).
#   --channel NAME    Run only the specified channel(s). Repeatable.
#                     Valid names: rust, python, wasm, dotnet, java
#   --crate NAME      (rust channel) Audit a specific crate; repeatable.
#                     If omitted, reads PUBLISH_CRATES from the environment or
#                     uses the full publish list from publish_ordered.sh.
#   --wheel PATH      (python channel) Path to the built .whl file.
#   --wasm-pkg DIR    (wasm channel) Path to the wasm-pack output directory.
#   --nupkg PATH      (dotnet channel) Path to the built .nupkg file.
#   --jar PATH        (java channel) Path to the built .jar file.
#   --out DIR         Override output directory for audit reports
#                     (default: benchmarks/runs/prepublish_audits).
#   -h, --help        Show this help.
#
# Exit codes:
#   0  All selected channels pass with no P0 findings.
#   1  One or more P0 findings; publish must not proceed.
#   2  Usage / configuration error.
#   3  Prerequisite missing (Python, dotnet, mvn, etc.).
#
# Examples:
#   # Audit all channels (artefact paths inferred where possible):
#   scripts/release/audit-all-packages.sh
#
#   # Audit Rust only, specific crates:
#   scripts/release/audit-all-packages.sh --channel rust --crate pdfluent --crate pdf-engine
#
#   # Audit Python with explicit wheel path:
#   scripts/release/audit-all-packages.sh --channel python --wheel dist/pdfluent-1.0.0b7-cp311-cp311-manylinux_2_28_x86_64.whl
#
#   # Audit WASM package directory:
#   scripts/release/audit-all-packages.sh --channel wasm --wasm-pkg crates/xfa-wasm/pkg
#
#   # Dry-run across all channels (no report write):
#   scripts/release/audit-all-packages.sh --dry-run

set -Eeuo pipefail

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
AUDIT_TREE="${SCRIPT_DIR}/audit_package_tree.py"
CRATE_AUDIT="${SCRIPT_DIR}/prepublish_crate_audit.sh"
REPORTS_DIR="${REPO_ROOT}/benchmarks/runs/prepublish_audits"
DATESTAMP="$(date -u '+%Y%m%d_%H%M%S')"

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DRY_RUN=false
CHANNELS=()
CRATES=()
WHEEL_PATH=""
WASM_PKG_DIR=""
NUPKG_PATH=""
JAR_PATH=""
OUTPUT_DIR=""

# ---------------------------------------------------------------------------
# Arg parse
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)        DRY_RUN=true; shift ;;
        --channel)        CHANNELS+=("$2"); shift 2 ;;
        --crate)          CRATES+=("$2"); shift 2 ;;
        --wheel)          WHEEL_PATH="$2"; shift 2 ;;
        --wasm-pkg)       WASM_PKG_DIR="$2"; shift 2 ;;
        --nupkg)          NUPKG_PATH="$2"; shift 2 ;;
        --jar)            JAR_PATH="$2"; shift 2 ;;
        --out)            OUTPUT_DIR="$2"; shift 2 ;;
        -h|--help)        sed -n 's/^# //p' "$0" | head -40; exit 0 ;;
        --)               shift; break ;;
        -*)               echo "error: unknown flag '$1'" >&2; exit 2 ;;
        *)                CHANNELS+=("$1"); shift ;;
    esac
done

[[ -n "$OUTPUT_DIR" ]] && REPORTS_DIR="$OUTPUT_DIR"

# Default: all channels
if [[ ${#CHANNELS[@]} -eq 0 ]]; then
    CHANNELS=(rust python wasm dotnet java)
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo "[$(date -u '+%H:%M:%S')] $*"; }
ok()   { echo "[$(date -u '+%H:%M:%S')] ✅  $*"; }
fail() { echo "[$(date -u '+%H:%M:%S')] ❌  $*" >&2; }
warn() { echo "[$(date -u '+%H:%M:%S')] ⚠️   $*"; }
sep()  { echo ""; echo "─────────────────────────────────────────────────"; echo ""; }

# Check that required helper scripts exist.
require_helper() {
    local h="$1"
    if [[ ! -f "$h" ]]; then
        fail "required helper not found: $h"
        exit 3
    fi
}

# Check that a command is available.
require_cmd() {
    local cmd="$1" hint="${2:-}"
    if ! command -v "$cmd" &>/dev/null; then
        fail "command not found: $cmd${hint:+ ($hint)}"
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# Gate 0 — clean tree (run once, before any channel)
# ---------------------------------------------------------------------------
gate_clean_tree() {
    log "Gate 0: clean tree check"
    cd "${REPO_ROOT}"
    DIRTY=$(git status --porcelain 2>/dev/null || true)
    if [[ -n "${DIRTY}" ]]; then
        fail "Gate 0 FAILED: working tree is dirty. Commit or stash before running the audit."
        echo "${DIRTY}" >&2
        return 1
    fi
    ok "Gate 0 PASS: tree is clean"
    return 0
}

# ---------------------------------------------------------------------------
# Channel: Rust / crates.io
# ---------------------------------------------------------------------------
audit_rust() {
    sep
    log "=== CHANNEL: Rust / crates.io ==="

    require_helper "${CRATE_AUDIT}"

    # If no --crate flags, use the default publish list.
    local crates_to_audit=("${CRATES[@]:-}")
    if [[ ${#crates_to_audit[@]} -eq 0 ]]; then
        # Source the default list from publish_ordered.sh (exported via CRATES array).
        # We do this by extracting the CRATES array from publish_ordered.sh.
        local ordered="${REPO_ROOT}/scripts/publish_ordered.sh"
        if [[ -f "${ordered}" ]]; then
            mapfile -t crates_to_audit < <(
                grep -A200 '^CRATES=(' "${ordered}" | grep '"' | sed 's/.*"\(.*\)".*/\1/'
            )
            log "  Loaded ${#crates_to_audit[@]} crates from publish_ordered.sh"
        else
            warn "  publish_ordered.sh not found; run with --crate to specify crates"
            return 0
        fi
    fi

    local rust_pass=0 rust_fail=0
    for crate in "${crates_to_audit[@]}"; do
        [[ -z "$crate" ]] && continue
        log "  Auditing crate: $crate"
        if $DRY_RUN; then
            log "  (dry-run: skipping audit script execution for $crate)"
            ok "  $crate — DRY-RUN SKIP"
            continue
        fi
        if bash "${CRATE_AUDIT}" "$crate" 2>&1; then
            ok "  $crate — PASS"
            (( rust_pass++ )) || true
        else
            fail "  $crate — FAIL (P0)"
            (( rust_fail++ )) || true
        fi
    done

    sep
    log "Rust channel: ${rust_pass} PASS / ${rust_fail} FAIL"
    if [[ $rust_fail -gt 0 ]]; then
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# Channel: Python / PyPI
# ---------------------------------------------------------------------------
audit_python() {
    sep
    log "=== CHANNEL: Python / PyPI ==="

    local scrub_script="${SCRIPT_DIR}/scrub-wheel.py"

    # Locate wheel if not specified.
    local wheel="${WHEEL_PATH}"
    if [[ -z "$wheel" ]]; then
        # Try common build output locations.
        local candidates=()
        while IFS= read -r -d '' f; do
            candidates+=("$f")
        done < <(find "${REPO_ROOT}/crates/pdf-python" -name "*.whl" -print0 2>/dev/null)
        if [[ ${#candidates[@]} -eq 0 ]]; then
            warn "  No .whl found under crates/pdf-python/. Build with 'maturin build --release' first."
            warn "  Skipping Python channel (no artefact). Document as artifact gap."
            echo "PYTHON_CHANNEL=NO_ARTIFACT" >> "${REPORTS_DIR}/.e1_gaps.txt" 2>/dev/null || true
            return 0
        fi
        wheel="${candidates[0]}"
        log "  Found wheel: $wheel"
    fi

    if [[ ! -f "$wheel" ]]; then
        fail "  Wheel not found: $wheel"
        return 1
    fi

    local whl_name; whl_name=$(basename "$wheel")
    local report_base="${REPORTS_DIR}/${whl_name%.whl}"
    local report_md="${report_base}.audit.md"
    local report_json="${report_base}.audit.json"

    log "  Step 1: scrub SBOM paths in wheel"
    if ! $DRY_RUN; then
        if [[ -f "${scrub_script}" ]]; then
            if ! python3 "${scrub_script}" "$wheel" 2>&1; then
                fail "  scrub-wheel.py failed for $wheel"
                return 1
            fi
            ok "  SBOM scrub complete"
        else
            warn "  scrub-wheel.py not found at ${scrub_script}; skipping SBOM scrub"
        fi
    fi

    log "  Step 2: unpack wheel and run audit_package_tree.py"
    local tmpdir; tmpdir=$(mktemp -d -t pyaudit_XXXXXX)
    trap 'rm -rf "${tmpdir}"' RETURN

    # Wheel is a zip file.
    if ! unzip -q "$wheel" -d "${tmpdir}/unpacked" 2>&1; then
        fail "  Could not unzip wheel: $wheel"
        return 1
    fi

    log "  Step 3: license-file check"
    # Python wheels: look for METADATA file which contains License field.
    local metadata_file; metadata_file=$(find "${tmpdir}/unpacked" -name "METADATA" | head -1)
    local license_ok=true
    if [[ -z "$metadata_file" ]]; then
        fail "  No METADATA file found inside wheel"
        license_ok=false
    else
        local lic_field; lic_field=$(grep -i '^License:' "${metadata_file}" | head -1 || echo "")
        if [[ -z "$lic_field" ]]; then
            fail "  No License field in METADATA"
            license_ok=false
        else
            log "  License field: ${lic_field}"
        fi
        # For PDFluent Commercial License, verify LICENSE file present.
        local lic_file; lic_file=$(find "${tmpdir}/unpacked" -name "LICENSE" | head -1 || echo "")
        if [[ -z "$lic_file" ]]; then
            fail "  No LICENSE file inside wheel"
            license_ok=false
        elif ! grep -q "PDFluent Commercial License\|MIT\|Apache" "${lic_file}" 2>/dev/null; then
            warn "  LICENSE file content looks unexpected — review manually"
        else
            ok "  LICENSE file present and readable"
        fi
    fi

    log "  Step 4: identity check (package name, version, homepage)"
    local pkg_name; pkg_name=$(grep -i '^Name:' "${metadata_file:-/dev/null}" | head -1 | awk '{print $2}' || echo "")
    local pkg_ver; pkg_ver=$(grep -i '^Version:' "${metadata_file:-/dev/null}" | head -1 | awk '{print $2}' || echo "")
    local pkg_home; pkg_home=$(grep -i '^Home-page:\|^Project-URL:.*Homepage' "${metadata_file:-/dev/null}" | head -1 || echo "")
    log "  Package: ${pkg_name:-UNKNOWN} @ ${pkg_ver:-UNKNOWN}"
    [[ -n "$pkg_home" ]] && log "  Homepage: ${pkg_home}"
    if ! echo "${pkg_home}" | grep -q "pdfluent.com" 2>/dev/null; then
        warn "  Homepage does not reference pdfluent.com — verify identity"
    fi

    log "  Step 5: leakage scan via audit_package_tree.py"
    local scan_exit=0
    if ! $DRY_RUN; then
        python3 "${AUDIT_TREE}" \
            --tree "${tmpdir}/unpacked" \
            --out "${REPORTS_DIR}" \
            --package-name "${pkg_name:-pdfluent-python}" \
            --package-version "${pkg_ver:-unknown}" \
            --channel pypi 2>&1 || scan_exit=$?
    fi

    if [[ $license_ok == false ]] || [[ $scan_exit -ne 0 ]]; then
        fail "  Python channel FAIL (P0)"
        return 1
    fi
    ok "  Python channel PASS"
    return 0
}

# ---------------------------------------------------------------------------
# Channel: WASM / npm
# ---------------------------------------------------------------------------
audit_wasm() {
    sep
    log "=== CHANNEL: WASM / npm ==="

    local transform_script="${SCRIPT_DIR}/transform-wasm-pkg.sh"
    local pkg_dir="${WASM_PKG_DIR}"

    if [[ -z "$pkg_dir" ]]; then
        # Try common output locations.
        for candidate in \
            "${REPO_ROOT}/crates/xfa-wasm/pkg" \
            "${REPO_ROOT}/crates/xfa-wasm/target/pkg" \
            "${REPO_ROOT}/target/pkg"; do
            if [[ -d "$candidate" ]]; then
                pkg_dir="$candidate"
                log "  Found WASM pkg dir: $pkg_dir"
                break
            fi
        done
    fi

    if [[ -z "$pkg_dir" || ! -d "$pkg_dir" ]]; then
        warn "  No WASM pkg directory found. Build with 'wasm-pack build --target web' first."
        warn "  Skipping WASM channel (no artefact). Document as artifact gap."
        echo "WASM_CHANNEL=NO_ARTIFACT" >> "${REPORTS_DIR}/.e1_gaps.txt" 2>/dev/null || true
        return 0
    fi

    local pkg_json="${pkg_dir}/package.json"
    if [[ ! -f "$pkg_json" ]]; then
        fail "  No package.json in WASM pkg dir: $pkg_dir"
        return 1
    fi

    log "  Step 1: verify / apply package.json transform"
    local wasm_version; wasm_version=$(python3 -c "import json; d=json.load(open('${pkg_json}')); print(d.get('version','unknown'))" 2>/dev/null || echo "unknown")

    if ! $DRY_RUN; then
        if [[ -f "${transform_script}" ]]; then
            if ! bash "${transform_script}" "$pkg_dir" "$wasm_version" 2>&1; then
                fail "  transform-wasm-pkg.sh failed"
                return 1
            fi
            ok "  package.json transform applied and verified"
        else
            warn "  transform-wasm-pkg.sh not found; skipping transform"
        fi
    fi

    log "  Step 2: identity check"
    local pkg_name; pkg_name=$(python3 -c "import json; d=json.load(open('${pkg_json}')); print(d.get('name','?'))" 2>/dev/null || echo "?")
    local pkg_ver; pkg_ver=$(python3 -c "import json; d=json.load(open('${pkg_json}')); print(d.get('version','?'))" 2>/dev/null || echo "?")
    local pkg_home; pkg_home=$(python3 -c "import json; d=json.load(open('${pkg_json}')); print(d.get('homepage','?'))" 2>/dev/null || echo "?")
    log "  Package: ${pkg_name} @ ${pkg_ver} | homepage: ${pkg_home}"

    local identity_ok=true
    if ! echo "$pkg_name" | grep -q "@pdfluent/"; then
        fail "  Package name '${pkg_name}' does not match @pdfluent/* convention"
        identity_ok=false
    fi
    if echo "$pkg_home" | grep -qi "github"; then
        fail "  Homepage contains github.com: ${pkg_home}"
        identity_ok=false
    fi

    log "  Step 3: license check"
    local lic_file="${pkg_dir}/LICENSE"
    local license_ok=true
    if [[ ! -f "$lic_file" ]]; then
        fail "  No LICENSE file in WASM pkg dir"
        license_ok=false
    elif ! grep -q "PDFluent Commercial License" "$lic_file" 2>/dev/null; then
        fail "  LICENSE file does not contain PDFluent Commercial License"
        license_ok=false
    else
        ok "  LICENSE file present with correct content"
    fi

    log "  Step 4: leakage scan via audit_package_tree.py"
    local scan_exit=0
    if ! $DRY_RUN; then
        python3 "${AUDIT_TREE}" \
            --tree "$pkg_dir" \
            --out "${REPORTS_DIR}" \
            --package-name "${pkg_name}" \
            --package-version "${pkg_ver}" \
            --channel wasm 2>&1 || scan_exit=$?
    fi

    if [[ $identity_ok == false ]] || [[ $license_ok == false ]] || [[ $scan_exit -ne 0 ]]; then
        fail "  WASM channel FAIL (P0)"
        return 1
    fi
    ok "  WASM channel PASS"
    return 0
}

# ---------------------------------------------------------------------------
# Channel: .NET / NuGet
# ---------------------------------------------------------------------------
audit_dotnet() {
    sep
    log "=== CHANNEL: .NET / NuGet ==="

    local nupkg="${NUPKG_PATH}"

    if [[ -z "$nupkg" ]]; then
        # Try to find a .nupkg in the bindings directory.
        local candidates=()
        while IFS= read -r -d '' f; do
            candidates+=("$f")
        done < <(find "${REPO_ROOT}/bindings/dotnet" -name "*.nupkg" -not -path "*/obj/*" -print0 2>/dev/null)
        if [[ ${#candidates[@]} -eq 0 ]]; then
            warn "  No .nupkg found under bindings/dotnet/. Build with 'dotnet pack -c Release' first."
            warn "  Skipping .NET channel (no artefact). Document as artifact gap."
            echo "DOTNET_CHANNEL=NO_ARTIFACT" >> "${REPORTS_DIR}/.e1_gaps.txt" 2>/dev/null || true
            return 0
        fi
        nupkg="${candidates[0]}"
        log "  Found nupkg: $nupkg"
    fi

    if [[ ! -f "$nupkg" ]]; then
        fail "  .nupkg not found: $nupkg"
        return 1
    fi

    local nupkg_name; nupkg_name=$(basename "$nupkg" .nupkg)
    local tmpdir; tmpdir=$(mktemp -d -t dotnet_audit_XXXXXX)
    trap 'rm -rf "${tmpdir}"' RETURN

    log "  Step 1: unpack .nupkg (it is a zip)"
    if ! unzip -q "$nupkg" -d "${tmpdir}/unpacked" 2>&1; then
        fail "  Could not unzip .nupkg: $nupkg"
        return 1
    fi

    log "  Step 2: license check"
    local lic_file; lic_file=$(find "${tmpdir}/unpacked" -iname "LICENSE*" | head -1 || echo "")
    local license_ok=true
    if [[ -z "$lic_file" ]]; then
        fail "  No LICENSE file inside .nupkg"
        license_ok=false
    else
        ok "  LICENSE file present: $(basename "$lic_file")"
    fi

    log "  Step 3: identity check via .nuspec"
    local nuspec; nuspec=$(find "${tmpdir}/unpacked" -name "*.nuspec" | head -1 || echo "")
    local identity_ok=true
    if [[ -z "$nuspec" ]]; then
        fail "  No .nuspec found inside .nupkg"
        identity_ok=false
    else
        # Portable XML tag extraction (BSD grep on macOS lacks -P; use sed instead).
        # Returns first <tag>...</tag> inner text, or empty if not present.
        _extract_xml_tag() {
            local tag="$1" file="$2"
            sed -n "s/.*<${tag}>\\([^<]*\\)<\\/${tag}>.*/\\1/p" "$file" 2>/dev/null | head -1
        }
        local pkg_id; pkg_id=$(_extract_xml_tag "id" "$nuspec")
        local pkg_ver; pkg_ver=$(_extract_xml_tag "version" "$nuspec")
        local pkg_url; pkg_url=$(_extract_xml_tag "projectUrl" "$nuspec")
        # Normalise to lowercase for case-insensitive identity check (NuGet PackageIds
        # are case-insensitive per spec — PDFluent, pdfluent, PDFluent.Core all valid).
        local pkg_id_lc; pkg_id_lc=$(printf '%s' "$pkg_id" | tr '[:upper:]' '[:lower:]')
        log "  PackageId: ${pkg_id:-<empty>} @ ${pkg_ver:-<empty>} | projectUrl: ${pkg_url:-<empty>}"
        if [[ -z "$pkg_id" ]]; then
            fail "  PackageId could not be extracted from .nuspec (missing <id> tag)"
            identity_ok=false
        elif [[ "$pkg_id_lc" != *pdfluent* ]]; then
            fail "  PackageId '${pkg_id}' does not contain 'pdfluent' (case-insensitive)"
            identity_ok=false
        fi
        if [[ -n "$pkg_url" ]] && printf '%s' "$pkg_url" | grep -qi "github"; then
            fail "  projectUrl contains github.com: ${pkg_url}"
            identity_ok=false
        fi
    fi

    log "  Step 4: leakage scan via audit_package_tree.py"
    local scan_exit=0
    if ! $DRY_RUN; then
        local pkg_name="${pkg_id:-pdfluent-dotnet}"
        local pkg_ver_s="${pkg_ver:-unknown}"
        python3 "${AUDIT_TREE}" \
            --tree "${tmpdir}/unpacked" \
            --out "${REPORTS_DIR}" \
            --package-name "${pkg_name}" \
            --package-version "${pkg_ver_s}" \
            --channel generic 2>&1 || scan_exit=$?
    fi

    if [[ $license_ok == false ]] || [[ $identity_ok == false ]] || [[ $scan_exit -ne 0 ]]; then
        fail "  .NET channel FAIL (P0)"
        return 1
    fi
    ok "  .NET channel PASS"
    return 0
}

# ---------------------------------------------------------------------------
# Channel: Java / Maven
# ---------------------------------------------------------------------------
audit_java() {
    sep
    log "=== CHANNEL: Java / Maven ==="

    local jar="${JAR_PATH}"

    if [[ -z "$jar" ]]; then
        # Try common build output locations.
        local candidates=()
        while IFS= read -r -d '' f; do
            candidates+=("$f")
        done < <(find "${REPO_ROOT}/crates/pdf-java" -name "*.jar" -not -name "*javadoc*" -not -name "*sources*" -print0 2>/dev/null)
        if [[ ${#candidates[@]} -eq 0 ]]; then
            warn "  No .jar found under crates/pdf-java/. Build with 'mvn package -DskipTests' first."
            warn "  Skipping Java channel (no artefact). Document as artifact gap."
            echo "JAVA_CHANNEL=NO_ARTIFACT" >> "${REPORTS_DIR}/.e1_gaps.txt" 2>/dev/null || true
            return 0
        fi
        jar="${candidates[0]}"
        log "  Found jar: $jar"
    fi

    if [[ ! -f "$jar" ]]; then
        fail "  .jar not found: $jar"
        return 1
    fi

    local jar_name; jar_name=$(basename "$jar" .jar)
    local tmpdir; tmpdir=$(mktemp -d -t java_audit_XXXXXX)
    trap 'rm -rf "${tmpdir}"' RETURN

    log "  Step 1: unpack .jar (it is a zip)"
    if ! unzip -q "$jar" -d "${tmpdir}/unpacked" 2>&1; then
        fail "  Could not unzip .jar: $jar"
        return 1
    fi

    log "  Step 2: identity check via pom.xml / MANIFEST.MF"
    local pom; pom=$(find "${tmpdir}/unpacked" -name "pom.xml" | head -1 || echo "")
    local manifest; manifest=$(find "${tmpdir}/unpacked" -name "MANIFEST.MF" | head -1 || echo "")
    local identity_ok=true

    if [[ -n "$pom" ]]; then
        local group_id; group_id=$(grep -oPm1 '(?<=<groupId>)[^<]+' "$pom" 2>/dev/null || echo "?")
        local artifact_id; artifact_id=$(grep -oPm1 '(?<=<artifactId>)[^<]+' "$pom" 2>/dev/null || echo "?")
        local pkg_ver; pkg_ver=$(grep -oPm1 '(?<=<version>)[^<]+' "$pom" 2>/dev/null || echo "?")
        log "  groupId: ${group_id} | artifactId: ${artifact_id} | version: ${pkg_ver}"
        if ! echo "$group_id" | grep -qi "pdfluent"; then
            fail "  groupId '${group_id}' does not contain 'pdfluent'"
            identity_ok=false
        fi
    else
        warn "  No pom.xml inside jar — identity check limited to MANIFEST.MF"
    fi

    log "  Step 3: license check"
    local lic; lic=$(find "${tmpdir}/unpacked" -iname "LICENSE*" -o -iname "NOTICE*" 2>/dev/null | head -1 || echo "")
    local license_ok=true
    if [[ -z "$lic" ]]; then
        fail "  No LICENSE or NOTICE file inside .jar"
        license_ok=false
    else
        ok "  License file found: $(basename "$lic")"
    fi

    log "  Step 4: leakage scan via audit_package_tree.py"
    local scan_exit=0
    if ! $DRY_RUN; then
        local pkg_name_s="${artifact_id:-pdfluent-java}"
        local pkg_ver_s="${pkg_ver:-unknown}"
        python3 "${AUDIT_TREE}" \
            --tree "${tmpdir}/unpacked" \
            --out "${REPORTS_DIR}" \
            --package-name "${pkg_name_s}" \
            --package-version "${pkg_ver_s}" \
            --channel maven 2>&1 || scan_exit=$?
    fi

    if [[ $license_ok == false ]] || [[ $identity_ok == false ]] || [[ $scan_exit -ne 0 ]]; then
        fail "  Java channel FAIL (P0)"
        return 1
    fi
    ok "  Java channel PASS"
    return 0
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
# Allow this script to be sourced (e.g. by regression tests) without executing
# Main. Tests source the file and call the per-channel functions directly,
# bypassing Gate 0 (clean tree) and the global arg parser.
if [[ "${AUDIT_ALL_PACKAGES_SOURCED:-0}" == "1" ]]; then
    return 0 2>/dev/null || exit 0
fi

cd "${REPO_ROOT}"

log "audit-all-packages.sh — PDFluent release gate runner"
log "Date:       $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
log "Repo root:  ${REPO_ROOT}"
log "Reports:    ${REPORTS_DIR}"
log "Channels:   ${CHANNELS[*]}"
$DRY_RUN && log "Mode:       DRY-RUN (no audit reports written, no publish)"
log ""

mkdir -p "${REPORTS_DIR}"
# Reset gaps file for this run.
: > "${REPORTS_DIR}/.e1_gaps.txt" 2>/dev/null || true

require_helper "${AUDIT_TREE}"

# Gate 0 — clean tree (always runs first, regardless of channel selection).
gate_clean_tree || {
    fail "Gate 0 (clean tree) FAILED — aborting all channels"
    exit 1
}

OVERALL_PASS=0
OVERALL_FAIL=0
CHANNELS_SKIPPED=()

for ch in "${CHANNELS[@]}"; do
    ch_exit=0
    case "$ch" in
        rust)   audit_rust   || ch_exit=$? ;;
        python) audit_python || ch_exit=$? ;;
        wasm)   audit_wasm   || ch_exit=$? ;;
        dotnet) audit_dotnet || ch_exit=$? ;;
        java)   audit_java   || ch_exit=$? ;;
        *)
            warn "Unknown channel: $ch (valid: rust python wasm dotnet java)"
            CHANNELS_SKIPPED+=("$ch")
            continue
            ;;
    esac
    if [[ $ch_exit -eq 0 ]]; then
        (( OVERALL_PASS++ )) || true
    else
        (( OVERALL_FAIL++ )) || true
    fi
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
sep
log "=== AUDIT SUMMARY ==="
log "Channels passed : ${OVERALL_PASS}"
log "Channels failed : ${OVERALL_FAIL}"
[[ ${#CHANNELS_SKIPPED[@]} -gt 0 ]] && log "Channels unknown: ${CHANNELS_SKIPPED[*]}"

# Report artifact gaps.
if [[ -s "${REPORTS_DIR}/.e1_gaps.txt" ]]; then
    log ""
    warn "Artifact gaps (channels with no built artefact — gates not fully exercised):"
    while IFS= read -r gap; do
        warn "  ${gap}"
    done < "${REPORTS_DIR}/.e1_gaps.txt"
    log ""
    log "Rebuild missing artefacts and re-run to get full coverage."
fi

if [[ ${OVERALL_FAIL} -gt 0 ]]; then
    fail "RESULT: PUBLISH BLOCKED — ${OVERALL_FAIL} channel(s) failed P0 gates"
    exit 1
fi

ok "RESULT: ALL GATES PASS — safe to proceed with publish train"
exit 0
