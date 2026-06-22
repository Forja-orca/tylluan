# =============================================================================
# ForjaNexus o3 — Multi-stage Dockerfile
# =============================================================================
# Stage 1: Rust builder (compiles the kernel binary)
# Stage 2: Runtime — Debian slim + Python guilds
#
# Models are NOT baked in — mount ./models as a volume.
# Users can populate it via dashboard (POST /api/v1/config/device) or:
#   docker run forjanexus:3.0 --download-models
# =============================================================================

# ── Stage 1: Rust builder ────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Layer dependencies separately for cache efficiency
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build only forja-kernel (no GUI, no evals)
RUN cargo build --release --locked -p forja-kernel

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# curl: required by healthcheck; libssl3/libsqlite3: required by the binary
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libssl3 \
    libsqlite3-0 \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 forja

# Install Python guild dependencies + ONNX Runtime (for ort load-dynamic)
RUN python3 -m venv /opt/forja-venv \
    && /opt/forja-venv/bin/pip install --no-cache-dir \
        "mcp>=1.27.0" \
        "aiofiles>=24.0.0" \
        "pypdf>=5.0.0" \
        "pdfplumber>=0.11.0" \
        "pyyaml>=6.0.0" \
        "psutil>=6.0.0" \
        "onnxruntime>=1.20.0" \
    && ONNX_LIB=$(find /opt/forja-venv -name 'libonnxruntime.so.*' ! -type l | head -1) \
    && cp "$ONNX_LIB" /usr/local/lib/ \
    && ln -sf "/usr/local/lib/$(basename $ONNX_LIB)" /usr/local/lib/libonnxruntime.so \
    && ldconfig /usr/local/lib/

COPY --from=builder /build/target/release/forja-nexus /usr/local/bin/forja-nexus
COPY --from=builder /usr/local/lib/ /usr/local/lib/

# Copy Python guilds
COPY guilds/ /opt/forja/guilds/

# Config, data, and models directories — models is a VOLUME (user-provided)
RUN mkdir -p /home/forja/data /home/forja/models \
    && ln -s /opt/forja/guilds /home/forja/guilds \
    && chown -R forja:forja /home/forja /opt/forja

COPY forja.docker.toml /home/forja/forja.toml

USER forja
WORKDIR /home/forja

EXPOSE 3030

# models/ is where BGE-M3 and compatible models live.
# Mount your local ./models here:
#   docker run -v ./models:/home/forja/models forjanexus:3.0
VOLUME ["/home/forja/models", "/home/forja/data"]

ENV RUST_LOG=info \
    PATH="/opt/forja-venv/bin:$PATH" \
    LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH \
    PYTHONPATH=/opt/forja

ENTRYPOINT ["/usr/local/bin/forja-nexus"]
CMD ["--config", "/home/forja/forja.toml"]
