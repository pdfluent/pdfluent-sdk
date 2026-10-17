#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# run_r4_node_tests.sh — the r4 Node suites against THIS tree.
#
# Installs the binding's npm dependencies, builds the native addon for the
# host (debug: these three suites need no ort model), and runs the three jest
# files the r4 merge added: known-images, plain-images, and the journal
# smoke. The per-push guards job runs the whole tests/ directory without the
# addon, where these files announce SKIPPED; here they run for real.
#
# Slow on a cold target dir (a debug napi build); incremental after that.
# Used by the landing/full lanes and the scoped CI step, never the fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo node npm; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_r4_node_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

cd crates/pdf-node
npm install --no-audit --no-fund
npm run build:debug
python3 ../../scripts/ci/tests_actually_ran.py node -- \
  npx jest --ci tests/known_images.test.js tests/plain_images.test.js \
    tests/journal.test.js
