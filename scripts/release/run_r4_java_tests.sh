#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# run_r4_java_tests.sh — the r4 Java suites against THIS tree.
#
# Builds the two native libraries bindings/java loads (the pdf-java JNI
# library and the pdf-capi C ABI library) for the JVM's own architecture --
# on this shop's mixed Macs java is x86_64 while rustc defaults to arm64,
# and a library for the other processor fails to load -- points
# NativeLoader at them through its documented environment overrides, and
# runs the two test classes the r4 merge added: KnownImagesTest and
# ImageExtractionTest. `mvn test` stops before the prepare-package natives
# enforcer, so no staged resources are needed.
#
# Slow on a cold target dir (two debug cdylib builds); incremental after
# that. Used by the landing/full lanes and the scoped CI step, never the
# fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo java mvn; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_r4_java_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

OS="$(uname -s)"
JVM_ARCH="$(java -XshowSettings:properties -version 2>&1 | awk -F' = ' '/os\.arch/ {print $2}' | tail -1)"
TARGET=""
case "$OS/$JVM_ARCH" in
  Darwin/x86_64|Darwin/amd64) TARGET="x86_64-apple-darwin"; LIB_EXT="dylib" ;;
  Darwin/*) LIB_EXT="dylib" ;;
  Linux/x86_64|Linux/amd64) TARGET="x86_64-unknown-linux-gnu"; LIB_EXT="so" ;;
  Linux/*) LIB_EXT="so" ;;
  *) echo "run_r4_java_tests.sh: unsupported OS $OS" >&2; exit 2 ;;
esac

BUILD_ARGS=()
if [ -n "$TARGET" ]; then
  BUILD_ARGS=(--target "$TARGET")
fi
# Word splitting on BUILD_ARGS is the point: empty when the JVM matches the
# default toolchain, one --target pair when it does not.
# shellcheck disable=SC2086
cargo build -p pdf-java -p pdf-capi --locked ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}

OUT="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
if [ -n "$TARGET" ]; then
  LIBDIR="$OUT/$TARGET/debug"
else
  LIBDIR="$OUT/debug"
fi
JNI_LIB="$LIBDIR/libpdfluent_java.$LIB_EXT"
CAPI_LIB="$LIBDIR/libpdf_capi.$LIB_EXT"
for lib in "$JNI_LIB" "$CAPI_LIB"; do
  [ -f "$lib" ] || { echo "run_r4_java_tests.sh: $lib was not built" >&2; exit 1; }
done

PDFLUENT_NATIVE_LIB="$JNI_LIB" PDFLUENT_CAPI_LIB="$CAPI_LIB" \
  python3 scripts/ci/tests_actually_ran.py java -- \
    mvn -B -f bindings/java/pom.xml \
      -Dtest='KnownImagesTest,ImageExtractionTest' test
