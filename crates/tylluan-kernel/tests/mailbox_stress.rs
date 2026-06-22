//! Mailbox Stress Test for TylluanNexus o3
//! 
//! Verifies:
//! 1. Atomic delivery under high concurrency
//! 2. Mutex/WAL stability under contention
//! 3. Latency tracking for performance calibration

#[cfg(test)]
mod stress {
    use tylluan_kernel::memory::mailbox::Mailbox;
    use std::sync::Arc;
    use tokio::time::{Instant, Duration};
    use tracing::info;

    const NUM_PRODUCERS: usize = 20;
    const NUM_CONSUMERS: usize = 5;
    const MESSAGES_PER_PRODUCER: usize = 50;
    const TOTAL_EXPECTED: usize = NUM_PRODUCERS * MESSAGES_PER_PRODUCER;

    #[tokio::test(flavor = "multi_thread")]
    async fn mailbox_concurrency_stress_in_memory() {
        let mailbox = Arc::new(Mailbox::in_memory().await.unwrap());
        run_stress_scenario(mailbox, "Memory-Baseline").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mailbox_concurrency_stress_on_disk() {
        let db_path = "data/test_stress_mailbox.db";
        // Cleanup old
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
        
        let mailbox = Arc::new(Mailbox::open(db_path).unwrap());
        mailbox.init().await.unwrap();
        
        run_stress_scenario(mailbox, "Disk-I/O").await;
        
        // Small delay so SQLite releases WAL locks before cleanup
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }

    async fn run_stress_scenario(mailbox: Arc<Mailbox>, label: &str) {
        let start = Instant::now();
        let mut producer_handles = vec![];
        let mut consumer_handles = vec![];

        info!("🚀 Starting Mailbox Stress Test [{}]: {} producers, {} consumers", label, NUM_PRODUCERS, NUM_CONSUMERS);

        // 1. Spawn Producers
        for p_id in 0..NUM_PRODUCERS {
            let mb = mailbox.clone();
            let handle = tokio::spawn(async move {
                let sender = format!("producer-{}", p_id);
                for m_id in 0..MESSAGES_PER_PRODUCER {
                    let payload = format!(r#"{{"data": "stress-test", "p": {}, "m": {}}}"#, p_id, m_id);
                    mb.send_mail(&sender, "central-hub", &payload).await.expect("Producer failed to send mail");
                }
            });
            producer_handles.push(handle);
        }

        // 2. Spawn Consumers (reading and marking as read)
        let received_count = Arc::new(tokio::sync::Mutex::new(0));
        for c_id in 0..NUM_CONSUMERS {
            let mb = mailbox.clone();
            let counter = received_count.clone();
            let handle = tokio::spawn(async move {
                let mut local_count = 0;
                let deadline = Instant::now() + Duration::from_secs(30);
                while local_count < TOTAL_EXPECTED && Instant::now() < deadline {
                    let batch = mb.check_mail("central-hub", true, 1000).await.expect("Consumer failed to check mail");
                    if !batch.is_empty() {
                        local_count += batch.len();
                        let mut global = counter.lock().await;
                        *global += batch.len();
                        if *global >= TOTAL_EXPECTED { break; }
                    } else {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }
                info!("📥 Consumer {} finished. Read {} messages.", c_id, local_count);
            });
            consumer_handles.push(handle);
        }

        // Wait for producers to finish sending
        for h in producer_handles {
            h.await.unwrap();
        }
        
        // Wait for consumers to finish reading (with timeout)
        let _ = tokio::time::timeout(Duration::from_secs(30), async {
            for h in consumer_handles {
                let _ = h.await;
            }
        }).await;

        let duration = start.elapsed();
        let final_count = *received_count.lock().await;
        
        info!("🏁 [{} Result] Time: {:?}, Throughput: {:.2} msg/sec", 
            label, duration, final_count as f64 / duration.as_secs_f64());

        assert_eq!(final_count, TOTAL_EXPECTED, "Integrity Failure: Some messages were lost in the void!");
    }
}
