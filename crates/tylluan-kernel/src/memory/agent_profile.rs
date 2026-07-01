use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub agent_id: String,
    pub first_seen: String,
    pub total_calls: u64,
    pub competencies: serde_json::Value,
    pub last_intent: Option<String>,
    pub role: String,
    pub reputation_score: f64,
    pub domain_scores: HashMap<String, f64>,
    pub persona: String,
    pub preferences: serde_json::Value,
}

pub struct AgentProfileStore {
    conn: Connection,
}

impl AgentProfileStore {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))
            .with_context(|| format!("Failed to open agent profile DB: {}", db_path))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_profiles (
                agent_id TEXT PRIMARY KEY,
                first_seen TEXT NOT NULL,
                total_calls INTEGER NOT NULL DEFAULT 0,
                competencies TEXT NOT NULL DEFAULT '{}',
                last_intent TEXT,
                role TEXT DEFAULT 'generalist'
            );"
        )?;
        // Migration: add role column if not present (idempotent)
        conn.execute_batch(
            "ALTER TABLE agent_profiles ADD COLUMN role TEXT DEFAULT 'generalist';"
        ).ok();
        // Migration: add persona and preferences columns (idempotent)
        conn.execute_batch(
            "ALTER TABLE agent_profiles ADD COLUMN persona TEXT DEFAULT '';"
        ).ok();
        conn.execute_batch(
            "ALTER TABLE agent_profiles ADD COLUMN preferences TEXT DEFAULT '{}';"
        ).ok();
        // agent_domain_scores: granular per-agent per-guild outcome tracking
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_domain_scores (
                agent_id TEXT NOT NULL,
                domain TEXT NOT NULL,
                successes INTEGER NOT NULL DEFAULT 0,
                failures INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (agent_id, domain)
            );"
        ).ok(); // ok() because it may already exist
        Ok(Self { conn })
    }

    pub fn upsert_activity(
        &self,
        agent_id: &str,
        guild: &str,
        success: bool,
        intent: Option<&str>,
    ) -> Result<()> {
        let role = if agent_id.contains("code") { "engineer" }
                   else if agent_id.contains("doc") { "writer" }
                   else if agent_id.contains("ops") { "operator" }
                   else { "generalist" };

        self.conn.execute(
            "INSERT OR IGNORE INTO agent_profiles (agent_id, first_seen, total_calls, competencies, role)
             VALUES (?1, datetime('now'), 0, '{}', ?2)",
            params![agent_id, role],
        )?;
        
        // The rest remains identical to record competence
        let competencies_json: String = self.conn.query_row(
            "SELECT competencies FROM agent_profiles WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )?;

        let mut counts: serde_json::Value =
            serde_json::from_str(&competencies_json).unwrap_or(serde_json::json!({}));
        if counts.get(guild).and_then(|v| v.as_object()).is_none() {
            counts[guild] = serde_json::json!({"successes": 0, "total": 0});
        }
        let stats = counts.get_mut(guild)
            .and_then(|v| v.as_object_mut())
            .expect("guild stats entry initialized immediately above");
        let total = stats.get("total").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        let successes = if success {
            stats.get("successes").and_then(|v| v.as_u64()).unwrap_or(0) + 1
        } else {
            stats.get("successes").and_then(|v| v.as_u64()).unwrap_or(0)
        };
        stats.insert("total".to_string(), serde_json::Value::Number(total.into()));
        stats.insert(
            "successes".to_string(),
            serde_json::Value::Number(successes.into()),
        );

        let updated_json = serde_json::to_string(&counts)?;

        self.conn.execute(
            "UPDATE agent_profiles SET total_calls = total_calls + 1, competencies = ?1, last_intent = COALESCE(?2, last_intent) WHERE agent_id = ?3",
            params![updated_json, intent, agent_id],
        )?;

                // Also update granular domain scores table
        self.conn.execute(
            "INSERT INTO agent_domain_scores (agent_id, domain, successes, failures)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_id, domain) DO UPDATE SET
                successes = successes + ?3,
                failures = failures + ?4",
            params![
                agent_id,
                guild,
                if success { 1i64 } else { 0i64 },
                if success { 0i64 } else { 1i64 },
            ],
        )?;
        Ok(())
    }

    /// Returns domain reputation for all agents: Vec<(domain, agent_id, rate, total)>
    pub fn get_domain_reputation(&self) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT domain, agent_id, successes, failures,
                    CAST(successes AS REAL) / MAX(1, successes + failures) AS rate
             FROM agent_domain_scores
             WHERE successes + failures >= 3
             ORDER BY domain, rate DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            let domain: String = row.get(0)?;
            let agent_id: String = row.get(1)?;
            let successes: i64 = row.get(2)?;
            let failures: i64 = row.get(3)?;
            let rate: f64 = row.get(4)?;
            Ok(serde_json::json!({
                "domain": domain,
                "agent_id": agent_id,
                "successes": successes,
                "failures": failures,
                "total": successes + failures,
                "rate": (rate * 100.0).round() / 100.0,
            }))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(anyhow::Error::from)
    }

    pub fn get_best_agent_for_domain(&self, domain: &str) -> Result<Option<serde_json::Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, successes, failures,
                    CAST(successes AS REAL) / MAX(1, successes + failures) AS rate
             FROM agent_domain_scores
             WHERE domain = ?1 AND successes + failures >= 3
             ORDER BY rate DESC, successes DESC
             LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![domain], |row| {
            let agent_id: String = row.get(0)?;
            let rate: f64 = row.get(3)?;
            Ok(serde_json::json!({
                "agent_id": agent_id,
                "rate": (rate * 100.0).round() / 100.0
            }))
        })?;
        
        if let Some(res) = rows.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    fn compute_competencies
(counts: &serde_json::Value) -> serde_json::Value {
        let mut competencies = serde_json::Map::new();
        if let Some(obj) = counts.as_object() {
            for (guild, stats) in obj {
                let s = stats.get("successes").and_then(|v| v.as_u64()).unwrap_or(0);
                let t = stats.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                let competence = if t > 0 {
                    (s as f64 + 1.0) / (t as f64 + 2.0)
                } else {
                    0.0
                };
                let rounded = (competence * 100.0).round() / 100.0;
                competencies.insert(guild.clone(), serde_json::json!(rounded));
            }
        }
        serde_json::Value::Object(competencies)
    }

    fn compute_reputation_and_scores(&self, agent_id: &str) -> Result<(f64, HashMap<String, f64>)> {
        let mut stmt = self.conn.prepare(
            "SELECT domain, CAST(successes AS REAL) / MAX(1, successes + failures) AS rate,
                    successes + failures AS total
             FROM agent_domain_scores
             WHERE agent_id = ?1"
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            let domain: String = row.get(0)?;
            let rate: f64 = row.get(1)?;
            let total: i64 = row.get(2)?;
            Ok((domain, rate, total))
        })?;
        let mut domain_scores = HashMap::new();
        let mut weighted_sum = 0.0_f64;
        let mut weight_total = 0.0_f64;
        for row in rows {
            let (domain, rate, total) = row?;
            let w = total as f64;
            weighted_sum += rate * w;
            weight_total += w;
            domain_scores.insert(domain, (rate * 100.0).round() / 100.0);
        }
        let reputation_score = if weight_total > 0.0 {
            (weighted_sum / weight_total * 100.0).round() / 100.0
        } else {
            0.0
        };
        Ok((reputation_score, domain_scores))
    }

    pub fn get_profile(&self, agent_id: &str) -> Result<Option<AgentProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, first_seen, total_calls, competencies, last_intent, role, persona, preferences FROM agent_profiles WHERE agent_id = ?1",
        )?;
        let mut rows = stmt.query(params![agent_id])?;
        if let Some(row) = rows.next()? {
            let agent_id: String = row.get(0)?;
            let first_seen: String = row.get(1)?;
            let total_calls: u64 = row.get(2)?;
            let comp_json: String = row.get(3)?;
            let last_intent: Option<String> = row.get(4)?;
            let role: String = row.get(5)?;
            let persona: String = row.get(6)?;
            let prefs_json: String = row.get(7)?;
            let counts: serde_json::Value =
                serde_json::from_str(&comp_json).unwrap_or(serde_json::json!({}));
            let preferences: serde_json::Value =
                serde_json::from_str(&prefs_json).unwrap_or(serde_json::json!({}));
            let (reputation_score, domain_scores) = self.compute_reputation_and_scores(&agent_id)
                .unwrap_or((0.0, HashMap::new()));
            Ok(Some(AgentProfile {
                agent_id,
                first_seen,
                total_calls,
                competencies: Self::compute_competencies(&counts),
                last_intent,
                role,
                reputation_score,
                domain_scores,
                persona,
                preferences,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn is_new_agent(&self, agent_id: &str) -> bool {
        self.conn
            .query_row(
                "SELECT total_calls FROM agent_profiles WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|calls| calls <= 1)
            .unwrap_or(false)
    }

    pub fn list_profiles(&self) -> Result<Vec<AgentProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, first_seen, total_calls, competencies, last_intent, role, persona, preferences FROM agent_profiles ORDER BY total_calls DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let agent_id: String = row.get(0)?;
            let first_seen: String = row.get(1)?;
            let total_calls: u64 = row.get(2)?;
            let comp_json: String = row.get(3)?;
            let last_intent: Option<String> = row.get(4)?;
            let role: String = row.get(5)?;
            let persona: String = row.get(6)?;
            let prefs_json: String = row.get(7)?;
            Ok((agent_id, first_seen, total_calls, comp_json, last_intent, role, persona, prefs_json))
        })?;
        let mut profiles = Vec::new();
        for row in rows {
            let (agent_id, first_seen, total_calls, comp_json, last_intent, role, persona, prefs_json) = row?;
            let counts: serde_json::Value =
                serde_json::from_str(&comp_json).unwrap_or(serde_json::json!({}));
            let preferences: serde_json::Value =
                serde_json::from_str(&prefs_json).unwrap_or(serde_json::json!({}));
            let (reputation_score, domain_scores) = self.compute_reputation_and_scores(&agent_id)
                .unwrap_or((0.0, HashMap::new()));
            profiles.push(AgentProfile {
                agent_id,
                first_seen,
                total_calls,
                competencies: Self::compute_competencies(&counts),
                last_intent,
                role,
                reputation_score,
                domain_scores,
                persona,
                preferences,
            });
        }
        Ok(profiles)
    }

    pub fn set_persona(&self, agent_id: &str, persona: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_profiles SET persona = ?1 WHERE agent_id = ?2",
            params![persona, agent_id],
        )?;
        Ok(())
    }

    pub fn set_preferences(&self, agent_id: &str, preferences: &serde_json::Value) -> Result<()> {
        let json_str = serde_json::to_string(preferences)?;
        self.conn.execute(
            "UPDATE agent_profiles SET preferences = ?1 WHERE agent_id = ?2",
            params![json_str, agent_id],
        )?;
        Ok(())
    }
}

/// Sync all agent profiles with computed reputation scores to SilvaDB as `agent_reputation` nodes.
/// Each node is idempotently upserted (same ID = same agent).
pub async fn sync_agent_reputation_to_silva(silva: &crate::memory::silva::SilvaDB, profiles: &[AgentProfile]) {
    for profile in profiles {
        let id = format!("agent_reputation:{}", profile.agent_id);
        let content = serde_json::json!({
            "reputation_score": profile.reputation_score,
            "domain_scores": profile.domain_scores,
            "agent_id": profile.agent_id,
            "total_calls": profile.total_calls,
            "role": profile.role,
        })
        .to_string();
        let metadata = serde_json::json!({
            "type": "agent_reputation",
            "agent_id": profile.agent_id,
            "reputation_score": profile.reputation_score,
        })
        .to_string();
        if let Err(e) = silva.upsert_node(&id, "agent_reputation", &content, &metadata).await {
            tracing::warn!(
                "⚠️ Failed to sync agent '{}' reputation to SilvaDB: {}",
                profile.agent_id,
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reputation_computation() {
        let dir = std::env::temp_dir().join(format!("test_agent_reputation_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test_agent_profiles.db");
        let store = AgentProfileStore::new(&db_path.to_string_lossy()).unwrap();

        let _ = store.upsert_activity("agent_a", "bash", true, Some("list files"));
        let _ = store.upsert_activity("agent_a", "bash", true, Some("run command"));
        let _ = store.upsert_activity("agent_a", "git", false, Some("commit"));
        let _ = store.upsert_activity("agent_a", "git", true, Some("push"));

        // Test via get_profile (public API)
        let profile = store.get_profile("agent_a").unwrap().unwrap();
        assert!((profile.reputation_score - 0.75).abs() < 0.01, "Expected 0.75, got {}", profile.reputation_score);
        assert_eq!(profile.domain_scores.len(), 2, "Expected two domains");
        assert!((profile.domain_scores["bash"] - 1.0).abs() < 0.01);
        assert!((profile.domain_scores["git"] - 0.5).abs() < 0.01);
        assert_eq!(profile.total_calls, 4);

        // Agent with no activity should get 0.0
        let empty_profile = store.get_profile("new_agent").unwrap();
        assert!(empty_profile.is_none());

        // Test via list_profiles
        let all = store.list_profiles().unwrap();
        assert_eq!(all.len(), 1, "Only agent_a should be listed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_persona() {
        let dir = std::env::temp_dir().join(format!("test_set_persona_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test_agent_profiles.db");
        let store = AgentProfileStore::new(&db_path.to_string_lossy()).unwrap();

        let _ = store.upsert_activity("agent_x", "bash", true, None);
        store.set_persona("agent_x", "I am a helpful coding assistant.").unwrap();

        let profile = store.get_profile("agent_x").unwrap().unwrap();
        assert_eq!(profile.persona, "I am a helpful coding assistant.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_preferences() {
        let dir = std::env::temp_dir().join(format!("test_set_preferences_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test_agent_profiles.db");
        let store = AgentProfileStore::new(&db_path.to_string_lossy()).unwrap();

        let _ = store.upsert_activity("agent_y", "git", true, None);
        let prefs = serde_json::json!({"theme": "dark", "notifications": true});
        store.set_preferences("agent_y", &prefs).unwrap();

        let profile = store.get_profile("agent_y").unwrap().unwrap();
        assert_eq!(profile.preferences["theme"], "dark");
        assert_eq!(profile.preferences["notifications"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
