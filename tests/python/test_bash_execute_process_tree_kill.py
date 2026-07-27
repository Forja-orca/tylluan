"""Regression test: on Windows, a timed-out run_command() must kill the
whole process tree, not just the immediate child.

asyncio's process.kill() only signals the direct child (powershell.exe).
Any grandchild it spawns (git.exe, cargo.exe, a detached background
process...) survives as an orphan when only the immediate child is killed.
Found living the real flow, 2026-07-26: git commands hung the bash guild
because the orphaned git.exe kept the pipe open past the timeout.
"""
import asyncio
import subprocess
import sys
import pathlib
import time

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.utils import run_command

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="tests a Windows-only process-tree kill path")


@pytest.mark.asyncio
async def test_timed_out_command_kills_grandchild_process(tmp_path):
    # PowerShell (the direct child) spawns a detached `ping` process (the
    # grandchild, a long-lived process trivially identifiable by name) and
    # writes its PID to a file, then sleeps well past the outer timeout. If
    # only the immediate child were killed, the grandchild would keep
    # running as an orphan after run_command returns.
    pid_file = tmp_path / "grandchild.pid"
    script = (
        "$p = Start-Process -FilePath ping -ArgumentList '-n','120','127.0.0.1' -PassThru -NoNewWindow; "
        f"Set-Content -Path '{pid_file}' -Value $p.Id; "
        "Start-Sleep -Seconds 60"
    )

    with pytest.raises(asyncio.TimeoutError):
        await run_command(["powershell", "-NoProfile", "-Command", script], timeout_secs=3)

    deadline = time.monotonic() + 5
    grandchild_pid = None
    while time.monotonic() < deadline:
        if pid_file.exists() and pid_file.read_text().strip():
            grandchild_pid = int(pid_file.read_text().strip())
            break
        time.sleep(0.2)

    assert grandchild_pid is not None, "grandchild never started -- test setup is broken, not the fix"

    # The grandchild must NOT still be running after the tree kill.
    result = subprocess.run(
        ["tasklist", "/FI", f"PID eq {grandchild_pid}"],
        capture_output=True, text=True,
    )
    assert str(grandchild_pid) not in result.stdout, (
        f"grandchild PID {grandchild_pid} is still running after timeout -- "
        f"process tree was not killed, only the immediate child"
    )
