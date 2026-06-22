/// Deterministic content distillation for write-path embedding.
///
/// Extracts the semantically densest fragment from `output_preview` to use as the
/// embedding target, instead of embedding the full operational trace wrapper.
/// The stored node content (trace) is NOT changed — only what gets passed to embed().
///
/// Priority rules:
/// 1. Output is knowledge prose (>60% alphabetic) → `intent: output` (rich semantic signal)
/// 2. Output is mixed (35-60% alphabetic) → `intent` + first meaningful words
/// 3. Output is operational (<35% alphabetic or known command prefix) → `intent` only
/// 4. Output empty or <15 chars → `intent` as fallback
pub(crate) fn distill_for_embedding(intent: &str, output_preview: &str) -> String {
    let output = output_preview.trim();

    if output.is_empty() || output.len() < 15 {
        return intent.to_string();
    }

    let first_line = output.lines().next().unwrap_or("");

    let is_operational = {
        let ops: &[&str] = &[
            "Set-Content", "Invoke-RestMethod", "Invoke-WebRequest", "Get-Content",
            "New-Item", "Remove-Item", "Copy-Item", "Move-Item",
            "curl ", "wget ", "git ", "cargo ", "npm ", "yarn ", "ssh ", "scp ",
            "cd ", "ls ", "dir ", "mkdir ", "rm ", "cp ", "mv ", "echo ", "cat ",
            "grep ", "find ", "chmod ", "chown ", "choco ", "winget ",
            "http://", "https://", "file://",
            "SELECT ", "INSERT ", "UPDATE ", "DELETE ", "CREATE ",
            "PSCustomObject", "Write-", "Start-", "Stop-", "Restart-",
        ];
        ops.iter().any(|p| first_line.starts_with(p))
            || first_line.starts_with('{')
            || first_line.starts_with('[')
            || first_line.starts_with("```")
    };

    let total = output.len().max(1);
    let alpha = output.chars().filter(|c| c.is_alphabetic() || c.is_whitespace()).count();
    let alpha_ratio = alpha as f64 / total as f64;

    if is_operational || alpha_ratio < 0.35 {
        intent.to_string()
    } else if alpha_ratio < 0.6 {
        let clean: String = output.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_' || *c == '.' || *c == '/')
            .collect();
        let words: Vec<&str> = clean.split_whitespace()
            .filter(|w| w.len() > 3 || w.chars().all(|c| c.is_uppercase()))
            .take(10)
            .collect();
        if words.is_empty() {
            intent.to_string()
        } else {
            format!("{}: {}", intent, words.join(" "))
        }
    } else {
        let preview: String = output.chars().take(200).collect();
        format!("{}: {}", intent, preview)
    }
}

/// Re-embed all existing episode and lesson nodes using the new `distill_for_embedding`
/// function. This fixes nodes that were created before M22 and have operational-wrapped
/// embeddings instead of semantically dense ones.
///
/// Called once at kernel startup from `TylluanServer::start_background_tasks`.
/// Only processes nodes that already have embeddings (skips unembedded nodes).
pub async fn re_embed_legacy_nodes(
    silva: &crate::memory::silva::SilvaDB,
    matcher: &crate::router::matcher::GuildMatcher,
) -> Result<usize, String> {
    let engine = match matcher.engine() {
        Some(e) => e,
        None => return Err("no embedding engine available".into()),
    };

    let target_model: &str = "distill-v1";
    let types = &["episode", "lesson"];
    let nodes = silva.get_nodes_by_types(types, 2000).await.map_err(|e| e.to_string())?;
    let mut count = 0usize;
    let mut skipped = 0usize;

    for node in &nodes {
        // Skip if already tagged with distill-v1 (idempotent across restarts)
        match silva.get_node_embedding_model(&node.id).await {
            Ok(Some(ref model)) if model == target_model => {
                skipped += 1;
                continue;
            }
            _ => {}
        }

        let (intent, preview) = parse_content_for_embedding(&node.content, &node.node_type);
        if intent.is_empty() {
            continue;
        }
        let target = distill_for_embedding(&intent, &preview);
        let emb = match engine.embed(&target) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if let Err(e) = silva.save_embedding(&node.id, &emb, target_model, None).await {
            tracing::warn!("re-embed: node {} failed: {}", node.id, e);
            continue;
        }
        count += 1;
        if count.is_multiple_of(50) {
            tracing::info!("re-embed: {}/{} nodes done (skipped {})", count, nodes.len(), skipped);
        }
    }
    tracing::info!("re-embed: {} new, {} already distilled, {} total", count, skipped, nodes.len());
    Ok(count)
}

/// Parse node content to extract `intent` and `output_preview` for re-embedding.
pub(super) fn parse_content_for_embedding(content: &str, node_type: &str) -> (String, String) {
    match node_type {
        "episode" => {
            // Format: tylluan_do episode | [agent: X |] intent: Y | guild: Z | tool: W | result: <preview>
            let parts: Vec<&str> = content.split(" | ").collect();
            let mut intent = String::new();
            let mut preview = String::new();
            for part in &parts {
                if let Some(val) = part.strip_prefix("intent: ") {
                    intent = val.to_string();
                } else if let Some(val) = part.strip_prefix("result: ") {
                    preview = val.to_string();
                }
            }
            (intent, preview)
        }
        "lesson" => {
            // Content: guild:X tool:Y intent:Z -- <preview>
            // Or short: guild:X tool:Y intent:Z
            if let Some(pos) = content.find("intent:") {
                let after_intent = content[pos + 7..].trim();
                if let Some(dash_pos) = after_intent.find(" -- ") {
                    let intent = after_intent[..dash_pos].trim().to_string();
                    let preview = after_intent[dash_pos + 4..].trim().to_string();
                    (intent, preview)
                } else {
                    // Old format: just ... intent:Z
                    (after_intent.to_string(), String::new())
                }
            } else {
                (String::new(), String::new())
            }
        }
        _ => (String::new(), String::new()),
    }
}
