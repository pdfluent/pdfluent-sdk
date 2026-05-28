#!/usr/bin/env bash
# smoke_binary.sh — Consumer smoke test for a PDFluent binary release tarball.
#
# Validates a `pdfluent-<version>-<target>.tar.gz` artifact (produced by the
# `package:binary-release` CI job) against the same path a downstream consumer
# would use: extract → verify the bundled SHA256SUMS → run `pdfluent --version`
# from the unpacked tree → grep for the expected version.
#
# Usage:
#   docs/release/consumer_smokes/smoke_binary.sh [--artifact PATH] [--version VER]
#
# Options:
#   --artifact PATH  Path to the local .tar.gz produced by binary-release
#                    (default: auto-detect newest .tar.gz under dist/).
#   --version VER    Expected version string (default: parsed from artifact
#                    filename; e.g. `pdfluent-1.0.0-beta.8-x86_64-...tar.gz`
#                    → `1.0.0-beta.8`).
#
# Exit codes:
#   0  smoke passed
#   1  checksum / extraction / version-check failed
#   2  argv / artifact-locate error / binary not runnable on this host
#
# This smoke runs the binary ONLY if its target triple matches the host —
# cross-target tarballs (Windows-gnu on a Linux runner, musl on macOS) skip
# the `--version` check with status 0 but explicit "skipped" log, since
# trying to execute a foreign-PE32+ binary on a host that cannot exec it
# is not a smoke failure (it's expected).

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
ARTIFACT=""
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact|--tar) ARTIFACT="$2"; shift 2 ;;
    --version)        EXPECTED_VERSION="$2"; shift 2 ;;
    -h|--help)
      sed -n '/^# Usage:/,/^# This smoke runs/p' "$0"; exit 0 ;;
    *) echo "[smoke_binary] unknown option: $1" >&2; exit 2 ;;
  esac
done

echo "=== PDFluent binary release smoke test ==="

# 1. Locate artifact --------------------------------------------------------
if [[ -z "$ARTIFACT" ]]; then
  ARTIFACT="$(ls -t "$REPO_ROOT"/dist/pdfluent-*.tar.gz 2>/dev/null | head -1)"
fi
if [[ -z "$ARTIFACT" || ! -f "$ARTIFACT" ]]; then
  echo "[smoke_binary] ERROR: no .tar.gz artifact found"
  echo "                (looked under dist/pdfluent-*.tar.gz; provide --artifact if elsewhere)"
  exit 2
fi
echo "[smoke_binary] artifact: $ARTIFACT"

# Derive target triple + version from filename:
#   pdfluent-1.0.0-beta.8-x86_64-unknown-linux-musl.tar.gz
#            └─ version ─┘└─── target triple ───┘
fname="$(basename "$ARTIFACT" .tar.gz)"
fname="${fname#pdfluent-}"
target="${fname#*-x86_64-}"
target="x86_64-${target}"
version_from_fname="${fname%-x86_64-*}"
if [[ -z "$EXPECTED_VERSION" ]]; then
  EXPECTED_VERSION="$version_from_fname"
fi
echo "[smoke_binary] version (expected): $EXPECTED_VERSION"
echo "[smoke_binary] target triple:      $target"

# 2. Optional sidecar sha256 check ------------------------------------------
SIDECAR="${ARTIFACT}.sha256"
if [[ -f "$SIDECAR" ]]; then
  echo "[smoke_binary] verifying sidecar sha256: $SIDECAR"
  if command -v sha256sum >/dev/null 2>&1; then
    ( cd "$(dirname "$ARTIFACT")" && sha256sum -c "$(basename "$SIDECAR")" ) || {
      echo "[smoke_binary] FAIL: sidecar sha256 mismatch"
      exit 1
    }
  else
    # macOS uses `shasum -a 256 -c` for the same job
    ( cd "$(dirname "$ARTIFACT")" && shasum -a 256 -c "$(basename "$SIDECAR")" ) || {
      echo "[smoke_binary] FAIL: sidecar sha256 mismatch (shasum)"
      exit 1
    }
  fi
  echo "[smoke_binary] sidecar sha256: OK"
fi

# 3. Extract ----------------------------------------------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
tar -xzf "$ARTIFACT" -C "$TMP"
unpacked_dir="$(ls -d "$TMP"/pdfluent-*/ | head -1)"
if [[ -z "$unpacked_dir" || ! -d "$unpacked_dir" ]]; then
  echo "[smoke_binary] FAIL: no pdfluent-* dir inside the tarball"
  exit 1
fi
echo "[smoke_binary] unpacked: $unpacked_dir"

# 4. Verify bundled SHA256SUMS ----------------------------------------------
if [[ -f "$unpacked_dir/SHA256SUMS" ]]; then
  echo "[smoke_binary] verifying bundled SHA256SUMS..."
  if command -v sha256sum >/dev/null 2>&1; then
    ( cd "$unpacked_dir" && sha256sum -c SHA256SUMS ) || {
      echo "[smoke_binary] FAIL: bundled SHA256SUMS verification failed"
      exit 1
    }
  else
    ( cd "$unpacked_dir" && shasum -a 256 -c SHA256SUMS ) || {
      echo "[smoke_binary] FAIL: bundled SHA256SUMS verification failed (shasum)"
      exit 1
    }
  fi
  echo "[smoke_binary] bundled SHA256SUMS: OK"
else
  echo "[smoke_binary] WARN: no bundled SHA256SUMS in tarball (older binary-release format?)"
fi

# 5. LICENSE present? -------------------------------------------------------
if [[ -f "$unpacked_dir/LICENSE" ]]; then
  echo "[smoke_binary] LICENSE present: $(wc -c <"$unpacked_dir/LICENSE") bytes"
else
  echo "[smoke_binary] FAIL: LICENSE file missing from tarball (publish-protocol §5 violation)"
  exit 1
fi

# 6. Locate the binary inside the unpacked dir ------------------------------
bin=""
for cand in "$unpacked_dir/pdfluent" "$unpacked_dir/pdfluent.exe"; do
  [[ -f "$cand" ]] && { bin="$cand"; break; }
done
if [[ -z "$bin" ]]; then
  echo "[smoke_binary] FAIL: no pdfluent / pdfluent.exe binary inside tarball"
  exit 1
fi
echo "[smoke_binary] binary: $bin ($(wc -c <"$bin") bytes)"

# 7. Run --version if host-target matches -----------------------------------
host_os="$(uname -s)"; host_arch="$(uname -m)"
case "$target" in
  x86_64-unknown-linux-*)
    if [[ "$host_os" = "Linux" && "$host_arch" = "x86_64" ]]; then
      run_it=1
    else
      run_it=0
    fi
    ;;
  x86_64-pc-windows-*)
    # PE32+ on Linux requires wine; on macOS not realistically. Skip in
    # both cases; the sha + LICENSE + structure checks above are still the
    # meaningful smoke gates.
    run_it=0
    ;;
  *)
    run_it=0
    ;;
esac

if [[ "$run_it" = "1" ]]; then
  chmod +x "$bin"
  echo "[smoke_binary] running $bin --version ..."
  reported="$( "$bin" --version 2>&1 | head -1 )" || {
    echo "[smoke_binary] FAIL: --version exited non-zero"
    exit 1
  }
  echo "[smoke_binary] reported: $reported"
  if echo "$reported" | grep -q "$EXPECTED_VERSION"; then
    echo "[smoke_binary] version match: OK ($EXPECTED_VERSION)"
  else
    echo "[smoke_binary] FAIL: version mismatch (binary said '$reported', expected to contain '$EXPECTED_VERSION')"
    exit 1
  fi
else
  echo "[smoke_binary] target=$target not runnable on this host ($host_os/$host_arch); skipping --version exec check"
  echo "[smoke_binary] (structural checks above are sufficient for cross-target validation)"
fi

echo "[smoke_binary] PASS"
exit 0
