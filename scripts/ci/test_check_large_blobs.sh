#!/usr/bin/env bash
# What the blob guard is a rule about, and what it is not.
#
# #599 wired `check_large_blobs.sh` into the gate with one sentence of reason: "a
# learned model is several MB of generated code, and the day one was committed
# whole is the day the policy stops". On 24-09-2026 the same rule stopped a daily
# regression round, whose run files are 1.8 to 2.4 MB by construction and which
# `structured_parity` reads out of git. Both of those are the rule working as
# written, and only one of them is the rule working as meant.
#
# So the boundary is pinned here rather than argued about again: generated code is
# caught wherever it is, measured evidence is not caught on SIZE, and nothing is
# exempt from the extension and build-output checks.
set -uo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/_a_fixture_inherits_no_git_env.sh"
cd "$(git rev-parse --show-toplevel)"
WACHT="$PWD/scripts/ci/check_large_blobs.sh"
[ -f "$WACHT" ] || { echo "FAIL: $WACHT does not exist"; exit 1; }

WERK="$(mktemp -d "${TMPDIR:-/tmp}/blobguard.XXXXXX")"
# THE REAL CHECKOUT IS RECORDED AND CHECKED. This fixture runs `git init`,
# `git config` and `git commit`, and two incidents in one night came from a
# fixture whose cd failed and whose git calls then landed in a real worktree.
# Checked in the EXIT trap, so it speaks whichever way the run ends.
IJK="$(bash scripts/ci/_the_checkout_is_unchanged.sh --record)"
trap 'rm -rf "$WERK"; bash scripts/ci/_the_checkout_is_unchanged.sh --check "$IJK" || exit 1' EXIT
FOUTEN=0
zeg() {
  if [ "$1" = ok ]; then printf '  ok    %s\n' "$2"
  else printf '  FAIL  %s   [%s]\n' "$2" "${3:-}"; FOUTEN=$((FOUTEN + 1)); fi
}

# A repository of its own, and every git call names it with -C: a bare `git`
# after a failed `cd` is how a fixture reaches the real checkout.
R="$WERK/repo"
git init -q "$R"
mkdir -p "$R/crates/pdf-learned/src" "$R/benchmarks/runs/structured" "$R/corpus/structured"
git -C "$R" config user.email t@t
git -C "$R" config user.name t
: > "$R/README.md"
git -C "$R" add README.md
git -C "$R" commit -q -m one
git -C "$R" branch -M master
# The guard's CI mode diffs against BASE_BRANCH, so master must be a ref it can
# name; a diff against nothing inspects nothing and passes for the wrong reason.
git -C "$R" checkout -q -b werk

# 1400 KB in one call: a loop appending a kilobyte at a time took minutes and
# this test is run by every push.
groot() { dd if=/dev/zero bs=1024 count=1400 2>/dev/null | tr '\0' 'x' > "$1"; }

# 1. Generated code over the limit is still caught. This is the case #599 wired
#    the guard for, and the one that must not be lost to an exemption.
groot "$R/crates/pdf-learned/src/model.rs"
git -C "$R" add -A
git -C "$R" commit -q -m "a model committed whole"
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "generated code over the limit is refused" "rc=$rc"
zeg "$(printf '%s' "$uit" | grep -q "crates/pdf-learned/src/model.rs" && echo ok || echo nee)" \
    "and the file is named" "$(printf '%s' "$uit" | grep -c VIOLATION) violation(s)"
git -C "$R" reset -q --hard HEAD~1

# 2. A round's run file over the limit is NOT caught on size: it is measured
#    evidence and the protocol reads it out of git.
groot "$R/benchmarks/runs/structured/structured-doclaynet-20260924T003759Z.json"
groot "$R/corpus/structured/DOCLAYNET_TEST.json"
git -C "$R" add -A
git -C "$R" commit -q -m "a round, and the manifest it redrew"
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -eq 0 ] && echo ok || echo nee)" \
    "a round's evidence over the limit is allowed" \
    "rc=$rc $(printf '%s' "$uit" | grep VIOLATION | head -2)"

# 3. And the exemption is of the SIZE check alone. A native binary under the same
#    path is exactly as wrong as anywhere else.
: > "$R/benchmarks/runs/structured/libthing.dylib"
git -C "$R" add -A
git -C "$R" commit -q -m "a binary in with the evidence"
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "a native binary under the exempt path is still refused" "rc=$rc"
zeg "$(printf '%s' "$uit" | grep -q "libthing.dylib" && echo ok || echo nee)" \
    "and it is named for its extension, not its size" \
    "$(printf '%s' "$uit" | grep VIOLATION | head -1)"

# 4. A hand-written source under a build-output fragment is NOT build output.
#    The napi post-build step lives at crates/pdf-node/scripts/build/, matches
#    `build/`, and already exists on master -- without the exemption the guard
#    fails every branch that touches it. Case 3's deliberate violation is
#    dropped first: the guard diffs the whole branch, so it would fail every
#    later case for the earlier file's sake.
git -C "$R" reset -q --hard HEAD~1
mkdir -p "$R/crates/pdf-node/scripts/build"
: > "$R/crates/pdf-node/scripts/build/postbuild.cjs"
git -C "$R" add -A
git -C "$R" commit -q -m "the napi post-build step, touched"
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -eq 0 ] && echo ok || echo nee)" \
    "a hand-written source under scripts/build/ is allowed" \
    "rc=$rc $(printf '%s' "$uit" | grep VIOLATION | head -2)"

# 5. And the exemption is of the PATH check alone, for that prefix alone.
#    Real build output under crates/pdf-node/build/ is still refused.
mkdir -p "$R/crates/pdf-node/build/Release"
: > "$R/crates/pdf-node/build/Release/x.node"
git -C "$R" add -A
git -C "$R" commit -q -m "a built addon, committed by accident"
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "build output under crates/pdf-node/build/ is still refused" "rc=$rc"
zeg "$(printf '%s' "$uit" | grep -q "crates/pdf-node/build/Release/x.node" && echo ok || echo nee)" \
    "and the built file is named" \
    "$(printf '%s' "$uit" | grep VIOLATION | head -1)"

# 6-9. A capped hand-written source. The cap pins one reviewed version (bytes
#    and sha256); the fixture cannot have the real file's hash, so these cases
#    run a copy of the guard whose single entry names the fixture's own size
#    and hash. Case 5's built addon is dropped first, as in case 4.
git -C "$R" reset -q --hard HEAD~1
P="crates/pdf-manip/src/pdfa_fonts.rs"
mkdir -p "$R/crates/pdf-manip/src" "$R/crates/pdf-other/src" "$R/vendor/crates/pdf-manip/src"
maak() { dd if=/dev/zero bs=1024 count="$2" 2>/dev/null | tr '\0' "${3:-x}" > "$1"; }
maak "$R/$P" 1256
CAPB=$(wc -c < "$R/$P" | tr -d ' ')
if command -v sha256sum >/dev/null 2>&1; then CAPS=$(sha256sum "$R/$P" | cut -d' ' -f1); else CAPS=$(shasum -a 256 "$R/$P" | cut -d' ' -f1); fi
GEPIND="$WERK/guard-pinned.sh"
sed "s|^    \"$P:[0-9]*:[0-9a-f]*\"\$|    \"$P:$CAPB:$CAPS\"|" "$WACHT" > "$GEPIND"
zeg "$(grep -q "\"$P:$CAPB:$CAPS\"" "$GEPIND" && echo ok || echo nee)" \
    "the guard copy pins the fixture" "sed did not replace the entry"
pinned() { (cd "$R" && BASE_BRANCH=master bash "$GEPIND" 2>&1); }

# 6. Exactly the pinned version, at exactly its ceiling, passes.
git -C "$R" add -A
git -C "$R" commit -q -m "the capped source, as reviewed"
uit="$(pinned)"; rc=$?
zeg "$([ $rc -eq 0 ] && echo ok || echo nee)" \
    "the pinned version at exactly its ceiling is allowed" \
    "rc=$rc $(printf '%s' "$uit" | grep VIOLATION | head -2)"
# ...and the real guard, whose entry pins the real file, refuses this fixture.
uit="$(cd "$R" && BASE_BRANCH=master bash "$WACHT" 2>&1)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "the real entry does not accept a different file at that path" "rc=$rc"

# 7. One byte past the ceiling is refused.
printf 'x' >> "$R/$P"
git -C "$R" commit -q -am "the capped source, one byte grown"
uit="$(pinned)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "one byte past the ceiling is refused" "rc=$rc"
zeg "$(printf '%s' "$uit" | grep -q "pdfa_fonts.rs.*reviewed ceiling $CAPB bytes" && echo ok || echo nee)" \
    "and it is named against its own ceiling" \
    "$(printf '%s' "$uit" | grep VIOLATION | head -1)"
git -C "$R" reset -q --hard HEAD~1

# 8. A same-size rewrite (generated tables in place of the source) is refused.
maak "$R/$P" 1256 y
git -C "$R" commit -q -am "the capped source, rewritten at its size"
uit="$(pinned)"; rc=$?
zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
    "a same-size rewrite of a capped source is refused" "rc=$rc"
zeg "$(printf '%s' "$uit" | grep -q "pdfa_fonts.rs.*edited since its review" && echo ok || echo nee)" \
    "and it is named as edited since its review" \
    "$(printf '%s' "$uit" | grep VIOLATION | head -1)"
git -C "$R" reset -q --hard HEAD~2

# 9. The cap is the exact path: a new file, a copy beside it, a vendored copy
#    and the same name in another crate are each held to the plain limit,
#    even when they are byte for byte the pinned version.
for other in crates/pdf-manip/src/pdfa_fonts_tables.rs "$P.orig" \
             "vendor/$P" crates/pdf-other/src/pdfa_fonts.rs; do
    mkdir -p "$(dirname "$R/$other")"
    maak "$R/$other" 1256
    git -C "$R" add -A
    git -C "$R" commit -q -m "$other at the capped size"
    uit="$(pinned)"; rc=$?
    zeg "$([ $rc -ne 0 ] && echo ok || echo nee)" \
        "$other at the capped size is refused" "rc=$rc"
    zeg "$(printf '%s' "$uit" | grep -q "$other  \[SIZE=1256KB (limit 1024KB)" && echo ok || echo nee)" \
        "and $other is held to the plain limit" \
        "$(printf '%s' "$uit" | grep VIOLATION | head -1)"
    git -C "$R" reset -q --hard HEAD~1
done

printf 'test_check_large_blobs: %d failure(s)\n' "$FOUTEN"
[ "$FOUTEN" -eq 0 ]
