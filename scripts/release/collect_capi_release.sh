#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# collect_capi_release.sh — assemble the C API release set from the natives.
#
# Usage:
#   scripts/release/collect_capi_release.sh --natives DIR --capi-dir DIR --tarballs DIR
#
# DIR/<platform>/capi/<lib> for the seven platforms (the natives artefacts
# laid side by side) becomes:
#   - capi-dir/: the seven libs under the gate's CAPI_ASSETS names plus
#     pdfluent.h — what the gate judges with --capi-dir;
#   - tarballs/: pdfluent-capi-<version>-<platform>.tar.gz per platform —
#     what the publish uploads.
#
# The platform -> asset map comes from the gate's PLATFORMS table: this
# script holds no second list. A missing platform is an error naming it —
# the release never ships partial. PARTIAL_OK=1 collects what exists for
# the release workflow's partial-validation path (release-natives.yml sets
# it only behind a non-empty .missing-legs); the gate still judges every
# gap, and the run still fails at the end.
set -Eeuo pipefail

cd "$(dirname "$0")/../.."

NATIVES=""; CAPI_DIR=""; TARBALLS=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --natives) NATIVES="$2"; shift 2 ;;
    --capi-dir) CAPI_DIR="$2"; shift 2 ;;
    --tarballs) TARBALLS="$2"; shift 2 ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -25; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$NATIVES" && -n "$CAPI_DIR" && -n "$TARBALLS" ]] \
  || { echo "need --natives DIR --capi-dir DIR --tarballs DIR" >&2; exit 2; }
[[ -d "$NATIVES" ]] || { echo "no natives dir: $NATIVES" >&2; exit 1; }

mkdir -p "$CAPI_DIR" "$TARBALLS"

# The key -> asset map is read off the gate: no second list lives here.
export PYTHONPATH="scripts/release${PYTHONPATH:+:$PYTHONPATH}"
MAP="$(python3 -c 'import artifacts_cover_every_platform as g; print("\n".join(f"{r[g._K]} {r[g._C]}" for r in g.PLATFORMS))')" \
  || { echo "cannot read the platform table from the gate" >&2; exit 1; }
[[ -n "$MAP" ]] || { echo "the gate names no platforms" >&2; exit 1; }

staged=0; skipped=0
while IFS=' ' read -r key asset; do
  [[ -n "$key" ]] || continue
  libs=( "$NATIVES/$key"/capi/* )
  if [[ ${#libs[@]} -ne 1 || ! -f "${libs[0]}" ]]; then
    if [[ "${PARTIAL_OK:-0}" == "1" ]]; then
      echo "capi: $key missing (PARTIAL_OK=1, skipped)" >&2
      skipped=$((skipped+1))
      continue
    fi
    echo "no capi lib for platform $key in $NATIVES/$key/capi/" >&2
    exit 1
  fi
  cp "${libs[0]}" "$CAPI_DIR/$asset"
  echo "capi: $key -> $asset"
  bash scripts/release/package_cabi.sh --lib "${libs[0]}" --plat "$key" --out "$TARBALLS" >/dev/null
  echo "capi: tarball for $key cut"
  staged=$((staged+1))
done <<< "$MAP"

[ "$staged" -gt 0 ] || { echo "capi: no platform collected" >&2; exit 1; }
cp crates/pdf-capi/include/pdfluent.h "$CAPI_DIR/"
echo "capi: header staged; $staged libs, $staged tarballs ($skipped skipped)."
