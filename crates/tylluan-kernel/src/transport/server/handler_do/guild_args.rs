//! ADR-033 **R1** — argumentos declarados por tool + error correctivo.
//!
//! Antes (`prepare.rs:157-159`) el router copiaba el intent crudo en **nueve**
//! campos de texto a la vez y añadía rutas/URLs a otros cinco nombres, sin mirar
//! el esquema de la herramienta. Eso acierta por casualidad cuando la tool
//! declara `query` o `path`, y falla por dentro (con un error del guild que no
//! dice nada útil) cuando declara otro nombre.
//!
//! R1: **solo se rellenan los parámetros que el esquema declara**, y lo que falta
//! produce un **error correctivo** que dice qué falta y qué acepta la tool.
//!
//! Cómo se integra sin romper nada: el blob histórico (y las pistas de ruta/URL/
//! comando, y los ajustes por guild de `prepare.rs`) siguen existiendo como
//! **borrador**. Aquí el borrador se recorta a los campos declarados y se valida
//! lo requerido. Así se conserva todo lo que hoy funciona —el borrador sigue
//! siendo la fuente de valores— pero deja de viajar al guild como campos que no
//! pidió, y lo que no se puede rellenar se dice en vez de adivinarse.
//!
//! Excepción deliberada y con fecha: una tool que **no declara `properties`** no
//! tiene esquema que honrar; se devuelve el borrador tal cual
//! ([`FinalArgs::Unchecked`]). R3/R4 traerán esquemas a esos guilds.
//!
//! Funciones puras: los tests no necesitan servidor, registro ni guilds.

use serde_json::{Map, Value};

/// Resultado de validar los argumentos contra el esquema de la tool.
pub(super) enum FinalArgs {
    /// La tool declara esquema: solo quedan campos declarados.
    Declared(Value),
    /// La tool no declara `properties`: el borrador pasa tal cual (compatibilidad).
    Unchecked(Value),
}

fn properties(schema: &Value) -> Option<&Map<String, Value>> {
    schema.get("properties").and_then(Value::as_object)
}

fn required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn declared_fields(props: &Map<String, Value>) -> String {
    let mut names: Vec<&str> = props.keys().map(String::as_str).collect();
    names.sort();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// Recorta `draft` a los campos declarados y valida lo requerido.
///
/// - `schema` sin `properties` → [`FinalArgs::Unchecked`] (nada que honrar).
/// - `properties: {}` → argumentos vacíos (la tool no acepta nada; mandarle el
///   borrador sería inventar campos).
/// - `explicit` (lo que el modelo pasó en `args`) con campos **no declarados** →
///   error: eso es el modelo pidiendo algo que la tool no ofrece.
/// - Requerido que no está en el borrador → **error correctivo** con la lista de
///   lo que acepta la tool (es el criterio de R1).
pub(super) fn finalize(
    schema: &Value,
    draft: &Value,
    explicit: Option<&Map<String, Value>>,
) -> Result<FinalArgs, String> {
    let Some(props) = properties(schema) else {
        return Ok(FinalArgs::Unchecked(draft.clone()));
    };

    let fields = declared_fields(props);

    if let Some(explicit) = explicit {
        let unknown: Vec<&str> = explicit
            .keys()
            .filter(|k| !props.contains_key(*k))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "args do not match the tool schema; unknown field(s): {}; declared fields: {fields}",
                unknown.join(", ")
            ));
        }
    }

    if props.is_empty() {
        return Ok(FinalArgs::Declared(Value::Object(Map::new())));
    }

    let mut args = Map::new();
    if let Some(obj) = draft.as_object() {
        for (key, value) in obj {
            if props.contains_key(key) {
                args.insert(key.clone(), value.clone());
            }
        }
    }
    if let Some(explicit) = explicit {
        for (key, value) in explicit {
            args.insert(key.clone(), value.clone());
        }
    }

    // El Vec se ata antes: `required(schema).iter()` devolvía referencias a un
    // temporal (E0716).
    let required_fields = required(schema);
    let missing: Vec<&str> = required_fields
        .iter()
        .filter(|k| !args.contains_key(*k))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "cannot fill the required field(s) {} from the intent: call it again with args={{...}}; declared fields: {fields}",
            missing.join(", ")
        ));
    }

    Ok(FinalArgs::Declared(Value::Object(args)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// El borrador que el router fabrica hoy (los nueve alias + los campos de
    /// ruta). Es lo que llega aquí en producción.
    fn legacy_draft(intent: &str) -> Value {
        json!({
            "command": intent, "intent": intent, "task": intent,
            "query": intent, "text": intent, "content": intent,
            "prompt": intent, "message": intent, "input": intent,
            "server_url": "", "url": "",
            "timeout_secs": 30, "language": "", "depth": 2, "max_results": 50,
        })
    }

    fn declared(outcome: FinalArgs) -> Value {
        match outcome {
            FinalArgs::Declared(v) => v,
            FinalArgs::Unchecked(_) => panic!("esperaba argumentos declarados"),
        }
    }

    fn error(outcome: Result<FinalArgs, String>) -> String {
        match outcome {
            Err(e) => e,
            Ok(_) => panic!("esperaba un error correctivo"),
        }
    }

    /// **Criterio del TL**: con un intent que no permite rellenar lo requerido,
    /// el error dice **qué argumentos espera la tool**.
    /// Mutación: quitar la comprobación de `missing` devuelve `Ok` y este test cae.
    #[test]
    fn r1_intent_ambiguo_devuelve_error_con_los_argumentos_esperados() {
        let schema = json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "root": {"type": "string"},
                "depth": {"type": "integer"}
            },
            "required": ["pattern"]
        });
        // El borrador trae `depth` (lo declara la tool) pero no `pattern`.
        let e = error(finalize(&schema, &legacy_draft("hazlo"), None));
        assert!(e.contains("pattern"), "{e}");
        assert!(e.contains("declared fields"), "{e}");
        assert!(e.contains("args={...}"), "{e}");
    }

    /// Solo sobreviven los campos que la tool declara: el borrador de nueve
    /// alias no viaja al guild.
    #[test]
    fn r1_el_borrador_se_recorta_a_lo_declarado() {
        let schema = json!({
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"]
        });
        let args = declared(finalize(&schema, &legacy_draft("busca el informe"), None).unwrap());
        assert_eq!(args, json!({"query": "busca el informe"}));
    }

    /// Args explícitos con un campo no declarado: error (el modelo pide algo que
    /// la tool no ofrece), nombrando lo que sí acepta.
    #[test]
    fn r1_campo_no_declarado_en_args_es_error() {
        let schema = json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        });
        let mut explicit = Map::new();
        explicit.insert("path".into(), json!("/tmp/x"));
        explicit.insert("recursive".into(), json!(true));
        let e = error(finalize(&schema, &legacy_draft("lista"), Some(&explicit)));
        assert!(e.contains("unknown field(s): recursive"), "{e}");
        assert!(e.contains("declared fields: path"), "{e}");
    }

    /// Los explícitos mandan sobre el borrador y también se recortan.
    #[test]
    fn r1_explicitos_mandan_y_pasan_solo_los_declarados() {
        let schema = json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "depth": {"type": "integer"}
            },
            "required": ["path", "depth"]
        });
        let mut explicit = Map::new();
        explicit.insert("path".into(), json!("/tmp/x"));
        explicit.insert("depth".into(), json!(3));
        let args = declared(finalize(&schema, &legacy_draft("lista"), Some(&explicit)).unwrap());
        assert_eq!(args, json!({"path": "/tmp/x", "depth": 3}));
    }

    /// `properties: {}` = «no acepta argumentos»: se llama vacío, no con el blob.
    #[test]
    fn r1_tool_sin_argumentos_se_llama_vacia() {
        let schema = json!({"type": "object", "properties": {}});
        let args = declared(finalize(&schema, &legacy_draft("hazlo"), None).unwrap());
        assert_eq!(args, json!({}));
    }

    /// Sin esquema no hay nada que honrar: el borrador pasa tal cual y se dice
    /// que no se ha comprobado (compatibilidad con fecha, no un descuido).
    #[test]
    fn r1_sin_esquema_el_borrador_pasa_sin_comprobar() {
        let draft = legacy_draft("lo que sea");
        assert!(matches!(
            finalize(&json!({}), &draft, None),
            Ok(FinalArgs::Unchecked(v)) if v == draft
        ));
        assert!(matches!(
            finalize(&json!({"type": "object"}), &draft, None),
            Ok(FinalArgs::Unchecked(v)) if v == draft
        ));
    }

    /// Un requerido que el borrador sí trae (por las pistas de ruta) no da error.
    #[test]
    fn r1_requerido_cubierto_por_el_borrador_no_da_error() {
        let schema = json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        });
        let mut draft = legacy_draft("lista /tmp/x");
        draft["path"] = json!("/tmp/x");
        let args = declared(finalize(&schema, &draft, None).unwrap());
        assert_eq!(args, json!({"path": "/tmp/x"}));
    }
}
