#!/usr/bin/env bash
# crates_topo_dry_run.sh — run `cargo publish --dry-run` for every publishable
# workspace crate in topological dependency order.
#
# Governed by docs/release/PUBLISH_PROTOCOL.md.
#
# Usage:
#   scripts/release/crates_topo_dry_run.sh                 # stop on first failure
#   scripts/release/crates_topo_dry_run.sh --keep-going    # collect full picture
#   scripts/release/crates_topo_dry_run.sh --out DIR       # override report dir
#
# What it does:
#   1. Calls `cargo metadata --no-deps` to enumerate workspace packages.
#   2. Filters out non-publishable packages (publish = []).
#   3. Computes a topological order from intra-workspace dependencies.
#   4. Runs `cargo publish --dry-run --allow-dirty -p <crate>` per crate.
#   5. Records per-crate exit status + stderr-tail into a markdown summary.
#
# This script NEVER publishes. It only invokes `cargo publish --dry-run`.

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
DEFAULT_OUT="${REPO_ROOT}/benchmarks/runs/ga_hardening_plan/release/r1"

KEEP_GOING=false
OUT_DIR="${DEFAULT_OUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep-going) KEEP_GOING=true; shift ;;
        --out)        OUT_DIR="$2"; shift 2 ;;
        -h|--help)
            sed -n 's/^# \{0,1\}//p' "$0" | head -25
            exit 0 ;;
        *) echo "Unknown flag: $1" >&2; exit 2 ;;
    esac
done

mkdir -p "${OUT_DIR}"
DATESTAMP="$(date -u +%Y%m%d_%H%M%SZ)"
REPORT_MD="${OUT_DIR}/CRATES_DRY_RUN_${DATESTAMP}.md"
ORDER_JSON="${OUT_DIR}/crates_publish_order.json"
LOG_DIR="${OUT_DIR}/dry_run_logs_${DATESTAMP}"
mkdir -p "${LOG_DIR}"

cd "${REPO_ROOT}"

echo "[topo-dry-run] computing publish order via cargo metadata..."
python3 - "${ORDER_JSON}" <<'PY'
import json, subprocess, sys, collections

out_path = sys.argv[1]
meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"]))

pkgs = {p["name"]: p for p in meta["packages"]}
publishable_names = set()
for name, p in pkgs.items():
    pub = p.get("publish")
    if pub == []:
        continue
    publishable_names.add(name)

# Build dependency graph restricted to publishable workspace crates.
graph = {n: set() for n in publishable_names}
for name in publishable_names:
    p = pkgs[name]
    for dep in p.get("dependencies", []):
        dn = dep["name"]
        if dn in publishable_names and dn != name:
            graph[name].add(dn)

# Kahn topological sort: leaves (no intra-workspace deps) first.
indeg = {n: 0 for n in publishable_names}
for n, deps in graph.items():
    for d in deps:
        indeg[n] = indeg[n]  # noop; we want indegree from dependents
# Use reverse: a node should come AFTER all its deps.
# So count in-edges = number of deps of n that are publishable workspace crates.
indeg = {n: len(graph[n]) for n in publishable_names}
# Build reverse adjacency: for each dep d, list crates that depend on it
rev = collections.defaultdict(set)
for n, deps in graph.items():
    for d in deps:
        rev[d].add(n)

ready = sorted([n for n, d in indeg.items() if d == 0])
order = []
while ready:
    n = ready.pop(0)
    order.append(n)
    for child in sorted(rev[n]):
        indeg[child] -= 1
        if indeg[child] == 0:
            ready.append(child)
            ready.sort()

if len(order) != len(publishable_names):
    missing = publishable_names - set(order)
    print(f"ERROR: cycle? unresolved crates: {missing}", file=sys.stderr)
    sys.exit(2)

entries = []
for n in order:
    p = pkgs[n]
    entries.append({
        "name": n,
        "version": p["version"],
        "publish": True,
        "license": p.get("license"),
        "deps_in_workspace": sorted(graph[n]),
    })

with open(out_path, "w") as f:
    json.dump({"order": entries}, f, indent=2)
print(f"wrote {out_path}: {len(entries)} publishable crates")
PY

# Read order back into a bash array (avoid mapfile for broader compat).
CRATES=()
while IFS= read -r line; do
    CRATES+=("$line")
done < <(python3 -c "
import json,sys
d=json.load(open('${ORDER_JSON}'))
for e in d['order']: print(e['name'])
")

echo "[topo-dry-run] ${#CRATES[@]} crates queued"
{
    echo "# Crates topological dry-run — ${DATESTAMP}"
    echo
    echo "Total publishable crates: ${#CRATES[@]}"
    echo
    echo "| # | Crate | Exit | Notes |"
    echo "|---|-------|-----:|-------|"
} > "${REPORT_MD}"

PASS=0
FAIL=0
FAILED=()
i=0
for crate in "${CRATES[@]}"; do
    i=$((i+1))
    log="${LOG_DIR}/${crate}.log"
    printf "[topo-dry-run] (%2d/%2d) %s ... " "$i" "${#CRATES[@]}" "$crate"
    set +e
    cargo publish --dry-run --allow-dirty -p "$crate" >"$log" 2>&1
    rc=$?
    set -e
    if [[ $rc -eq 0 ]]; then
        echo "OK"
        PASS=$((PASS+1))
        echo "| $i | \`$crate\` | 0 | ok |" >> "${REPORT_MD}"
    else
        echo "FAIL (rc=$rc)"
        FAIL=$((FAIL+1))
        FAILED+=("$crate")
        tail_msg=$(tail -n 3 "$log" | tr '\n' ' ' | sed 's/|/ /g' | cut -c1-180)
        echo "| $i | \`$crate\` | $rc | $tail_msg |" >> "${REPORT_MD}"
        if ! $KEEP_GOING; then
            echo "[topo-dry-run] stopping on first failure (use --keep-going to collect full picture)"
            break
        fi
    fi
done

{
    echo
    echo "## Summary"
    echo
    echo "- Pass: ${PASS}"
    echo "- Fail: ${FAIL}"
    if [[ ${#FAILED[@]} -gt 0 ]]; then
        echo "- Failed crates:"
        for c in "${FAILED[@]}"; do echo "  - \`$c\`"; done
    fi
    echo
    echo "Logs: \`${LOG_DIR}\`"
    echo "Order: \`${ORDER_JSON}\`"
} >> "${REPORT_MD}"

echo "[topo-dry-run] report: ${REPORT_MD}"
echo "[topo-dry-run] pass=${PASS} fail=${FAIL}"

if [[ $FAIL -gt 0 ]] && ! $KEEP_GOING; then
    exit 1
fi
exit 0
