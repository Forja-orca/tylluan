use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Represents one agent declaration in .tylluan/agents.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContractEntry {
    pub role: String,
    #[serde(default)]
    pub description: String,
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
                tracing::debug!(
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
            description: "Rust implementation".to_string(),
        });
        agents.insert("claude".to_string(), AgentContractEntry {
            role: "admin".to_string(),
            description: "Tech lead".to_string(),
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
            description: "".to_string(),
        });
        agents.insert("bob".to_string(), AgentContractEntry {
            role: "contributor".to_string(),
            description: "".to_string(),
        });
        let c = AgentsContract { agents };
        let ids: Vec<&String> = c.agent_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&"alice".to_string()));
        assert!(ids.contains(&&"bob".to_string()));
    }
}
