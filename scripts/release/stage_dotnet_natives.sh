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

declare -A SRC=(
  [linux-x64/native/libpdf_capi.so]="${CAPI_LINUX_X64:-$TGT/release/libpdf_capi.so}"
  [osx-arm64/native/libpdf_capi.dylib]="${CAPI_OSX_ARM64:-$TGT/release/libpdf_capi.dylib}"
  [osx-x64/native/libpdf_capi.dylib]="${CAPI_OSX_X64:-$TGT/x86_64-apple-darwin/release/libpdf_capi.dylib}"
  [win-x64/native/pdf_capi.dll]="${CAPI_WIN_X64:-$TGT/x86_64-pc-windows-msvc/release/pdf_capi.dll}"
)
staged=0; skipped=0
for rel in "${!SRC[@]}"; do
  src="${SRC[$rel]}"
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
done
echo "stage_dotnet_natives: $staged staged, $skipped skipped."
[ "$staged" -gt 0 ] || { echo "ERROR: no natives staged" >&2; exit 1; }
