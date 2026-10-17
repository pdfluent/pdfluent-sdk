#!/usr/bin/env bash
# cli_build_archives.sh — stage the seven CLI release archives (T6 §1).
#
# Each archive is pdfluent-<v>-<triple>.tar.gz (.zip on Windows), holding the
# `pdfluent[.exe]` binary plus LICENSE, LICENSE-COMMERCIAL, NOTICE,
# THIRD_PARTY_LICENSES.txt, README.md and the man pages under man/man1/,
# with SHA256SUMS beside them.
#
# This script compiles nothing: it stages archives from prebuilt binaries,
# one --binary per --triple, in order. Build first, on the host that owns
# the target (the four Linux targets cross-build with cargo zigbuild, here
# or on the Hetzner lane machine, never the Mac for Linux targets; macOS
# builds on the operator's Mac; Windows on the Windows host), then stage:
#
#   scripts/release/cli_build_archives.sh --version <v> --out dist/cli \
#       --triple x86_64-unknown-linux-gnu \
#       --binary target/x86_64-unknown-linux-gnu/release/pdfluent [...]
#
# The default is the four Linux triples; pass --triple on each host for the
# targets it owns. The automated build lives in release-binaries.yml, which
# builds and stages inline; this script is the by-hand path from the runbook.
# A binary still named `pdfluent-cli` is refused: the [[bin]] rename to
# `pdfluent` has landed, and staging the old name would ship an archive the
# gate rejects.
#
# Usage:
#   scripts/release/cli_build_archives.sh [--version V] [--out DIR]
#       [--triple T ...] --binary PATH [...] [--skip-gate]
set -Eeuo pipefail

cd "$(dirname "$0")/../.."

VERSION=""
OUT="dist/cli"
TRIPLES=()
BINARIES=()
SKIP_GATE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --triple) TRIPLES+=("$2"); shift 2 ;;
    --binary) BINARIES+=("$2"); shift 2 ;;
    --skip-gate) SKIP_GATE=1; shift ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -30; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

if [[ ${#TRIPLES[@]} -eq 0 ]]; then
  # The default set is the Linux rows of the canonical platform table
  # (scripts/release/artifacts_cover_every_platform.py), not a second list:
  # a platform added there is staged here without a second edit. The while
  # loop is the macOS bash 3.2 portable form (no mapfile).
  TRIPLES=()
  while IFS= read -r line; do TRIPLES+=("$line"); done < <(
    python3 -c "
import sys
sys.path.insert(0, 'scripts/release')
from artifacts_cover_every_platform import CLI_PLATFORMS
for plat, triple, _ext in CLI_PLATFORMS:
    if plat.startswith('linux'):
        print(triple)
"
  )
  [[ ${#TRIPLES[@]} -gt 0 ]] || {
    echo "[cli-build] FAIL: cannot read the Linux triples from the platform table" >&2
    exit 2
  }
fi
if [[ -z "$VERSION" ]]; then
  VERSION="$(grep -m1 '^version' crates/pdfluent-cli/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
fi
[[ -n "$VERSION" ]] || { echo "cannot read crate version" >&2; exit 2; }
[[ ${#BINARIES[@]} -eq ${#TRIPLES[@]} ]] || {
  echo "[cli-build] FAIL: ${#BINARIES[@]} --binary for ${#TRIPLES[@]} triples:" >&2
  echo "  pass one prebuilt binary per triple, in triple order." >&2
  exit 2
}

sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }

mkdir -p "$OUT"
echo "[cli-build] version=$VERSION out=$OUT triples=${TRIPLES[*]}"

build_one() { # $1 = triple, $2 = prebuilt binary
  local triple="$1" bin="$2"
  echo "[cli-build] === $triple ==="
  case "$triple" in
    *-unknown-linux-*|*-apple-darwin|*-pc-windows-msvc) ;;
    *)
      echo "unknown triple: $triple (the seven platforms are the closed set)" >&2; exit 2 ;;
  esac

  case "$(basename "$bin")" in
    pdfluent-cli|pdfluent-cli.exe)
      echo "[cli-build] FAIL: $triple binary is still named pdfluent-cli:" >&2
      echo "  pass the \`pdfluent\` binary; staging the old name would ship" >&2
      echo "  an archive the gate rejects, so this stops here instead." >&2
      exit 1
      ;;
  esac
  [[ -f "$bin" ]] || { echo "[cli-build] FAIL: no binary at $bin" >&2; exit 1; }

  # Fail fast on a dynamic musl binary, reading the binary's own headers
  # through the gate's elf_is_static: judging a cross-built aarch64 binary
  # with the host's loader answers is how a dynamic binary ships as static.
  case "$triple" in
    *-unknown-linux-musl)
      if command -v python3 >/dev/null 2>&1; then
        if python3 -c "
import sys
sys.path.insert(0, 'scripts/release')
from artifacts_cover_every_platform import elf_is_static
sys.exit(0 if elf_is_static(open(sys.argv[1], 'rb').read()) else 1)
" "$bin"; then
          echo "[cli-build] $triple: statically linked"
        else
          echo "[cli-build] FAIL: $triple binary is dynamically linked" >&2; exit 1
        fi
      else
        echo "[cli-build] WARN: no python3 on this host; the gate checks static linking"
      fi
      ;;
  esac
  if command -v file >/dev/null 2>&1; then
    echo "[cli-build] file: $(file -b "$bin")"
  fi

  local name="pdfluent-$VERSION-$triple"
  local stage="$OUT/$name"
  rm -rf "$stage"; mkdir -p "$stage"
  case "$triple" in
    *-windows-*) cp "$bin" "$stage/pdfluent.exe" ;;
    *) cp "$bin" "$stage/pdfluent"; chmod +x "$stage/pdfluent" ;;
  esac
  cp LICENSE LICENSE-COMMERCIAL NOTICE THIRD_PARTY_LICENSES.txt "$stage/"
  cp crates/pdfluent-cli/README.md "$stage/README.md"
  # The R1 man pages, generated from the clap tree: the gate requires
  # man/man1/pdfluent.1 in every archive, Homebrew installs the rest.
  mkdir -p "$stage/man/man1"
  cp docs/cli/man/*.1 "$stage/man/man1/"
  case "$triple" in
    *-windows-*)
      ( cd "$OUT" && rm -f "$name.zip" && python3 -c "
import sys, zipfile
from pathlib import Path
root = Path('$name')
with zipfile.ZipFile('$name.zip', 'w', zipfile.ZIP_DEFLATED) as zf:
    for f in sorted(root.rglob('*')):
        if f.is_file():
            zf.write(f, f)
" )
      rm -rf "$stage"
      echo "[cli-build] wrote $OUT/$name.zip"
      ;;
    *)
      ( cd "$OUT" && rm -f "$name.tar.gz" && tar -czf "$name.tar.gz" "$name" )
      rm -rf "$stage"
      echo "[cli-build] wrote $OUT/$name.tar.gz"
      ;;
  esac
}

for i in "${!TRIPLES[@]}"; do
  build_one "${TRIPLES[$i]}" "${BINARIES[$i]}"
done

( cd "$OUT" && rm -f SHA256SUMS && for a in pdfluent-"$VERSION"-*.tar.gz pdfluent-"$VERSION"-*.zip; do
    [[ -f "$a" ]] || continue
    sha256 "$a"
  done | sort > SHA256SUMS )
echo "[cli-build] SHA256SUMS:"
cat "$OUT/SHA256SUMS"

if [[ "$SKIP_GATE" == "0" ]]; then
  # One host stages its own triples; the gate wants all seven. It runs when
  # this invocation completed the set, on top of archives staged earlier.
  have=0
  for a in "$OUT"/pdfluent-"$VERSION"-*.tar.gz "$OUT"/pdfluent-"$VERSION"-*.zip; do
    [[ -f "$a" ]] && have=$((have + 1))
  done
  if [[ "$have" -eq 7 ]]; then
    python3 scripts/release/artifacts_cover_every_platform.py \
      --cli-dir "$OUT" --cli-version "$VERSION"
  else
    echo "[cli-build] partial set ($have/7); the gate runs when all seven are staged"
  fi
fi

echo "[cli-build] ALL DONE"
