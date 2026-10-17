#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The semver gate selects crates by "published at v1.0.0", never by their
# CURRENT version (#574). A crate at 1.1.0 today that shipped 1.0.0 is still
# checked; one that never shipped 1.0.0 is not. The old loop (`[ "$version" =
# "1.0.0" ] || continue`) checked nothing once the workspace moved on.
set -u
# GIT_DIR IS INHERITED FROM THE HOOK THAT RUNS THE GATE, and it outranks
# `-C`: without this line a fixture's git calls land in the real repository.
# See scripts/ci/_a_fixture_inherits_no_git_env.sh for the four ways that went
# wrong on 09-09-2026 (#454).
. "$(dirname "${BASH_SOURCE[0]}")/_a_fixture_inherits_no_git_env.sh"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
# This fixture builds a git repository; the checkout this runs from must
# come out of it unchanged.
IJK="$(bash scripts/ci/_the_checkout_is_unchanged.sh --record)"
trap 'rm -rf "$WORK"; bash scripts/ci/_the_checkout_is_unchanged.sh --check "$IJK" || exit 1' EXIT
fails=0; cases=0
check() { cases=$((cases+1)); if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2${3:+   [$3]}"; fails=$((fails+1)); fi; }
. "$ROOT/scripts/ci/_semver_gate_select.sh"

# Fixture repo. Tag v1.0.0 ships: aaa 1.0.0 (publishable), bbb 1.0.0
# (publish = false), ddd 0.9.0, and eee inheriting the workspace 1.0.0.
# HEAD then moves aaa and eee to 1.1.0 and adds ccc 1.1.0, which never
# shipped 1.0.0 at all.
#
# Every git call carries `-C`, and the reads the sourced helper makes run
# in subshells below: a bare `cd` that failed would land every one of
# these in the checkout this runs from, and the helper takes its
# repository from the current directory.
FIX="$WORK/fixture"
mkdir -p "$FIX/crates/aaa" "$FIX/crates/bbb" "$FIX/crates/ddd" "$FIX/crates/eee"
git -C "$FIX" init -q
git -C "$FIX" config user.email "semver-gate-test@pdfluent.test"
git -C "$FIX" config user.name "semver gate test"
git -C "$FIX" config commit.gpgsign false
cat > "$FIX/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/aaa", "crates/bbb", "crates/ddd", "crates/eee"]
[workspace.package]
version = "1.0.0"
EOF
printf '[package]\nname = "aaa"\nversion = "1.0.0"\n' > "$FIX/crates/aaa/Cargo.toml"
printf '[package]\nname = "bbb"\nversion = "1.0.0"\npublish = false\n' > "$FIX/crates/bbb/Cargo.toml"
printf '[package]\nname = "ddd"\nversion = "0.9.0"\n' > "$FIX/crates/ddd/Cargo.toml"
printf '[package]\nname = "eee"\nversion.workspace = true\n' > "$FIX/crates/eee/Cargo.toml"
git -C "$FIX" add -A && git -C "$FIX" commit -qm "v1.0.0 fixture" && git -C "$FIX" tag v1.0.0
printf '[package]\nname = "aaa"\nversion = "1.1.0"\n' > "$FIX/crates/aaa/Cargo.toml"
printf '[package]\nname = "eee"\nversion.workspace = true\n' > "$FIX/crates/eee/Cargo.toml"
mkdir -p "$FIX/crates/ccc"
printf '[package]\nname = "ccc"\nversion = "1.1.0"\n' > "$FIX/crates/ccc/Cargo.toml"
git -C "$FIX" add -A && git -C "$FIX" commit -qm "HEAD fixture" >/dev/null

# Fake publish order (current versions) plus a hand-written manifest map.
printf 'aaa 1.1.0\nbbb 1.0.0\nccc 1.1.0\nddd 0.9.0\neee 1.1.0\n' > "$WORK/order.txt"
printf 'aaa=crates/aaa/Cargo.toml\nbbb=crates/bbb/Cargo.toml\nccc=crates/ccc/Cargo.toml\nddd=crates/ddd/Cargo.toml\neee=crates/eee/Cargo.toml\n' > "$WORK/map.txt"
export SEMVER_GATE_MANIFEST_MAP="$WORK/map.txt"

if (cd "$FIX" && semver_gate_published_at_1_0_0 crates/aaa/Cargo.toml); then got=ok; else got=miss; fi
check "$got" "aaa at 1.1.0 is still selected (it shipped 1.0.0)"
if (cd "$FIX" && semver_gate_published_at_1_0_0 crates/bbb/Cargo.toml); then got=ok; else got=miss; fi
check "$([ "$got" = miss ] && echo ok)" "bbb is skipped (publish = false at the tag)"
if (cd "$FIX" && semver_gate_published_at_1_0_0 crates/ccc/Cargo.toml); then got=ok; else got=miss; fi
check "$([ "$got" = miss ] && echo ok)" "ccc is skipped (absent at the tag)"
if (cd "$FIX" && semver_gate_published_at_1_0_0 crates/ddd/Cargo.toml); then got=ok; else got=miss; fi
check "$([ "$got" = miss ] && echo ok)" "ddd is skipped (0.9.0 at the tag, not 1.0.0)"
if (cd "$FIX" && semver_gate_published_at_1_0_0 crates/eee/Cargo.toml); then got=ok; else got=miss; fi
check "$got" "eee is selected (version.workspace = true over a 1.0.0 workspace)"

selected="$(cd "$FIX" && semver_gate_select "$WORK/order.txt")"
check "$([ "$selected" = "$(printf 'aaa 1.1.0\neee 1.1.0')" ] && echo ok)" \
    "the fake order selects exactly aaa and eee, current versions shown" "$selected"

echo "test_semver_gate_selection: $cases case(s), $fails failure(s)"
[ "$fails" -eq 0 ]
