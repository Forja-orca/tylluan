"""Unit tests for TRINITY coordinator Thinker decomposition — no kernel required."""
import sys
import pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.core.coordinator import _split_intent, _is_failure, MAX_TASKS


def test_split_then_connector():
    parts = _split_intent("search the web for X then summarize the results")
    assert len(parts) == 2
    assert "search" in parts[0]
    assert "summarize" in parts[1]


def test_split_numbered():
    parts = _split_intent("1. run git status 2. show the diff 3. commit")
    assert len(parts) == 3


def test_split_single_intent():
    parts = _split_intent("run git status")
    assert len(parts) == 1
    assert parts[0] == "run git status"


def test_split_caps_at_max_tasks():
    # MAX_TASKS was raised 3->5 in 1c10da5 (M18-P3a, real parallel execution)
    # -- this test hardcoded 3 and went stale until 2026-07-26 dogfooding
    # caught it failing in CI. Reference the constant so it can't drift again.
    parts = _split_intent("do A then do B then do C then do D then do E then do F then do G")
    assert len(parts) <= MAX_TASKS


def test_split_spanish_connector():
    parts = _split_intent("busca en la web luego resume los resultados")
    assert len(parts) == 2


def test_is_failure_empty():
    assert _is_failure("") is True
    assert _is_failure("   ") is True


def test_is_failure_error_marker():
    assert _is_failure("\u274c HTTP 500") is True
    assert _is_failure('{"error": "not found"}') is True


def test_is_failure_ok():
    assert _is_failure("Here are the results: ...") is False
    assert _is_failure("git status: clean") is False
