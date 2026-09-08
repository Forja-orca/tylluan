//! Phase 0 SLM Society Evaluation Phase — NightConsolidation phase that
//! runs the 3-arm SLM deliberative society benchmark harness
//! (`benchmarks/spikes/slm_society/slm_society_harness.py`) on a low frequency
//! cycle (default 24h), strictly gated behind `[eval] slm_society_eval_enabled`.
//!
//! Architectural principles (ADR-011 / DESIGN_slm_society_phase0.md):
//! - **Opt-in by default (default FALSE).** Preserves the core invariant that
//!   no local inference starts without explicit operator opt-in.
//! - **3-Arm Hypothesis Falsation.** Evaluates Arm A (1-pass Baseline),
//!   Arm B (Self-MoA 3-sample compute-matched), and Arm C (A-SSA Asymmetric
//!   Sequential Scratchpad Arbitration).
//! - **Isolated from Critical Path.** Executes purely within NightConsolidation
//!   with OS-level timeout protection; zero impact on live interactive routing.

use super::{Phase, PhaseContext, PhaseReport};
use chrono::{DateTime, Utc};
use std::time::Instant;

/// Path to the SLM society benchmark harness relative to the kernel root.
const HARNESS_SCRIPT: &str = "benchmarks/spikes/slm_society/slm_society_harness.py";

/// Subprocess timeout in seconds (30 minutes ceiling for batch evaluation on CPU).
const SUBPROCESS_TIMEOUT_SECS: u64 = 1800;

/// Marker filename in `data_dir` holding the last run timestamp.
const LAST_RUN_MARKER: &str = "slm_society_eval_last_run";

/// Resolve python interpreter name from environment or default to "python".
fn python_interpreter() -> String {
    std::env::var("TYLLUAN_SLM_SOCIETY_PYTHON")
        .or_else(|_| std::env::var("TYLLUAN_DEEP_EVAL_PYTHON"))
        .unwrap_or_else(|_| "python".to_string())
}

/// Frequency check: returns true if never run or elapsed >= interval_hours.
fn should_run(last: Option<DateTime<Utc>>, interval_hours: u64, now: DateTime<Utc>) -> bool {
    match last {
        None => true,
        Some(t) => {
            let elapsed = now.signed_duration_since(t);
            elapsed >= chrono::Duration::hours(interval_hours as i64)
        }
    }
}

/// Parse machine-readable `SLM_SOCIETY_RESULT_JSON={...}` from stdout.
fn parse_society_result(stdout: &str) -> Option<serde_json::Value> {
    let mut last: Option<serde_json::Value> = None;
    for line in stdout.lines() {
        if let Some(json_str) = line.strip_prefix("SLM_SOCIETY_RESULT_JSON=")
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str)
        {
            last = Some(v);
        }
    }
    last
}

fn read_last_run(data_dir: &std::path::Path) -> Option<DateTime<Utc>> {
    let content = std::fs::read_to_string(data_dir.join(LAST_RUN_MARKER)).ok()?;
    DateTime::parse_from_rfc3339(content.trim())
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn write_last_run(data_dir: &std::path::Path, now: DateTime<Utc>) {
    let _ = std::fs::write(data_dir.join(LAST_RUN_MARKER), now.to_rfc3339());
}

/// Phase 0 SLM Society Phase for NightConsolidation.
pub struct SlmSocietyPhase;

#[async_trait::async_trait]
impl Phase for SlmSocietyPhase {
    fn name(&self) -> &'static str {
        "SlmSocietyEval"
    }

    async fn run(&self, ctx: &PhaseContext) -> PhaseReport {
        let (enabled, interval_hours) = {
            let server = ctx.server.read().await;
            (
                server.slm_society_eval_enabled,
                server.slm_society_eval_interval_hours,
            )
        };

        if !enabled {
            return PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: true,
                detail: "disabled ([eval] slm_society_eval_enabled=false) — opt-in only".to_string(),
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

        if !std::path::Path::new(HARNESS_SCRIPT).exists() {
            write_last_run(&ctx.data_dir, now);
            return PhaseReport {
                name: self.name(),
                duration_ms: 0,
                ok: false,
                detail: format!("harness script not found at {HARNESS_SCRIPT} (run from repo root)"),
            };
        }

        let start = Instant::now();
        let child = match tokio::process::Command::new(python_interpreter())
            .arg(HARNESS_SCRIPT)
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
                        "failed to spawn {} {HARNESS_SCRIPT}: {e} (is python on PATH?)",
                        python_interpreter()
                    ),
                };
            }
        };

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
                if let Some(pid) = child_pid {
                    #[cfg(windows)]
                    let _ = std::process::Command::new("taskkill")
                        .args(["/PID", &pid.to_string(), "/F", "/T"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    #[cfg(not(windows))]
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                write_last_run(&ctx.data_dir, now);
                return PhaseReport {
                    name: self.name(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    ok: false,
                    detail: format!("timed out after {SUBPROCESS_TIMEOUT_SECS}s"),
                };
            }
        };

        write_last_run(&ctx.data_dir, now);
        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let err_snippet: String = stderr
                .lines()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            return PhaseReport {
                name: self.name(),
                duration_ms,
                ok: false,
                detail: format!(
                    "exit {}: {}",
                    output.status.code().unwrap_or(-1),
                    if err_snippet.is_empty() {
                        "no stderr".to_string()
                    } else {
                        err_snippet
                    }
                ),
            };
        }

        if let Some(res) = parse_society_result(&stdout) {
            let arm_a = res.get("arm_a_accuracy").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let arm_b = res.get("arm_b_accuracy").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let arm_c = res.get("arm_c_accuracy").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let gate = res.get("gate_verdict").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
            let cases_count = res.get("num_cases").and_then(|v| v.as_u64()).unwrap_or(0);

            PhaseReport {
                name: self.name(),
                duration_ms,
                ok: true,
                detail: format!(
                    "completed ({cases_count} cases) — Arm A: {arm_a:.1}%, Arm B: {arm_b:.1}%, Arm C: {arm_c:.1}%, Gate: {gate}"
                ),
            }
        } else {
            PhaseReport {
                name: self.name(),
                duration_ms,
                ok: true,
                detail: "completed (raw stdout parsed, no JSON marker found)".to_string(),
            }
        }
    }
}

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
    fn parse_extracts_society_result_json() {
        let stdout = "some log line\nSLM_SOCIETY_RESULT_JSON={\"gate_verdict\":\"GO\",\"arm_a_accuracy\":46.7,\"arm_b_accuracy\":60.0,\"arm_c_accuracy\":66.7,\"num_cases\":15}\n";
        let v = parse_society_result(stdout).expect("should parse");
        assert_eq!(v["gate_verdict"], "GO");
        assert_eq!(v["arm_c_accuracy"], 66.7);
        assert_eq!(v["num_cases"], 15);
    }

    #[test]
    fn parse_returns_none_without_marker_line() {
        assert!(parse_society_result("no marker here\n").is_none());
    }

    #[test]
    fn parse_returns_none_for_invalid_json() {
        assert!(parse_society_result("SLM_SOCIETY_RESULT_JSON={invalid json}\n").is_none());
    }

    #[test]
    fn parse_takes_last_valid_marker() {
        let stdout = "SLM_SOCIETY_RESULT_JSON={\"gate_verdict\":\"STOP\"}\nSLM_SOCIETY_RESULT_JSON={\"gate_verdict\":\"GO\"}\n";
        let v = parse_society_result(stdout).expect("should parse");
        assert_eq!(v["gate_verdict"], "GO");
    }
}
