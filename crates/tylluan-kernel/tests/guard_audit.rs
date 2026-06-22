use tylluan_kernel::guard::GuardedTask;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn test_guard_fast_task() {
    let guard = GuardedTask::new("Fast Task", Duration::from_secs(5));
    let result = guard.run(async move {
        Ok("success")
    }).await;
    
    assert_eq!(result.unwrap(), "success");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_guard_slow_but_alive_task() {
    let guard = GuardedTask {
        name: "Slow Task".to_string(),
        initial_timeout: Duration::from_millis(100),
        max_extensions: 2,
    };
    
    // Total wait will be 100ms (init) + 150ms (ext1) + 225ms (ext2) = 475ms
    // We sleep for 300ms, so it should succeed after 1-2 extensions.
    let result = guard.run(async move {
        sleep(Duration::from_millis(300)).await;
        Ok("recovered")
    }).await;
    
    assert_eq!(result.unwrap(), "recovered");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_guard_dead_task() {
    let guard = GuardedTask {
        name: "Zonbi Task".to_string(),
        initial_timeout: Duration::from_millis(50),
        max_extensions: 1,
    };
    
    // Total wait: 50ms + 75ms = 125ms.
    // We sleep for 500ms -> should fail.
    let result = guard.run(async move {
        sleep(Duration::from_millis(500)).await;
        Ok("should_fail")
    }).await;
    
    assert!(result.is_err(), "Guard should have abandoned the dead task");
}
