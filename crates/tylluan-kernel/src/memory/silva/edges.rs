use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use std::collections::{HashMap, HashSet, VecDeque};

use super::GraphNode;

impl super::SilvaDB {
    /// Add a typed edge (relationship) between two nodes.
    /// Upserts — if the edge exists, weight and metadata are updated.
    pub async fn add_edge(
        &self,
        source: &str,
        target: &str,
        edge_type: &str,
        weight: f64,
        metadata: &str,
    ) -> Result<()> {
        self.add_edge_with_validity(source, target, edge_type, weight, metadata, None, None).await
    }

    /// Add a typed edge with temporal validity constraints.
    pub async fn add_edge_with_validity(
        &self,
        source: &str,
        target: &str,
        edge_type: &str,
        weight: f64,
        metadata: &str,
        valid_from: Option<i64>,
        valid_until: Option<i64>,
    ) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            conn.execute(
                "INSERT INTO edges (source, target, type, weight, metadata, valid_from, valid_until)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(source, target, type) DO UPDATE SET
                    weight = excluded.weight,
                    metadata = excluded.metadata,
                    valid_from = excluded.valid_from,
                    valid_until = excluded.valid_until",
                params![source, target, edge_type, weight, metadata, valid_from, valid_until],
            )?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    /// Increment weight of an existing edge (Hebbian reinforcement).
    /// Returns `true` if the edge existed and was strengthened, `false` if no matching edge was found.
    pub async fn strengthen_edge(&self, source: &str, target: &str, edge_type: &str, delta: f64) -> Result<bool> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let affected = conn.execute(
                "UPDATE edges SET weight = weight + ?1 
                 WHERE source = ?2 AND target = ?3 AND type = ?4
                   AND (valid_until IS NULL OR valid_until >= strftime('%s', 'now'))",
                params![delta, source, target, edge_type],
            )?;
            Ok(affected > 0)
        })
    }

    /// Find nodes that have edges from both agents (shared knowledge).
    /// Returns nodes that both `agent_a` and `agent_b` have edges pointing to, ordered by weight descending.
    pub async fn find_shared_knowledge(&self, agent_a: &str, agent_b: &str, limit: usize) -> Result<Vec<GraphNode>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT DISTINCT n.id, n.type, n.content, n.metadata, n.weight, n.protected, n.conflicted, n.topic_key, n.created_at, n.updated_at, n.shareable
                 FROM nodes n
                 INNER JOIN edges e1 ON e1.target = n.id
                 INNER JOIN edges e2 ON e2.target = n.id
                 WHERE e1.source = ?1 AND e2.source = ?2
                   AND (e1.valid_until IS NULL OR e1.valid_until >= strftime('%s', 'now'))
                   AND (e2.valid_until IS NULL OR e2.valid_until >= strftime('%s', 'now'))
                 ORDER BY n.weight DESC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![agent_a, agent_b, limit as i64], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    content: row.get(2)?,
                    metadata: row.get(3)?,
                    weight: row.get(4)?,
                    protected: row.get::<_, i32>(5)? != 0,
                    conflicted: row.get::<_, i32>(6)? != 0,
                    topic_key: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    shareable: row.get::<_, i32>(10)? != 0,
                    last_touched: Utc::now(),
                    valid_from: None,
                    valid_until: None,
                })
            })?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }

    /// BFS traversal from a start node, collecting context up to max_depth hops.
    /// Uses a single lock acquisition for the entire traversal to ensure snapshot isolation.
    pub async fn get_context(&self, start_node_id: &str, max_depth: usize) -> Result<Vec<GraphNode>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut visited = std::collections::HashSet::new();
            let mut current_level = vec![start_node_id.to_string()];
            let mut results = Vec::new();

            for _depth in 0..=max_depth {
                if current_level.is_empty() {
                    break;
                }

                let placeholders = std::iter::repeat_n("?", current_level.len())
                    .collect::<Vec<_>>()
                    .join(",");

                let nodes_query = format!(
                    "SELECT id, type, content, metadata, weight, protected, conflicted, topic_key, created_at, updated_at, shareable FROM nodes WHERE id IN ({})",
                    placeholders
                );
                let mut stmt = conn.prepare(&nodes_query)?;
                let nodes: Vec<GraphNode> = stmt.query_map(rusqlite::params_from_iter(current_level.clone()), |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        node_type: row.get(1)?,
                        content: row.get(2)?,
                        metadata: row.get(3)?,
                        weight: row.get(4)?,
                        protected: row.get::<_, i32>(5)? != 0,
                        conflicted: row.get::<_, i32>(6)? != 0,
                        topic_key: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                        shareable: row.get::<_, i32>(10)? != 0,
                        last_touched: Utc::now(),
                        valid_from: None,
                        valid_until: None,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

                for node in &nodes {
                    visited.insert(node.id.clone());
                }
                results.extend(nodes);

                let edges_query = format!(
                    "SELECT source, target FROM edges WHERE (source IN ({}) OR target IN ({})) AND (valid_until IS NULL OR valid_until >= strftime('%s', 'now'))",
                    placeholders, placeholders
                );
                let mut stmt = conn.prepare(&edges_query)?;
                let all_ids: Vec<String> = current_level.clone();
                let edges: Vec<(String, String)> = stmt.query_map(rusqlite::params_from_iter(all_ids.iter().chain(all_ids.iter())), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

                let mut next_level = Vec::new();
                for (source, target) in edges {
                    if !visited.contains(&source) {
                        next_level.push(source);
                    }
                    if !visited.contains(&target) {
                        next_level.push(target);
                    }
                }

                current_level = next_level;
            }

            Ok(results)
        })
    }

    /// Find the shortest undirected edge path between two nodes, capped by max_depth hops.
    pub async fn shortest_path(&self, source: &str, target: &str, max_depth: usize) -> Result<Option<Vec<String>>> {
        if source == target {
            return Ok(Some(vec![source.to_string()]));
        }

        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare("SELECT source, target FROM edges WHERE valid_until IS NULL OR valid_until >= strftime('%s', 'now')")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;

            let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
            for row in rows {
                let (s, t) = row?;
                adjacency.entry(s.clone()).or_default().push(t.clone());
                adjacency.entry(t).or_default().push(s);
            }

            let mut visited: HashSet<String> = HashSet::new();
            let mut previous: HashMap<String, String> = HashMap::new();
            let mut queue: VecDeque<(String, usize)> = VecDeque::new();

            visited.insert(source.to_string());
            queue.push_back((source.to_string(), 0));

            while let Some((current, depth)) = queue.pop_front() {
                if depth >= max_depth {
                    continue;
                }

                if let Some(neighbors) = adjacency.get(&current) {
                    for neighbor in neighbors {
                        if !visited.insert(neighbor.clone()) {
                            continue;
                        }

                        previous.insert(neighbor.clone(), current.clone());
                        if neighbor == target {
                            let mut path = vec![target.to_string()];
                            let mut cursor = target.to_string();

                            while cursor != source {
                                match previous.get(&cursor) {
                                    Some(parent) => {
                                        cursor = parent.clone();
                                        path.push(cursor.clone());
                                    }
                                    None => return Ok(None),
                                }
                            }

                            path.reverse();
                            return Ok(Some(path));
                        }

                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }

            Ok(None)
        })
    }

    pub async fn get_triples_by_entity(&self, entity: &str) -> Result<Vec<serde_json::Value>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT source, type, target, weight, metadata FROM edges WHERE source = ?1 OR target = ?1"
            )?;
            let rows = stmt.query_map(params![entity], |row| {
                Ok(serde_json::json!({
                    "subject": row.get::<_, String>(0)?,
                    "predicate": row.get::<_, String>(1)?,
                    "object": row.get::<_, String>(2)?,
                    "weight": row.get::<_, f64>(3)?,
                    "metadata": row.get::<_, String>(4)?
                }))
            })?;
            let mut results = Vec::new();
            for row in rows { results.push(row?); }
            Ok(results)
        })
    }
    pub async fn get_all_edges(&self) -> Result<Vec<serde_json::Value>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT source, target, type, weight FROM edges"
            )?;

            let rows = stmt.query_map([], |row| {
                Ok(serde_json::json!({
                    "source": row.get::<_, String>(0)?,
                    "target": row.get::<_, String>(1)?,
                    "type": row.get::<_, String>(2)?,
                    "weight": row.get::<_, f64>(3)?
                }))
            })?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
    }
    pub async fn get_triples_for_entity(&self, entity: &str) -> Result<Vec<(String, String, String)>> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            let mut stmt = conn.prepare(
                "SELECT source, type, target FROM edges WHERE source = ?1 OR target = ?1"
            )?;
            let rows = stmt.query_map(params![entity], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            let mut results = Vec::new();
            for row in rows { results.push(row?); }
            Ok(results)
        })
    }
}
