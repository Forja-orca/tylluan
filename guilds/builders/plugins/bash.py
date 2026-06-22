"""
TylluanNexus Bash Guild — Secure shell command execution.

This guild provides the `bash_execute` tool, which runs shell commands
on the local system with configurable safety controls.

Security:
    - Command pattern blocking (rm -rf /, format, etc.)
    - Configurable timeout (default: 30s)
    - Output truncation to prevent context overflow
"""

import asyncio
import os
import re
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

# Patterns that are always blocked for safety
BLOCKED_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"rm\s+(-rf?|--recursive)\s+/\s*$", re.IGNORECASE),
    re.compile(r"mkfs\.", re.IGNORECASE),
    re.compile(r"dd\s+if=.*of=/dev/", re.IGNORECASE),
    re.compile(r"format\s+[a-zA-Z]:", re.IGNORECASE),
    re.compile(r":(){ :\|:& };:", re.IGNORECASE),  # fork bomb
]

MAX_OUTPUT_CHARS = 50_000  # Truncate output to avoid context explosion


@mcp.tool()
async def bash_execute(
    command: str = "",
    cwd: str | None = None,
    timeout_secs: int = 30,
    intent: str = "",
) -> str:
    """Execute a shell command and return stdout + stderr. [approval="always"]

    Use for: run command, execute command, bash, shell, run script, run cargo,
    run python, run npm, run git, ejecutar comando, correr script.

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

    # Security: block dangerous patterns
    for pattern in BLOCKED_PATTERNS:
        if pattern.search(command):
            return f"🚫 BLOCKED: Command matches a dangerous pattern and was not executed.\nPattern: {pattern.pattern}"

    # Resolve working directory
    work_dir = cwd or os.getcwd()
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
