//! T-DSH-27 â€” forma y linea base de los corpus ES (benigno + inyeccion).
//!
//! Valida esquema y recuentos de `benchmarks/benign_corpus_es_v1.json` y
//! `benchmarks/injection_corpus_es_v1.json`, y mide la LINEA BASE de la capa 1
//! (`matches_injection_pattern`, solo patrones en ingles): hoy debe detectar
//! 0 de las inyecciones en espanol y 0 de los textos benignos.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test corpus_es_shape -- --nocapture

use std::collections::HashSet;
use std::path::PathBuf;

use tylluan_kernel::security::poison_patterns::matches_injection_pattern;

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

fn common_meta(doc: &serde_json::Value, expected_dataset: &str) {
    assert_eq!(doc["dataset"].as_str(), Some(expected_dataset), "nombre de dataset");
    assert_eq!(doc["licencia"].as_str(), Some("MIT"), "licencia MIT declarada");
    assert!(
        !doc["procedencia"].as_str().unwrap_or("").is_empty(),
        "procedencia no vacia (texto original, sin fuentes externas)"
    );
    assert!(
        !doc["autor"].as_str().unwrap_or("").is_empty(),
        "autor declarado"
    );
}

#[test]
fn benign_corpus_es_shape_and_layer1_baseline() {
    let doc = load("benign_corpus_es_v1.json");
    common_meta(&doc, "benign_corpus_es_v1");
    let ms = samples(&doc);
    assert!(ms.len() >= 300, "benignos: {} < 300", ms.len());

    let mut ids = HashSet::new();
    let mut texts = HashSet::new();
    let (mut tecnico, mut seguridad, mut frase, mut flagged) = (0, 0, 0, 0);
    for m in ms {
        let id = m["id"].as_str().expect("id string");
        assert!(ids.insert(id.to_string()), "id duplicado: {id}");
        let t = text_of(m);
        assert!(t.chars().count() >= 15, "texto demasiado corto ({id}): {t}");
        assert!(texts.insert(t.to_string()), "texto duplicado en benignos ({id})");
        match m["tipo"].as_str() {
            Some("tecnico") => tecnico += 1,
            Some("seguridad") => seguridad += 1,
            Some("frase_inocua") => frase += 1,
            other => panic!("tipo desconocido en {id}: {other:?}"),
        }
        if matches_injection_pattern(t) {
            flagged += 1;
        }
    }
    assert!(tecnico >= 200, "tecnicos: {tecnico} < 200");
    assert!(seguridad >= 60, "de seguridad: {seguridad} < 60");
    assert!(frase >= 40, "con frase gatillo: {frase} < 40");
    assert_eq!(flagged, 0, "la capa 1 marco {flagged} benignos ES (deberia ser 0)");

    println!("| benign_corpus_es_v1 | total {} | tecnico {} | seguridad {} | frase_inocua {} | capa1 marcados {} |", ms.len(), tecnico, seguridad, frase, flagged);
}

#[test]
fn injection_corpus_es_shape_and_layer1_baseline() {
    let doc = load("injection_corpus_es_v1.json");
    common_meta(&doc, "injection_corpus_es_v1");
    let ms = samples(&doc);
    assert!(ms.len() >= 80, "inyecciones totales: {} < 80", ms.len());

    let mut ids = HashSet::new();
    let (mut base, mut flagged) = (0usize, 0usize);
    let mut variants: HashSet<String> = HashSet::new();
    let mut saw_zero_width = false;
    let mut saw_fullwidth = false;
    let mut saw_cyrillic = false;
    let mut saw_punct_inside = false;

    for m in ms {
        let id = m["id"].as_str().expect("id string");
        assert!(ids.insert(id.to_string()), "id duplicado: {id}");
        let t = text_of(m);
        assert!(!t.trim().is_empty(), "texto vacio en {id}");
        if m["variante"].as_str() == Some("directa") {
            base += 1;
        } else if let Some(v) = m["variante"].as_str() {
            variants.insert(v.to_string());
        }
        if t.contains('\u{200b}') {
            saw_zero_width = true;
        }
        if t.chars().any(|c| ('\u{ff01}'..='\u{ff5e}').contains(&c)) {
            saw_fullwidth = true;
        }
        if t.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)) {
            saw_cyrillic = true;
        }
        // puntuacion intercalada: un punto entre letras dentro de una misma palabra
        if t.as_bytes().windows(3).any(|w| w[0].is_ascii_alphabetic() && w[1] == b'.' && w[2].is_ascii_alphabetic()) {
            saw_punct_inside = true;
        }
        if matches_injection_pattern(t) {
            flagged += 1;
        }
    }
    assert!(base >= 60, "inyecciones base ES: {base} < 60");
    for required in ["espaciado", "ancho_cero", "homoglifo_cirilico", "ancho_completo", "puntuacion"] {
        assert!(variants.contains(required), "falta la variante ofuscada '{required}'");
    }
    assert!(saw_zero_width, "ninguna variante con ancho cero real");
    assert!(saw_fullwidth, "ninguna variante con ancho completo real");
    assert!(saw_cyrillic, "ninguna variante con homoglifo cirilico real");
    assert!(saw_punct_inside, "ninguna variante con puntuacion intercalada real");
    assert_eq!(
        flagged, 0,
        "linea base: la capa 1 ya detecta {flagged} inyecciones ES (se esperaba 0 hoy)"
    );

    println!("| injection_corpus_es_v1 | base {} | variantes {} | total {} | capa1 detectadas {} |", base, variants.len(), ms.len(), flagged);
}
