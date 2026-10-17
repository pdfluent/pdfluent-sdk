#!/usr/bin/env bash
# cli_docker_build.sh — build ghcr.io/pdfluent/pdfluent from the musl archives.
#
# The image holds exactly the binaries the artefact gate checked: this stages
# a build context from the two static musl archives of §1 (never a compile
# inside the Dockerfile) and runs `docker buildx build` over
# crates/pdfluent-cli/Dockerfile.
#
# NOTHING IS PUSHED unless --push is passed, by an operator, by hand. Pushing
# to ghcr is account-visible; so is the first `docker login`.
#
# Usage:
#   scripts/release/cli_docker_build.sh --version 1.0.0 --archives dist/cli
#   scripts/release/cli_docker_build.sh --version 1.0.0 --archives dist/cli --push
#   scripts/release/cli_docker_build.sh --version 1.0.0 --archives dist/cli \
#       --load amd64   # single-arch, into the local daemon, for the smoke
#
# --load takes amd64|arm64 and builds only that arch with --load, so the smoke
# can run the image on this host before anything is pushed.
set -Eeuo pipefail

cd "$(dirname "$0")/../.."
VERSION=""; ARCHIVES=""; IMAGE="ghcr.io/pdfluent/pdfluent"; PUSH=0; LOAD=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --archives) ARCHIVES="$2"; shift 2 ;;
    --image) IMAGE="$2"; shift 2 ;;
    --push) PUSH=1; shift ;;
    --load) LOAD="$2"; shift 2 ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -24; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$VERSION" && -n "$ARCHIVES" ]] || { echo "need --version and --archives" >&2; exit 2; }
[[ -d "$ARCHIVES" ]] || { echo "no such archives dir: $ARCHIVES" >&2; exit 2; }
if [[ -n "$LOAD" ]]; then
  case "$LOAD" in amd64|arm64) ;; *) echo "--load takes amd64|arm64" >&2; exit 2 ;; esac
  [[ "$PUSH" == "0" ]] || { echo "--load and --push exclude each other" >&2; exit 2; }
fi
command -v docker >/dev/null 2>&1 || { echo "need docker on PATH" >&2; exit 2; }

# The archives the gate checked, with their sums verified before anything
# is staged: an image built from a tampered archive is worse than no image.
if [[ -f "$ARCHIVES/SHA256SUMS" ]]; then
  ( cd "$ARCHIVES" && { command -v sha256sum >/dev/null && sha256sum -c SHA256SUMS || shasum -a 256 -c SHA256SUMS; } ) \
    || { echo "SHA256SUMS verification failed in $ARCHIVES" >&2; exit 1; }
else
  echo "no SHA256SUMS in $ARCHIVES (the gate requires it)" >&2; exit 1
fi

WORK="$(mktemp -d -t pdfluent_docker_XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

stage_arch() { # $1 = docker arch (amd64|arm64), $2 = rust arch (x86_64|aarch64)
  local arch="$1" rust="$2"
  local archive="$ARCHIVES/pdfluent-${VERSION}-${rust}-unknown-linux-musl.tar.gz"
  [[ -f "$archive" ]] || { echo "missing musl archive: $archive" >&2; exit 1; }
  local tmp="$WORK/unpack-$arch"; mkdir -p "$tmp"
  tar -xzf "$archive" -C "$tmp"
  local bin
  bin="$(find "$tmp" -name pdfluent -type f | head -1)"
  [[ -n "$bin" ]] || { echo "no pdfluent binary in $archive" >&2; exit 1; }
  cp "$bin" "$WORK/pdfluent-$arch"
  # Licence files travel from the archives, so the image and the tarball
  # always say the same thing.
  for f in LICENSE LICENSE-COMMERCIAL NOTICE THIRD_PARTY_LICENSES.txt; do
    local src
    src="$(find "$tmp" -name "$f" | head -1)"
    [[ -n "$src" ]] || { echo "$f missing from $archive" >&2; exit 1; }
    cp "$src" "$WORK/$f"
  done
}

stage_arch amd64 x86_64
stage_arch arm64 aarch64
cp crates/pdfluent-cli/Dockerfile "$WORK/Dockerfile"

echo "[docker] context staged in $WORK"
ls -la "$WORK"

if [[ -n "$LOAD" ]]; then
  echo "[docker] building single-arch linux/$LOAD with --load (smoke only, no push)"
  docker buildx build --platform "linux/$LOAD" --load \
    --build-arg "VERSION=$VERSION" \
    -t "$IMAGE:$VERSION" "$WORK"
  echo "[docker] loaded $IMAGE:$VERSION (linux/$LOAD)"
  exit 0
fi

ARGS=(buildx build --platform linux/amd64,linux/arm64
  --build-arg "VERSION=$VERSION"
  -t "$IMAGE:$VERSION" -t "$IMAGE:latest" "$WORK")
if [[ "$PUSH" == "1" ]]; then
  echo "[docker] PUSHING $IMAGE:$VERSION + :latest to ghcr (manual, account-visible)"
  docker "${ARGS[@]}" --push
else
  docker "${ARGS[@]}"
  echo "[docker] built (no push). To push, by hand:"
  echo "  scripts/release/cli_docker_build.sh --version $VERSION --archives $ARCHIVES --push"
fi
