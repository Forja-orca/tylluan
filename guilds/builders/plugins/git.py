"""
TylluanNexus Git Guild — Fast access to repository operations.

This guild provides tools to interact with Git repositories locally,
allowing agents to check state, view history, and commit changes.
"""

import asyncio
import os

from mcp.server.fastmcp import FastMCP

from guilds.builders.plugins import utils

mcp = FastMCP("tylluan-git")

MAX_OUTPUT_CHARS = 50_000


async def _run_git(args: list[str], cwd: str | None = None, use_fallback: bool = False) -> str:
    """Helper to run git commands in the background."""
    work_dir = cwd or os.getcwd()

    # Disable fsmonitor, hooks and credential helpers — speeds up AV-scanned environments
    cmd = [
        "git",
        "-c", "core.fsmonitor=false",
        "-c", "core.untrackedcache=false",
        "-c", "core.hooksPath=/dev/null",
        "-c", "credential.helper=",
    ] + args

    env = os.environ.copy()
    env["GIT_TERMINAL_PROMPT"] = "0"
    env["GIT_ASK_YESNO"] = "false"
    env["GCM_INTERACTIVE"] = "never"
    env["GIT_OPTIONAL_LOCKS"] = "0"
    # Always skip system config — avoids AV scanning global git config
    env["GIT_CONFIG_NOSYSTEM"] = "1"

    if use_fallback:
        env["GIT_CEILING_DIRECTORIES"] = work_dir

    try:
        process = await asyncio.create_subprocess_exec(
            *cmd,
            cwd=work_dir,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
        )

        stdout, stderr = await asyncio.wait_for(
            process.communicate(),
            timeout=45,  # AV on Windows can delay first subprocess spawn 15-30s
        )

        output = ""
        if stdout:
            output += stdout.decode(errors="replace")
        if stderr:
            output += stderr.decode(errors="replace")

        if len(output) > MAX_OUTPUT_CHARS:
            output = output[:MAX_OUTPUT_CHARS] + "\n\n⚠️ Output truncated..."

        return output.strip() if output else "✅ (No output)"

    except FileNotFoundError:
        return "❌ Error: 'git' command not found. Ensure Git is installed and in PATH."
    except TimeoutError:
        if not use_fallback:
            # First retry with stripped env (AV sometimes blocks on specific env vars)
            result = await _run_git(args, cwd, use_fallback=True)
            if not result.startswith("⏱️"):
                return result
            # Final fallback: shell=True bypasses AV child-process interception
            return await _run_git_shell(args, cwd)
        return await _run_git_shell(args, cwd)
    except Exception as e:
        return f"❌ Error: {e}"


async def _run_git_shell(args: list[str], cwd: str | None = None) -> str:
    """Fallback using shell=True to avoid AV intercepting child process."""
    work_dir = cwd or os.getcwd()

    cmd_str = f"git -c core.fsmonitor=false -c core.untrackedcache=false {args[0]} --porcelain=v2 --branch --untracked-files=no --no-ahead-behind"

    env = os.environ.copy()
    env["GIT_TERMINAL_PROMPT"] = "0"
    env["GIT_ASK_YESNO"] = "false"
    env["GCM_INTERACTIVE"] = "never"
    env["GIT_OPTIONAL_LOCKS"] = "0"

    try:
        process = await asyncio.create_subprocess_shell(
            cmd_str,
            cwd=work_dir,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
        )

        stdout, stderr = await asyncio.wait_for(
            process.communicate(),
            timeout=8,
        )

        output = ""
        if stdout:
            output += stdout.decode(errors="replace")
        if stderr:
            output += stderr.decode(errors="replace")

        if len(output) > MAX_OUTPUT_CHARS:
            output = output[:MAX_OUTPUT_CHARS] + "\n\n⚠️ Output truncated..."

        return output.strip() if output else "✅ (No output)"

    except FileNotFoundError:
        return "❌ Error: 'git' command not found."
    except TimeoutError:
        return "⏱️ git shell timeout (>45s). Add git.exe and python.exe to Kaspersky exclusions for full speed."
    except Exception as e:
        return f"❌ Error: {e}"


@mcp.tool()
async def git_status(cwd: str | None = None) -> str:
    """Quick git status. Use for: git status, show changes, what changed, repo state (tracked files only)."""
    return await _run_git(["--no-optional-locks", "status", "--porcelain=v2", "--branch", "--untracked-files=no", "--no-ahead-behind"], cwd)


@mcp.tool()
async def git_status_quick(cwd: str | None = None) -> str:
    """Quick git status — modified tracked files only, no untracked scan."""
    return await _run_git(["--no-optional-locks", "status", "--porcelain=v2", "--branch", "--untracked-files=no", "--no-ahead-behind"], cwd)


@mcp.tool()
async def git_status_shell(cwd: str | None = None) -> str:
    """Ultra-light git status using shell=True to bypass AV interception."""
    return await _run_git_shell(["status"], cwd)


@mcp.tool()
async def git_diff(cached: bool = False, cwd: str | None = None) -> str:
    """Show changes in the working tree or index."""
    args = ["diff"]
    if cached:
        args.append("--cached")
    return await _run_git(args, cwd)


@mcp.tool()
async def git_add(path: str = ".", cwd: str | None = None) -> str:
    """Add file contents to the index."""
    return await _run_git(["add", path], cwd)


@mcp.tool()
async def git_log(n: int = 10, cwd: str | None = None) -> str:
    """Show the commit history."""
    args = ["--no-optional-locks", "log", "-n", str(n), "--oneline", "--decorate"]
    return await _run_git(args, cwd)


@mcp.tool()
async def git_commit(message: str = "", intent: str = "", cwd: str | None = None) -> str:
    """Commit changes to the repository."""
    if not message and intent:
        import re as _re
        m = _re.search(r'commit\s+["\']([^"\']+)["\']', intent, _re.IGNORECASE)
        if m:
            message = m.group(1)
        else:
            pat = r'(?:with\s+message|mensaje)\s+["\']?([^"\']+?)(?:["\']|$)'
            m = _re.search(pat, intent, _re.IGNORECASE)
            if m:
                message = m.group(1).strip()
    if not message:
        return "❌ Error: Commit message cannot be empty."
    return await _run_git(["commit", "-m", message], cwd)


@mcp.tool()
async def git_branch(cwd: str | None = None) -> str:
    """List local branches (avoids slow remote enumeration on Windows)."""
    return await _run_git(["branch", "--no-color"], cwd)


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
