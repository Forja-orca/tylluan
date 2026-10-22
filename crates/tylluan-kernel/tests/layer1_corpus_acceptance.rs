//! T-DSH-31 — aceptación del corpus de capa 1 (capa 1 = `matches_injection_pattern`).
//!
//! Prerrequisito de aceptación de D-31 (patrones/normalización en español). Este
//! test fija DOS invariantes medibles y deja el trabajo de D-31 verificable con
//! una sola ejecución:
//!
//! (a) **Cero falsos positivos**: ninguno de los textos de los dos corpus
//!     benignos (inglés y español) debe coincidir con un patrón de inyección.
//!     Debe pasar HOY y protege a D-31 de romperlo al añadir patrones.
//! (b) **Suelo de detección por categoría**: cada categoría de los corpus de
//!     inyección (inglés literal, español y cada ofuscación: espaciado, ancho
//!     cero, homoglifo cirílico, ancho completo, puntuación intercalada) debe
//!     detectar al menos lo que detecta HOY. Si una categoría hoy detecta 0, su
//!     suelo es 0 (nunca regresión). Cuando Deep entregue D-31, este test sube
//!     esos suelos y el delta por categoría queda a la vista.
//!
//! La tabla de línea base se imprime con `--nocapture`:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test layer1_corpus_acceptance -- --nocapture
//!
//! Los corpus son datos de benchmark, no código de producción; este fichero es
//! un test de aceptación nuevo y no toca `src/`.

use std::path::PathBuf;

use tylluan_kernel::security::poison_patterns::matches_injection_pattern;

const BENIGN_EN: &str = "benign_corpus_v1.json";
const BENIGN_ES: &str = "benign_corpus_es_v1.json";
const INJECTION_EN: &str = "injection_corpus_v1.json";
const INJECTION_ES: &str = "injection_corpus_es_v1.json";

/// Suelo de detección por categoría, MEDIDO el 2026-10-22 con la capa 1 de
/// entonces (10 patrones en inglés, sin normalización). Cualquier patrón nuevo
/// de D-31 solo puede SUBIR estos números; bajar cualquiera es una regresión.
const FLOORS: &[(&str, usize)] = &[
    ("en_literal", 8),
    ("es", 0),
    ("es_espaciado", 0),
    ("es_ancho_cero", 0),
    ("es_homoglifo_cirilico", 0),
    ("es_ancho_completo", 0),
    ("es_puntuacion", 0),
];

fn corpus_path(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks")
        .join(name)
}

fn load(name: &str) -> serde_json::Value {
    let path = corpus_path(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("JSON invalido en {}: {e}", path.display()))
}

fn samples(doc: &serde_json::Value) -> &Vec<serde_json::Value> {
    doc["muestras"]
        .as_array()
        .unwrap_or_else(|| panic!("falta 'muestras' como array"))
}

fn text_of(m: &serde_json::Value) -> &str {
    m["texto"].as_str().unwrap_or_else(|| panic!("muestra sin 'texto' string: {m}"))
}

fn meta_of<'a>(m: &'a serde_json::Value, key: &str) -> &'a str {
    m[key].as_str().unwrap_or_else(|| panic!("muestra sin '{key}' string: {m}"))
}

/// Acumula (detectadas, total) por categoría, sin asumir el orden de los corpus.
#[derive(Default)]
struct Buckets {
    en_literal: (usize, usize),
    es: (usize, usize),
    es_espaciado: (usize, usize),
    es_ancho_cero: (usize, usize),
    es_homoglifo_cirilico: (usize, usize),
    es_ancho_completo: (usize, usize),
    es_puntuacion: (usize, usize),
}

impl Buckets {
    fn add(&mut self, category: &str, flagged: bool, m: &serde_json::Value) {
        let slot = match category {
            "en_literal" => &mut self.en_literal,
            "es" => &mut self.es,
            "es_espaciado" => &mut self.es_espaciado,
            "es_ancho_cero" => &mut self.es_ancho_cero,
            "es_homoglifo_cirilico" => &mut self.es_homoglifo_cirilico,
            "es_ancho_completo" => &mut self.es_ancho_completo,
            "es_puntuacion" => &mut self.es_puntuacion,
            other => panic!("categoria desconocida en el corpus: {other} (muestra {m})"),
        };
        slot.1 += 1;
        if flagged {
            slot.0 += 1;
        }
    }

    /// (nombre, detectadas, total) en el orden de la tabla.
    fn rows(&self) -> Vec<(&'static str, usize, usize)> {
        vec![
            ("en_literal", self.en_literal.0, self.en_literal.1),
            ("es", self.es.0, self.es.1),
            ("es_espaciado", self.es_espaciado.0, self.es_espaciado.1),
            ("es_ancho_cero", self.es_ancho_cero.0, self.es_ancho_cero.1),
            (
                "es_homoglifo_cirilico",
                self.es_homoglifo_cirilico.0,
                self.es_homoglifo_cirilico.1,
            ),
            ("es_ancho_completo", self.es_ancho_completo.0, self.es_ancho_completo.1),
            ("es_puntuacion", self.es_puntuacion.0, self.es_puntuacion.1),
        ]
    }
}

fn floor_for(category: &str) -> usize {
    FLOORS
        .iter()
        .find(|(c, _)| *c == category)
        .map(|(_, n)| *n)
        .unwrap_or_else(|| panic!("falta suelo para la categoria '{category}'"))
}

/// (a) Cero falsos positivos sobre los dos corpus benignos.
#[test]
fn layer1_never_flags_benign_corpus() {
    let mut checked = 0usize;
    for name in [BENIGN_EN, BENIGN_ES] {
        let doc = load(name);
        for m in samples(&doc) {
            let text = text_of(m);
            assert!(
                !matches_injection_pattern(text),
                "{name}: falso positivo de la capa 1 en la muestra '{}': {text}",
                m["id"].as_str().unwrap_or("?")
            );
            checked += 1;
        }
    }
    assert!(checked >= 340, "benignos revisados: {checked} < 340");
    println!("capa 1: 0 falsos positivos en {checked} muestras benignas (EN+ES)");
}

/// (b) Suelo de detección por categoría, nunca por debajo de lo medido hoy.
#[test]
fn layer1_detection_floors_by_category() {
    let mut buckets = Buckets::default();

    // Corpus inglés: separa los textos en español que el propio corpus declara
    // con `cat == "espanol"` para no mezclar idiomas en la categoría literal.
    let en = load(INJECTION_EN);
    for m in samples(&en) {
        let category = if meta_of(m, "cat") == "espanol" { "es" } else { "en_literal" };
        buckets.add(category, matches_injection_pattern(text_of(m)), m);
    }

    // Corpus español: 60 variantes base + 20 ofuscadas por variante declarada.
    let es = load(INJECTION_ES);
    for m in samples(&es) {
        let variante = meta_of(m, "variante");
        let category = if variante == "directa" {
            "es"
        } else {
            match variante {
                "espaciado" => "es_espaciado",
                "ancho_cero" => "es_ancho_cero",
                "homoglifo_cirilico" => "es_homoglifo_cirilico",
                "ancho_completo" => "es_ancho_completo",
                "puntuacion" => "es_puntuacion",
                other => panic!("variante desconocida en el corpus: {other} (muestra {m})"),
            }
        };
        buckets.add(category, matches_injection_pattern(text_of(m)), m);
    }

    println!("=== linea base de capa 1 por categoria (medido 2026-10-22) ===");
    println!("{:<24} {:>10} {:>8} {:>8} {:>7}", "categoria", "detectadas", "total", "suelo", "estado");
    let mut failures = Vec::new();
    for (category, flagged, total) in buckets.rows() {
        let floor = floor_for(category);
        assert!(total > 0, "categoria '{category}' sin muestras");
        assert!(floor <= total, "suelo de '{category}' ({floor}) > total ({total})");
        let status = if flagged >= floor { "OK" } else { "REGRESION" };
        println!("{category:<24} {flagged:>10} {total:>8} {floor:>8} {status:>7}");
        if flagged < floor {
            failures.push(format!("{category}: {flagged} < suelo {floor} (total {total})"));
        }
    }
    assert!(
        failures.is_empty(),
        "la capa 1 detecta menos que la linea base en: {}",
        failures.join("; ")
    );
}
