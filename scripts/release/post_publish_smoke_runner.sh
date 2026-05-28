#!/usr/bin/env bash
# post_publish_smoke_runner.sh — orchestrator for the post-publish consumer
# smoke tests defined under `docs/release/consumer_smokes/`.
#
# Governed by:
#   - docs/release/PUBLISH_PROTOCOL.md §11 (post-publish verification)
#   - docs/release/sha_ledger/README.md   (R6 SHA ledger, called per channel)
#
# What this script does:
#   1. For each requested channel, locate the local artefact (per-channel
#      conventions: `cargo package` output, `npm pack` tarball, `cargo
#      build --release` binary, etc.) or take the operator-supplied path.
#   2. Invoke the channel's `smoke_<channel>.sh` from
#      `docs/release/consumer_smokes/` with the appropriate flags.
#   3. Capture each smoke's stdout + exit code into a per-run report
#      under `benchmarks/runs/post_publish_verify/<timestamp>/`.
#   4. Aggregate exit codes — the runner exits non-zero if ANY smoke
#      failed, so the publish train cannot silently progress past a
#      consumer-side defect.
#
# Usage:
#   scripts/release/post_publish_smoke_runner.sh \
#       [--channel <crates_io|npm|pypi|maven|nuget|wasm|binary|node|all>] \
#       [--artifact <path>]                # required for --channel != all
#       [--version <semver>]               # default: read from artefact / Cargo
#       [--out-dir <dir>]                  # default: benchmarks/runs/post_publish_verify/<ts>
#
# Defaults:
#   --channel all    (runs every smoke whose channel artifact is locatable)
#
# Exit codes:
#   0  all attempted smokes PASS
#   1  one or more smokes FAILED
#   2  argv error / unknown channel
#   3  smoke script for the channel does not exist
#
# This orchestrator does NOT publish, does NOT yank, does NOT touch the
# registry. It only runs the consumer-side smokes against locally-built
# artefacts (the same artefacts the publish step is about to upload, or
# just uploaded). For the registry-download verify, use
# `scripts/release/ledger_verify.py`.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "$REPO_ROOT"

SMOKE_DIR="docs/release/consumer_smokes"
CHANNEL="all"
ARTIFACT=""
VERSION=""
OUT_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)  CHANNEL="$2"; shift 2 ;;
    --artifact) ARTIFACT="$2"; shift 2 ;;
    --version)  VERSION="$2"; shift 2 ;;
    --out-dir)  OUT_DIR="$2"; shift 2 ;;
    -h|--help)
      sed -n '/^# Usage:/,/^# This orchestrator/p' "$0"
      exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

KNOWN_CHANNELS=(rust python wasm dotnet java node binary)
ALIAS_crates_io=rust
ALIAS_npm=wasm        # default; node smoke is a separate channel via --channel node
ALIAS_pypi=python
ALIAS_maven=java
ALIAS_nuget=dotnet

# Map registry-channel names → smoke-script names.
resolve_smoke_name() {
  case "$1" in
    rust|python|wasm|dotnet|java|node|binary) echo "$1" ;;
    crates_io) echo "rust" ;;
    pypi)      echo "python" ;;
    maven)     echo "java" ;;
    nuget)     echo "dotnet" ;;
    npm)       echo "wasm" ;;
    *) return 1 ;;
  esac
}

ts="$(date -u +'%Y%m%dT%H%M%SZ')"
[[ -n "$OUT_DIR" ]] || OUT_DIR="benchmarks/runs/post_publish_verify/${ts}"
mkdir -p "$OUT_DIR"
echo "[smokes] out dir: $OUT_DIR"

run_one() {
  local name="$1"
  local script="$SMOKE_DIR/smoke_${name}.sh"
  if [[ ! -x "$script" ]]; then
    echo "[smokes] SKIP $name (no $script or not executable)"
    return 3
  fi
  echo "[smokes] === $name === ($script)"
  local log="$OUT_DIR/smoke_${name}.log"
  local rc=0
  # Each smoke knows its own argv shape; pass --artifact and --version if
  # the operator gave them, but never make them mandatory at this layer.
  # `${args[@]+"${args[@]}"}` is the `set -u`-safe expansion of a possibly-
  # empty bash array — a bare `"${args[@]}"` triggers "unbound variable"
  # under `set -u` when the operator omits both --artifact and --version
  # (the common case for the `--channel all` aggregate run).
  local args=()
  [[ -n "$ARTIFACT" ]] && args+=(--artifact "$ARTIFACT")
  [[ -n "$VERSION" ]] && args+=(--version "$VERSION")
  if "$script" ${args[@]+"${args[@]}"} >"$log" 2>&1; then
    echo "[smokes]   $name: PASS  (log: $log)"
    rc=0
  else
    rc=$?
    echo "[smokes]   $name: FAIL (rc=$rc, log: $log)"
    tail -20 "$log" | sed 's/^/    | /'
  fi
  return $rc
}

declare -i fails=0
declare -a ran=()
declare -a failed=()

if [[ "$CHANNEL" = "all" ]]; then
  for name in "${KNOWN_CHANNELS[@]}"; do
    if run_one "$name"; then
      ran+=("$name")
    else
      rc=$?
      ran+=("$name")
      [[ $rc -ne 3 ]] && { fails+=1; failed+=("$name"); }
    fi
  done
else
  smoke_name="$(resolve_smoke_name "$CHANNEL")" || {
    echo "ERROR: unknown channel '$CHANNEL'" >&2
    echo "       known: ${KNOWN_CHANNELS[*]} crates_io npm pypi maven nuget" >&2
    exit 2
  }
  if run_one "$smoke_name"; then
    ran+=("$smoke_name")
  else
    rc=$?
    [[ $rc -ne 3 ]] && { fails+=1; failed+=("$smoke_name"); }
  fi
fi

# Aggregate report under $OUT_DIR/SUMMARY.md (machine-readable section
# + human-readable section). Linked from the verify_report path stored on
# the ledger entry (R6).
{
  echo "# Post-publish smoke run — ${ts}"
  echo
  echo "**Repo:** \`pdfluent-group/PDFluent-project\`"
  echo "**Triggered by:** ${USER:-unknown}@$(hostname)"
  echo "**Channel arg:** \`${CHANNEL}\`"
  echo "**Artifact arg:** \`${ARTIFACT:-<none>}\`"
  echo "**Version arg:** \`${VERSION:-<none>}\`"
  echo
  echo "## Result"
  echo
  # `${ran[*]:-none}` is set -u-safe (default-value expansion); the bare
  # `${failed[*]}` and `${ran[*]}` on the FAIL branch are equivalent when
  # at least one fail or one ran event was recorded (the failed-branch is
  # only reached when fails>0), but use the safe form unconditionally.
  ran_str="${ran[*]:-none}"
  failed_str="${failed[*]:-none}"
  if (( fails == 0 )); then
    echo "**PASS** (ran: ${ran_str})"
  else
    echo "**FAIL** (failed: ${failed_str}, ran: ${ran_str})"
  fi
  echo
  echo "## Per-smoke logs"
  for n in ${ran[@]+"${ran[@]}"}; do
    echo "- [${n}](smoke_${n}.log)"
  done
} > "$OUT_DIR/SUMMARY.md"

echo
echo "[smokes] summary: ran=${#ran[@]} fails=${fails} (see $OUT_DIR/SUMMARY.md)"
exit $(( fails > 0 ? 1 : 0 ))
