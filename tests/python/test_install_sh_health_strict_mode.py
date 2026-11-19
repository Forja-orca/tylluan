"""install.sh reads `readiness` from /health under `set -euo pipefail` (TL, 2026-11-19).

Real regression: a kernel published before D212 (v0.17.2, the one the Install Smoke
installs) does not emit `readiness`; `grep -o` finds no match, the pipeline returns 1 and,
under `set -e` + `pipefail`, install.sh died silently with exit code 1 (pushes 375 and 376).
The test runs the REAL assignment line from install.sh in strict mode.
"""
import pathlib
import re
import shutil
import subprocess

import pytest

INSTALL_SH = pathlib.Path(__file__).resolve().parent.parent.parent / "install.sh"
BASH = shutil.which("bash")

pytestmark = pytest.mark.skipif(BASH is None, reason="bash not available")


def _readiness_line() -> str:
    text = INSTALL_SH.read_text(encoding="utf-8")
    m = re.search(r"^[ \t]*READINESS=\$\(.*\)[ \t]*$", text, re.MULTILINE)
    assert m, "the READINESS=... assignment in install.sh was not found"
    return m.group(0).strip()


def _run(body: str) -> subprocess.CompletedProcess:
    script = (
        "set -euo pipefail\n"
        f"HEALTH_BODY='{body}'\n"
        f"{_readiness_line()}\n"
        'echo "READINESS=[$READINESS]"\n'
    )
    return subprocess.run([BASH, "-c", script], capture_output=True, text=True, timeout=30)


def test_kernel_published_before_d212_does_not_kill_the_installer():
    r = _run('{"status":"ok","version":"0.17.2","commit":"cef53772"}')
    assert r.returncode == 0, (r.stdout, r.stderr)
    assert "READINESS=[]" in r.stdout


@pytest.mark.parametrize("value", ["ok", "degraded", "failed"])
def test_new_kernel_readiness_is_read(value):
    r = _run('{"status":"ok","readiness":"%s","guilds_down":[]}' % value)
    assert r.returncode == 0, (r.stdout, r.stderr)
    assert f"READINESS=[{value}]" in r.stdout
