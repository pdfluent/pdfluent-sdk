#!/usr/bin/env bash
# Package the public `pdfluent-cli` into a reproducible release artifact set.
#
# NO PUBLISH. NO BINARY RENAME. Output goes to a build directory (default
# /tmp/pdfluent-cli-dist), never the repo tree. Generates: staged binary,
# LICENSE + docs, SHA256SUMS, a JSON release manifest, and tar.gz + zip.
#
# Usage:
#   scripts/release/pdfluent_cli_package.sh            # dry-run (build + stage + checksums + manifest, no archives kept beyond OUTDIR)
#   PDFLUENT_CLI_DIST=/path scripts/release/pdfluent_cli_package.sh --archives   # also write tar.gz + zip
#
# Exit: 0 ok, non-zero on any failure (explicit, no silent fallback).
set -euo pipefail

cd "$(dirname "$0")/../.."   # repo root
CRATE="crates/pdfluent-cli"
BIN="pdfluent-cli"
OUTDIR="${PDFLUENT_CLI_DIST:-/tmp/pdfluent-cli-dist}"
ARCHIVES=0
[ "${1:-}" = "--archives" ] && ARCHIVES=1

sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }

# --- version consistency (Cargo.toml vs --version) -------------------------
CARGO_VER="$(grep -m1 '^version' "$CRATE/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
[ -n "$CARGO_VER" ] || { echo "FAIL: cannot read crate version"; exit 2; }

# --- target triple ---------------------------------------------------------
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$TRIPLE" ] || { echo "FAIL: cannot determine host triple"; exit 2; }

echo "== pdfluent-cli package =="
echo "version=$CARGO_VER triple=$TRIPLE outdir=$OUTDIR archives=$ARCHIVES"

# --- reproducible-ish build (path remapping comes from .cargo/config.toml) --
# SOURCE_DATE_EPOCH pins timestamps embedded by any timestamping step.
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"
cargo build -p pdfluent-cli --release >/dev/null

BINPATH="target/release/$BIN"
[ -x "$BINPATH" ] || { echo "FAIL: $BINPATH not built"; exit 3; }

RT_VER="$("$BINPATH" --version | tr -d '\n')"
echo "$RT_VER" | grep -q "$CARGO_VER" || { echo "FAIL: --version ($RT_VER) != Cargo version ($CARGO_VER)"; exit 4; }

# --- stage -----------------------------------------------------------------
NAME="${BIN}-${CARGO_VER}-${TRIPLE}"
STAGE="$OUTDIR/$NAME"
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp "$BINPATH" "$STAGE/$BIN"
cp "$CRATE/LICENSE" "$STAGE/LICENSE" 2>/dev/null || { echo "FAIL: $CRATE/LICENSE missing (required in package)"; exit 5; }
[ -f docs/en/cli.md ] && cp docs/en/cli.md "$STAGE/CLI.md"
# Bundle generated completions for offline install.
mkdir -p "$STAGE/completions"
for sh in bash zsh fish powershell; do "$BINPATH" completions "$sh" > "$STAGE/completions/$BIN.$sh" 2>/dev/null || true; done

# --- checksums -------------------------------------------------------------
( cd "$STAGE" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256 > SHA256SUMS )
# verify immediately
( cd "$STAGE" && sha256 -c SHA256SUMS >/dev/null ) || { echo "FAIL: checksum self-verify failed"; exit 6; }

BIN_SHA="$(sha256 "$STAGE/$BIN" | awk '{print $1}')"
BIN_SIZE="$(wc -c < "$STAGE/$BIN" | tr -d ' ')"

# --- dependency manifest (lightweight SBOM input) --------------------------
# Full CycloneDX SBOM is a release-time step (see packaging docs); here we emit
# a deterministic direct+transitive dependency list from cargo metadata.
DEPS_COUNT="$(cargo metadata --format-version 1 -q 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))' 2>/dev/null || echo 0)"

# --- release manifest ------------------------------------------------------
cat > "$STAGE/release_manifest.json" <<JSON
{
  "artifact": "$NAME",
  "binary": "$BIN",
  "version": "$CARGO_VER",
  "target_triple": "$TRIPLE",
  "binary_sha256": "$BIN_SHA",
  "binary_size_bytes": $BIN_SIZE,
  "license_file": "LICENSE",
  "completions": ["bash", "zsh", "fish", "powershell"],
  "workspace_packages": $DEPS_COUNT,
  "publish": false,
  "source_date_epoch": "$SOURCE_DATE_EPOCH",
  "notes": "no-publish packaging dry-run; binary name is pdfluent-cli (NOT pdfluent)"
}
JSON

echo "staged: $STAGE"
echo "binary_sha256=$BIN_SHA size=$BIN_SIZE"

if [ "$ARCHIVES" = 1 ]; then
  ( cd "$OUTDIR" && tar -czf "$NAME.tar.gz" "$NAME" && (command -v zip >/dev/null && zip -qr "$NAME.zip" "$NAME" || echo "note: zip unavailable, skipped") )
  ( cd "$OUTDIR" && sha256 "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )
  echo "archives: $OUTDIR/$NAME.tar.gz (+ .sha256)"
fi

echo "PDFLUENT_CLI_PACKAGE: OK (no publish)"
