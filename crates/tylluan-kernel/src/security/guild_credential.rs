//! D-34 �?" Guild-class credentials.
//!
//! Until now every stdio guild received `TYLLUAN_TOKEN` at spawn, i.e. the
//! **master** token: a compromised guild (malicious Python dependency, a
//! `bash` allowlist bypass) could call the kernel's operator-class routes
//! (`admin/*`, `maintenance/*`, `config`, `dispatches/*`, approvals) with full
//! authority. This module issues an **additive, per-process** credential for
//! guilds instead. The master token is never generated, rotated or touched.
//!
//! Properties:
//! - one random 32-byte token per guild spawn, prefixed `tyg_`;
//! - in memory only: never written to disk, never logged, gone on restart;
//! - revocable per guild (the store maps token �+' guild name);
//! - it authenticates HTTP requests as [`AclAuthKind::GuildToken`], which the
//!   existing operator gate (`operator_gate_allows`) already rejects, so the
//!   guild inherits no operator authority by default.
//!
//! The single audited exception (Jose/TL, 2026-10-23, option A) is
//! `POST /api/v1/silva/edge`, because `guilds/core/silva_utils.py` writes
//! triples there and no non-operator route exists yet. See
//! `docs/internal/DESIGN_D-34_credencial_guilds_deep_2026-10.md`.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;

use crate::security::guard::ExecutionGuard;

/// Prefix that makes a guild credential recognizable in logs/audits without
/// revealing it (the operator can tell "this is not the master token").
pub const TOKEN_PREFIX: &str = "tyg_";
/// 32 bytes of entropy; 43 base64url chars after the prefix.
const TOKEN_BYTES: usize = 32;

/// token -> guild name. Populated at spawn, emptied at shutdown.
fn store() -> &'static RwLock<HashMap<String, String>> {
    static STORE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Issue a fresh credential for `guild` and register it.
///
/// Called by `GuildProcess::spawn` right before the child is started. The
/// returned value goes into the child's environment as
/// `TYLLUAN_GUILD_TOKEN`; it is never persisted.
pub fn issue(guild: &str) -> String {
    let mut raw = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw));
    if let Ok(mut map) = store().write() {
        map.insert(token.clone(), guild.to_string());
    }
    token
}

/// Resolve a bearer to the guild that owns it. Constant-time compare against
/// the registered tokens so a wrong token cannot be probed by timing.
/// Empty tokens never authenticate (the operator may have an empty file).
pub fn authenticate(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    let map = store().read().ok()?;
    map.iter()
        .find(|(known, _)| ExecutionGuard::secure_compare(token, known))
        .map(|(_, guild)| guild.clone())
}

/// Revoke one credential. Returns true if it was registered.
pub fn revoke(token: &str) -> bool {
    store()
        .write()
        .map(|mut map| map.remove(token).is_some())
        .unwrap_or(false)
}

/// Revoke every credential issued to `guild` (guild stop/shutdown).
pub fn revoke_guild(guild: &str) -> usize {
    let Ok(mut map) = store().write() else {
        return 0;
    };
    let before = map.len();
    map.retain(|_, owner| owner != guild);
    before - map.len()
}

/// Number of live credentials (diagnostics/tests).
pub fn active_count() -> usize {
    store().read().map(|map| map.len()).unwrap_or(0)
}

/// True when the value *looks* like a guild credential. Cheap and non-secret:
/// used only to give a clearer 401 message, never for authorization.
pub fn is_guild_token(token: &str) -> bool {
    token.starts_with(TOKEN_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test on purpose: the store is process-global and these assertions
    /// must not interleave with each other under the parallel test runner.
    #[test]
    fn issue_authenticate_and_revoke_are_scoped_per_guild() {
        let a = issue("guild-a-test");
        let b = issue("guild-b-test");
        assert!(is_guild_token(&a) && is_guild_token(&b));
        assert_ne!(a, b, "each spawn gets its own credential");
        assert_eq!(authenticate(&a).as_deref(), Some("guild-a-test"));
        assert_eq!(authenticate(&b).as_deref(), Some("guild-b-test"));

        // A token looks like ours but was never issued.
        assert_eq!(authenticate("tyg_not-a-real-token"), None);
        assert_eq!(authenticate(""), None, "empty must never authenticate");
        assert_eq!(authenticate(&a[..TOKEN_PREFIX.len() + 4]), None);

        // Revocation is per guild and idempotent.
        assert!(revoke(&a));
        assert!(!revoke(&a));
        assert_eq!(authenticate(&a), None);
        assert_eq!(authenticate(&b).as_deref(), Some("guild-b-test"));

        let c = issue("guild-a-test");
        assert_eq!(revoke_guild("guild-a-test"), 1);
        assert_eq!(authenticate(&c), None);
        assert_eq!(revoke_guild("guild-a-test"), 0);
    }
}
