#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
# run_r4_capi_tests.sh — the C API tests against THIS tree.
#
# Builds pdf-capi (release, locked: the Makefile links the release dylib)
# and runs `make run` in crates/pdf-capi/tests, whose TESTS list the r4
# merge extended with test_journal, test_form_write and test_plain_images.
# Same two commands capi-ephemeral.yml runs on its instance, so this gate
# can never drift from it. A positive executed count is required, so a run
# that built zero binaries still fails.
#
# Slow on a cold target dir (a release cdylib build); incremental after
# that. Used by the landing/full lanes and the scoped CI step, never the
# fast lane.
set -Eeuo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

for tool in cargo cc make python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "run_r4_capi_tests.sh: need $tool on PATH" >&2
    exit 1
  fi
done

cargo build -p pdf-capi --release --locked
python3 scripts/ci/tests_actually_ran.py capi -- \
  make -C crates/pdf-capi/tests run
