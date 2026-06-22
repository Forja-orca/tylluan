//! ExecutionGuard — Channel-aware tool access gating.
//!
//! Security policy:
//! - Trusted channels (stdio, sse, cli, local): full access to all tools
//! - Untrusted channels (http without auth, unknown): blocked from dangerous tools
//!
//! Dangerous tools are those that can modify the filesystem, execute commands,
//! or manage containers.

use tylluan_common::types::Channel;
use crate::registry::tools::RiskLevel;
use subtle::ConstantTimeEq;

/// Result of an execution guard check.
#[derive(Debug)]
pub struct GuardResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub requires_hitl: bool,
}

pub struct ExecutionGuard;

impl ExecutionGuard {
    /// Check whether a tool call is permitted from a given channel.
    /// Uses RiskLevel from Tool Registry as single source of truth.
    /// HITL (Human-In-The-Loop) required for High risk tools on untrusted channels.
    pub fn check(tool_name: &str, channel: &Channel, risk_level: &RiskLevel) -> GuardResult {
        let is_trusted = channel.is_trusted();
        
        // High risk tools are BLOCKED on untrusted channels
        if *risk_level == RiskLevel::High && !is_trusted {
            tracing::warn!("🚫 BLOCKED: Tool '{}' (High risk) from untrusted channel '{}'", tool_name, channel);
            return GuardResult {
                allowed: false,
                reason: Some("Security: High-risk tools are blocked on anonymous/untrusted channels.".to_string()),
                requires_hitl: false,
            };
        }

        // Medium risk: warn but allow
        if *risk_level == RiskLevel::Medium && !is_trusted {
            tracing::warn!("⚠️ Medium risk tool '{}' from untrusted channel", tool_name);
        }

        GuardResult {
            allowed: true,
            reason: None,
            requires_hitl: false,
        }
    }

    /// Legacy check for backward compatibility (defaults to Medium risk for unknown tools).
    #[deprecated(note = "Use ExecutionGuard::check() with explicit RiskLevel instead")]
    pub fn check_legacy(tool_name: &str, channel: &Channel) -> GuardResult {
        Self::check(tool_name, channel, &RiskLevel::Medium)
    }

    /// Constant-time token comparison to prevent timing attacks.
    pub fn secure_compare(token: &str, expected: &str) -> bool {
        token.as_bytes().ct_eq(expected.as_bytes()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_risk_blocked_on_untrusted() {
        let result = ExecutionGuard::check("bash_execute", &Channel::Http { authenticated: false }, &RiskLevel::High);
        assert!(!result.allowed);
        assert!(!result.requires_hitl);
    }

    #[test]
    fn test_high_risk_allows_on_trusted() {
        let result = ExecutionGuard::check("bash_execute", &Channel::Stdio, &RiskLevel::High);
        assert!(result.allowed);
        assert!(!result.requires_hitl);
    }

    #[test]
    fn test_low_risk_allows_no_hitl() {
        let result = ExecutionGuard::check("memory_search", &Channel::Http { authenticated: false }, &RiskLevel::Low);
        assert!(result.allowed);
        assert!(!result.requires_hitl);
    }
}
