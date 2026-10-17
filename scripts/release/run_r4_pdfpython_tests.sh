#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# run_r4_pdfpython_tests.sh — the r4 pdf-python suites against THIS tree.
#
# Builds the pdf-python extension from the working tree into a throwaway venv
# (maturin develop) and runs the four pytest files the r4 merge added: the
# font-info door test, the known-images and plain-images suites, and the door
# OCR content test. Hermetic: nothing is installed into the caller's Python,
# and the suites test the tree rather than whatever pdfluent the machine
# happens to have. The door difference self-test needs no built binding, so
# it runs as its own fast gate instead of here.
#
# Slow on a cold target dir (a debug engine build); incremental after that.
# Used by the landing/full lanes and the scoped CI step, never the fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_r4_pdfpython_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/r4-pdfpython-tests.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
python3 -m venv "$WORK/venv" || {
  echo "run_r4_pdfpython_tests.sh: python3 -m venv failed; on Debian/Ubuntu that is the missing python3-venv package (apt install python3-venv python3.12-venv for 3.12) -- install it and retry" >&2
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
"$VENV_PY" ../../scripts/ci/tests_actually_ran.py python -- \
  "$VENV_PY" -m pytest tests/test_compat_check_font_info.py \
    tests/test_known_images.py tests/test_plain_images.py \
    tests/door/test_l6_ocr_content.py -q
