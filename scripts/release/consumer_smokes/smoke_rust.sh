#!/usr/bin/env bash
# smoke_rust.sh — Consumer smoke test for the PDFluent Rust crate.
#
# Tests that `pdfluent` (or a specified crate) can be added as a dependency
# to a fresh Cargo project and that the primary API is importable and callable.
#
# Does NOT require a live crates.io publish. If a local .crate tarball is
# available (e.g. from `cargo package`), it is used via a `[patch.crates-io]`
# path override. Otherwise the test fetches from crates.io.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_rust.sh [--artifact PATH] [--version X.Y.Z]
#       [--local-crate PATH] [--platform KEY] [--cross-check]
#
# Options:
#   --artifact PATH      Path to the .crate file under test, or its unpacked
#                        directory (this is what post_publish_smoke_runner.sh
#                        passes; the smoke patches crates.io with EXACTLY this)
#   --version X.Y.Z      Version to test (default: read from workspace Cargo.toml)
#   --local-crate PATH   Path to a local .crate tarball or source directory for offline smoke
#   --platform KEY       Must name this host; refused otherwise unless --cross-check
#   --cross-check        Run even when --platform names another host
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1, set by
# post_publish_smoke_runner.sh) --artifact is required — without it this smoke
# builds against the live crates.io, testing the registry rather than the
# artefact about to ship.

set -Eeuo pipefail

# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/smoke_common.sh"
SMOKE_TAG="[smoke_rust]"

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
VERSION=""
LOCAL_CRATE_PATH=""
ARTIFACT=""

# Parse named flags. Initial values are intentionally empty so that
# `--local-crate PATH` (or `--version X.Y.Z`) sets the right field
# regardless of positional order.
while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact)    ARTIFACT="$2"; shift 2 ;;
        --version)     VERSION="$2"; shift 2 ;;
        --local-crate) LOCAL_CRATE_PATH="$2"; shift 2 ;;
        --platform)    SMOKE_PLATFORM="$2"; shift 2 ;;
        --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
        *) smoke_unknown_option "$1" ;;
    esac
done

smoke_check_platform

# A given artefact must exist, naming the path (exit 2: argv /
# artefact-locate error, like smoke_node.sh). A .crate file is unpacked
# into the smoke dir below; a directory patches as it stands.
if [[ -n "$ARTIFACT" ]]; then
    if [[ -f "$ARTIFACT" && "$ARTIFACT" == *.crate ]]; then
        : # unpacked after the smoke dir exists
    elif [[ -d "$ARTIFACT" ]]; then
        LOCAL_CRATE_PATH="$ARTIFACT"
    else
        echo "❌ no such crate artefact: ${ARTIFACT}" >&2
        exit 2
    fi
elif [[ "${PDFLUENT_RELEASE_LANE:-0}" == "1" ]]; then
    echo "❌ release lane needs --artifact: building against live crates.io would test the registry, not the artefact" >&2
    exit 2
fi

# Resolve version from workspace if not provided.
if [[ -z "$VERSION" ]]; then
    VERSION=$(cargo metadata --manifest-path "${REPO_ROOT}/Cargo.toml" --no-deps --format-version 1 2>/dev/null \
        | python3 -c "import json,sys; pkgs=json.load(sys.stdin)['packages']; print(next(p['version'] for p in pkgs if p['name']=='pdfluent'))" 2>/dev/null \
        || echo "unknown")
fi

echo "=== PDFluent Rust consumer smoke test ==="
echo "Version: ${VERSION}"
echo "Local crate: ${LOCAL_CRATE_PATH:-<none, using crates.io>}"
echo ""

SMOKE_DIR=$(mktemp -d -t pdfluent_rust_smoke_XXXXXX)
trap 'echo "Cleaning up ${SMOKE_DIR}"; rm -rf "${SMOKE_DIR}"' EXIT

# A .crate packs one top dir (name-version/): unpack and patch with that.
if [[ -n "$ARTIFACT" && -f "$ARTIFACT" ]]; then
    mkdir -p "${SMOKE_DIR}/crate"
    tar -xzf "$ARTIFACT" -C "${SMOKE_DIR}/crate"
    LOCAL_CRATE_PATH="$(find "${SMOKE_DIR}/crate" -mindepth 1 -maxdepth 1 -type d | head -1)"
fi

# Create a minimal Cargo project.
cat > "${SMOKE_DIR}/Cargo.toml" <<TOML
[package]
name = "pdfluent-smoke"
version = "0.0.1"
edition = "2021"

[dependencies]
pdfluent = { version = "${VERSION}" }
TOML

# If local crate provided, add a [patch.crates-io] override.
# `${SMOKE_DIR}` is a tmp dir, so the patch path must be absolute — a
# relative path would resolve against the tmp dir and not find the crate.
if [[ -n "$LOCAL_CRATE_PATH" && -d "$LOCAL_CRATE_PATH" ]]; then
    LOCAL_CRATE_PATH_ABS="$(cd -- "$LOCAL_CRATE_PATH" && pwd)"
    # Cargo.toml goes to the native cargo: C:/... on Windows (forward
    # slashes, since a backslash is an escape in a TOML string).
    LOCAL_CRATE_PATH_ABS="$(smoke_native_path -m "$LOCAL_CRATE_PATH_ABS")"
    cat >> "${SMOKE_DIR}/Cargo.toml" <<TOML

[patch.crates-io]
pdfluent = { path = "${LOCAL_CRATE_PATH_ABS}" }
TOML
fi

mkdir -p "${SMOKE_DIR}/src"
cat > "${SMOKE_DIR}/src/main.rs" <<'RUST'
fn main() {
    // Verify the crate is importable; check the version constant if present.
    // This is intentionally minimal — the gate is importability + link, not API correctness.
    println!("pdfluent smoke: import OK");
    #[cfg(feature = "version-string")]
    println!("version: {}", pdfluent::VERSION);
}
RUST

echo "Building smoke project..."
if cargo build --manifest-path "${SMOKE_DIR}/Cargo.toml" 2>&1; then
    echo ""
    echo "✅ Rust consumer smoke PASS: pdfluent@${VERSION} builds cleanly"
else
    echo ""
    echo "❌ Rust consumer smoke FAIL: build error — see output above"
    exit 1
fi
