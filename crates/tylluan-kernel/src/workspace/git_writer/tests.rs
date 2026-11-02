//! Tests de F9-1: SOLO repositorios temporales (`git init` en `tempdir`),
//! nunca el checkout real ni ningun worktree de agente.

use super::*;
use crate::workspace::{TaskSpec, WorkspaceStore};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn run_git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} fallo: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Repo temporal con un commit base y ficheros de varias ramas.
fn init_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.email", "f91@example.invalid"]);
    run_git(&repo, &["config", "user.name", "F9-1 tests"]);
    run_git(&repo, &["config", "commit.gpgsign", "false"]);
    for (path, content) in files {
        let full = repo.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, content).unwrap();
    }
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-q", "-m", "base"]);
    (dir, repo)
}

fn store() -> (tempfile::TempDir, WorkspaceStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = WorkspaceStore::open(dir.path().join("workspace.db")).expect("store");
    (dir, store)
}

fn claim_task(store: &WorkspaceStore, task: &str, glob: &str) -> Claim {
    let now = chrono::Utc::now().timestamp_millis();
    store
        .create_task(now, &TaskSpec::new(task, glob))
        .expect("create_task");
    match store.claim("a0", task, glob, now).expect("claim") {
        crate::workspace::ClaimOutcome::Claimed(claim) => claim,
        other => panic!("reclamo inesperado: {other:?}"),
    }
}

fn changed_files(repo: &Path) -> Vec<String> {
    run_git(repo, &["status", "--porcelain"])
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| l[3..].trim().to_string())
        .collect()
}

#[test]
fn commit_paths_commits_only_the_claim_and_leaves_other_dirty_files_alone() {
    let (_repo_dir, repo) = init_repo(&[
        ("claimed/a.txt", "a0"),
        ("claimed/b.txt", "b0"),
        ("other/z.txt", "z0"),
    ]);
    let (_db_dir, store) = store();
    let claim = claim_task(&store, "t1", "claimed/**");

    std::fs::write(repo.join("claimed/a.txt"), "a1").unwrap();
    std::fs::write(repo.join("claimed/new.txt"), "nuevo").unwrap();
    std::fs::write(repo.join("other/z.txt"), "z1").unwrap();

    let outcome = commit_paths(&store, &repo, &claim, "solo rutas reclamadas").expect("commit");
    assert_eq!(outcome.commit.len(), 40, "hash de commit");
    let shown = run_git(&repo, &["show", "--name-only", "--pretty=format:", "HEAD"]);
    let mut names: Vec<&str> = shown.lines().filter(|l| !l.is_empty()).collect();
    names.sort();
    assert_eq!(names, vec!["claimed/a.txt", "claimed/new.txt"]);
    // El fichero sucio de fuera del reclamo sigue sucio (no se ha tocado).
    assert_eq!(std::fs::read_to_string(repo.join("other/z.txt")).unwrap(), "z1");
    assert!(
        changed_files(&repo).iter().any(|p| p == "other/z.txt"),
        "other/z.txt debe seguir pendiente"
    );
    assert_eq!(outcome.paths, vec!["claimed/a.txt", "claimed/new.txt"]);
}

#[test]
fn commit_paths_rejects_paths_outside_the_claim() {
    let (_repo_dir, repo) = init_repo(&[("claimed/a.txt", "a0"), ("other/z.txt", "z0")]);
    let (_db_dir, store) = store();
    let claim = claim_task(&store, "t1", "claimed/**");
    std::fs::write(repo.join("other/z.txt"), "z1").unwrap();
    let head_before = run_git(&repo, &["rev-parse", "HEAD"]);

    let error = commit_paths(&store, &repo, &claim, "no debe pasar").unwrap_err();
    assert!(
        error.to_string().contains("no hay cambios dentro del reclamo"),
        "{error}"
    );
    assert_eq!(run_git(&repo, &["rev-parse", "HEAD"]), head_before);
}

#[test]
fn paths_with_dotdot_or_absolute_are_rejected_before_git() {
    let (_repo_dir, repo) = init_repo(&[("a.txt", "a")]);
    // Debe ser igual en Windows y Linux: `\` se rechaza siempre (en Linux
    // `a\..\..\b` es un solo componente normal; en Windows escaparia), igual
    // que unidad `X:` y UNC `\\srv`.
    for bad in [
        "../escape.txt",
        "/etc/passwd",
        "a/../../b",
        "C:\\Windows\\x",
        "C:/x",
        "C:Windows/x",
        "\\srv\\share\\x",
        "a\\..\\..\\b",
        "a\\b.txt",
    ] {
        let error = match validate_repo_relative_path(&repo, bad) {
            Ok(_) => panic!("ruta {bad:?} debe fallar"),
            Err(error) => error,
        };
        assert!(!error.to_string().is_empty(), "ruta {bad:?} debe fallar");
    }
    assert!(validate_repo_relative_path(&repo, "a.txt").is_ok());
    assert!(validate_repo_relative_path(&repo, "sub/dir/a.txt").is_ok());
    assert!(validate_repo_relative_path(&repo, "sub/a-b_c.123").is_ok());
}

#[test]
fn two_commits_by_distinct_paths_do_not_mix() {
    let (_repo_dir, repo) = init_repo(&[("a/1.txt", "a0"), ("b/1.txt", "b0")]);
    let (_db_dir, store) = store();
    let claim_a = claim_task(&store, "ta", "a/**");
    let claim_b = claim_task(&store, "tb", "b/**");
    std::fs::write(repo.join("a/1.txt"), "a1").unwrap();
    std::fs::write(repo.join("b/1.txt"), "b1").unwrap();

    commit_paths(&store, &repo, &claim_a, "commit a").expect("commit a");
    commit_paths(&store, &repo, &claim_b, "commit b").expect("commit b");

    let names_a = run_git(&repo, &["show", "--name-only", "--pretty=format:", "HEAD~1"]);
    let names_b = run_git(&repo, &["show", "--name-only", "--pretty=format:", "HEAD"]);
    assert_eq!(names_a.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(), vec!["a/1.txt"]);
    assert_eq!(names_b.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(), vec!["b/1.txt"]);
}

#[test]
fn stale_claim_cannot_commit() {
    let (_repo_dir, repo) = init_repo(&[("claimed/a.txt", "a0")]);
    let (_db_dir, store) = store();
    let claim = claim_task(&store, "t1", "claimed/**");
    std::fs::write(repo.join("claimed/a.txt"), "a1").unwrap();
    // Caduca el arrendamiento del reclamo y el commit debe rechazarse.
    let expired = store
        .expire_agent_leases("a0", chrono::Utc::now().timestamp_millis() + crate::workspace::LEASE_MS + 1)
        .unwrap();
    assert_eq!(expired, 1);
    let error = commit_paths(&store, &repo, &claim, "tarde").unwrap_err();
    assert!(error.to_string().contains("no vigente"), "{error}");
}

#[test]
fn recovery_patch_applies_on_a_clean_clone_and_does_not_touch_the_tree() {
    let (_repo_dir, repo) = init_repo(&[("claimed/a.txt", "a0"), ("other/z.txt", "z0")]);
    let (_db_dir, store) = store();
    let claim = claim_task(&store, "t1", "claimed/**");
    std::fs::write(repo.join("claimed/a.txt"), "a1").unwrap();
    std::fs::write(repo.join("claimed/new.txt"), "nuevo").unwrap();
    std::fs::write(repo.join("other/z.txt"), "z1").unwrap();
    store
        .set_dirty_paths("t1", "claimed/a.txt,claimed/new.txt")
        .unwrap();

    let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let data = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("TYLLUAN_DATA_DIR", data.path()) };
    let tree_before = std::fs::read_to_string(repo.join("claimed/a.txt")).unwrap();
    let patch = recovery_patch(&store, &repo, &claim).expect("parche de recuperacion");
    unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
    drop(lock);

    assert!(patch.exists(), "el parche debe existir: {}", patch.display());
    assert!(patch.starts_with(data.path()), "el parche va al DATA_DIR");
    // El arbol de trabajo NO se toca y la tarea pasa a review.
    assert_eq!(std::fs::read_to_string(repo.join("claimed/a.txt")).unwrap(), tree_before);
    assert!(repo.join("claimed/new.txt").exists());
    assert_eq!(
        store.task_status("t1").unwrap(),
        Some(crate::workspace::TaskStatus::Review)
    );

    // El parche aplica sobre un clon limpio de la base.
    let clone_dir = tempfile::tempdir().unwrap();
    let clone = clone_dir.path().join("clone");
    run_git(
        clone_dir.path(),
        &["clone", "-q", repo.to_str().unwrap(), clone.to_str().unwrap()],
    );
    run_git(&clone, &["config", "user.email", "f91@example.invalid"]);
    let check = Command::new("git")
        .args(["apply", "--check", patch.to_str().unwrap()])
        .current_dir(&clone)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "git apply --check debe pasar: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn snapshot_before_claim_saves_patch_and_untracked_list_without_touching_the_tree() {
    let (_repo_dir, repo) = init_repo(&[("a.txt", "a0")]);
    std::fs::write(repo.join("a.txt"), "a1").unwrap();
    std::fs::write(repo.join("nuevo.txt"), "n").unwrap();

    let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let data = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("TYLLUAN_DATA_DIR", data.path()) };
    let snapshot = snapshot_before_claim(&repo, "deadbeef").expect("snapshot");
    unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
    drop(lock);

    assert!(snapshot.patch_path.exists());
    assert!(snapshot.meta_path.exists());
    assert_eq!(snapshot.base_commit, "deadbeef");
    assert_eq!(snapshot.untracked, vec!["nuevo.txt"]);
    let patch = std::fs::read_to_string(&snapshot.patch_path).unwrap();
    assert!(patch.contains("a.txt"), "el diff guarda a.txt: {patch}");
    let status = changed_files(&repo);
    assert!(status.iter().any(|p| p == "a.txt"));
    assert!(status.iter().any(|p| p == "nuevo.txt"));
}

#[test]
fn index_lock_is_a_clear_error_and_is_never_deleted() {
    let (_repo_dir, repo) = init_repo(&[("claimed/a.txt", "a0")]);
    let (_db_dir, store) = store();
    let claim = claim_task(&store, "t1", "claimed/**");
    std::fs::write(repo.join("claimed/a.txt"), "a1").unwrap();
    let git_dir = git_dir(&repo).unwrap();
    let lock = git_dir.join("index.lock");
    std::fs::write(&lock, b"").unwrap();

    let error = commit_paths(&store, &repo, &claim, "con lock").unwrap_err();
    assert!(error.to_string().contains("index.lock"), "{error}");
    assert!(lock.exists(), "el lock NO se borra nunca");
    std::fs::remove_file(&lock).unwrap();
    // Sin lock, el mismo commit funciona.
    commit_paths(&store, &repo, &claim, "sin lock").expect("commit tras quitar el lock");
}
