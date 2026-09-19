//! # Dispatch executor — the last mile (BWC-4, coloquio_dispatcher_spec.md)
//!
//! Closes the dispatcher loop: periodically polls the BWC-1 queue for
//! dispatches a human has approved (hash-bound, BWC-2), claims each one
//! CAS-style (`Approved → Executed`, exactly-once even with concurrent
//! pollers), and spawns the FIXED argv stored in the row.
//!
//! Why this is safe to exist (the three mitigation layers, José's decision
//! of 2026-09-17 — both are mandatory, not either/or):
//! 1. The filter chain upstream (`dispatch_subscriber::evaluate_event`,
//!    Deep's BWC-3) is fail-closed: no `[wake]` config, no allowlisted
//!    author, no queue row. Today `agents.toml` ships no `[wake]` at all —
//!    zero dispatches exist until the operator enables one.
//! 2. A row only becomes `Approved` through the human HITL endpoint
//!    (BWC-2) whose approval is hash-bound to the exact content snapshot
//!    the reviewer saw. This executor NEVER approves anything.
//! 3. The claim here is a CAS: exactly one executor instance can take a
//!    given dispatch, and only from `Approved`.
//!
//! Command construction is deliberately rigid: `argv[0]` is the program,
//! the rest are literal args, no shell is ever involved, stdio is null,
//! and the child inherits the kernel's working directory (the repo root in
//! production). Spawn failures are logged loudly and the row STAYS
//! `Executed` — automatic retry would be an unattended loop hammering the
//! store, so retry is a human decision.
//!
//! CONTRACT-01 note: this module is a kernel-internal security primitive,
//! not an MCP tool — `all_tools()` still exposes exactly the 5 sovereign
//! tools. WORK_PROTOCOL remains the authority: `--exec`-class execution is
//! activated only by a `[wake]` config the operator wrote, per-message.

use crate::security::dispatch_queue::{ClaimOutcome, DispatchQueue, PendingDispatch};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// How often the executor polls the queue for human-approved dispatches.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// What one `execute_one` attempt did. Exists so tests can pin the
/// executor's contract without asserting on real child processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionReport {
    /// Claim won and the fixed argv spawned successfully.
    Spawned,
    /// The row existed but was not `Approved` — no spawn (idempotency:
    /// second claim, already executed, still pending, rejected, expired).
    NotClaimed(&'static str),
    /// Claim won but the spawn failed. The row stays `Executed`.
    SpawnFailed(String),
    /// The id does not exist in the queue.
    NotFound,
}

/// Claim one dispatch CAS-style and, when the claim wins, spawn its fixed
/// argv. The transition lands BEFORE the process spawns.
pub fn execute_one(queue: &DispatchQueue, id: &str) -> anyhow::Result<ExecutionReport> {
    match queue.claim_approved(id)? {
        ClaimOutcome::NotFound => Ok(ExecutionReport::NotFound),
        ClaimOutcome::AlreadyResolved(state) => Ok(ExecutionReport::NotClaimed(state.as_str())),
        ClaimOutcome::Claimed(dispatch) => Ok(spawn_fixed_argv(&dispatch)),
    }
}

fn spawn_fixed_argv(dispatch: &PendingDispatch) -> ExecutionReport {
    let Some(program) = dispatch.command.first() else {
        // enqueue() cannot produce an empty argv, but a security primitive
        // never trusts "cannot happen": fail loudly, row stays Executed.
        error!(
            "[dispatch-executor] dispatch={} has EMPTY argv — refusing to spawn (row stays Executed)",
            dispatch.id
        );
        return ExecutionReport::SpawnFailed("empty argv".to_string());
    };
    let mut cmd = Command::new(program);
    cmd.args(&dispatch.command[1..]);
    // No shell, no interpolation: the argv was written by the operator in
    // `[wake].command` and persisted verbatim. stdio detached — the kernel
    // is a service, not a console for children.
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            info!(
                "[dispatch-executor] SPAWNED dispatch={} agent={} pid={} argv={:?} hash={} (human-approved)",
                dispatch.id,
                dispatch.agent_id,
                child.id(),
                dispatch.command,
                &dispatch.content_hash[..8]
            );
            // Reap asynchronously so long-lived kernels don't accumulate
            // zombies; we never block the executor loop on a child.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            ExecutionReport::Spawned
        }
        Err(e) => {
            error!(
                "[dispatch-executor] spawn FAILED dispatch={} agent={} argv={:?}: {e} — row stays Executed; retry is a human decision",
                dispatch.id, dispatch.agent_id, dispatch.command
            );
            ExecutionReport::SpawnFailed(e.to_string())
        }
    }
}

/// Background task: poll the queue and execute human-approved dispatches.
/// The ONLY source of work is rows the HITL endpoint (BWC-2) moved to
/// `Approved` — this loop can never approve, only execute.
pub fn spawn_dispatch_executor(queue: Arc<DispatchQueue>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "[dispatch-executor] active (poll interval {}s) — executes ONLY hash-approved dispatches",
            POLL_INTERVAL.as_secs()
        );
        loop {
            match queue.list_approved() {
                Ok(approved) => {
                    for d in approved {
                        if let Err(e) = execute_one(&queue, &d.id) {
                            error!("[dispatch-executor] execute_one failed for {}: {e:#}", d.id);
                        }
                    }
                }
                Err(e) => {
                    warn!("[dispatch-executor] list_approved failed, retrying next tick: {e:#}");
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::dispatch_queue::DispatchState;

    fn queue() -> DispatchQueue {
        DispatchQueue::in_memory().unwrap()
    }

    fn approved(queue: &DispatchQueue, agent: &str, argv: Vec<String>) -> crate::security::dispatch_queue::PendingDispatch {
        let d = queue.enqueue(agent, "claude-code", "general", 1, "snapshot", argv).unwrap();
        match queue.approve(&d.id, &d.content_hash).unwrap() {
            crate::security::dispatch_queue::ApprovalOutcome::Transitioned => {}
            other => panic!("expected approval to transition, got {other:?}"),
        }
        d
    }

    #[test]
    fn executor_spawns_real_command_exactly_once() {
        // Real spawn once (cross-platform harmless one-shot), then the row
        // is Executed and a second attempt reports NotClaimed — the
        // exactly-once guarantee observable end to end.
        let q = queue();
        let argv: Vec<String> = if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), "exit 0".into()]
        } else {
            vec!["true".into()]
        };
        let d = approved(&q, "deep", argv);
        assert_eq!(execute_one(&q, &d.id).unwrap(), ExecutionReport::Spawned);
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Executed);
        assert_eq!(
            execute_one(&q, &d.id).unwrap(),
            ExecutionReport::NotClaimed("Executed")
        );
    }

    #[test]
    fn executor_never_touches_unapproved_rows() {
        // Pending (human hasn't decided), Rejected and nonexistent ids must
        // all report a skip — the executor cannot become an approval path.
        let q = queue();
        let pending = q.enqueue("deep", "claude-code", "general", 2, "s", vec!["cmd".into()]).unwrap();
        assert_eq!(
            execute_one(&q, &pending.id).unwrap(),
            ExecutionReport::NotClaimed("Pending")
        );
        assert_eq!(q.get(&pending.id).unwrap().unwrap().state, DispatchState::Pending);

        let rejected = q.enqueue("deep", "claude-code", "general", 3, "s", vec!["cmd".into()]).unwrap();
        q.reject(&rejected.id).unwrap();
        assert_eq!(
            execute_one(&q, &rejected.id).unwrap(),
            ExecutionReport::NotClaimed("Rejected")
        );

        assert_eq!(execute_one(&q, "no-such-id").unwrap(), ExecutionReport::NotFound);
    }

    #[test]
    fn spawn_failure_keeps_row_executed_and_reports() {
        // A claim that wins but cannot spawn must NOT roll back to Approved
        // (no automatic retry loop): row stays Executed, error is reported.
        let q = queue();
        let d = approved(&q, "deep", vec!["definitely-not-a-real-binary-4f8a2c".into()]);
        let report = execute_one(&q, &d.id).unwrap();
        assert!(matches!(report, ExecutionReport::SpawnFailed(_)), "got {report:?}");
        assert_eq!(q.get(&d.id).unwrap().unwrap().state, DispatchState::Executed);
    }
}
