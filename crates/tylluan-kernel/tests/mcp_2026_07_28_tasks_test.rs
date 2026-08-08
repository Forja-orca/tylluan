//! Integration test for MCP 2026-07-28 Tasks extension & server/discover endpoints.

use tylluan_kernel::memory::jobs::JobQueue;
use std::path::Path;

#[test]
fn test_mcp_tasks_job_queue_lifecycle() {
    let q = JobQueue::open(Path::new(":memory:")).unwrap();

    // 1. Enqueue job
    let task_id = q.enqueue("mcp_task", &serde_json::json!({"action": "analyze"})).unwrap();
    assert!(task_id.starts_with("job:mcp_task:"));

    // 2. Query via get_by_id
    let job = q.get_by_id(&task_id).unwrap().expect("job should exist");
    assert_eq!(job.status, "pending");

    // 3. Update status (working / input_required / completed)
    let updated = q.update_status(&task_id, "working", Some(&serde_json::json!({"progress": 25}))).unwrap();
    assert!(updated);
    let job_working = q.get_by_id(&task_id).unwrap().unwrap();
    assert_eq!(job_working.status, "working");

    // 4. Cancel task
    let cancelled = q.cancel(&task_id).unwrap();
    assert!(cancelled);
    let job_cancelled = q.get_by_id(&task_id).unwrap().unwrap();
    assert_eq!(job_cancelled.status, "cancelled");
}
