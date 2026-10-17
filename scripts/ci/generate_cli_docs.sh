#!/usr/bin/env bash
# Generate docs/cli/*.md from --help and docs/cli/man/*.1 from the clap tree.
#
#   BIN=target/debug/pdfluent bash scripts/ci/generate_cli_docs.sh          # write
#   BIN=target/debug/pdfluent bash scripts/ci/generate_cli_docs.sh --check  # verify current
#
# The markdown pages embed `pdfluent <cmd> --help`; the man pages are rendered
# from the clap tree itself with clap_mangen by the binary's internal
# `generate-man-pages` command. The command list is derived from
# `pdfluent --help`, so a new subcommand without regenerated docs fails
# --check instead of shipping undocumented.
# Output is deterministic (no timestamps): --check is a plain recursive diff
# plus a stale-file check. Its CI coverage is pdfluent-cli tests/cli_docs.rs
# (markdown) and tests/cli_man.rs (man pages), which run this script with
# --check inside the workspace test suite.
#
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${BIN:-$REPO/target/debug/pdfluent}"
CHECK=0
if [ "${1:-}" = "--check" ]; then CHECK=1; fi

if [ ! -x "$BIN" ]; then
  echo "generate_cli_docs: no executable BIN=$BIN (build pdfluent-cli first)" >&2
  exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
GEN_DOCS="$WORK/docs"
GEN_MAN="$WORK/man"
mkdir -p "$GEN_DOCS" "$GEN_MAN"

VERSION="$("$BIN" --version | awk '{print $2}')"

# Commands from the top-level help: first words under "Commands:", minus help.
"$BIN" --help > "$WORK/top.txt"
COMMANDS=""
while IFS= read -r cmd; do
  COMMANDS="$COMMANDS $cmd"
done < <(awk '/^Commands:/{f=1;next} f&&/^$/{f=0} f&&/^  [a-z]/{print $1}' "$WORK/top.txt" | grep -v '^help$')
# shellcheck disable=SC2206
COMMANDS_ARR=($COMMANDS)

write_md() { # <slug> <title-invocation> <help-file> <outfile>
  local slug="$1" inv="$2" help="$3" out="$4"
  {
    echo "# \`$inv\`"
    echo
    echo "> Generated from \`$inv --help\` by \`scripts/ci/generate_cli_docs.sh\`. Do not edit; edit the clap definitions in \`crates/pdfluent-cli\` instead."
    echo
    echo "Version: \`$VERSION\`"
    echo
    echo "## Reference"
    echo
    echo '```text'
    cat "$help"
    echo '```'
  } > "$out"
}

# Top-level page.
write_md "index" "pdfluent" "$WORK/top.txt" "$GEN_DOCS/README.md"

# One page per command, plus xfa's subcommand.
for cmd in "${COMMANDS_ARR[@]}"; do
  "$BIN" "$cmd" --help > "$WORK/help-$cmd.txt"
  write_md "$cmd" "pdfluent $cmd" "$WORK/help-$cmd.txt" "$GEN_DOCS/$cmd.md"
  if [ "$cmd" = "xfa" ]; then
    "$BIN" xfa flatten --help > "$WORK/help-xfa-flatten.txt"
    write_md "xfa-flatten" "pdfluent xfa flatten" "$WORK/help-xfa-flatten.txt" "$GEN_DOCS/xfa-flatten.md"
  fi
done

# Man pages from the clap tree itself (clap_mangen), not from --help text.
"$BIN" generate-man-pages "$GEN_MAN" >&2

# Command index inside the top markdown page.
{
  echo
  echo "## Commands"
  echo
  for cmd in "${COMMANDS_ARR[@]}"; do
    echo "- [\`pdfluent $cmd\`]($cmd.md)"
  done
  echo "- [\`pdfluent xfa flatten\`](xfa-flatten.md)"
} >> "$GEN_DOCS/README.md"

if [ "$CHECK" = 1 ]; then
  fail=0
  # Markdown compares top-level files only: docs/cli/man/ is judged below.
  if [ ! -d "$REPO/docs/cli" ]; then echo "missing directory: $REPO/docs/cli" >&2; fail=1; fi
  if ! diff -q <(ls "$GEN_DOCS" | sort) <(ls "$REPO/docs/cli" | grep '\.md$' | sort) > "$WORK/diff.txt" 2>&1; then
    cat "$WORK/diff.txt"
    fail=1
  fi
  for f in "$GEN_DOCS"/*.md; do
    base="$(basename "$f")"
    if ! diff -q "$f" "$REPO/docs/cli/$base" > /dev/null 2>&1; then
      echo "differs (regenerate docs): docs/cli/$base" >&2
      fail=1
    fi
  done
  # Man pages compare the whole directory.
  if [ ! -d "$REPO/docs/cli/man" ]; then echo "missing directory: $REPO/docs/cli/man" >&2; fail=1; fi
  if ! diff -r -q "$GEN_MAN" "$REPO/docs/cli/man" > "$WORK/diff.txt" 2>&1; then
    grep -v "^Only in $REPO/docs/cli/man" "$WORK/diff.txt" || true
    fail=1
  fi
  # diff -r reports extras only one way; stale committed files fail too.
  while IFS= read -r f; do
    rel="${f#$REPO/docs/cli/man/}"
    if [ ! -e "$GEN_MAN/$rel" ]; then echo "stale file (regenerate docs): docs/cli/man/$rel" >&2; fail=1; fi
  done < <(find "$REPO/docs/cli/man" -type f 2>/dev/null | sort)
  if [ "$fail" = 1 ]; then
    echo "CLI docs are stale: run BIN=$BIN bash scripts/ci/generate_cli_docs.sh" >&2
    exit 1
  fi
  echo "CLI docs are current."
else
  mkdir -p "$REPO/docs/cli" "$REPO/docs/cli/man"
  cp "$GEN_DOCS"/* "$REPO/docs/cli/"
  cp "$GEN_MAN"/* "$REPO/docs/cli/man/"
  # Remove committed files the generator no longer emits (renamed commands).
  while IFS= read -r f; do
    rel="${f#$REPO/docs/cli/}"
    if [ ! -e "$GEN_DOCS/$rel" ]; then rm -f "$f"; fi
  done < <(find "$REPO/docs/cli" -maxdepth 1 -type f | sort)
  while IFS= read -r f; do
    rel="${f#$REPO/docs/cli/man/}"
    if [ ! -e "$GEN_MAN/$rel" ]; then rm -f "$f"; fi
  done < <(find "$REPO/docs/cli/man" -type f | sort)
  echo "Wrote docs/cli ($(ls "$GEN_DOCS" | wc -l | tr -d ' ') pages) and docs/cli/man ($(ls "$GEN_MAN" | wc -l | tr -d ' ') pages)."
fi
