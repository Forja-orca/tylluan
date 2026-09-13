//! # System State Snapshot (WS5 — 2026-09-13 external audit, finding #13)
//!
//! One canonical, hash-addressed identity of the running system, computed
//! once at boot and referenced by every benchmark, audit row and action.
//!
//! Why this exists: the J-13 routing benchmark ran against a kernel whose
//! commit differed from the docs' cited HEAD, and three TEB result revisions
//! coexisted without any machine-checkable identity of what produced them.
//! Every measurement ambiguity of that class resolves by citing one snapshot
//! ID instead of reconstructing state after the fact.
//!
//! Design constraints (real, not aspirational):
//! - Computed from live surfaces (`TylluanConfig`, `GuildRegistry`, the Silva
//!   schema), never from docs — docs drift is the failure mode this replaces.
//! - The guild capability surface (`capability_registry_hash`) covers name +
//!   always_on + full tool serialization per guild, so `46 guilds` the number
//!   stops being a claim and becomes a hash anyone can re-derive.
//! - Zero I/O at read time: stored in a `OnceLock`, read by `/health` and the
//!   audit writer for free. `Null` (not an error) if read before boot sets it.
//! - Not a security boundary: hashes are integrity/triage aids (SHA-256 over
//!   config serialization), not attestation.

use serde_json::json;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Canonical identity of the running system. All hashes are SHA-256 hex.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SystemSnapshot {
    /// Git commit the binary was built from (`TYLLUAN_GIT_COMMIT`, compile-time).
    pub commit: String,
    /// Cargo package version (`CARGO_PKG_VERSION`, compile-time).
    pub version: String,
    /// SHA-256 over the serialized boot config (`tylluan.toml` as loaded).
    pub config_hash: String,
    /// SHA-256 over the guild capability surface (name+always_on+tools per guild).
    pub catalog_hash: String,
    /// Embedding model identifier from config (not hashed content — the model
    /// file itself is external state; the identifier is what callers compare).
    pub model_hash: String,
    /// SilvaDB `PRAGMA user_version` observed at boot, when a DB was reachable.
    pub schema_version: Option<i32>,
    /// Alias of `catalog_hash` kept for audit-field naming parity with the
    /// design docs (capability registry = the guild tool surface today; will
    /// become the WS8 ProviderFabric registry when that lands).
    pub capability_registry_hash: String,
    /// RFC 3339 timestamp of when this snapshot was computed.
    pub computed_at: String,
}

impl SystemSnapshot {
    /// Short, human-comparable snapshot ID for logs and audit rows:
    /// `{commit[..10]}/{config_hash[..12]}`. Two boots with the same commit
    /// but different configs produce different IDs — that is the point.
    pub fn id(&self) -> String {
        format!("{}/{}", &self.commit[..self.commit.len().min(10)], &self.config_hash[..self.config_hash.len().min(12)])
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(json!(null))
    }
}

/// SHA-256 hex of a string, shared by every hash in this module.
fn hash_hex(s: &str) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(s.as_bytes()))
}

/// Hash of a guild capability surface: BTreeMap<guild_name, payload> so the
/// hash is order-independent (HashMap iteration order is not).
/// Payload per guild: always_on flag + serialized tools (the executable
/// contract — the thing that changes when a guild gains/loses a tool).
fn capability_hash_from_entries(entries: &BTreeMap<String, String>) -> String {
    let mut canonical = String::new();
    for (name, payload) in entries {
        canonical.push_str(name);
        canonical.push('=');
        canonical.push_str(payload);
        canonical.push(';');
    }
    hash_hex(&canonical)
}

/// Extract the per-guild capability payload from a live registry.
/// Empty registry (e.g. test helpers) yields an empty map — a valid,
/// stable surface, not an error: boot-time registries can legitimately be
/// empty before discovery.
pub fn capability_entries(registry: &crate::registry::guild_process::GuildRegistry) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for (name, guild) in &registry.guilds {
        // Tools serialize via serde (MCP protocol type); Debug fallback keeps
        // this robust across rmcp versions. Launcher serialized the same way —
        // a python→docker launcher swap is a capability-surface change.
        let tools = serde_json::to_string(&guild.tools)
            .unwrap_or_else(|_| format!("{:?}", guild.tools));
        let launcher = serde_json::to_string(&guild.launcher)
            .unwrap_or_else(|_| format!("{:?}", guild.launcher));
        entries.insert(name.clone(), format!("always_on={};launcher={};tools={}", guild.always_on, launcher, tools));
    }
    entries
}

impl SystemSnapshot {
    /// Compute the snapshot from live boot surfaces. `registry` may be `None`
    /// in early-boot or test contexts (catalog hash degrades to the empty
    /// surface, schema version to `None` — both honest, both stable).
    pub fn compute(
        config: &crate::config::TylluanConfig,
        registry: Option<&crate::registry::guild_process::GuildRegistry>,
        schema_version: Option<i32>,
    ) -> SystemSnapshot {
        let config_json = serde_json::to_string(config).unwrap_or_default();
        let entries = registry
            .map(capability_entries)
            .unwrap_or_default();
        let catalog_hash = capability_hash_from_entries(&entries);
        SystemSnapshot {
            commit: env!("TYLLUAN_GIT_COMMIT").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_hash: hash_hex(&config_json),
            catalog_hash: catalog_hash.clone(),
            model_hash: config.memory.embedding_model.clone(),
            schema_version,
            capability_registry_hash: catalog_hash,
            computed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

static BOOT_SNAPSHOT: OnceLock<SystemSnapshot> = OnceLock::new();

/// Install the boot snapshot. Called once from `main.rs`; later calls are
/// no-ops (first wins — snapshots are boot facts, not mutable state).
pub fn set_global(snapshot: SystemSnapshot) {
    let _ = BOOT_SNAPSHOT.set(snapshot);
}

/// Read the boot snapshot. `None` before boot computes it (e.g. unit tests
/// that never ran `main`).
pub fn global() -> Option<&'static SystemSnapshot> {
    BOOT_SNAPSHOT.get()
}

/// JSON form for embedding in API responses and audit rows: the snapshot
/// object, or `null` when not yet computed (honest absence, not a placeholder).
pub fn global_json() -> serde_json::Value {
    global().map(|s| s.to_json()).unwrap_or(json!(null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_snapshot_changes_on_config_change() {
        let mut c1 = crate::config::TylluanConfig::default();
        c1.memory.db_path = "data/a.db".to_string();
        let mut c2 = crate::config::TylluanConfig::default();
        c2.memory.db_path = "data/b.db".to_string();
        let s1 = SystemSnapshot::compute(&c1, None, None);
        let s2 = SystemSnapshot::compute(&c2, None, None);
        assert_ne!(s1.config_hash, s2.config_hash, "a config change must change the snapshot");
        assert_ne!(s1.id(), s2.id(), "snapshot ID must distinguish configs");
    }

    #[test]
    fn test_capability_hash_captures_tool_surface() {
        let mut a = BTreeMap::new();
        a.insert("bash".to_string(), "always_on=true;tools=[run]".to_string());
        let mut b = a.clone();
        b.insert("bash".to_string(), "always_on=true;tools=[run,write]".to_string());
        assert_ne!(
            capability_hash_from_entries(&a),
            capability_hash_from_entries(&b),
            "a tool change must change the capability hash"
        );
        // Order independence: same entries in different insertion order.
        let mut c = BTreeMap::new();
        c.insert("alpha".to_string(), "x".to_string());
        c.insert("beta".to_string(), "y".to_string());
        let mut d = BTreeMap::new();
        d.insert("beta".to_string(), "y".to_string());
        d.insert("alpha".to_string(), "x".to_string());
        assert_eq!(
            capability_hash_from_entries(&c),
            capability_hash_from_entries(&d),
            "hash must be order-independent"
        );
    }

    #[test]
    fn test_snapshot_id_format() {
        let mut c = crate::config::TylluanConfig::default();
        c.memory.db_path = "data/t.db".to_string();
        let s = SystemSnapshot::compute(&c, None, None);
        let id = s.id();
        let parts: Vec<&str> = id.split('/').collect();
        assert_eq!(parts.len(), 2, "ID is commit/config: {id}");
        // TYLLUAN_GIT_COMMIT is a SHORT (7-char) hash from build.rs; id() caps
        // at 10 — so the commit part is min(len, 10) = 7 in practice.
        assert!(
            (7..=10).contains(&parts[0].len()),
            "commit part is the short hash capped at 10: {id}"
        );
        assert_eq!(parts[1].len(), 12, "config part is 12 chars: {id}");
        assert_eq!(s.model_hash, c.memory.embedding_model, "model_hash mirrors config's embedding model");
    }

    #[test]
    fn test_global_unset_returns_null_json() {
        // The OnceLock may be unset (this test process may or may not have
        // computed one); global_json must never panic and must be an object
        // or null — never an error, never a fake placeholder.
        let v = global_json();
        assert!(v.is_null() || v.is_object(), "global_json is null-or-object, got: {v}");
    }

    #[test]
    fn test_schema_version_flows_through() {
        let c = crate::config::TylluanConfig::default();
        let with = SystemSnapshot::compute(&c, None, Some(26));
        assert_eq!(with.schema_version, Some(26));
        let without = SystemSnapshot::compute(&c, None, None);
        assert_eq!(without.schema_version, None, "unreachable DB degrades honestly to None");
    }
}
