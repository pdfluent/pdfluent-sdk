#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The feature pass over a fake crate (#636). A new exhaustive enum behind a
# feature is flagged in the feature run and not in the default run; sealed,
# the feature run passes too. The feature itself is new since the baseline,
# so the run also proves per-side filtering (HEAD-only enabling). Cases
# 1-4 need no toolchain; the end-to-end cases need the pinned nightly
# (TOOLCHAIN overrides it) and skip loudly without it. Nothing here touches
# the real repository: the fixture lives in a mktemp dir under --repo-root.
set -u
# GIT_DIR IS INHERITED FROM THE HOOK THAT RUNS THE GATE, and it outranks
# `-C`: without this line a fixture's git calls land in the real repository.
# See scripts/ci/_a_fixture_inherits_no_git_env.sh for the four ways that went
# wrong on 09-09-2026 (#454).
. "$(dirname "${BASH_SOURCE[0]}")/_a_fixture_inherits_no_git_env.sh"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
# The end-to-end cases build a git repository; the checkout this runs from
# must come out of them unchanged.
IJK="$(bash scripts/ci/_the_checkout_is_unchanged.sh --record)"
trap 'rm -rf "$WORK"; bash scripts/ci/_the_checkout_is_unchanged.sh --check "$IJK" || exit 1' EXIT
fails=0; cases=0
check() { cases=$((cases+1)); if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2${3:+   [$3]}"; fails=$((fails+1)); fi; }
CHECK="$ROOT/scripts/ci/check_new_public_enums.py"
: > "$WORK/empty-allow.txt"

# 1. A malformed features file fails closed without any toolchain.
printf 'this line has no colon\n' > "$WORK/malformed-features.txt"
out=$(python3 "$CHECK" --crates demo --repo-root "$WORK" \
  --features-file "$WORK/malformed-features.txt" --allowlist "$WORK/empty-allow.txt" \
  --baseline-rev HEAD 2>&1); st=$?
check "$([ "$st" -eq 2 ] && echo ok)" "a malformed features file exits 2" "exit=$st out=$out"

# 2. A missing features file fails closed too.
out=$(python3 "$CHECK" --crates demo --repo-root "$WORK" \
  --features-file "$WORK/does-not-exist.txt" --allowlist "$WORK/empty-allow.txt" \
  --baseline-rev HEAD 2>&1); st=$?
check "$([ "$st" -eq 2 ] && echo ok)" "a missing features file exits 2" "exit=$st out=$out"

# 3. Features contradict fixture JSONs (there is nothing to build).
printf '{"format_version": 57, "root": 0, "index": {}, "paths": {}}\n' > "$WORK/empty.json"
printf 'demo: gated\n' > "$WORK/features.txt"
out=$(python3 "$CHECK" --head-json "$WORK/empty.json" --baseline-json "$WORK/empty.json" \
  --features-file "$WORK/features.txt" --allowlist "$WORK/empty-allow.txt" \
  --crate demo 2>&1); st=$?
check "$([ "$st" -eq 2 ] && echo ok)" "--features-file refuses fixture mode" "exit=$st out=$out"

# 4. The gate's bash lookup reads the same grammar (comments, inline
# comments, blank lines, missing crates).
# shellcheck source=scripts/ci/_semver_gate_select.sh
. "$ROOT/scripts/ci/_semver_gate_select.sh"
cat > "$WORK/lookup.txt" <<'EOF'
# a comment
pdf-engine: vision serde   # trailing comment

pdf-invoice:serde
EOF
export SEMVER_GATE_FEATURES_FILE="$WORK/lookup.txt"
got="$(semver_gate_features_for pdf-engine)"
check "$([ "$got" = "vision serde" ] && echo ok)" "the lookup strips comments" "got=$got"
got="$(semver_gate_features_for pdf-invoice)"
check "$([ "$got" = "serde" ] && echo ok)" "the lookup needs no spaces" "got=$got"
got="$(semver_gate_features_for pdf-ocr)"
check "$([ -z "$got" ] && echo ok)" "a crate with no line gets nothing" "got=$got"
unset SEMVER_GATE_FEATURES_FILE

# The rest builds real rustdoc JSON for a fake crate, so it needs the
# pinned nightly and git. Without the toolchain the promise below is
# untested here, and the skip says so.
TOOLCHAIN="${TOOLCHAIN:-$(tr -d '[:space:]' < "$ROOT/scripts/ci/NIGHTLY_RUSTDOC_JSON")}"
if ! cargo "+$TOOLCHAIN" --version >/dev/null 2>&1; then
  echo "  SKIP  end-to-end cases need toolchain $TOOLCHAIN (TOOLCHAIN overrides)"
  echo "cases=$cases fails=$fails"
  [ "$fails" -eq 0 ]
  exit $?
fi
if ! command -v git >/dev/null 2>&1; then
  echo "  SKIP  end-to-end cases need git"
  echo "cases=$cases fails=$fails"
  [ "$fails" -eq 0 ]
  exit $?
fi

FIX="$WORK/repo"
mkdir -p "$FIX/src"
git -C "$FIX" init -q -b main
git -C "$FIX" config user.email test@example.invalid
git -C "$FIX" config user.name "feature-pass test"
cat > "$FIX/Cargo.toml" <<'EOF'
[package]
name = "demo"
version = "0.1.0"
edition = "2021"
EOF
cat > "$FIX/src/lib.rs" <<'EOF'
//! A fake crate for the feature-pass test.
pub fn hello() -> &'static str {
    "hello"
}
EOF
git -C "$FIX" add -A
git -C "$FIX" commit -qm baseline
BASE="$(git -C "$FIX" rev-parse HEAD)"
# HEAD adds the feature AND a new exhaustive enum behind it.
cat > "$FIX/Cargo.toml" <<'EOF'
[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[features]
gated = []
EOF
cat > "$FIX/src/lib.rs" <<'EOF'
//! A fake crate for the feature-pass test.
pub fn hello() -> &'static str {
    "hello"
}

/// New since the baseline, exhaustive, and behind `gated`.
#[cfg(feature = "gated")]
#[derive(Debug)]
pub enum Fresh {
    /// First variant.
    A,
    /// Second variant.
    B,
}
EOF
git -C "$FIX" add -A
git -C "$FIX" commit -qm gated-enum

# The ambient target dir would collect fixture artefacts; the fixture
# builds in its own tree.
run_check() {
  env -u CARGO_TARGET_DIR python3 "$CHECK" --crates demo --repo-root "$FIX" \
    --toolchain "$TOOLCHAIN" --allowlist "$WORK/empty-allow.txt" \
    --baseline-rev "$BASE" "$@"
}

# 5. The default run is blind to the gated enum.
out=$(run_check 2>&1); st=$?
check "$([ "$st" -eq 0 ] && echo ok)" "the default run passes over a gated enum" "exit=$st out=$out"

# 6. The feature run flags it, by path, enabling the new feature on HEAD only.
out=$(run_check --features-file "$WORK/features.txt" 2>&1); st=$?
check "$([ "$st" -eq 1 ] && echo ok)" "the feature run flags a gated enum" "exit=$st out=$out"
case "$out" in
  *demo::Fresh*) check ok "the feature run names the enum path" ;;
  *) check FAIL "the feature run names the enum path" "out=$out" ;;
esac
case "$out" in
  *"baseline lacks features gated (enabled on HEAD only)"*) check ok "a new feature is HEAD-only" ;;
  *) check FAIL "a new feature is HEAD-only" "out=$out" ;;
esac

# 7. Sealed behind the feature, the feature run passes.
cat > "$FIX/src/lib.rs" <<'EOF'
//! A fake crate for the feature-pass test.
pub fn hello() -> &'static str {
    "hello"
}

/// New since the baseline, sealed, and behind `gated`.
#[cfg(feature = "gated")]
#[derive(Debug)]
#[non_exhaustive]
pub enum Fresh {
    /// First variant.
    A,
    /// Second variant.
    B,
}
EOF
git -C "$FIX" add -A
git -C "$FIX" commit -qm sealed-enum
out=$(run_check --features-file "$WORK/features.txt" 2>&1); st=$?
check "$([ "$st" -eq 0 ] && echo ok)" "a sealed gated enum passes the feature run" "exit=$st out=$out"

echo "cases=$cases fails=$fails"
[ "$fails" -eq 0 ]
