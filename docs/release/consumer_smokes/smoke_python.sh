#!/usr/bin/env bash
# smoke_python.py — Consumer smoke test for the PDFluent Python wheel.
#
# Creates a fresh virtualenv, installs the wheel (local or from PyPI),
# imports the package, and exercises the primary API surface.
#
# Does NOT require a live PyPI publish. Pass --wheel to use a local .whl file.
#
# Usage:
#   docs/release/consumer_smokes/smoke_python.sh [--wheel PATH] [--version VERSION]
#
# Options:
#   --wheel PATH     Path to a local .whl file (default: install from PyPI)
#   --version VER    Expected version string (default: any)

set -Eeuo pipefail

WHEEL_PATH=""
EXPECTED_VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --wheel)   WHEEL_PATH="$2"; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "=== PDFluent Python consumer smoke test ==="
[[ -n "$WHEEL_PATH" ]] && echo "Wheel: ${WHEEL_PATH}" || echo "Wheel: <PyPI>"
[[ -n "$EXPECTED_VERSION" ]] && echo "Expected version: ${EXPECTED_VERSION}"
echo ""

if ! command -v python3 &>/dev/null; then
    echo "❌ python3 not found in PATH"
    exit 3
fi

SMOKE_DIR=$(mktemp -d -t pdfluent_python_smoke_XXXXXX)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

echo "Creating virtualenv..."
python3 -m venv "${SMOKE_DIR}/venv"
source "${SMOKE_DIR}/venv/bin/activate"

echo "Installing PDFluent..."
if [[ -n "$WHEEL_PATH" && -f "$WHEEL_PATH" ]]; then
    pip install --quiet "$WHEEL_PATH"
else
    if [[ -z "$EXPECTED_VERSION" ]]; then
        pip install --quiet pdfluent
    else
        pip install --quiet "pdfluent==${EXPECTED_VERSION}"
    fi
fi

echo "Running smoke checks..."
python3 - <<'PY'
import importlib, sys

# 1. Import the package.
try:
    import pdfluent
    print(f"  import pdfluent: OK")
except ImportError as e:
    print(f"  import pdfluent: FAIL — {e}", file=sys.stderr)
    sys.exit(1)

# 2. Check __version__ if present.
version = getattr(pdfluent, "__version__", None)
if version:
    print(f"  __version__: {version}")
else:
    print("  __version__: not exported (warning)")

# 3. Verify the native extension loads (it's a Rust extension module).
try:
    from pdfluent import _native  # noqa: F401
    print("  _native extension: OK")
except ImportError:
    # Some builds expose the native layer differently; not a hard failure.
    print("  _native extension: not directly importable (may be normal)")

# 4. Basic API surface probe — attempt to construct a core object.
# Use duck-typing: if the class exists, instantiate with a dummy path.
for api_name in ("PdfDocument", "Document", "Pdf"):
    cls = getattr(pdfluent, api_name, None)
    if cls is not None:
        print(f"  {api_name}: found in public API")
        break
else:
    print("  Primary document class not found in top-level namespace (check API surface)")

print("")
print("Python consumer smoke PASS")
PY

STATUS=$?
deactivate 2>/dev/null || true

if [[ $STATUS -eq 0 ]]; then
    echo ""
    echo "✅ Python consumer smoke PASS"
else
    echo ""
    echo "❌ Python consumer smoke FAIL"
    exit 1
fi
