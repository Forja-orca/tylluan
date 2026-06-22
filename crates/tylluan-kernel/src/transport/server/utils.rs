use serde::Serialize;

/// Serialize a value to pretty JSON, returning a safe error string on failure.
pub fn json_pretty(v: &impl Serialize) -> String {
    serde_json::to_string_pretty(v)
        .unwrap_or_else(|e| format!("{{\"error\":\"serialization failed: {}\"}}", e))
}

/// Extract the most likely filesystem path from a natural-language intent.
pub fn extract_path_from_intent(intent: &str) -> String {
    let looks_like_abs_path = |s: &str| -> bool {
        // URLs are never filesystem paths
        if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://") {
            return false;
        }
        // Only match real path prefixes, not any string containing a slash
        s.starts_with('/') || s.starts_with("./") || s.starts_with("../")
            || s.starts_with("~/")
            || (s.len() >= 2 && s.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) && s[1..].starts_with(':'))
    };

    // filename.ext pattern — word that looks like a bare filename
    let looks_like_filename = |s: &str| -> bool {
        if s.len() < 3 || !s.contains('.') { return false; }
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 2 || parts.last().map(|e| e.is_empty()).unwrap_or(true) { return false; }
        // Require known file extensions (not generic dot-separated words)
        let known_extensions = ["rs", "py", "ts", "tsx", "js", "json", "toml", "yaml", "yml",
            "md", "txt", "csv", "xml", "html", "css", "sql", "db", "env", "lock", "bat", "ps1",
            "sh", "exe", "dll", "so", "wasm", "ico", "png", "jpg", "svg", "woff2", "ttf"];
        let ext = parts[parts.len() - 1];
        if !known_extensions.contains(&ext) { return false; }
        // All parts must be valid identifier chars
        parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
    };

    // Scan each word — prefer absolute paths, fall back to bare filenames
    let mut filename_hint: Option<String> = None;
    for word in intent.split_whitespace() {
        let clean = word.trim_matches(|c: char| "\"',()[]{}".contains(c));
        if clean.is_empty() { continue; }
        if looks_like_abs_path(clean) {
            return clean.to_string();
        }
        if filename_hint.is_none() && looks_like_filename(clean) {
            filename_hint = Some(clean.to_string());
        }
    }
    if let Some(hint) = filename_hint {
        return hint;
    }

    // "in X" / "at X" / "from X" / "to X" preposition hints
    let prepositions = ["in ", "at ", "from ", "to ", "into ", "inside "];
    let lower = intent.to_lowercase();
    for prep in &prepositions {
        if let Some(pos) = lower.find(prep) {
            let after = &intent[pos + prep.len()..];
            let candidate = after.split_whitespace().next().unwrap_or("").trim_matches(|c: char| "\"',".contains(c));
            if !candidate.is_empty() && (looks_like_abs_path(candidate) || looks_like_filename(candidate)) {
                return candidate.to_string();
            }
        }
    }

    ".".to_string()
}

/// Extract the first URL (http/https/stdio://) from a natural-language intent.
/// Used to populate `server_url` for MCP bridge tools.
pub fn extract_url_from_intent(intent: &str) -> Option<String> {
    for word in intent.split_whitespace() {
        let clean = word.trim_matches(|c: char| "\"',()[]{}".contains(c));
        if clean.starts_with("http://") || clean.starts_with("https://") || clean.starts_with("stdio://") {
            return Some(clean.to_string());
        }
    }
    None
}

/// Extract a clean command from a natural-language intent by stripping NL wrappers.
/// "run ls -la" → "ls -la"
/// "execute bash command: echo hello" → "echo hello"
/// "run git status" → "git status"
/// "bash: pip install torch" → "pip install torch"
/// Falls back to the original intent if no wrapper is detected.
pub fn extract_command_from_intent(intent: &str) -> &str {
    let trimmed = intent.trim();
    let lower = trimmed.to_lowercase();

    // Ordered list of NL wrappers — longer patterns first to avoid partial matches.
    // Only strips the NL verb, never part of the actual command.
    let wrappers = [
        "execute bash command: ",
        "execute bash: ",
        "execute bash ",
        "run bash command: ",
        "run bash ",
        "bash command: ",
        "bash: ",
        "execute command: ",
        "execute command ",
        "execute: ",
        "execute ",
        "run command: ",
        "run command ",
        "run ",
    ];

    for prefix in &wrappers {
        if lower.starts_with(prefix) {
            let remainder = trimmed[prefix.len()..].trim();
            if !remainder.is_empty() {
                return remainder;
            }
        }
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_command_run() {
        assert_eq!(extract_command_from_intent("run ls -la"), "ls -la");
    }

    #[test]
    fn test_extract_command_execute_bash() {
        assert_eq!(
            extract_command_from_intent("execute bash command: echo hello"),
            "echo hello"
        );
    }

    #[test]
    fn test_extract_command_bash_colon() {
        assert_eq!(
            extract_command_from_intent("bash: pip install torch"),
            "pip install torch"
        );
    }

    #[test]
    fn test_extract_command_run_git() {
        assert_eq!(
            extract_command_from_intent("run git status"),
            "git status"
        );
    }

    #[test]
    fn test_extract_command_run_bash() {
        assert_eq!(
            extract_command_from_intent("run bash pip install torch"),
            "pip install torch"
        );
    }

    #[test]
    fn test_extract_command_no_wrapper() {
        assert_eq!(extract_command_from_intent("ls -la"), "ls -la");
    }

    #[test]
    fn test_extract_command_plain_git() {
        assert_eq!(extract_command_from_intent("git status"), "git status");
    }
}
