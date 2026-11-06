//! D-107 — redacción de previews de auditoría (diseño aprobado en TL T2689).
//!
//! Una sola puerta: `log_audit_entry_at` (audit.rs) y `capture_friction_from_result`
//! (outcome.rs) pasan el texto por [`scrub_result_preview`] antes de tocar disco.
//! Dos capas: los patrones conocidos de `logging::scrub` y las cadenas registradas como
//! secretas (valores de `headers`/`env` de servidores MCP externos, con o sin `vault:`,
//! registrados al resolverlos en el arranque y en el alta en caliente).
//!
//! Diseño: `docs/internal/IMPL_D-107_choke_point_result_preview_2026-11-04.md`.

use std::collections::BTreeSet;
use std::sync::{OnceLock, RwLock};

/// Longitud mínima para registrar una cadena como secreta: evita que valores como
/// `on`, `true` o `cpu` destrocen previews legítimos.
const MIN_SECRET_LEN: usize = 8;

/// Longitud del preview de auditoría (igual que los llamadores actuales).
pub const PREVIEW_CHARS: usize = 200;

static REGISTERED: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();

fn registry() -> &'static RwLock<BTreeSet<String>> {
    REGISTERED.get_or_init(|| RwLock::new(BTreeSet::new()))
}

/// Registra un valor que nunca debe aparecer en un preview. Idempotente.
/// Se llama desde la resolución de `vault:` y desde los `headers`/`env` en claro
/// (TL T2689: dentro de `resolve_vault_map`).
pub fn register_secret_value(value: &str) {
    if value.chars().count() < MIN_SECRET_LEN {
        return;
    }
    // D-111 (H26 de Buffy): si el lock se envenena, NO se descarta el registro —
    // un secreto no registrado es un secreto que sale sin redactar.
    let mut set = match registry().write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    set.insert(value.to_string());
}

/// Cuántas cadenas hay registradas (diagnóstico y tests).
pub fn registered_secret_count() -> usize {
    registry().read().map(|s| s.len()).unwrap_or(0)
}

/// Sustituye por `***` cualquier cadena registrada que aparezca en `text`.
pub fn scrub_registered(text: &str) -> String {
    // D-111 (H26 de Buffy): un guardián de seguridad no puede fallar ABIERTO. Si el
    // RwLock se envenenó (pánico mientras alguien tenía el write), recuperamos el
    // registro con `into_inner()` y redactamos igual. Devolver el texto intacto era
    // la peor elección posible: el preview salía sin limpiar y nadie se enteraba.
    let set = match registry().read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut out = text.to_string();
    // De más larga a más corta: si una contiene a otra, la larga debe ganar.
    let mut values: Vec<&String> = set.iter().collect();
    values.sort_by_key(|v| std::cmp::Reverse(v.chars().count()));
    for value in values {
        if out.contains(value.as_str()) {
            out = out.replace(value.as_str(), "***");
        }
    }
    out
}

/// Punto único de redacción de un preview de auditoría.
///
/// Orden: patrones conocidos (`logging::scrub`) → cadenas registradas → recorte.
///
pub fn scrub_result_preview(text: &str) -> String {
    let known = crate::logging::scrub(text);
    let masked = scrub_registered(&known);
    masked.chars().take(PREVIEW_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D-107 (1): un token registrado como secreto no puede salir en el preview.
    #[test]
    fn d107_token_registrado_se_enmascara() {
        // Valor SIN patrón conocido a propósito: así se prueba la capa 2 (registro)
        // y no la capa 1 (`logging::scrub`), que ya tiene sus propios tests.
        register_secret_value("SecretoSinPatronD107");
        let out = scrub_result_preview("resultado: ok valor SecretoSinPatronD107 fin");
        assert!(!out.contains("SecretoSinPatronD107"), "no debe quedar el valor: {out}");
        assert!(out.contains("***"), "debe quedar la marca de la capa 2: {out}");
    }

    /// D-111 (H26 de Buffy): si el registro se ENVENENA (pánico mientras alguien
    /// tenía el write), la redacción NO puede fallar abierta. Antes devolvía el
    /// texto intacto; este test se pone rojo con ese comportamiento.
    #[test]
    fn d111_lock_envenenado_sigue_redactando() {
        const SECRETO: &str = "SecretoPoisonD111";
        register_secret_value(SECRETO);

        // Envenenar a propósito: el hilo toma el write y panica dentro.
        let h = std::thread::spawn(|| {
            let _guard = registry().write().expect("write");
            panic!("envenenando el registro a proposito");
        });
        assert!(h.join().is_err(), "el hilo debe haber paniqueado");

        // El lock queda envenenado para el resto del proceso; el registro sigue ahí.
        let out = scrub_result_preview(&format!("salida: ok valor {SECRETO} fin"));
        assert!(
            !out.contains(SECRETO),
            "fail-open con el lock envenenado: el secreto salió intacto: {out}"
        );
    }

    /// D-107 (2): los patrones que ya conoce el proyecto siguen aplicándose.
    #[test]
    fn d107_patron_conocido_se_scrubbea() {
        let out = scrub_result_preview("Authorization: Bearer abcdef123456");
        assert!(
            !out.contains("abcdef123456"),
            "el patrón Bearer debe redactarse: {out}"
        );
    }

    /// D-107 (3): un preview limpio no se toca (no romper lo útil).
    #[test]
    fn d107_preview_limpio_sale_identico() {
        let clean = "commit 3f2a1b aplicado; 4 ficheros, sin conflictos";
        assert_eq!(scrub_result_preview(clean), clean);
    }

    /// D-107 (4): recorte a 200 caracteres DESPUÉS de limpiar.
    #[test]
    fn d107_recorta_a_200_despues_de_limpiar() {
        register_secret_value("SecretoAlFinalD107");
        let mut text = "x".repeat(190);
        text.push_str("SecretoAlFinalD107");
        text.push_str(&"y".repeat(50));
        let out = scrub_result_preview(&text);
        assert!(out.chars().count() <= PREVIEW_CHARS, "largo {}", out.chars().count());
        assert!(!out.contains("SecretoAlFinalD107"), "el secreto está en el tramo recortado: {out}");
    }

    /// D-107 (5): los valores cortos no se registran (no destrozan texto normal).
    #[test]
    fn d107_valores_cortos_no_se_registran() {
        let before = registered_secret_count();
        register_secret_value("on");
        register_secret_value("cpu");
        assert_eq!(registered_secret_count(), before, "no debe crecer con valores cortos");
    }
}
