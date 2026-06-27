use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OraclePair {
    pub query: String,
    pub expected_id: String,
}

/// Default oracle — 8 hardcoded pairs calibrated for 40-80% recall band.
/// Node content verified in live silva.db before each oracle update.
pub fn default_oracle() -> Vec<OraclePair> {
    vec![
        OraclePair {
            query: "agent agent-1 with active session history".into(),
            expected_id: "agent:agent-1".into(),
        },
        OraclePair {
            query: "consolidation of vote blocks from coloquio M17 with unanimous consensus".into(),
            expected_id: "memory:1780957284984".into(),
        },
        OraclePair {
            query: "vote for M18 including NOCTUA plan and idle cycle utilization".into(),
            expected_id: "agent_memory:agent-2:cf92f8a216f64806834acbac40ecfbd3".into(),
        },
        OraclePair {
            query: "investigation of three AI ecosystem opportunities in sovereign environment".into(),
            expected_id: "agent_memory:agent-3:5647a0a36efb4495aec238df473301f7".into(),
        },
        OraclePair {
            query: "synthesis of consolidation votes from coloquio round M17".into(),
            expected_id: "agent_memory:agent-4:530ced5f7efa4131a6f7f2572d2923e3".into(),
        },
        OraclePair {
            query: "kernel soberano del sistema TylluanNexus".into(),
            expected_id: "agent:tylluan-nexus-o3".into(),
        },
        OraclePair {
            query: "user shares autoresearch repository from karpathy to leverage idle cycles in embeddings".into(),
            expected_id: "agent_memory:user-1:d5e035e1106c4f34bc276f60887db83d".into(),
        },
        OraclePair {
            query: "problema de verbosidad en tylluan_recall donde limit=5 genera respuestas de 72KB por falta de truncado".into(),
            expected_id: "agent_memory:agent:b82dca534da44da8b722da37863fcf58".into(),
        },
    ]
}

/// Load oracle from JSON file. Fallback to `default_oracle()` on any error.
pub fn load_oracle(path: &std::path::Path) -> Vec<OraclePair> {
    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(_) => return default_oracle(),
    };
    match serde_json::from_str::<Vec<OraclePair>>(&raw) {
        Ok(list) if !list.is_empty() => list,
        _ => default_oracle(),
    }
}
