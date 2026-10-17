#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# rosetta_x64.sh: run a command with the runner's x64 tools under Rosetta (#636).
#
#   bash scripts/release/rosetta_x64.sh <command...>
#
# The osx-x64 legs cross-build on the Apple Silicon Mac, so their smokes
# cannot run on the native tools: this wrapper runs the command under
# arch -x86_64 with a PATH whose first entries are the x64 python, node,
# java and dotnet dirs from $PDFLUENT_TOOLS, then /usr/bin:/bin, and
# JAVA_HOME/DOTNET_ROOT set to the x64 sides. The maven dir rides along
# when it is there: maven is architecture-independent (a launcher over
# whatever java is on PATH, here the x64 one) and the java smoke refuses
# without mvn.
#
# It refuses unless on Darwin with Rosetta available (arch -x86_64 runs),
# $PDFLUENT_TOOLS names the tool tree, and the four x64 dirs exist. Before
# the command it logs the machine architecture, the x64 python's
# platform.machine() and the x64 node's process.arch, refusing unless they
# read x86_64/x64.
set -euo pipefail

die() { echo "rosetta_x64: REFUSED: $*" >&2; exit 2; }
log() { echo "rosetta_x64: $*" >&2; }

[ "$(uname -s)" = "Darwin" ] || die "this wrapper runs x64 macOS tools under Rosetta; refusing on $(uname -s)"
arch -x86_64 /usr/bin/true 2>/dev/null || die "Rosetta is not available here (arch -x86_64 fails); refusing"
[ -n "${PDFLUENT_TOOLS:-}" ] || die "PDFLUENT_TOOLS is unset: the x64 tools live under \$PDFLUENT_TOOLS (the runner's .env sets it)"
[ "$#" -ge 1 ] || die "want a command to run (rosetta_x64.sh <command...>)"

# One python minor and one Temurin per runner, but take the newest when
# several are there, and refuse when none is.
py_candidates=()
for d in "$PDFLUENT_TOOLS"/python-*-x64; do
  [ -x "$d/bin/python3" ] && py_candidates+=( "$d" )
done
[ "${#py_candidates[@]}" -ge 1 ] \
  || die "no x64 python under ${PDFLUENT_TOOLS} (want python-<minor>-x64 from the setup)"
py_x64="$(printf '%s\n' "${py_candidates[@]}" | sort | tail -1)"

node_x64="$PDFLUENT_TOOLS/node-x64"
[ -x "$node_x64/bin/node" ] \
  || die "no x64 node under ${PDFLUENT_TOOLS} (want node-x64 from the setup)"

java_candidates=()
for d in "$PDFLUENT_TOOLS"/temurin-*-x64; do
  [ -x "$d/bin/javac" ] && java_candidates+=( "$d" )
done
[ "${#java_candidates[@]}" -ge 1 ] \
  || die "no x64 java under ${PDFLUENT_TOOLS} (want temurin-17-x64 from the setup)"
java_x64="$(printf '%s\n' "${java_candidates[@]}" | sort | tail -1)"

dotnet_x64="$PDFLUENT_TOOLS/dotnet-x64"
[ -x "$dotnet_x64/dotnet" ] \
  || die "no x64 dotnet under ${PDFLUENT_TOOLS} (want dotnet-x64 from the setup)"

maven=""
for d in "$PDFLUENT_TOOLS"/apache-maven-*; do
  if [ -x "$d/bin/mvn" ]; then
    if [ -z "$maven" ] || [[ "$d" > "$maven" ]]; then maven="$d"; fi
  fi
done

new_path="$py_x64/bin:$node_x64/bin:$java_x64/bin:$dotnet_x64"
[ -n "$maven" ] && new_path="$new_path:$maven/bin"
new_path="$new_path:/usr/bin:/bin"
export PATH="$new_path"
export JAVA_HOME="$java_x64"
export DOTNET_ROOT="$dotnet_x64"

machine="$(arch -x86_64 uname -m)"
py_machine="$(arch -x86_64 python3 -c 'import platform;print(platform.machine())')"
node_arch="$(arch -x86_64 node -p process.arch)"
log "arch: $(arch) / uname -m: ${machine} / python3: ${py_machine} / node: ${node_arch}"
[ "$machine" = "x86_64" ] || die "self-check: uname -m reads ${machine}, not x86_64"
[ "$py_machine" = "x86_64" ] || die "self-check: the x64 python reports ${py_machine}, not x86_64"
[ "$node_arch" = "x64" ] || die "self-check: the x64 node reports ${node_arch}, not x64"

exec arch -x86_64 "$@"
