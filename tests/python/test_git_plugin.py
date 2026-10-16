"""TC-04 (2026-10-16): puerto del test muerto `tests/guilds/test_git_guild.py`.

El original importaba `guilds.core.git`, un módulo que ya no existe (el código
vive en `guilds/builders/plugins/git.py`) y la CI solo ejecuta `tests/python/`,
así que nunca corrió — y sus aserciones ni siquiera describen el contrato
ACTUAL: `git_status` usa `--porcelain=v2 --branch --untracked-files=no` (estado
rápido, solo tracked), que no imprime "No commits yet" ni lista untracked.
Aquí se porta contra el módulo real con las aserciones verificadas contra la
salida real de git.
"""

import os
import pathlib
import shutil
import subprocess
import sys

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent))

from guilds.builders.plugins.git import git_add, git_commit, git_log, git_status


@pytest.fixture
def temp_repo(tmp_path, monkeypatch):
    """Repositorio git aislado: sin config global del sistema, identidad dummy.

    El aislamiento va por os.environ (monkeypatch) para que también lo vean
    las llamadas del plugin, que copia os.environ al ejecutar git.
    """
    temp_dir = str(tmp_path)
    monkeypatch.setenv("GIT_CONFIG_NOSYSTEM", "1")
    monkeypatch.setenv("GIT_CONFIG_GLOBAL", os.path.join(temp_dir, ".gitconfig"))

    subprocess.run(["git", "init", "-q", temp_dir], check=True, env=os.environ.copy())
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"], cwd=temp_dir, check=True
    )
    subprocess.run(["git", "config", "user.name", "Test User"], cwd=temp_dir, check=True)

    yield temp_dir

    # Los .git objects no son read-only; ignore_errors evita que un lock de
    # Windows/Kaspersky convierta la limpieza en fallo del test.
    shutil.rmtree(temp_dir, ignore_errors=True)


@pytest.mark.asyncio
async def test_git_status_empty_repo_shows_initial_branch(temp_repo):
    status = await git_status(cwd=temp_repo)
    # Contrato actual (porcelain v2 --branch): rama y oid inicial, no prosa.
    assert "# branch." in status
    assert "(initial)" in status


@pytest.mark.asyncio
async def test_git_status_skips_untracked_by_design(temp_repo):
    # git_status es estado RÁPIDO: --untracked-files=no es intencionado
    # (ver docstrings de git_status/git_status_quick). Un untracked no aparece.
    with open(os.path.join(temp_repo, "hello.txt"), "w", encoding="utf-8") as f:
        f.write("hello world")

    status = await git_status(cwd=temp_repo)
    assert "hello.txt" not in status, "untracked files are skipped by design"


@pytest.mark.asyncio
async def test_git_commit_flow(temp_repo):
    with open(os.path.join(temp_repo, "hello.txt"), "w", encoding="utf-8") as f:
        f.write("hello world")

    await git_add(".", cwd=temp_repo)

    # Tras add, porcelain v2 muestra la entrada staged con la ruta.
    status = await git_status(cwd=temp_repo)
    assert "hello.txt" in status, f"staged file must appear in v2 status: {status}"

    commit_res = await git_commit("test commit", cwd=temp_repo)
    assert "test commit" in commit_res

    log = await git_log(cwd=temp_repo)
    assert "test commit" in log
