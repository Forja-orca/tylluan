// ─── H-20261121-COLOQUIO-POST: el informe no puede decir «passed» si no pasó nada ──
// Nota: este módulo ya está declarado como `#[cfg(test)] mod action_report_tests;`
// en `handler_do/mod.rs`, así que aquí NO va otro `mod` envolvente.
use crate::transport::server::handler_do::error_prefixes::{action_performed_ok, looks_like_failure};
use crate::transport::server::handler_do::build_action_report;

/// Textos REALES que devuelven las tools del Coloquio/`filesystem` cuando no
/// hacen lo pedido. Medidos el 2026-11-22 contra `guilds/core/coloquio.py`
/// (`❌ Specify channel and content…`) y `file_read` (`🚫 BLOCKED:`).
#[test]
fn los_fallos_de_gremio_se_reconocen_en_el_texto() {
    for texto in [
        "❌ Specify channel and content: 'post to coloquio <channel>: <message>'",
        "🚫 BLOCKED: Path '/x' is outside the allowed root '/y'.",
        "ACCESS_DENIED: agent 'x' is not allowed to use tool 'tylluan_do'",
        "GUILD_ERROR|coloquio|boom",
        "GUILD_TIMEOUT|coloquio|30s",
        "RATE_LIMITED: coloquio",
        "MISSING_ARGS: falta content",
        "   ❌ con espacios delante",
    ] {
        assert!(looks_like_failure(texto), "tendría que ser fallo: {texto:?}");
        assert!(
            !action_performed_ok(None, texto),
            "el informe no puede dar por hecha esta acción: {texto:?}"
        );
    }
}

/// Un éxito no se confunde, **ni siquiera citando un ❌ dentro del cuerpo**.
#[test]
fn el_exito_no_se_confunde_y_un_emoji_citado_no_cuenta() {
    for texto in [
        "✅ Posted to 'general' as @deep — turn 4984 (msg_id: 1e769fd9…)",
        "Recordado: nodo 42",
        "",
        // el cliente recita un fallo ajeno: no es nuestro fallo
        "Transcripción del aviso: «❌ Specify channel and content» (otro agente)",
    ] {
        assert!(!looks_like_failure(texto), "no es un fallo: {texto:?}");
        assert!(action_performed_ok(None, texto), "es un éxito: {texto:?}");
    }
}

/// El fallo de transporte manda: `is_error = true` es fallo aunque el texto
/// parezca un éxito (nunca debe poder «blanquearse» con un ✅ en el cuerpo).
#[test]
fn el_fallo_de_transporte_manda_sobre_el_texto() {
    assert!(!action_performed_ok(Some(true), "✅ todo bien según el texto"));
    assert!(action_performed_ok(Some(false), "✅ Posted to 'general'"));
}

/// El informe COMPLETO: es el asiento que leía el agente y decía «passed» con un
/// fallo delante. MUTACION: si `build_action_report` vuelve a mirar solo
/// `is_error`, los dos casos de fallo dicen "ok"/"passed" y esto se pone rojo.
#[test]
fn el_informe_dice_que_fallo_cuando_el_gremio_contesto_un_fallo() {
    for texto in [
        "❌ Specify channel and content: 'post to coloquio <channel>: <message>'",
        "🚫 BLOCKED: Path '/x' is outside the allowed root '/y'.",
    ] {
        let (report, performed_ok) = build_action_report(
            "publica en coloquio general: hola", "coloquio", "post_to_channel", None, texto, Some("turno en el canal"),
        );
        assert!(!performed_ok, "no se hizo: {texto:?}");
        assert_eq!(report["performed"], "guild='coloquio' tool='post_to_channel' (error)");
        assert_eq!(report["verified"]["status"], "failed");
        assert_eq!(report["verified"]["method"], "turno en el canal");
        assert!(
            report["changed"].as_str().unwrap_or_default().starts_with("call failed:"),
            "changed tiene que decir que falló: {}",
            report["changed"]
        );
    }
}

/// Y el éxito se informa como éxito (control: no convertir todo en fallo).
#[test]
fn el_informe_dice_que_paso_cuando_el_gremio_publico() {
    let (report, performed_ok) = build_action_report(
        "publica en coloquio general: hola", "coloquio", "post_to_channel", None,
        "✅ Posted to 'general' as @deep — turn 4997", Some("turno en el canal"),
    );
    assert!(performed_ok);
    assert_eq!(report["performed"], "guild='coloquio' tool='post_to_channel' (ok)");
    assert_eq!(report["verified"]["status"], "passed");
    assert!(!report["changed"].as_str().unwrap_or_default().starts_with("call failed:"));
}

