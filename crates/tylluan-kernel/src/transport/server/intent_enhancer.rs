
const ACTION_VERBS: &[&str] = &[
    "list", "show", "get", "find", "search", "read", "write", "create", "edit",
    "delete", "run", "execute", "build", "test", "check", "install", "update",
    "move", "copy", "rename", "open", "close", "start", "stop", "restart",
    "listar", "mostrar", "obtener", "buscar", "leer", "crear", "editar",
    "borrar", "ejecutar", "construir", "probar", "verificar", "instalar",
];

/// Returns true if the intent is considered ambiguous and needs enrichment.
pub fn is_ambiguous(intent: &str) -> bool {
    let words: Vec<&str> = intent.split_whitespace().collect();
    if words.len() < 5 { return true; }
    let lower = intent.to_lowercase();
    !ACTION_VERBS.iter().any(|v| lower.contains(v))
}

/// Enrich an ambiguous intent with session context.
/// `recent_intents`: last 3 intents from the same session (most recent first).
/// Returns the enriched intent string.
pub fn enrich_intent(original: &str, recent_intents: &[String]) -> String {
    if recent_intents.is_empty() {
        return original.to_string();
    }
    // Extract the most recent non-trivial context word
    let context: Vec<&str> = recent_intents
        .iter()
        .flat_map(|i| i.split_whitespace())
        .filter(|w| w.len() > 4)  // skip short words
        .take(5)
        .collect();

    if context.is_empty() {
        return original.to_string();
    }

    // Prepend context hint to the original intent
    format!("[ctx: {}] {}", context.join(" "), original)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ambiguous_short() {
        assert!(is_ambiguous("edita el archivo"));
        assert!(!is_ambiguous("list all files in the current directory please"));
    }

    #[test]
    fn test_is_ambiguous_no_verb() {
        assert!(is_ambiguous("the python script in my project folder here"));
        assert!(!is_ambiguous("run the python script in my project folder"));
    }

    #[test]
    fn test_enrich_intent_adds_context() {
        let recent = vec!["edit the python file".to_string()];
        let enriched = enrich_intent("fix it", &recent);
        assert!(enriched.contains("python") || enriched.contains("ctx"));
        assert!(enriched.contains("fix it"));
    }

    #[test]
    fn test_enrich_intent_empty_context() {
        let enriched = enrich_intent("fix it", &[]);
        assert_eq!(enriched, "fix it");
    }
}
