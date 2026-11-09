use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug};
use sysinfo::System;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::memory::idle_lab::IdleLab;

// Global flag to enable/disable the autoresearch daemon at runtime
pub static AUTORESEARCH_ACTIVE: AtomicBool = AtomicBool::new(false);

/// AutoResearch Daemon (M18)
/// Monitors CPU usage and system inactivity to trigger background optimizations.
/// Implements Zero-Lag logic by yielding when CPU is > 20%.
pub async fn autoresearch_daemon(
    idle_lab: Arc<IdleLab>,
    matcher: Arc<crate::router::matcher::GuildMatcher>,
    reranker: Arc<crate::router::embeddings::RerankerSlot>,
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
            
            // D-160: el motor se pide AQUI, no al arrancar. Antes el daemon
            // recibía un `Arc<EmbeddingEngine>` y lo retenía toda su vida
            // (aunque `AUTORESEARCH_ACTIVE` estuviera en false), así que el TTL
            // de inactividad nunca podía liberar la sesión ONNX.
            let engine = matcher.engine();
            // D-160 f2: el reranker también se pide AQUÍ. Antes el daemon guardaba un
            // `Arc<RerankEngine>` toda su vida y el TTL no podía liberarlo.
            let reranker_now = reranker.get();
            // Run exactly 1 organic experiment to prevent starving resources
            idle_lab.run_experiments(engine.as_deref(), reranker_now.as_deref(), 1).await;
        } else {
            debug!("🧪 AutoResearch: System under load (CPU {:.1}% > 20%). Deferring optimization.", cpu_usage);
        }
    }
}
