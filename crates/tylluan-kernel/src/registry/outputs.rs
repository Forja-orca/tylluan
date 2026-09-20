//! Guild outputs ledger (bwc-d0fb0812) — the kernel-side index of artifacts
//! that guilds write under `data/outputs/`.
//!
//! Ownership split, deliberately rigid:
//! - **The guild owns the bytes.** The kernel never moves, rewrites, or
//!   executes anything a guild produced. Artifacts stay wherever the guild
//!   wrote them (comfy: `data/outputs/comfy/`, flat).
//! - **The kernel owns the index.** After each guild tool call that produced
//!   artifacts, the kernel writes
//!   `data/outputs/<guild>/<run_id>/manifest.json` — a small ledger entry
//!   referencing the files it observed, with sha256 integrity digests.
//!   The run directory contains ONLY the manifest.
//!
//! Discovery modes, in order of trust:
//! 1. **Window diff-scan** (covers today's guilds, e.g. comfy which returns
//!    markdown): any file under the guild's output dir with an mtime inside
//!    the call window (1s margin for coarse filesystem mtime granularity).
//! 2. **Claimed outputs** (opt-in for future guilds): a tool result that
//!    parses as JSON with a `"tylluan_outputs": [path, ...]` array, each path
//!    verified to live under `data/outputs/<guild>/` before indexing. A
//!    claimed path outside the guild's namespace is ignored (and logged) —
//!    outputs are data, never a way to make the kernel acknowledge arbitrary
//!    filesystem locations.
//!
//! Runs with zero artifacts leave no trace: no run dir, no manifest (text
//! tool calls must not spam the ledger). `delivery_status` is `ok` when the
//! call succeeded, `partial` when artifacts exist but the call reported an
//! error.
//!
//! Path ownership: `outputs_root()` is the single resolver for the store
//! root (`TYLLUAN_OUTPUTS_DIR` env seam, default `data/outputs`). Write and
//! read paths MUST go through it — same seam asymmetry lesson as the audit
//! and confusion stores before this one.
//!
//! TTL pruning is opt-in (`TYLLUAN_OUTPUTS_TTL_SECS`, unset = never prune).
//! Pruning deletes a run dir AND the artifact files that manifest indexed,
//! but only files under the guild's own outputs dir — the operator who
//! enables a TTL accepts that indexed artifacts are not immortal.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Upper bound on indexed bytes per run. Files beyond this are left on disk
/// (the guild owns them) but stop being added to the manifest and the run is
/// marked `partial`. Prevents one runaway guild from building an unbounded
/// ledger entry on a toaster-class host.
pub const MAX_INDEXED_BYTES_PER_RUN: u64 = 512 * 1024 * 1024;

/// How far back the discovery window extends past the call start, in
/// seconds, to absorb filesystems with coarse (1s) mtime granularity.
const MTIME_MARGIN_SECS: u64 = 1;

/// Single owner of the outputs root path. Env seam for tests
/// (`TYLLUAN_OUTPUTS_DIR`), default `data/outputs` relative to cwd —
/// mirroring where comfy and every existing guild already write.
pub fn outputs_root() -> PathBuf {
    match std::env::var("TYLLUAN_OUTPUTS_DIR") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => PathBuf::from("data/outputs"),
    }
}

/// TTL for ledger entries + indexed artifacts, in seconds. `None` = pruning
/// disabled (the default; nothing is ever deleted unless an operator asks).
pub fn outputs_ttl_secs() -> Option<u64> {
    std::env::var("TYLLUAN_OUTPUTS_TTL_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFileEntry {
    /// Repo-relative path of the artifact, as the guild wrote it.
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputManifest {
    pub schema_version: u32,
    pub guild: String,
    pub tool: String,
    pub requested_by: String,
    pub run_id: String,
    /// Unix seconds at manifest write time.
    pub created_at: u64,
    pub call_success: bool,
    /// `ok` = call succeeded with artifacts, `partial` = artifacts exist but
    /// the call errored, or the byte cap truncated indexing.
    pub delivery_status: String,
    pub files: Vec<OutputFileEntry>,
    /// Discovery bookkeeping (diagnostics, not contract).
    pub claimed_outputs_verified: usize,
    pub window_scan_started_unix: u64,
}

/// Summary row for `GET /api/v1/outputs`.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub guild: String,
    pub run_id: String,
    pub created_at: u64,
    pub requested_by: String,
    pub tool: String,
    pub files_count: usize,
    pub total_bytes: u64,
    pub delivery_status: String,
}

/// Inputs the actor passes in after a guild tool call resolves.
#[derive(Debug, Clone)]
pub struct CallRecord<'a> {
    pub guild: &'a str,
    pub tool: &'a str,
    pub requested_by: &'a str,
    pub success: bool,
    /// Call window bounds, unix seconds. The actor stamps these around the
    /// whole retry loop (a wider window only widens attribution).
    pub started_unix: u64,
    pub ended_unix: u64,
    /// Raw serialized CallToolResult, used for claimed-outputs extraction.
    pub result_json: &'a str,
    /// Pre-generated run id (deterministic tests). Production callers leave
    /// this unset and get a fresh uuid-simple per call.
    pub run_id_override: Option<String>,
}

/// Validate a run_id coming from HTTP input before it ever touches a path:
/// uuid-simple ids are lowercase hex; be strict (alnum + dashes), fixed
/// length-independent, and refuse anything path-shaped.
fn run_id_is_safe(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 64
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !run_id.starts_with('-')
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Extract claimed output paths from a serialized CallToolResult. Only
/// accepts results whose first text content item parses as JSON carrying a
/// `tylluan_outputs` string array. Anything else (markdown, plain text) is
/// simply not a claiming result.
fn claimed_outputs(result_json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(result_json) else {
        return Vec::new();
    };
    let Some(items) = v.get("content").and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    let mut texts = Vec::new();
    for item in items {
        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
            texts.push(t.to_string());
        }
    }
    let joined = texts.join("\n");
    let Ok(claim) = serde_json::from_str::<serde_json::Value>(&joined) else {
        return Vec::new();
    };
    claim
        .get("tylluan_outputs")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The store. Cheap to construct; all state lives on disk.
pub struct OutputsStore {
    root: PathBuf,
}

impl OutputsStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Store at the canonical root (single owner: `outputs_root()`).
    pub fn at_default_root() -> Self {
        Self::new(outputs_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn guild_dir(&self, guild: &str) -> PathBuf {
        self.root.join(guild)
    }

    fn run_dir(&self, guild: &str, run_id: &str) -> PathBuf {
        self.guild_dir(guild).join(run_id)
    }

    /// Index one resolved tool call. Returns the written manifest, or `None`
    /// when the call produced no artifacts (no run dir is created). All
    /// failures are logged-and-swallowed: the ledger must never break the
    /// call path it observes.
    pub fn index_call(&self, call: &CallRecord<'_>) -> Option<OutputManifest> {
        let result = self.index_call_inner(call);
        match result {
            Ok(Some(m)) => Some(m),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    "OutputsStore: indexing failed for guild '{}' run: {e}",
                    call.guild
                );
                None
            }
        }
    }

    fn index_call_inner(&self, call: &CallRecord<'_>) -> std::io::Result<Option<OutputManifest>> {
        let window_start = call.started_unix.saturating_sub(MTIME_MARGIN_SECS);
        let mut files: Vec<OutputFileEntry> = Vec::new();
        let mut verified_claims = 0usize;

        // Mode 2 first: claimed outputs (strictly inside the guild namespace).
        let guild_prefix = self.guild_dir(call.guild);
        for claimed in claimed_outputs(call.result_json) {
            let claimed_path = PathBuf::from(&claimed);
            if claimed_path.starts_with(&guild_prefix) && claimed_path.is_file() {
                if let Ok(meta) = fs::metadata(&claimed_path) {
                    verified_claims += 1;
                    files.push(OutputFileEntry {
                        path: claimed,
                        bytes: meta.len(),
                        sha256: sha256_file(&claimed_path).unwrap_or_default(),
                    });
                }
            } else {
                tracing::warn!(
                    "OutputsStore: guild '{}' claimed output outside its namespace, ignored: {claimed}",
                    call.guild
                );
            }
        }

        // Mode 1: window diff-scan of the guild's outputs dir. Skips run dirs
        // (manifests are ours, never artifacts) and the manifest of THIS run.
        let scan_root = guild_prefix.clone();
        if scan_root.is_dir() {
            let mut stack = vec![scan_root];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_type = match entry.file_type() {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    if file_type.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    // Run dirs hold only our manifests, never artifacts, and
                    // are created after this scan anyway.
                    if path.file_name().and_then(|n| n.to_str()) == Some("manifest.json") {
                        continue;
                    }
                    let Ok(meta) = fs::metadata(&path) else { continue };
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if mtime < window_start || mtime > call.ended_unix + MTIME_MARGIN_SECS {
                        continue;
                    }
                    files.push(OutputFileEntry {
                        path: path.to_string_lossy().to_string(),
                        bytes: meta.len(),
                        sha256: sha256_file(&path).unwrap_or_default(),
                    });
                }
            }
        }

        // De-duplicate (a file both claimed and scanned).
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files.dedup_by(|a, b| a.path == b.path);

        if files.is_empty() {
            return Ok(None);
        }

        // Byte cap: stop indexing, mark partial.
        let mut status = if call.success { "ok" } else { "partial" }.to_string();
        let mut total: u64 = 0;
        files.retain(|f| {
            if total + f.bytes > MAX_INDEXED_BYTES_PER_RUN {
                return false;
            }
            total += f.bytes;
            true
        });
        if total >= MAX_INDEXED_BYTES_PER_RUN {
            status = "partial".to_string();
        }

        let manifest = OutputManifest {
            schema_version: 1,
            guild: call.guild.to_string(),
            tool: call.tool.to_string(),
            requested_by: call.requested_by.to_string(),
            run_id: call.run_id_dir_name(),
            created_at: unix_now(),
            call_success: call.success,
            delivery_status: status,
            files,
            claimed_outputs_verified: verified_claims,
            window_scan_started_unix: window_start,
        };

        let run_dir = self.run_dir(call.guild, &manifest.run_id);
        fs::create_dir_all(&run_dir)?;
        let manifest_path = run_dir.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
        tracing::info!(
            "OutputsStore: run {}/{} indexed {} file(s) ({})",
            call.guild,
            manifest.run_id,
            manifest.files.len(),
            manifest.delivery_status
        );
        Ok(Some(manifest))
    }

    /// List run summaries, newest first. `guild` filters to one guild.
    pub fn list_runs(&self, guild: Option<&str>, limit: usize) -> Vec<RunSummary> {
        let mut out = Vec::new();
        let guild_dirs: Vec<PathBuf> = match guild {
            Some(g) => vec![self.guild_dir(g)],
            None => match fs::read_dir(&self.root) {
                Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
                Err(_) => Vec::new(),
            },
        };
        for gdir in guild_dirs {
            let Ok(runs) = fs::read_dir(&gdir) else { continue };
            for run in runs.flatten() {
                let manifest_path = run.path().join("manifest.json");
                if !manifest_path.is_file() {
                    continue;
                }
                let Ok(bytes) = fs::read(&manifest_path) else { continue };
                let Ok(m) = serde_json::from_slice::<OutputManifest>(&bytes) else {
                    continue;
                };
                out.push(RunSummary {
                    guild: m.guild,
                    run_id: m.run_id,
                    created_at: m.created_at,
                    requested_by: m.requested_by,
                    tool: m.tool,
                    files_count: m.files.len(),
                    total_bytes: m.files.iter().map(|f| f.bytes).sum(),
                    delivery_status: m.delivery_status,
                });
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.run_id.cmp(&b.run_id)));
        out.truncate(limit);
        out
    }

    /// Read one manifest by run_id (searches all guild dirs). The run_id is
    /// HTTP-facing input: strictly sanitized before any path use.
    pub fn read_manifest(&self, run_id: &str) -> Option<OutputManifest> {
        if !run_id_is_safe(run_id) {
            return None;
        }
        let guilds = fs::read_dir(&self.root).ok()?;
        for g in guilds.flatten() {
            let candidate = g.path().join(run_id).join("manifest.json");
            if candidate.is_file() {
                let bytes = fs::read(&candidate).ok()?;
                return serde_json::from_slice(&bytes).ok();
            }
        }
        None
    }

    /// Opt-in TTL prune: removes expired run dirs and the artifact files
    /// they indexed — but only artifacts inside the store root (an indexed
    /// path that escaped the namespace is never deleted). Returns the number
    /// of runs removed.
    pub fn prune_expired(&self, max_age_secs: u64) -> usize {
        let cutoff = unix_now().saturating_sub(max_age_secs);
        let mut removed = 0usize;
        let Ok(guilds) = fs::read_dir(&self.root) else { return 0 };
        for g in guilds.flatten() {
            let gdir = g.path();
            let Ok(runs) = fs::read_dir(&gdir) else { continue };
            for run in runs.flatten() {
                let run_dir = run.path();
                let manifest_path = run_dir.join("manifest.json");
                if !manifest_path.is_file() {
                    continue;
                }
                let manifest: OutputManifest = match fs::read(&manifest_path)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                {
                    Some(m) => m,
                    None => continue,
                };
                if manifest.created_at >= cutoff {
                    continue;
                }
                // Delete indexed artifacts, strictly inside the store root.
                for f in &manifest.files {
                    let p = PathBuf::from(&f.path);
                    if p.starts_with(&self.root) && p.is_file() {
                        let _ = fs::remove_file(&p);
                    }
                }
                if fs::remove_dir_all(&run_dir).is_ok() {
                    removed += 1;
                }
            }
        }
        if removed > 0 {
            tracing::info!("OutputsStore: pruned {removed} expired run(s)");
        }
        removed
    }
}

impl<'a> CallRecord<'a> {
    /// The actor generates one run id per resolved call (uuid-simple, the
    /// same shape the dispatch queue uses). Tests may pin one via
    /// `run_id_override` for deterministic assertions.
    pub fn run_id_dir_name(&self) -> String {
        self.run_id_override
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_store(tag: &str) -> (OutputsStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tylluan_outputs_{}_{}",
            tag,
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        (OutputsStore::new(dir.clone()), dir)
    }

    fn record<'a>(result_json: &'a str) -> (CallRecord<'a>, String) {
        let run_id = "deadbeef00000001".to_string();
        (
            CallRecord {
                guild: "comfy",
                tool: "txt2img",
                requested_by: "buffy",
                success: true,
                started_unix: unix_now() - 5,
                ended_unix: unix_now(),
                result_json,
                run_id_override: Some(run_id.clone()),
            },
            run_id,
        )
    }

    #[test]
    fn empty_call_leaves_no_trace() {
        let (store, dir) = temp_store("empty");
        let (rec, _) = record(r#"{"content":[{"type":"text","text":"hello"}]}"#);
        let m = store.index_call(&rec);
        assert!(m.is_none());
        assert!(store.list_runs(None, 10).is_empty());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn window_scan_indexes_new_file_and_writes_manifest() {
        let (store, dir) = temp_store("scan");
        let artifact = dir.join("comfy").join("img_001.png");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"PNGDATA").unwrap();

        let (rec, run_id) = record(r#"{"content":[{"type":"text","text":"done"}]}"#);
        let m = store.index_call(&rec).expect("manifest expected");
        assert_eq!(m.delivery_status, "ok");
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].bytes, 7);
        assert_eq!(m.files[0].sha256.len(), 64);

        // Read back through the read path.
        let runs = store.list_runs(Some("comfy"), 10);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].requested_by, "buffy");
        let back = store.read_manifest(&run_id).expect("manifest readable");
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].path, artifact.to_string_lossy());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_files_outside_window_are_ignored() {
        let (store, dir) = temp_store("window");
        let artifact = dir.join("comfy").join("old.png");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"old").unwrap();
        // Force an old mtime (35s ago, outside window+margin). std setter,
        // no extra dev-dependency.
        let past = SystemTime::now() - Duration::from_secs(35);
        fs::File::options().write(true).open(&artifact)
            .unwrap()
            .set_modified(past)
            .unwrap();

        let (rec, _) = record(r#"{"content":[{"type":"text","text":"x"}]}"#);
        assert!(store.index_call(&rec).is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn claimed_paths_outside_guild_namespace_are_rejected() {
        let (store, dir) = temp_store("claims");
        // Guild claims a file OUTSIDE its outputs dir: must be ignored even
        // though it exists (outputs are data, never an ack of arbitrary fs).
        let outside = dir.join("secrets.txt");
        fs::write(&outside, b"secret").unwrap();
        // The claim JSON arrives as the *text* of the content item.
        let claim = format!(
            "{{\"tylluan_outputs\":[{}]}}",
            serde_json::json!(outside.to_string_lossy().to_string())
        );
        let result = format!(
            r#"{{"content":[{{"type":"text","text":{}}}]}}"#,
            serde_json::json!(claim)
        );
        let (rec, _) = record(&result);
        assert!(store.index_call(&rec).is_none(), "no artifacts, no manifest");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn claimed_paths_inside_namespace_are_verified() {
        let (store, dir) = temp_store("claims_ok");
        let artifact = dir.join("comfy").join("out.bin");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"BYTES").unwrap();
        let claim = format!(
            "{{\"tylluan_outputs\":[{}]}}",
            serde_json::json!(artifact.to_string_lossy().to_string())
        );
        let result = format!(
            r#"{{"content":[{{"type":"text","text":{}}}]}}"#,
            serde_json::json!(claim)
        );
        let (rec, run_id) = record(&result);
        let m = store.index_call(&rec).expect("claimed artifact indexed");
        assert_eq!(m.claimed_outputs_verified, 1);
        assert_eq!(m.files.len(), 1);
        assert!(store.read_manifest(&run_id).is_some());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn failed_call_with_artifacts_is_partial() {
        let (store, dir) = temp_store("partial");
        let artifact = dir.join("comfy").join("img.png");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"x").unwrap();
        let (mut rec, run_id) = record(r#"{"content":[{"type":"text","text":"x"}]}"#);
        rec.success = false;
        let m = store.index_call(&rec).expect("artifacts exist");
        assert_eq!(m.delivery_status, "partial");
        assert!(!m.call_success);
        assert!(store.read_manifest(&run_id).is_some());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prune_removes_expired_runs_and_their_artifacts_only() {
        let (store, dir) = temp_store("prune");
        let artifact = dir.join("comfy").join("old.bin");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"PRUNEME").unwrap();
        let (rec, run_id) = record(r#"{"content":[{"type":"text","text":"x"}]}"#);
        store.index_call(&rec).expect("indexed");
        assert!(artifact.is_file());

        // Not expired yet: prune with a huge TTL keeps everything.
        assert_eq!(store.prune_expired(10_000), 0);
        assert!(store.read_manifest(&run_id).is_some());

        // Backdate the on-disk manifest to make the run deterministically
        // expired (created_at is wall-clock stamped at write time).
        let manifest_path = store
            .root()
            .join("comfy")
            .join(&run_id)
            .join("manifest.json");
        let mut manifest: OutputManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.created_at = unix_now().saturating_sub(3600);
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        // Expired: run dir + indexed artifact go, nothing else.
        assert_eq!(store.prune_expired(0), 1);
        assert!(!artifact.is_file(), "indexed artifact pruned");
        assert!(store.read_manifest(&run_id).is_none());

        // An unindexed sibling file survives.
        let sibling = dir.join("comfy").join("keep.txt");
        fs::write(&sibling, b"keep").unwrap();
        assert_eq!(store.prune_expired(0), 0);
        assert!(sibling.is_file());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn hostile_run_ids_never_reach_the_filesystem() {
        let (store, dir) = temp_store("traversal");
        assert!(store.read_manifest("../../etc/passwd").is_none());
        assert!(store.read_manifest("..\\..\\windows").is_none());
        assert!(store.read_manifest("").is_none());
        assert!(store.read_manifest(&"a".repeat(65)).is_none());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn seam_resolver_honors_env_var() {
        // SAFETY (set_var/remove_var): edition-2024 unsafe ops, guarded
        // single-user section — the lib test harness runs one test at a time
        // and no other test in this binary touches this variable (same
        // pattern as dispatch_queue.rs and confusion.rs).
        let old = std::env::var("TYLLUAN_OUTPUTS_DIR").ok();
        unsafe { std::env::set_var("TYLLUAN_OUTPUTS_DIR", "/tmp/tylluan_outputs_seam_probe") };
        assert_eq!(outputs_root(), PathBuf::from("/tmp/tylluan_outputs_seam_probe"));
        match old {
            Some(v) => unsafe { std::env::set_var("TYLLUAN_OUTPUTS_DIR", v) },
            None => unsafe { std::env::remove_var("TYLLUAN_OUTPUTS_DIR") },
        }
    }
}
