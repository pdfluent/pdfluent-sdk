#!/usr/bin/env bash
# run_office_tests.sh — the extract_office suite against THIS tree.
#
# Builds the pdf-python extension from the working tree into a throwaway venv
# (maturin develop) and runs tests/test_extract_office.py against it. Hermetic:
# nothing is installed into the caller's Python, and the suite tests the tree
# rather than whatever pdfluent the machine happens to have.
#
# Slow on a cold target dir (a debug engine build); incremental after that.
# Used by the landing/full lanes and the scoped CI step, never the fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_office_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/office-tests.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
python3 -m venv "$WORK/venv" || {
  echo "run_office_tests.sh: python3 -m venv failed; on Debian/Ubuntu that is the missing python3-venv package (apt install python3-venv python3.12-venv for 3.12) -- install it and retry" >&2
  exit 1
}
# The venv interpreter by path, not activate: activate is what put a
# Windows drive-letter path into PATH and sent pip to the global
# interpreter (r9d). maturin finds the venv through VIRTUAL_ENV.
if [[ -x "$WORK/venv/Scripts/python.exe" ]]; then VENV_PY="$WORK/venv/Scripts/python.exe"; else VENV_PY="$WORK/venv/bin/python"; fi
export VIRTUAL_ENV="$WORK/venv"
"$VENV_PY" -c "import sys; assert sys.prefix != sys.base_prefix, sys.prefix"
# A conda shell exports CONDA_PREFIX beside the venv's VIRTUAL_ENV and maturin
# refuses the pair; the venv is the environment here, conda is the launcher.
unset CONDA_PREFIX
"$VENV_PY" -m pip install maturin pytest

cd crates/pdf-python
"$VENV_PY" -m maturin develop
"$VENV_PY" -m pytest tests/test_extract_office.py -q
