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
    let list_triggers = [
        "lista canales", "lista los canales", "lista de canales",
        "list channels", "list canales",
        "ver canales", "ver los canales",
        "mostrar canales", "mostrar los canales", "muestra los canales",
    ];
    if list_triggers.iter().any(|t| lower.starts_with(t) || lower == *t) {
        return (None, None, "list");
    }

    // ── Post patterns: extract channel_id and content after colon ──
    // Pattern: <action phrase> <channel_id>: <content>
    // Action phrases: publica en, post to, send to, escribe en, envia al canal, message, etc.
    // Channel_id is everything between the action phrase and the colon.
    let post_prefixes = [
        "publica en coloquio", "post to coloquio", "post to",
        "escribe en coloquio", "escribe en canal",
        "send to coloquio", "send to", "send message to coloquio", "send message to",
        "envia al canal", "envía al canal", "envia a canal",
        "message coloquio", "message",
        "responde en coloquio", "responde en",
        "publicar en coloquio", "publicar en",
    ];

    for prefix in &post_prefixes {
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
    let create_prefixes = ["crea canal", "create channel", "nuevo canal", "crea un canal"];
    for prefix in &create_prefixes {
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
    let read_prefixes = [
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
    for prefix in &read_prefixes {
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
    let strip_triggers = &[
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
    for trigger in strip_triggers {
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
}
