#!/usr/bin/env bash
# Regression test for AUDIT-DOTNET-PACKAGEID-FIX.
#
# Verifies that audit-all-packages.sh extracts the NuGet <id> from a .nuspec
# regardless of grep flavour (BSD grep on macOS lacks -P), and that the
# PackageId substring check is case-insensitive (NuGet PackageIds are
# case-insensitive per spec — PDFluent, pdfluent, PDFluent.Core all valid).
#
# Cases:
#   1. <id>PDFluent</id>      → PASS (canonical mixed-case identity)
#   2. <id>pdfluent</id>      → PASS (lowercase variant)
#   3. <id>PDFluent.Core</id> → PASS (sub-package, mixed-case)
#   4. <id>XfaPdf</id>        → FAIL with clear error (non-PDFluent identity)
#   5. no <id> tag            → FAIL with clear error (extraction failure)

set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
AUDIT_SCRIPT="${SCRIPT_DIR}/../audit-all-packages.sh"

if [[ ! -f "$AUDIT_SCRIPT" ]]; then
    echo "FATAL: audit-all-packages.sh not found at $AUDIT_SCRIPT" >&2
    exit 1
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "SKIP: required command '$1' not available" >&2
        exit 0
    fi
}
require_cmd zip
require_cmd unzip

TMPROOT="$(mktemp -d -t audit_dotnet_test_XXXXXX)"
KEEP_LOGS=0
cleanup() {
    if [[ "$KEEP_LOGS" == "1" ]]; then
        echo "Logs retained at: ${TMPROOT}"
    else
        rm -rf "${TMPROOT}"
    fi
}
trap cleanup EXIT

# Source the audit script under guard so Main is skipped. This lets us call
# audit_dotnet() directly with isolated arguments — no Gate 0, no global state.
export AUDIT_ALL_PACKAGES_SOURCED=1
# shellcheck disable=SC1090
source "$AUDIT_SCRIPT"
unset AUDIT_ALL_PACKAGES_SOURCED

# Build a minimal .nupkg fixture from a nuspec body.
#   $1 = label (used in directory name)
#   $2 = nuspec XML content
# Echoes the absolute path to the resulting .nupkg.
make_nupkg() {
    local label="$1" body="$2"
    local dir="${TMPROOT}/${label}"
    mkdir -p "$dir/pkgroot"
    printf '%s' "$body" > "$dir/pkgroot/${label}.nuspec"
    printf 'PDFluent Commercial License\n' > "$dir/pkgroot/LICENSE"
    (
        cd "$dir/pkgroot"
        zip -q -r "../${label}.nupkg" .
    )
    printf '%s' "$dir/${label}.nupkg"
}

NUSPEC_PDFLUENT='<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2012/06/nuspec.xsd">
  <metadata>
    <id>PDFluent</id>
    <version>1.0.0-beta.6</version>
    <projectUrl>https://pdfluent.com/</projectUrl>
  </metadata>
</package>'

NUSPEC_LOWERCASE='<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2012/06/nuspec.xsd">
  <metadata>
    <id>pdfluent</id>
    <version>1.0.0-beta.6</version>
    <projectUrl>https://pdfluent.com/</projectUrl>
  </metadata>
</package>'

NUSPEC_SUBPKG='<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2012/06/nuspec.xsd">
  <metadata>
    <id>PDFluent.Core</id>
    <version>1.0.0-beta.6</version>
    <projectUrl>https://pdfluent.com/</projectUrl>
  </metadata>
</package>'

NUSPEC_BAD_ID='<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2012/06/nuspec.xsd">
  <metadata>
    <id>XfaPdf</id>
    <version>1.0.0-beta.6</version>
    <projectUrl>https://pdfluent.com/</projectUrl>
  </metadata>
</package>'

NUSPEC_NO_ID='<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2012/06/nuspec.xsd">
  <metadata>
    <version>1.0.0-beta.6</version>
    <projectUrl>https://pdfluent.com/</projectUrl>
  </metadata>
</package>'

NUPKG_PDFLUENT=$(make_nupkg "pdfluent_mixed" "$NUSPEC_PDFLUENT")
NUPKG_LOWERCASE=$(make_nupkg "pdfluent_lower" "$NUSPEC_LOWERCASE")
NUPKG_SUBPKG=$(make_nupkg    "pdfluent_core"  "$NUSPEC_SUBPKG")
NUPKG_BAD=$(make_nupkg       "xfapdf_bad"     "$NUSPEC_BAD_ID")
NUPKG_NO_ID=$(make_nupkg     "noid"           "$NUSPEC_NO_ID")

# Call audit_dotnet() with --dry-run semantics: it still extracts identity from
# the nuspec but skips the python audit_package_tree.py step.
run_dotnet_audit() {
    local nupkg="$1" outlog="$2"
    mkdir -p "${TMPROOT}/reports"
    # Run in a subshell so audit_dotnet's `trap RETURN` (which references a
    # function-local tmpdir) does not leak into our caller.
    set +e
    (
        NUPKG_PATH="$nupkg"
        DRY_RUN=true
        REPORTS_DIR="${TMPROOT}/reports"
        audit_dotnet
    ) >"$outlog" 2>&1
    local rc=$?
    set -e
    return $rc
}

expect_pkgid_pass() {
    local label="$1" nupkg="$2"
    local log="${TMPROOT}/$(basename "$nupkg" .nupkg).log"
    run_dotnet_audit "$nupkg" "$log" || true
    if grep -q "PackageId .* does not contain 'pdfluent'" "$log"; then
        echo "  FAIL: $label — PackageId check rejected a valid 'pdfluent' identity"
        echo "    log: $log"
        cat "$log"
        return 1
    fi
    if grep -q "PackageId could not be extracted" "$log"; then
        echo "  FAIL: $label — PackageId extraction failed on a valid nuspec"
        echo "    log: $log"
        cat "$log"
        return 1
    fi
    if ! grep -q "\.NET channel PASS" "$log"; then
        echo "  FAIL: $label — .NET channel did not report PASS"
        echo "    log: $log"
        cat "$log"
        return 1
    fi
    echo "  PASS: $label"
    return 0
}

expect_pkgid_fail() {
    local label="$1" nupkg="$2" expected_pattern="$3"
    local log="${TMPROOT}/$(basename "$nupkg" .nupkg).log"
    run_dotnet_audit "$nupkg" "$log" || true
    if ! grep -qE "$expected_pattern" "$log"; then
        echo "  FAIL: $label — expected error pattern '$expected_pattern' not found"
        echo "    log: $log"
        cat "$log"
        return 1
    fi
    if ! grep -q "\.NET channel FAIL" "$log"; then
        echo "  FAIL: $label — .NET channel did not report FAIL"
        echo "    log: $log"
        cat "$log"
        return 1
    fi
    echo "  PASS: $label (rejected as expected)"
    return 0
}

echo "=== AUDIT-DOTNET-PACKAGEID-FIX regression tests ==="

rc=0
expect_pkgid_pass "<id>PDFluent</id> (canonical mixed-case) accepted" "$NUPKG_PDFLUENT"  || rc=1
expect_pkgid_pass "<id>pdfluent</id> (lowercase) accepted"             "$NUPKG_LOWERCASE" || rc=1
expect_pkgid_pass "<id>PDFluent.Core</id> (sub-package) accepted"      "$NUPKG_SUBPKG"    || rc=1
expect_pkgid_fail "<id>XfaPdf</id> rejected"                           "$NUPKG_BAD" \
    "PackageId 'XfaPdf' does not contain 'pdfluent'"                                       || rc=1
expect_pkgid_fail "missing <id> reported with clear error"             "$NUPKG_NO_ID" \
    "PackageId could not be extracted from .nuspec"                                        || rc=1

if [[ $rc -eq 0 ]]; then
    echo "All audit-dotnet-packageid regression tests pass."
else
    echo "One or more regression tests FAILED — see logs in ${TMPROOT}"
    KEEP_LOGS=1
fi
exit $rc
