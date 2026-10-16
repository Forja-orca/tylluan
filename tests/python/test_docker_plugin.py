"""TC-04 (2026-10-16): puerto del test muerto `tests/guilds/test_docker_guild.py`.

El original importaba `guilds.core.docker`, módulo que ya no existe (el código
vive en `guilds/builders/plugins/docker.py`) y la CI solo ejecuta
`tests/python/`, así que nunca corrió. El mock sigue siendo válido porque
`_run_docker` aún llama directamente a `asyncio.create_subprocess_exec`.
"""

import pathlib
import sys
from unittest.mock import patch

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.docker import docker_ps, docker_run


class MockProcess:
    def __init__(self, stdout=b"", stderr=b"", returncode=0):
        self.stdout_data = stdout
        self.stderr_data = stderr
        self.returncode = returncode

    async def communicate(self):
        return self.stdout_data, self.stderr_data


@pytest.mark.asyncio
async def test_docker_ps_mock():
    with patch("asyncio.create_subprocess_exec") as mock_exec:
        stdout = (
            b"CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES\n"
            b"abc123         alpine    \"sh\"      1m ago    Up 1m               test-container"
        )
        mock_exec.return_value = MockProcess(stdout=stdout)

        result = await docker_ps()

        assert "abc123" in result
        assert "test-container" in result
        mock_exec.assert_called_once()
        args = mock_exec.call_args[0]
        assert args[0] == "docker"
        assert args[1] == "ps"


@pytest.mark.asyncio
async def test_docker_run_mock():
    with patch("asyncio.create_subprocess_exec") as mock_exec:
        mock_exec.return_value = MockProcess(stdout=b"container_id_xyz_123")

        result = await docker_run(image="nginx", name="my-nginx", ports="80:80")

        assert "container_id_xyz_123" in result
        args = mock_exec.call_args[0]
        assert "run" in args
        assert "nginx" in args
        assert "--name" in args
        assert "my-nginx" in args


@pytest.mark.asyncio
async def test_docker_error_handling():
    with patch("asyncio.create_subprocess_exec") as mock_exec:
        mock_exec.side_effect = FileNotFoundError()

        result = await docker_ps()
        assert "docker' command not found" in result
