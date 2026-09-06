#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Download every artefact packaging/desktop-release.toml records and check its
# size and checksum against the record (#192).
#
# WHY THIS IS NOT A GATE
#
# It needs the network and it moves 56 MB. A gate that fails when a CDN is
# having a bad afternoon is a gate people learn to rerun until it passes, and a
# gate that costs a minute of downloading on every push is a gate people learn
# to skip. `scripts/ci/every_package_manifest_names_the_same_release.py` asks
# the question that can be answered offline -- do the manifests agree with the
# record -- and this one asks the other half: does the record still describe
# what is actually being served.
#
# Run it when a release is published, and when `verified` in the record is old
# enough to be a claim rather than an observation.
#
#     bash scripts/release/package_artifacts_still_match.sh
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RECORD="${REPO}/packaging/desktop-release.toml"
[ -f "$RECORD" ] || { echo "[artefacts] FAIL: $RECORD is missing" >&2; exit 1; }

WERK="$(mktemp -d "${TMPDIR:-/tmp}/artefacts.XXXXXX")"
trap 'rm -rf "$WERK"' EXIT

# One line per artefact: platform, url, size, checksum. Parsed by Python because
# the record is TOML and a shell that guesses at TOML is a shell that will read
# a comment as a value one day.
python3 - "$RECORD" >"${WERK}/rows" <<'PY'
import sys, tomllib
record = tomllib.load(open(sys.argv[1], "rb"))
for row in record["artifact"]:
    print(row["platform"], row["url"], row["size"], row["sha256"].lower(), sep="\t")
PY

aantal=0
fouten=0
while IFS=$'\t' read -r platform url size sha; do
  aantal=$((aantal + 1))
  doel="${WERK}/${platform}"
  echo "[artefacts] ${platform}: ${url}"
  if ! curl -fsSL -o "$doel" "$url"; then
    echo "[artefacts] FAIL: ${platform}: the download failed" >&2
    fouten=$((fouten + 1))
    continue
  fi
  gemeten_size="$(wc -c <"$doel" | tr -d ' ')"
  gemeten_sha="$(shasum -a 256 "$doel" | cut -d' ' -f1)"
  [ "$gemeten_size" = "$size" ] || {
    echo "[artefacts] FAIL: ${platform}: ${gemeten_size} bytes, the record says ${size}" >&2
    fouten=$((fouten + 1))
  }
  [ "$gemeten_sha" = "$sha" ] || {
    echo "[artefacts] FAIL: ${platform}: sha256 ${gemeten_sha}, the record says ${sha}" >&2
    fouten=$((fouten + 1))
  }
done <"${WERK}/rows"

# A run over zero artefacts is a broken parse, not a clean bill of health.
[ "$aantal" -ge 2 ] || {
  echo "[artefacts] FAIL: ${aantal} artefact(s) read from the record, expected at least 2" >&2
  exit 1
}
[ "$fouten" -eq 0 ] || {
  echo "[artefacts] FAIL: ${fouten} problem(s) over ${aantal} artefact(s)." >&2
  exit 1
}
echo "[artefacts] OK: ${aantal} artefact(s) are served at the size and checksum the record names."
