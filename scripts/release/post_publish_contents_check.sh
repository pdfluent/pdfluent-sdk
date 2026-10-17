#!/usr/bin/env bash
# post_publish_contents_check.sh — re-read the LIVE registries after a publish
# (m30 requirement 2) and run the artefact gate over what is actually there.
#
# The runbook §4 check reads the registry PAGES (title, licence, links). Pages
# say what SHOULD ship; this reads the PACKAGES: it downloads the published
# artefacts for a version and requires every platform's native, with the
# platform named on a miss:
#   PyPI   pypi.org JSON + files          (every wheel tag)
#   Maven  repo1.maven.org               (every /native/<platform>/ entry)
#   NuGet  api.nuget.org flat container  (every runtimes/<rid>/ entry)
#   npm    registry.npmjs.org            (main + every optional platform tgz)
#   C API  GitHub release assets         (one library per target + header)
#   CLI    GitHub release assets         (seven pdfluent archives + SHA256SUMS,
#                                        the Homebrew formula, the ghcr manifest)
#
# MANUAL job only: it runs after every publish, by an operator, with a report
# in benchmarks/runs/post_publish_verify/<ts>-contents/. It never publishes.
#
# Usage:
#   scripts/release/post_publish_contents_check.sh --channel <pypi|maven|nuget|npm|capi|cli|all>
#       --version <semver> [--repo <owner/name>] [--out-dir <dir>]
#   --repo selects the GitHub repo holding the C API and CLI release assets
#   (default: pdfluent/engine; tag v<version>).
#   The cli channel also reads the Homebrew formula from pdfluent/homebrew-tap
#   and the image manifest from ghcr.io/pdfluent/pdfluent (needs docker).
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "$REPO_ROOT"

CHANNEL=""; VERSION=""; REPO="pdfluent/engine"; OUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel) CHANNEL="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --repo) REPO="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    -h|--help) sed -n 's/^# //p' "$0" | head -32; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$CHANNEL" && -n "$VERSION" ]] || { echo "need --channel and --version" >&2; exit 2; }
case "$CHANNEL" in pypi|maven|nuget|npm|capi|cli|all) ;; *) echo "unknown channel: $CHANNEL" >&2; exit 2 ;; esac

ts="$(date -u +'%Y%m%dT%H%M%SZ')"
[[ -n "$OUT_DIR" ]] || OUT_DIR="benchmarks/runs/post_publish_verify/${ts}-contents"
mkdir -p "$OUT_DIR"
WORK="$(mktemp -d -t pdfluent_contents_XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "[contents] version=$VERSION out=$OUT_DIR"

GATE="python3 scripts/release/artifacts_cover_every_platform.py"
declare -a failed=()
declare -a ran=()

fetch() { # $1=url $2=dest — curl with one retry, fail loud
  curl -fsSL --retry 2 -o "$2" "$1"
}

check_pypi() {
  local dir="$WORK/pypi"; mkdir -p "$dir"
  fetch "https://pypi.org/pypi/pdfluent/${VERSION}/json" "$dir/meta.json"
  python3 - "$dir/meta.json" "$dir" <<'PY'
import json, sys, urllib.request
meta, dest = sys.argv[1], sys.argv[2]
doc = json.load(open(meta))
urls = [u for u in doc.get("urls", []) if u["packagetype"] == "bdist_wheel"]
if not urls:
    print("pypi: version has no wheels on the live index", file=sys.stderr)
    sys.exit(1)
for u in urls:
    out = f"{dest}/{u['filename']}"
    urllib.request.urlretrieve(u["url"], out)
    print(f"  fetched {u['filename']} ({u['size']} bytes)")
PY
  $GATE --wheels "$dir" 2>&1 | tee "$OUT_DIR/contents_pypi.log"
}

check_maven() {
  local dir="$WORK/maven"; mkdir -p "$dir"
  local base="https://repo1.maven.org/maven2/com/pdfluent/pdfluent/${VERSION}"
  fetch "$base/pdfluent-${VERSION}.jar" "$dir/pdfluent-${VERSION}.jar"
  $GATE --jar "$dir/pdfluent-${VERSION}.jar" 2>&1 | tee "$OUT_DIR/contents_maven.log"
}

check_nuget() {
  local dir="$WORK/nuget"; mkdir -p "$dir"
  # NuGet normalizes 1.0.1 to 1.0.1; pre-release casing is lower-cased.
  local norm="$(echo "$VERSION" | tr 'A-Z' 'a-z')"
  fetch "https://api.nuget.org/v3-flatcontainer/pdfluent/${norm}/pdfluent.${norm}.nupkg" \
    "$dir/PDFluent.${VERSION}.nupkg"
  $GATE --nupkg "$dir/PDFluent.${VERSION}.nupkg" 2>&1 | tee "$OUT_DIR/contents_nuget.log"
}

check_npm() {
  local dir="$WORK/npm"; mkdir -p "$dir/npm"
  fetch "https://registry.npmjs.org/@pdfluent%2fnode" "$dir/main.json"
  python3 - "$dir/main.json" "$dir" "$VERSION" <<'PY'
import json, sys, urllib.request
meta, dest, version = sys.argv[1], sys.argv[2], sys.argv[3]
doc = json.load(open(meta))
ver = doc["versions"][version]
optional = ver.get("optionalDependencies", {})
json.dump({"name": doc["name"], "version": version,
           "optionalDependencies": optional},
          open(f"{dest}/package.json", "w"))
print(f"  main tarball optionalDependencies: {len(optional)} entries")
PY
  # Every optional platform package: fetch its tarball for this version and
  # lay it out as npm/<tag>/index.<tag>.node, the shape the gate reads.
  python3 - "$dir/package.json" "$dir" "$VERSION" <<'PY'
import json, sys, tarfile, urllib.request, pathlib
manifest, dest, version = sys.argv[1], sys.argv[2], sys.argv[3]
od = json.load(open(manifest))["optionalDependencies"]
for name, ver in sorted(od.items()):
    tag = name.split("@pdfluent/node-")[1]
    url = f"https://registry.npmjs.org/{name.replace('/', '%2f')}/-/{name.split('/')[1]}-{ver.lstrip('^~=')}.tgz"
    tgz = f"{dest}/{tag}.tgz"
    urllib.request.urlretrieve(url, tgz)
    with tarfile.open(tgz, "r:gz") as t:
        node = [m for m in t.getmembers() if m.name.endswith(f"index.{tag}.node")]
        if not node:
            print(f"npm: {name}@{ver} has no index.{tag}.node", file=sys.stderr)
            sys.exit(1)
        outdir = pathlib.Path(dest) / "npm" / tag
        outdir.mkdir(parents=True, exist_ok=True)
        with t.extractfile(node[0]) as src, open(outdir / f"index.{tag}.node", "wb") as dst:
            dst.write(src.read())
    print(f"  fetched {name}@{ver}")
PY
  $GATE --node-dir "$dir" 2>&1 | tee "$OUT_DIR/contents_npm.log"
}

check_capi() {
  local dir="$WORK/capi"; mkdir -p "$dir"
  local tag="v${VERSION}"
  local assets="libpdf_capi.linux-x64.so libpdf_capi.linux-arm64-gnu.so libpdf_capi.linux-x64-musl.so libpdf_capi.linux-arm64-musl.so libpdf_capi.osx-x64.dylib libpdf_capi.osx-arm64.dylib pdf_capi.win-x64.dll pdfluent.h"
  if command -v gh >/dev/null 2>&1; then
    ( cd "$dir" && gh release download "$tag" -R "$REPO" $assets )
  else
    for a in $assets; do
      fetch "https://github.com/${REPO}/releases/download/${tag}/${a}" "$dir/$a"
    done
  fi
  $GATE --capi-dir "$dir" 2>&1 | tee "$OUT_DIR/contents_capi.log"
}

check_cli() {
  local dir="$WORK/cli"; mkdir -p "$dir"
  local tag="v${VERSION}"
  local assets="pdfluent-${VERSION}-aarch64-apple-darwin.tar.gz pdfluent-${VERSION}-x86_64-apple-darwin.tar.gz pdfluent-${VERSION}-x86_64-unknown-linux-gnu.tar.gz pdfluent-${VERSION}-aarch64-unknown-linux-gnu.tar.gz pdfluent-${VERSION}-x86_64-unknown-linux-musl.tar.gz pdfluent-${VERSION}-aarch64-unknown-linux-musl.tar.gz pdfluent-${VERSION}-x86_64-pc-windows-msvc.zip SHA256SUMS"
  if command -v gh >/dev/null 2>&1; then
    ( cd "$dir" && gh release download "$tag" -R "$REPO" $assets )
  else
    for a in $assets; do
      fetch "https://github.com/${REPO}/releases/download/${tag}/${a}" "$dir/$a"
    done
  fi
  # All three sub-checks run even when one fails, so one invocation names
  # every drift instead of the first.
  local rc=0
  $GATE --cli-dir "$dir" --cli-version "$VERSION" 2>&1 | tee "$OUT_DIR/contents_cli.log" || rc=1
  # The formula is pushed to the tap by hand; this reads what is actually
  # there and requires its sha256 to equal the published SHA256SUMS.
  if fetch "https://raw.githubusercontent.com/pdfluent/homebrew-tap/main/Formula/pdfluent.rb" \
    "$dir/pdfluent.rb"; then
    python3 scripts/release/artifacts_cover_every_platform.py \
      --formula "$dir/pdfluent.rb" --sums "$dir/SHA256SUMS" \
      --cli-version "$VERSION" 2>&1 | tee "$OUT_DIR/contents_cli_homebrew.log" || rc=1
  else
    echo "[contents] could not fetch the tap formula" >&2; rc=1
  fi
  # The image manifest, saved so the gate reads bytes, not a daemon.
  if command -v docker >/dev/null 2>&1; then
    if docker buildx imagetools inspect "ghcr.io/pdfluent/pdfluent:${VERSION}" --raw \
      > "$dir/manifest.json"; then
      python3 scripts/release/artifacts_cover_every_platform.py \
        --docker-manifest "$dir/manifest.json" 2>&1 | tee "$OUT_DIR/contents_cli_docker.log" || rc=1
    else
      echo "[contents] imagetools inspect failed" >&2; rc=1
    fi
  else
    echo "[contents] need docker on PATH for the ghcr manifest check" >&2; rc=1
  fi
  return $rc
}

run_one() { # $1 = channel
  echo "[contents] === $1 ==="
  ran+=("$1")
  if "check_$1"; then
    echo "[contents]   $1: PASS"
  else
    echo "[contents]   $1: FAIL (see $OUT_DIR/contents_$1.log)"
    failed+=("$1")
  fi
}

if [[ "$CHANNEL" == "all" ]]; then
  for c in pypi maven nuget npm capi cli; do run_one "$c"; done
else
  run_one "$CHANNEL"
fi

{
  echo "# Post-publish contents check — ${ts}"
  echo
  echo "**Version:** \`${VERSION}\`"
  echo "**Channel arg:** \`${CHANNEL}\`"
  echo "**C API repo:** \`${REPO}\`"
  echo
  if (( ${#failed[@]} == 0 )); then
    echo "**PASS** (ran: ${ran[*]})"
  else
    echo "**FAIL** (failed: ${failed[*]}, ran: ${ran[*]})"
  fi
} > "$OUT_DIR/SUMMARY.md"

echo "[contents] summary: ran=${#ran[@]} fails=${#failed[@]} (see $OUT_DIR/SUMMARY.md)"
exit $(( ${#failed[@]} > 0 ? 1 : 0 ))
