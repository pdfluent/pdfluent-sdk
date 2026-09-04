#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Install veraPDF, or fail. There is no third outcome.
#
# The two installs this replaces both ended in `|| true`, and one of them then
# put /opt/verapdf on PATH whether or not anything had landed there:
#
#     java -jar "$INSTALLER_JAR" auto-install.xml || true
#     /opt/verapdf/verapdf --version || echo "veraPDF install optional"
#
# That is the whole distance between the local run and the CI run on #1543.
# Locally veraPDF answered and the wrapper counted five failures; in CI nothing
# was installed, the wrapper found no report to read, and five failures became
# five "errors" -- a different word for the same nothing, under a green tick.
# A gate that cannot find its own tool has not measured anything, and the only
# honest thing it can do is stop.
#
# The version is pinned on purpose. veraPDF's verdicts change between releases,
# and the gate downstream compares a conformance count against a fixed floor;
# a floating installer would move that count for reasons that have nothing to
# do with this repository, and the first person to look would be debugging our
# converter over somebody else's bug fix.
#
# Usage: install_verapdf.sh [INSTALL_DIR]   (default /opt/verapdf)

set -euo pipefail

VERSION="1.28.2"
SERIES="1.28"
DEST="${1:-/opt/verapdf}"
URL="https://software.verapdf.org/releases/${SERIES}/verapdf-greenfield-${VERSION}-installer.zip"

if [ -x "${DEST}/verapdf" ]; then
    # "Already present" is not "already correct". The Hetzner image is reused
    # between runs, so a validator installed by an older commit survives here --
    # and the retention floors this repository records were measured against a
    # named version. A different one grades the same bytes differently, which
    # produces a pass or a failure nobody can trace to a version change.
    # (codex, #1617)
    have="$("${DEST}/verapdf" --version 2>&1 | head -1)"
    if printf '%s' "${have}" | grep -qF "${VERSION}"; then
        echo "[verapdf] ${VERSION} already present at ${DEST}"
        exit 0
    fi
    echo "[verapdf] ${DEST} holds a different version than the pinned ${VERSION}:"
    echo "[verapdf]   ${have}"
    echo "[verapdf] replacing it, so the floors are graded by the validator they"
    echo "[verapdf] were measured with."
    rm -rf "${DEST}"
fi

java -version >/dev/null 2>&1 || {
    echo "[verapdf] FATAL: no working java; the installer is a JAR and the" >&2
    echo "[verapdf] launcher it writes is a JVM wrapper. On macOS \`command -v java\`" >&2
    echo "[verapdf] finds a stub that is not a runtime, so this asks java itself." >&2
    exit 1
}

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

echo "[verapdf] downloading ${URL}"
curl -fsSL --retry 3 "${URL}" -o "${work}/verapdf-installer.zip"
unzip -q "${work}/verapdf-installer.zip" -d "${work}/installer"

jar="$(find "${work}/installer" -name '*.jar' -print -quit)"
[ -n "${jar}" ] || {
    echo "[verapdf] FATAL: the archive contains no installer JAR" >&2
    exit 1
}

cat > "${work}/auto-install.xml" <<XML
<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<AutomatedInstallation langpack="eng">
  <com.izforge.izpack.panels.target.TargetPanel id="install_dir">
    <installpath>${DEST}</installpath>
  </com.izforge.izpack.panels.target.TargetPanel>
  <com.izforge.izpack.panels.install.InstallPanel id="install"/>
</AutomatedInstallation>
XML

# No `|| true`. If izpack cannot write to DEST, this is where the run stops.
java -jar "${jar}" "${work}/auto-install.xml"

# And the install is not believed until the binary answers. izpack can exit 0
# having written a directory that holds no runnable veraPDF.
[ -x "${DEST}/verapdf" ] || {
    echo "[verapdf] FATAL: installer exited 0 but ${DEST}/verapdf is not executable" >&2
    ls -la "${DEST}" >&2 || true
    exit 1
}
installed="$("${DEST}/verapdf" --version 2>&1 || true)"
if ! printf '%s' "${installed}" | grep -q "veraPDF ${VERSION}"; then
    echo "[verapdf] FATAL: asked for ${VERSION}; the launcher answered:" >&2
    printf '%s\n' "${installed}" | head -3 >&2
    echo "[verapdf] The conformance floor downstream was measured against" >&2
    echo "[verapdf] ${VERSION}, so any other build grades a different question." >&2
    echo "[verapdf] (\"Unable to locate a Java Runtime\" here means the launcher" >&2
    echo "[verapdf] needs JAVA_HOME, not that the install failed.)" >&2
    exit 1
fi
echo "[verapdf] $(printf '%s' "${installed}" | head -1) at ${DEST}"
