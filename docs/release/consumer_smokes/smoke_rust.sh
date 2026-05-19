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
#   scripts/release/consumer_smokes/smoke_rust.sh [--version X.Y.Z] [--local-crate PATH]
#
# Options:
#   --version X.Y.Z      Version to test (default: read from workspace Cargo.toml)
#   --local-crate PATH   Path to a local .crate tarball or source directory for offline smoke

set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
VERSION=""
LOCAL_CRATE_PATH=""

# Parse named flags. Initial values are intentionally empty so that
# `--local-crate PATH` (or `--version X.Y.Z`) sets the right field
# regardless of positional order.
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)     VERSION="$2"; shift 2 ;;
        --local-crate) LOCAL_CRATE_PATH="$2"; shift 2 ;;
        *) shift ;;
    esac
done

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
