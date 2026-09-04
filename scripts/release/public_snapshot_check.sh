#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build the public snapshot the way a stranger would, and fail if it needs
# anything only we have (#222, step 3).
#
# WHY A SEPARATE CLONE AND NOT `cargo build` IN THE WORKTREE
#
# A build in our checkout succeeds for reasons a reader does not get: the corpus
# is mounted, the cargo registry is warm, $HOME holds a keychain and a git
# identity, and `.cargo/config.toml` names paths under /Users/<us>. None of that
# travels. The only honest test is a tree copied into an empty directory, given a
# fresh HOME, and built there.
#
# WHAT "OUTSIDE THE CLONE" MEANS
#
# Any absolute path into somebody's home, /opt, or a temp directory. Measured on
# master before this script existed: 139 published files carry one, led by
# tests/curated-1k.txt (1000), retest_list.txt (600), failing_pdfs.txt (300) and
# .cargo/config.toml (17, the owner's home in --remap-path-prefix). Those are the
# check's reason for existing, not a hypothetical.
set -euo pipefail

# `--audit-only` runs the path audit and stops before the build.
#
# The two halves cost very differently. Measured on this tree: assembling the
# snapshot and grepping it takes 154 seconds, which is real but small next to the
# cold `cargo build` in a fresh clone with an empty HOME that follows it -- that
# one is minutes and gigabytes. Running the whole thing on every push would make
# the pre-push gate unusable; running neither is how #222 got to 139 files.
#
# So the audit runs locally on every push and the build runs in CI. The audit is
# also the half that catches the regression this exists for: a build failure
# means the snapshot cannot be built, a path hit means it was published with our
# machines in it.
AUDIT_ONLY=0
if [ "${1-}" = "--audit-only" ]; then
    AUDIT_ONLY=1
    shift
fi

BOOM="${1-}"
if [ -z "$BOOM" ]; then
    echo "usage: public_snapshot_check.sh [--audit-only] <directory-or-tarball>" >&2
    exit 2
fi

WERK="$(mktemp -d)"
trap 'rm -rf "$WERK"' EXIT
KLOON="$WERK/clone"
mkdir -p "$KLOON"

if [ -d "$BOOM" ]; then
    # `.` and no --exclude: the snapshot is what it is. Filtering here would test
    # a tree nobody publishes.
    (cd "$BOOM" && tar cf - .) | (cd "$KLOON" && tar xf -)
elif [ -f "$BOOM" ]; then
    tar xf "$BOOM" -C "$KLOON"
else
    echo "SKIPPED (not a pass): $BOOM is neither a directory nor a file, so no" >&2
    echo "  snapshot was built and nothing was checked." >&2
    exit 1
fi

# ---------------------------------------------------------------- path audit --
#
# Before building, because a build failure would bury this: a tree that compiles
# and still names our machines is not publishable, and the build is the slow half.
PATROON='/Users/[a-z]|/home/[a-z]|/opt/xfa|/private/tmp/|/var/folders/'
TREFFERS="$WERK/paths.txt"
: > "$TREFFERS"

# Files ALLOWED to contain the pattern, because quoting it is their job. The
# publish protocol, the release gate contract and the WASM checklist define the
# leak rules -- they list `/Users/`, `/home/`, `/opt/xfa` precisely so a human
# and a grep can both find them. "Fixing" those mentions would delete the rule
# and leave the file looking clean: a guard that reads its own explanation,
# which is the exact defect this check exists to catch elsewhere.
#
# cabi_packaging.md is allowed for a different reason: it records that an
# UPSTREAM crate ships /Users/runner/... paths inside its artefacts. That is a
# fact about someone else's build, and it is why we scan at all.
#
# Every entry needs a reason. "Inconvenient to fix" is not one.
#
# .cargo/config.toml is allowed for a third reason, and it is worth stating
# because "just take it out" is wrong twice over. Its --remap-path-prefix lines
# name the owner's home precisely so that home does NOT end up inside compiled
# artefacts; deleting them would put it back in every published binary, which is
# the thing #222 exists to prevent. And the file is not only remapping: it also
# carries the wasm32 target features (+simd128,+bulk-memory) and the musl and
# mingw linkers, so excluding the file from the published tree would break a
# reader's WASM and cross builds.
#
# The remaps could move to CI environment (RUSTFLAGS) instead of a tracked file.
# That touches release reproducibility, so it is an owner decision rather than
# something to slip into a leak audit.
TOEGESTAAN='^(docs/agent_skills/publish_protocol/SKILL\.md|docs/release/PUBLISH_PROTOCOL\.md|docs/release/release_gate_contract\.md|docs/release/checklists/wasm\.md|docs/release/cabi_packaging\.md|scripts/ci/no_dead_host_in_a_connecting_script\.py|scripts/release/public_snapshot_check\.sh|\.cargo/config\.toml)$'

while IFS= read -r -d '' f; do
    case "$f" in
        *.pdf|*.png|*.jpg|*.jpeg|*.woff|*.woff2|*.ttf|*.otf|*.zip|*.gz|*.wasm|*.bin|*.pack|*.idx) continue ;;
    esac
    rel="${f#"$KLOON"/}"
    if printf '%s' "$rel" | grep -qE "$TOEGESTAAN"; then
        continue
    fi
    if LC_ALL=C grep -nEI "$PATROON" "$f" >/dev/null 2>&1; then
        n=$(LC_ALL=C grep -cEI "$PATROON" "$f" 2>/dev/null || echo 0)
        printf '%8s  %s\n' "$n" "${f#"$KLOON"/}" >> "$TREFFERS"
    fi
done < <(find "$KLOON" -type f -not -path '*/.git/*' -print0)

if [ -s "$TREFFERS" ]; then
    echo "[snapshot] the published tree points outside itself:" >&2
    echo >&2
    sort -rn "$TREFFERS" | head -20 >&2
    TOTAAL=$(wc -l < "$TREFFERS" | tr -d ' ')
    echo >&2
    echo "  $TOTAAL file(s). A reader who clones this gets paths into a machine" >&2
    echo "  they do not have. Exclude the file in docs/PUBLIC_TREE.toml if it is" >&2
    echo "  internal, or make the path relative if the file has to ship." >&2
    exit 1
fi

if [ "$AUDIT_ONLY" = "1" ]; then
    echo "[snapshot] OK: the published tree names no path outside itself"
    echo "  (--audit-only: the build half was not run)"
    exit 0
fi

# ------------------------------------------------------------------- build ----
#
# A git identity, because some build scripts ask for one; an empty HOME, because
# the point is that nothing outside the clone is needed.
#
# TWO THINGS DELIBERATELY SURVIVE THE EMPTY HOME, and the distinction is the whole
# design: a reader is assumed to have a Rust toolchain and a network, and is NOT
# assumed to have our corpus, our keychain or our git identity. So RUSTUP_HOME and
# CARGO_HOME are pinned to the real ones -- emptying HOME hides rustup's default
# toolchain and the build dies with "could not choose a version of cargo" before
# it has compiled a line, which measures our environment rather than our tree.
# (Measured: that is exactly how the first version of this script failed its own
# clean control.) Everything else -- ssh keys, netrc, keychain, gitconfig, the
# corpus mount -- stays gone.
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export HOME="$WERK/home"
mkdir -p "$HOME"
(cd "$KLOON" && git init -q -b main . && git add -A && \
    git -c user.name=snapshot -c user.email=snapshot@invalid commit -qm "public snapshot")

echo "[snapshot] building from a clean clone with an empty HOME"
(cd "$KLOON" && cargo build --locked --workspace 2>&1 | tail -20)

# The smoke test names what it ran. A hardcoded `-p pdfluent` fails with "did not
# match any packages" on any tree that does not happen to contain it, and that
# failure reads exactly like a broken snapshot -- so ask the tree what it has.
if (cd "$KLOON" && cargo metadata --no-deps --format-version 1 2>/dev/null \
        | grep -q '"name":"pdfluent"'); then
    echo "[snapshot] smoke tests (pdfluent lib)"
    (cd "$KLOON" && cargo test --locked -q -p pdfluent --lib 2>&1 | tail -10)
else
    echo "[snapshot] smoke tests (whole workspace: no pdfluent package here)"
    (cd "$KLOON" && cargo test --locked -q --workspace --lib 2>&1 | tail -10)
fi

echo "[snapshot] OK: the snapshot builds from an empty environment and names no"
echo "  path outside itself"
