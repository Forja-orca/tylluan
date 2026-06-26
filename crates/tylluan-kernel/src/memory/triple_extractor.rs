//! Deterministic extraction of semantic triples from text without LLM.

use std::collections::HashSet;

/// Extract up to 3 semantic triples from content using simple patterns.
/// Returns `(subject, predicate, object)` tuples.
pub fn extract_triples_local(content: &str) -> Vec<(String, String, String)> {
    let mut triples: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let lower = content.to_lowercase();

    let patterns: &[(&str, &str)] = &[
        (" es ",    "es_un"),
        (" is ",    "es_un"),
        (" usa ",   "usa"),
        (" uses ",  "usa"),
        (" tiene ", "tiene"),
        (" has ",   "tiene"),
        (" -> ",    "relacionado_con"),
        (" → ",     "relacionado_con"),
    ];

    for (sep, predicate) in patterns {
        if triples.len() >= 3 {
            break;
        }
        if let Some(pos) = lower.find(sep) {
            let subj = lower[..pos].split_whitespace().last().unwrap_or("").trim().to_string();
            let rest = &lower[pos + sep.len()..];
            let obj  = rest.split_whitespace().next().unwrap_or("").trim()
                .trim_end_matches([',', '.', ';', ':'])
                .to_string();

            if subj.is_empty() || obj.is_empty() || subj.len() > 40 || obj.len() > 40 {
                continue;
            }
            let key = format!("{}|{}|{}", subj, predicate, obj);
            if seen.insert(key) {
                triples.push((subj, predicate.to_string(), obj));
            }
        }
    }

    triples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_es_pattern() {
        let t = extract_triples_local("Rust es un lenguaje de programación");
        assert!(!t.is_empty());
        assert_eq!(t[0].1, "es_un");
    }

    #[test]
    fn test_limit_three() {
        let t = extract_triples_local("a es b, c usa d, e tiene f, g -> h, i has j");
        assert!(t.len() <= 3);
    }

    #[test]
    fn test_arrow_pattern() {
        let t = extract_triples_local("tylluan_do -> guild via curriculum");
        assert!(!t.is_empty());
        assert_eq!(t[0].1, "relacionado_con");
    }
}
