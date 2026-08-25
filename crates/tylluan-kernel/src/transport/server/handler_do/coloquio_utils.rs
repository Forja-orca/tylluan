// ── Deterministic prefix lists for Coloquio intents ────────────────────────
// Shared by `parse_coloquio_intent` (tool resolution once routed to the
// coloquio guild) and `is_coloquio_dispatch_intent` (pre-routing gate that
// forces these intents to the coloquio guild BEFORE semantic routing, so a
// long "publica en coloquio X: ..." can never be swallowed by the Proactive
// Cascade / matcher — observed live 2026-08-25: scored >=0.6 on complexity,
// was sent to coordinator (required arg `task`) and the post failed).

/// List channels — all unambiguous channel operations.
const LIST_TRIGGERS: &[&str] = &[
    "lista canales", "lista los canales", "lista de canales",
    "list channels", "list canales",
    "ver canales", "ver los canales",
    "mostrar canales", "mostrar los canales", "muestra los canales",
];

/// Post patterns: <action phrase> <channel_id>: <content>.
/// Channel_id is everything between the action phrase and the colon.
const POST_PREFIXES: &[&str] = &[
    "publica en coloquio", "post to coloquio", "post to",
    "escribe en coloquio", "escribe en canal",
    "send to coloquio", "send to", "send message to coloquio", "send message to",
    "envia al canal", "envía al canal", "envia a canal",
    "message coloquio", "message",
    "responde en coloquio", "responde en",
    "publicar en coloquio", "publicar en",
];

/// Create channel patterns.
const CREATE_PREFIXES: &[&str] = &[
    "crea canal", "create channel", "nuevo canal", "crea un canal",
];

/// Read patterns — specific (coloquio in prefix, must come before generic ones)
/// then generic (no "coloquio" required — channel name follows directly).
const READ_PREFIXES: &[&str] = &[
    // Specific (coloquio in prefix — must come before generic ones)
    "lee el coloquio", "lee el canal coloquio", "lee canal coloquio",
    "leer coloquio", "leer canal coloquio",
    "read coloquio channel", "read channel coloquio", "read coloquio",
    "ver canal coloquio", "ver coloquio",
    "mostrar coloquio", "mostrar canal coloquio",
    "muestra el coloquio", "historial coloquio",
    "canal coloquio",
    // Generic (no "coloquio" required — channel name follows directly)
    "leer canal ", "lee canal ", "leer el canal ", "lee el canal ",
    "read channel ", "ver canal ", "ver el canal ", "mostrar canal ",
];

/// Generic _CHANNEL_STRIP fallback (mirrors coloquio.py).
const STRIP_TRIGGERS: &[&str] = &[
    "lee el canal coloquio ", "lee el coloquio ",
    "leer canal coloquio ", "leer coloquio ",
    "ver canal coloquio ", "ver coloquio ",
    "read coloquio channel ", "read channel coloquio ", "read coloquio ",
    "mostrar canal coloquio ", "mostrar coloquio ",
    "muestra el coloquio ", "historial coloquio ",
    "canal coloquio ",
    // Generic (no "coloquio" in prefix)
    "leer canal ", "lee canal ", "leer el canal ", "lee el canal ",
    "read channel ", "ver canal ", "ver el canal ", "mostrar canal ",
];

/// True only for prefixes that unambiguously reference a Coloquio channel.
/// Bare forms like "post to", "send to", "message" or "responde en" are too
/// generic to force routing — those stay with semantic routing (status quo).
fn mentions_channel_word(prefix: &str) -> bool {
    prefix.contains("coloquio")
        || prefix.contains("canal")
        || prefix.contains("canale")
        || prefix.contains("channel")
        || prefix.contains("channels")
}

/// Deterministic Coloquio dispatch detector — used to force guild="coloquio"
/// BEFORE semantic routing. Mirrors `parse_coloquio_intent`'s prefix lists but
/// only for patterns that unambiguously mean a Coloquio channel operation:
///
/// - list/create triggers are channel operations by definition;
/// - post/read prefixes must mention coloquio/canal/channel explicitly.
///
/// The word-based fallback inside `parse_coloquio_intent` (any "<word> <chan>:"
/// heuristics) is deliberately NOT included: it can false-positive on unrelated
/// sentences containing "canal"/"coloquio" plus a colon, so it must never force
/// routing away from the semantic router.
pub(super) fn is_coloquio_dispatch_intent(intent: &str) -> bool {
    let lower = intent.trim().to_lowercase();

    for t in LIST_TRIGGERS {
        if lower.starts_with(t) || lower == *t {
            return true;
        }
    }
    for p in POST_PREFIXES {
        if mentions_channel_word(p) && lower.starts_with(p) {
            return true;
        }
    }
    for p in CREATE_PREFIXES {
        if lower.starts_with(p) {
            return true;
        }
    }
    for p in READ_PREFIXES {
        if mentions_channel_word(p) && lower.starts_with(p) {
            return true;
        }
    }
    for t in STRIP_TRIGGERS {
        if mentions_channel_word(t) && lower.starts_with(t) {
            return true;
        }
    }
    false
}

/// Extract channel_id and optionally message content from a coloquio intent.
/// Returns (channel_id, content_or_name, tool_hint).
/// tool_hint is "read", "post", "list", or "create".
pub(super) fn parse_coloquio_intent(intent: &str) -> (Option<String>, Option<String>, &'static str) {
    let trimmed = intent.trim();
    let lower = trimmed.to_lowercase();

    // ── List channels ──
    // Word-boundary prefix match, not substring: `contains("lista")` used to match
    // "listando" / "listado" / "artista" anywhere in a long post body (e.g. a status
    // report that mentions "filesystem listando raiz"), silently rerouting a real
    // post_to_channel into list_channels and swallowing the message.
    if LIST_TRIGGERS.iter().any(|t| lower.starts_with(t) || lower == *t) {
        return (None, None, "list");
    }

    // ── Post patterns: extract channel_id and content after colon ──
    // Pattern: <action phrase> <channel_id>: <content>
    for prefix in POST_PREFIXES {
        if lower.starts_with(prefix) {
            let after = trimmed[prefix.len()..].trim();
            if let Some(col_idx) = after.find(':') {
                let channel_id = after[..col_idx].trim().to_string();
                let content = after[col_idx + 1..].trim().to_string();
                if !channel_id.is_empty() && !content.is_empty() {
                    return (Some(channel_id), Some(content), "post");
                }
            } else if !after.is_empty() {
                // No colon: treat the entire remainder as channel_id, no content
                // (content will be extracted from the generic tool_args fallback)
                return (Some(after.to_string()), None, "post");
            }
        }
    }

    // ── Create channel patterns ──
    for prefix in CREATE_PREFIXES {
        if lower.starts_with(prefix) {
            let after = trimmed[prefix.len()..].trim();
            if let Some(col_idx) = after.find(':') {
                let channel_id = after[..col_idx].trim().to_string();
                let name = after[col_idx + 1..].trim().to_string();
                if !channel_id.is_empty() {
                    return (Some(channel_id), Some(name), "create");
                }
            } else if !after.is_empty() {
                return (Some(after.to_string()), None, "create");
            }
        }
    }

    // ── Read patterns: extract channel_id ──
    for prefix in READ_PREFIXES {
        if lower.starts_with(prefix) {
            let raw = trimmed[prefix.len()..].trim();
            if !raw.is_empty() {
                return (Some(_clean_coloquio_channel_id(raw)), None, "read");
            }
        }
    }

    // ── Fallback: if the word "coloquio" or "canal" is present with a colon,
    //     try to extract channel_id from <word> <channel>: content pattern
    if (lower.contains("coloquio") || lower.contains("canal"))
        && let Some(col_idx) = trimmed.find(':') {
            let before = trimmed[..col_idx].trim();
            let after = trimmed[col_idx + 1..].trim();
            // Take the last word before the colon as channel_id
            let words: Vec<&str> = before.split_whitespace().collect();
            if let Some(last) = words.last()
                && last.len() >= 2 && !last.contains("coloquio") {
                    let content = after.to_string();
                    let channel_id = last.to_string();
                    return (Some(channel_id), Some(content), "post");
                }
        }

    // ── Generic _CHANNEL_STRIP fallback (mirrors coloquio.py) ──
    for trigger in STRIP_TRIGGERS {
        if lower.starts_with(trigger) {
            let remainder = trimmed[trigger.len()..].trim();
            if !remainder.is_empty() {
                return (Some(_clean_coloquio_channel_id(remainder)), None, "read");
            }
        }
    }

    (None, None, "")
}

/// Strip pagination keywords and natural-language suffixes from a channel_id.
/// e.g., "mision-activa ultimos 5 mensajes" -> "mision-activa"
pub(super) fn _clean_coloquio_channel_id(raw: &str) -> String {
    let stop_signals = [
        " ultimos ", " últimos ", " ultim ", " últim ",
        " limit ", " limite ", " límite ",
        " offset ", " desde turno ",
        " mensajes", " messages", " mensaje", " message",
        " channel_id", " channelid",
    ];
    let lower = raw.to_lowercase();
    let mut cut = raw.len();
    for sig in &stop_signals {
        if let Some(p) = lower.find(sig)
            && p < cut { cut = p; }
    }
    if cut < raw.len() {
        raw[..cut].trim().to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Extract (limit, offset) from a coloquio intent string.
/// Returns (0, 0) if not found — the Python guild uses its own defaults.
pub(super) fn _parse_coloquio_pagination(intent: &str) -> (i64, i64) {
    let lower = intent.to_lowercase();
    let limit = _parse_pagination_value(&lower, &["limit ", "ultimos ", "últimos "]).min(500);
    let offset = _parse_pagination_value(&lower, &["offset ", "desde turno "]).min(5000);
    (limit, offset)
}

fn _parse_pagination_value(lower: &str, keywords: &[&str]) -> i64 {
    for kw in keywords {
        if let Some(pos) = lower.find(kw) {
            let after = lower[pos + kw.len()..].trim_start();
            let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num.parse::<i64>() {
                return n;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_intent_containing_listando_is_not_misrouted_to_list() {
        // Regression: "lower.contains(\"lista\")" used to match the substring inside
        // "listando" anywhere in a long post body, silently discarding the post and
        // rerouting to list_channels instead. Found 2026-07-26 living the real flow --
        // this exact sentence misrouted a real Coloquio post.
        let intent = "publica en coloquio equipo: filesystem listando raiz en vez de subdirectorio";
        let (channel, content, tool) = parse_coloquio_intent(intent);
        assert_eq!(tool, "post", "must route to post, not list, despite containing 'lista' as a substring of 'listando'");
        assert_eq!(channel.as_deref(), Some("equipo"));
        assert!(content.unwrap().contains("filesystem listando raiz"));
    }

    #[test]
    fn explicit_list_channels_intent_still_routes_to_list() {
        for intent in ["lista canales", "list channels", "ver canales", "mostrar canales", "Lista los canales"] {
            let (_, _, tool) = parse_coloquio_intent(intent);
            assert_eq!(tool, "list", "'{intent}' should still route to list");
        }
    }

    #[test]
    fn post_intent_with_artista_substring_is_not_misrouted_to_list() {
        let intent = "publica en coloquio equipo: el artista dashboard quedo listo";
        let (_, _, tool) = parse_coloquio_intent(intent);
        assert_eq!(tool, "post", "'artista' also contains 'lista' as a substring -- must not misroute");
    }

    #[test]
    fn read_intent_with_literal_channel_id_param_does_not_pollute_channel_name() {
        // Regression: an agent typing "lee canal equipo channel_id=equipo" (trying to be
        // explicit about the parameter) got a literal channel named
        // "equipo channel_id=equipo" created/read instead of "equipo", because
        // _clean_coloquio_channel_id's stop_signals didn't include "channel_id" --
        // reproduced live 2026-07-30 reading #equipo from Coloquio.
        let intent = "lee canal equipo channel_id=equipo";
        let (channel, _, tool) = parse_coloquio_intent(intent);
        assert_eq!(tool, "read");
        assert_eq!(channel.as_deref(), Some("equipo"));
    }

    // ── is_coloquio_dispatch_intent (pre-routing gate) ──────────────────────

    #[test]
    fn dispatch_force_recognizes_coloquio_post_intents() {
        // The exact failure class fixed 2026-08-25: a long post that scored >=0.6
        // on the Proactive Cascade and was routed to coordinator (required `task`).
        for intent in [
            "publica en coloquio mision-activa: propuesta larga de debate con plan y ejecucion",
            "publica en coloquio equipo: resumen del cierre",
            "post to coloquio general: hello world",
            "escribe en coloquio coloquio: nota rapida",
            "send message to coloquio equipo: aviso",
            "publicar en coloquio general: resumen",
            "envia al canal mision-activa: mensaje",
            "message coloquio equipo: hola",
        ] {
            assert!(is_coloquio_dispatch_intent(intent), "'{intent}' must force coloquio routing");
        }
    }

    #[test]
    fn dispatch_force_recognizes_read_list_create() {
        for intent in [
            "lee el coloquio mision-activa",
            "leer coloquio equipo",
            "lee canal mision-activa",
            "ver canal coloquio equipo",
            "historial coloquio mision-activa",
            "lista canales",
            "ver canales",
            "crea canal test-canal",
            "create channel alpha",
            "nuevo canal beta",
        ] {
            assert!(is_coloquio_dispatch_intent(intent), "'{intent}' must force coloquio routing");
        }
    }

    #[test]
    fn dispatch_force_ignores_bare_or_unrelated_intents() {
        // Bare posting phrases without a channel word stay with semantic routing,
        // and unrelated sentences containing "canal" + ':' (the heuristic fallback)
        // must NOT be forced to coloquio.
        for intent in [
            "post to the coordinator: execute the migration",
            "message the coordinator about the failed build",
            "send to production: deploy",
            "responde en la reunion: preparado",
            "cuando el canal de error muestre: algo raro",
            "investiga el fallo del canal de pago y resume: el problema",
            "publica el informe en la wiki",
        ] {
            assert!(!is_coloquio_dispatch_intent(intent), "'{intent}' must NOT force coloquio routing");
        }
    }
}
