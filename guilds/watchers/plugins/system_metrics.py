"""
TylluanNexus System Metrics Guild — Local system monitoring tools.

Provides tools to get system metrics (CPU, memory, disk) without external dependencies.
"""

from guilds.core import utils
import os
import sys
import psutil
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tylluan-system-metrics")

def get_process_memory_mb() -> float:
    """Get current process memory in MB."""
    process = psutil.Process(os.getpid())
    return process.memory_info().rss / (1024 * 1024)

def get_system_metrics() -> dict:
    """Get comprehensive system metrics."""
    cpu_percent = psutil.cpu_percent(interval=0.1)
    memory = psutil.virtual_memory()
    disk = psutil.disk_usage('/')
    
    return {
        "cpu_percent": cpu_percent,
        "cpu_count": psutil.cpu_count(),
        "memory_total_mb": memory.total / (1024 * 1024),
        "memory_used_mb": memory.used / (1024 * 1024),
        "memory_percent": memory.percent,
        "disk_total_gb": disk.total / (1024 * 1024 * 1024),
        "disk_used_gb": disk.used / (1024 * 1024 * 1024),
        "disk_percent": disk.percent,
    }

@mcp.tool()
async def system_cpu() -> str:
    """Get current CPU usage percentage.
    
    Returns:
        CPU usage as percentage (0-100).
    """
    return f"CPU: {psutil.cpu_percent(interval=0.5)}%"

@mcp.tool()
async def system_memory() -> str:
    """Get current memory usage.
    
    Returns:
        Memory usage details.
    """
    mem = psutil.virtual_memory()
    return f"Memory: {mem.percent}% used ({mem.used // (1024**2)}MB / {mem.total // (1024**2)}MB)"

@mcp.tool()
async def system_disk() -> str:
    """Get current disk usage.
    
    Returns:
        Disk usage for root partition.
    """
    disk = psutil.disk_usage('/')
    return f"Disk: {disk.percent}% used ({disk.used // (1024**3)}GB / {disk.total // (1024**3)}GB)"

@mcp.tool()
async def system_metrics() -> str:
    """Get all system metrics at once: CPU, memory, and disk usage.

    Use for: system metrics, show metrics, cpu usage, memory usage, disk usage,
    system health, system status, resource usage, how much cpu, how much memory,
    system stats, show system info, métricas del sistema.

    Returns:
        Complete system status summary including CPU%, RAM%, and disk%.
    """
    metrics = get_system_metrics()
    cpu = metrics['cpu_percent']
    mem_pct = metrics['memory_percent']
    mem_used = metrics['memory_used_mb'] / 1024
    mem_total = metrics['memory_total_mb'] / 1024
    disk_pct = metrics['disk_percent']
    disk_used = metrics['disk_used_gb']
    disk_total = metrics['disk_total_gb']
    cores = metrics['cpu_count']
    return (
        f"🖥️  System Metrics\n"
        f"CPU:    {cpu:>5.1f}%  ({cores} cores)\n"
        f"Memory: {mem_pct:>5.1f}%  ({mem_used:.1f}GB / {mem_total:.1f}GB)\n"
        f"Disk:   {disk_pct:>5.1f}%  ({disk_used:.1f}GB / {disk_total:.1f}GB)"
    )

@mcp.tool()
async def process_info() -> str:
    """Get TylluanNexus process information.
    
    Returns:
        Current process memory and CPU usage.
    """
    process = psutil.Process(os.getpid())
    mem_mb = process.memory_info().rss / (1024 * 1024)
    cpu_times = process.cpu_times()
    return f"Process PID {os.getpid()}: {mem_mb:.1f}MB, CPU time: {cpu_times.user + cpu_times.system:.1f}s"

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)