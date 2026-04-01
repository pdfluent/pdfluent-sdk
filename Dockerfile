# syntax=docker/dockerfile:1.4
FROM debian:bookworm-slim AS builder

ARG TARGETARCH
ARG PYTHON_VERSION=3.11

RUN apt-get update && apt-get install -y --no-install-recommends \
    python${PYTHON_VERSION}=${PYTHON_VERSION}.* \
    python3-dev \
    python3-pip \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN case "${TARGETARCH}" in \
      amd64) ARCH=x86_64 ;; \
      arm64) ARCH=aarch64 ;; \
      *) ARCH=${TARGETARCH} ;; \
    esac && \
    curl -L --proto '=https' --tlsv1.2 -sSf \
      "https://github.com/jasperdew/xfa-native-rust/releases/latest/download/xfa-cli-${ARCH}-unknown-linux-musl.tar.gz" \
    | tar -xz -C /usr/local/bin/ xfa-cli && \
    chmod +x /usr/local/bin/xfa-cli

RUN pip install --no-cache-dir --break-system-packages \
    pdfluent==1.0.0-beta.1 \
    || true

FROM debian:bookworm-slim

ARG TARGETARCH
ARG PYTHON_VERSION=3.11

RUN apt-get update && apt-get install -y --no-install-recommends \
    python${PYTHON_VERSION}=${PYTHON_VERSION}.* \
    python3-pip \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

COPY --from=builder /usr/local/bin/xfa-cli /usr/local/bin/xfa-cli
COPY --from=builder /usr/local/lib/python3.11/dist-packages /usr/local/lib/python3.11/dist-packages

RUN chmod +x /usr/local/bin/xfa-cli \
    && xfa-cli --version

WORKDIR /data

ENTRYPOINT ["xfa-cli"]
CMD ["--help"]
