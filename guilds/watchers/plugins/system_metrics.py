"""
TylluanNexus System Metrics Guild — Local system monitoring tools.

Provides tools to get system metrics (CPU, memory, disk) without external dependencies.
"""

from guilds.core import utils
import asyncio
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

def _family_label(name: str, ppid: int, kernel_pids: set) -> str:
    """Classify a process into a family label (read-only, name-based)."""
    n = (name or "").lower()
    if n.endswith(".exe"):
        n = n[:-4]
    if n in ("tylluan-nexus", "tylluan-kernel", "tylluan-cli", "tylluan"):
        return "tylluan-kernel"
    if ppid in kernel_pids:
        return "guild-hijo-del-kernel"
    if n in ("llama-server", "llama-server.exe") or n.startswith("llama"):
        return "llama-server"
    if n.startswith("strata") or "watcher" in n:
        return "strata/flota"
    return "otros"


def collect_process_top(n: int = 10, window_secs: float = 5.0, iter_procs=None,
                        sleep_fn=None) -> list:
    """Top-N processes by AVERAGE CPU cores over a sampling window. Read-only.

    psutil's cpu_percent(None) returns the percentage since the previous call
    for the same Process object (100.0 == one full core). So we prime every
    process, wait the window, then read the delta: average_cores = pct / 100.
    This deliberately reports average cores over the window, NOT cumulative
    CPU-time totals.

    iter_procs/sleep_fn are injectable so tests can simulate psutil without
    touching real processes.
    """
    import time as _time

    if sleep_fn is None:
        sleep_fn = _time.sleep
    if iter_procs is None:
        iter_procs = lambda: psutil.process_iter(["pid", "ppid", "name", "memory_info"])

    procs = []
    for pr in iter_procs():
        try:
            pr.cpu_percent(None)  # prime the per-process counter
            procs.append(pr)
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            continue
    sleep_fn(window_secs)

    kernel_pids = {p.info["pid"] for p in procs
                   if _family_label(p.info.get("name", ""), 0, set()) == "tylluan-kernel"}
    parent_of = {p.info["pid"]: p.info.get("ppid") for p in procs}

    def kernel_ancestor(pid):
        """PID of the kernel above `pid` (any depth, up to 8 levels), else None.
        Guild processes are often a launcher plus the real interpreter, i.e. the
        kernel's grandchildren."""
        cur = parent_of.get(pid)
        for _ in range(8):
            if cur is None:
                return None
            if cur in kernel_pids:
                return cur
            cur = parent_of.get(cur)
        return None

    rows = []
    for pr in procs:
        try:
            pct = pr.cpu_percent(None)
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            continue
        info = pr.info
        if info["pid"] == 0:
            continue  # "System Idle Process" is idle time, not a consumer
        rows.append({
            "pid": info["pid"],
            "ppid": info["ppid"],
            "name": info.get("name") or "?",
            "avg_cores": round(pct / 100.0, 2),
            "mem_mb": round((info["memory_info"].rss if info.get("memory_info") else 0) / (1024 * 1024), 1),
            "family": _family_label(
                info.get("name", ""),
                kernel_ancestor(info["pid"]) or info["ppid"],
                kernel_pids,
            ),
        })
    rows.sort(key=lambda r: r["avg_cores"], reverse=True)
    return rows[:n]


@mcp.tool()
async def process_top(n: int = 10, window_secs: float = 5.0) -> str:
    """Top N processes by AVERAGE CPU cores over a sampling window. Read-only.

    Use for: top processes, what is using cpu, cpu hogs, quiénes consumen cpu,
    qué proceso consume más, process audit, kernel children, familias de
    procesos, ver procesos.

    NEVER kills, signals or modifies any process: pure psutil sampling.

    Returns:
        Table with PID, parent PID, average cores (NOT cumulative CPU time),
        memory MB, name and family label (tylluan-kernel, guild-hijo-del-kernel,
        llama-server, strata/flota, otros).
    """
    # The sampling window sleeps: run it off the event loop so the guild keeps
    # answering other MCP calls meanwhile.
    rows = await asyncio.to_thread(collect_process_top, n=n, window_secs=window_secs)
    out = [f"Top {n} procesos por nucleos medios (ventana {window_secs:.1f}s, solo lectura):"]
    for r in rows:
        out.append(
            f"  {r['avg_cores']:>5.2f} cores  PID {r['pid']:>7}  PPID {r['ppid']:>7}"
            f"  {r['mem_mb']:>9.1f} MB  [{r['family']}]  {r['name']}"
        )
    return chr(10).join(out)

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)