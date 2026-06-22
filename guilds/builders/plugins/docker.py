"""
TylluanNexus Docker Guild — Container lifecycle management.

This guild provides tools to list, run, and monitor Docker containers.
"""

import asyncio
import os
import re as _re
import sys
from mcp.server.fastmcp import FastMCP
from guilds.builders.plugins import utils

mcp = FastMCP("tylluan-docker")

MAX_OUTPUT_CHARS = 50_000

async def _run_docker(args: list[str]) -> str:
    """Helper to run docker commands."""
    cmd = ["docker"] + args
    
    try:
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        
        stdout, stderr = await asyncio.wait_for(
            process.communicate(),
            timeout=60, # Docker ops can be slow
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
        return "❌ Error: 'docker' command not found. Ensure Docker is installed and running."
    except asyncio.TimeoutError:
        return "⏰ Error: Docker command timed out."
    except Exception as e:
        return f"❌ Error: {e}"

@mcp.tool()
async def docker_ps(all: bool = False) -> str:
    """List Docker containers."""
    args = ["ps"]
    if all:
        args.append("-a")
    return await _run_docker(args)

def _extract_container(intent: str) -> str:
    """Extract container ID or name from natural-language intent."""
    m = _re.search(r'\b([a-f0-9]{6,12}|[\w][\w\-]{2,})\b(?:\s+container)?', intent, _re.IGNORECASE)
    return m.group(1) if m else ""

@mcp.tool()
async def docker_run(
    image: str = "", name: str | None = None, ports: str | None = None, intent: str = ""
) -> str:
    """Run a new container from an image.

    Use for: docker run, start container, launch image, run ubuntu, run python container.
    """
    if not image:
        m = _re.search(r'(?:run|start|launch|pull)\s+([\w:./\-]+)', intent, _re.IGNORECASE)
        image = m.group(1) if m else ""
    if not image:
        return "❌ No image specified. Example: 'docker run ubuntu'"
    args = ["run", "-d"]
    if name:
        args.extend(["--name", name])
    if ports:
        args.extend(["-p", ports])
    args.append(image)
    return await _run_docker(args)

@mcp.tool()
async def docker_stop(container_id: str = "", intent: str = "") -> str:
    """Stop a running container. Use for: docker stop, stop container, kill container."""
    if not container_id:
        container_id = _extract_container(intent)
    if not container_id:
        return "❌ No container ID specified. Example: 'stop container abc123'"
    return await _run_docker(["stop", container_id])

@mcp.tool()
async def docker_logs(container_id: str = "", tail: int = 50, intent: str = "") -> str:
    """Get logs from a container. Use for: docker logs, show container logs, container output."""
    if not container_id:
        container_id = _extract_container(intent)
    if not container_id:
        return "❌ No container ID specified. Example: 'docker logs abc123'"
    return await _run_docker(["logs", "--tail", str(tail), container_id])

@mcp.tool()
async def docker_exec(container_id: str = "", command: str = "", intent: str = "") -> str:
    """Execute a command in a running container. Use for: docker exec, run command in container."""
    if not container_id:
        container_id = _extract_container(intent)
    if not command:
        command = intent
    if not container_id or not command:
        return "❌ Need both container ID and command. Example: 'exec abc123 ls /app'"
    return await _run_docker(["exec", container_id, "sh", "-c", command])

@mcp.tool()
async def docker_status() -> str:
    """Get Docker daemon status and info."""
    return await _run_docker(["info"])

@mcp.tool()
async def docker_images() -> str:
    """List available Docker images."""
    return await _run_docker(["images"])

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
