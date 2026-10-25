//! ADR-019 step 2 — per-peer and per-space permissions (fail-closed).
//!
//! Scope of enforcement: peer ingestion (receive / pull / sync_both /
//! pull_from_peer_internal), export and push filtering, and the P2P dispatch
//! allow-list. `coloquio_channels` is stored but INERT until step 6.
//!
//! Everything here is fail-closed: absent, empty or invalid JSON means
//! deny-all, and an error never grants.

use serde::{Deserialize, Serialize};

use super::FederationPeer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    None,
    Pull,
    Push,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpacePermission {
    pub space: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerPermissions {
    #[serde(default)]
    pub sync_spaces: Vec<SpacePermission>,
    #[serde(default)]
    pub dispatch_allow: Vec<String>,
    /// Stored since step 2; INERT until step 6 (no enforcement yet).
    #[serde(default)]
    pub coloquio_channels: Vec<String>,
}

impl PeerPermissions {
    /// May the local node SEND `space` to the peer? (direction push|both)
    pub fn may_send_space(&self, space: &str) -> bool {
        self.sync_spaces.iter().any(|s| {
            s.space == space && matches!(s.direction, Direction::Push | Direction::Both)
        })
    }

    /// May the local node RECEIVE `space` pushed by the peer? (pull|both)
    pub fn may_receive_space(&self, space: &str) -> bool {
        self.sync_spaces.iter().any(|s| {
            s.space == space && matches!(s.direction, Direction::Pull | Direction::Both)
        })
    }

    /// Exact `guild.tool` match against the allow-list. Empty list denies all.
    pub fn may_dispatch(&self, guild: &str, tool: &str) -> bool {
        let key = format!("{guild}.{tool}");
        self.dispatch_allow.iter().any(|e| e == &key)
    }
}

/// Fail-closed parse of the stored TEXT column. Absent/empty/`{}`/invalid =>
/// deny-all (+ warn on invalid).
pub fn parse_permissions(raw: &str) -> PeerPermissions {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return PeerPermissions::default();
    }
    match serde_json::from_str::<PeerPermissions>(trimmed) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "federation permissions: unparseable JSON ({e}) — denying everything for this peer"
            );
            PeerPermissions::default()
        }
    }
}

pub fn permissions_to_json(p: &PeerPermissions) -> String {
    serde_json::to_string(p).unwrap_or_else(|_| "{}".to_string())
}

/// Resolve a node's space deterministically: `topic_key` -> `metadata["space"]`
/// -> `"default"`. No migration, no new column.
pub fn resolve_space(topic_key: Option<&str>, metadata: &str) -> String {
    if let Some(tk) = topic_key {
        let tk = tk.trim();
        if !tk.is_empty() {
            return tk.to_string();
        }
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(metadata)
        && let Some(s) = v.get("space").and_then(|s| s.as_str())
    {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    "default".to_string()
}

/// Families whose dispatch tools execute code; sandbox is off by default, so
/// listing them yields a STRONG warning plus an audit entry (TL decision), not
/// an error.
pub const DANGEROUS_DISPATCH_FAMILIES: [&str; 3] = ["bash", "filesystem", "code"];

/// Validation for the PUT endpoint. `Err(message)` => 400.
pub fn validate_permissions(p: &PeerPermissions) -> Result<(), String> {
    for s in &p.sync_spaces {
        let name = s.space.trim();
        if name.is_empty() {
            return Err("sync_spaces: space name must not be empty".to_string());
        }
        if name == "default" {
            return Err(
                "sync_spaces: 'default' cannot be shared — every node without topic_key or \
                 metadata.space resolves to it. Tag the nodes you want to share with \
                 metadata.space (e.g. tylluan_remember metadata={\"space\": \"proyecto-x\"})."
                    .to_string(),
            );
        }
        if name.contains('*') {
            return Err(format!("sync_spaces: wildcards are not allowed ('{name}')"));
        }
    }
    for raw in &p.dispatch_allow {
        let e = raw.trim();
        if e.is_empty() {
            return Err("dispatch_allow: entries must not be empty".to_string());
        }
        if e.contains('*') {
            return Err(format!("dispatch_allow: wildcards are not allowed ('{e}')"));
        }
        let parts: Vec<&str> = e.split('.').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(format!(
                "dispatch_allow: entry '{e}' must have the exact form 'guild.tool'"
            ));
        }
    }
    Ok(())
}

/// Entries of `dispatch_allow` that belong to a dangerous family (code-executing).
pub fn dangerous_dispatch_entries(p: &PeerPermissions) -> Vec<String> {
    p.dispatch_allow
        .iter()
        .filter(|e| {
            let e = e.trim();
            DANGEROUS_DISPATCH_FAMILIES
                .iter()
                .any(|f| e.starts_with(&format!("{f}.")))
        })
        .cloned()
        .collect()
}

fn decode_hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

/// Map an approved peer by the X25519 static key revealed by the Noise
/// handshake (hex string `pipe.peer_id`). Mirrors the kernel's approved-key
/// derivation (`x25519_from_ed25519_hex` over the stored Ed25519 pubkey).
pub fn peer_by_x25519<'a>(
    peers: &'a [FederationPeer],
    remote_x25519_hex: &str,
) -> Option<&'a FederationPeer> {
    let remote = decode_hex32(remote_x25519_hex)?;
    peers.iter().find(|p| {
        p.approved
            && !p.ed25519_pubkey.is_empty()
            && match tylluan_link::noise::x25519_from_ed25519_hex(&p.ed25519_pubkey) {
                Some(key) => key == remote,
                None => {
                    // T-DSH-79: la denegación sigue siendo fail-closed, pero ya no silenciosa.
                    tracing::warn!(
                        peer = %p.name,
                        "P2P: pubkey Ed25519 no decodificable; peer aprobado no resuelto"
                    );
                    false
                }
            }
    })
}

/// Decision of the P2P dispatch gate for one inbound request: the audit label
/// of the caller and whether `guild.tool` may run. Fail-closed: a caller that
/// was approved at handshake but cannot be resolved now (e.g. revoked
/// mid-flight) is denied. Pure so the real rule is testable without a socket.
pub fn dispatch_gate_decision(
    peers: &[FederationPeer],
    caller_x25519_hex: &str,
    guild: &str,
    tool: &str,
) -> (String, bool) {
    match peer_by_x25519(peers, caller_x25519_hex) {
        Some(p) => (p.name.clone(), p.permissions.may_dispatch(guild, tool)),
        None => (format!("x25519:{caller_x25519_hex}"), false),
    }
}

/// Denial audit: one hash-chained row in `guild_audit_log` (same store as
/// `tylluan_do` dispatches) plus a warn. Errors from the audit write are
/// non-fatal (same posture as `log_audit_entry` itself).
pub fn audit_permission_denied(peer: &str, flow: &str, guild: &str, tool: &str, reason: &str) {
    tracing::warn!("federation permissions: {flow} denied for peer '{peer}': {reason}");
    let intent = format!("peer {peer}: {flow} denied — {reason}");
    // D-51: un fallo de la auditoría no debe ser invisible. Sigue siendo
    // non-fatal (la denegación ya se aplicó), pero queda en el log.
    if let Err(e) = crate::transport::server::handler_do::log_audit_entry(
        &intent, guild, tool, peer, false, reason, 0, false,
    ) {
        tracing::warn!(
            "federation permissions: failed to audit denial for peer '{peer}': {e}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(json: &str) -> PeerPermissions {
        parse_permissions(json)
    }

    #[test]
    fn parse_fail_closed_on_invalid_and_empty() {
        assert_eq!(perms(""), PeerPermissions::default());
        assert_eq!(perms("{}"), PeerPermissions::default());
        assert_eq!(perms("not json"), PeerPermissions::default());
        assert_eq!(perms("{\"sync_spaces\": 42}"), PeerPermissions::default());
        // valid JSON parses
        let p = perms(r#"{"sync_spaces":[{"space":"x","direction":"both"}]}"#);
        assert_eq!(p.sync_spaces.len(), 1);
    }

    #[test]
    fn direction_semantics_send_and_receive() {
        let p = perms(
            r#"{"sync_spaces":[
                {"space":"out","direction":"push"},
                {"space":"in","direction":"pull"},
                {"space":"bi","direction":"both"},
                {"space":"off","direction":"none"}
            ]}"#,
        );
        assert!(p.may_send_space("out") && !p.may_receive_space("out"));
        assert!(!p.may_send_space("in") && p.may_receive_space("in"));
        assert!(p.may_send_space("bi") && p.may_receive_space("bi"));
        assert!(!p.may_send_space("off") && !p.may_receive_space("off"));
        assert!(!p.may_send_space("unknown"));
    }

    #[test]
    fn dispatch_allow_exact_match_only() {
        let p = perms(r#"{"dispatch_allow":["git.status"]}"#);
        assert!(p.may_dispatch("git", "status"));
        assert!(!p.may_dispatch("git", "commit"));
        assert!(!p.may_dispatch("bash", "execute"));
        let empty = PeerPermissions::default();
        assert!(!empty.may_dispatch("git", "status"), "empty allow-list denies all");
    }

    #[test]
    fn resolve_space_order_topic_key_then_metadata_then_default() {
        assert_eq!(resolve_space(Some("tk"), r#"{"space":"meta"}"#), "tk");
        assert_eq!(resolve_space(Some("  "), r#"{"space":"meta"}"#), "meta");
        assert_eq!(resolve_space(None, r#"{"space":"meta"}"#), "meta");
        assert_eq!(resolve_space(None, r#"{"space":"   "}"#), "default");
        assert_eq!(resolve_space(None, "{}"), "default");
        assert_eq!(resolve_space(None, "not json"), "default");
    }

    #[test]
    fn validate_rejects_default_wildcards_and_bad_format() {
        let d = perms(r#"{"sync_spaces":[{"space":"default","direction":"both"}]}"#);
        let err = validate_permissions(&d).unwrap_err();
        assert!(err.contains("'default' cannot be shared"), "clear error: {err}");

        let w = perms(r#"{"sync_spaces":[{"space":"proj*","direction":"push"}]}"#);
        assert!(validate_permissions(&w).is_err());

        let bad = perms(r#"{"dispatch_allow":["bash"]}"#);
        assert!(validate_permissions(&bad).is_err());
        let bad2 = perms(r#"{"dispatch_allow":["*"] }"#);
        assert!(validate_permissions(&bad2).is_err());
        let star = perms(r#"{"sync_spaces":[{"space":"ok","direction":"both"}],"dispatch_allow":["bash.*"]}"#);
        assert!(validate_permissions(&star).is_err());

        let good = perms(
            r#"{"sync_spaces":[{"space":"proyecto-x","direction":"both"}],"dispatch_allow":["git.status","monitor.top"]}"#,
        );
        assert!(validate_permissions(&good).is_ok());
    }

    #[test]
    fn dangerous_families_detected() {
        let p = perms(r#"{"dispatch_allow":["bash.execute","filesystem.read_file","code.run","git.status"]}"#);
        let d = dangerous_dispatch_entries(&p);
        assert_eq!(d.len(), 3);
        assert!(d.contains(&"bash.execute".to_string()));
        assert!(d.contains(&"code.run".to_string()));
        let clean = perms(r#"{"dispatch_allow":["git.status"]}"#);
        assert!(dangerous_dispatch_entries(&clean).is_empty());
    }

    #[test]
    fn peer_by_x25519_maps_only_approved_with_matching_key() {
        use tylluan_link::identity::NodeIdentity;
        let dir = std::env::temp_dir().join("tylluan_perm_test_keys");
        let _ = std::fs::create_dir_all(&dir);
        let id = NodeIdentity::load_or_create(&dir.join("a.identity")).expect("identity");
        let pub_hex = id.public_key_hex().to_string();
        let x = tylluan_link::noise::x25519_from_ed25519_hex(&pub_hex).expect("x25519");
        let x_hex: String = x.iter().map(|b| format!("{b:02x}")).collect();

        let mk = |approved: bool| FederationPeer {
            name: "p".into(),
            url: "http://x".into(),
            auth_token: "t".into(),
            shared_secret: String::new(),
            last_sync: None,
            approved,
            added_at: 0,
            ed25519_pubkey: pub_hex.clone(),
            external_address: String::new(),
            permissions: PeerPermissions::default(),
        };

        let peers = vec![mk(true)];
        assert!(peer_by_x25519(&peers, &x_hex).is_some(), "approved matching key maps");
        let peers = vec![mk(false)];
        assert!(peer_by_x25519(&peers, &x_hex).is_none(), "unapproved never maps");
        let peers = vec![mk(true)];
        assert!(peer_by_x25519(&peers, "00").is_none(), "malformed key rejected");
        let other: String = std::iter::repeat_n("aa", 32).collect();
        assert!(peer_by_x25519(&peers, &other).is_none(), "unknown key rejected");
    }

    #[test]
    fn dispatch_gate_decision_is_deny_by_default_and_exact() {
        use tylluan_link::identity::NodeIdentity;
        let dir = std::env::temp_dir().join(format!("tylluan_gate_keys_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let id = NodeIdentity::load_or_create(&dir.join("g.identity")).expect("identity");
        let pub_hex = id.public_key_hex().to_string();
        let x = tylluan_link::noise::x25519_from_ed25519_hex(&pub_hex).expect("x25519");
        let x_hex: String = x.iter().map(|b| format!("{b:02x}")).collect();
        let mk = |approved: bool, json: &str| FederationPeer {
            name: "hospital".into(),
            url: "http://x".into(),
            auth_token: "t".into(),
            shared_secret: String::new(),
            last_sync: None,
            approved,
            added_at: 0,
            ed25519_pubkey: pub_hex.clone(),
            external_address: String::new(),
            permissions: perms(json),
        };

        // Approved peer with no permissions: everything denied, labelled by name.
        let (label, ok) = dispatch_gate_decision(&[mk(true, "")], &x_hex, "git", "status");
        assert_eq!(label, "hospital");
        assert!(!ok, "deny-by-default");

        // Exact guild.tool only.
        let peers = [mk(true, r#"{"dispatch_allow":["git.status"]}"#)];
        assert!(dispatch_gate_decision(&peers, &x_hex, "git", "status").1);
        assert!(!dispatch_gate_decision(&peers, &x_hex, "git", "log").1, "other tool of same guild");
        assert!(!dispatch_gate_decision(&peers, &x_hex, "docker", "status").1, "same tool other guild");

        // Dangerous families are NOT blocked at dispatch time (documented decision:
        // warning + audit on save); allow-listed means allowed.
        let peers = [mk(true, r#"{"dispatch_allow":["bash.execute"]}"#)];
        assert!(dispatch_gate_decision(&peers, &x_hex, "bash", "execute").1);
        assert!(!dispatch_gate_decision(&peers, &x_hex, "bash", "other").1);

        // Not approved / unknown caller / malformed key: denied, labelled by key.
        let peers = [mk(false, r#"{"dispatch_allow":["git.status"]}"#)];
        let (label, ok) = dispatch_gate_decision(&peers, &x_hex, "git", "status");
        assert!(!ok && label.starts_with("x25519:"), "unapproved peer never dispatches");
        let approved = [mk(true, r#"{"dispatch_allow":["git.status"]}"#)];
        let stranger: String = std::iter::repeat_n("aa", 32).collect();
        assert!(!dispatch_gate_decision(&approved, &stranger, "git", "status").1);
        assert!(!dispatch_gate_decision(&approved, "00", "git", "status").1);
    }

    #[test]
    fn serde_roundtrip_is_stable() {
        let p = perms(r#"{"sync_spaces":[{"space":"x","direction":"both"}],"dispatch_allow":["git.status"],"coloquio_channels":["general"]}"#);
        let json = permissions_to_json(&p);
        let back: PeerPermissions = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    /// T-DSH-79: una pubkey Ed25519 indescifrable no resuelve al peer
    /// (fail-closed) y ahora deja warn.
    #[test]
    fn peer_by_x25519_warns_on_undecodable_pubkey() {
        use crate::federation::FederationPeer;
        let peer = FederationPeer {
            name: "bad-key".into(),
            url: "http://bad".into(),
            auth_token: "t".into(),
            shared_secret: String::new(),
            last_sync: None,
            approved: true,
            added_at: 0,
            ed25519_pubkey: "no-es-hex".into(),
            external_address: String::new(),
            permissions: PeerPermissions::default(),
        };
        let remote: String = std::iter::repeat_n("aa", 32).collect();
        let captured = crate::test_support::capture_warns(|| {
            assert!(
                peer_by_x25519(std::slice::from_ref(&peer), &remote).is_none(),
                "una pubkey inválida nunca resuelve al peer"
            );
        });
        assert!(
            captured.contains("no decodificable"),
            "la pubkey ilegible debe avisar, got: {captured}"
        );
    }
}
