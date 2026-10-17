#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# run_r4_dotnet_tests.sh — the r4 .NET suite against THIS tree.
#
# Builds the pdf-capi C ABI library for the dotnet runtime's own
# architecture -- on this shop's mixed Macs dotnet is x64 while rustc
# defaults to arm64, and tests_actually_ran.py exists because a library for
# the other processor once reported the whole suite green without running a
# single test -- and runs the PDFluent.Tests project, which carries the r4
# bodies: the seven mirrored tests (Skip ON until the desktop run proves
# them green), the known-images body (Skip ON), and ImageExtractionTests. A
# positive executed count is required, so an all-skipped run still fails.
#
# Slow on a cold target dir (a debug cdylib build); incremental after that.
# Used by the landing/full lanes and the scoped CI step, never the fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo dotnet; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_r4_dotnet_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

OS="$(uname -s)"
DOTNET_ARCH="$(dotnet --info 2>/dev/null | awk '/^ *Architecture:/ {print $2}' | tail -1)"
TARGET=""
case "$OS/$DOTNET_ARCH" in
  Darwin/x64) TARGET="x86_64-apple-darwin"; LIB_EXT="dylib"; LIB_VAR="DYLD_LIBRARY_PATH" ;;
  Darwin/*) LIB_EXT="dylib"; LIB_VAR="DYLD_LIBRARY_PATH" ;;
  Linux/x64) TARGET="x86_64-unknown-linux-gnu"; LIB_EXT="so"; LIB_VAR="LD_LIBRARY_PATH" ;;
  Linux/*) LIB_EXT="so"; LIB_VAR="LD_LIBRARY_PATH" ;;
  *) echo "run_r4_dotnet_tests.sh: unsupported OS $OS" >&2; exit 2 ;;
esac

BUILD_ARGS=()
if [ -n "$TARGET" ]; then
  BUILD_ARGS=(--target "$TARGET")
fi
# Word splitting on BUILD_ARGS is the point: empty when dotnet matches the
# default toolchain, one --target pair when it does not.
# shellcheck disable=SC2086
cargo build -p pdf-capi --locked ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}

OUT="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
if [ -n "$TARGET" ]; then
  LIBDIR="$OUT/$TARGET/debug"
else
  LIBDIR="$OUT/debug"
fi
[ -f "$LIBDIR/libpdf_capi.$LIB_EXT" ] \
  || { echo "run_r4_dotnet_tests.sh: $LIBDIR/libpdf_capi.$LIB_EXT was not built" >&2; exit 1; }

# NuGetAudit=false: the audit reaches the network for every run, and the
# pinned test packages are already in the local cache.
# GeneratePackageOnBuild=false: packing demands all seven staged runtimes
# natives, and a test run ships nothing.
export "$LIB_VAR"="$LIBDIR"
python3 scripts/ci/tests_actually_ran.py dotnet -- \
  dotnet test bindings/dotnet/test/PDFluent.Tests/PDFluent.Tests.csproj \
    -p:NuGetAudit=false -p:GeneratePackageOnBuild=false
