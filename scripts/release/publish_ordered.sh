#!/usr/bin/env bash
# publish_ordered.sh — publish every publishable workspace crate to crates.io
# in topological dependency order.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md. Read that first; this script
# enforces it, it does not replace it.
#
# Why this exists: `.gitlab-ci.yml` has called this file since the release jobs
# were written, and the file was never in the repository. The only automated
# publish path would have failed in seconds, and nobody would have found out
# until the moment they tried to ship (found 22-08-2026, during a 1.0.0
# attempt). scripts/ci/ci_config_lint.py now fails on a job that calls a script
# that is not there.
#
# Usage:
#   scripts/release/publish_ordered.sh              # dry run, publishes nothing
#   scripts/release/publish_ordered.sh --live       # actually publishes
#   scripts/release/publish_ordered.sh --from NAME  # resume at a crate
#
# Resumable on purpose. A 32-crate publish that dies at crate 19 must not force
# a choice between re-publishing eighteen versions (impossible; crates.io is
# append-only) and hand-editing a list under time pressure. Crates already on
# crates.io at the exact version are skipped with a line saying so.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

LIVE=false
START_AT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --live) LIVE=true; shift ;;
    --from) START_AT="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
    *) echo "publish_ordered: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Protocol §4 step 1 and 3: clean tree, and --allow-dirty is banned outright.
if [ -n "$(git status --porcelain)" ]; then
  echo "publish_ordered: working tree is not clean — PUBLISH_PROTOCOL.md §4.1" >&2
  git status --short >&2
  exit 1
fi

echo "[publish] computing topological order via cargo metadata..."
ORDER_FILE="$(mktemp)"
python3 - "${ORDER_FILE}" <<'PY'
import collections, json, subprocess, sys

meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"]))
pkgs = {p["name"]: p for p in meta["packages"]}
publishable = {n for n, p in pkgs.items() if p.get("publish") != []}

graph = {n: {d["name"] for d in pkgs[n].get("dependencies", [])
             if d["name"] in publishable and d["name"] != n}
         for n in publishable}
indeg = {n: len(graph[n]) for n in publishable}
rev = collections.defaultdict(set)
for n, deps in graph.items():
    for d in deps:
        rev[d].add(n)

ready = sorted(n for n, d in indeg.items() if d == 0)
order = []
while ready:
    n = ready.pop(0)
    order.append(n)
    for child in sorted(rev[n]):
        indeg[child] -= 1
        if indeg[child] == 0:
            ready.append(child)
            ready.sort()

if len(order) != len(publishable):
    print(f"ERROR: dependency cycle among {publishable - set(order)}", file=sys.stderr)
    sys.exit(2)

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    for n in order:
        fh.write(f"{n}\t{pkgs[n]['version']}\n")
PY

TOTAL="$(wc -l < "${ORDER_FILE}" | tr -d ' ')"
echo "[publish] ${TOTAL} publishable crates"
$LIVE || echo "[publish] DRY RUN — nothing will be published. Pass --live to publish."

SKIPPING=false
[ -n "${START_AT}" ] && SKIPPING=true

i=0
while IFS=$'\t' read -r NAME VERSION; do
  i=$((i + 1))
  if $SKIPPING; then
    if [ "${NAME}" = "${START_AT}" ]; then SKIPPING=false; else
      echo "[publish] ${i}/${TOTAL} ${NAME} — skipped (before --from ${START_AT})"
      continue
    fi
  fi

  # Already on crates.io at this exact version? Then this is a resumed run, or
  # a crate that did not change. Either way republishing is impossible and
  # stopping would be wrong.
  if cargo info "${NAME}@${VERSION}" >/dev/null 2>&1; then
    echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — already on crates.io, skipping"
    continue
  fi

  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — auditing"
  if ! bash "${SCRIPT_DIR}/prepublish_crate_audit.sh" "${NAME}"; then
    echo "publish_ordered: audit failed for ${NAME} — stopping (PUBLISH_PROTOCOL.md §2)" >&2
    exit 1
  fi

  if ! $LIVE; then
    echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — dry-run publish"
    cargo publish --dry-run -p "${NAME}"
    continue
  fi

  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — PUBLISHING"
  cargo publish -p "${NAME}"

  # crates.io serves the index a moment after the upload. Publishing the next
  # crate before its dependency is resolvable fails with a message about a
  # version that "does not exist", which reads like a manifest error and is not.
  for _ in $(seq 1 60); do
    cargo info "${NAME}@${VERSION}" >/dev/null 2>&1 && break
    sleep 5
  done
  echo "[publish] ${i}/${TOTAL} ${NAME} ${VERSION} — visible in the index"
done < "${ORDER_FILE}"

rm -f "${ORDER_FILE}"
echo "[publish] done ($($LIVE && echo live || echo "dry run"))"
