//! Fault injection DST — exercises PartitionableTransport's 5 modes
//! against GossipEngine in realistic partition/recovery scenarios.
//!
//! Uses the same deterministic pattern as gossip_dst.rs: InMemoryTransport
//! (mpsc channels) wrapped in PartitionableTransport for fault injection.

use std::time::Duration;
use tylluan_link::gossip::{GossipEngine, GossipConfig, GossipMessage, HardwareCaps};
use tylluan_link::transport::{FaultMode, MeshTransport, PartitionableTransport, in_memory_pair};

fn make_engine(id: &str) -> GossipEngine {
    GossipEngine::new(id.to_string(), GossipConfig::default())
}

fn default_hw() -> HardwareCaps {
    HardwareCaps::default()
}

async fn respond_to_sync(
    responder: &mut GossipEngine,
    transport: &mut impl MeshTransport,
    initiator_id: &str,
) {
    let raw = transport.receive().await.expect("responder: receive Pull");
    let msg: GossipMessage = serde_json::from_slice(&raw).unwrap();
    responder.handle_incoming_message(transport, initiator_id, &msg).await.unwrap();
    if let Ok(raw2) = transport.receive().await {
        let msg2: GossipMessage = serde_json::from_slice(&raw2).unwrap();
        responder.handle_incoming_message(transport, initiator_id, &msg2).await.ok();
    }
}

async fn respond_to_sync_with_timeout(
    responder: &mut GossipEngine,
    transport: &mut impl MeshTransport,
    initiator_id: &str,
    timeout: Duration,
) -> Result<(), ()> {
    let raw = tokio::time::timeout(timeout, transport.receive()).await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let msg: GossipMessage = serde_json::from_slice(&raw).unwrap();
    responder.handle_incoming_message(transport, initiator_id, &msg).await.unwrap();
    if let Ok(Ok(data)) = tokio::time::timeout(timeout, transport.receive()).await
        && let Ok(msg2) = serde_json::from_slice::<GossipMessage>(&data) {
            responder.handle_incoming_message(transport, initiator_id, &msg2).await.ok();
        }
    Ok(())
}

/// Partition → heal → convergence.
/// First sync attempt fails (Partition mode blocks receive), then heal to
/// Transparent and second sync succeeds.
#[tokio::test]
async fn test_fault_dst_partition_heal_convergence() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("10.0.0.1:9000", vec!["mesh".into()], default_hw());
    engine_a.store_entries(&[entry]);

    // Attempt 1: Partition mode — responder never gets the Pull
    {
        let (t_a_raw, mut t_b_raw) = in_memory_pair();
        let mut t_a = PartitionableTransport::new(t_a_raw);
        t_a.set_mode(FaultMode::Partition);

        // Both sides get timeouts: responder gets error from Partition mode,
        // initiator hangs waiting for PullResponse that never arrives.
        let sync = tokio::time::timeout(
            Duration::from_millis(1000),
            engine_b.perform_sync(&mut t_b_raw, "node-a"),
        );
        let handle = respond_to_sync_with_timeout(
            &mut engine_a, &mut t_a, "node-b", Duration::from_millis(500),
        );

        let (sync_result, _) = tokio::join!(sync, handle);
        assert!(sync_result.is_err() || sync_result.as_ref().unwrap().is_err(),
            "sync must fail under partition");
    }

    // Attempt 2: Healed — Transparent mode, sync succeeds
    {
        let (t_a_raw, mut t_b_raw) = in_memory_pair();
        let mut t_a = PartitionableTransport::new(t_a_raw);
        t_a.set_mode(FaultMode::Transparent);

        let sync = engine_b.perform_sync(&mut t_b_raw, "node-a");
        let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");
        let (result, _) = tokio::join!(sync, handle);
        result.unwrap();
    }

    assert!(
        engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "B must converge with A after partition heal"
    );
}

/// Latency injection — 100ms artificial delay on each send.
/// Sync must still succeed but take measurably longer.
#[tokio::test]
async fn test_fault_dst_latency_injection() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("10.0.0.1:9000", vec!["latency".into()], default_hw());
    engine_a.store_entries(&[entry]);

    let (t_a_raw, mut t_b_raw) = in_memory_pair();
    let mut t_a = PartitionableTransport::new(t_a_raw);
    t_a.set_mode(FaultMode::Latency(Duration::from_millis(100)));
    // B-side is transparent — only A-side has latency

    let start = std::time::Instant::now();
    let sync = engine_b.perform_sync(&mut t_b_raw, "node-a");
    let handle = respond_to_sync(&mut engine_a, &mut t_a, "node-b");
    let (result, _) = tokio::join!(sync, handle);
    result.unwrap();
    let elapsed = start.elapsed();

    // With 100ms latency, each send takes at least 100ms.
    // Push-pull does 2 sends minimum (Pull + PushResponse over the latency link).
    // Total should be > 200ms.
    assert!(elapsed >= Duration::from_millis(150),
        "sync with 100ms latency should take >150ms, took {elapsed:?}");

    assert!(
        engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "B must converge with A under latency injection"
    );
}

/// One sync attempt over a fresh transport pair, with `mode` injected on the
/// responder (A) side. The initiator (B) must complete the full pull and hold
/// A's entry for the round to count.
///
/// Failure semantics kept identical to the original inline loop: A never
/// answers (Pull dropped, PullResponse dropped) → B's 1000ms budget expires,
/// while A's own 500ms responder budget absorbs its failed second receive.
async fn attempt_sync_round(
    mode: FaultMode,
    engine_a: &mut GossipEngine,
    engine_b: &mut GossipEngine,
) -> bool {
    let (t_a_raw, mut t_b_raw) = in_memory_pair();
    let mut t_a = PartitionableTransport::new(t_a_raw);
    t_a.set_mode(mode);

    let sync = tokio::time::timeout(
        Duration::from_millis(1000),
        engine_b.perform_sync(&mut t_b_raw, "node-a"),
    );
    let handle = respond_to_sync_with_timeout(
        engine_a, &mut t_a, "node-b", Duration::from_millis(500),
    );

    let (sync_result, _) = tokio::join!(sync, handle);

    // sync_result is Ok(Ok(())) on success, Ok(Err(_)) or Err(timeout) on failure
    matches!(sync_result, Ok(Ok(())))
        && engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a")
}

/// Retry budget for the drop-rate test.
///
/// A round succeeds only if two independent 30%-drop gates on A's side pass:
/// receive Pull (`0.7`) and send PullResponse (`0.7`) — B never waits for a
/// Push ack, so P(round ok) = 0.49 and P(all 20 rounds fail) = 0.51^20
/// ≈ 1.4e-6 (~1 in 700k test runs).
///
/// At the previous budget (10 rounds) the tail was 0.51^10 ≈ 1.19e-3 (~1 in
/// 840 runs) and the test panicked on it — observed live as a 10.1s flake
/// (10 rounds × 1s timeout) with the binary reporting "3 passed; 1 failed".
const DROP_RETRY_ROUNDS: u32 = 20;

/// Partial message loss — 30% drop rate on both directions.
/// After the retry budget, sync must have converged.
#[tokio::test]
async fn test_fault_dst_drop_rate_eventual_convergence() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("10.0.0.1:9000", vec!["drop".into()], default_hw());
    engine_a.store_entries(&[entry]);

    // Fresh transports each round — drop randomness across rounds.
    let mut converged = false;
    for _round in 1..=DROP_RETRY_ROUNDS {
        if attempt_sync_round(FaultMode::Drop(0.3), &mut engine_a, &mut engine_b).await {
            converged = true;
            break;
        }
    }

    assert!(
        converged,
        "drop rate test: no convergence after {DROP_RETRY_ROUNDS} rounds with 30% loss"
    );
}

/// Loop contract regression: on a healthy link the retry round must succeed
/// on its first attempt — the same convergence check the drop-rate test uses,
/// exercised without randomness.
#[tokio::test]
async fn test_fault_dst_transparent_converges_on_first_round() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("10.0.0.1:9000", vec!["transparent".into()], default_hw());
    engine_a.store_entries(&[entry]);

    assert!(
        attempt_sync_round(FaultMode::Transparent, &mut engine_a, &mut engine_b).await,
        "healthy link must converge on the first round"
    );
    assert!(
        engine_b.entries_since(0).iter().any(|e| e.node_id == "node-a"),
        "B must store A's entry after the successful round"
    );
}

/// Error mode — transport returns Protocol error on every operation.
/// Sync must fail gracefully without panicking or corrupting state.
#[tokio::test]
async fn test_fault_dst_error_mode_graceful_failure() {
    let mut engine_a = make_engine("node-a");
    let mut engine_b = make_engine("node-b");

    engine_a.advance_clock();
    let entry = engine_a.local_entry("10.0.0.1:9000", vec!["error".into()], default_hw());
    engine_a.store_entries(&[entry]);

    let (t_a_raw, mut t_b_raw) = in_memory_pair();
    let mut t_a = PartitionableTransport::new(t_a_raw);
    t_a.set_mode(FaultMode::Error);

    let sync = tokio::time::timeout(
        Duration::from_millis(500),
        engine_b.perform_sync(&mut t_b_raw, "node-a"),
    );
    let handle = respond_to_sync_with_timeout(
        &mut engine_a, &mut t_a, "node-b", Duration::from_millis(200),
    );

    let (sync_result, _) = tokio::join!(sync, handle);

    assert!(sync_result.is_err() || sync_result.as_ref().unwrap().is_err(),
        "sync must fail under Error mode");
    assert_eq!(engine_b.entries_since(0).len(), 0, "B state must stay clean after Error mode sync failure");
}
