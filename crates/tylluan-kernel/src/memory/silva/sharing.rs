//! Sharing policy for SilvaDB.
//!
//! Controls which nodes are `shareable = 1` based on configurable filters:
//! weight threshold, recency, and node type. Includes a kill-switch that
//! resets all nodes to `shareable = 0`.

use anyhow::Result;
use chrono::Utc;
use rusqlite::params;

use super::SilvaDB;

impl SilvaDB {
    /// Reset all nodes to shareable = 0 (kill-switch).
    pub async fn reset_all_shareable(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute("UPDATE nodes SET shareable = 0", [])?;
            Ok(())
        })
    }

    /// Apply sharing policy: set shareable = 1 on nodes matching weight, recency, and type filters.
    pub async fn apply_sharing_policy(
        &self,
        min_weight: f64,
        min_activity_hours: u64,
        node_types: &[String],
    ) -> Result<()> {
        let since_secs = Utc::now().timestamp() - (min_activity_hours as i64 * 3600);
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute("UPDATE nodes SET shareable = 0 WHERE protected = 0", [])?;
            if node_types.is_empty() {
                conn.execute(
                    "UPDATE nodes SET shareable = 1 WHERE weight >= ?1 AND updated_at >= ?2 AND protected = 0",
                    params![min_weight, since_secs],
                )?;
            } else {
                for t in node_types {
                    conn.execute(
                        "UPDATE nodes SET shareable = 1 WHERE type = ?1 AND weight >= ?2 AND updated_at >= ?3 AND protected = 0",
                        params![t, min_weight, since_secs],
                    )?;
                }
            }
            Ok(())
        })
    }
}
