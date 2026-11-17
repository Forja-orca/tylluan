//! ADR-033 **R2** — recuperador BM25 sobre los descriptores de guild.
//!
//! Esto es **solo el recuperador** (decisión del TL, T4507): la compuerta
//! sintáctica de shell queda aparcada (50 % de precisión en muestra fresca) y no
//! se cablea.
//!
//! Por qué existe: sin embeddings, el matcher compara por subcadena y el banco R0
//! mide ~23 % de acierto de guild; BM25 sobre los descriptores da **69,3 %** en el
//! banco held-out (dsh, 2026-11-17).
//!
//! **Apagado por defecto.** ADR-033 sigue en estado *Propuesto*: encenderlo es una
//! decisión del operador (`[nexus] bm25_retriever = true`), y si se enciende y no
//! encuentra nada, el enrutador sigue su camino de siempre.
//!
//! Funciones puras: los tests no necesitan servidor, registry ni embeddings.

use std::collections::HashMap;

use super::catalog::GuildDescriptor;

/// Tokenizador del banco (idéntico al prototipo para que las cifras sean
/// comparables): minúsculas, `_` → espacio, palabras de ≥2 caracteres.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('_', " ")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_string)
        .collect()
}

/// Documento indexable de un guild.
#[derive(Debug, Clone)]
pub struct GuildDoc {
    pub name: String,
    pub tokens: Vec<String>,
}

impl GuildDoc {
    /// Texto que se indexa de un guild del catálogo: nombre (x2), descripción,
    /// frases disparadoras y argumentos requeridos. **No** se indexan nombres de
    /// tool: medido en D201, aportan en el banco de diseño y nada en held-out.
    pub fn from_descriptor(g: &GuildDescriptor) -> Self {
        let mut texto = format!("{} {} {}", g.name, g.name, g.description);
        for frase in &g.trigger_phrases {
            texto.push(' ');
            texto.push_str(frase);
        }
        for arg in &g.required_args {
            texto.push(' ');
            texto.push_str(arg);
        }
        Self { name: g.name.clone(), tokens: tokenize(&texto) }
    }
}

/// Índice BM25 (k1=1.5, b=0.75 — los valores del banco).
pub struct Bm25Index {
    docs: Vec<GuildDoc>,
    df: HashMap<String, usize>,
    avg_len: f64,
}

impl Bm25Index {
    pub fn new(docs: Vec<GuildDoc>) -> Self {
        let mut df: HashMap<String, usize> = HashMap::new();
        for doc in &docs {
            let mut vistos: Vec<&String> = doc.tokens.iter().collect();
            vistos.sort();
            vistos.dedup();
            for t in vistos {
                *df.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let avg_len = if docs.is_empty() {
            0.0
        } else {
            docs.iter().map(|d| d.tokens.len() as f64).sum::<f64>() / docs.len() as f64
        };
        Self { docs, df, avg_len }
    }

    fn score(&self, query: &[String], doc: &GuildDoc) -> f64 {
        if self.avg_len <= 0.0 {
            return 0.0;
        }
        let (k1, b) = (1.5, 0.75);
        let n = self.docs.len() as f64;
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for t in &doc.tokens {
            *tf.entry(t.as_str()).or_insert(0) += 1;
        }
        let mut total = 0.0;
        let mut vistas: Vec<&String> = query.iter().collect();
        vistas.sort();
        vistas.dedup();
        for q in vistas {
            let Some(&f) = tf.get(q.as_str()) else { continue };
            let df = *self.df.get(q).unwrap_or(&0) as f64;
            if df == 0.0 {
                continue;
            }
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            let largo = doc.tokens.len() as f64;
            total +=
                idf * (f as f64 * (k1 + 1.0)) / (f as f64 + k1 * (1.0 - b + b * largo / self.avg_len));
        }
        total
    }

    /// Guild mejor puntuado para `query`, si alguno puntúa > 0.
    pub fn best_guild(&self, query: &str) -> Option<String> {
        let q = tokenize(query);
        if q.is_empty() {
            return None;
        }
        let mut mejor: Option<(f64, &str)> = None;
        for doc in &self.docs {
            let s = self.score(&q, doc);
            if s > 0.0 && mejor.is_none_or(|(sm, _)| s > sm) {
                mejor = Some((s, doc.name.as_str()));
            }
        }
        mejor.map(|(_, nombre)| nombre.to_string())
    }
}

/// **Decisión pura del flag** (es lo que se testea sin servidor): con el flag
/// apagado el recuperador **ni se consulta**; encendido, decide BM25 y si no hay
/// coincidencia devuelve `None` para que el llamante siga su camino (fallback).
pub fn bm25_candidate<'a>(
    enabled: bool,
    guilds: impl IntoIterator<Item = &'a GuildDescriptor>,
    intent: &str,
) -> Option<String> {
    if !enabled {
        return None;
    }
    let docs: Vec<GuildDoc> = guilds.into_iter().map(GuildDoc::from_descriptor).collect();
    if docs.is_empty() {
        return None;
    }
    Bm25Index::new(docs).best_guild(intent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::catalog::builtin_catalog;

    fn catalogo() -> Vec<GuildDescriptor> {
        builtin_catalog()
    }

    /// **Criterio 1 del TL**: con el flag encendido, BM25 acierta un caso donde el
    /// router actual falla. «list the files» es el ejemplo del ADR-033 (el matcher
    /// por subcadena devolvía `NO_GUILD_MATCH`).
    #[test]
    fn d202_con_flag_bm25_acierta_donde_el_router_falla() {
        let guilds = catalogo();
        let intent = "list the files of the docs directory";
        let elegido = bm25_candidate(true, guilds.iter(), intent)
            .expect("BM25 debe encontrar un guild para este intent");
        assert_eq!(elegido, "filesystem", "BM25 debe elegir filesystem");

        // Y el camino actual (matcher por subcadena, sin embeddings) NO lo resuelve así:
        let actual = crate::router::matcher::GuildMatcher::new(catalogo())
            .match_guild(intent, None, 0.25, None)
            .map(|m| m.guild_name);
        assert_ne!(
            actual.as_deref(),
            Some("filesystem"),
            "este test solo vale si el router actual falla aquí (actual: {actual:?})"
        );
    }

    /// **Criterio 2 del TL**: con el flag apagado, nada cambia. La función ni
    /// construye índice: devuelve `None` aunque BM25 tendría respuesta.
    #[test]
    fn d202_con_flag_apagado_no_se_consulta_bm25() {
        let guilds = catalogo();
        let intent = "list the files of the docs directory";
        assert!(bm25_candidate(true, guilds.iter(), intent).is_some());
        assert!(
            bm25_candidate(false, guilds.iter(), intent).is_none(),
            "apagado = idéntico a hoy: el recuperador no participa"
        );
    }

    /// **Fallback**: si BM25 no encuentra nada, devuelve `None` y el llamante sigue
    /// su camino de siempre (no un error).
    #[test]
    fn d202_sin_coincidencia_hay_fallback() {
        let guilds = catalogo();
        assert!(bm25_candidate(true, guilds.iter(), "zzzz qqqq xxxx").is_none());
        assert!(bm25_candidate(true, [].iter(), "list the files").is_none());
    }

    /// El índice no inventa: sin términos en común, no hay ganador.
    #[test]
    fn d202_el_indice_no_inventa_ganador() {
        let guilds = catalogo();
        let texto = bm25_candidate(true, guilds.iter(), "zzzz qqqq").is_none();
        assert!(texto);
    }
}
