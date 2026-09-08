//! J-14 DeepEvalPhase — NightConsolidation phase that runs the J-6/J-7
//! DeepEval pilot (`benchmarks/benchmark_j6_j7_deepeval.py`) on a low
//! frequency cycle (default 24h), behind an explicit opt-in flag.
//!
//! Design decisions (documented, 2026-09-07):
//! - **Subprocess, not a Rust port.** The pilot depends on the `deepeval`
//!   pip framework (FaithfulnessMetric / ContextualPrecisionMetric with a
//!   custom `DeepEvalBaseLLM` judge backed by llama_backend). Porting it to
//!   Rust would mean reimplementing a whole eval framework; the subprocess
//!   keeps the already-functional pilot 100% intact. The pilot prints a
//!   machine-readable `DEEPEVAL_RESULT_JSON=` line, which this phase parses.
//! - **Opt-in by default (default FALSE).** Same rule as
//!   `[security] coherence_gate_hybrid_enabled` (CLAUDE.md standing rule):
//!   no phase that can start real local inference is born enabled. The
//!   pilot calls the llama_backend guild, which can auto-start a
//!   llama-server subprocess — exactly the class of side effect that must
//!   stay off until the human turns it on.
//! - **Low frequency (default 24h, configurable).** The rest of the night
//!   cycle runs every 30 minutes; a full eval pass spawns real inference
//!   per trace and must not. A last-run marker file in the data dir gates
//!   the frequency without a new DB table.
//!
//! Infrastructure failures (python missing, `deepeval` not installed,
//! script absent) surface as `ok=false` PhaseReports — never panics.

use super::{Phase, PhaseContext, PhaseReport};
use chrono::{DateTime, Utc};

/// Where the pilot lives, relative to the kernel's working directory.
const PILOT_SCRIPT: &str = "benchmarks/benchmark_j6_j7_deepeval.py";

/// Generous ceiling for the subprocess: the pilot itself allows 180s per
/// judge call and evaluates every resolved trace in the DB. 30 minutes
/// covers a realistic trace volume on CPU; beyond that we treat the run
/// as hung and report failure rather than blocking the night slot forever.
const SUBPROCESS_TIMEOUT_SECS: u64 = 1800;

/// Marker filename (inside `data_dir`) holding the last attempt's RFC3339
/// timestamp. Written after EVERY attempt (success or failure) so a broken
/// setup (e.g. `deepeval` not installed) logs once per interval instead of
/// retrying on every 30-minute night cycle.
const LAST_RUN_MARKER: &str = "deep_eval_last_run";

/// Python interpreter used for the subprocess. Overridable via the
/// `TYLLUAN_DEEP_EVAL_PYTHON` env var (the project hit a hardcoded-`python3`
/// portability bug before — STATUS.md Pilar 2 — so this stays configurable).
fn python_interpreter() -> String {
    std::env::var("TYLLUAN_DEEP_EVAL_PYTHON").unwrap_or_else(|_| "python".to_string())
}

/// Frequency gate, pure so it is testable without a filesystem.
///
/// `last` is the parsed last-attempt timestamp; `None` means never ran.
/// An interval of 0 means "every night slot" (opt-in at full cadence).
fn should_run(last: Option<DateTime<Utc>>, interval_hours: u64, now: DateTime<Utc>) -> bool {
    match last {
        None => true,
        Some(t) => {
            let elapsed = now.signed_duration_since(t);
            elapsed >= chrono::Duration::hours(interval_hours as i64)
        }
    }
}

/// Extract and parse the `DEEPEVAL_RESULT_JSON={...}` line the pilot prints
/// as its machine-readable completion status. Returns the last valid JSON
/// object if the line appears multiple times.
fn parse_deepeval_result(stdout: &str) -> Option<serde_json::Value> {
    let mut last: Option<serde_json::Value> = None;
    for line in stdout.lines() {
        if let Some(json_str) = line.strip_prefix("DEEPEVAL_RESULT_JSON=")
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str)
        {
            last = Some(v);
        }
    }
    last
}

/// Read the last-attempt marker; `None` if missing or unparseable.
fn read_last_run(data_dir: &std::path::Path) -> Option<DateTime<Utc>> {
    let content = std::fs::read_to_string(data_dir.join(LAST_RUN_MARKER)).ok()?;
    DateTime::parse_from_rfc3339(content.trim())
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn write_last_run(data_dir: &std::path::Path, now: DateTime<Utc>) {
    let _ = std::fs::write(data_dir.join(LAST_RUN_MARKER), now.to_rfc3339());
}

/// J-14 DeepEvalPhase — runs the DeepEval J-6/J-7 pilot during
/// NightConsolidation when explicitly enabled via `[eval] deep_eval_enabled`.
pub struct DeepEvalPhase;

#[async_trait::async_trait]
impl Phase for DeepEvalPhase {
    fn name(&self) -> &'static str {
        "DeepEval"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        // Opt-in gate: read the flag wired from [eval] deep_eval_enabled
        // (default false — this phase can trigger real local inference).
        let (enabled, interval_hours) = {
            let server = ctx.server.read().await;
            (server.deep_eval_enabled, server.deep_eval_interval_hours)
        };
        if !enabled {
            return PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: true,
                detail: "disabled ([eval] deep_eval_enabled=false) — opt-in only".to_string(),
            };
        }

        let now = Utc::now();
        let last = read_last_run(&ctx.data_dir);
        if !should_run(last, interval_hours, now) {
            let ago = last
                .map(|t| format!("{}h ago", (now - t).num_hours().max(0)))
                .unwrap_or_else(|| "unknown".to_string());
            return PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: true,
                detail: format!("skipped — last attempt {ago}, interval {interval_hours}h"),
            };
        }

        if !std::path::Path::new(PILOT_SCRIPT).exists() {
            write_last_run(&ctx.data_dir, now);
            return PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: false,
                detail: format!("pilot script not found at {PILOT_SCRIPT} (run from repo root)"),
            };
        }

        let start = Instant::now();
        let child = match tokio::process::Command::new(python_interpreter())
            .arg(PILOT_SCRIPT)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                write_last_run(&ctx.data_dir, now);
                return PhaseReport {
                    name: self.name(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ok: false,
                    detail: format!(
                        "failed to spawn {} {PILOT_SCRIPT}: {e} (is python on PATH?)",
                        python_interpreter()
                    ),
                };
            }
        };

        // Bounded wait: kill on timeout so the night slot is never blocked.
        // wait_with_output() consumes the child, so the timeout path uses the
        // OS-level kill-by-pid (child.id()) instead of the moved value.
        let child_pid = child.id();
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                write_last_run(&ctx.data_dir, now);
                return PhaseReport {
                    name: self.name(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ok: false,
                    detail: format!("subprocess error: {e}"),
                };
            }
            Err(_) => {
                // Child was consumed by wait_with_output; kill by pid. On
                // Windows, pid reuse is a theoretical race but the window is
                // milliseconds; acceptable for a maintenance phase.
                if let Some(pid) = child_pid {
                    #[cfg(windows)]
                    let _ = std::process::Command::new("taskkill")
                        .args(["/PID", &pid.to_string(), "/F", "/T"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    #[cfg(not(windows))]
                    unsafe {
                        libc::kill(pid as i32, libc::SIGKILL);
                    }
                }
                write_last_run(&ctx.data_dir, now);
                return PhaseReport {
                    name: self.name(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ok: false,
                    detail: format!("timed out after {SUBPROCESS_TIMEOUT_SECS}s — killed"),
                };
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        write_last_run(&ctx.data_dir, now);

        if !output.status.success() {
            let stderr_tail: String = String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect();
            return PhaseReport {
                name: self.name(),
                duration_ms: start.elapsed().as_millis() as u64,
                ok: false,
                detail: format!(
                    "pilot exited {} — is `deepeval` installed? last stderr: {stderr_tail}",
                    output.status
                ),
            };
        }

        let result = match parse_deepeval_result(&stdout) {
            Some(v) => v,
            None => {
                return PhaseReport {
                    name: self.name(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ok: false,
                    detail: "pilot exited 0 but printed no DEEPEVAL_RESULT_JSON line".to_string(),
                };
            }
        };

        // Persist the full machine-readable result for dashboards/history.
        let out_dir = ctx.data_dir.join("benchmarks");
        let _ = std::fs::create_dir_all(&out_dir);
        let out_path = out_dir.join("j6_j7_last_run.json");
        let saved = std::fs::write(&out_path, serde_json::to_string_pretty(&result).unwrap_or_default())
            .is_ok();

        let status = result.get("status").and_then(|s| s.as_str()).unwrap_or("unknown");
        let summary = match (
            result.get("trace_count").and_then(|v| v.as_i64()),
            result.get("avg_faithfulness").and_then(|v| v.as_f64()),
            result.get("avg_contextual_precision").and_then(|v| v.as_f64()),
        ) {
            (Some(n), Some(f), Some(p)) => format!(
                "status={status} traces={n} faithfulness={f:.2} contextual_precision={p:.2}"
            ),
            _ => format!("status={status}"),
        };

        PhaseReport {
            name: self.name(),
            duration_ms: start.elapsed().as_millis() as u64,
            ok: true,
            detail: format!("{summary} — saved={saved} at {out_path:?}"),
        }
    }
}

use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn should_run_when_never_ran() {
        let now = ts(1_000_000);
        assert!(should_run(None, 24, now));
    }

    #[test]
    fn should_not_run_inside_interval() {
        let now = ts(1_000_000);
        let last = ts(1_000_000 - 3600); // 1h ago
        assert!(!should_run(Some(last), 24, now));
    }

    #[test]
    fn should_run_after_interval_elapsed() {
        let now = ts(1_000_000);
        let last = ts(1_000_000 - 25 * 3600); // 25h ago
        assert!(should_run(Some(last), 24, now));
    }

    #[test]
    fn interval_zero_always_runs() {
        let now = ts(1_000_000);
        let last = ts(1_000_000 - 60); // 1min ago
        assert!(should_run(Some(last), 0, now));
    }

    #[test]
    fn parse_extracts_json_line() {
        let stdout = "some log line\nDEEPEVAL_RESULT_JSON={\"status\":\"functional\",\"trace_count\":2}\n";
        let v = parse_deepeval_result(stdout).expect("should parse");
        assert_eq!(v["status"], "functional");
        assert_eq!(v["trace_count"], 2);
    }

    #[test]
    fn parse_returns_none_without_marker_line() {
        assert!(parse_deepeval_result("no marker here\n").is_none());
    }

    #[test]
    fn parse_returns_none_for_invalid_json() {
        assert!(parse_deepeval_result("DEEPEVAL_RESULT_JSON={not json}\n").is_none());
    }

    #[test]
    fn parse_takes_last_valid_marker() {
        let stdout = "DEEPEVAL_RESULT_JSON={\"status\":\"failed\"}\nDEEPEVAL_RESULT_JSON={\"status\":\"functional\"}\n";
        let v = parse_deepeval_result(stdout).expect("should parse");
        assert_eq!(v["status"], "functional");
    }
}
