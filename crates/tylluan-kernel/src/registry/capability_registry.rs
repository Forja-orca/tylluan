//! # Capability Registry (MVP) — read-only index over sources that ALREADY exist
//!
//! TaskContext v2, the ProviderSelector (ADR-014) and the Cognitive Scheduler
//! all need to answer "what capabilities exist, who provides them, at what
//! declared risk". The sources already exist in production:
//!
//! - `registry::tools::TOOL_METADATA` — ~151 canonical kernel tools with
//!   category / risk_level / enriched_description (already consumed by the
//!   scheduler's risk chain via `enrich_tool`).
//! - `router::catalog` — the real guild registry: `GuildDescriptor.name` is
//!   the owner of every tool discovered in its Python file
//!   (`extract_subtools` over `@mcp.tool`/`@app.tool`/`@tool` declarations →
//!   `subtools`), with `weight` (Light/Medium/Heavy timeouts) and category.
//!
//! THIS MODULE ONLY INDEXES AND EXPOSES THEM — pure read layer, zero changes
//! to routing, matching, scheduling or dispatch behavior. Risk is copied
//! verbatim from TOOL_METADATA (the same source `enrich_tool` already
//! applies at runtime); guild tools absent from TOOL_METADATA keep the
//! scheduler's own convention (default Low, never fabricated). Providers are
//! always `Local` for now — `ProviderRef` leaves the variant space open for
//! Remote/Internal (ADR-014) without a breaking change.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::config::GuildWeight;
use crate::router::catalog::builtin_catalog;
use crate::registry::tools::{RiskLevel, TOOL_METADATA};

/// Who can serve a capability. MVP: only local guilds; the variants exist so
/// ADR-014's ProviderSelector can extend this without breaking the schema.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderRef {
    /// A local Python guild subprocess (registry owner of the tool).
    Local { guild: String },
}

/// One capability = one tool callable through Tylluan, with its declared risk
/// and the providers that serve it.
#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub name: String,
    /// TOOL_METADATA category, or "guild" for guild-only tools.
    pub category: String,
    /// Declared risk, copied verbatim from TOOL_METADATA (scheduler's own
    /// source of truth). Tools absent from TOOL_METADATA default to Low —
    /// the same convention `enrich_tool` applies today, never fabricated.
    pub risk_level: RiskLevel,
    pub providers: Vec<ProviderRef>,
    /// Enriched description when TOOL_METADATA has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Timeout class of the owning guild (Light/Medium/Heavy) — the
    /// operational cost hint TaskContext v2 will need.
    pub weight: GuildWeight,
}

impl Capability {
    /// Build the canonical id used by future ADR-014 work:
    /// `capability:<name>` (stable, URI-like, collision-free per tool name).
    pub fn capability_id(&self) -> String {
        format!("capability:{}", self.name)
    }
}

/// Build the full capability index from the live catalog + TOOL_METADATA.
/// Both sources are process-static (catalog is `OnceLock`-cached by
/// `builtin_catalog`), so the index is built once and shared.
pub fn build_registry() -> HashMap<String, Capability> {
    let mut index: HashMap<String, Capability> = HashMap::new();

    for guild in builtin_catalog() {
        for tool in &guild.subtools {
            let meta = TOOL_METADATA.get(tool.as_str());
            let entry = index.entry(tool.clone()).or_insert_with(|| Capability {
                name: tool.clone(),
                category: meta
                    .map(|m| m.category.clone())
                    .unwrap_or_else(|| "guild".to_string()),
                risk_level: meta
                    .map(|m| m.risk_level.clone())
                    .unwrap_or(RiskLevel::Low),
                providers: Vec::new(),
                description: meta.map(|m| m.enriched_description.clone()),
                weight: guild.weight,
            });
            entry.providers.push(ProviderRef::Local { guild: guild.name.clone() });
        }
    }

    // Kernel-canonical tools that are not owned by any scanned guild file
    // (e.g. the 5 sovereign MCP tools) still deserve an entry — provided by
    // the kernel itself.
    for (name, meta) in TOOL_METADATA.iter() {
        if !index.contains_key(name) {
            index.insert(
                name.clone(),
                Capability {
                    name: name.clone(),
                    category: meta.category.clone(),
                    risk_level: meta.risk_level.clone(),
                    providers: vec![ProviderRef::Local { guild: "kernel".to_string() }],
                    description: Some(meta.enriched_description.clone()),
                    weight: GuildWeight::Medium,
                },
            );
        }
    }

    index
}

/// Process-wide registry. Built once on first use; the underlying catalog is
/// itself cached (`builtin_catalog`), so this costs one walk of ~46 guilds.
pub fn registry() -> &'static HashMap<String, Capability> {
    static REGISTRY: LazyLock<HashMap<String, Capability>> = LazyLock::new(build_registry);
    &REGISTRY
}

/// All capabilities provided by one guild (empty if the guild exposes no
/// declared tools).
pub fn capabilities_for_guild(guild: &str) -> Vec<&'static Capability> {
    registry()
        .values()
        .filter(|c| {
            c.providers.iter().any(|p| match p {
                ProviderRef::Local { guild: g } => g == guild,
            })
        })
        .collect()
}

/// Lookup one capability by exact tool name.
pub fn find_capability(name: &str) -> Option<&'static Capability> {
    registry().get(name)
}

/// JSON envelope for the HTTP endpoints.
pub fn registry_json() -> serde_json::Value {
    let mut caps: Vec<&Capability> = registry().values().collect();
    caps.sort_by(|a, b| a.name.cmp(&b.name));
    serde_json::json!({
        "capabilities": caps,
        "count": caps.len(),
        "source": "TOOL_METADATA + router::catalog (read-only index)",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for the registry's core invariant: the index is built from
    /// the REAL sources (no mocks). If catalog scanning or TOOL_METADATA
    /// change shape, this fails loudly instead of silently emptying the
    /// registry.
    #[test]
    fn index_built_from_real_sources_is_populated() {
        let reg = registry();
        assert!(!reg.is_empty(), "registry must index real tools, got 0");
        // TOOL_METADATA canonical entries are always present.
        assert!(find_capability("bash_execute").is_some(), "canonical TOOL_METADATA tool missing");
        assert!(find_capability("memory_search").is_some());
    }

    /// DoD: tests against REAL TOOL_METADATA covering 3+ guilds.
    #[test]
    fn capabilities_for_guild_covers_at_least_three_real_guilds() {
        let reg = registry();
        let mut guilds_with_caps: std::collections::HashSet<String> = std::collections::HashSet::new();
        for cap in reg.values() {
            for p in &cap.providers {
                match p {
                    ProviderRef::Local { guild } => {
                        guilds_with_caps.insert(guild.clone());
                    }
                }
            }
        }
        assert!(
            guilds_with_caps.len() >= 3,
            "expected tools indexed for 3+ real guilds, got {guilds_with_caps:?}"
        );
    }

    /// A guild tool with a TOOL_METADATA entry carries its REAL declared
    /// risk (verbatim from the scheduler's own source), not a guess.
    #[test]
    fn risk_is_copied_verbatim_from_tool_metadata() {
        let bash = find_capability("bash_execute").expect("bash_execute indexed");
        assert_eq!(bash.risk_level, RiskLevel::High);
        assert_eq!(bash.category, "system");
        assert!(bash.description.is_some());
    }

    /// Multi-guild tools: one capability, several Local providers, and the
    /// guild query resolves the same set.
    #[test]
    fn providers_aggregate_across_guilds_and_guild_query_resolves() {
        let reg = registry();
        let multi: Vec<&Capability> = reg.values().filter(|c| c.providers.len() > 1).collect();
        if let Some(cap) = multi.first() {
            let first = match &cap.providers[0] {
                ProviderRef::Local { guild } => guild.clone(),
            };
            let from_guild = capabilities_for_guild(&first);
            assert!(
                from_guild.iter().any(|c| c.name == cap.name),
                "capabilities_for_guild must resolve tools the index attributes to that guild"
            );
        }
    }

    /// Coverage invariant: the registry is a SUPERSET of TOOL_METADATA —
    /// every canonical tool is present, either provided by a real guild or
    /// by the kernel fallback (never silently dropped by the index build).
    #[test]
    fn registry_covers_every_canonical_tool_metadata_entry() {
        let reg = registry();
        for name in TOOL_METADATA.keys() {
            let cap = reg
                .get(name)
                .unwrap_or_else(|| panic!("canonical tool {name} missing from registry"));
            assert!(!cap.providers.is_empty(), "{name} has no providers");
        }
    }

    /// Capability ids are stable and URI-like (ADR-014 precursor contract).
    #[test]
    fn capability_id_is_stable_and_prefixed() {
        let cap = find_capability("memory_search").expect("indexed");
        assert_eq!(cap.capability_id(), "capability:memory_search");
    }
}
