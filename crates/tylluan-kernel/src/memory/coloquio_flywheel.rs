//! T-DSH-166 (F1, segundo escalon): flywheel Coloquio -> SilvaDB.
//!
//! Arregla tres defectos del bucle que vivia en `main.rs`:
//!
//! 1. La marca de agua estaba SOLO en memoria (`HashMap`): al reiniciar se
//!    re-leian turnos ya vistos. Ahora vive en `silva_kv` por canal
//!    ([`watermark_key`]).
//! 2. La consulta usaba `OFFSET = ultimo turno`, que cuenta FILAS, no turnos.
//!    Ahora usa `turn > marca` ([`ColoquioDb::get_messages_since`]).
//! 3. Cada upsert reseteaba el peso de un nodo ya decaido a 1.0
//!    (`MAX(nodes.weight, excluded.weight)`). Ahora se escribe con
//!    `NodeWriteOptions::preserve_weight(true)`.
//!
//! El canal puede tener espacios en el id (p. ej. "principios y recuerdos"):
//! es una clave de texto en SQLite y no se interpola en ningun identificador.

use anyhow::Result;

use crate::memory::coloquio::ColoquioDb;
use crate::memory::silva::nodes::{build_contextual_text, NodeWriteOptions};
use crate::memory::silva::SilvaDB;

/// Prefijo de la clave en `silva_kv`. Lleva el id del canal tal cual (espacios
/// incluidos) y `turn:` para versionar el formato si hiciera falta.
pub const WATERMARK_KEY_PREFIX: &str = "coloquio_flywheel_watermark:";

/// Cuantos mensajes nuevos se ingieren por canal y pasada.
pub const INGEST_BATCH: usize = 50;

pub fn watermark_key(channel_id: &str) -> String {
    format!("{WATERMARK_KEY_PREFIX}{channel_id}")
}

/// Lee la marca de agua persistida (0 si el canal nunca se ingirio).
pub async fn load_watermark(silva: &SilvaDB, channel_id: &str) -> Result<i64> {
    Ok(silva
        .kv_get(&watermark_key(channel_id))
        .await?
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .unwrap_or(0))
}

/// Persiste la marca de agua del canal.
pub async fn store_watermark(silva: &SilvaDB, channel_id: &str, turn: i64) -> Result<()> {
    silva
        .kv_set(&watermark_key(channel_id), &turn.to_string())
        .await
}

/// Ingiene en SilvaDB los turnos del canal posteriores a la marca persistida.
/// Devuelve cuantos nodos se escribieron (los ya vistos no se reescriben).
pub async fn ingest_channel(
    coloquio: &ColoquioDb,
    silva: &SilvaDB,
    channel_id: &str,
) -> Result<usize> {
    let watermark = load_watermark(silva, channel_id).await?;
    let messages = coloquio
        .get_messages_since(channel_id, watermark, INGEST_BATCH)
        .await?;
    let mut ingested = 0usize;
    let mut max_turn = watermark;
    for msg in &messages {
        if msg.turn <= watermark {
            continue;
        }
        let node_id = format!("coloquio_{}_{}", channel_id, msg.turn);
        let metadata = serde_json::json!({
            "channel_id": channel_id,
            "turn": msg.turn,
            "author_id": msg.author_id,
            "source": "coloquio_flywheel",
        });
        let metadata = metadata.to_string();
        let content = build_contextual_text(&metadata, &msg.content);
        // preserve_weight: releer un turno ya decaido no lo devuelve a 1.0.
        silva
            .upsert_node_with_validity(
                &node_id,
                "coloquio_memory",
                &content,
                &metadata,
                NodeWriteOptions::new("guild_output").preserve_weight(true),
            )
            .await?;
        ingested += 1;
        max_turn = max_turn.max(msg.turn);
    }
    if max_turn > watermark {
        store_watermark(silva, channel_id, max_turn).await?;
    }
    Ok(ingested)
}

/// Recorre todos los canales del Coloquio y devuelve el total ingerido.
pub async fn ingest_all(coloquio: &ColoquioDb, silva: &SilvaDB) -> Result<usize> {
    let channels = coloquio.list_channels().await?;
    let mut total = 0usize;
    for ch in &channels {
        match ingest_channel(coloquio, silva, &ch.channel_id).await {
            Ok(n) => total += n,
            Err(e) => tracing::warn!(
                "Coloquio flywheel: ingest_channel({}) failed: {}",
                ch.channel_id,
                e
            ),
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (ColoquioDb, SilvaDB) {
        let coloquio = ColoquioDb::in_memory().expect("coloquio in_memory");
        coloquio
            .create_channel("general", "General")
            .await
            .expect("create_channel");
        let silva = SilvaDB::in_memory().await.expect("silva in_memory");
        (coloquio, silva)
    }

    async fn node_weight(silva: &SilvaDB, id: &str) -> f64 {
        let conn = silva.conn_lock();
        let conn = conn.lock().await;
        conn.query_row(
            "SELECT weight FROM nodes WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .expect("nodo del flywheel")
    }

    async fn set_node_weight(silva: &SilvaDB, id: &str, weight: f64) {
        let conn = silva.conn_lock();
        let conn = conn.lock().await;
        conn.execute(
            "UPDATE nodes SET weight = ?1 WHERE id = ?2",
            rusqlite::params![weight, id],
        )
        .expect("decay manual");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn restart_does_not_reingest_seen_turns() {
        let (coloquio, silva) = fixture().await;
        for i in 0..2 {
            coloquio
                .post_message("general", "alice", "agent", &format!("mensaje {i}"), "{}")
                .await
                .expect("post");
        }

        let first = ingest_channel(&coloquio, &silva, "general")
            .await
            .expect("primera ingesta");
        assert_eq!(first, 2, "debe ingerir los 2 turnos nuevos");
        assert_eq!(load_watermark(&silva, "general").await.unwrap(), 2);

        // "Reinicio": el estado en memoria se pierde, pero la marca vive en
        // silva_kv. Otra pasada no debe re-escribir nada.
        let second = ingest_channel(&coloquio, &silva, "general")
            .await
            .expect("segunda ingesta");
        assert_eq!(second, 0, "un reinicio no puede re-ingerir turnos ya vistos");
        assert_eq!(
            load_watermark(&silva, "general").await.unwrap(),
            2,
            "la marca no cambia si no hay turnos nuevos"
        );

        let count: i64 = {
            let conn = silva.conn_lock();
            let conn = conn.lock().await;
            conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count, 2, "ni nodos duplicados ni nuevos");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rereading_a_turn_does_not_reset_decayed_weight() {
        let (coloquio, silva) = fixture().await;
        coloquio
            .post_message("general", "alice", "agent", "mensaje viejo", "{}")
            .await
            .expect("post");
        ingest_channel(&coloquio, &silva, "general")
            .await
            .expect("ingesta inicial");

        let node_id = "coloquio_general_1";
        assert!((node_weight(&silva, node_id).await - 1.0).abs() < f64::EPSILON);

        set_node_weight(&silva, node_id, 0.1).await;
        // Simula una marca vieja (un reinicio con checkpoint perdido / rebobinado
        // manual): se vuelve a leer el mismo turno.
        store_watermark(&silva, "general", 0).await.unwrap();
        let again = ingest_channel(&coloquio, &silva, "general")
            .await
            .expect("relectura");
        assert_eq!(again, 1, "la relectura re-procesa el turno");
        assert!(
            (node_weight(&silva, node_id).await - 0.1).abs() < 1e-9,
            "releer no puede devolver el peso decaido a 1.0"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn channel_id_with_spaces_works() {
        let coloquio = ColoquioDb::in_memory().expect("coloquio in_memory");
        let channel = "principios y recuerdos";
        coloquio
            .create_channel(channel, "Principios y recuerdos")
            .await
            .expect("create_channel");
        coloquio
            .post_message(channel, "jose", "human", "hola desde un canal con espacios", "{}")
            .await
            .expect("post");
        let silva = SilvaDB::in_memory().await.expect("silva in_memory");

        let ingested = ingest_channel(&coloquio, &silva, channel)
            .await
            .expect("ingesta con espacios");
        assert_eq!(ingested, 1);
        assert_eq!(load_watermark(&silva, channel).await.unwrap(), 1, "clave kv con espacios");
        assert_eq!(
            watermark_key(channel),
            "coloquio_flywheel_watermark:principios y recuerdos"
        );

        let conn = silva.conn_lock();
        let conn = conn.lock().await;
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1",
                rusqlite::params![format!("coloquio_{channel}_1")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "el nodo del canal con espacios existe");
    }
}
