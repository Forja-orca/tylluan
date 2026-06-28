# =============================================================================
# Tylluan o3 — Multi-stage Dockerfile
# =============================================================================
# Stage 1: Rust builder (compiles the kernel binary)
# Stage 2: Runtime — Debian slim + Python guilds
#
# Models are NOT baked in — mount ./models as a volume.
# Users can populate it via dashboard or:
#   docker run tylluan:3.0 --download-models
# =============================================================================

# ── Stage 1: Rust builder ────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    perl \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 22 for dashboard build
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Layer Rust dependencies separately for cache efficiency
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Copy dashboard and build it BEFORE cargo build (build.rs checks dist/ → no-op)
COPY dashboard/ ./dashboard/
RUN cd dashboard && npm install && npm run build

# Build only tylluan-kernel (no GUI, no evals)
# build.rs finds dist/ already present and skips rebuild
RUN cargo build --release --locked -p tylluan-kernel --features encryption

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
    && useradd -m -u 1000 tylluan

# Install Python guild dependencies + ONNX Runtime (for ort load-dynamic)
RUN python3 -m venv /opt/tylluan-venv \
    && /opt/tylluan-venv/bin/pip install --no-cache-dir \
        "mcp>=1.27.0" \
        "aiofiles>=24.0.0" \
        "pypdf>=5.0.0" \
        "pdfplumber>=0.11.0" \
        "pyyaml>=6.0.0" \
        "psutil>=6.0.0" \
        "onnxruntime>=1.20.0" \
    && ONNX_LIB=$(/opt/tylluan-venv/bin/python -c "import onnxruntime, pathlib, os; so = [f for f in pathlib.Path(onnxruntime.__file__).parent.rglob('libonnxruntime*.so*') if not os.path.islink(f)][0]; print(so)") \
    && cp "$ONNX_LIB" /usr/local/lib/ \
    && ln -sf "/usr/local/lib/$(basename $ONNX_LIB)" /usr/local/lib/libonnxruntime.so \
    && ldconfig /usr/local/lib/

COPY --from=builder /build/target/release/tylluan-nexus /usr/local/bin/tylluan-nexus
COPY --from=builder /build/dashboard/dist /home/tylluan/dashboard/dist
COPY --from=builder /usr/local/lib/ /usr/local/lib/

# Copy Python guilds
COPY guilds/ /opt/tylluan/guilds/

# Config, data, and models directories — models is a VOLUME (user-provided)
# Generate a local .encryption_key file secure with 600 permissions
RUN mkdir -p /home/tylluan/data /home/tylluan/models \
    && ln -s /opt/tylluan/guilds /home/tylluan/guilds \
    && openssl rand -hex 32 > /home/tylluan/.encryption_key \
    && chmod 600 /home/tylluan/.encryption_key \
    && chown -R tylluan:tylluan /home/tylluan /opt/tylluan

COPY tylluan.docker.toml /home/tylluan/tylluan.toml

USER tylluan
WORKDIR /home/tylluan

EXPOSE 3030

# models/ is where BGE-M3 and compatible models live.
# Mount your local ./models here:
#   docker run -v ./models:/home/tylluan/models tylluan:3.0
VOLUME ["/home/tylluan/models", "/home/tylluan/data"]

ENV RUST_LOG=info \
    PATH="/opt/tylluan-venv/bin:$PATH" \
    LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH \
    PYTHONPATH=/opt/tylluan

ENTRYPOINT ["/usr/local/bin/tylluan-nexus"]
CMD ["--config", "/home/tylluan/tylluan.toml"]
