//! T-DSH-176: corpus de anclas de enrutado con respaldo tracked.
//!
//! El kernel leía `data/routing_anchors_seed.jsonl`, pero `data/` está en
//! `.gitignore` y una instalación limpia arrancaba sin anclas. Ahora el corpus
//! viaja **compilado** desde `assets/routing_anchors_seed.jsonl` (ruta tracked) y
//! `data/routing_anchors_seed.jsonl` sigue siendo una **sobrescritura opcional**
//! del operador (misma ruta que antes, sin romper producción).

use serde_json::Value;

/// Corpus embebido en el binario desde la ruta tracked `assets/`.
pub const EMBEDDED_SEED: &str = include_str!("../../../../assets/routing_anchors_seed.jsonl");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorEntry {
    pub guild: String,
    pub intent: String,
    pub source: String,
}

/// Parsea líneas JSONL (`{"guild","intent","source"}`); ignora líneas vacías o
/// entradas sin guild/intent. `source` por defecto = `"seed"`.
pub fn parse_anchors(content: &str) -> Vec<AnchorEntry> {
    let mut out = Vec::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let guild = entry.get("guild").and_then(Value::as_str).unwrap_or_default();
        let intent = entry.get("intent").and_then(Value::as_str).unwrap_or_default();
        if guild.is_empty() || intent.is_empty() {
            continue;
        }
        let source = entry
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("seed")
            .to_string();
        out.push(AnchorEntry {
            guild: guild.to_string(),
            intent: intent.to_string(),
            source,
        });
    }
    out
}

/// Corpus efectivo: `data/routing_anchors_seed.jsonl` (bajo `TYLLUAN_DATA_DIR`)
/// si existe; si no, el embebido. Devuelve también de dónde salió.
pub fn anchor_seed_corpus() -> (String, &'static str) {
    anchor_seed_corpus_in(&crate::config::data_dir())
}

/// Igual que [`anchor_seed_corpus`] pero con el directorio de datos explicito:
/// los tests lo usan para no tocar la variable de entorno global (varios modulos de
/// test mutaban `TYLLUAN_DATA_DIR` con cerrojos distintos y podian pisarse).
pub fn anchor_seed_corpus_in(data_dir: &std::path::Path) -> (String, &'static str) {
    let override_path = data_dir.join("routing_anchors_seed.jsonl");
    match std::fs::read_to_string(&override_path) {
        Ok(content) => (content, "data-override"),
        // Sin override: caso normal, el corpus embebido.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (EMBEDDED_SEED.to_string(), "embedded-asset")
        }
        // Hay un fichero (o algo) en la ruta del override pero no se puede leer: caer al
        // embebido en silencio haria creer al operador que su corpus esta activo.
        Err(e) => {
            tracing::warn!(
                "anchor seed: el override {} existe pero no se puede leer ({e}); uso el corpus embebido",
                override_path.display()
            );
            (EMBEDDED_SEED.to_string(), "embedded-asset-override-unreadable")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_seed_has_the_180_rescued_anchors() {
        let entries = parse_anchors(EMBEDDED_SEED);
        assert!(
            entries.len() >= 180,
            "se esperaban >=180 anclas embebidas, hay {}",
            entries.len()
        );
        assert!(entries.iter().any(|e| e.guild == "bash"));
        assert!(entries.iter().all(|e| !e.guild.is_empty() && !e.intent.is_empty()));
    }

    #[test]
    fn data_dir_override_wins_and_embedded_is_the_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let (content, origin) = anchor_seed_corpus_in(dir.path());
        assert_eq!(origin, "embedded-asset");
        assert_eq!(parse_anchors(&content).len(), parse_anchors(EMBEDDED_SEED).len());

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("routing_anchors_seed.jsonl"),
            "{\"guild\":\"bash\",\"intent\":\"solo una ancla de prueba\"}\n",
        )
        .unwrap();
        let (content, origin) = anchor_seed_corpus_in(dir.path());
        assert_eq!(origin, "data-override");
        let entries = parse_anchors(&content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "seed");
    }

    /// Buffy H3 (2026-11-04): un override presente pero ilegible no puede pasar por
    /// "embebido normal". Un directorio con ese nombre falla al leerse con un error que no es NotFound.
    #[test]
    fn unreadable_override_is_reported_not_silently_embedded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("routing_anchors_seed.jsonl")).unwrap();
        let (content, origin) = anchor_seed_corpus_in(dir.path());
        assert_eq!(origin, "embedded-asset-override-unreadable");
        assert_eq!(parse_anchors(&content).len(), parse_anchors(EMBEDDED_SEED).len());
    }
}
