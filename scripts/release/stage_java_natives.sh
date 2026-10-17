#!/usr/bin/env bash
# stage_java_natives.sh — stage built natives into the Java binding's
# src/main/resources/native/<platform>/ layout before `mvn package`.
#
# The 1.0.0 jar held NO native library at all: bindings/java/pom.xml bundles
# only the licence files, src/main/resources did not exist, and nothing staged
# what NativeLoader looks up at /native/<platform>/<lib>. NuGet has
# stage_dotnet_natives.sh; this is Java's counterpart.
#
# Both libraries, seven platforms:
#   libpdfluent_java (JNI, backing PdfluentDocument) and libpdf_capi (C ABI,
#   backing PdfDocument via JNA) for linux-x64, linux-arm64, linux-x64-musl,
#   linux-arm64-musl, osx-x64, osx-arm64, win-x64.
#
# Usage: scripts/release/stage_java_natives.sh [MODULE_DIR]
#   env overrides (absolute paths to prebuilt natives):
#     JNI_<PLAT>, CAPI_<PLAT> with <PLAT> in LINUX_X64, LINUX_ARM64,
#     LINUX_X64_MUSL, LINUX_ARM64_MUSL, OSX_X64, OSX_ARM64, WIN_X64
#     e.g. CAPI_LINUX_ARM64=/build/aarch64/libpdf_capi.so
#   PARTIAL_OK=1 stages whatever is available (dev loop); without it a missing
#   platform is an error. `mvn package` refuses an incomplete /native/ anyway
#   (enforcer rule in bindings/java/pom.xml), so a partial staging never ships.
# Default sources are the conventional cargo target paths.
set -uo pipefail
MOD="${1:-bindings/java}"
TGT="${CARGO_TARGET_DIR:-target}"
DST="$MOD/src/main/resources/native"

PLATS="linux-x64 linux-arm64 linux-x64-musl linux-arm64-musl osx-x64 osx-arm64 win-x64"

triple_for() {
  case "$1" in
    linux-x64)        echo "x86_64-unknown-linux-gnu" ;;
    linux-arm64)      echo "aarch64-unknown-linux-gnu" ;;
    linux-x64-musl)   echo "x86_64-unknown-linux-musl" ;;
    linux-arm64-musl) echo "aarch64-unknown-linux-musl" ;;
    osx-x64)          echo "x86_64-apple-darwin" ;;
    osx-arm64)        echo "aarch64-apple-darwin" ;;
    win-x64)          echo "x86_64-pc-windows-msvc" ;;
  esac
}
libname() { # $1 = jni|capi, $2 = plat
  case "$2" in
    win-x64)  [ "$1" = jni ] && echo "pdfluent_java.dll" || echo "pdf_capi.dll" ;;
    osx-*)    [ "$1" = jni ] && echo "libpdfluent_java.dylib" || echo "libpdf_capi.dylib" ;;
    *)        [ "$1" = jni ] && echo "libpdfluent_java.so" || echo "libpdf_capi.so" ;;
  esac
}
default_src() { # $1 = jni|capi, $2 = plat
  triple="$(triple_for "$2")"
  echo "$TGT/$triple/release/$(libname "$1" "$2")"
}
envkey() { echo "$1" | tr 'a-z-' 'A-Z_'; } # linux-arm64-musl -> LINUX_ARM64_MUSL

staged=0; missing=0
for plat in $PLATS; do
  key="$(envkey "$plat")"
  for lib in jni capi; do
    case "$lib" in
      jni)  var="JNI_${key}" ;;
      capi) var="CAPI_${key}" ;;
    esac
    src="${!var:-$(default_src "$lib" "$plat")}"
    # NO host fallback: a build in $TGT/release/ is the host's arch and libc,
    # and staging it under a named platform put an x86 dylib in the osx-arm64
    # slot. A missing platform stays missing — PARTIAL_OK=1 for the dev loop,
    # an error otherwise — and the artefact gate judges arch and libc anyway.
    dst="$DST/$plat/$(libname "$lib" "$plat")"
    if [ -f "$src" ]; then
      mkdir -p "$(dirname "$dst")"
      cp -f "$src" "$dst"
      echo "staged   $plat/$(libname "$lib" "$plat")  <-  $src"
      staged=$((staged+1))
    else
      echo "missing  $plat/$(libname "$lib" "$plat")  (no native at $src; override with \$$var)"
      missing=$((missing+1))
    fi
  done
done
echo "stage_java_natives: $staged staged, $missing missing."
if [ "$missing" -gt 0 ] && [ "${PARTIAL_OK:-0}" != "1" ]; then
  echo "ERROR: $missing natives missing; build all seven platforms or set PARTIAL_OK=1" >&2
  exit 1
fi
[ "$staged" -gt 0 ] || { echo "ERROR: no natives staged" >&2; exit 1; }
