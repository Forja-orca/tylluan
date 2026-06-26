use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug};
use sysinfo::System;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::memory::idle_lab::IdleLab;
use crate::router::embeddings::EmbeddingEngine;

// Global flag to enable/disable the autoresearch daemon at runtime
pub static AUTORESEARCH_ACTIVE: AtomicBool = AtomicBool::new(false);

/// AutoResearch Daemon (M18)
/// Monitors CPU usage and system inactivity to trigger background optimizations.
/// Implements Zero-Lag logic by yielding when CPU is > 20%.
pub async fn autoresearch_daemon(
    idle_lab: Arc<IdleLab>,
    engine: Option<Arc<EmbeddingEngine>>,
    reranker: Option<Arc<crate::router::embeddings::RerankEngine>>,
) {
    let mut sys = System::new();
    sys.refresh_cpu_usage();
    
    info!("🧪 AutoResearch Daemon initialized. Waiting for trigger...");

    loop {
        // Evaluate every 60 seconds
        sleep(Duration::from_secs(60)).await;
        
        if !AUTORESEARCH_ACTIVE.load(Ordering::Relaxed) {
            continue;
        }

        sys.refresh_cpu_usage();
        // Calculate average CPU usage across all cores
        let cpus = sys.cpus();
        let cpu_usage = if !cpus.is_empty() {
            cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
        } else {
            0.0
        };
        
        // Zero-Lag UI condition: Only run if CPU < 20%
        if cpu_usage < 20.0 {
            info!("🧪 AutoResearch: CPU is idle ({:.1}% < 20%). Starting organic benchmark cycle...", cpu_usage);
            
            // Run exactly 1 organic experiment to prevent starving resources
            idle_lab.run_experiments(engine.as_deref(), reranker.as_deref(), 1).await;
        } else {
            debug!("🧪 AutoResearch: System under load (CPU {:.1}% > 20%). Deferring optimization.", cpu_usage);
        }
    }
}
