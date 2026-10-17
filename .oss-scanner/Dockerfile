# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Build image for external security scanning of this repository.
#
# The build context is a checkout of this repository. The build has network
# access; whatever runs in the image afterwards does not. So every toolchain,
# crate and tool used later is fetched here, and the test and fuzz binaries are
# compiled here too.
#
# What it leaves in the image:
#   /src                                   the checkout
#   /src/target/release/pdfluent           the command-line driver (crate xfa-cli)
#   /src/target/release/deps/*-<hash>      compiled test binaries of the core crates
#   /src/fuzz/target/<triple>/release/     the libFuzzer targets, built with
#                                          AddressSanitizer and debug assertions
#   /src/fuzz/corpus/<target>/             seed inputs from the repository's PDFs
#
# Re-run a fuzz target offline:
#   cd /src/fuzz && cargo +nightly-2026-10-01 fuzz run fuzz_pdf_parser -- -max_total_time=60
#
# Locally, on a small machine, pass --build-arg JOBS=4 to bound memory.
FROM rust:1.94.0-bookworm

ARG JOBS=
ARG NIGHTLY=nightly-2026-10-01
ARG CARGO_FUZZ_VERSION=0.13.2

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_TERM_COLOR=never \
    CARGO_NET_RETRY=5 \
    CARGO_INCREMENTAL=0 \
    RUST_BACKTRACE=1 \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_STRIP=none \
    CARGO_PROFILE_RELEASE_DEBUG=1 \
    RUSTFLAGS="-C force-frame-pointers=yes"

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates build-essential clang pkg-config git python3 \
    && rm -rf /var/lib/apt/lists/*

COPY . /src
WORKDIR /src

# The stable toolchain pinned in rust-toolchain.toml, the nightly that
# cargo-fuzz needs, and cargo-fuzz itself. A later `cargo` call must not try
# to download a toolchain or component, because it would have no network.
RUN rustup toolchain install \
    && rustup toolchain install "$NIGHTLY" --profile minimal \
    && cargo install cargo-fuzz --version "$CARGO_FUZZ_VERSION" --locked \
    && rustup show active-toolchain

# Every crate either workspace can reach, fetched while the network is up.
# fuzz/ is a workspace of its own. Its Cargo.lock trails the main workspace's,
# and its CI job builds without --locked, so it is resolved here the same way.
RUN cargo fetch --locked \
    && cd fuzz && cargo +"$NIGHTLY" fetch

# The public facade crate and the command-line driver. Release profile with
# LTO off, debug info on and symbols kept, so a backtrace names its frames.
RUN cargo build --release --locked ${JOBS:+-j "$JOBS"} -p pdfluent -p xfa-cli --bin pdfluent

# Test binaries of the parser and the crates closest to untrusted input, so
# they can be run offline. Building them must succeed; running them below is
# only a check that the image works.
RUN cargo test --release --locked ${JOBS:+-j "$JOBS"} --no-run \
      -p pdf-syntax -p pdfluent-lopdf -p xfa-dom-resolver -p formcalc-interpreter -p pdfluent
RUN cargo test --release --locked ${JOBS:+-j "$JOBS"} -p pdf-syntax --lib \
    || echo "WARNING: pdf-syntax tests reported failures"

# Every fuzz target, with AddressSanitizer and debug assertions.
RUN if [ -n "$JOBS" ]; then export CARGO_BUILD_JOBS="$JOBS"; fi \
    && cd fuzz && cargo +"$NIGHTLY" fuzz build --debug-assertions \
    && cargo +"$NIGHTLY" fuzz list

# Seed corpora for the targets that take a whole PDF: the small PDFs that are
# already in the repository.
RUN for t in fuzz_pdf_parser fuzz_lopdf_roundtrip fuzz_redact fuzz_pdfa_validate fuzz_xfa_extract; do \
      mkdir -p "fuzz/corpus/$t"; \
      find fixtures tests crates -name '*.pdf' -size -65k -not -path '*/target/*' \
        -exec cp {} "fuzz/corpus/$t/" \; ; \
    done \
    && ls fuzz/corpus/fuzz_pdf_parser | wc -l

# Prove the fuzz binaries start and run offline: a few seconds on one target.
RUN cd fuzz && cargo +"$NIGHTLY" fuzz run fuzz_pdf_parser corpus/fuzz_pdf_parser -- \
      -max_total_time=10 -max_len=65536 -rss_limit_mb=2048 \
    || echo "WARNING: fuzz_pdf_parser smoke run reported a failure"
