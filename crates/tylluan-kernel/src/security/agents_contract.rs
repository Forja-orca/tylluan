use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Represents one agent declaration in .tylluan/agents.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContractEntry {
    pub role: String,
    #[serde(default)]
    pub description: String,
    /// Optional push-dispatch policy for this agent (turn 550 design split:
    /// this table is the POLICY half — who may trigger this agent and how;
    /// the EXISTENCE half — is this agent id real right now — lives in
    /// SilvaDB identity, not here). Absent entirely for an agent that
    /// hasn't opted into push wake-up.
    #[serde(default)]
    pub wake: Option<WakeConfig>,
}

/// Push-dispatch policy for one agent (`.tylluan/agents.toml`,
/// `[agents.<id>.wake]`). NOT wired to actually spawn anything yet — this
/// is data + validation only, prototype for the design discussed in
/// docs/architecture/coloquio_push_dispatch_research.md and
/// event_driven_agent_triggers_research.md (2026-09-17). The kernel-side
/// dispatcher that reads this and invokes `command` is a separate,
/// not-yet-built piece — deliberately, per José's decision that both
/// mitigations (allowlist + human confirmation) must exist before anything
/// executes automatically.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WakeConfig {
    /// Master switch. Defaults to false — an agent must opt in explicitly;
    /// declaring a `[wake]` table with no `enabled = true` is inert.
    #[serde(default)]
    pub enabled: bool,
    /// Author allowlist (case-insensitive at use site — mirrors the
    /// @mention bridge fix in api_coloquio.rs). Empty means no author is
    /// trusted, NOT "everyone" — this is the opposite of the @mention
    /// bridge's empty-contract fallback, because here the cost of getting
    /// it wrong is unattended command execution, not a dropped
    /// notification. Fail-closed, always.
    #[serde(default)]
    pub trusted_authors: Vec<String>,
    /// Fixed argv the dispatcher would invoke (e.g. `["opencode", "run"]`).
    /// The triggering message's content is never used to build this list —
    /// only ever appended as a single trailing argument by whatever
    /// dispatcher eventually consumes this config, exactly like
    /// coloquio_watcher.py's existing `subprocess.Popen` pattern (no
    /// `shell=True`, so message content can't inject additional argv).
    #[serde(default)]
    pub command: Vec<String>,
}

impl WakeConfig {
    /// True only when the config is meaningfully turned on: `enabled` AND
    /// has both a non-empty allowlist and a non-empty command. A config
    /// with `enabled = true` but no trusted authors or no command is
    /// treated as inert, not as "trust everyone" or "run nothing" — this
    /// is the fail-closed validation the future dispatcher must call
    /// before doing anything with this entry.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.trusted_authors.is_empty() && !self.command.is_empty()
    }

    /// Case-insensitive membership check against `trusted_authors`.
    pub fn trusts(&self, author_id: &str) -> bool {
        self.trusted_authors.iter().any(|a| a.eq_ignore_ascii_case(author_id))
    }
}

/// Declarative agent contract loaded from `.tylluan/agents.toml`.
/// Maps `agent_id` → `(role, description)`.
///
/// This is repo-local declarative data (committed to version control),
/// separate from the operator-controlled `TylluanConfig.security.acl`
/// which carries secrets-adjacent token→role mappings.
///
/// Missing file → empty contract (fully optional, zero behavior change).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsContract {
    pub agents: HashMap<String, AgentContractEntry>,
}

impl AgentsContract {
    /// Load from `.tylluan/agents.toml` relative to `workspace_root`.
    /// Missing file returns an empty contract (not an error).
    pub fn load(workspace_root: &std::path::Path) -> Self {
        let path = workspace_root.join(".tylluan").join("agents.toml");
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match toml::from_str::<AgentsContract>(&content) {
                    Ok(contract) => {
                        tracing::info!(
                            "✅ AgentsContract: loaded {} agent(s) from {}",
                            contract.agents.len(),
                            path.display()
                        );
                        contract
                    }
                    Err(e) => {
                        tracing::warn!(
                            "⚠️ AgentsContract: failed to parse {}: {}. Using empty contract.",
                            path.display(),
                            e
                        );
                        AgentsContract::empty()
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(
                    "AgentsContract: {} not found — using empty contract (feature is optional).",
                    path.display()
                );
                AgentsContract::empty()
            }
            Err(e) => {
                tracing::warn!(
                    "⚠️ AgentsContract: cannot read {}: {}. Using empty contract.",
                    path.display(),
                    e
                );
                AgentsContract::empty()
            }
        }
    }

    /// Empty contract — no agents declared, all features operate as if
    /// the file doesn't exist (fully backward compatible).
    pub fn empty() -> Self {
        AgentsContract {
            agents: HashMap::new(),
        }
    }

    /// Look up an agent's declared role. Returns `None` if the agent
    /// is not declared in the contract.
    pub fn get_role(&self, agent_id: &str) -> Option<&str> {
        self.agents.get(agent_id).map(|e| e.role.as_str())
    }

    /// Returns all declared agent IDs.
    pub fn agent_ids(&self) -> impl Iterator<Item = &String> {
        self.agents.keys()
    }

    /// Returns the number of declared agents.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Returns the agent's wake policy, if declared and meaningfully
    /// active (see `WakeConfig::is_active`). Returns `None` for an
    /// undeclared agent, a declared agent with no `[wake]` table, or a
    /// `[wake]` table that's inert (missing `enabled`, authors, or
    /// command).
    pub fn active_wake_config(&self, agent_id: &str) -> Option<&WakeConfig> {
        self.agents.get(agent_id)
            .and_then(|e| e.wake.as_ref())
            .filter(|w| w.is_active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_contract() {
        let c = AgentsContract::empty();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.get_role("anyone").is_none());
    }

    #[test]
    fn test_get_role_returns_declared() {
        let mut agents = HashMap::new();
        agents.insert("deepseek".to_string(), AgentContractEntry {
            role: "contributor".to_string(),
            description: "Rust implementation".to_string(), wake: None,
        });
        agents.insert("claude".to_string(), AgentContractEntry {
            role: "admin".to_string(),
            description: "Tech lead".to_string(), wake: None,
        });
        let c = AgentsContract { agents };

        assert_eq!(c.get_role("deepseek"), Some("contributor"));
        assert_eq!(c.get_role("claude"), Some("admin"));
        assert!(c.get_role("unknown").is_none());
    }

    #[test]
    fn test_load_from_nonexistent_file_returns_empty() {
        let tmp = std::env::temp_dir();
        let c = AgentsContract::load(&tmp);
        assert!(c.is_empty());
    }

    #[test]
    fn test_load_from_valid_file() {
        let tmp = std::env::temp_dir().join("test_agents_contract_load");
        let _ = std::fs::create_dir_all(tmp.join(".tylluan"));
        let toml_path = tmp.join(".tylluan").join("agents.toml");
        let toml_content = r#"
[agents.claude-code]
role = "admin"
description = "Tech lead — orchestration, planning"

[agents.deepseek-opencode]
role = "contributor"
description = "Rust/CLI implementation"
"#;
        std::fs::write(&toml_path, toml_content).expect("write test agents.toml");
        let c = AgentsContract::load(&tmp);
        assert_eq!(c.len(), 2);
        assert_eq!(c.get_role("claude-code"), Some("admin"));
        assert_eq!(c.get_role("deepseek-opencode"), Some("contributor"));
        assert_eq!(
            c.agents.get("deepseek-opencode").unwrap().description,
            "Rust/CLI implementation"
        );
        let _ = std::fs::remove_dir_all(tmp.join(".tylluan"));
    }

    #[test]
    fn test_load_from_malformed_file_returns_empty() {
        let tmp = std::env::temp_dir().join("test_agents_contract_malformed");
        let _ = std::fs::create_dir_all(tmp.join(".tylluan"));
        let toml_path = tmp.join(".tylluan").join("agents.toml");
        std::fs::write(&toml_path, "not valid toml {{{").expect("write malformed");
        let c = AgentsContract::load(&tmp);
        assert!(c.is_empty(), "malformed TOML must produce empty contract");
        let _ = std::fs::remove_dir_all(tmp.join(".tylluan"));
    }

    #[test]
    fn test_agent_ids_iteration() {
        let mut agents = HashMap::new();
        agents.insert("alice".to_string(), AgentContractEntry {
            role: "admin".to_string(),
            description: "".to_string(), wake: None,
        });
        agents.insert("bob".to_string(), AgentContractEntry {
            role: "contributor".to_string(),
            description: "".to_string(), wake: None,
        });
        let c = AgentsContract { agents };
        let ids: Vec<&String> = c.agent_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&"alice".to_string()));
        assert!(ids.contains(&&"bob".to_string()));
    }

    #[test]
    fn test_load_from_nested_cwd_regression() {
        let tmp = std::env::temp_dir().join("test_agents_contract_nested_cwd");
        let _ = std::fs::create_dir_all(tmp.join(".tylluan"));
        let _ = std::fs::create_dir_all(tmp.join("crates").join("tylluan-kernel"));

        std::fs::write(tmp.join("tylluan.toml"), "[nexus]\nport = 47004\n").unwrap();
        let agents_toml = r#"
[agents.deep]
role = "contributor"
description = "Rust implementation"
"#;
        std::fs::write(tmp.join(".tylluan").join("agents.toml"), agents_toml).unwrap();

        let saved_cwd = std::env::current_dir().ok();

        let nested = tmp.join("crates").join("tylluan-kernel");
        std::env::set_current_dir(&nested).expect("cd to nested dir");

        let root = crate::transport::http::find_workspace_root();
        assert_eq!(root, tmp, "find_workspace_root must walk up to the dir with tylluan.toml");

        let contract = AgentsContract::load(&root);
        assert_eq!(contract.len(), 1, "contract must load from found workspace root");
        assert_eq!(contract.get_role("deep"), Some("contributor"));

        if let Some(cwd) = saved_cwd {
            let _ = std::env::set_current_dir(&cwd);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── WakeConfig / [agents.<id>.wake] (push-dispatch prototype, 2026-09-17) ──

    #[test]
    fn wake_config_inactive_by_default() {
        let w = WakeConfig::default();
        assert!(!w.is_active(), "a bare WakeConfig::default() must never be active");
    }

    #[test]
    fn wake_config_requires_all_three_fields_to_be_active() {
        let enabled_no_authors = WakeConfig {
            enabled: true,
            trusted_authors: vec![],
            command: vec!["opencode".to_string(), "run".to_string()],
        };
        assert!(!enabled_no_authors.is_active(), "enabled with no trusted authors must stay inert");

        let enabled_no_command = WakeConfig {
            enabled: true,
            trusted_authors: vec!["claude-code".to_string()],
            command: vec![],
        };
        assert!(!enabled_no_command.is_active(), "enabled with no command must stay inert");

        let disabled_but_configured = WakeConfig {
            enabled: false,
            trusted_authors: vec!["claude-code".to_string()],
            command: vec!["opencode".to_string(), "run".to_string()],
        };
        assert!(!disabled_but_configured.is_active(), "enabled=false must stay inert regardless of the rest");

        let fully_active = WakeConfig {
            enabled: true,
            trusted_authors: vec!["claude-code".to_string()],
            command: vec!["opencode".to_string(), "run".to_string()],
        };
        assert!(fully_active.is_active());
    }

    #[test]
    fn wake_config_trusts_is_case_insensitive() {
        let w = WakeConfig {
            enabled: true,
            trusted_authors: vec!["Claude-Code".to_string(), "jose".to_string()],
            command: vec!["opencode".to_string(), "run".to_string()],
        };
        assert!(w.trusts("claude-code"));
        assert!(w.trusts("CLAUDE-CODE"));
        assert!(w.trusts("Jose"));
        assert!(!w.trusts("deep"));
    }

    #[test]
    fn active_wake_config_none_for_undeclared_agent() {
        let c = AgentsContract::empty();
        assert!(c.active_wake_config("deep").is_none());
    }

    #[test]
    fn active_wake_config_none_when_wake_table_absent() {
        let mut agents = HashMap::new();
        agents.insert("deep".to_string(), AgentContractEntry {
            role: "writer".to_string(),
            description: "".to_string(),
            wake: None,
        });
        let c = AgentsContract { agents };
        assert!(c.active_wake_config("deep").is_none());
    }

    #[test]
    fn active_wake_config_none_when_inert() {
        let mut agents = HashMap::new();
        agents.insert("deep".to_string(), AgentContractEntry {
            role: "writer".to_string(),
            description: "".to_string(),
            wake: Some(WakeConfig { enabled: false, trusted_authors: vec!["jose".to_string()], command: vec!["opencode".to_string()] }),
        });
        let c = AgentsContract { agents };
        assert!(c.active_wake_config("deep").is_none(), "enabled=false must not surface as an active config");
    }

    #[test]
    fn active_wake_config_some_when_fully_configured() {
        let mut agents = HashMap::new();
        agents.insert("deep".to_string(), AgentContractEntry {
            role: "writer".to_string(),
            description: "".to_string(),
            wake: Some(WakeConfig {
                enabled: true,
                trusted_authors: vec!["claude-code".to_string()],
                command: vec!["opencode".to_string(), "run".to_string()],
            }),
        });
        let c = AgentsContract { agents };
        let wake = c.active_wake_config("deep").expect("must be Some for a fully-configured active entry");
        assert!(wake.trusts("claude-code"));
        assert_eq!(wake.command, vec!["opencode".to_string(), "run".to_string()]);
    }

    #[test]
    fn wake_table_parses_from_toml() {
        let tmp = std::env::temp_dir().join("test_agents_contract_wake_toml");
        let _ = std::fs::create_dir_all(tmp.join(".tylluan"));
        let toml_content = r#"
[agents.deep]
role = "writer"
description = "Backend"

[agents.deep.wake]
enabled = true
trusted_authors = ["claude-code", "jose"]
command = ["opencode", "run"]
"#;
        std::fs::write(tmp.join(".tylluan").join("agents.toml"), toml_content).unwrap();
        let c = AgentsContract::load(&tmp);
        let wake = c.active_wake_config("deep").expect("wake table must parse and be active");
        assert!(wake.trusts("jose"));
        assert_eq!(wake.command, vec!["opencode".to_string(), "run".to_string()]);
        let _ = std::fs::remove_dir_all(tmp.join(".tylluan"));
    }

    #[test]
    fn agent_without_wake_table_still_parses_fine() {
        let tmp = std::env::temp_dir().join("test_agents_contract_no_wake_toml");
        let _ = std::fs::create_dir_all(tmp.join(".tylluan"));
        let toml_content = r#"
[agents.qwen]
role = "reader"
description = "Research"
"#;
        std::fs::write(tmp.join(".tylluan").join("agents.toml"), toml_content).unwrap();
        let c = AgentsContract::load(&tmp);
        assert_eq!(c.len(), 1);
        assert!(c.active_wake_config("qwen").is_none());
        let _ = std::fs::remove_dir_all(tmp.join(".tylluan"));
    }
}
