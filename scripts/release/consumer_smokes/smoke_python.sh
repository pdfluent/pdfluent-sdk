#!/usr/bin/env bash
# smoke_python.sh — Consumer smoke test for the PDFluent Python wheel.
#
# Creates a fresh virtualenv, installs the wheel (local or from PyPI), and
# extracts a fixture PDF to markdown: an import alone never touches the native
# extension the way a real consumer does, and the 1.1.0 matrix exists so that
# `pip install pdfluent` on Linux arm64 and Alpine/musl finds a wheel instead
# of falling back to the sdist (which asks for a Rust toolchain).
#
# Does NOT require a live PyPI publish. Pass --wheel to use a local .whl file.
#
# Usage:
#   scripts/release/consumer_smokes/smoke_python.sh [--artifact PATH] [--wheel PATH]
#       [--version VERSION] [--platform KEY] [--fixture PATH] [--sentence TEXT]
#       [--extract-pkg PATH]
#
# Options:
#   --artifact PATH  Path to the .whl under test (same as --wheel; this is what
#                    post_publish_smoke_runner.sh passes, and the smoke tests
#                    EXACTLY this file — never whatever is on PyPI)
#   --wheel PATH     Same as --artifact
#   --extract-pkg PATH  pdfluent-extract sdist or wheel: installed into the
#                    same venv (the uvx-style leg), then its `pdfluent-extract`
#                    console script extracts the fixture. Skipped when absent.
#   --version VER    Expected version string (default: read off the wheel name
#                    when --artifact is given, else any)
#   --platform KEY   Must name this host (e.g. linux-arm64-musl on that host);
#                    the smoke runs where it is, against the given wheel, so a
#                    foreign platform tests pip's refusal rather than the wheel.
#                    Aliases macos-/darwin- to osx- and x86_64/aarch64 to x64/arm64.
#   --cross-check    Run even when --platform names another host. The explicit
#                    override: without it a mismatch is refused (exit 2).
#   --fixture PATH   Fixture PDF to extract (default: fixtures/structured.pdf)
#   --sentence TEXT  Known sentence the markdown must contain (default: Safety rules)
#
# (--artifact, --platform, --cross-check) is the contract every smoke shares;
# it lives in smoke_common.sh, sourced below. An unknown flag exits 2.
#
# Lanes: in a release lane (PDFLUENT_RELEASE_LANE=1, set by
# post_publish_smoke_runner.sh) --artifact is required — installing from PyPI
# would test the registry, not the artefact about to ship.

set -Eeuo pipefail

# The shared argument contract (--artifact, --platform, --cross-check):
# one implementation in smoke_common.sh, sourced by every smoke.
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/smoke_common.sh"
SMOKE_TAG="[smoke_python]"

WHEEL_PATH=""
EXTRACT_PKG=""
EXPECTED_VERSION=""
FIXTURE=""
SENTENCE="Safety rules"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact|--wheel) WHEEL_PATH="$2"; shift 2 ;;
        --extract-pkg) EXTRACT_PKG="$2"; shift 2 ;;
        --version) EXPECTED_VERSION="$2"; shift 2 ;;
        --platform) SMOKE_PLATFORM="$2"; shift 2 ;;
        --cross-check) SMOKE_CROSS_CHECK=1; shift ;;
        --fixture) FIXTURE="$2"; shift 2 ;;
        --sentence) SENTENCE="$2"; shift 2 ;;
        *) smoke_unknown_option "$1" ;;
    esac
done

smoke_check_platform

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../../.." && pwd)"
[[ -z "$FIXTURE" ]] && FIXTURE="${REPO_ROOT}/fixtures/structured.pdf"

echo "=== PDFluent Python consumer smoke test ==="
[[ -n "$WHEEL_PATH" ]] && echo "Wheel: ${WHEEL_PATH}" || echo "Wheel: <PyPI>"
[[ -n "$EXPECTED_VERSION" ]] && echo "Expected version: ${EXPECTED_VERSION}"
[[ -n "$SMOKE_PLATFORM" ]] && echo "Platform: ${SMOKE_PLATFORM}"
echo "Fixture: ${FIXTURE}"
echo ""

# Decided before the virtualenv exists: a missing artefact must fail fast,
# naming the path, rather than fall through to a PyPI install of something
# else (exit 2: argv / artefact-locate error, like smoke_node.sh).
if [[ -n "$WHEEL_PATH" ]]; then
    if [[ ! -f "$WHEEL_PATH" ]]; then
        echo "❌ no such wheel: ${WHEEL_PATH}" >&2
        exit 2
    fi
    case "$WHEEL_PATH" in
        *.whl) ;;
        *) echo "❌ not a wheel: ${WHEEL_PATH}" >&2; exit 2 ;;
    esac
    if [[ -z "$EXPECTED_VERSION" ]]; then
        EXPECTED_VERSION="$(basename "$WHEEL_PATH" | cut -d- -f2)"
        echo "Expected version: ${EXPECTED_VERSION} (off the wheel name)"
    fi
elif [[ "${PDFLUENT_RELEASE_LANE:-0}" == "1" ]]; then
    echo "❌ release lane needs --artifact: installing from PyPI would test the registry, not the artefact" >&2
    exit 2
fi

if [[ -n "$EXTRACT_PKG" ]]; then
    if [[ ! -f "$EXTRACT_PKG" ]]; then
        echo "❌ no such extract package: ${EXTRACT_PKG}" >&2
        exit 2
    fi
    case "$EXTRACT_PKG" in
        *.whl|*.tar.gz) ;;
        *) echo "❌ not an sdist or wheel: ${EXTRACT_PKG}" >&2; exit 2 ;;
    esac
    echo "Extract package: ${EXTRACT_PKG}"
fi

if ! command -v python3 &>/dev/null; then
    echo "❌ python3 not found in PATH"
    exit 3
fi

SMOKE_DIR=$(mktemp -d -t pdfluent_python_smoke_XXXXXX)
trap 'rm -rf "${SMOKE_DIR}"' EXIT

echo "Creating virtualenv..."
python3 -m venv "${SMOKE_DIR}/venv"
# Call the venv interpreter by path. activate is not used: on Windows the
# 3.11 script writes a drive-letter path into a colon-separated PATH, which
# Git Bash splits, so off that drive python3 and pip fall through to globals.
if [[ -x "${SMOKE_DIR}/venv/Scripts/python.exe" ]]; then
    VENV_PY="${SMOKE_DIR}/venv/Scripts/python.exe"
else
    VENV_PY="${SMOKE_DIR}/venv/bin/python"
fi
"$VENV_PY" -c "import sys; assert sys.prefix != sys.base_prefix, sys.prefix; print('venv:', sys.prefix)"

echo "Installing PDFluent..."
if [[ -n "$WHEEL_PATH" ]]; then
    # --no-index: the file under test is the whole install. Without it pip
    # may reach PyPI and the smoke vouches for a mix of local and remote.
    "$VENV_PY" -m pip install --no-index "$WHEEL_PATH"
else
    if [[ -z "$EXPECTED_VERSION" ]]; then
        "$VENV_PY" -m pip install pdfluent
    else
        "$VENV_PY" -m pip install "pdfluent==${EXPECTED_VERSION}"
    fi
fi

if [[ ! -f "$FIXTURE" ]]; then
    echo "❌ Fixture not found: ${FIXTURE}"
    exit 1
fi

# Without a licence key: the extraction must work, which is also a check of
# what the free tier promises. A key leaked into this environment would make
# the smoke prove nothing about the free tier.
unset PDFLUENT_LICENSE_KEY

echo "Running smoke checks..."
FIXTURE="$FIXTURE" SENTENCE="$SENTENCE" EXPECTED_VERSION="$EXPECTED_VERSION" "$VENV_PY" - <<'PY'
import json
import os
import sys

# 1. Import the package.
try:
    import pdfluent
    print("  import pdfluent: OK")
except ImportError as e:
    print(f"  import pdfluent: FAIL — {e}", file=sys.stderr)
    sys.exit(1)

# 2. The installed version is the artefact's version. With --artifact the
# expectation came off the wheel name: a mismatch means pip installed
# something else, and the smoke would vouch for the wrong file.
expected = os.environ.get("EXPECTED_VERSION") or ""
version = getattr(pdfluent, "__version__", None)
if expected:
    if version != expected:
        print(f"  __version__: FAIL — want {expected!r}, installed {version!r}",
              file=sys.stderr)
        sys.exit(1)
    print(f"  __version__: {version} (matches the artefact)")
elif version:
    print(f"  __version__: {version}")
else:
    print("  __version__: not exported (warning)")

# 3. Extract the fixture to markdown through the native extension. This is the
# smoke: it opens a real PDF, runs the engine, and asserts a known sentence.
# An import-only check passes on a wheel whose native module is broken.
fixture = os.environ["FIXTURE"]
sentence = os.environ["SENTENCE"]
try:
    with pdfluent.Document(fixture) as doc:
        markdown = doc.extract_markdown()
        if sentence not in markdown:
            print(f"  extract_markdown: FAIL — {sentence!r} not in output",
                  file=sys.stderr)
            print(markdown[:500], file=sys.stderr)
            sys.exit(1)
        print(f"  extract_markdown: OK ({len(markdown)} chars, {sentence!r} found)")
        first = markdown.splitlines()[0] if markdown.splitlines() else ""
        print(f"  first line: {first}")
        # 4. The DoclingDocument JSON parses: the second extraction surface.
        docling = json.loads(doc.extract_docling(name="smoke"))
        assert docling["name"] == "smoke", docling.get("name")
        print("  extract_docling: OK (valid DoclingDocument JSON)")
except Exception as e:
    print(f"  extraction: FAIL — {type(e).__name__}: {e}", file=sys.stderr)
    sys.exit(1)

print("")
print("Python consumer smoke PASS")
PY

STATUS=$?

# 5. The uvx-style leg: pdfluent-extract installed beside the wheel under
# test, its console script extracting the same fixture. --no-index keeps pip
# off the registry: the `pdfluent` dependency resolves to the wheel already
# in this venv, so the script runs against the artefact under test.
if [[ $STATUS -eq 0 && -n "$EXTRACT_PKG" ]]; then
    echo "Installing pdfluent-extract (--extract-pkg leg)..."
    if "$VENV_PY" -m pip install --no-index "$EXTRACT_PKG"; then
        echo "Running pdfluent-extract console script..."
        # The console script lives beside the venv interpreter (bin/ or Scripts/):
        # call it by path, as the interpreter itself (activate is not used).
        EXTRACT_BIN="$(dirname "$VENV_PY")/pdfluent-extract"
        [[ -x "$EXTRACT_BIN" ]] || EXTRACT_BIN="${EXTRACT_BIN}.exe"
        if [[ ! -x "$EXTRACT_BIN" ]]; then
            echo "  pdfluent-extract: FAIL — console script not in the venv after install ($EXTRACT_BIN)" >&2
            STATUS=1
        else
            EXTRACT_OUT="$("$EXTRACT_BIN" "$FIXTURE" 2>"${SMOKE_DIR}/extract.err")" || {
                echo "  pdfluent-extract: FAIL — exit $?" >&2
                sed 's/^/    /' "${SMOKE_DIR}/extract.err" >&2
                STATUS=1
            }
            if [[ $STATUS -eq 0 ]]; then
                case "$EXTRACT_OUT" in
                    *"$SENTENCE"*)
                        echo "  pdfluent-extract: OK (${#EXTRACT_OUT} chars, '$SENTENCE' found)"
                        echo "  first line: $(printf '%s' "$EXTRACT_OUT" | head -1)"
                        ;;
                    *)
                        echo "  pdfluent-extract: FAIL — '$SENTENCE' not in output" >&2
                        printf '%s' "$EXTRACT_OUT" | head -20 | sed 's/^/    /' >&2
                        STATUS=1
                        ;;
                esac
            fi
        fi
    else
        echo "  pdfluent-extract: FAIL — pip install of $EXTRACT_PKG failed" >&2
        STATUS=1
    fi
fi


if [[ $STATUS -eq 0 ]]; then
    echo ""
    echo "✅ Python consumer smoke PASS"
else
    echo ""
    echo "❌ Python consumer smoke FAIL"
    exit 1
fi
