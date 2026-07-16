"""
TylluanNexus Bash Guild — Secure shell command execution.

This guild provides the `bash_execute` tool, which runs shell commands
on the local system with configurable safety controls.

Security:
    - Strict command allowlist (only known-safe binaries)
    - shlex.split() to extract the first token
    - cwd validated against project root via validate_path()
    - Configurable timeout (default: 30s)
    - Output truncation to prevent context overflow
"""

import asyncio
import os
import re
import shlex
import sys

from mcp.server.fastmcp import FastMCP

from guilds.builders.plugins import utils

mcp = FastMCP("tylluan-bash")

# --- Agentic Lifecycle Management ---
STATE_FILE = os.path.join("data", "checkpoints", "bash.json")

@mcp.tool()
async def state_checkpoint(reason: str = "manual") -> str:
    """Save the current guild state to disk for non-disruptive hot-reloading.
    
    Args:
        reason: Why the checkpoint is being taken (e.g. 'update', 'shutdown').
    """
    try:
        os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
        state = {
            "last_cwd": os.getcwd(),
            "pid": os.getpid(),
            "reason": reason,
            "version": "1.0.0"
        }
        with open(STATE_FILE, "w", encoding="utf-8") as f:
            import json
            json.dump(state, f, indent=2)
        return f"✅ Checkpoint saved successfully: {STATE_FILE}"
    except Exception as e:
        return f"❌ Checkpoint failed: {e}"

@mcp.tool()
async def state_restore() -> str:
    """Restore the guild state from the last saved checkpoint."""
    try:
        if not os.path.exists(STATE_FILE):
            return "ℹ️ No checkpoint found to restore."
        
        with open(STATE_FILE, "r", encoding="utf-8") as f:
            import json
            state = json.load(f)
        
        target_cwd = state.get("last_cwd")
        if target_cwd and os.path.isdir(target_cwd):
            os.chdir(target_cwd)
            return f"✅ State restored. CWD: {target_cwd} (Reason: {state.get('reason')})"
        return "⚠️ Checkpoint found but CWD is invalid or missing."
    except Exception as e:
        return f"❌ Restoration failed: {e}"

# ------------------------------------

# Strict allowlist of known-safe binaries. Any command whose first token
# is not in this set is blocked. Update this list as project needs evolve.
ALLOWED_COMMANDS: frozenset[str] = frozenset({
    # Version control
    "git",
    # Rust toolchain
    "cargo", "rustc", "rustup",
    # Python
    "python", "python3", "uv", "pip",
    # Node.js
    "node", "npm", "pnpm", "npx", "tsx",
    # File inspection (read-only)
    "ls", "cat", "head", "tail", "wc", "find", "grep", "rg", "diff",
    "file", "which", "where", "stat", "du", "df", "tree", "sort",
    "uniq", "cut", "tr", "echo", "printf",
    # Build tools
    "make", "cmake", "ninja",
    # Shell builtins passed as -c argument are checked by the parser below
    # NOTA: "bash" and "powershell" are NOT in the allowlist — use the
    # tool's native execution mode (powershell on win32, bash on posix).
})

# On Windows, also allow cmdlets invoked via powershell -Command
ALLOWED_PWSH_CMDLETS: frozenset[str] = frozenset({
    "Get-ChildItem", "Get-Content", "Set-Content", "Select-String",
    "Test-Path", "Get-Item", "Remove-Item", "New-Item", "Copy-Item",
    "Move-Item", "Write-Output", "Write-Host",
})

MAX_OUTPUT_CHARS = 50_000  # Truncate output to avoid context explosion


def _first_token(command: str) -> str | None:
    """Extract the first token from a command string using shlex.split()."""
    try:
        parts = shlex.split(command)
        return parts[0] if parts else None
    except ValueError:
        return None


def _check_allowlist(command: str) -> str | None:
    """Check if command is allowed. Returns None if OK, error string if blocked."""
    parts = shlex.split(command)
    if not parts:
        return "❌ Empty command"
    first = parts[0]

    # On Windows, powershell cmdlets are called as "powershell -Command <cmdlet>"
    if sys.platform == "win32" and first.lower() == "powershell":
        for i, part in enumerate(parts):
            if part.lower() in ("-command", "-c") and i + 1 < len(parts):
                cmdlet = parts[i + 1].split()[0] if parts[i + 1].split() else ""
                if cmdlet and cmdlet not in ALLOWED_PWSH_CMDLETS:
                    return (
                        f"🚫 BLOCKED: '{cmdlet}' is not in the allowed PowerShell cmdlet list. "
                        f"Allowed: {', '.join(sorted(ALLOWED_PWSH_CMDLETS))}"
                    )
                return None  # -c with an allowed cmdlet — OK
        return None  # powershell with flags but no -c — OK

    if first not in ALLOWED_COMMANDS:
        return (
            f"🚫 BLOCKED: '{first}' is not in the allowed command list. "
            f"Allowed binaries: {', '.join(sorted(ALLOWED_COMMANDS))}"
        )
    return None


@mcp.tool()
async def bash_execute(
    command: str = "",
    cwd: str | None = None,
    timeout_secs: int = 30,
    intent: str = "",
) -> str:
    """Execute a shell command and return stdout + stderr.

    SECURITY: Only commands whose first token is in the allowlist are executed.
    cwd is validated to be within the project root.
    This is NOT a general-purpose shell — use the allowlist for safety.

    Use for: run command, execute command, bash, shell, run script, run cargo,
    cargo test, cargo build, run python, run npm, run git, ejecutar comando, correr script.

    Args:
        command: The shell command to execute.
        cwd: Working directory (defaults to current directory).
        timeout_secs: Maximum execution time in seconds (default: 30).
        intent: Natural language intent for fallback command extraction.

    Returns:
        Combined stdout and stderr output, truncated if too long.
    """
    # Extract actual shell command from natural-language intent
    cmd_source = command or intent
    if cmd_source:
        action_prefixes = [
            "run command: ", "execute command: ", "run the command ",
            "execute the command ", "run ", "execute ", "ejecutar ", "correr ",
        ]
        for prefix in action_prefixes:
            if cmd_source.lower().startswith(prefix):
                cmd_source = cmd_source[len(prefix):]
                break
        # Extract "in [the] <dir> [directory]" suffix → cwd
        dir_match = re.search(
            r'\s+in(?:\s+the)?\s+([\w/\\:.-]+)(?:\s+directory)?\s*$',
            cmd_source, re.IGNORECASE
        )
        if dir_match:
            cwd = dir_match.group(1).strip()
            cmd_source = cmd_source[:dir_match.start()].strip()
        command = cmd_source

    if not command:
        return "❌ No command provided. Specify a shell command to execute."

    # Security step 1: allowlist check on first token
    block_reason = _check_allowlist(command)
    if block_reason:
        return block_reason

    # Security step 2: validate cwd against project root
    kernel_root = os.environ.get("TYLLUAN_ROOT", os.getcwd())
    work_dir = cwd or kernel_root
    if not utils.validate_path(kernel_root, work_dir):
        return f"🚫 BLOCKED: Working directory '{work_dir}' is outside the allowed project root '{kernel_root}'."
    if not os.path.isdir(work_dir):
        return f"❌ Error: Directory does not exist: {work_dir}"

    try:
        # Determine shell based on platform
        if sys.platform == "win32":
            shell_cmd = ["powershell", "-NoProfile", "-Command", command]
        else:
            shell_cmd = ["bash", "-c", command]
        
        # Run command with timeout
        returncode, stdout, stderr = await utils.run_command(
            shell_cmd,
            cwd=work_dir,
            timeout_secs=timeout_secs,
        )
        
        output = stdout
        if stderr:
            output += "\n--- stderr ---\n" + stderr
        
        # Truncate if too long
        output = utils.truncate_output(output, MAX_OUTPUT_CHARS)
        
        exit_info = f"\n\n📋 Exit code: {returncode}"
        return output + exit_info

    except asyncio.TimeoutError:
        return f"⏰ Command timed out after {timeout_secs} seconds and was killed."
    except Exception as e:
        return f"❌ Execution error: {e}"


if __name__ == "__main__":
    # Sovereign Auto-Restore: Attempt to recover last known state before handshake
    try:
        if os.path.exists(STATE_FILE):
            import json
            with open(STATE_FILE, "r", encoding="utf-8") as f:
                state = json.load(f)
            cwd = state.get("last_cwd")
            if cwd and os.path.isdir(cwd):
                os.chdir(cwd)
    except Exception:
        pass
        
    utils.safe_mcp_run(mcp)
