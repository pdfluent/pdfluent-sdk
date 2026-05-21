#!/usr/bin/env bash
# stage_dotnet_natives.sh — stage built native libpdf_capi.* into the .NET
# binding's runtimes/<rid>/native/ layout before `dotnet pack`.
#
# The PDFluent.csproj declares per-RID `None Include` items with
# Condition="Exists(...)", so any RID not staged is cleanly omitted from the
# package. This script copies whichever natives are available; absent RIDs
# (e.g. win-x64 without a Windows build) are skipped with a notice.
#
# Usage: scripts/release/stage_dotnet_natives.sh [PROJECT_DIR]
#   env overrides (absolute paths to prebuilt natives):
#     CAPI_LINUX_X64, CAPI_OSX_ARM64, CAPI_OSX_X64, CAPI_WIN_X64
# Default sources are the conventional cargo target paths.
set -uo pipefail
PROJ="${1:-bindings/dotnet/src/PDFluent}"
TGT="${CARGO_TARGET_DIR:-target}"

# Parallel arrays (portable to bash 3.2 / macOS; no associative arrays).
RELS="linux-x64/native/libpdf_capi.so
osx-arm64/native/libpdf_capi.dylib
osx-x64/native/libpdf_capi.dylib
win-x64/native/pdf_capi.dll"
src_for() {
  case "$1" in
    linux-x64/*)  echo "${CAPI_LINUX_X64:-$TGT/release/libpdf_capi.so}" ;;
    osx-arm64/*)  echo "${CAPI_OSX_ARM64:-$TGT/release/libpdf_capi.dylib}" ;;
    osx-x64/*)    echo "${CAPI_OSX_X64:-$TGT/x86_64-apple-darwin/release/libpdf_capi.dylib}" ;;
    win-x64/*)    echo "${CAPI_WIN_X64:-$TGT/x86_64-pc-windows-msvc/release/pdf_capi.dll}" ;;
  esac
}
staged=0; skipped=0
while IFS= read -r rel; do
  [ -n "$rel" ] || continue
  src="$(src_for "$rel")"
  dst="$PROJ/runtimes/$rel"
  if [ -f "$src" ]; then
    mkdir -p "$(dirname "$dst")"
    cp -f "$src" "$dst"
    echo "staged   $rel  <-  $src"
    staged=$((staged+1))
  else
    echo "skip     $rel  (no native at $src)"
    skipped=$((skipped+1))
  fi
done <<EOF
$RELS
EOF
echo "stage_dotnet_natives: $staged staged, $skipped skipped."
[ "$staged" -gt 0 ] || { echo "ERROR: no natives staged" >&2; exit 1; }
