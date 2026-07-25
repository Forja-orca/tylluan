use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tracing::info;

const DEFAULT_CAPACITY: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceEntry {
    pub intent: String,
    pub embedding: Vec<f32>,
    pub guild_chosen: String,
    pub guild_confidence: f32,
    pub success: bool,
    pub agent_id: Option<String>,
    pub timestamp: String,
    pub latency_ms: u64,
}

pub struct ReplayBuffer {
    entries: VecDeque<ExperienceEntry>,
    capacity: usize,
    export_path: PathBuf,
}

impl ReplayBuffer {
    pub fn new(data_dir: &Path) -> Self {
        let export_path = data_dir.join("mlp_experiences.jsonl");
        Self {
            entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
            capacity: DEFAULT_CAPACITY,
            export_path,
        }
    }

    pub fn push(&mut self, entry: ExperienceEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.entries.len() >= self.capacity
    }

    pub fn export_training_data(&self) -> std::io::Result<usize> {
        if self.entries.is_empty() {
            return Ok(0);
        }
        if let Some(parent) = self.export_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.export_path)?;
        let mut writer = BufWriter::new(file);
        let mut count = 0;
        for entry in &self.entries {
            let line = serde_json::to_string(entry).unwrap_or_default();
            writeln!(writer, "{line}")?;
            count += 1;
        }
        info!("Exported {} training experiences to {:?}", count, self.export_path);
        Ok(count)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Record a routing outcome — shorthand for push with timestamp auto-fill.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        intent: &str,
        embedding: Vec<f32>,
        guild_chosen: &str,
        guild_confidence: f32,
        success: bool,
        agent_id: Option<&str>,
        latency_ms: u64,
    ) {
        self.push(ExperienceEntry {
            intent: intent.to_string(),
            embedding,
            guild_chosen: guild_chosen.to_string(),
            guild_confidence,
            success,
            agent_id: agent_id.map(|s| s.to_string()),
            timestamp: Utc::now().to_rfc3339(),
            latency_ms,
        });
    }
}