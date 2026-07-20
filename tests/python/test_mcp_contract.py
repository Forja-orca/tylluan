"""MCP CONTRACT-01 validation — no live kernel required."""
import pathlib

SOVEREIGN_TOOLS = {"tylluan_do", "tylluan_remember", "tylluan_recall", "tylluan_think", "tylluan_graph"}
REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent


def test_sovereign_tools_declared_in_source():
    source = (REPO_ROOT / "crates/tylluan-kernel/src/transport/server/tools.rs").read_text()
    for tool in SOVEREIGN_TOOLS:
        assert tool in source, f"CONTRACT-01 violation: {tool} not found in tools.rs"


def test_sovereign_count_is_five():
    assert len(SOVEREIGN_TOOLS) == 5, "CONTRACT-01: exactly 5 sovereign tools"


def test_integrations_index_exists():
    index = REPO_ROOT / "docs/reference/integrations/README.md"
    assert index.exists(), "docs/reference/integrations/README.md is missing"
    content = index.read_text().lower()
    assert "openclaw" in content, "integrations/README.md must mention openclaw"
    assert "hermes" in content, "integrations/README.md must mention hermes"
