#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The one training environment for learned decisions (#598), for every lane.
#
#   bash scripts/extraction/learned/setup_venv.sh
#   ~/.cache/pdfluent/learned-venv/bin/python scripts/extraction/learned/train.py ...
#
# OUTSIDE THE TREE on purpose: the push gate scans every script under scripts/,
# and a venv there is thousands of third-party files it would judge.
#
# WHY A VENV OF ITS OWN. LightGBM crashed on this Mac with no traceback as soon
# as it used more than one thread, in fit and in predict (T4 lost three runs).
# Two OpenMP runtimes were loaded into one process: the libomp the LightGBM
# wheel links (brew's, /usr/local/opt/libomp) and the one anaconda's MKL numpy
# brings. A venv with pip's numpy (OpenBLAS, no OpenMP) loads exactly one.
# KMP_DUPLICATE_LIB_OK=TRUE hides the crash and can give silently wrong
# results; it is refused below, not set.
set -euo pipefail
HIER="$(cd "$(dirname "$0")" && pwd)"
VENV="${LEARNED_VENV:-$HOME/.cache/pdfluent/learned-venv}"
PY="${LEARNED_PYTHON:-python3}"

if [ -n "${KMP_DUPLICATE_LIB_OK:-}" ]; then
  echo "setup_venv: KMP_DUPLICATE_LIB_OK is set; unset it. It hides two OpenMP runtimes instead of removing one." >&2
  exit 1
fi
if [ "$(uname -s)" = Darwin ] && [ ! -e /usr/local/opt/libomp/lib/libomp.dylib ] && [ ! -e /opt/homebrew/opt/libomp/lib/libomp.dylib ]; then
  echo "setup_venv: no libomp; 'brew install libomp' first (the LightGBM wheel links it)." >&2
  exit 1
fi
[ -x "$VENV/bin/python" ] || "$PY" -m venv "$VENV"
"$VENV/bin/python" -m pip install -q --disable-pip-version-check -r "$HIER/requirements.txt"
"$VENV/bin/python" "$HIER/smoke_threads.py"
