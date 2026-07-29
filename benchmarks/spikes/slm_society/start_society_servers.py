"""Start 3 llama-server instances for the SLM Society spike.

Each instance runs on a different port with a different GGUF model.
Verified GGUFs (2026-07-29):
  Proposer:     SmolLM2-1.7B-Instruct   (bartowski, 56.7% IFEval, Apache 2.0)
  Critic:       Phi-3.5-mini-instruct    (bartowski, BBH 57.75, MIT)
  Synthesizer:  Qwen3-0.6B               (lmstudio-community, Apache 2.0)

Usage:
  python benchmarks/spikes/slm_society/start_society_servers.py

Or with environment variables:
  SLM_PROPOSER_REPO=bartowski/SmolLM2-1.7B-Instruct-GGUF
  SLM_PROPOSER_FILE=SmolLM2-1.7B-Instruct-Q4_K_M.gguf
  SLM_CRITIC_REPO=bartowski/Phi-3.5-mini-instruct-GGUF
  SLM_CRITIC_FILE=Phi-3.5-mini-instruct-Q4_K_M.gguf
  SLM_SYNTHESIZER_REPO=lmstudio-community/Qwen3-0.6B-GGUF
  SLM_SYNTHESIZER_FILE=Qwen3-0.6B-Q4_K_M.gguf
"""
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

# ── Configuration ──────────────────────────────────────────────────────────

SERVERS = [
    {
        "role": "proposer",
        "port": int(os.getenv("SLM_PROPOSER_PORT", "9001")),
        "repo": os.getenv("SLM_PROPOSER_REPO", "bartowski/SmolLM2-1.7B-Instruct-GGUF"),
        "file": os.getenv("SLM_PROPOSER_FILE", "SmolLM2-1.7B-Instruct-Q4_K_M.gguf"),
    },
    {
        "role": "critic",
        "port": int(os.getenv("SLM_CRITIC_PORT", "9002")),
        "repo": os.getenv("SLM_CRITIC_REPO", "bartowski/Phi-3.5-mini-instruct-GGUF"),
        "file": os.getenv("SLM_CRITIC_FILE", "Phi-3.5-mini-instruct-Q4_K_M.gguf"),
    },
    {
        "role": "synthesizer",
        "port": int(os.getenv("SLM_SYNTHESIZER_PORT", "9003")),
        "repo": os.getenv("SLM_SYNTHESIZER_REPO", "lmstudio-community/Qwen3-0.6B-GGUF"),
        "file": os.getenv("SLM_SYNTHESIZER_FILE", "Qwen3-0.6B-Q4_K_M.gguf"),
    },
]

# llama.cpp config (same defaults as llama_backend.py)
CTX_SIZE = int(os.getenv("LLAMA_CTX_SIZE", "2048"))
THREADS = int(os.getenv("LLAMA_THREADS", "0"))  # 0 = auto
N_GPU_LAYERS = int(os.getenv("LLAMA_N_GPU_LAYERS", "0"))
BATCH_SIZE = int(os.getenv("LLAMA_BATCH_SIZE", "512"))

# ── Helpers ────────────────────────────────────────────────────────────────

def find_llama_server():
    """Find llama-server binary. Same logic as llama_backend.py."""
    import shutil
    for name in ["llama-server", "llama-server.exe"]:
        found = shutil.which(name)
        if found:
            return found
    scripts = Path(sys.executable).parent / "Scripts"
    for name in ["llama-server.exe", "llama-server"]:
        candidate = scripts / name
        if candidate.exists():
            return str(candidate)
    cache_dir = Path.home() / ".cache" / "tylluan" / "llama-cpp"
    if cache_dir.exists():
        for root, dirs, files in os.walk(cache_dir):
            for f in files:
                if f in ("llama-server", "llama-server.exe"):
                    return str(Path(root) / f)
    return None


def download_model(repo_id, filename):
    """Download GGUF from HuggingFace. Returns local path."""
    from huggingface_hub import hf_hub_download
    cache_dir = Path.home() / ".cache" / "huggingface" / "hub"
    print(f"  Downloading {repo_id}::{filename}...")
    path = hf_hub_download(
        repo_id=repo_id,
        filename=filename,
        cache_dir=str(cache_dir),
    )
    print(f"  Ready: {path}")
    return path


def is_port_open(port):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        s.bind(("127.0.0.1", port))
        s.close()
        return True
    except OSError:
        s.close()
        return False


def wait_for_server(port, timeout=60):
    """Wait until llama-server responds on /v1/models."""
    url = f"http://127.0.0.1:{port}/v1/models"
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as r:
                if r.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(1)
    return False


def shutdown_servers(processes):
    """Gracefully stop all llama-server processes."""
    for p in processes:
        if p.poll() is None:
            print(f"  Stopping port {p.port}...")
            p.terminate()
    for p in processes:
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()


# ── Main ───────────────────────────────────────────────────────────────────

def main():
    print("=" * 72)
    print("SLM Society — Starting 3 llama-server instances")
    print("=" * 72)

    server_path = find_llama_server()
    if not server_path:
        print("ERROR: llama-server not found. Install llama.cpp first.")
        print("  pip install llama-cpp-python")
        print("  or download from https://github.com/ggerganov/llama.cpp/releases")
        sys.exit(1)
    print(f"Server binary: {server_path}")

    threads = THREADS if THREADS > 0 else (os.cpu_count() or 4)

    # Check ports
    for srv in SERVERS:
        if not is_port_open(srv["port"]):
            print(f"ERROR: Port {srv['port']} ({srv['role']}) already in use.")
            print("  Stop any existing llama-server on that port first.")
            sys.exit(1)

    # Download models
    print("\nDownloading models (if not cached)...")
    model_paths = {}
    for srv in SERVERS:
        model_paths[srv["role"]] = download_model(srv["repo"], srv["file"])

    # Start servers
    processes = []
    print("\nStarting servers...")
    for srv in SERVERS:
        cmd = [
            server_path,
            "--model", model_paths[srv["role"]],
            "--host", "127.0.0.1",
            "--port", str(srv["port"]),
            "--n-gpu-layers", str(N_GPU_LAYERS),
            "--ctx-size", str(CTX_SIZE),
            "--threads", str(threads),
            "--batch-size", str(BATCH_SIZE),
        ]
        import platform
        is_arm = platform.machine().lower() in ("armv7l", "aarch64", "arm64")
        if is_arm or N_GPU_LAYERS == 0:
            cmd.append("--mlock")

        p = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        p.port = srv["port"]
        p.role = srv["role"]
        processes.append(p)
        print(f"  {srv['role']:12s} port {srv['port']} (PID {p.pid})")

    # Wait for all servers to be ready
    print("\nWaiting for servers to load models...")
    all_ready = True
    for srv, p in zip(SERVERS, processes):
        if p.poll() is not None:
            stderr = p.stderr.read() if p.stderr else ""
            print(f"  {srv['role']:12s} FAILED to start: {stderr[:200]}")
            all_ready = False
            continue
        ready = wait_for_server(srv["port"], timeout=120)
        if ready:
            print(f"  {srv['role']:12s} READY on port {srv['port']}")
        else:
            print(f"  {srv['role']:12s} TIMEOUT on port {srv['port']}")
            all_ready = False

    if not all_ready:
        print("\nERROR: Not all servers started. Check errors above.")
        shutdown_servers(processes)
        sys.exit(1)

    # Write status file for experiment.py
    status = {
        "servers": {
            srv["role"]: {
                "port": srv["port"],
                "repo": srv["repo"],
                "file": srv["file"],
                "model_path": model_paths[srv["role"]],
            }
            for srv in SERVERS
        },
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
    }
    status_path = Path(__file__).parent / "servers_status.json"
    status_path.write_text(json.dumps(status, indent=2))
    print(f"\nStatus written to: {status_path}")

    print("\n" + "=" * 72)
    print("All 3 servers running. Press Ctrl+C to stop.")
    print("=" * 72)

    # Handle shutdown
    def on_signal(sig, frame):
        print("\nShutting down...")
        shutdown_servers(processes)
        sys.exit(0)

    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)

    # Keep running
    try:
        while True:
            # Check if any process died
            for p in processes:
                if p.poll() is not None:
                    print(f"\nWARNING: {p.role} (port {p.port}) exited unexpectedly.")
            time.sleep(5)
    except KeyboardInterrupt:
        pass
    finally:
        shutdown_servers(processes)


if __name__ == "__main__":
    main()
