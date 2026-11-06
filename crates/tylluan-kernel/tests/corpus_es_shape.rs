//! T-DSH-27 — forma y linea base de los corpus ES (benigno + inyeccion).
//!
//! Valida esquema y recuentos de `benchmarks/benign_corpus_es_v1.json` y
//! `benchmarks/injection_corpus_es_v1.json`, y mide la LINEA BASE de la capa 1
//! (`matches_injection_pattern`).
//!
//! T-DSH-151 (2026-10-31, base b9f1331): la capa 1 ya cubre 13 de las
//! inyecciones ES (antes 0: solo había patrones en inglés; D-31 y los patrones
//! ES posteriores las detectan). El test fija ese recuento para que una
//! regresión a menos detecciones o un salto raro se vean; si sube, actualizar
//! con motivo (y comprobar que no son falsos positivos sobre benignos, que
//! siguen exigiendo 0).
//!
//! T-DSH-180 (2026-11-04, base 3d3eb1b3; antes etiquetada 179, renombrada por
//! orden del TL en T2771): la línea base sube 13 -> 32 CON
//! MOTIVO, no en silencio. Qué lo explica: B-24 (`7d7802b6`) amplió el corpus
//! ES de inyección a 105 muestras añadiendo cinco familias de ofuscación más
//! (etiqueta_unicode, bidi, ansi, base64, reinyectado, 5 de cada), y B-29
//! añadió los detectores de codificación (`has_hidden_tag_run`,
//! `has_bidi_override`, `has_hazardous_ansi`, `base64_payload_matches`) que
//! las cubren. Recuento MEDIDO en 3d3eb1b3: `deep_run_corpus_es_shape.log`
//! muestra la aserción fallando con `left: 32 / right: 13`. Lado benigno:
//! los 8 controles B-23 ya tienen `tipo` (clasificación por contenido de
//! @deep en T2749, aplicada por el dueño del corpus) y el test por fin los
//! mide; los falsos positivos siguen exigiendo 0.
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
    // 13 -> 32 con motivo (cabecera, T-DSH-180): ampliacion B-24 del corpus a
    // 105 + detectores de codificacion B-29; medido en 3d3eb1b3
    // (deep_run_corpus_es_shape.log: left 32 / right 13).
    assert_eq!(
        flagged, 32,
        "linea base actualizada (T-DSH-151, subida a 32 con motivo en T-DSH-180): \
         la capa 1 ya detecta {flagged} inyecciones ES de 105; \
         si baja, hay regresion; si sube, actualizar con motivo y revisar benignos"
    );

    println!("| injection_corpus_es_v1 | base {} | variantes {} | total {} | capa1 detectadas {} |", base, variants.len(), ms.len(), flagged);
}
