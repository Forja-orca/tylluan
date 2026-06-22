import os
import shutil
import tempfile
import pytest
import asyncio
import stat
from guilds.core.git import git_status, git_add, git_commit, git_log

def on_rmtree_error(func, path, exc_info):
    """Handle read-only files on Windows during rmtree."""
    os.chmod(path, stat.S_IWRITE)
    func(path)

@pytest.fixture
async def temp_repo():
    """Create a temporary git repository for testing."""
    temp_dir = tempfile.mkdtemp()
    
    # Isolate git config
    env = os.environ.copy()
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CONFIG_GLOBAL"] = os.path.join(temp_dir, ".gitconfig")
    
    # Initialize git repo
    process = await asyncio.create_subprocess_exec(
        "git", "init",
        cwd=temp_dir,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env
    )
    await process.communicate()
    
    # Configure dummy user for commits
    await asyncio.create_subprocess_exec("git", "config", "user.email", "test@example.com", cwd=temp_dir, env=env)
    await asyncio.create_subprocess_exec("git", "config", "user.name", "Test User", cwd=temp_dir, env=env)
    
    yield temp_dir
    
    # Cleanup with retry logic for Windows
    try:
        shutil.rmtree(temp_dir, onerror=on_rmtree_error)
    except Exception:
        pass

@pytest.mark.asyncio
async def test_git_status_empty(temp_repo):
    status = await git_status(cwd=temp_repo)
    # git status -sb shows '## No commits yet on master' (or main)
    assert "##" in status
    assert "No commits yet" in status

@pytest.mark.asyncio
async def test_git_commit_flow(temp_repo):
    # 1. Create a file
    file_path = os.path.join(temp_repo, "hello.txt")
    with open(file_path, "w") as f:
        f.write("hello world")
        
    # 2. Check status (should see untracked file)
    status = await git_status(cwd=temp_repo)
    assert "hello.txt" in status
    
    # 3. Add and commit
    await git_add(".", cwd=temp_repo)
    
    commit_res = await git_commit("test commit", cwd=temp_repo)
    assert "test commit" in commit_res or "root-commit" in commit_res

    log = await git_log(cwd=temp_repo)
    assert "test commit" in log
