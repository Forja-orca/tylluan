"""Tests for the process_top tool in guilds/watchers/plugins/system_metrics.py.

Two layers:
  1. Simulated psutil (fake Process objects) — verifies the window semantics
     (average cores = cpu_percent delta / 100), the top-N sort, the family
     labels and the resilience to processes dying mid-window. No real
     processes are touched.
  2. One real call with a short window — smoke test that the real psutil
     path returns well-formed rows on this host.
"""
import pathlib
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.watchers.plugins.system_metrics import _family_label, collect_process_top


class FakeMem:
    def __init__(self, rss_mb):
        self.rss = rss_mb * 1024 * 1024


class FakeProc:
    """Mimics the psutil.Process surface used by collect_process_top."""

    def __init__(self, pid, ppid, name, rss_mb, samples, raise_after_prime=False):
        self.info = {
            "pid": pid,
            "ppid": ppid,
            "name": name,
            "memory_info": FakeMem(rss_mb),
        }
        self._samples = list(samples)
        self._calls = 0
        self._raise_after_prime = raise_after_prime

    def cpu_percent(self, interval):
        assert interval is None, "must poll non-blocking; window sleep is external"
        self._calls += 1
        if self._calls > 1 and self._raise_after_prime:
            import psutil

            raise psutil.NoSuchProcess(self.info["pid"])
        return self._samples.pop(0) if self._samples else 0.0


def _fake_psutil_exc():
    import psutil

    return psutil


def test_avg_cores_over_window_and_sort():
    procs = [
        FakeProc(100, 1, "hog", 500, [0.0, 250.0]),   # 2.5 cores average
        FakeProc(101, 1, "mid", 300, [0.0, 100.0]),   # 1.0 core
        FakeProc(102, 1, "idle", 100, [0.0, 10.0]),   # 0.1 cores
    ]
    rows = collect_process_top(n=2, window_secs=0.0, iter_procs=lambda: iter(procs),
                               sleep_fn=lambda s: None)
    assert [r["name"] for r in rows] == ["hog", "mid"]  # top N, sorted desc
    assert rows[0]["avg_cores"] == pytest.approx(2.5)   # 250 pct -> 2.5 cores
    assert rows[0]["mem_mb"] == pytest.approx(500.0)
    assert rows[0]["ppid"] == 1
    assert rows[0]["family"] == "otros"


def test_family_labels():
    kernel = FakeProc(10, 1, "tylluan-nexus.exe", 900, [0.0, 50.0])
    guild_child = FakeProc(11, 10, "python.exe", 120, [0.0, 5.0])   # ppid = kernel
    llama = FakeProc(12, 1, "llama-server.exe", 4000, [0.0, 1.0])
    other = FakeProc(13, 1, "chrome.exe", 800, [0.0, 1.0])
    rows = collect_process_top(n=10, window_secs=0.0,
                               iter_procs=lambda: iter([kernel, guild_child, llama, other]),
                               sleep_fn=lambda s: None)
    fam = {r["name"]: r["family"] for r in rows}
    assert fam["tylluan-nexus.exe"] == "tylluan-kernel"
    assert fam["python.exe"] == "guild-hijo-del-kernel"
    assert fam["llama-server.exe"] == "llama-server"
    assert fam["chrome.exe"] == "otros"


def test_process_dying_mid_window_is_tolerated():
    # dies between prime and read: must not raise, simply absent from rows
    procs = [
        FakeProc(20, 1, "stable", 50, [0.0, 50.0]),
        FakeProc(21, 1, "vanishing", 50, [0.0], raise_after_prime=True),
    ]
    rows = collect_process_top(n=10, window_secs=0.0, iter_procs=lambda: iter(procs),
                               sleep_fn=lambda s: None)
    assert [r["name"] for r in rows] == ["stable"]


def test_family_label_unit():
    assert _family_label("tylluan-cli", 1, set()) == "tylluan-kernel"
    assert _family_label("python.exe", 42, {42}) == "guild-hijo-del-kernel"
    assert _family_label("strata-node", 1, set()) == "strata/flota"
    assert _family_label("claude_watcher", 1, set()) == "strata/flota"
    assert _family_label("anything-else", 1, set()) == "otros"


def test_real_call_smoke():
    """One real psutil call with a short window: rows well-formed, read-only."""
    rows = collect_process_top(n=5, window_secs=0.3)
    assert rows, "expected at least one process on a live host"
    assert len(rows) <= 5
    for r in rows:
        assert set(r) == {"pid", "ppid", "name", "avg_cores", "mem_mb", "family"}
        assert r["pid"] >= 0  # Windows System Idle Process is PID 0
        assert r["avg_cores"] >= 0.0
        assert r["family"] in {"tylluan-kernel", "guild-hijo-del-kernel",
                               "llama-server", "strata/flota", "otros"}
    # sorted by average cores, descending
    cores = [r["avg_cores"] for r in rows]
    assert cores == sorted(cores, reverse=True)


def test_nested_guild_process_is_labelled_as_kernel_child():
    # launcher (ppid = kernel) plus the real interpreter (ppid = launcher):
    # both belong to the kernel family, not "otros".
    procs = [
        FakeProc(1000, 1, "tylluan-nexus.exe", 9000, [0.0, 50.0]),
        FakeProc(1001, 1000, "python.exe", 80, [0.0, 10.0]),
        FakeProc(1002, 1001, "python.exe", 80, [0.0, 20.0]),
        FakeProc(2000, 1, "python.exe", 80, [0.0, 5.0]),
    ]
    rows = collect_process_top(n=10, window_secs=0.0, iter_procs=lambda: procs, sleep_fn=lambda s: None)
    family = {r["pid"]: r["family"] for r in rows}
    assert family[1001] == "guild-hijo-del-kernel"
    assert family[1002] == "guild-hijo-del-kernel"
    assert family[2000] == "otros"


def test_system_idle_process_is_not_listed():
    procs = [
        FakeProc(0, 0, "System Idle Process", 0, [0.0, 2600.0]),
        FakeProc(500, 1, "real-hog", 100, [0.0, 150.0]),
    ]
    rows = collect_process_top(n=10, window_secs=0.0, iter_procs=lambda: procs, sleep_fn=lambda s: None)
    assert [r["pid"] for r in rows] == [500]
