#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Every generated register, regenerated or checked, in one command. (#526)
#
# WHY
#
# Six files in this tree are generated from the code and committed beside it,
# and each had its own gate with its own `--check` flag and no command that
# produced them all. On 10-09-2026 that cost three of ten gate runs in a row:
# the NOTICE copy, then docs/PUBLIC_API.json, then docs/CAPABILITY_REGISTER.md
# twice -- each one found by the run after the one that fixed the last, because
# a gate stops at the first failure and the next stale register was invisible
# until then. Half an hour of gate time for four `git add`s.
#
# So: one command that writes them all, and one check that names every stale one
# at once with the command that fixes them.
#
#   bash scripts/ci/regenerate_registers.sh           # write them
#   bash scripts/ci/regenerate_registers.sh --check   # name the stale ones
#
# THE NOTICE COPY IS A COPY, and that is the whole of its rule: the .NET package
# carries `bindings/dotnet/src/PDFluent/NOTICE` and a reader of one file has to
# be reading the other. `notice_names_what_exists.py` refuses when they differ;
# this is what makes them the same.
set -uo pipefail

WORTEL="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$WORTEL" || exit 2

MODUS="write"
case "${1-}" in
    --check) MODUS="check" ;;
    "") ;;
    *) echo "usage: regenerate_registers.sh [--check]" >&2; exit 2 ;;
esac

# file | the command that writes it | the command that checks it.
#
# Three of these take a bare `--check` and write with no flag at all; two do not,
# and pretending they did is how a list like this goes quietly wrong. So both
# commands are written out per file rather than derived. (codex, #1889)
NAMEN=(
    "docs/TEST_REACHABILITY.md|python3 scripts/ci/test_reachability.py|python3 scripts/ci/test_reachability.py --check"
    "docs/CAPABILITY_REGISTER.md|python3 scripts/ci/capability_register.py|python3 scripts/ci/capability_register.py --check"
    "docs/PUBLIC_API.json|python3 scripts/ci/export_public_api.py|python3 scripts/ci/export_public_api.py --check"
    "docs/site/snippets.json|python3 scripts/ci/extract_site_snippets.py --write docs/site/snippets.json|python3 scripts/ci/extract_site_snippets.py --check docs/site/snippets.json"
    "docs/site/cookbook.json|python3 scripts/ci/cookbook_recipes.py --write|python3 scripts/ci/cookbook_recipes.py --check"
)
# The copies of the two root registers that packaging tools cannot reach out to
# (#541): npm packs only what is inside the package directory and maturin
# refuses `..` in an include pattern, so the .NET, Node and Python packages
# carry committed copies. The wasm package's copies are made at packaging time
# by transform-wasm-pkg.sh, and the jar reads the root files directly, because
# Maven can name a directory outside the module.
KOPIEEN=(
    "bindings/dotnet/src/PDFluent/NOTICE|NOTICE"
    "bindings/dotnet/src/PDFluent/THIRD_PARTY_LICENSES.txt|THIRD_PARTY_LICENSES.txt"
    "crates/pdf-node/NOTICE|NOTICE"
    "crates/pdf-node/THIRD_PARTY_LICENSES.txt|THIRD_PARTY_LICENSES.txt"
    "crates/pdf-python/NOTICE|NOTICE"
    "crates/pdf-python/THIRD_PARTY_LICENSES.txt|THIRD_PARTY_LICENSES.txt"
    # The RAG loader packages carry our two licence texts in their sdists (#630).
    "integrations/langchain-pdfluent/LICENSE|LICENSE"
    "integrations/langchain-pdfluent/LICENSE-COMMERCIAL|LICENSE-COMMERCIAL"
    "integrations/llama-index-readers-pdfluent/LICENSE|LICENSE"
    "integrations/llama-index-readers-pdfluent/LICENSE-COMMERCIAL|LICENSE-COMMERCIAL"
    # And the MCP server package (#625).
    "integrations/pdfluent-mcp/LICENSE|LICENSE"
    "integrations/pdfluent-mcp/LICENSE-COMMERCIAL|LICENSE-COMMERCIAL"
    # And the two extract packages (#636).
    "integrations/pdfluent-extract/LICENSE|LICENSE"
    "integrations/pdfluent-extract/LICENSE-COMMERCIAL|LICENSE-COMMERCIAL"
    "integrations/pdfluent-extract-js/LICENSE|LICENSE"
    "integrations/pdfluent-extract-js/LICENSE-COMMERCIAL|LICENSE-COMMERCIAL"
)

# Test seam for scripts/ci/test_regenerate_registers.py: stub rows in place of
# the real generators, and no copies, so the failure paths run in milliseconds
# without cargo and without touching the real registers. Unset in every other
# use; with the seam absent production behaviour is identical.
if [ -n "${REGENERATE_REGISTERS_ROWS-}" ]; then
    NAMEN=()
    while IFS= read -r regel || [ -n "$regel" ]; do
        [ -n "$regel" ] && NAMEN+=("$regel")
    done <<<"${REGENERATE_REGISTERS_ROWS-}"
fi
if [ -n "${REGENERATE_REGISTERS_COPIES+x}" ]; then
    KOPIEEN=()
    while IFS= read -r paar || [ -n "$paar" ]; do
        [ -n "$paar" ] && KOPIEEN+=("$paar")
    done <<<"${REGENERATE_REGISTERS_COPIES-}"
fi

# A file the run compares a fresh write against. Before each generator runs,
# its file is backdated to the same stamp; afterwards the file must be newer.
# Backdated rather than marked fresh because `test -nt` resolves to the second
# on this machine's bash: a marker touched a millisecond before the generator
# runs reads as simultaneous with what it wrote. Year 2000 against today needs
# no resolution at all, and a rerun within the same second still passes.
tijdelijk="$(mktemp -d "${TMPDIR:-/tmp}/registers.XXXXXX")" \
    || { echo "[registers] GENERATION FAILED: mktemp: cannot create a temp dir" >&2; exit 2; }
trap 'rm -rf "$tijdelijk"' EXIT
referentie="$tijdelijk/referentie"
: >"$referentie" && touch -t 200001010000 "$referentie"

# A generator step that failed, on stderr, with the generator's own last lines:
# the run ends non-zero below and never reports OK, current, or no diff over
# this. The full output stays on stdout where it always went.
generatie_mislukt() {
    printf '[registers] GENERATION FAILED: %s: %s\n' "$1" "$2" >&2
    if [ -n "$3" ]; then
        printf '%s\n' "$3" | tail -n 10 | sed 's/^/    /' >&2
    fi
}

# What exit 126/127 mean in a cause: the tool itself is missing, not the file
# it was asked about. Worth one word because the fix is on PATH, not in the tree.
afsluit_hint() {
    case "$1" in
        127) printf ' (command not found)' ;;
        126) printf ' (not executable)' ;;
    esac
}

verouderd=()
mislukt=()
onleesbaar=()

for regel in "${NAMEN[@]}"; do
    bestand="${regel%%|*}"
    rest="${regel#*|}"
    maker="${rest%%|*}"
    controle="${rest#*|}"
    if [ "$MODUS" = check ]; then
        uit="$($controle 2>&1)"; code=$?
        case "$code" in
            0) ;;
            # A GENERATOR THAT COULD NOT RUN IS NOT A STALE FILE (codex, #1889).
            # `capability_register.py` exits 2 when `cargo metadata` fails, and
            # telling somebody to regenerate a file cannot repair a missing
            # toolchain -- it sends them to fix the wrong thing, which is the
            # shape of every SKIPPED-reported-as-a-failure in this repository,
            # with the sign flipped. It is still a failure, and named as one.
            1) verouderd+=("$bestand"); printf '%s\n' "$uit" | sed 's/^/    /' ;;
            *) generatie_mislukt "$controle" \
                   "$controle exited $code$(afsluit_hint "$code")" "$uit"
               onleesbaar+=("$bestand (exit $code)")
               printf '%s\n' "$uit" | sed 's/^/    /' ;;
        esac
    else
        # Backdate first, so a generator that exits 0 without writing is
        # visible: its file still carries the old stamp afterwards. Only the
        # mtime is touched; the bytes stay the stale ones until a real write.
        if [ -e "$bestand" ] && ! touch -t 200001010000 "$bestand" 2>/dev/null; then
            generatie_mislukt "$maker" "cannot backdate $bestand: permission denied?" ""
            mislukt+=("$bestand")
            continue
        fi
        if uit="$($maker 2>&1)"; then
            if [ -s "$bestand" ] && [ "$bestand" -nt "$referentie" ]; then
                echo "  wrote $bestand"
            else
                if [ ! -e "$bestand" ]; then
                    oorzaak="$bestand was not written"
                elif [ ! -s "$bestand" ]; then
                    oorzaak="$bestand is empty"
                else
                    oorzaak="$bestand was not rewritten"
                fi
                generatie_mislukt "$maker" "$oorzaak" "$uit"
                mislukt+=("$bestand")
                printf '%s\n' "$uit" | sed 's/^/    /'
            fi
        else
            code=$?
            generatie_mislukt "$maker" "$maker exited $code$(afsluit_hint "$code")" "$uit"
            mislukt+=("$bestand")
            printf '%s\n' "$uit" | sed 's/^/    /'
        fi
    fi
done

# The copies, which no generator owns.
for paar in "${KOPIEEN[@]}"; do
    kopie="${paar%%|*}"
    origineel="${paar#*|}"
    if [ "$MODUS" = check ]; then
        if [ ! -f "$kopie" ] || ! cmp -s "$origineel" "$kopie"; then
            verouderd+=("$kopie")
            echo "    $kopie is not the $origineel beside it"
        fi
    else
        if cp "$origineel" "$kopie"; then
            echo "  wrote $kopie (a copy of $origineel)"
        else
            mislukt+=("$kopie")
        fi
    fi
done

if [ "$MODUS" = check ]; then
    if [ ${#onleesbaar[@]} -ne 0 ]; then
        echo ""
        echo "[registers] SKIPPED (not a pass): ${#onleesbaar[@]} generator(s) could not run at all,"
        echo "  so nothing is claimed about the file(s) they own:"
        for f in "${onleesbaar[@]}"; do echo "    $f"; done
        echo ""
        echo "  Regenerating cannot repair this: the generator needs something this"
        echo "  machine does not have. Read its output above. (#526)"
        exit 2
    fi
    if [ ${#verouderd[@]} -eq 0 ]; then
        echo "[registers] OK: ${#NAMEN[@]} generated register(s) and ${#KOPIEEN[@]} copies are current"
        exit 0
    fi
    echo ""
    echo "[registers] ${#verouderd[@]} generated file(s) are stale:"
    for f in "${verouderd[@]}"; do echo "  $f"; done
    echo ""
    echo "  ALL of them, in one command, and then commit what it changes:"
    echo ""
    echo "      bash scripts/ci/regenerate_registers.sh"
    echo ""
    echo "  They are named together because they used to be found one gate run at"
    echo "  a time: a run stops at the first failure, so the second stale register"
    echo "  was invisible until the first was fixed. (#526)"
    exit 1
fi

if [ ${#mislukt[@]} -ne 0 ]; then
    echo ""
    echo "[registers] ${#mislukt[@]} generator(s) failed; nothing about the others is claimed:"
    for f in "${mislukt[@]}"; do echo "  $f"; done
    exit 1
fi
echo "[registers] wrote ${#NAMEN[@]} register(s) and ${#KOPIEEN[@]} copies. Commit what changed."
