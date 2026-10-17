# syntax=docker/dockerfile:1.4
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# The `pdfluent` command-line tool, built from this checkout.
#
#   docker build -t pdfluent .
#   docker run --rm -v "$PWD:/data" pdfluent info /data/file.pdf
#
# Everything it needs is in this repository and on crates.io, so it builds
# from a public clone with no credentials.
FROM rust:1.94.0-bookworm AS builder

WORKDIR /src
COPY . .
RUN rustup toolchain install \
    && cargo build --release --locked -p xfa-cli --bin pdfluent \
    && install -m 0755 target/release/pdfluent /usr/local/bin/pdfluent

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/pdfluent /usr/local/bin/pdfluent
RUN pdfluent --version

WORKDIR /data

ENTRYPOINT ["pdfluent"]
CMD ["--help"]
