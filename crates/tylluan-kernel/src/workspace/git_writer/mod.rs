//! F9-1 (ADR-026): escritor de git unico con commit por rutas y parche de
//! recuperacion.
//!
//! Reglas duras:
//! - **nunca** `git add -A`, `git add .` ni `git commit -a`: solo las rutas
//!   reclamadas, pasadas como argumentos separados tras `--` (sin shell);
//! - **nunca** `reset`, `checkout`, `stash` ni borrado de ficheros: un reclamo
//!   caducado con cambios sucios deja un parche aplicable y la tarea pasa a
//!   `review`;
//! - una ruta fuera del reclamo, con `..`, absoluta o que salga del repositorio
//!   se rechaza **antes** de ejecutar git;
//! - `index.lock` presente = error claro tras reintentos acotados; el lock
//!   **no se borra nunca**.
//!
//! Los tests del modulo usan exclusivamente repositorios temporales (ver
//! `tests.rs`), nunca el checkout real.

use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use super::{Claim, WorkspaceStore};

/// Maximo de intentos ante un `index.lock` (nunca se borra el lock).
pub const MAX_INDEX_LOCK_RETRIES: u32 = 3;

/// Resultado de un commit por rutas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub commit: String,
    pub paths: Vec<String>,
}

/// Resultado de la foto previa a un reclamo (no toca el arbol).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotOutcome {
    pub patch_path: PathBuf,
    pub meta_path: PathBuf,
    pub base_commit: String,
    pub untracked: Vec<String>,
}

/// Directorio `<DATA_DIR>/workspace/patches` (respeta `TYLLUAN_DATA_DIR`).
pub fn patches_dir() -> PathBuf {
    crate::config::data_dir().join("workspace").join("patches")
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("ejecutando git {args:?} en {}", repo.display()))?;
    if !out.status.success() {
        bail!(
            "git {args:?} fallo en {}: {}{}",
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim(),
            if out.stdout.is_empty() {
                String::new()
            } else {
                format!(" / {}", String::from_utf8_lossy(&out.stdout).trim())
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn git_dir(repo: &Path) -> Result<PathBuf> {
    let raw = git(repo, &["rev-parse", "--git-dir"])?;
    let raw = raw.trim();
    let path = PathBuf::from(raw);
    Ok(if path.is_absolute() {
        path
    } else {
        repo.join(path)
    })
}

fn index_lock_path(repo: &Path) -> Result<PathBuf> {
    Ok(git_dir(repo)?.join("index.lock"))
}

fn is_lock_error(error: &anyhow::Error) -> bool {
    let text = error.to_string();
    text.contains("index.lock") || text.contains("Unable to create") && text.contains(".lock")
}

/// Rechaza una ruta que no sea relativa, no tenga `..`, ni salga del repo.
pub fn validate_repo_relative_path(repo: &Path, raw: &str) -> Result<PathBuf> {
    if raw.trim().is_empty() {
        bail!("ruta vacia");
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        bail!("ruta absoluta rechazada: {raw}");
    }
    let mut parts = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => parts += 1,
            Component::CurDir => {}
            Component::ParentDir => bail!("ruta con '..' rechazada: {raw}"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("ruta absoluta rechazada: {raw}")
            }
        }
    }
    if parts == 0 {
        bail!("ruta vacia o solo '.': {raw}");
    }
    let full = repo.join(path);
    // Si el fichero ya existe, que su ruta canonica no escape del repo
    // (protege ante enlaces simbolicos dentro del arbol).
    if full.exists() {
        let repo_canon = repo
            .canonicalize()
            .with_context(|| format!("canonicalizando repo {}", repo.display()))?;
        let full_canon = full
            .canonicalize()
            .with_context(|| format!("canonicalizando {}", full.display()))?;
        if !full_canon.starts_with(&repo_canon) {
            bail!("ruta fuera del repositorio rechazada: {raw}");
        }
    }
    Ok(path.to_path_buf())
}

/// ¿La ruta (siempre con `/`) casa con el glob de la tarea?
pub fn path_matches_glob(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let s: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match_segments(&p, &s)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => match_segments(&pattern[1..], path) || match_segments(pattern, &path[1..]),
        (Some(seg), Some(name)) => {
            segment_matches(seg, name) && match_segments(&pattern[1..], &path[1..])
        }
        _ => false,
    }
}

fn segment_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = name.chars().collect();
    fn go(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            Some(c) => !t.is_empty() && *c == t[0] && go(&p[1..], &t[1..]),
        }
    }
    go(&p, &t)
}

/// Rutas con cambios (incluye no rastreados) segun `git status --porcelain`.
fn changed_paths(repo: &Path) -> Result<Vec<String>> {
    let out = git(repo, &["status", "--porcelain"])?;
    let mut paths = Vec::new();
    for line in out.lines() {
        if line.len() < 4 {
            continue;
        }
        let mut rest = &line[3..];
        if let Some(idx) = rest.find(" -> ") {
            rest = &rest[idx + 4..];
        }
        let path = rest.trim();
        if path.is_empty() {
            continue;
        }
        let path = if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            path[1..path.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
        } else {
            path.to_string()
        };
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn untracked_paths(repo: &Path) -> Result<Vec<String>> {
    let out = git(repo, &["ls-files", "--others", "--exclude-standard"])?;
    let mut paths: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Valida y devuelve las rutas con cambios dentro del glob del reclamo.
fn changed_paths_inside(repo: &Path, glob: &str) -> Result<Vec<String>> {
    let mut inside = Vec::new();
    for path in changed_paths(repo)? {
        validate_repo_relative_path(repo, &path)?;
        if path_matches_glob(glob, &path) {
            inside.push(path);
        }
    }
    Ok(inside)
}

fn commit_with_retries(repo: &Path, paths: &[String], message: &str) -> Result<()> {
    let lock = index_lock_path(repo)?;
    for attempt in 1..=MAX_INDEX_LOCK_RETRIES {
        if lock.exists() {
            if attempt == MAX_INDEX_LOCK_RETRIES {
                bail!(
                    "index.lock presente en {}: otro proceso esta usando el indice; \
                     no se borra el lock (reintenta cuando termine)",
                    lock.display()
                );
            }
            sleep(Duration::from_millis(150 * u64::from(attempt)));
            continue;
        }
        let mut add: Vec<&str> = vec!["add", "--"];
        add.extend(paths.iter().map(String::as_str));
        match git(repo, &add) {
            Ok(_) => {}
            Err(error) if is_lock_error(&error) => {
                sleep(Duration::from_millis(150 * u64::from(attempt)));
                continue;
            }
            Err(error) => return Err(error),
        }
        let mut commit: Vec<&str> = vec!["commit", "-m", message, "--"];
        commit.extend(paths.iter().map(String::as_str));
        match git(repo, &commit) {
            Ok(_) => return Ok(()),
            Err(error) if is_lock_error(&error) => {
                sleep(Duration::from_millis(150 * u64::from(attempt)));
            }
            Err(error) => return Err(error),
        }
    }
    bail!("no se pudo commitear tras {MAX_INDEX_LOCK_RETRIES} intentos (index.lock)")
}

/// Commit **solo** de las rutas del reclamo vigente, con `-- <rutas>`.
///
/// Las demas rutas sucias del repositorio se quedan como estaban (ni `add -A`
/// ni `commit -a`). Valida token/epoca/arrendamiento contra la base de F9-0 y
/// rechaza cualquier ruta fuera del glob reclamado, con `..`, absoluta o que
/// escape del repo.
pub fn commit_paths(
    store: &WorkspaceStore,
    repo: &Path,
    claim: &Claim,
    message: &str,
) -> Result<CommitOutcome> {
    if message.trim().is_empty() {
        bail!("mensaje de commit vacio");
    }
    if !store.claim_is_current(claim, now_ms())? {
        bail!(
            "reclamo no vigente para {} (token/epoca/arrendamiento): no se toca git",
            claim.task_id
        );
    }
    let glob = store
        .claim_path_glob(&claim.task_id, claim.epoch)?
        .with_context(|| format!("no encuentro el glob del reclamo {}#{}", claim.task_id, claim.epoch))?;
    let paths = changed_paths_inside(repo, &glob)?;
    if paths.is_empty() {
        bail!(
            "no hay cambios dentro del reclamo {} (glob {glob:?}); no se ejecuta git",
            claim.task_id
        );
    }
    commit_with_retries(repo, &paths, message)?;
    let commit = git(repo, &["rev-parse", "HEAD"])?.trim().to_string();
    Ok(CommitOutcome { commit, paths })
}

/// Foto previa: apunta `base_commit` y guarda los cambios sin confirmar
/// (`git diff` + lista de no rastreados) en `<data>/workspace/patches/`, sin
/// tocar el arbol de trabajo.
pub fn snapshot_before_claim(repo: &Path, base_commit: &str) -> Result<SnapshotOutcome> {
    let head = git(repo, &["rev-parse", "HEAD"])?.trim().to_string();
    let diff = git(repo, &["diff", "HEAD", "--binary"])?;
    let untracked = untracked_paths(repo)?;
    let dir = patches_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creando {}", dir.display()))?;
    let ts = now_ms();
    let patch_path = dir.join(format!("snapshot_{ts}.patch"));
    std::fs::write(&patch_path, diff.as_bytes())
        .with_context(|| format!("escribiendo {}", patch_path.display()))?;
    let meta_path = dir.join(format!("snapshot_{ts}.json"));
    let meta = json!({
        "kind": "snapshot_before_claim",
        "base_commit": base_commit,
        "head": head,
        "untracked": untracked,
        "created_at_ms": ts,
    });
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("escribiendo {}", meta_path.display()))?;
    Ok(SnapshotOutcome {
        patch_path,
        meta_path,
        base_commit: base_commit.to_string(),
        untracked,
    })
}

/// Parche de recuperacion **solo** de las rutas del reclamo (tracked modificadas
/// y no rastreadas dentro del glob). No toca el arbol: no hay reset/checkout/
/// stash/borrado. Deja la tarea en `review` (transicion de F9-0) y devuelve la
/// ruta del parche, que debe aplicar con `git apply --check` sobre un clon
/// limpio de la base.
pub fn recovery_patch(store: &WorkspaceStore, repo: &Path, claim: &Claim) -> Result<PathBuf> {
    let glob = store
        .claim_path_glob(&claim.task_id, claim.epoch)?
        .with_context(|| format!("no encuentro el glob del reclamo {}#{}", claim.task_id, claim.epoch))?;
    let tracked = changed_paths_inside(repo, &glob)?;
    let untracked: Vec<String> = untracked_paths(repo)?
        .into_iter()
        .filter(|path| path_matches_glob(&glob, path))
        .collect();
    if tracked.is_empty() && untracked.is_empty() {
        bail!(
            "no hay cambios sucios dentro del reclamo {} (glob {glob:?}); no hay parche",
            claim.task_id
        );
    }
    // Los no rastreados entran en `git diff` con intent-to-add (solo indice, no
    // arbol). No se commitea ni se borra nada.
    if !untracked.is_empty() {
        for path in &untracked {
            validate_repo_relative_path(repo, path)?;
        }
        let mut add: Vec<&str> = vec!["add", "-N", "--"];
        add.extend(untracked.iter().map(String::as_str));
        git(repo, &add).with_context(|| "git add -N de los no rastreados del reclamo")?;
    }
    let mut all = tracked;
    all.extend(untracked.iter().cloned());
    all.sort();
    all.dedup();
    let mut args: Vec<&str> = vec!["diff", "--binary", "--"];
    args.extend(all.iter().map(String::as_str));
    let patch = git(repo, &args)?;

    let dir = patches_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creando {}", dir.display()))?;
    let ts = now_ms();
    let safe_task = claim
        .task_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let path = dir.join(format!("recovery_{safe_task}_{}_{ts}.patch", claim.epoch));
    std::fs::write(&path, patch.as_bytes())
        .with_context(|| format!("escribiendo {}", path.display()))?;
    store.mark_task_review(&claim.task_id, ts)?;
    Ok(path)
}

#[cfg(test)]
mod tests;
