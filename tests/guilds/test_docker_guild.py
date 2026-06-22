import pytest
from unittest.mock import patch, MagicMock
from guilds.core.docker import docker_ps, docker_run, docker_stop, docker_logs
import asyncio

# Mock for asyncio.create_subprocess_exec return value
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
        # Simulate 'docker ps' output
        stdout = b"CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES\nabc123         alpine    \"sh\"      1m ago    Up 1m               test-container"
        mock_process = MockProcess(stdout=stdout)
        mock_exec.return_value = mock_process
        
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
        stdout = b"container_id_xyz_123"
        mock_process = MockProcess(stdout=stdout)
        mock_exec.return_value = mock_process
        
        result = await docker_run(image="nginx", name="my-nginx", ports="80:80")
        
        assert "container_id_xyz_123" in result
        # Check call arguments
        args = mock_exec.call_args[0]
        assert "run" in args
        assert "nginx" in args
        assert "--name" in args
        assert "my-nginx" in args

@pytest.mark.asyncio
async def test_docker_error_handling():
    with patch("asyncio.create_subprocess_exec") as mock_exec:
        # Simulate FileNotFoundError (docker not installed)
        mock_exec.side_effect = FileNotFoundError()
        
        result = await docker_ps()
        assert "docker' command not found" in result
