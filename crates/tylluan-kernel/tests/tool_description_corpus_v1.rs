//! M-BUFFY-6 — validación del esquema de `benchmarks/tool_description_corpus_v1.json`
//! y medición de la capa 1 (`matches_injection_pattern`) sobre él, sin arrancar
//! kernel ni tocar `src/`.
//!
//! El corpus son datos de benchmark escritos por Buffy: 30 descripciones de
//! herramientas tipo MCP, 15 maliciosas y 15 benignas. Este fichero hace dos
//! cosas:
//!
//! 1. `tool_description_corpus_schema` fija el esquema: campos por caso
//!    (`id`, `cat`, `tecnica`, `texto`, `procedencia`), ids únicos, 15/15 por
//!    categoría, técnicas dentro de las declaradas y ningún par de textos con
//!    Jaccard de trigramas >= 0,8 (ni duplicados exactos, que quedan por
//!    debajo de ese umbral).
//! 2. `layer1_measure_tool_descriptions` mide detección y falsos positivos con
//!    el mismo método que `layer1_hard_corpus_measure.rs` e imprime las tablas
//!    con `--nocapture`. A propósito NO fija umbrales de paso: la cola
//!    M-BUFFY-6 pide reportar los números reales de la capa 1 actual, sin
//!    ajustar la capa (ni el test) para que salgan bien.
//!
//! Run:
//!   cargo test -p tylluan-kernel --test tool_description_corpus_v1 -- --nocapture

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use tylluan_kernel::security::poison_patterns::matches_injection_pattern;

/// Técnicas declaradas en la cola M-BUFFY-6 para las 15 maliciosas.
const TECNICAS_MALICIOSAS: &[&str] = &[
    "instruccion_oculta",
    "exfiltracion",
    "orden_ignorar_usuario",
    "texto_invisible",
    "homoglifos",
];

/// Técnicas de los 15 benignos de aspecto parecido (largos, «ignore» legítimo…).
const TECNICAS_BENIGNAS: &[&str] = &[
    "ignore_legitimo",
    "descripcion_larga",
    "contexto_seguridad",
    "terminologia_tecnica",
    "ejemplo_codigo",
];

/// Conjunto de trigramas de un texto (alias: evita `clippy::type_complexity`).
type Trigrams = HashSet<(char, char, char)>;

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/tool_description_corpus_v1.json")
}

fn load() -> serde_json::Value {
    let path = corpus_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("JSON inválido: {e}"))
}

fn str_field<'a>(m: &'a serde_json::Value, key: &str) -> &'a str {
    m[key]
        .as_str()
        .unwrap_or_else(|| panic!("muestra sin '{key}' string: {m}"))
}

/// Trigramas de caracteres sobre el texto en minúsculas, sin puntuación ni
/// espacios: misma idea de comparación que en los tests de corpus duro.
fn trigrams(texto: &str) -> Trigrams {
    let norm: Vec<char> = texto
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    norm.windows(3).map(|w| (w[0], w[1], w[2])).collect()
}

fn jaccard(a: &Trigrams, b: &Trigrams) -> f64 {
    let inter = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Esquema del corpus: campos, recuentos, ids únicos, técnicas y parecidos.
#[test]
fn tool_description_corpus_schema() {
    let doc = load();
    for key in [
        "dataset",
        "creado",
        "autor",
        "licencia",
        "procedencia",
        "descripcion",
        "muestras",
    ] {
        assert!(!doc[key].is_null(), "falta la clave superior '{key}'");
    }
    let muestras = doc["muestras"]
        .as_array()
        .expect("'muestras' no es un array");
    assert_eq!(muestras.len(), 30, "el corpus debe tener 30 casos");

    let mut ids: HashSet<&str> = HashSet::new();
    let mut vistos_mal: HashSet<&str> = HashSet::new();
    let mut vistos_ben: HashSet<&str> = HashSet::new();
    let (mut n_mal, mut n_ben) = (0usize, 0usize);

    for m in muestras {
        let id = str_field(m, "id");
        let cat = str_field(m, "cat");
        let tecnica = str_field(m, "tecnica");
        let texto = str_field(m, "texto");
        let procedencia = str_field(m, "procedencia");

        assert!(ids.insert(id), "id duplicado: {id}");
        assert!(!texto.trim().is_empty(), "{id}: texto vacío");
        assert!(!procedencia.trim().is_empty(), "{id}: procedencia vacía");

        match cat {
            "maliciosa" => {
                n_mal += 1;
                assert!(
                    id.starts_with("tdp_mal_"),
                    "{id}: cat 'maliciosa' con id fuera del patrón tdp_mal_*"
                );
                assert!(
                    TECNICAS_MALICIOSAS.contains(&tecnica),
                    "{id}: técnica '{tecnica}' fuera de las declaradas para maliciosas"
                );
                vistos_mal.insert(tecnica);
            }
            "benigna" => {
                n_ben += 1;
                assert!(
                    id.starts_with("tdp_ben_"),
                    "{id}: cat 'benigna' con id fuera del patrón tdp_ben_*"
                );
                assert!(
                    TECNICAS_BENIGNAS.contains(&tecnica),
                    "{id}: técnica '{tecnica}' fuera de las declaradas para benignas"
                );
                vistos_ben.insert(tecnica);
            }
            other => panic!("cat desconocida '{other}' en {id}"),
        }
    }

    assert_eq!(n_mal, 15, "maliciosas: {n_mal} (deben ser 15)");
    assert_eq!(n_ben, 15, "benignas: {n_ben} (deben ser 15)");
    for t in TECNICAS_MALICIOSAS {
        assert!(
            vistos_mal.contains(t),
            "ningún caso malicioso usa la técnica '{t}'"
        );
    }
    for t in TECNICAS_BENIGNAS {
        assert!(
            vistos_ben.contains(t),
            "ningún caso benigno usa la técnica '{t}'"
        );
    }

    // Sin duplicados exactos ni casi-duplicados: Jaccard de trigramas < 0,8
    // entre cualquier par de los 30 textos (435 pares).
    let grams: Vec<(&str, Trigrams)> = muestras
        .iter()
        .map(|m| {
            let id = str_field(m, "id");
            (id, trigrams(str_field(m, "texto")))
        })
        .collect();
    let mut peor = (0.0f64, "", "");
    let mut fuera = Vec::new();
    for i in 0..grams.len() {
        for j in (i + 1)..grams.len() {
            let s = jaccard(&grams[i].1, &grams[j].1);
            if s > peor.0 {
                peor = (s, grams[i].0, grams[j].0);
            }
            if s >= 0.8 {
                fuera.push(format!("{}↔{}={s:.2}", grams[i].0, grams[j].0));
            }
        }
    }
    println!(
        "Jaccard de trigramas más alto entre pares: {:.3} ({} ↔ {})",
        peor.0, peor.1, peor.2
    );
    assert!(
        fuera.is_empty(),
        "pares con Jaccard >= 0,8 (casi duplicados): {}",
        fuera.join("; ")
    );
}

/// Mide la capa 1 actual sobre las 30 descripciones. Imprime la tabla y los
/// ids; NO afirma umbrales de detección (reportar, no ajustar).
#[test]
fn layer1_measure_tool_descriptions() {
    let doc = load();
    let muestras = doc["muestras"]
        .as_array()
        .expect("'muestras' no es un array");

    let mut tabla: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut mal_detectadas: Vec<&str> = Vec::new();
    let mut mal_pasan: Vec<&str> = Vec::new();
    let mut ben_fp: Vec<&str> = Vec::new();
    let (mut n_mal, mut n_ben) = (0usize, 0usize);

    for m in muestras {
        let id = str_field(m, "id");
        let cat = str_field(m, "cat");
        let tecnica = str_field(m, "tecnica");
        let flagged = matches_injection_pattern(str_field(m, "texto"));

        let slot = tabla.entry(format!("{cat}/{tecnica}")).or_insert((0, 0));
        slot.1 += 1;
        if flagged {
            slot.0 += 1;
        }

        match cat {
            "maliciosa" => {
                n_mal += 1;
                if flagged {
                    mal_detectadas.push(id);
                } else {
                    mal_pasan.push(id);
                }
            }
            "benigna" => {
                n_ben += 1;
                if flagged {
                    ben_fp.push(id);
                }
            }
            other => panic!("cat desconocida '{other}' en {id}"),
        }
    }

    println!("=== M-BUFFY-6 · capa 1 sobre tool_description_corpus_v1 ===");
    println!("{:<40} {:>11} {:>7}", "categoria/tecnica", "detectadas", "total");
    for (clave, (det, total)) in &tabla {
        println!("{clave:<40} {det:>11} {total:>7}");
    }
    println!(
        "maliciosas detectadas: {}/{} = {:.1} %",
        mal_detectadas.len(),
        n_mal,
        100.0 * mal_detectadas.len() as f64 / n_mal as f64
    );
    println!("    que PASAN sin detectar: {mal_pasan:?}");
    println!(
        "falsos positivos sobre benignas: {}/{} = {:.1} %",
        ben_fp.len(),
        n_ben,
        100.0 * ben_fp.len() as f64 / n_ben as f64
    );
    println!("    falsos positivos: {ben_fp:?}");

    // El único invariant del test de medición: se han medido los 30 casos.
    assert_eq!(
        n_mal + n_ben,
        muestras.len(),
        "medidos {} de {} casos",
        n_mal + n_ben,
        muestras.len()
    );
}
