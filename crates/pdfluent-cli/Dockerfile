# syntax=docker/dockerfile:1.4
# Image: ghcr.io/pdfluent/pdfluent:<v> and :latest,
# multi-arch linux/amd64 + linux/arm64 (T6 §4).
#
# The image holds exactly the binaries the artefact gate checked: the build
# context is staged by scripts/release/cli_docker_build.sh from the two musl
# archives of §1 (pdfluent-amd64, pdfluent-arm64), not compiled here.
# Base is alpine:3.20 — a shell is there for debugging, and it is small.
FROM alpine:3.20

ARG TARGETARCH
ARG VERSION=0.0.0-dev

LABEL org.opencontainers.image.title="pdfluent" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="https://github.com/pdfluent/engine" \
      org.opencontainers.image.licenses="AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

WORKDIR /data

COPY pdfluent-${TARGETARCH} /usr/local/bin/pdfluent
COPY LICENSE LICENSE-COMMERCIAL NOTICE THIRD_PARTY_LICENSES.txt /usr/share/doc/pdfluent/

RUN chmod +x /usr/local/bin/pdfluent && /usr/local/bin/pdfluent --version

ENTRYPOINT ["pdfluent"]
