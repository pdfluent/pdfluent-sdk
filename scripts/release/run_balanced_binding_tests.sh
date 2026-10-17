#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# run_balanced_binding_tests.sh -- Balanced-tier binding parity, both channels.
#
# Installs the layout and tables models from a given base, then runs the
# balanced binding suites of the Python wheel and the Node addon against
# the CLI references they pin. Exits non-zero on any failure or on any
# skipped test: with the models installed nothing may skip, so a skip is
# an environment fault or a regression, never a pass.
#
# Usage:
#   PDFLUENT_MODELS_BASE=https://...|file:///... \
#   [PDFLUENT_CLI=path/to/pdfluent] [PDFLUENT_MODEL_DIR=dir] \
#   scripts/release/run_balanced_binding_tests.sh
#
# Environment:
#   PDFLUENT_MODELS_BASE  Model download base, required. `https://` or
#                         `file://`; the installer enforces the release pins
#                         identically against either.
#   PDFLUENT_CLI          The `pdfluent` CLI that runs `models install`.
#                         Default: `pdfluent` from PATH.
#   PDFLUENT_MODEL_DIR    Where the models install to and where the
#                         bindings look for them. Default: the per-user
#                         model dir both sides resolve on their own.
#
# Prerequisites (failing clearly when absent): the CLI above, `python3`
# with `pytest` and the built wheel importable, `node` with the addon's
# dependencies installed (`npx --no-install jest` must resolve).

set -Eeuo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOG_DIR=""

fail() {
  echo "run_balanced_binding_tests: FAIL: $1" >&2
  exit 1
}

note() {
  echo "run_balanced_binding_tests: $1"
}

if [ -z "${PDFLUENT_MODELS_BASE:-}" ]; then
  fail "PDFLUENT_MODELS_BASE is not set (want an https:// or file:/// base)"
fi

if ! command -v python3 >/dev/null 2>&1; then
  fail "python3 is not on PATH"
fi
if ! command -v node >/dev/null 2>&1; then
  fail "node is not on PATH"
fi
if ! command -v npx >/dev/null 2>&1; then
  fail "npx is not on PATH"
fi

CLI="${PDFLUENT_CLI:-pdfluent}"
if ! command -v "$CLI" >/dev/null 2>&1; then
  if [ ! -x "$CLI" ]; then
    fail "no pdfluent CLI: '$CLI' is neither on PATH nor an executable file (set PDFLUENT_CLI)"
  fi
fi

LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/balanced-binding-tests.XXXXXX")"
note "logs in $LOG_DIR"

note "installing layout + tables from $PDFLUENT_MODELS_BASE"
if PDFLUENT_MODELS_BASE="$PDFLUENT_MODELS_BASE" "$CLI" models install layout tables >"$LOG_DIR/models-install.log" 2>&1; then
  note "models installed"
else
  cat "$LOG_DIR/models-install.log" >&2
  fail "'$CLI models install layout tables' failed (log above)"
fi

note "running the Python balanced suite"
if (cd "$ROOT/crates/pdf-python" && python3 -m pytest tests/test_balanced_bindings.py -v >"$LOG_DIR/pytest.log" 2>&1); then
  note "pytest exited 0"
else
  cat "$LOG_DIR/pytest.log" >&2
  fail "pytest tests/test_balanced_bindings.py failed (log above)"
fi
if tail -n 3 "$LOG_DIR/pytest.log" | grep -Eq '[0-9]+ (skipped|failed|error|deselected|xfailed|xpassed)'; then
  cat "$LOG_DIR/pytest.log" >&2
  fail "pytest reported an unexpected skip or failure (log above)"
fi

note "running the Node balanced suite"
if (cd "$ROOT/crates/pdf-node" && PDFLUENT_REQUIRE_ADDON=1 npx --no-install jest tests/balanced_bindings.test.js --json --outputFile="$LOG_DIR/jest.json" >"$LOG_DIR/jest.log" 2>&1); then
  note "jest exited 0"
else
  cat "$LOG_DIR/jest.log" >&2
  fail "jest tests/balanced_bindings.test.js failed (log above)"
fi
JEST_COUNTS="$(node -e 'const fs = require("fs"); const r = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); console.log(r.numFailedTests + " " + r.numPendingTests);' "$LOG_DIR/jest.json")"
if [ "$JEST_COUNTS" != "0 0" ]; then
  cat "$LOG_DIR/jest.log" >&2
  fail "jest reported failed/pending tests: '$JEST_COUNTS' (want '0 0'; log above)"
fi

note "PASS: both balanced suites green with no skips"
