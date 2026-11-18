# Tylluan — Docker image
# Base: debian:bookworm-slim (ONNX Runtime needs glibc)
#
# Usage:
#   docker build -t tylluan:latest .
#   docker run -d --name tylluan -p 3030:3030 -v ~/.tylluan:/data tylluan:latest

# ── Stage 1: Builder ──────────────────────────────────────────────────────────
FROM rust:1.99.1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY assets/ ./assets/
COPY docs/ ./docs/

# Dashboard build
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && corepack enable \
    && rm -rf /var/lib/apt/lists/*
COPY packages/ ./packages/
RUN cd packages/tylluan-ui-core && npm ci --ignore-scripts && npm run build
COPY dashboard/ ./dashboard/
RUN cd dashboard && pnpm install --frozen-lockfile --ignore-scripts && pnpm run build

RUN cargo build --release --locked -p tylluan-kernel -p tylluan-cli --features encryption,bundled-dashboard

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl unzip libssl3 libsqlite3-0 libgomp1 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 tylluan

COPY --from=builder /build/target/release/tylluan-nexus /usr/local/bin/
COPY --from=builder /build/target/release/tylluan-cli /usr/local/bin/

# Install ONNX Runtime shared library based on target architecture.
#
# D204 (H-20261118-ORT-404): el .tgz de github.com/microsoft/onnxruntime dejó de
# existir (404 entero el 2026-11-18) y con él se caía esta imagen. La librería sale
# ahora de la RUEDA de onnxruntime en PyPI (`onnxruntime==1.22.0`), cuyo sha256 está
# fijado en el script y se verifica ANTES de extraer nada. Mismo script que usan
# ci.yml y scheduled-heavy-tests.yml: una sola fuente de verdad para el binario.
COPY scripts/install_onnxruntime.sh /usr/local/bin/install_onnxruntime.sh
RUN bash /usr/local/bin/install_onnxruntime.sh --dest /usr/lib

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
