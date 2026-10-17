#!/usr/bin/env bash
# smoke_cli.sh — Consumer smoke test for the `pdfluent` CLI (T6 §6).
#
# Unpacks a release archive into a clean directory (or takes an installed
# binary / a Docker image), then runs `pdfluent --version` and
# `pdfluent extract <fixture> --format md` the way a downstream
# consumer would. The markdown must contain a known sentence: exit 0 alone
# proves the binary starts, not that it extracts.
#
# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises. A key leaked into this environment would make
# the smoke prove nothing about the free tier.
#
# Every output format the CLI's own --help advertises runs here, so help and
# smoke cannot drift apart: a format gained without the smoke exercising it
# ships untested. Before m32 delivers `extract` at all, this smoke SKIPs in
# a development lane and FAILs in a release lane (PDFLUENT_RELEASE_LANE=1).
#
# Usage:
#   scripts/release/consumer_smokes/smoke_cli.sh [--artifact PATH] [--bin PATH]
#       [--docker-image REF] [--version VER] [--fixture PATH] [--sentence TEXT]
#       [--platform KEY] [--cross-check]
#
# Options:
#   --artifact PATH  Release archive (.tar.gz/.zip) to unpack (default:
#                    newest dist/cli/pdfluent-*.tar.gz)
#   --bin PATH       Installed binary (Homebrew / cargo install post-publish)
#   --docker-image REF  Image to run (post-publish ghcr smoke)
#   --version VER    Expected version string (default: any, still runs --version)
#   --fixture PATH   Fixture PDF (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain
#                    (default: Safety rules)
#   --platform KEY   Must name this host; refused otherwise unless --cross-check
#   --cross-check    Run even when --platform names another host
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$SCRIPT_DIR/smoke_common.sh"
SMOKE_TAG="[smoke_cli]"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
ARTIFACT=""; BIN_PATH=""; DOCKER_IMAGE=""
EXPECTED_VERSION=""
FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"
SENTENCE="Safety rules"
STRICT="${PDFLUENT_RELEASE_LANE:-0}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact) ARTIFACT="$2"; shift 2 ;;
    --bin) BIN_PATH="$2"; shift 2 ;;
    --docker-image) DOCKER_IMAGE="$2"; shift 2 ;;
    --version) EXPECTED_VERSION="$2"; shift 2 ;;
    --fixture) FIXTURE="$2"; shift 2 ;;
    --sentence) SENTENCE="$2"; shift 2 ;;
    --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
    --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
    -h|--help) sed -n '/^# Usage:/,/^#   --platform/p' "$0"; exit 0 ;;
    *) smoke_unknown_option "$1" ;;
  esac
done

smoke_check_platform

echo "=== PDFluent CLI consumer smoke test ==="
[[ -n "$SMOKE_PLATFORM" ]] && echo "Platform: $SMOKE_PLATFORM"

skip_or_fail() { # $1 = reason
  if [[ "$STRICT" == "1" ]]; then
    echo "[smoke_cli] FAIL (release lane: a skip is a failure): $1"
    exit 1
  fi
  echo "SKIPPED (not a pass): $1"
  exit 0
}

# The free tier is what is under test.
unset PDFLUENT_LICENSE_KEY

[[ -f "$FIXTURE" ]] || { echo "[smoke_cli] FAIL: fixture not found: $FIXTURE"; exit 1; }

MODES=0
[[ -n "$ARTIFACT" ]] && MODES=$((MODES + 1))
[[ -n "$BIN_PATH" ]] && MODES=$((MODES + 1))
[[ -n "$DOCKER_IMAGE" ]] && MODES=$((MODES + 1))
[[ "$MODES" -le 1 ]] || { echo "[smoke_cli] pass at most one of --artifact/--bin/--docker-image" >&2; exit 2; }

SMOKE_DIR="$(mktemp -d -t pdfluent_cli_smoke_XXXXXX)"
trap 'rm -rf "$SMOKE_DIR"' EXIT

# Resolve the thing under test into: RUN (argv prefix as an array) and
# FIXTURE_PATH (the path the CLI sees for the fixture).
RUN=()
FIXTURE_PATH="$FIXTURE"

if [[ -n "$DOCKER_IMAGE" ]]; then
  command -v docker >/dev/null 2>&1 || skip_or_fail "docker not on PATH"
  mkdir -p "$SMOKE_DIR/mnt"
  cp "$FIXTURE" "$SMOKE_DIR/mnt/"
  FIXTURE_PATH="/data/$(basename "$FIXTURE")"
  RUN=(docker run --rm -v "$SMOKE_DIR/mnt:/data:ro" "$DOCKER_IMAGE")
  echo "Image: $DOCKER_IMAGE"
elif [[ -n "$BIN_PATH" ]]; then
  [[ -x "$BIN_PATH" ]] || { echo "[smoke_cli] FAIL: not executable: $BIN_PATH"; exit 1; }
  RUN=("$BIN_PATH")
  echo "Binary: $BIN_PATH"
else
  # A given artefact must exist, naming the path: a typo'd --artifact that
  # skipped instead would vouch for nothing while reading green.
  if [[ -n "$ARTIFACT" && ! -f "$ARTIFACT" ]]; then
    echo "[smoke_cli] no such artifact: $ARTIFACT" >&2
    exit 2
  fi
  if [[ -z "$ARTIFACT" ]]; then
    ARTIFACT="$(ls -t "$REPO_ROOT"/dist/cli/pdfluent-*.tar.gz 2>/dev/null | head -1)"
  fi
  [[ -n "$ARTIFACT" && -f "$ARTIFACT" ]] || skip_or_fail "no CLI archive (looked under dist/cli/pdfluent-*.tar.gz; pass --artifact)"
  echo "Artifact: $ARTIFACT"
  case "$ARTIFACT" in
    *.tar.gz|*.tgz) tar -xzf "$ARTIFACT" -C "$SMOKE_DIR" ;;
    *.zip) python3 -c "import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$ARTIFACT" "$SMOKE_DIR" ;;
    *) echo "[smoke_cli] FAIL: unknown archive type: $ARTIFACT"; exit 1 ;;
  esac
  BIN_PATH="$(find "$SMOKE_DIR" \( -name pdfluent -o -name pdfluent.exe \) -type f | head -1)"
  [[ -n "$BIN_PATH" ]] || { echo "[smoke_cli] FAIL: no pdfluent binary in $ARTIFACT"; exit 1; }
  chmod +x "$BIN_PATH"
  RUN=("$BIN_PATH")
  echo "Unpacked binary: $BIN_PATH"
fi
echo "Fixture: $FIXTURE"
echo ""

# 1. --version ---------------------------------------------------------------
echo "[smoke_cli] --version ..."
reported="$("${RUN[@]}" --version 2>&1)" || { echo "[smoke_cli] FAIL: --version exited non-zero: $reported"; exit 1; }
echo "[smoke_cli] reported: $reported"
if [[ -n "$EXPECTED_VERSION" ]]; then
  echo "$reported" | grep -q "$EXPECTED_VERSION" || {
    echo "[smoke_cli] FAIL: version mismatch (said '$reported', expected '$EXPECTED_VERSION')"
    exit 1
  }
  echo "[smoke_cli] version match: OK"
fi

# 2. extract to markdown ------------------------------------------------------
HELP_TOP="$("${RUN[@]}" --help 2>&1)" || { echo "[smoke_cli] FAIL: --help exited non-zero"; exit 1; }
# `extract-text` (pre-m32) contains "extract" but is not the `extract`
# subcommand: match the word on its own, not as a prefix.
if ! echo "$HELP_TOP" | grep -qE '(^|[[:space:]])extract([[:space:],]|$)'; then
  skip_or_fail "CLI predates m32 \`extract\` (no \`extract\` in --help)"
fi
echo "[smoke_cli] extract to markdown ..."
markdown="$("${RUN[@]}" extract "$FIXTURE_PATH" --format md 2>"$SMOKE_DIR/extract.err")" || {
  echo "[smoke_cli] FAIL: extract exited non-zero:"
  sed 's/^/    /' "$SMOKE_DIR/extract.err"
  exit 1
}
# Exit 0 is not enough on its own: the known sentence must be in the output.
echo "$markdown" | grep -qF "$SENTENCE" || {
  echo "[smoke_cli] FAIL: sentence '$SENTENCE' not in markdown output:"
  echo "$markdown" | head -20 | sed 's/^/    /'
  exit 1
}
echo "[smoke_cli] extract markdown: OK (${#markdown} chars, sentence found)"
echo "[smoke_cli] first line: $(printf '%s' "$markdown" | head -1)"

# 3. every further format, as advertised ---------------------------------------
# Help layouts differ (formats may be listed top-level or under extract), so
# both help texts count as "what --help advertises". The loop is over what
# --help lists for --format — clap's `- value:` list, possible-values
# brackets, or bare words after the flag — not over a fixed pair: md already
# ran above with the sentence assertion, everything else advertised must run
# here. Only the --format section is read, so --quality's values never join
# the list; and list items are the word after `- `, never the doc words
# after the colon (m492: those read as `--format with`, `--format headings`
# and failed every format step against a real CLI).
HELP_EXTRACT="$("${RUN[@]}" extract --help 2>&1)" || HELP_EXTRACT=""
HELP_ALL="$HELP_TOP
$HELP_EXTRACT"
command -v python3 >/dev/null 2>&1 || { echo "[smoke_cli] FAIL: need python3 to read the advertised formats"; exit 1; }
FORMATS="$(printf '%s' "$HELP_ALL" | python3 -c "
import re, sys
text = sys.stdin.read()
found = []
def add(names):
    for n in names:
        n = n.strip().strip(',;:\'\"()[]<>')
        if re.fullmatch(r'[a-z][a-z0-9_-]*', n) and n not in found:
            found.append(n)
sections = re.split(r'(?m)^(?=[ \t]*(?:-\w,[ \t]*)?--[a-z])', text)
fmt = ''.join(s for s in sections
              if re.match(r'[ \t]*(?:-\w,[ \t]*)?--format\b', s))
scope = fmt if fmt else text
for m in re.finditer(r'\[possible values?:\s*([^\]\n]+)\]', scope, re.I):
    add(re.split(r'[,|/\s]+', m.group(1)))
for m in re.finditer(r'possible values:[ \t]*\[([^\]\n]+)\]', scope, re.I):
    add(re.split(r'[,|/\s]+', m.group(1)))
for m in re.finditer(r'(?m)^[ \t]*-[ \t]+([a-z][a-z0-9_-]*)\s*:', scope):
    add([m.group(1)])
for m in re.finditer(r'--format(?:=|\s+)([^\n]*)', scope):
    if m.group(0).startswith('--format='):
        tail = m.group(1).split()
        add(re.split(r'[,|]+', tail[0]) if tail else [])
    else:
        for tok in m.group(1).split():
            if not re.fullmatch(r'[a-z][a-z0-9_-]*', tok):
                break
            add([tok])
print(' '.join(f for f in found if f != 'md'))
")"
if [[ -z "$FORMATS" ]]; then
  echo "[smoke_cli] no further formats advertised; nothing more to run"
else
  echo "[smoke_cli] advertised formats: $FORMATS"
fi
for fmt in $FORMATS; do
  echo "[smoke_cli] extract --format $fmt ..."
  out="$("${RUN[@]}" extract "$FIXTURE_PATH" --format "$fmt" 2>"$SMOKE_DIR/fmt-$fmt.err")" || {
    echo "[smoke_cli] FAIL: --format $fmt exited non-zero:"
    sed 's/^/    /' "$SMOKE_DIR/fmt-$fmt.err"
    exit 1
  }
  [[ -n "$out" ]] || { echo "[smoke_cli] FAIL: --format $fmt produced empty output"; exit 1; }
  if [[ "$fmt" == "json" ]]; then
    echo "$out" | python3 -c "import json, sys; json.load(sys.stdin)" || {
      echo "[smoke_cli] FAIL: --format json is not valid JSON"
      exit 1
    }
  fi
  echo "[smoke_cli] --format $fmt: OK (${#out} chars)"
done

echo ""
echo "[smoke_cli] PASS"
