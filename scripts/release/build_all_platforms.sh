#!/usr/bin/env bash
# build_all_platforms.sh — one lane for the new Linux targets, all five channels.
#
# The 1.1.0 matrix adds, per channel:
#   Python  manylinux2014 aarch64, musllinux_1_2 x86_64 + aarch64
#   npm     linux-arm64-musl
#   C API   linux-arm64-gnu, linux-x64-musl, linux-arm64-musl
#   Java    natives for all seven platforms (the 1.0.0 jar held none)
#   .NET    linux-arm64, linux-musl-x64, linux-musl-arm64
#
# The new Linux targets are cross-built with cargo-zigbuild — the route the
# runbook already recommends for manylinux. This script runs the real build
# commands; it does not fake a platform it cannot build.
#
# Build hosts follow the current rules: at most one Hetzner machine, never the
# Mac for Linux targets, and the build runs on the machine the runbook names
# for the channel. The smoke tests run on GCP (§2), not on Hetzner.
#
# Usage:
#   scripts/release/build_all_platforms.sh [--channel python|node|capi|java|dotnet|all]
#                                          [--out DIR] [--check-only]
#   OUT defaults to dist/linux-new-targets. --check-only verifies the
#   prerequisites and rust targets for the selected channels, then exits 0
#   without building: the operator's preflight before a long lane.
#
# Requirements on PATH: cargo, cargo-zigbuild (cargo-zigbuild), maturin,
# rustup targets for the three triples, node+npx for the node channel,
# mvn for the java channel, dotnet for the dotnet channel.
#
# Every channel gates what it built: the artefact gate in --allow-missing
# mode judges the artefacts on disk and reports the rest explicitly as
# `not built: ...` lines. Gating three wheels against six platforms cannot
# pass, and skipping the gate cannot vouch — both shapes lived here, one
# reporting "partial (3/6)" forever without naming anything.
set -Eeuo pipefail

CHANNEL="all"
OUT="dist/linux-new-targets"
CHECK_ONLY=0
cd "$(dirname "$0")/../.."

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel) CHANNEL="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -30; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

case "$CHANNEL" in
  python|node|capi|java|dotnet|all) ;;
  *) echo "unknown channel: $CHANNEL" >&2; exit 2 ;;
esac

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing on PATH: $1" >&2; exit 1; }; }
need cargo
need rustc

# A missing target fails here, naming the fix -- not pages deep in a
# compile as `E0463 can't find crate for 'core'`.
need_target() {
  need rustup
  rustup target list --installed 2>/dev/null | grep -qx "$1" \
    || { echo "missing rust target $1: rustup target add $1" >&2; exit 1; }
}

# Known trap (build-wheels.yml): rquickjs on aarch64 needs gnu99 -- the
# aarch64 cross-compiler in the manylinux image defaults to gnu89 and rejects
# quickjs.c's C99 `for` declarations. The musl compilers get the same flag:
# zig cc defaults vary by target the same way, and the flag is harmless where
# the default is already newer.
#
# Second trap, musl only: rustc cannot produce a cdylib under musl's default
# static CRT, so each musl cdylib build below sources
# scripts/release/musl_cdylib_rustflags.sh (the `-C target-feature=-crt-static`
# plus the path scrubbing repeated, because env flags replace the config's
# block outright). Sourced in a subshell per musl target, never for a CLI
# binary: the musl CLI must stay statically linked (P0).
export CFLAGS_aarch64_unknown_linux_gnu="${CFLAGS_aarch64_unknown_linux_gnu:-} -std=gnu99"
export CFLAGS_aarch64_unknown_linux_musl="${CFLAGS_aarch64_unknown_linux_musl:-} -std=gnu99"
export CFLAGS_x86_64_unknown_linux_musl="${CFLAGS_x86_64_unknown_linux_musl:-} -std=gnu99"

GNU_AARCH64="aarch64-unknown-linux-gnu"
MUSL_X86_64="x86_64-unknown-linux-musl"
MUSL_AARCH64="aarch64-unknown-linux-musl"

mkdir -p "$OUT"
echo "[build-lane] channel=$CHANNEL out=$OUT"

# Prerequisites up front: every selected channel's tools and rust targets
# are verified before anything builds, so a missing piece fails here naming
# the fix instead of pages deep in a compile. --check-only stops after this
# phase: the operator's preflight before a long lane, and the proof an
# installed target passes the check.
if [[ "$CHANNEL" == "python" || "$CHANNEL" == "all" ]]; then
  need maturin
  need cargo-zigbuild
  need_target "$GNU_AARCH64"
  need_target "$MUSL_X86_64"
  need_target "$MUSL_AARCH64"
fi
if [[ "$CHANNEL" == "node" || "$CHANNEL" == "all" ]]; then
  need node; need npx
  need_target "$MUSL_AARCH64"
fi
if [[ "$CHANNEL" == "capi" || "$CHANNEL" == "all" ]]; then
  need cargo-zigbuild
  need_target "$GNU_AARCH64"
  need_target "$MUSL_X86_64"
  need_target "$MUSL_AARCH64"
fi
if [[ "$CHANNEL" == "java" || "$CHANNEL" == "all" ]]; then
  need mvn
fi
if [[ "$CHANNEL" == "dotnet" || "$CHANNEL" == "all" ]]; then
  need dotnet
fi
if [[ "$CHECK_ONLY" == "1" ]]; then
  echo "[build-lane] prerequisites ok ($CHANNEL)"
  exit 0
fi

# ---------------------------------------------------------------- python ---
if [[ "$CHANNEL" == "python" || "$CHANNEL" == "all" ]]; then
  echo "[build-lane] python: three zigbuild wheels"
  for spec in "$GNU_AARCH64:manylinux2014" "$MUSL_X86_64:musllinux_1_2" "$MUSL_AARCH64:musllinux_1_2"; do
    triple="${spec%%:*}"; compat="${spec##*:}"
    # musl cdylibs need the dynamic CRT (see above); the gnu wheel must not
    # take it — the flag is per-target, in a subshell, or it leaks sideways.
    if [[ "$triple" == *-musl ]]; then
      ( source scripts/release/musl_cdylib_rustflags.sh && cd crates/pdf-python && \
        maturin build --release --strip --zig --target "$triple" \
          --compatibility "$compat" --out "$OLDPWD/$OUT/python" )
    else
      ( cd crates/pdf-python && \
        maturin build --release --strip --zig --target "$triple" \
          --compatibility "$compat" --out "$OLDPWD/$OUT/python" )
    fi
  done
  for whl in "$OUT"/python/*.whl; do
    python3 scripts/release/fix_python_wheel_paths.py "$whl"
  done
  for whl in "$OUT"/python/*.whl; do
    rm -rf "$OUT/python-audit" && mkdir -p "$OUT/python-audit"
    tmp="$(mktemp -d)"; python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$whl" "$tmp"
    python3 scripts/release/audit_package_tree.py --tree "$tmp" --out "$OUT/python-audit" \
      --package-name pdfluent --package-version "$(grep -m1 '^version' crates/pdf-python/pyproject.toml | sed 's/.*\"\(.*\)\"/\1/')" \
      --channel pypi
    rm -rf "$tmp"
  done
  # The sdist travels with the wheels: the gate requires exactly one beside
  # them, and the builder refuses the maturin that panics on this layout.
  bash scripts/release/build_pdfluent_sdist.sh --out "$OUT/python"
  python3 scripts/release/artifacts_cover_every_platform.py --wheels "$OUT/python" --allow-missing
  echo "[build-lane] python: done"
fi

# --------------------------------------------------------------- node -----
if [[ "$CHANNEL" == "node" || "$CHANNEL" == "all" ]]; then
  echo "[build-lane] node: aarch64-unknown-linux-musl"
  # The 2.x CLI takes the destination as a positional [destDir];
  # --output-dir is an "Unsupported option name" there.
  ( source scripts/release/musl_cdylib_rustflags.sh && cd crates/pdf-node && \
    npx napi build --target "$MUSL_AARCH64" --release . )
  ( cd crates/pdf-node && node scripts/release/generate-npm-dirs.cjs )
  ( cd crates/pdf-node && node scripts/release/guard-prepublish.cjs )
  python3 scripts/release/artifacts_cover_every_platform.py --node-dir crates/pdf-node --allow-missing
  echo "[build-lane] node: done"
fi

# --------------------------------------------------------------- capi -----
if [[ "$CHANNEL" == "capi" || "$CHANNEL" == "all" ]]; then
  echo "[build-lane] capi: three zigbuild libs"
  mkdir -p "$OUT/capi"
  cp crates/pdf-capi/include/pdfluent.h "$OUT/capi/"
  for triple in "$GNU_AARCH64" "$MUSL_X86_64" "$MUSL_AARCH64"; do
    # Same split as the python channel: the CRT flag rides only the musl
    # cdylibs, in a subshell, so the gnu lib never takes it.
    if [[ "$triple" == *-musl ]]; then
      ( source scripts/release/musl_cdylib_rustflags.sh && \
        cargo zigbuild -p pdf-capi --release --target "$triple" )
    else
      cargo zigbuild -p pdf-capi --release --target "$triple"
    fi
  done
  cp "target/$GNU_AARCH64/release/libpdf_capi.so" "$OUT/capi/libpdf_capi.linux-arm64-gnu.so"
  cp "target/$MUSL_X86_64/release/libpdf_capi.so" "$OUT/capi/libpdf_capi.linux-x64-musl.so"
  cp "target/$MUSL_AARCH64/release/libpdf_capi.so" "$OUT/capi/libpdf_capi.linux-arm64-musl.so"
  python3 scripts/release/artifacts_cover_every_platform.py --capi-dir "$OUT/capi" --allow-missing
  echo "[build-lane] capi: done"
fi

# --------------------------------------------------------------- java -----
if [[ "$CHANNEL" == "java" || "$CHANNEL" == "all" ]]; then
  echo "[build-lane] java: stage natives for all seven platforms, then package"
  # The seven platform libs come from their own build machines (runbook §3);
  # this lane stages whatever the environment points at and lets the staging
  # script + the pom enforcer name what is still missing.
  bash scripts/release/stage_java_natives.sh
  ( cd bindings/java && mvn -B package )
  jar="$(ls -t bindings/java/target/*.jar | grep -v -e '-sources' -e '-javadoc' | head -n 1)"
  python3 scripts/release/artifacts_cover_every_platform.py --jar "$jar"
  bash scripts/release/maven_channel_guard.sh --require-jar
  echo "[build-lane] java: done"
fi

# ------------------------------------------------------------- dotnet -----
if [[ "$CHANNEL" == "dotnet" || "$CHANNEL" == "all" ]]; then
  echo "[build-lane] dotnet: stage seven RIDs, then pack"
  bash scripts/release/stage_dotnet_natives.sh
  dotnet build bindings/dotnet/src/PDFluent/PDFluent.csproj -c Release
  rm -rf "$OUT/nuget" && mkdir -p "$OUT/nuget"
  dotnet pack bindings/dotnet/src/PDFluent/PDFluent.csproj -c Release --no-build -o "$OUT/nuget"
  nupkg="$(ls -t "$OUT"/nuget/*.nupkg | head -n 1)"
  python3 scripts/release/artifacts_cover_every_platform.py --nupkg "$nupkg"
  echo "[build-lane] dotnet: done"
fi

echo "[build-lane] ALL DONE ($CHANNEL)"
