import asyncio
import json
import logging
import os
import time
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-cron-scheduler")

CRON_DIR = Path(os.environ.get("CRON_DIR") or "data/cron")
JOBS_FILE = CRON_DIR / "jobs.json"

_jobs: list[dict] = []
_timers: dict[str, asyncio.TimerHandle] = {}
_loop: asyncio.AbstractEventLoop | None = None


def _ensure_dir():
    CRON_DIR.mkdir(parents=True, exist_ok=True)


def _load():
    global _jobs
    try:
        if JOBS_FILE.exists():
            _jobs = json.loads(JOBS_FILE.read_text())
    except Exception:
        _jobs = []


def _save():
    _ensure_dir()
    JOBS_FILE.write_text(json.dumps(_jobs, indent=2))


def _start_timer(job: dict):
    global _loop
    if _loop is None:
        _loop = asyncio.get_event_loop()
    jid = job["id"]
    if jid in _timers:
        _timers[jid].cancel()

    async def _tick():
        job["lastRun"] = time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime())
        job["runCount"] += 1
        try:
            rc, out, err = await utils.run_command(["cmd", "/c", job["command"]], timeout_secs=30)
        except Exception as exc:
            pass
        if job.get("maxRuns") and job["runCount"] >= job["maxRuns"]:
            job["active"] = False
            _timers.pop(jid, None)
        _save()
        if job.get("active"):
            _schedule_next(job)

    def _wrapper():
        asyncio.ensure_future(_tick(), loop=_loop)

    _schedule_next(job)


def _schedule_next(job: dict):
    global _loop
    if _loop is None:
        _loop = asyncio.get_event_loop()
    h = _loop.call_later(job["intervalMs"] / 1000.0, _start_timer, job)
    _timers[job["id"]] = h


def _init():
    _load()
    for job in _jobs:
        if job.get("active"):
            _start_timer(job)


@mcp.tool()
async def cron_schedule(name: str, command: str, interval_minutes: int, max_runs: int | None = None) -> str:
    """Schedule a recurring task. Returns the job ID."""
    jid = f"cron_{int(time.time() * 1000):x}"
    job = {
        "id": jid, "name": name, "intervalMs": interval_minutes * 60 * 1000,
        "command": command, "createdAt": time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()),
        "lastRun": None, "runCount": 0, "maxRuns": max_runs, "active": True,
    }
    _jobs.append(job)
    _start_timer(job)
    _save()
    return f"Scheduled: '{name}'\n  ID: {jid}\n  Interval: every {interval_minutes} min\n  Command: {command}"


@mcp.tool()
async def cron_list() -> str:
    """List all scheduled cron jobs."""
    if not _jobs:
        return "No cron jobs scheduled."
    lines = [f"Cron Jobs ({len(_jobs)}):"]
    for j in _jobs:
        icon = "active" if j.get("active") else "inactive"
        runs = f"{j['runCount']}" + (f"/{j['maxRuns']}" if j.get("maxRuns") else "")
        lines.append(f"  {j['id']}: '{j['name']}' every {round(j['intervalMs'] / 60000)}min runs: {runs} [{icon}]")
    return "\n".join(lines)


@mcp.tool()
async def cron_cancel(job_id: str) -> str:
    """Cancel a scheduled cron job by ID."""
    job = next((j for j in _jobs if j["id"] == job_id), None)
    if not job:
        return f"Job not found: {job_id}"
    job["active"] = False
    if job_id in _timers:
        _timers[job_id].cancel()
        del _timers[job_id]
    _save()
    return f"Cancelled job: '{job['name']}' ({job_id})"


_init()

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
