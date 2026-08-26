"""llama_backend guild: llama-server subprocess with GGUF auto-download.

P0 (M19 infrastructure): replaces the manual ONNX Gemma-4 loop with a
production-grade llama.cpp backend. Manages a llama-server subprocess
that exposes an OpenAI-compatible HTTP API on a local port. Other guilds
(night_reasoner, DeepEval, CoherenceGate) call this endpoint instead of
doing manual ONNX inference.

Architecture:
- Auto-installs llama-cpp-python (→ llama-server binary) on first use
- Auto-downloads GGUF model from HuggingFace hub
- Starts llama-server as a managed subprocess
- Health-check endpoint for dashboard
- Agnostic to the caller: any guild can POST /v1/chat/completions

Default model: SmolLM2-135M-Instruct GGUF (~200MB, works on everything).
Dashboard (P2) will add a selector for different model sizes.
"""
import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("llama_backend")

DEFAULT_MODEL = "bartowski/Qwen2.5-0.5B-Instruct-GGUF"
DEFAULT_MODEL_FILE = "Qwen2.5-0.5B-Instruct-Q4_K_M.gguf"


def _read_config():
    """Read llama-server config from tylluan.toml [inference.llama] section.
    Falls back to reasonable defaults if the section is missing."""
    import tomllib
    root = Path(__file__).resolve().parent.parent.parent
    config_path = root / "tylluan.toml"
    try:
        with open(config_path, "rb") as f:
            cfg = tomllib.load(f)
        llama_cfg = cfg.get("inference", {}).get("llama", {})
    except Exception:
        llama_cfg = {}

    return {
        "port": llama_cfg.get("port", 9000),
        "n_gpu_layers": llama_cfg.get("n_gpu_layers", 0),
        "ctx_size": llama_cfg.get("ctx_size", 2048),
        "n_keep": llama_cfg.get("n_keep", 0),  # tokens del prompt inicial conservados al desplazar contexto
        "threads": llama_cfg.get("threads", 0),  # 0 = auto (cpu_count)
        "batch_size": llama_cfg.get("batch_size", 512),
        "temperature": llama_cfg.get("temperature", 0.7),
        "top_p": llama_cfg.get("top_p", 0.95),
        "top_k": llama_cfg.get("top_k", 64),
        "repeat_penalty": llama_cfg.get("repeat_penalty", 1.1),
    }


_CFG = _read_config()
_CFG_LOADED_TS = time.time()
LLAMA_PORT = _CFG["port"]


def _get_config():
    """Return current config, reloading from tylluan.toml every 60s.
    Allows runtime config changes without guild restart."""
    global _CFG, _CFG_LOADED_TS, LLAMA_PORT
    now = time.time()
    if now - _CFG_LOADED_TS > 60:
        try:
            new_cfg = _read_config()
            _CFG = new_cfg
            LLAMA_PORT = new_cfg["port"]
            _CFG_LOADED_TS = now
        except Exception:
            pass  # keep stale config if file is temporarily unreadable
    return _CFG

# P1: External backend support — if the user has external LLM providers
# configured via `[[external_providers]]` in tylluan.toml (OpenAI-compatible,
# Anthropic-compatible, or Ollama-compatible), use those instead of starting
# our own llama-server. Supports MULTIPLE providers simultaneously.
#
# Config format (tylluan.toml):
#   [[external_providers]]
#   name = "openai"
#   type = "openai_compatible"
#   base_url = "https://api.openai.com/v1"
#   api_key_env = "OPENAI_API_KEY"
#   models = ["gpt-4o", "gpt-4o-mini"]
#
# Falls back to legacy env-var detection (OPENAI_BASE_URL, OLLAMA_HOST, etc.)
# when no [[external_providers]] are configured.
_EXTERNAL_PROVIDERS = {}       # name -> {type, base_url, api_key, models}
_EXTERNAL_API_BASE = None      # backward-compat: first reachable URL
_EXTERNAL_PROVIDERS_LOADED_TS = 0.0


def _read_external_providers():
    """Read [[external_providers]] from tylluan.toml.
    Returns list of provider dicts with resolved api_key from env vars.
    Only includes providers whose api_key_env is set (if required)."""
    import tomllib
    root = Path(__file__).resolve().parent.parent.parent
    config_path = root / "tylluan.toml"
    try:
        with open(config_path, "rb") as f:
            cfg = tomllib.load(f)
        ext = cfg.get("external_providers", [])
    except Exception:
        ext = []

    results = []
    for p in ext:
        ptype = p.get("type", "openai_compatible")
        name = p.get("name", "unnamed")
        base_url = p.get("base_url", "").rstrip("/")
        api_key_env = p.get("api_key_env", "")

        # API key from environment, NEVER from config file
        api_key = os.environ.get(api_key_env, "") if api_key_env else ""
        models = p.get("models", [])

        # Skip providers where the API key is required but missing
        if not api_key and ptype in ("openai_compatible", "anthropic_compatible"):
            continue

        results.append({
            "name": name,
            "type": ptype,
            "base_url": base_url,
            "api_key": api_key,
            "models": models,
        })
    return results


def _reload_external_providers():
    """Reload external provider config from tylluan.toml at most once per 60s."""
    global _EXTERNAL_PROVIDERS, _EXTERNAL_API_BASE, _EXTERNAL_PROVIDERS_LOADED_TS
    now = time.time()
    if now - _EXTERNAL_PROVIDERS_LOADED_TS < 60 and _EXTERNAL_PROVIDERS:
        return

    providers = _read_external_providers()
    _EXTERNAL_PROVIDERS = {p["name"]: p for p in providers}

    # Backward-compat: set _EXTERNAL_API_BASE to the first reachable provider
    for p in providers:
        if _is_backend_reachable(p["base_url"]):
            _EXTERNAL_API_BASE = p["base_url"]
            _EXTERNAL_PROVIDERS_LOADED_TS = now
            return

    # No reachable provider from config: fall back to legacy detection
    _EXTERNAL_API_BASE = None
    for env_var in ["OPENAI_BASE_URL", "LITELLM_API_BASE", "OLLAMA_HOST"]:
        val = os.environ.get(env_var)
        if val:
            normalized = _normalize_backend_url(val, is_ollama_host=(env_var == "OLLAMA_HOST"))
            if _is_backend_reachable(normalized):
                sys.stderr.write(f"[llama_backend] Using {env_var}={val} -> {normalized}\n")
                _EXTERNAL_API_BASE = normalized
                _EXTERNAL_PROVIDERS_LOADED_TS = now
                return
            sys.stderr.write(f"[llama_backend] {env_var}={val} set but not reachable at {normalized}, ignoring\n")

    # Auto-discovery on common ports (legacy)
    import urllib.request as _urllib
    for port in [11434, 1234, 8000]:
        try:
            url = f"http://127.0.0.1:{port}/api/tags" if port == 11434 else f"http://127.0.0.1:{port}/v1/models"
            req = _urllib.Request(url, method="GET")
            with _urllib.urlopen(req, timeout=2) as r:
                if r.status == 200:
                    _EXTERNAL_API_BASE = f"http://127.0.0.1:{port}/v1"
                    sys.stderr.write(f"[llama_backend] External backend detected on port {port}\n")
                    _EXTERNAL_PROVIDERS_LOADED_TS = now
                    return
        except Exception:
            pass
    _EXTERNAL_PROVIDERS_LOADED_TS = now


def _get_external_providers():
    """Return dict of {name: provider_info} for all configured external providers."""
    _reload_external_providers()
    return dict(_EXTERNAL_PROVIDERS)


def _get_provider_for_model(model_name):
    """Find the external provider that supports the given model name.
    Returns provider dict or None."""
    for p in _get_external_providers().values():
        if model_name in p.get("models", []):
            return p
    return None


def _call_external_provider(provider, prompt, max_tokens=256, temperature=0.7, grammar=""):
    """Call an external LLM provider with the given prompt.
    Handles both OpenAI-compatible and Anthropic-compatible endpoints."""
    import urllib.request as _urllib
    ptype = provider["type"]
    base = provider["base_url"].rstrip("/")
    api_key = provider["api_key"]
    model = provider["models"][0] if provider["models"] else "gpt-4o-mini"

    if ptype == "anthropic_compatible":
        url = f"{base}/v1/messages"
        body = {
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": prompt}],
        }
        data = json.dumps(body).encode("utf-8")
        req = _urllib.Request(
            url, data=data,
            headers={
                "Content-Type": "application/json",
                "x-api-key": api_key,
                "anthropic-version": "2023-06-01",
            },
            method="POST",
        )
        with _urllib.urlopen(req, timeout=120) as resp:
            result = json.loads(resp.read())
            # Anthropic response format: content[0].text
            return result["content"][0]["text"]

    elif ptype == "ollama_compatible":
        url = f"{base}/api/chat"
        body = {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": False,
        }
        data = json.dumps(body).encode("utf-8")
        req = _urllib.Request(
            url, data=data,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with _urllib.urlopen(req, timeout=120) as resp:
            result = json.loads(resp.read())
            return result["message"]["content"]

    else:
        # openai_compatible (default)
        url = f"{base}/v1/chat/completions"
        body = {
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": False,
        }
        if grammar:
            body["grammar"] = grammar
        data = json.dumps(body).encode("utf-8")
        headers = {"Content-Type": "application/json"}
        if api_key:
            headers["Authorization"] = f"Bearer {api_key}"
        req = _urllib.Request(url, data=data, headers=headers, method="POST")
        with _urllib.urlopen(req, timeout=120) as resp:
            result = json.loads(resp.read())
            return result["choices"][0]["message"]["content"]


def _normalize_backend_url(val, is_ollama_host=False):
    """Normalize an env-var-provided backend address into a usable API base URL.

    OLLAMA_HOST in particular is a bare `host:port` (Ollama's own listen-address
    format, e.g. "0.0.0.0:11434" to accept connections on all interfaces) --
    not a URL. Using it as-is breaks urllib with "unknown url type: 0.0.0.0".
    0.0.0.0/:: is a *bind* address, never a valid address to *connect to* --
    normalize it to 127.0.0.1 for the client side.
    """
    if "://" not in val:
        val = f"http://{val}"
    val = val.replace("://0.0.0.0", "://127.0.0.1").replace("://[::]", "://127.0.0.1")
    if is_ollama_host and not val.rstrip("/").endswith("/v1"):
        val = val.rstrip("/") + "/v1"
    return val


def _is_backend_reachable(base_url):
    """Real reachability probe -- an env var being *set* doesn't mean the
    backend is actually *running* there (found live: OLLAMA_HOST was set
    in the shell but Ollama wasn't started, causing a connection-refused
    crash instead of falling through to starting our own llama-server)."""
    import urllib.request as _urllib
    probe = base_url.rstrip("/") + "/models"
    try:
        with _urllib.urlopen(_urllib.Request(probe, method="GET"), timeout=2) as r:
            return r.status == 200
    except Exception:
        return False


def _detect_external_backend():
    """Check for external LLM backends. Returns API base URL or None.
    
    Priority order:
    1. [[external_providers]] from tylluan.toml (first reachable provider)
    2. Legacy env vars (OPENAI_BASE_URL, LITELLM_API_BASE, OLLAMA_HOST)
    3. Auto-discovery on common ports (11434, 1234, 8000)
    """
    _reload_external_providers()
    return _EXTERNAL_API_BASE


def _get_backend_url():
    """Return the backend URL: external if available, otherwise local llama-server."""
    external = _detect_external_backend()
    if external:
        return external
    return f"http://127.0.0.1:{LLAMA_PORT}/v1"

_llama_process = None
_model_loaded = False
_start_lock = asyncio.Lock()


def _find_llama_server():
    """Find llama-server binary. Returns path or None."""
    import shutil
    for name in ["llama-server", "llama-server.exe"]:
        found = shutil.which(name)
        if found:
            return found
    # Check Python Scripts directory
    scripts = Path(sys.executable).parent / "Scripts"
    for name in ["llama-server.exe", "llama-server"]:
        candidate = scripts / name
        if candidate.exists():
            return str(candidate)
    # Check our own cache directory (precompiled downloads)
    cache_dir = Path.home() / ".cache" / "tylluan" / "llama-cpp"
    for root, dirs, files in os.walk(cache_dir) if cache_dir.exists() else ():
        for f in files:
            if f in ("llama-server", "llama-server.exe"):
                return str(Path(root) / f)
    return None


def _install_llama_server():
    """Install llama-server binary.

    Three paths, in order:
    1. Precompiled CPU binary from GitHub Releases (17.5MB, no compilation)
    2. Precompiled CUDA binary (235MB) if NVIDIA GPU detected
    3. pip install llama-cpp-python (compiles from source, slow but works everywhere)
    """
    import platform
    import subprocess as sp
    import urllib.request as _urllib
    import zipfile
    import tempfile

    sys_name = platform.system().lower()
    machine = platform.machine().lower()

    # Detect GPU for CUDA path
    has_cuda = False
    try:
        sp.run(["nvidia-smi"], capture_output=True, timeout=5)
        has_cuda = True
    except Exception:
        pass

    # Build list of release assets to try
    assets = []
    if sys_name == "windows" and machine in ("amd64", "x86_64"):
        # CPU binary first (small, universal)
        assets.append(("llama-b10158-bin-win-cpu-x64.zip", "CPU (x64)"))
        if has_cuda:
            assets.append(("llama-b10158-bin-win-cuda-13.3-x64.zip", "CUDA 13.3 (x64)"))

    elif sys_name == "linux" and machine in ("x86_64", "amd64"):
        assets.append(("llama-b10158-bin-linux-x64.zip", "CPU (Linux x64)"))

    elif sys_name == "darwin" and machine == "arm64":
        assets.append(("llama-b10158-bin-macos-arm64.zip", "CPU (macOS ARM)"))

    for asset_name, label in assets:
        sys.stderr.write(f"[llama_backend] Trying precompiled binary: {label}...\n")
        try:
            url = f"https://github.com/ggerganov/llama.cpp/releases/download/b10158/{asset_name}"
            dest_dir = Path.home() / ".cache" / "tylluan" / "llama-cpp"
            dest_dir.mkdir(parents=True, exist_ok=True)

            zip_path = dest_dir / asset_name
            if not zip_path.exists():
                sys.stderr.write(f"[llama_backend] Downloading {asset_name}...\n")
                _urllib.urlretrieve(url, zip_path)

            # Extract the WHOLE zip, not just llama-server(.exe) -- the binary is a
            # thin launcher dynamically linked against ggml-*.dll/llama-*.dll shipped
            # in the same archive. Extracting only the .exe left it unable to load
            # its dependencies and exit silently with no stderr output (found live,
            # 2026-07-28: "llama-server exited early:" with an empty stderr capture).
            with zipfile.ZipFile(zip_path) as zf:
                zf.extractall(dest_dir)

            binary = None
            for name in ("llama-server.exe", "llama-server"):
                candidate = dest_dir / name
                if candidate.exists():
                    binary = candidate
                    break
            if binary is None:
                sys.stderr.write(f"[llama_backend] llama-server not found in {asset_name}\n")
                continue
            if not sys_name.startswith("win"):
                binary.chmod(0o755)
            sys.stderr.write(f"[llama_backend] Extracted: {binary}\n")
            return str(binary)
        except Exception as e:
            sys.stderr.write(f"[llama_backend] Precompiled download failed: {e}\n")

    # Fallback: pip install (compiles from source)
    sys.stderr.write("[llama_backend] Precompiled binary unavailable, installing via pip...\n")
    result = sp.run(
        [sys.executable, "-m", "pip", "install", "llama-cpp-python"],
        capture_output=True, text=True, timeout=600
    )
    if result.returncode != 0:
        raise RuntimeError(f"Failed to install llama-cpp-python: {result.stderr[-300:]}")

    # After pip install, search for the binary in Scripts/
    path = _find_llama_server()
    if path:
        return path

    # Last resort: search in site-packages for compiled binary
    import site
    for sp_dir in site.getsitepackages():
        for root, dirs, files in os.walk(sp_dir):
            for f in files:
                if f in ("llama-server", "llama-server.exe"):
                    return os.path.join(root, f)

    raise RuntimeError(
        "llama-server not found after all installation attempts. "
        "Please install llama.cpp manually: pip install llama-cpp-python"
    )


def _resolve_model_path():
    """Download GGUF model from HuggingFace hub if not cached. Returns path."""
    from huggingface_hub import hf_hub_download
    sys.stderr.write(f"[llama_backend] Resolving model: {DEFAULT_MODEL} :: {DEFAULT_MODEL_FILE}\n")
    path = hf_hub_download(
        repo_id=DEFAULT_MODEL,
        filename=DEFAULT_MODEL_FILE,
        cache_dir=str(Path.home() / ".cache" / "huggingface" / "hub"),
    )
    sys.stderr.write(f"[llama_backend] Model ready: {path}\n")
    return path


def _is_port_open(port):
    import socket
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        s.bind(("127.0.0.1", port))
        s.close()
        return True
    except OSError:
        s.close()
        return False


def _find_free_port(preferred, max_tries=20):
    """Find a free TCP port, starting from `preferred` and scanning upward.
    Never binds to steal a port already in use -- only used to pick an
    alternative when the preferred one is taken by something else."""
    for offset in range(max_tries):
        candidate = preferred + offset
        if _is_port_open(candidate):
            return candidate
    # Extremely unlikely (20 consecutive ports all occupied); let the OS
    # pick any free ephemeral port rather than give up.
    import socket
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _is_http_ready(port):
    """Real HTTP readiness probe -- the port being occupied only means
    llama-server has bound the socket, not that it finished loading the
    model and is actually serving requests. Found live, reproduced 3
    times: query_model got a real HTTP 503 right after the port-bound
    check passed, because the model was still loading."""
    import urllib.request as _urllib
    try:
        with _urllib.urlopen(f"http://127.0.0.1:{port}/v1/models", timeout=1) as r:
            return r.status == 200
    except Exception:
        return False


async def _start_llama_server():
    """Start llama-server subprocess on LLAMA_PORT. Idempotent.

    Guarded by _start_lock: concurrent query_model calls before the server
    is up would otherwise race past the `_model_loaded` check (it's only
    set True at the very end) and each try to spawn their own subprocess.
    """
    async with _start_lock:
        await _start_llama_server_locked()


async def _start_llama_server_locked():
    global _llama_process, _model_loaded
    if _model_loaded:
        return
    if _llama_process is not None and _llama_process.poll() is None:
        _model_loaded = True
        sys.stderr.write("[llama_backend] llama-server already running\n")
        return

    server_path = _find_llama_server()
    if not server_path:
        # Blocking pip install (up to 300s) -- run off the event loop thread so
        # other tool calls to this guild (e.g. backend_health) don't hang too.
        # Found live: a first query_model call blocked the whole guild process
        # for minutes, timing out every other call including health checks.
        server_path = await asyncio.to_thread(_install_llama_server)

    model_path = await asyncio.to_thread(_resolve_model_path)

    if not _is_port_open(LLAMA_PORT):
        # Something is already bound to our configured port. It might be a
        # llama-server we started in a previous run (fine, reuse it) -- or it
        # might be a completely unrelated process (LM Studio, ComfyUI, some
        # dev server the user happened to have on 9000). Blindly assuming
        # "already loaded" made Tylluan silently talk to the wrong service
        # instead of isolating itself, which is exactly the kind of
        # interference this guild must never cause. Verify compatibility
        # first; if it's not one of ours, find a free port instead of
        # fighting over the occupied one.
        if await asyncio.to_thread(_is_http_ready, LLAMA_PORT):
            sys.stderr.write(f"[llama_backend] Port {LLAMA_PORT} in use by a compatible server, reusing it\n")
            _model_loaded = True
            return
        new_port = _find_free_port(LLAMA_PORT)
        sys.stderr.write(
            f"[llama_backend] Port {LLAMA_PORT} is occupied by an unrelated process "
            f"(not an OpenAI-compatible server) -- switching to {new_port} instead of "
            f"interfering with it\n"
        )
        globals()["LLAMA_PORT"] = new_port

    threads = _get_config()["threads"] if _get_config()["threads"] > 0 else (os.cpu_count() or 4)
    cmd = [
        server_path,
        "--model", model_path,
        "--host", "127.0.0.1",
        "--port", str(LLAMA_PORT),
        "--n-gpu-layers", str(_get_config()["n_gpu_layers"]),
        "--ctx-size", str(_get_config()["ctx_size"]),
    ]
    # --n-keep: llama.cpp keeps the first N prompt tokens cached across context
    # shifts (multi-turn / long generations). Default 0 keeps nothing; a positive
    # value preserves a stable prefix (e.g. system prompt) so it isn't recomputed.
    if _get_config()["n_keep"] > 0:
        cmd.append("--n-keep")
        cmd.append(str(_get_config()["n_keep"]))
    cmd.extend([
        "--threads", str(threads),
        "--batch-size", str(_get_config()["batch_size"]),
    ])
    # --mlock on ARM or low-RAM devices prevents model swap.
    # The doctor-in-Africa anchor (Raspberry Pi 4, 4-8GB RAM) benefits most.
    # On high-RAM systems the flag is cheap and harmless.
    if platform := __import__("platform"):
        is_arm = platform.machine().lower() in ("armv7l", "aarch64", "arm64")
        if is_arm or _get_config()["n_gpu_layers"] == 0:
            cmd.append("--mlock")

    sys.stderr.write(f"[llama_backend] Starting: {' '.join(cmd)}\n")
    _llama_process = subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )

    for _ in range(30):
        await asyncio.sleep(0.5)
        if not _is_port_open(LLAMA_PORT) and await asyncio.to_thread(_is_http_ready, LLAMA_PORT):
            _model_loaded = True
            sys.stderr.write("[llama_backend] llama-server ready\n")
            # Drain stderr continuously from here on -- a PIPE that's read only
            # once (on crash) fills up once the server is running long enough
            # to log past the OS pipe buffer, which blocks the child process
            # on its next write() and silently hangs llama-server.
            asyncio.create_task(_drain_process_stderr(_llama_process))
            return
        if _llama_process.poll() is not None:
            stderr = _llama_process.stderr.read() if _llama_process.stderr else ""
            raise RuntimeError(f"llama-server exited early: {stderr[:200]}")

    raise RuntimeError("llama-server did not start within 15s")


async def _drain_process_stderr(proc):
    """Continuously read and discard a running llama-server's stderr so its
    OS pipe buffer never fills up and blocks the child on write()."""
    if proc.stderr is None:
        return
    try:
        while True:
            line = await asyncio.to_thread(proc.stderr.readline)
            if not line:
                break
    except Exception:
        pass


async def _stop_llama_server():
    global _llama_process, _model_loaded
    if _llama_process is not None:
        sys.stderr.write("[llama_backend] Stopping llama-server...\n")
        if sys.platform == "win32":
            _llama_process.terminate()
        else:
            _llama_process.send_signal(signal.SIGTERM)
        try:
            _llama_process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            _llama_process.kill()
        _llama_process = None
    _model_loaded = False


@mcp.tool()
async def query_model(prompt: str, model: str = "", max_tokens: int = 256, temperature: float | None = None, grammar: str = "") -> str:
    """Query the LLM backend with a prompt. Returns generated text.

    Routes to an external provider if one matches the requested model,
    otherwise falls back to the local llama-server.

    Args:
        prompt: The prompt to send to the model.
        model: Optional model name. If set, routes to an external provider
            that lists this model. If empty, uses the first available backend.
        max_tokens: Maximum tokens to generate (default 256).
        temperature: Sampling temperature. Defaults to tylluan.toml's
            [inference.llama].temperature when not given.
        grammar: Optional GBNF grammar string to constrain output
            (only works with local llama-server, ignored for external providers).
    """
    import urllib.request as _urllib

    # Try routing to a configured external provider by model name
    if model:
        provider = _get_provider_for_model(model)
        if provider:
            t = temperature if temperature is not None else _get_config()["temperature"]
            return await asyncio.to_thread(
                _call_external_provider, provider, prompt, max_tokens, t, grammar
            )

    # Fall back to legacy backend URL
    backend_url = _get_backend_url()
    is_external = _detect_external_backend() is not None

    if not is_external:
        await _start_llama_server()

    t = temperature if temperature is not None else _get_config()["temperature"]

    request_body = {
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": t,
        "top_p": _get_config()["top_p"],
        "top_k": _get_config()["top_k"],
        "repeat_penalty": _get_config()["repeat_penalty"],
        "stream": False,
    }
    if grammar:
        request_body["grammar"] = grammar

    data = json.dumps(request_body).encode("utf-8")

    req = _urllib.Request(
        f"{backend_url}/chat/completions",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with _urllib.urlopen(req, timeout=120) as resp:
        result = json.loads(resp.read())
        return result["choices"][0]["message"]["content"]


@mcp.tool()
async def backend_health() -> str:
    """Check backend status: local llama-server + all configured external providers."""
    status = "running" if (_model_loaded and _llama_process is not None
                          and _llama_process.poll() is None) else "stopped"

    ext_providers = _get_external_providers()
    return json.dumps({
        "status": status,
        "model": f"{DEFAULT_MODEL}::{DEFAULT_MODEL_FILE}",
        "port": LLAMA_PORT,
        "backend": "llama.cpp",
        "external_backend": _detect_external_backend(),
        "external_providers": [
            {
                "name": name,
                "type": info["type"],
                "base_url": info["base_url"],
                "models": info["models"],
                "has_key": bool(info["api_key"]),
            }
            for name, info in ext_providers.items()
        ],
        "params": {
            "n_gpu_layers": _get_config()["n_gpu_layers"],
            "ctx_size": _get_config()["ctx_size"],
            "n_keep": _get_config()["n_keep"],
            "threads": _get_config()["threads"],
            "batch_size": _get_config()["batch_size"],
        },
    })


@mcp.tool()
async def list_models() -> str:
    """List cached GGUF models in the HuggingFace hub cache."""
    import glob as _glob
    cache_dir = Path.home() / ".cache" / "huggingface" / "hub"
    models = []
    # Only scan 3 levels deep: repos/models--org--name/snapshots/HASH/*.gguf
    for gguf in _glob.glob(str(cache_dir / "models--*" / "snapshots" / "*" / "*.gguf")):
        path = Path(gguf)
        size_mb = path.stat().st_size / (1024 * 1024)
        # Extract repo name from path: models--org--name -> org/name
        parts = path.parts
        try:
            snap_idx = parts.index("snapshots")
            repo_dir = parts[snap_idx - 1]
            repo = repo_dir.replace("models--", "").replace("--", "/")
        except ValueError:
            repo = "unknown"
        models.append({
            "repo": repo,
            "file": path.name,
            "size_mb": round(size_mb, 1),
            "path": str(gguf),
        })
    return json.dumps({"models": sorted(models, key=lambda m: m["size_mb"])})


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
