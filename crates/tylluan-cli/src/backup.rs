//! `tylluan-cli backup` / `tylluan-cli restore` — hot-consistent backup of
//! hub databases without stopping the kernel (T915, roadmap "backup/restore
//! de SilvaDB sin CLI").
//!
//! Backup uses SQLite `VACUUM INTO`: the same consistency guarantees as the
//! `sqlite3 .backup` command, but without depending on the `sqlite3` binary
//! (Windows does not ship it). It works while the kernel is live — the source
//! is only read, WAL included.
//!
//! T-DSH-156 (H-60): the default source list covers the 12 databases the
//! kernel keeps in DATA_DIR (agent_profiles, audit, contracts, curriculum,
//! jobs, journal, mailbox, peers, pending_dispatches, silva, task_context,
//! tylluan). Before this, only 5 were backed up. `--with-secrets` adds the two
//! sensitive files (`identity.key`, `oauth_jwt_secret`); they are copied
//! verbatim (not SQLite) and only to a trusted destination.
//!
//! Restore refuses to run while the kernel that would use the target is up:
//! SQLite has no safe hot restore (a live kernel would keep writing to the
//! file we replace, and a stale `-wal` sidecar next to the restored file
//! corrupts it). With `--target-dir` the guard only checks the given `--port`
//! (a kernel of ANOTHER data dir, e.g. production on :47004, does not block a
//! restore to a different directory). Before touching anything, restore
//! snapshots the current databases to `pre_restore_<timestamp>/`.

use anyhow::{bail, Context, Result};
use chrono::Local;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};

pub(crate) const MANIFEST_NAME: &str = "backup-manifest.json";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// T-DSH-156 (H-60): the 12 databases the kernel keeps in DATA_DIR. The list
/// mirrors the inventory measured in T-DSH-133 ("12/12 bases").
pub(crate) const DATA_DBS: &[&str] = &[
    "agent_profiles.db",
    "audit.db",
    "contracts.db",
    "curriculum.db",
    "jobs.db",
    "journal.db",
    "mailbox.db",
    "peers.db",
    "pending_dispatches.db",
    "silva.db",
    "task_context.db",
    "tylluan.db",
];

/// T-DSH-156: sensitive files, copied verbatim and only with `--with-secrets`.
pub(crate) const SECRET_FILES: &[&str] = &["identity.key", "oauth_jwt_secret"];

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub created_at: String,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ManifestEntry {
    /// Plain file name inside the backup dir (no path separators).
    pub name: String,
    /// Where the database came from (informational; restore targets this
    /// unless `--target-dir` is given).
    pub source: String,
    pub size: u64,
    /// T-DSH-156: true for `identity.key` / `oauth_jwt_secret` (copied only
    /// with `--with-secrets`, not SQLite). Old manifests default to false.
    #[serde(default)]
    pub secret: bool,
}

/// Escape a string for inlining as a SQLite single-quoted literal.
fn escape_sq_literal(s: &str) -> String {
    s.replace('\'', "''")
}

/// True iff the file starts with the SQLite 3 magic header.
/// Files that do not (e.g. SQLCipher-encrypted builds) are rejected with an
/// explicit error — same limitation the external `sqlite3` CLI would have.
fn has_sqlite_header(path: &Path) -> Result<bool> {
    let mut f =
        fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut buf = [0u8; 16];
    let n = f.read(&mut buf)?;
    Ok(n == 16 && buf[..] == *SQLITE_MAGIC)
}

fn open_source(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("opening database {}", path.display()))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}

/// `PRAGMA quick_check` — returns "ok" when the file is structurally sound.
fn quick_check(path: &Path) -> Result<String> {
    let conn = open_source(path)?;
    conn.query_row("PRAGMA quick_check;", [], |row| row.get(0))
        .with_context(|| format!("quick_check on {}", path.display()))
}

/// Read `[memory].db_path` from tylluan.toml. `Ok(None)` when the file or the
/// key is absent (then the caller falls back to `./data/tylluan.db`).
fn load_memory_db_path(toml_path: &Path) -> Result<Option<PathBuf>> {
    let text = match fs::read_to_string(toml_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", toml_path.display())),
    };
    let parsed: toml::Value =
        toml::from_str(&text).with_context(|| format!("parsing {}", toml_path.display()))?;
    Ok(parsed
        .get("memory")
        .and_then(|m| m.get("db_path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from))
}

/// Read `[silva].db_path` from tylluan.toml. `Ok(None)` when the file or the
/// key is absent (then the caller falls back to `./data/silva.db`).
fn load_silva_db_path(toml_path: &Path) -> Result<Option<PathBuf>> {
    let text = match fs::read_to_string(toml_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", toml_path.display())),
    };
    let parsed: toml::Value =
        toml::from_str(&text).with_context(|| format!("parsing {}", toml_path.display()))?;
    Ok(parsed
        .get("silva")
        .and_then(|m| m.get("db_path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from))
}

fn data_dir_of(memory_db: &Path) -> PathBuf {
    memory_db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("./data"))
}

fn resolve_sources(extras: &[String], with_secrets: bool) -> Result<Vec<PathBuf>> {
    resolve_sources_from(Path::new("tylluan.toml"), extras, with_secrets)
}

/// T-DSH-158: el kernel honra `TYLLUAN_DATA_DIR` para TODAS las bases del hub
/// (incluida `journal.db`, que abre en `<DATA_DIR>/journal.db`). El backup debe
/// resolver el MISMO DATA_DIR: si el env var esta definido, manda sobre
/// `tylluan.toml` (que puede tener rutas relativas al cwd y no ser el DATA_DIR
/// real del kernel).
fn data_dir_env() -> Option<PathBuf> {
    std::env::var("TYLLUAN_DATA_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

/// T-DSH-159: DATA_DIR efectivo del kernel (env si esta, si no el derivado del
/// toml). Se usa para el snapshot in situ y para localizar su fichero PID.
fn resolve_data_dir(toml_path: &Path) -> Result<PathBuf> {
    if let Some(base) = data_dir_env() {
        return Ok(base);
    }
    let silva = load_silva_db_path(toml_path)?
        .unwrap_or_else(|| PathBuf::from("./data/silva.db"));
    Ok(data_dir_of(&silva))
}

/// Default sources: `[silva].db_path` and `[memory].db_path` from tylluan.toml
/// (fallbacks `./data/silva.db` / `./data/tylluan.db`) plus the 12 canonical
/// databases of the data dir (T-DSH-156). Extras (`--db`) are names relative
/// to that data dir, or full paths. `--with-secrets` appends `SECRET_FILES`.
fn resolve_sources_from(
    toml_path: &Path,
    extras: &[String],
    with_secrets: bool,
) -> Result<Vec<PathBuf>> {
    // T-DSH-158: misma resolucion que el kernel: TYLLUAN_DATA_DIR manda y
    // TODAS las bases (incluida journal.db) salen de ese DATA_DIR.
    let (silva, memory, data_dir) = match data_dir_env() {
        Some(base) => (base.join("silva.db"), base.join("tylluan.db"), base),
        None => {
            let silva = load_silva_db_path(toml_path)?
                .unwrap_or_else(|| PathBuf::from("./data/silva.db"));
            let memory = load_memory_db_path(toml_path)?
                .unwrap_or_else(|| PathBuf::from("./data/tylluan.db"));
            let data_dir = data_dir_of(&silva);
            (silva, memory, data_dir)
        }
    };
    let mut sources: Vec<PathBuf> = Vec::new();
    for candidate in [silva, memory] {
        if !sources.contains(&candidate) {
            sources.push(candidate);
        }
    }
    // T-DSH-156 (H-60): las 12 bases canonicas del DATA_DIR (antes solo 5).
    for name in DATA_DBS {
        let candidate = data_dir.join(name);
        if !sources.contains(&candidate) {
            sources.push(candidate);
        }
    }
    for extra in extras {
        let p = PathBuf::from(extra);
        let resolved =
            if p.is_absolute() || p.components().count() > 1 { p } else { data_dir.join(&p) };
        if !sources.contains(&resolved) {
            sources.push(resolved);
        }
    }
    if with_secrets {
        for name in SECRET_FILES {
            let candidate = data_dir.join(name);
            if !sources.contains(&candidate) {
                sources.push(candidate);
            }
        }
    }
    Ok(sources)
}

/// Back up `sources` into `dir` and write the manifest.
/// Missing or non-SQLite database files are skipped with a warning; a failing
/// `quick_check` on the source is a warning too (a rescue copy of a damaged
/// database is better than none — the restore side re-validates). Sensitive
/// files (`SECRET_FILES`) are copied verbatim.
pub(crate) fn backup_files(dir: &Path, sources: &[PathBuf]) -> Result<Manifest> {
    fs::create_dir_all(dir)
        .with_context(|| format!("creating backup dir {}", dir.display()))?;

    let mut seen_names: Vec<String> = Vec::new();
    let mut entries: Vec<ManifestEntry> = Vec::new();

    for src in sources {
        let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if seen_names.contains(&name) {
            bail!(
                "duplicate database file name `{name}` among the sources — \
                 back them up separately or rename one"
            );
        }
        if !src.exists() {
            println!("  ⚠️  {} not found, skipping", src.display());
            continue;
        }
        let secret = SECRET_FILES.contains(&name.as_str());
        if !secret {
            if !has_sqlite_header(src)? {
                println!(
                    "  ⚠️  {} is not a plain SQLite file (encrypted build?) — skipping",
                    src.display()
                );
                continue;
            }
            match quick_check(src) {
                Ok(status) if status == "ok" => {}
                Ok(status) => println!(
                    "  ⚠️  {name} quick_check: {status} — attempting rescue backup anyway"
                ),
                Err(e) => println!("  ⚠️  {name} quick_check failed: {e} — attempting rescue backup anyway"),
            }
        }

        let dest = dir.join(&name);
        let tmp = dir.join(format!(".{name}.tmp"));
        if tmp.exists() {
            fs::remove_file(&tmp)
                .with_context(|| format!("removing stale {}", tmp.display()))?;
        }
        if secret {
            fs::copy(src, &tmp).with_context(|| {
                format!("copying sensitive file {}", src.display())
            })?;
        } else {
            let conn = open_source(src)?;
            let sql = format!("VACUUM INTO '{}'", escape_sq_literal(&tmp.to_string_lossy()));
            conn.execute_batch(&sql)
                .with_context(|| format!("VACUUM INTO for {}", src.display()))?;
        }
        if dest.exists() {
            fs::remove_file(&dest)
                .with_context(|| format!("replacing existing {}", dest.display()))?;
        }
        fs::rename(&tmp, &dest)
            .with_context(|| format!("renaming {} into place", tmp.display()))?;

        let size = fs::metadata(&dest)
            .with_context(|| format!("stat {}", dest.display()))?
            .len();
        let kind = if secret { "sensible" } else { "db" };
        println!("  ✅ {name} ({kind}, {size} bytes)");
        entries.push(ManifestEntry {
            name: name.clone(),
            source: src.display().to_string(),
            size,
            secret,
        });
        seen_names.push(name);
    }

    if entries.is_empty() {
        bail!("nothing backed up — no usable source database found in {sources:?}");
    }

    let manifest = Manifest { created_at: Local::now().to_rfc3339(), entries };
    fs::write(
        dir.join(MANIFEST_NAME),
        serde_json::to_string_pretty(&manifest).context("serializing manifest")?,
    )
    .with_context(|| format!("writing manifest in {}", dir.display()))?;
    Ok(manifest)
}

pub(crate) fn run_backup(dir: &Path, extras: &[String], with_secrets: bool) -> Result<()> {
    if with_secrets {
        println!(
            "🔐 --with-secrets: se incluiran identity.key y oauth_jwt_secret. \
             Copia el backup solo a un destino de confianza (nunca a un repo)."
        );
    }
    let sources = resolve_sources(extras, with_secrets)?;
    println!("🗄️  Backing up {} database source(s) → {}", sources.len(), dir.display());
    let manifest = backup_files(dir, &sources)?;
    println!(
        "✅ {} database(s) backed up; manifest at {}",
        manifest.entries.len(),
        dir.join(MANIFEST_NAME).display()
    );
    println!("💡 Restore requires a stopped kernel: tylluan-cli restore {}", dir.display());
    Ok(())
}

/// A restore plan: each backup file mapped to its target path (the recorded
/// source location, or `--target-dir` when given), validated before anything
/// is replaced.
#[derive(Debug)]
pub(crate) struct RestorePlan {
    pub steps: Vec<(PathBuf, PathBuf)>,
    pub pre_dir: PathBuf,
    /// T-DSH-156: el plan incluye ficheros sensibles (identity.key / oauth).
    pub has_secrets: bool,
}

/// Read + validate the manifest and every backup file: name traversal guard,
/// presence, size match, SQLite header and fresh `quick_check` for databases
/// (sensitive files are only checked for presence/size).
pub(crate) fn plan_restore(dir: &Path, target_dir: Option<&Path>) -> Result<RestorePlan> {
    let mpath = dir.join(MANIFEST_NAME);
    let mtext = fs::read_to_string(&mpath).with_context(|| {
        format!("{} is not a tylluan backup (missing {MANIFEST_NAME})", dir.display())
    })?;
    let manifest: Manifest =
        serde_json::from_str(&mtext).context("parsing backup manifest")?;
    if manifest.entries.is_empty() {
        bail!("backup manifest lists no databases");
    }

    let mut steps: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut has_secrets = false;
    for entry in &manifest.entries {
        let name_path = Path::new(&entry.name);
        if entry.name.is_empty()
            || entry.name.contains("..")
            || name_path.components().count() != 1
        {
            bail!("manifest entry with unsafe name: {:?}", entry.name);
        }
        let src = dir.join(&entry.name);
        if !src.exists() {
            bail!("backup file missing: {}", src.display());
        }
        let actual = fs::metadata(&src)
            .with_context(|| format!("stat {}", src.display()))?
            .len();
        if actual != entry.size {
            bail!(
                "size mismatch for {}: manifest says {} bytes, file has {} \
                 (truncated copy?)",
                entry.name,
                entry.size,
                actual
            );
        }
        if entry.secret || SECRET_FILES.contains(&entry.name.as_str()) {
            has_secrets = true;
        } else {
            if !has_sqlite_header(&src)? {
                bail!("{} is not a plain SQLite file (encrypted or corrupt?)", entry.name);
            }
            let status = quick_check(&src)?;
            if status != "ok" {
                bail!("{} failed quick_check: {status}", entry.name);
            }
        }
        let target = match target_dir {
            Some(base) => base.join(&entry.name),
            None => PathBuf::from(&entry.source),
        };
        steps.push((src, target));
    }

    let ts = Local::now().format("%Y%m%d_%H%M%S");
    let pre_dir = match target_dir {
        Some(base) => base.join(format!("pre_restore_{ts}")),
        None => resolve_data_dir(Path::new("tylluan.toml"))?
            .join(format!("pre_restore_{ts}")),
    };
    Ok(RestorePlan { steps, pre_dir, has_secrets })
}

/// HTTP `/health` probe for the kernel that would own the target port.
async fn health_running(port: u16) -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("building HTTP client")?;
    let url = format!("http://127.0.0.1:{port}/health");
    if let Ok(resp) = client.get(&url).send().await
        && resp.status().is_success()
    {
        return Ok(Some(format!("health endpoint {url} is responding")));
    }
    Ok(None)
}

/// Process-table probe (`tylluan-nexus`). Solo lo usa `kernel_running` en
/// tests: `run_restore` ya no hace barrido global (T-DSH-159: solo el kernel
/// de ESTE DATA_DIR, via fichero PID, bloquea el restore in situ).
#[cfg(test)]
fn kernel_process_running() -> Option<String> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for (pid, process) in sys.processes() {
        if process.name().to_string_lossy().contains("tylluan-nexus") {
            return Some(format!("kernel process {pid} ({:?}) is running", process.name()));
        }
    }
    None
}

/// T-DSH-159: fichero PID del kernel para un DATA_DIR (mismo nombre que
/// `stop.rs`), para poder dar el PID en el mensaje de rechazo del restore.
fn kernel_pid_file(data_dir: &Path) -> PathBuf {
    data_dir.join(crate::stop::PID_FILE_NAME)
}

fn read_kernel_pid(data_dir: &Path) -> Option<u32> {
    crate::stop::read_pid_file(&kernel_pid_file(data_dir))
}

fn pid_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(
        ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
    );
    sys.process(sysinfo::Pid::from_u32(pid)).is_some()
}

/// Texto para el mensaje: "PID <n>" o "PID no identificado".
fn describe_pid(data_dir: &Path) -> String {
    match read_kernel_pid(data_dir) {
        Some(pid) => format!("PID {pid}"),
        None => "PID no identificado".to_string(),
    }
}

/// Refuse the restore while the kernel is up. Two signals, mirroring
/// `tylluan-cli status` / `tylluan-cli stop`: the health endpoint and the
/// process table (`tylluan-nexus`). Solo lo usan los tests; `run_restore`
/// separa las dos senales (con `--target-dir` no mira la tabla de procesos).
#[cfg(test)]
pub(crate) async fn kernel_running(port: u16) -> Result<Option<String>> {
    if let Some(reason) = health_running(port).await? {
        return Ok(Some(reason));
    }
    Ok(kernel_process_running())
}

/// Execute a validated plan: snapshot every existing target (database +
/// `-wal`, so an unclean-shutdown WAL is not lost from the snapshot) into
/// `pre_dir`, then replace targets via tmp+rename, dropping stale
/// `-wal`/`-shm` sidecars so the old WAL cannot corrupt the restored file.
pub(crate) fn execute_restore(plan: &RestorePlan) -> Result<PathBuf> {
    let existing: Vec<&(PathBuf, PathBuf)> =
        plan.steps.iter().filter(|(_, target)| target.exists()).collect();
    if !existing.is_empty() {
        fs::create_dir_all(&plan.pre_dir).with_context(|| {
            format!("creating pre-restore dir {}", plan.pre_dir.display())
        })?;
        for (_, target) in &existing {
            let name = target.file_name().unwrap_or_default();
            fs::copy(target, plan.pre_dir.join(name)).with_context(|| {
                format!("snapshotting current {}", target.display())
            })?;
            let mut wal = target.as_os_str().to_owned();
            wal.push("-wal");
            let wal = PathBuf::from(wal);
            if wal.exists() {
                let wal_name = wal.file_name().unwrap_or_default();
                fs::copy(&wal, plan.pre_dir.join(wal_name)).with_context(|| {
                    format!("snapshotting WAL {}", wal.display())
                })?;
            }
        }
    }

    for (src, target) in &plan.steps {
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("creating target dir {}", parent.display()))?;

        for suffix in ["-wal", "-shm"] {
            let mut sidecar = target.as_os_str().to_owned();
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                fs::remove_file(&sidecar).with_context(|| {
                    format!("removing stale sidecar {}", sidecar.display())
                })?;
            }
        }
        if target.exists() {
            fs::remove_file(target)
                .with_context(|| format!("removing current {}", target.display()))?;
        }
        let name = target.file_name().unwrap_or_default();
        let tmp = parent.join(format!(".restore-{}", name.to_string_lossy()));
        if tmp.exists() {
            fs::remove_file(&tmp)
                .with_context(|| format!("removing stale {}", tmp.display()))?;
        }
        fs::copy(src, &tmp)
            .with_context(|| format!("copying {} into place", src.display()))?;
        fs::rename(&tmp, target)
            .with_context(|| format!("renaming {} into place", target.display()))?;
        println!("  ✅ {} restored → {}", target.file_name().unwrap_or_default().to_string_lossy(), target.display());
    }
    Ok(plan.pre_dir.clone())
}

/// T-DSH-156: `target_dir` redirige los destinos (restore fuera de sitio) y
/// `port` es el puerto del kernel que se comprobara; un kernel de otro DATA_DIR
/// (produccion) no bloquea la restauracion a un directorio distinto.
pub(crate) async fn run_restore(
    dir: &Path,
    port: u16,
    target_dir: Option<&Path>,
) -> Result<()> {
    let in_place = target_dir.is_none();
    // T-DSH-159: restore in situ seguro. Se comprueba el kernel de ESTE
    // DATA_DIR (su fichero PID y el /health del puerto indicado); un kernel de
    // OTRO DATA_DIR no escribe estos ficheros y no debe bloquear el restore.
    let data_dir = resolve_data_dir(Path::new("tylluan.toml"))?;
    if let Some(reason) = health_running(port).await? {
        let pid = describe_pid(&data_dir);
        bail!(
            "rechazando restore in situ: el kernel parece estar vivo ({reason}). \
             Puerto comprobado: {port}. {pid}. \
             SQLite no se puede restaurar en caliente de forma segura; \
             paralo primero con: tylluan-cli stop --port {port}"
        );
    }
    if in_place
        && let Some(pid) = read_kernel_pid(&data_dir)
        && pid_alive(pid)
    {
        bail!(
            "rechazando restore in situ: el kernel de este DATA_DIR sigue vivo \
             (PID {pid}, puerto {port}). Paralo primero con: \
             tylluan-cli stop --port {port}"
        );
    }
    let plan = plan_restore(dir, target_dir)?;
    println!("♻️  Restoring {} database(s) from {}", plan.steps.len(), dir.display());
    if plan.has_secrets {
        println!(
            "  🔐 el backup incluye ficheros sensibles (identity.key / oauth_jwt_secret): \
             restaura solo a un destino de confianza"
        );
    }
    let pre_dir = execute_restore(&plan)?;
    println!("✅ Restore complete.");
    if pre_dir.exists() {
        println!(
            "📁 Previous state snapshotted at {} — keep it until you have verified the hub.",
            pre_dir.display()
        );
    }
    println!("👉 Start the kernel: tylluan-cli start");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_db(path: &Path, marker: &str) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (v TEXT); DELETE FROM t;").unwrap();
        conn.execute("INSERT INTO t (v) VALUES (?1)", [marker]).unwrap();
    }

    fn read_marker(path: &Path) -> String {
        let conn = Connection::open(path).unwrap();
        conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap()
    }

    fn write_toml(dir: &Path) {
        // TOML en Windows: barras normales para no escapar `\` como secuencia.
        let base = dir.display().to_string().replace('\\', "/");
        fs::write(
            dir.join("tylluan.toml"),
            format!("[silva]\ndb_path = \"{base}/silva.db\"\n[memory]\ndb_path = \"{base}/tylluan.db\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn escapes_single_quotes_for_vacuum_into() {
        assert_eq!(escape_sq_literal("a'b"), "a''b");
        assert_eq!(escape_sq_literal("plain/path.db"), "plain/path.db");
    }

    #[test]
    fn header_detection() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.db");
        make_db(&good, "x");
        assert!(has_sqlite_header(&good).unwrap());

        let short = dir.path().join("short.db");
        fs::write(&short, b"SQLite").unwrap();
        assert!(!has_sqlite_header(&short).unwrap());

        let garbage = dir.path().join("garbage.db");
        fs::write(&garbage, b"not a database at all").unwrap();
        assert!(!has_sqlite_header(&garbage).unwrap());
    }

    #[test]
    fn backup_writes_valid_copies_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let src_a = dir.path().join("tylluan.db");
        let src_b = dir.path().join("peers.db");
        make_db(&src_a, "alpha");
        make_db(&src_b, "beta");

        let out = dir.path().join("out");
        let manifest = backup_files(&out, &[src_a.clone(), src_b.clone()]).unwrap();
        assert_eq!(manifest.entries.len(), 2);

        let mtext = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        let parsed: Manifest = serde_json::from_str(&mtext).unwrap();
        assert_eq!(parsed, manifest);

        // Copies are real databases with the data intact.
        let copy = out.join("tylluan.db");
        assert_eq!(read_marker(&copy), "alpha");
        assert_eq!(fs::metadata(&copy).unwrap().len(), manifest.entries[0].size);
        // No temp litter.
        assert!(!out.join(".tylluan.db.tmp").exists());
    }

    #[test]
    fn backup_skips_missing_and_non_sqlite_sources() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.db");
        make_db(&good, "ok");
        let missing = dir.path().join("missing.db");
        let encrypted = dir.path().join("encrypted.db");
        fs::write(&encrypted, [0x33u8; 64]).unwrap(); // random header, no magic

        let out = dir.path().join("out");
        let manifest = backup_files(&out, &[good, missing, encrypted]).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].name, "good.db");
    }

    #[test]
    fn backup_rejects_duplicate_basenames() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let dup_a = a.join("peers.db");
        let dup_b = b.join("peers.db");
        make_db(&dup_a, "1");
        make_db(&dup_b, "2");

        let out = dir.path().join("out");
        let err = backup_files(&out, &[dup_a, dup_b]).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {err}");
    }

    #[test]
    fn resolve_sources_covers_all_twelve_databases() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(dir.path());
        let toml = dir.path().join("tylluan.toml");
        let sources = resolve_sources_from(&toml, &[], false).unwrap();
        let names: Vec<String> = sources
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        for expected in DATA_DBS {
            assert!(
                names.iter().any(|n| n == expected),
                "falta {expected} en las fuentes por defecto: {names:?}"
            );
        }
        assert!(
            !names.iter().any(|n| SECRET_FILES.contains(&n.as_str())),
            "sin --with-secrets no deben aparecer secretos: {names:?}"
        );
    }

    #[test]
    fn with_secrets_adds_identity_and_oauth_secret() {
        let dir = tempfile::tempdir().unwrap();
        write_toml(dir.path());
        let toml = dir.path().join("tylluan.toml");
        let sources = resolve_sources_from(&toml, &[], true).unwrap();
        let names: Vec<String> = sources
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        for secret in SECRET_FILES {
            assert!(names.iter().any(|n| n == secret), "falta {secret}: {names:?}");
        }
    }

    #[test]
    fn backup_copies_secret_files_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let identity = dir.path().join("identity.key");
        fs::write(&identity, b"dsh-identity-bytes").unwrap();
        let oauth = dir.path().join("oauth_jwt_secret");
        fs::write(&oauth, b"ab".repeat(32)).unwrap();

        let out = dir.path().join("out");
        let manifest = backup_files(&out, &[identity, oauth]).unwrap();
        assert_eq!(manifest.entries.len(), 2);
        assert!(manifest.entries.iter().all(|e| e.secret));
        assert_eq!(fs::read(out.join("identity.key")).unwrap(), b"dsh-identity-bytes");

        // plan_restore accepts secret entries (no SQLite quick_check).
        let plan = plan_restore(&out, Some(dir.path())).unwrap();
        assert!(plan.has_secrets);
        assert_eq!(plan.steps.len(), 2);
    }

    #[test]
    fn restore_rejects_bad_manifests_and_truncated_files() {
        let dir = tempfile::tempdir().unwrap();

        // No manifest at all.
        let err = plan_restore(dir.path(), None).unwrap_err();
        assert!(err.to_string().contains("not a tylluan backup"), "got: {err}");

        // Build a real backup, then truncate one file.
        let src = dir.path().join("tylluan.db");
        make_db(&src, "payload");
        let out = dir.path().join("out");
        backup_files(&out, std::slice::from_ref(&src)).unwrap();

        let db = out.join("tylluan.db");
        let full_len = fs::metadata(&db).unwrap().len();
        let data = fs::read(&db).unwrap();
        fs::write(&db, &data[..full_len as usize - 1]).unwrap();
        let err = plan_restore(&out, None).unwrap_err();
        assert!(err.to_string().contains("size mismatch"), "got: {err}");

        // Restore requires path-safe names.
        let m: Manifest = serde_json::from_str(&fs::read_to_string(out.join(MANIFEST_NAME)).unwrap()).unwrap();
        let evil = Manifest {
            created_at: m.created_at,
            entries: vec![ManifestEntry {
                name: "..\\evil.db".into(),
                source: "x".into(),
                size: 1,
                secret: false,
            }],
        };
        fs::write(out.join(MANIFEST_NAME), serde_json::to_string(&evil).unwrap()).unwrap();
        let err = plan_restore(&out, None).unwrap_err();
        assert!(err.to_string().contains("unsafe name"), "got: {err}");
    }

    #[test]
    fn plan_restore_target_dir_redirects_targets() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tylluan.db");
        make_db(&src, "state");
        let out = dir.path().join("out");
        backup_files(&out, std::slice::from_ref(&src)).unwrap();

        let target = dir.path().join("restored");
        let plan = plan_restore(&out, Some(&target)).unwrap();
        assert_eq!(plan.steps[0].1, target.join("tylluan.db"));
        assert!(plan.pre_dir.starts_with(&target));
    }

    #[test]
    fn execute_replaces_targets_and_snapshots_previous_state() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tylluan.db");
        make_db(&src, "old-state");

        let out = dir.path().join("out");
        backup_files(&out, std::slice::from_ref(&src)).unwrap();

        // Simulate live state diverging after the backup + a stale WAL pair.
        make_db(&src, "new-state");
        let mut wal = src.as_os_str().to_owned();
        wal.push("-wal");
        let wal = PathBuf::from(wal);
        fs::write(&wal, b"stale-wal-bytes").unwrap();
        let mut shm = src.as_os_str().to_owned();
        shm.push("-shm");
        let shm = PathBuf::from(shm);
        fs::write(&shm, b"stale-shm-bytes").unwrap();

        let plan = plan_restore(&out, None).unwrap();
        let pre_dir = execute_restore(&plan).unwrap();

        // Target now holds the backup content; stale sidecars are gone.
        assert_eq!(read_marker(&src), "old-state");
        assert!(!wal.exists());
        assert!(!shm.exists());

        // Previous state was snapshotted (db + wal) before replacement.
        // The wal assertion MUST run before `read_marker` opens the copy:
        // SQLite unlinks a stray `-wal` next to a rollback-mode database on
        // open, which would hide the snapshot we are asserting on.
        assert_eq!(fs::read(pre_dir.join("tylluan.db-wal")).unwrap(), b"stale-wal-bytes");
        assert_eq!(read_marker(&pre_dir.join("tylluan.db")), "new-state");
    }

    #[tokio::test]
    async fn kernel_running_is_none_on_dead_endpoint() {
        // Nothing listens on the test port: no health, no `tylluan-nexus`
        // process in the test environment → restore is allowed.
        let found = kernel_running(9).await.unwrap();
        // Port 9 (discard) — if something unexpectedly listens there, skip.
        if found.as_deref().is_some_and(|r| r.contains("health endpoint")) {
            return;
        }
        // Process check may legitimately find a live kernel on a dev box.
        if let Some(reason) = found {
            assert!(reason.contains("kernel process"), "unexpected: {reason}");
        }
    }

    #[test]
    fn resolve_sources_includes_silva_db_by_default() {
        let sources = resolve_sources(&[], false).unwrap();
        let file_names: Vec<String> = sources
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert!(
            file_names.contains(&"silva.db".to_string()),
            "default sources must include silva.db, got: {file_names:?}"
        );
        assert!(
            file_names.contains(&"peers.db".to_string()),
            "default sources must include peers.db, got: {file_names:?}"
        );
    }
}
