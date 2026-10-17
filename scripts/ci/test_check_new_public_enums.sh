#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The new-public-enums check, over canned rustdoc JSON pairs (#574). A new
# enum passes sealed or allowlisted and fails exhaustive with its path; an
# enum that already existed at the baseline is ignored; an unknown
# format_version is SKIPPED, never guessed at. No git, no toolchain: the
# fixtures stand in for both sides of the diff.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
fails=0; cases=0
check() { cases=$((cases+1)); if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2${3:+   [$3]}"; fails=$((fails+1)); fi; }
CHECK="$ROOT/scripts/ci/check_new_public_enums.py"

# Baseline: one exhaustive enum that shipped at v1.0.0.
cat > "$WORK/base.json" <<'EOF'
{"format_version": 57, "root": 0,
 "index": {
  "0": {"name": "demo", "visibility": "public", "attrs": [],
        "inner": {"module": {"is_crate": true, "items": [1], "is_stripped": false}}},
  "1": {"name": "Old", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}}},
 "paths": {
  "0": {"crate_id": 0, "path": ["demo"], "kind": "module"},
  "1": {"crate_id": 0, "path": ["demo", "Old"], "kind": "enum"}}}
EOF
: > "$WORK/empty-allow.txt"

# 1. A new sealed enum passes.
cat > "$WORK/sealed.json" <<'EOF'
{"format_version": 57, "root": 0,
 "index": {
  "0": {"name": "demo", "visibility": "public", "attrs": [],
        "inner": {"module": {"is_crate": true, "items": [1, 2], "is_stripped": false}}},
  "1": {"name": "Old", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}},
  "2": {"name": "Fresh", "visibility": "public", "attrs": ["non_exhaustive"],
        "inner": {"enum": {"variants": [], "impls": []}}}},
 "paths": {
  "0": {"crate_id": 0, "path": ["demo"], "kind": "module"},
  "1": {"crate_id": 0, "path": ["demo", "Old"], "kind": "enum"},
  "2": {"crate_id": 0, "path": ["demo", "Fresh"], "kind": "enum"}}}
EOF
out=$(python3 "$CHECK" --head-json "$WORK/sealed.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/empty-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 0 ] && echo ok)" "a new non_exhaustive enum passes" "exit=$st out=$out"

# 2. A new exhaustive enum fails and names its path.
cat > "$WORK/raw.json" <<'EOF'
{"format_version": 57, "root": 0,
 "index": {
  "0": {"name": "demo", "visibility": "public", "attrs": [],
        "inner": {"module": {"is_crate": true, "items": [1, 2], "is_stripped": false}}},
  "1": {"name": "Old", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}},
  "2": {"name": "Fresh", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}}},
 "paths": {
  "0": {"crate_id": 0, "path": ["demo"], "kind": "module"},
  "1": {"crate_id": 0, "path": ["demo", "Old"], "kind": "enum"},
  "2": {"crate_id": 0, "path": ["demo", "Fresh"], "kind": "enum"}}}
EOF
out=$(python3 "$CHECK" --head-json "$WORK/raw.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/empty-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 1 ] && echo ok)" "a new exhaustive enum fails" "exit=$st out=$out"
case "$out" in
  *demo::Fresh*) check ok "the failure names the enum path" ;;
  *) check FAIL "the failure names the enum path" "out=$out" ;;
esac

# 3. An exhaustive enum that already existed at the baseline is ignored.
out=$(python3 "$CHECK" --head-json "$WORK/base.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/empty-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 0 ] && echo ok)" "an old exhaustive enum is ignored" "exit=$st out=$out"

# 4. The allowlist excuses a new exhaustive enum.
printf 'demo::Fresh two-state switch\n' > "$WORK/allow.txt"
out=$(python3 "$CHECK" --head-json "$WORK/raw.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 0 ] && echo ok)" "the allowlist is honoured" "exit=$st out=$out"

# 5. Reachability, both directions: an enum smuggled in through a public
# re-export of a private module still counts; an enum in the index that
# nothing reaches does not.
cat > "$WORK/reexport.json" <<'EOF'
{"format_version": 57, "root": 0,
 "index": {
  "0": {"name": "demo", "visibility": "public", "attrs": [],
        "inner": {"module": {"is_crate": true, "items": [1, 3], "is_stripped": false}}},
  "1": {"name": "Old", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}},
  "2": {"name": "Smuggled", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}},
  "3": {"name": null, "visibility": "public", "attrs": [],
        "inner": {"use": {"source": "inner::Smuggled", "name": "Smuggled", "id": 2, "is_glob": false}}},
  "4": {"name": "Unreachable", "visibility": "public", "attrs": [],
        "inner": {"enum": {"variants": [], "impls": []}}}},
 "paths": {
  "0": {"crate_id": 0, "path": ["demo"], "kind": "module"},
  "1": {"crate_id": 0, "path": ["demo", "Old"], "kind": "enum"},
  "2": {"crate_id": 0, "path": ["demo", "inner", "Smuggled"], "kind": "enum"},
  "4": {"crate_id": 0, "path": ["demo", "inner", "Unreachable"], "kind": "enum"}}}
EOF
out=$(python3 "$CHECK" --head-json "$WORK/reexport.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/empty-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 1 ] && echo ok)" "a re-exported new enum fails" "exit=$st out=$out"
case "$out" in
  *demo::inner::Smuggled*) check ok "the re-export is reported by definition path" ;;
  *) check FAIL "the re-export is reported by definition path" "out=$out" ;;
esac
case "$out" in
  *Unreachable*) check FAIL "an unreachable enum is not reported" "out=$out" ;;
  *) check ok "an unreachable enum is not reported" ;;
esac

# 6. An unknown rustdoc format is SKIPPED, never guessed at.
sed 's/"format_version": 57/"format_version": 999/' "$WORK/raw.json" > "$WORK/future.json"
out=$(python3 "$CHECK" --head-json "$WORK/future.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/empty-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 2 ] && echo ok)" "an unknown format_version exits 2" "exit=$st out=$out"
case "$out" in
  *"SKIPPED (not a pass): unknown rustdoc format_version 999"*) check ok "the skip names the version" ;;
  *) check FAIL "the skip names the version" "out=$out" ;;
esac

# 7. An allowlist entry without a reason fails closed.
printf 'demo::Fresh\n' > "$WORK/malformed-allow.txt"
out=$(python3 "$CHECK" --head-json "$WORK/raw.json" --baseline-json "$WORK/base.json" --allowlist "$WORK/malformed-allow.txt" --crate demo 2>&1); st=$?
check "$([ "$st" -eq 2 ] && echo ok)" "a reasonless allowlist entry exits 2" "exit=$st out=$out"

echo "cases=$cases fails=$fails"
[ "$fails" -eq 0 ]
