"""
TylluanNexus Sandbox Guild — Ephemeral Docker execution.

Executes commands and scripts in fully isolated containers. Each invocation
creates a fresh container, runs the code, captures all output, and destroys
the container — regardless of outcome (success, error, or timeout).

Isolation guarantees per container:
  --network none          No internet or LAN access
  --memory 512m           Hard RAM limit
  --cpus 0.5              CPU cap
  --read-only             Read-only root filesystem
  --tmpfs /tmp:size=64m   Writable scratch space (in-memory only)
  --no-new-privileges     Block privilege escalation
  --rm                    Auto-remove on exit (belt)
  docker rm -f            Force-remove in finally block (suspenders)
"""

import asyncio
import sys
import uuid
from mcp.server.fastmcp import FastMCP
from guilds.core import utils

mcp = FastMCP("tylluan-sandbox")

MAX_TIMEOUT_SECS = 120
MAX_SCRIPT_BYTES = 65_536   # 64 KB
MAX_OUTPUT_CHARS = 20_000

_ISOLATION_FLAGS = [
    "--network", "none",
    "--memory", "512m",
    "--cpus", "0.5",
    "--read-only",
    "--tmpfs", "/tmp:size=64m",
    "--no-new-privileges",
]


async def _docker_available() -> bool:
    rc, _, _ = await utils.run_command(["docker", "info"], timeout_secs=8)
    return rc == 0


async def _run_in_sandbox(
    command_args: list[str],
    image: str,
    stdin_data: bytes | None,
    timeout_secs: int,
    container_name: str,
) -> dict:
    """Core executor. Always cleans up the container."""
    cmd = [
        "docker", "run",
        *([ "-i"] if stdin_data else []),
        "--rm",
        "--name", container_name,
        "--stop-timeout", str(timeout_secs),
        *_ISOLATION_FLAGS,
        image,
        *command_args,
    ]

    try:
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE if stdin_data else None,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        stdout, stderr = await asyncio.wait_for(
            process.communicate(input=stdin_data),
            timeout=timeout_secs + 5,   # +5s grace over docker's own timeout
        )

        return {
            "exit_code": process.returncode,
            "stdout": stdout.decode(errors="replace") if stdout else "",
            "stderr": stderr.decode(errors="replace") if stderr else "",
            "timed_out": False,
        }

    except asyncio.TimeoutError:
        # Subprocess wrapper timed out — force-kill the container immediately
        await utils.run_command(["docker", "kill", container_name], timeout_secs=5)
        return {
            "exit_code": -1,
            "stdout": "",
            "stderr": f"Sandbox killed: exceeded {timeout_secs}s hard timeout.",
            "timed_out": True,
        }

    except FileNotFoundError:
        return {
            "exit_code": -1,
            "stdout": "",
            "stderr": "docker command not found — is Docker Desktop running?",
            "timed_out": False,
        }

    finally:
        # Belt-and-suspenders: remove even if --rm didn't fire (e.g. docker kill above)
        await utils.run_command(
            ["docker", "rm", "-f", container_name],
            timeout_secs=5,
        )


def _format_result(result: dict, image: str, container_name: str) -> str:
    parts = [
        f"Sandbox : {container_name}",
        f"Image   : {image}",
        f"Exit    : {result['exit_code']}" + ("  [TIMEOUT]" if result["timed_out"] else ""),
    ]

    stdout = result["stdout"].strip()
    stderr = result["stderr"].strip()

    if stdout:
        parts += ["", "── stdout ──", utils.truncate_output(stdout, MAX_OUTPUT_CHARS)]
    if stderr:
        parts += ["", "── stderr ──", utils.truncate_output(stderr, MAX_OUTPUT_CHARS // 2)]
    if not stdout and not stderr:
        parts.append("(no output)")

    return "\n".join(parts)


# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
async def sandbox_run(
    command: str,
    image: str = "python:3.11-slim",
    timeout_secs: int = 30,
) -> str:
    """
    Run a shell command in an ephemeral isolated Docker container.

    The container starts from scratch, executes the command, and is
    destroyed immediately — regardless of outcome. No network access,
    no persistence, no privilege escalation.

    Args:
        command: Shell command string (e.g. "ls /tmp", "python --version")
        image: Docker image name. Must exist locally or be pullable.
               Default: python:3.11-slim. Use 'alpine' for lightweight shell tasks.
        timeout_secs: Hard time limit 1–120 seconds (default 30).
    """
    if not command.strip():
        return "❌ command must not be empty."

    timeout_secs = max(1, min(timeout_secs, MAX_TIMEOUT_SECS))
    container_name = f"tylluan-sandbox-{uuid.uuid4().hex[:8]}"

    result = await _run_in_sandbox(
        command_args=["sh", "-c", command],
        image=image,
        stdin_data=None,
        timeout_secs=timeout_secs,
        container_name=container_name,
    )
    return _format_result(result, image, container_name)


@mcp.tool()
async def sandbox_python(
    script: str = "",
    image: str = "python:3.11-slim",
    timeout_secs: int = 30,
    intent: str = "",
    query: str = "",
) -> str:
    """
    Execute a Python script in an ephemeral isolated Docker container.

    The script is piped via stdin — nothing is written to host disk.
    Container is fully isolated and destroyed after execution.

    Args:
        script: Python source code to execute.
        image: Docker image with Python. Default: python:3.11-slim.
        timeout_secs: Hard time limit 1–120 seconds (default 30).
    """
    if not script:
        script = intent or query or ""
    if not script.strip():
        return "❌ No script provided. Include the Python code to execute."

    script_bytes = script.encode()
    if len(script_bytes) > MAX_SCRIPT_BYTES:
        return f"❌ Script too large ({len(script_bytes)} bytes, max {MAX_SCRIPT_BYTES})."

    timeout_secs = max(1, min(timeout_secs, MAX_TIMEOUT_SECS))
    container_name = f"tylluan-sandbox-{uuid.uuid4().hex[:8]}"

    result = await _run_in_sandbox(
        command_args=["python", "-"],
        image=image,
        stdin_data=script_bytes,
        timeout_secs=timeout_secs,
        container_name=container_name,
    )
    return _format_result(result, image, container_name)


@mcp.tool()
async def sandbox_status() -> str:
    """
    Check sandbox infrastructure availability.

    Verifies Docker is reachable and lists any residual tylluan-sandbox
    containers left over from crashed runs (should normally be zero).
    """
    if not await _docker_available():
        return "❌ Docker is not available. Start Docker Desktop and retry."

    rc, stdout, _ = await utils.run_command(
        [
            "docker", "ps", "-a",
            "--filter", "name=tylluan-sandbox",
            "--format", "table {{.Names}}\t{{.Status}}\t{{.Image}}",
        ],
        timeout_secs=10,
    )

    lines = ["✅ Docker sandbox infrastructure ready."]

    residual = stdout.strip() if rc == 0 else ""
    # Remove header-only response
    if residual and residual != "NAMES\tSTATUS\tIMAGE":
        lines += [
            "",
            "⚠️  Residual sandbox containers (from crashed runs):",
            residual,
            "",
            "Clean up: docker rm -f $(docker ps -aq --filter name=tylluan-sandbox)",
        ]
    else:
        lines.append("No residual containers. System clean.")

    return "\n".join(lines)


@mcp.tool()
async def sandbox_ingest(
    repo_path: str = "", path: str = "", directory: str = "", intent: str = ""
) -> str:
    """
    Analyze a locally-cloned repository to detect its guild type.

    Mounts the repo read-only inside an ephemeral container with no network
    access and inspects key manifest files to determine if the repo is a
    FastMCP Python guild, a Node MCP server, or an unknown type.

    This tool does NOT perform the git clone — the caller must clone the repo
    to a local path first. Only the analysis runs inside the sandbox.

    Args:
        repo_path: Absolute path to the cloned repository on the host.

    Returns:
        JSON string with keys:
          - guild_type: "fastmcp-python" | "node-mcp" | "generic-mcp" | "unknown"
          - entry_point: detected main file or module (empty if unknown)
          - evidence: list of files/keys that led to the verdict
          - error: non-empty if analysis failed
    """
    import json as _json
    from pathlib import Path as _Path

    if not repo_path:
        repo_path = path or directory or intent or ""
    repo = _Path(repo_path).resolve()
    if not repo.is_dir():
        return _json.dumps({
            "guild_type": "unknown",
            "entry_point": "",
            "evidence": [],
            "error": f"Path does not exist or is not a directory: {repo_path}",
        })

    if not await _docker_available():
        return _json.dumps({
            "guild_type": "unknown",
            "entry_point": "",
            "evidence": [],
            "error": "Docker is not available. Start Docker Desktop and retry.",
        })

    # Analysis script runs inside the container with the repo mounted read-only.
    # It outputs a single JSON line to stdout.
    _ANALYSIS_SCRIPT = r"""
import json, sys
from pathlib import Path

repo = Path("/workspace")
result = {"guild_type": "unknown", "entry_point": "", "evidence": [], "error": ""}

# --- FastMCP Python detection ---
for toml_name in ["pyproject.toml", "setup.cfg", "setup.py"]:
    p = repo / toml_name
    if p.exists():
        content = p.read_text(errors="replace")
        if "fastmcp" in content.lower() or "mcp" in content.lower():
            # Find likely entry point
            entry = ""
            for candidate in ["main.py", "server.py", "guild.py", "__main__.py"]:
                if (repo / candidate).exists():
                    entry = candidate.replace(".py", "")
                    break
            if not entry:
                # Try src/<name>/main.py pattern
                src = repo / "src"
                if src.is_dir():
                    for d in src.iterdir():
                        if d.is_dir() and (d / "main.py").exists():
                            entry = f"src.{d.name}.main"
                            break
            result["guild_type"] = "fastmcp-python"
            result["entry_point"] = entry or "main"
            result["evidence"].append(f"{toml_name}: contains 'mcp'")
            break

# --- Node MCP detection ---
if result["guild_type"] == "unknown":
    pkg = repo / "package.json"
    if pkg.exists():
        try:
            data = json.loads(pkg.read_text())
            deps = {**data.get("dependencies", {}), **data.get("devDependencies", {})}
            if any("modelcontextprotocol" in k or k == "@anthropic/mcp" for k in deps):
                main = data.get("main", "index.js")
                result["guild_type"] = "node-mcp"
                result["entry_point"] = main
                result["evidence"].append(f"package.json: MCP dep found ({list(deps.keys())[:3]})")
        except Exception as e:
            result["evidence"].append(f"package.json parse error: {e}")

# --- Generic MCP config detection ---
if result["guild_type"] == "unknown":
    for cfg in ["mcp.json", "mcp.toml", ".mcp.json"]:
        if (repo / cfg).exists():
            result["guild_type"] = "generic-mcp"
            result["entry_point"] = ""
            result["evidence"].append(f"found {cfg}")
            break

print(json.dumps(result))
"""

    container_name = f"tylluan-sandbox-ingest-{uuid.uuid4().hex[:8]}"
    cmd = [
        "docker", "run",
        "--rm",
        "--name", container_name,
        "--network", "none",
        "--memory", "256m",
        "--cpus", "0.5",
        "--read-only",
        "--tmpfs", "/tmp:size=32m",
        "--no-new-privileges",
        "-v", f"{repo}:/workspace:ro",  # mount repo read-only
        "-i",
        "python:3.11-slim",
        "python", "-",
    ]

    try:
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await asyncio.wait_for(
            process.communicate(input=_ANALYSIS_SCRIPT.encode()),
            timeout=45,
        )

        raw = stdout.decode(errors="replace").strip()
        # Extract last JSON line (ignore any stray prints)
        for line in reversed(raw.splitlines()):
            line = line.strip()
            if line.startswith("{"):
                try:
                    verdict = _json.loads(line)
                    verdict["error"] = verdict.get("error", "")
                    return _json.dumps(verdict, indent=2)
                except _json.JSONDecodeError:
                    pass

        # Fallback: return raw output as error
        return _json.dumps({
            "guild_type": "unknown",
            "entry_point": "",
            "evidence": [],
            "error": f"Analysis script produced unexpected output: {raw[:500]}",
        })

    except asyncio.TimeoutError:
        await utils.run_command(["docker", "kill", container_name], timeout_secs=5)
        return _json.dumps({
            "guild_type": "unknown",
            "entry_point": "",
            "evidence": [],
            "error": "Analysis timed out (45s).",
        })
    except FileNotFoundError:
        return _json.dumps({
            "guild_type": "unknown",
            "entry_point": "",
            "evidence": [],
            "error": "docker command not found — is Docker Desktop running?",
        })
    finally:
        await utils.run_command(["docker", "rm", "-f", container_name], timeout_secs=5)


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
