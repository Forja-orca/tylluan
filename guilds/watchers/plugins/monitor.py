"""
TylluanNexus Watcher Guild — System Monitoring.

Provides real-time system metrics (CPU, Memory, Disk, Network)
for the local host.
"""

import asyncio
import os
import shutil
import platform
from mcp.server.fastmcp import FastMCP
from guilds.core import utils

# We'll use psutil if available, otherwise fallback to basic os/platform calls
try:
    import psutil
except ImportError:
    psutil = None

mcp = FastMCP("tylluan-monitor")

@mcp.tool()
async def system_info() -> str:
    """Get a summary of system health (CPU, RAM, Disk, OS)."""
    info = {
        "os": f"{platform.system()} {platform.release()}",
        "node": platform.node(),
    }

    if psutil:
        info["cpu_percent"] = psutil.cpu_percent(interval=1)
        info["ram_percent"] = psutil.virtual_memory().percent
        info["disk_percent"] = psutil.disk_usage("/").percent
        info["boot_time"] = psutil.boot_time()
    else:
        # Basic fallback for disk
        total, used, free = shutil.disk_usage("/")
        info["disk_percent"] = round((used / total) * 100, 1)

    return f"📊 System Health: {info}"

@mcp.tool()
async def process_list(limit: int = 10) -> str:
    """List the top processes currently running on the system.

    Args:
        limit: Number of processes to return (default: 10).
    """
    if not psutil:
        return "❌ Error: 'psutil' library not installed for process tracking."

    try:
        cpu_count = psutil.cpu_count() or 1
        cpu_max = 100.0 * cpu_count

        # First pass: seed cpu_percent counters
        for p in psutil.process_iter(['name', 'cpu_percent', 'memory_percent']):
            try:
                p.cpu_percent(interval=None)
            except Exception:
                pass
        await asyncio.sleep(0.5)

        procs = []
        for p in psutil.process_iter(['name', 'cpu_percent', 'memory_percent', 'pid']):
            try:
                info = p.info
                if info['name']:
                    procs.append(info)
            except Exception:
                pass

        procs.sort(key=lambda x: x.get('cpu_percent') or 0, reverse=True)

        lines = [f"{'Process':<28} {'CPU%':>6}  {'MEM%':>6}  {'PID':>7}"]
        lines.append("-" * 55)
        for p in procs[:limit]:
            name = (p.get('name') or 'unknown')[:27]
            raw_cpu = p.get('cpu_percent') or 0.0
            cpu = min(raw_cpu, cpu_max) / cpu_count  # normalize to 0-100 per-core scale
            mem = p.get('memory_percent') or 0.0
            pid = p.get('pid', 0)
            lines.append(f"{name:<28} {cpu:>5.1f}%  {mem:>5.1f}%  {pid:>7}")

        return f"🔝 Top {limit} Processes:\n" + "\n".join(lines)
    except Exception as e:
        return f"❌ Process Error: {e}"

@mcp.tool()
async def network_stats() -> str:
    """Get network IO statistics (bytes in/out)."""
    if not psutil:
        return "❌ Error: 'psutil' library not installed for network stats."

    try:
        net = psutil.net_io_counters()
        sent_gb = net.bytes_sent / (1024**3)
        recv_gb = net.bytes_recv / (1024**3)
        return (
            f"🌐 Network IO\n"
            f"Sent:     {sent_gb:.2f} GB  ({net.packets_sent:,} packets)\n"
            f"Received: {recv_gb:.2f} GB  ({net.packets_recv:,} packets)\n"
            f"Errors:   in={net.errin}  out={net.errout}\n"
            f"Dropped:  in={net.dropin}  out={net.dropout}"
        )
    except Exception as e:
        return f"❌ Network Error: {e}"

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
