#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Install cargo-deny, or fail. There is no third outcome.
#
# WHY NOT THE ACTION
#
# `EmbarkStudios/cargo-deny-action@v2` is a Docker container action, and GitHub
# builds that container on the runner. Neither machine this repository owns has
# docker: the desktop does not, and the Hetzner snapshot the wave buys does not
# either. Measured over the last hundred runs of security-audit on master before
# 07-09-2026: 28 failures, 1 pass, and the last of them is
# `Build EmbarkStudios/cargo-deny-action@v2` answering `docker: command not
# found`. Moving the job to the bought instance changed the machine, not the
# missing dependency (#350, #361).
#
# Four vulnerabilities sat behind that job for months (#287). A licence and
# advisory scan that cannot start is worse than none, because a check by that
# name still appears in the run list.
#
# WHY A RELEASE BINARY AND NOT `cargo install`
#
# `cargo install cargo-deny --locked` compiles some four hundred crates on an
# instance that is billed by the hour, every wave, for a tool whose output does
# not depend on how it was built. The upstream release is a static musl binary;
# it starts in a second and costs nothing to keep.
#
# WHY THE HASH IS IN THIS FILE
#
# Upstream publishes a .sha256 next to each asset, and fetching it from the same
# host proves nothing: whoever could replace the tarball could replace the
# checksum beside it. The digests below were taken from the assets on
# 07-09-2026 and are what this repository has agreed to run. They also pin the
# VERSION, because cargo-deny's verdicts change between releases and a floating
# tool would fail this job for reasons that have nothing to do with our
# dependency graph.
#
# Usage: install_cargo_deny.sh [INSTALL_DIR]   (default /opt/cargo-deny)

set -euo pipefail

VERSION="0.20.2"
DEST="${1:-/opt/cargo-deny}"

case "$(uname -m)" in
    x86_64|amd64)  ARCH="x86_64";  SHA256="9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f" ;;
    aarch64|arm64) ARCH="aarch64"; SHA256="995c82be0defc7a025cae49a2aa2644ce8245c9a3318fc4103907c6a285e8c7d" ;;
    *)
        echo "[cargo-deny] FATAL: no pinned release for $(uname -m); add its digest here" >&2
        echo "[cargo-deny] rather than letting the job fetch something unverified." >&2
        exit 1
        ;;
esac

NAAM="cargo-deny-${VERSION}-${ARCH}-unknown-linux-musl"
URL="https://github.com/EmbarkStudios/cargo-deny/releases/download/${VERSION}/${NAAM}.tar.gz"

if [ -x "${DEST}/cargo-deny" ]; then
    # "Already present" is not "already correct". The Hetzner instance is reused
    # inside its paid hour, so a binary installed by an older commit survives
    # here, and a different cargo-deny grades the same dependency graph
    # differently. Same trap as install_verapdf.sh.
    have="$("${DEST}/cargo-deny" --version 2>&1 | head -1)"
    if printf '%s' "${have}" | grep -qF "${VERSION}"; then
        echo "[cargo-deny] ${VERSION} already present at ${DEST}"
        exit 0
    fi
    echo "[cargo-deny] ${DEST} holds ${have}, not the pinned ${VERSION}; replacing it"
    rm -rf "${DEST}"
fi

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

echo "[cargo-deny] downloading ${URL}"
curl -fsSL --retry 3 "${URL}" -o "${work}/deny.tar.gz"

# No `|| true`, and the comparison is on the digest this repository recorded --
# not on one fetched from the same place as the bytes.
if command -v sha256sum >/dev/null 2>&1; then
    gemeten="$(sha256sum "${work}/deny.tar.gz" | cut -d' ' -f1)"
else
    gemeten="$(shasum -a 256 "${work}/deny.tar.gz" | cut -d' ' -f1)"
fi
if [ "${gemeten}" != "${SHA256}" ]; then
    echo "[cargo-deny] FATAL: the archive is not the one this repository pinned." >&2
    echo "[cargo-deny]   expected ${SHA256}" >&2
    echo "[cargo-deny]   measured ${gemeten}" >&2
    echo "[cargo-deny] Either upstream replaced the asset or something else answered" >&2
    echo "[cargo-deny] the request. Neither is a thing to run with our credentials." >&2
    exit 1
fi

tar -xzf "${work}/deny.tar.gz" -C "${work}"
mkdir -p "${DEST}"
install -m 0755 "${work}/${NAAM}/cargo-deny" "${DEST}/cargo-deny"

# The install is not believed until the binary answers, and answers with the
# version that was asked for. A truncated extract can leave an executable file
# that is not a working program.
installed="$("${DEST}/cargo-deny" --version 2>&1 || true)"
if ! printf '%s' "${installed}" | grep -qF "${VERSION}"; then
    echo "[cargo-deny] FATAL: asked for ${VERSION}; the binary answered:" >&2
    printf '%s\n' "${installed}" | head -3 >&2
    exit 1
fi
echo "[cargo-deny] $(printf '%s' "${installed}" | head -1) at ${DEST}"
