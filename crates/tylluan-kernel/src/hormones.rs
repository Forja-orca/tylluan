use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A hormone signal: system-wide broadcast with exponential decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HormoneSignal {
    pub signal_type: String,
    pub intensity: f64,
    pub source: Option<String>,
    pub payload: Option<String>,
    pub half_life_secs: u64,
    pub created_at: DateTime<Utc>,
    pub last_decay: DateTime<Utc>,
}

/// The hormonal signal bus. Tracks active signals with exponential decay.
#[derive(Debug)]
pub struct HormoneSystem {
    signals: HashMap<String, HormoneSignal>,
    pub signal_count_this_tick: u32,
}

impl Default for HormoneSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl HormoneSystem {
    pub fn new() -> Self {
        Self { 
            signals: HashMap::new(),
            signal_count_this_tick: 0,
        }
    }

    /// Emit a hormone signal. Signals accumulate intensity within the same tick.
    pub fn emit(&mut self, signal_type: &str, intensity: f64, source: Option<&str>, payload: Option<&str>, half_life_secs: u64) {
        self.signal_count_this_tick += 1;
        let now = Utc::now();
        let entry = self.signals.entry(signal_type.to_string());
        
        let new_signal = HormoneSignal {
            signal_type: signal_type.to_string(),
            intensity,
            source: source.map(|s| s.to_string()),
            payload: payload.map(|p| p.to_string()),
            half_life_secs,
            created_at: now,
            last_decay: now,
        };

        entry.and_modify(|existing| {
            // BUG-06 Fix: Accumulate intensity instead of higher-wins, but cap at 2.0 to avoid runaway
            existing.intensity = (existing.intensity + intensity).min(2.0);
            existing.source = new_signal.source.clone().or_else(|| existing.source.clone());
            existing.payload = new_signal.payload.clone().or_else(|| existing.payload.clone());
            existing.last_decay = now;
        }).or_insert(new_signal);
    }

    /// Apply exponential decay to all signals.
    /// Formula: intensity = intensity * (0.5 ^ (elapsed / half_life))
    /// - Stress (~300s half-life): Decays to baseline in ~10 min.
    /// - Novelty (~60s half-life): Rapid burst, decays fast to encourage continuous discovery.
    /// - Saturation (~600s half-life): Long-term pressure signal.
    pub fn tick(&mut self) {
        self.signal_count_this_tick = 0;
        let now = Utc::now();
        self.signals.retain(|_, signal| {
            let elapsed = (now - signal.last_decay).num_seconds().max(0) as f64;
            let half_life = signal.half_life_secs as f64;
            if half_life > 0.0 {
                // Exponential decay formula
                signal.intensity *= 0.5_f64.powf(elapsed / half_life);
            }
            signal.last_decay = now;
            signal.intensity > 0.01 // Purge trace signals
        });
    }

    /// Get active signals with intensity > 0.2 (for injection into responses).
    pub fn active_signals(&self) -> Vec<serde_json::Value> {
        self.signals.values()
            .filter(|s| s.intensity > 0.2)
            .map(|s| {
                serde_json::json!({
                    "type": s.signal_type,
                    "intensity": (s.intensity * 100.0).round() / 100.0,
                    "source": s.source,
                    "payload": s.payload,
                })
            }).collect()
    }

    /// Consume and return signals with intensity > 0.2, then reset them to near-zero.
    /// Use this to avoid showing the same signal repeatedly across responses.
    pub fn drain_signals(&mut self) -> Vec<serde_json::Value> {
        let result: Vec<serde_json::Value> = self.signals.values()
            .filter(|s| s.intensity > 0.2)
            .map(|s| serde_json::json!({
                "type": s.signal_type,
                "intensity": (s.intensity * 100.0).round() / 100.0,
                "source": s.source,
                "payload": s.payload,
            }))
            .collect();
        // Reduce drained signals so they don't repeat in next response
        for s in self.signals.values_mut() {
            if s.intensity > 0.2 {
                s.intensity *= 0.3;
            }
        }
        result
    }

    /// Get a specific signal's intensity (for routing decisions).
    pub fn get_intensity(&self, signal_type: &str) -> f64 {
        self.signals.get(signal_type).map(|s| s.intensity).unwrap_or(0.0)
    }

    /// Emit common signals:
    /// - "stress": when error rate spikes (call this from handler when catching errors)
    /// - "novelty": when a novel node is created (call from silva)
    /// - "saturation": when disk or memory is high (call from maintenance)
    pub fn emit_stress(&mut self, source: &str) {
        self.emit("stress", 0.7, Some(source), None, 300); // 5 min half-life
    }
    pub fn emit_novelty(&mut self, intensity: f64) {
        self.emit("novelty", intensity, None, None, 60); // 1 min half-life (rapid decay)
    }
    pub fn emit_saturation(&mut self, payload: &str) {
        let intensity = (payload.len() as f64 / 500.0).min(1.0);
        self.emit("saturation", intensity, None, Some(payload), 600); // 10 min half-life
    }

    /// Overall stress level (0.0–1.0) for routing decisions.
    pub fn stress_level(&self) -> f64 {
        (self.get_intensity("stress") + self.get_intensity("saturation")).min(1.0)
    }

    pub fn energy_level(&self) -> f64 {
        // High novelty and low stress = high energy
        let novelty = self.get_intensity("novelty");
        let stress = self.stress_level();
        (0.5 + novelty - stress).max(0.0).min(1.0)
    }

    pub fn focus_level(&self) -> f64 {
        // High intensity of any single signal reduces focus
        let total_intensity: f64 = self.signals.values().map(|s| s.intensity).sum();
        if total_intensity < 0.1 { return 1.0; }
        (1.0 - (total_intensity / 5.0)).max(0.0).min(1.0)
    }
}
