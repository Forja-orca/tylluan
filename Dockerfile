# Tylluan — Docker image
# Base: debian:bookworm-slim (ONNX Runtime needs glibc)
#
# Usage:
#   docker build -t tylluan:latest .
#   docker run -d --name tylluan -p 3030:3030 -v ~/.tylluan:/data tylluan:latest

# ── Stage 1: Builder ──────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Dashboard build
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*
COPY dashboard/ ./dashboard/
RUN cd dashboard && npm install && npm run build

RUN cargo build --release --locked -p tylluan-kernel -p tylluan-cli --features encryption

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl libssl3 libsqlite3-0 libgomp1 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 tylluan

COPY --from=builder /build/target/release/tylluan-nexus /usr/local/bin/
COPY --from=builder /build/target/release/tylluan-cli /usr/local/bin/
COPY --from=builder /build/dashboard/dist /home/tylluan/dashboard/dist

# Install ONNX Runtime shared library based on target architecture
RUN dpkgArch="$(dpkg --print-architecture)" \
    && case "${dpkgArch}" in \
        amd64) ortArch="x64" ;; \
        arm64) ortArch="aarch64" ;; \
        *) echo "Unsupported architecture: ${dpkgArch}"; exit 1 ;; \
    esac \
    && curl -L -o /tmp/onnxruntime.tgz "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-${ortArch}-1.22.0.tgz" \
    && tar -zxvf /tmp/onnxruntime.tgz -C /tmp \
    && mv /tmp/onnxruntime-linux-${ortArch}-1.22.0/lib/libonnxruntime.so* /usr/lib/ \
    && rm -rf /tmp/onnxruntime* \
    && ldconfig

COPY tylluan.docker.toml /etc/tylluan/tylluan.toml
RUN mkdir -p /data && chown tylluan:tylluan /data

USER tylluan
WORKDIR /data
EXPOSE 3030

ENV RUST_LOG=info

VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -sf http://127.0.0.1:3030/health || exit 1

ENTRYPOINT ["/usr/local/bin/tylluan-nexus"]
CMD ["--config", "/etc/tylluan/tylluan.toml"]
