//! Punto único de limpieza del índice FTS5 de `nodes` (D-23).
//!
//! `nodes_fts` es una tabla FTS5 de **contenido externo** (`content=nodes`,
//! `content_rowid=rowid`) y **sin triggers** (schema v11). Por eso cada sitio
//! que borra de `nodes` tiene que borrar además la fila del índice por
//! `rowid`; los sitios que no lo hacían dejaban docids huérfanos (medido por
//! DSH: 55,3 % del índice apuntaba a nodos ya borrados).
//!
//! Este módulo es el **único** punto de borrado del índice: `delete_nodes_where`
//! para borrados en lote y `fts_delete_rowids` para los que ya tienen su lista
//! de `rowid` dentro de una transacción. `fts_rebuild` existe como comando
//! disponible pero **no se ejecuta en producción**: reconstruir una base grande
//! es una decisión del operador (mueve el índice entero y tarda).

use rusqlite::{Connection, Result, ToSql};

use crate::memory::silva::SilvaDB;

/// Desajuste entre el índice `nodes_fts` y la tabla `nodes`.
///
/// `orphan` = docids indexados que ya no tienen nodo; `missing` = nodos que no
/// están en el índice. Los dos son basura/búsqueda perdida respectivamente.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FtsDesync {
    pub orphan: i64,
    pub missing: i64,
}

impl FtsDesync {
    pub(crate) fn total(&self) -> i64 {
        self.orphan + self.missing
    }
}

/// Borra del índice las filas de esos `rowid` que estén realmente indexadas.
/// Idempotente: repetir la llamada sobre el mismo docid no falla ni deja rastro.
/// Devuelve cuántas entradas se borraron de verdad.
///
/// Dos trampas de FTS5 con `content=` externo que esta función encapsula:
/// 1. `DELETE` sobre un docid que ya no está indexado aborta con error de
///    corrupción, así que primero se comprueba contra `nodes_fts_docsize`.
/// 2. `changes()` no informa de estas escrituras, por eso el recuento se lleva
///    a mano.
///
/// Debe llamarse **antes** de borrar los nodos si se hace en dos pasos (FTS5
/// necesita leer el contenido para desindexar), o dentro de la misma transacción.
pub(crate) fn fts_delete_rowids(conn: &Connection, rowids: &[i64]) -> Result<usize> {
    if rowids.is_empty() {
        return Ok(0);
    }
    let mut present = conn.prepare_cached("SELECT 1 FROM nodes_fts_docsize WHERE id = ?1")?;
    let mut delete = conn.prepare_cached("DELETE FROM nodes_fts WHERE rowid = ?1")?;
    let mut deleted = 0;
    for rowid in rowids {
        if present.exists(rusqlite::params![rowid])? {
            delete.execute(rusqlite::params![rowid])?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// Borra de `nodes` las filas que cumplen `where_sql` **y** sus entradas del
/// índice FTS. Devuelve cuántos nodos se borraron.
///
/// `where_sql` es SQL de confianza: se compone aquí desde constantes del crate,
/// nunca desde entrada de usuario; los valores van siempre por `params`.
pub(crate) fn delete_nodes_where(
    conn: &Connection,
    where_sql: &str,
    params: &[&dyn ToSql],
) -> Result<usize> {
    let rowids: Vec<i64> = {
        let mut stmt = conn.prepare(&format!("SELECT rowid FROM nodes WHERE {where_sql}"))?;
        let rows = stmt.query_map(params, |r| r.get::<_, i64>(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    fts_delete_rowids(conn, &rowids)?;
    conn.execute(&format!("DELETE FROM nodes WHERE {where_sql}"), params)
}

/// Vacía el índice entero (para un `DELETE FROM nodes` masivo). `delete-all` es
/// el comando canónico de FTS5 para tablas de contenido externo.
pub(crate) fn fts_clear_all(conn: &Connection) -> Result<()> {
    conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('delete-all')", [])?;
    Ok(())
}

/// `orphan`/`missing` contados contra `nodes_fts_docsize`, que tiene una fila
/// por docid **realmente indexado** (a diferencia de `SELECT COUNT(*) FROM
/// nodes_fts`, que en una tabla de contenido externo lee la tabla `nodes` y por
/// eso no podía detectar el desajuste que reportó DSH).
pub(crate) fn fts_desync_counts(conn: &Connection) -> Result<FtsDesync> {
    let orphan = conn.query_row(
        "SELECT COUNT(*) FROM nodes_fts_docsize d
         WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.rowid = d.id)",
        [],
        |r| r.get(0),
    )?;
    let missing = conn.query_row(
        "SELECT COUNT(*) FROM nodes n
         WHERE NOT EXISTS (SELECT 1 FROM nodes_fts_docsize d WHERE d.id = n.rowid)",
        [],
        |r| r.get(0),
    )?;
    Ok(FtsDesync { orphan, missing })
}

/// Reconstruye el índice completo desde `nodes` (comando `rebuild` de FTS5).
///
/// **No se llama en ningún camino de producción**: deja el índice correcto pero
/// reescribe la tabla entera, y hacerlo sobre la base de un operador es una
/// decisión suya, no un efecto de arrancar el kernel.
// Intencionadamente sin cablear a ningún arranque ni endpoint (D-23): el
// operador decide cuándo reconstruir su índice. Se mantiene compilado y
// probado para que el comando exista de verdad, no solo en documentación.
#[allow(dead_code)]
pub(crate) fn fts_rebuild(conn: &Connection) -> Result<()> {
    conn.execute("INSERT INTO nodes_fts(nodes_fts) VALUES('rebuild')", [])?;
    Ok(())
}

impl SilvaDB {
    /// Desajuste actual entre `nodes_fts` y `nodes`. Solo lectura.
    pub(crate) async fn fts_desync(&self) -> Result<FtsDesync> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            fts_desync_counts(&conn)
        })
    }

    /// Comando de reconstrucción del índice FTS. **No está cableado a ningún
    /// endpoint ni arranque**: existe para que el operador lo invoque de forma
    /// explícita (tests, o una decisión manual sobre su base).
    #[allow(dead_code)]
    pub(crate) async fn rebuild_fts_index(&self) -> Result<()> {
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();
            fts_rebuild(&conn)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (
                 id TEXT PRIMARY KEY,
                 type TEXT NOT NULL DEFAULT 'episode',
                 content TEXT NOT NULL DEFAULT '',
                 metadata TEXT DEFAULT '{}'
             );
             CREATE VIRTUAL TABLE nodes_fts USING fts5(
                 id UNINDEXED,
                 content,
                 metadata,
                 content=nodes,
                 content_rowid=rowid,
                 tokenize='porter unicode61'
             );",
        )
        .unwrap();
        conn
    }

    fn add_node(conn: &Connection, id: &str, content: &str) {
        conn.execute(
            "INSERT INTO nodes (id, content) VALUES (?1, ?2)",
            rusqlite::params![id, content],
        )
        .unwrap();
        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM nodes WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO nodes_fts(rowid, id, content, metadata) VALUES (?1, ?2, ?3, '{}')",
            rusqlite::params![rowid, id, content],
        )
        .unwrap();
    }

    #[test]
    fn desync_counts_detects_orphan_and_missing() {
        let conn = db();
        add_node(&conn, "a", "alpha memoria");
        add_node(&conn, "b", "beta memoria");
        assert_eq!(fts_desync_counts(&conn).unwrap(), FtsDesync::default());

        // Borrado "a lo bruto": nodo fuera, índice intacto → huérfano.
        conn.execute("DELETE FROM nodes WHERE id = 'a'", []).unwrap();
        assert_eq!(
            fts_desync_counts(&conn).unwrap(),
            FtsDesync { orphan: 1, missing: 0 }
        );

        // Nodo nuevo sin indexar → missing.
        conn.execute(
            "INSERT INTO nodes (id, content) VALUES ('c', 'gamma')",
            [],
        )
        .unwrap();
        assert_eq!(
            fts_desync_counts(&conn).unwrap(),
            FtsDesync { orphan: 1, missing: 1 }
        );
        assert_eq!(fts_desync_counts(&conn).unwrap().total(), 2);
    }

    #[test]
    fn delete_nodes_where_leaves_zero_desync() {
        let conn = db();
        add_node(&conn, "keep", "se queda");
        add_node(&conn, "drop1", "basura uno");
        add_node(&conn, "drop2", "basura dos");

        let deleted = delete_nodes_where(&conn, "content LIKE ?1", &[&"%basura%"]).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(fts_desync_counts(&conn).unwrap(), FtsDesync::default());

        // Y el índice ya no devuelve los borrados (sin resultados fantasma).
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'basura'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 0);

        // Idempotente: borrar otra vez no toca nada ni desajusta.
        let again = delete_nodes_where(&conn, "content LIKE ?1", &[&"%basura%"]).unwrap();
        assert_eq!(again, 0);
        assert_eq!(fts_desync_counts(&conn).unwrap(), FtsDesync::default());
    }

    #[test]
    fn fts_delete_rowids_is_idempotent() {
        let conn = db();
        add_node(&conn, "a", "alpha");
        let rowid: i64 = conn
            .query_row("SELECT rowid FROM nodes WHERE id = 'a'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_delete_rowids(&conn, &[rowid]).unwrap(), 1);
        // El nodo sigue en `nodes`, así que lo que queda es "missing", no "orphan".
        assert_eq!(
            fts_desync_counts(&conn).unwrap(),
            FtsDesync { orphan: 0, missing: 1 }
        );
        // Segunda pasada sobre el mismo docid: 0 borrados, sin error (un docid
        // ausente del índice haría abortar a FTS5 si se intentara el DELETE).
        assert_eq!(fts_delete_rowids(&conn, &[rowid]).unwrap(), 0);
        assert_eq!(
            fts_desync_counts(&conn).unwrap(),
            FtsDesync { orphan: 0, missing: 1 }
        );
        assert_eq!(fts_delete_rowids(&conn, &[]).unwrap(), 0);
    }

    #[test]
    fn fts_clear_all_empties_the_index_not_the_content() {
        let conn = db();
        add_node(&conn, "a", "alpha");
        add_node(&conn, "b", "beta");
        fts_clear_all(&conn).unwrap();
        let nodes: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(nodes, 2, "delete-all no debe tocar la tabla de contenido");
        assert_eq!(
            fts_desync_counts(&conn).unwrap(),
            FtsDesync { orphan: 0, missing: 2 }
        );
    }

    #[test]
    fn fts_rebuild_repairs_both_directions() {
        let conn = db();
        add_node(&conn, "a", "alpha");
        add_node(&conn, "b", "beta");
        conn.execute("DELETE FROM nodes WHERE id = 'a'", []).unwrap();
        conn.execute("INSERT INTO nodes (id, content) VALUES ('c', 'gamma')", [])
            .unwrap();
        assert_ne!(fts_desync_counts(&conn).unwrap(), FtsDesync::default());

        fts_rebuild(&conn).unwrap();
        assert_eq!(fts_desync_counts(&conn).unwrap(), FtsDesync::default());
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'gamma'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "el nodo que faltaba queda buscable tras rebuild");
    }
}

/// D-23: cada camino de borrado que existe hoy en producción debe dejar el
/// índice al día. No se prueba el SQL en abstracto: se invocan los métodos
/// reales sobre una `SilvaDB::in_memory()`.
#[cfg(test)]
mod desync_paths {
    use super::*;
    use crate::memory::silva::SilvaDB;

    async fn db_with(nodes: &[(&str, &str)]) -> SilvaDB {
        let db = SilvaDB::in_memory().await.unwrap();
        for (id, content) in nodes {
            db.upsert_node(id, "memory", content, "{}").await.unwrap();
        }
        db
    }

    async fn assert_synced(db: &SilvaDB, ctx: &str) {
        let d = db.fts_desync().await.unwrap();
        assert_eq!(d, FtsDesync::default(), "desync tras {ctx}: {d:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn delete_node_leaves_zero_desync() {
        let db = db_with(&[("keep", "se queda"), ("gone", "se va")]).await;
        assert!(db.delete_node("gone").await.unwrap());
        assert_synced(&db, "delete_node").await;
        assert!(!db.delete_node("gone").await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prune_dead_nodes_and_apply_cleanup_leave_zero_desync() {
        let db = db_with(&[("a", "alpha"), ("b", "beta"), ("c", "gamma")]).await;
        let pruned = db.prune_dead_nodes(10.0).await.unwrap();
        assert_eq!(pruned, 3);
        assert_synced(&db, "prune_dead_nodes").await;

        let db = db_with(&[("d", "delta"), ("e", "epsilon")]).await;
        let cleaned = db.apply_cleanup(10.0).await.unwrap();
        assert_eq!(cleaned, 2);
        assert_synced(&db, "apply_cleanup").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn purge_deprecated_lessons_leaves_zero_desync() {
        let db = SilvaDB::in_memory().await.unwrap();
        for id in ["lesson:intent:vieja", "lesson:intent:otra"] {
            db.upsert_node(id, "lesson", "leccion", "{}").await.unwrap();
        }
        db.upsert_node("memoria", "memory", "no es leccion", "{}").await.unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET weight = 0.1 WHERE id LIKE 'lesson:intent:%'",
                [],
            )
            .unwrap();
        }
        let purged = db.purge_deprecated_lessons().await.unwrap();
        assert_eq!(purged, 2);
        assert_synced(&db, "purge_deprecated_lessons").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cleanup_orphan_nodes_leaves_zero_desync() {
        let db = db_with(&[("suelto", "sin aristas")]).await;
        let removed = db.cleanup_orphan_nodes().await.unwrap();
        assert_eq!(removed, 1);
        assert_synced(&db, "cleanup_orphan_nodes").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn legacy_salience_and_cold_pruning_leave_zero_desync() {
        let db = db_with(&[("a", "alpha"), ("b", "beta")]).await;
        db.prune_by_salience(10.0).await.unwrap();
        assert_synced(&db, "prune_by_salience").await;

        let db = db_with(&[("c", "gamma"), ("d", "delta")]).await;
        db.prune_cold_nodes(10.0).await.unwrap();
        assert_synced(&db, "prune_cold_nodes").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn merge_node_into_leaves_zero_desync() {
        let db = db_with(&[("destino", "destino"), ("origen", "origen fusionado")]).await;
        db.merge_node_into("origen", "destino").await.unwrap();
        assert_synced(&db, "merge_node_into").await;
        let hits: i64 = {
            let conn = db.conn.lock().await;
            conn.query_row(
                "SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'fusionado'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits, 0, "el nodo fusionado no debe seguir siendo buscable");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forget_agent_leaves_zero_desync() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("mio", "memory", "de deep", r#"{"agent":"deep"}"#)
            .await
            .unwrap();
        db.upsert_node("ajeno", "memory", "de otro", r#"{"agent":"otro"}"#)
            .await
            .unwrap();
        let deleted = db.forget_agent("deep").await.unwrap();
        assert_eq!(deleted, 1);
        assert_synced(&db, "forget_agent").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prune_superseded_leaves_zero_desync() {
        let db = SilvaDB::in_memory().await.unwrap();
        let opts = || crate::memory::silva::NodeWriteOptions::new("test").drift_allowed(true);
        db.upsert_node_with_validity(
            "viejo",
            "agent_summary",
            "resumen viejo",
            r#"{"superseded_by":"nuevo"}"#,
            opts(),
        )
        .await
        .unwrap();
        db.upsert_node_with_validity("nuevo", "agent_summary", "resumen nuevo", "{}", opts())
            .await
            .unwrap();
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE nodes SET updated_at = datetime('now', '-10 days') WHERE id = 'viejo'",
                [],
            )
            .unwrap();
        }
        let pruned = db.prune_superseded("agent_summary", 1).await.unwrap();
        assert_eq!(pruned, 1);
        assert_synced(&db, "prune_superseded").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn legacy_nested_graphrag_purge_leaves_zero_desync() {
        let db = db_with(&[
            ("graphrag_summary:cluster:graphrag_summary:cluster:x", "corrupto"),
            ("graphrag_summary:cluster:ok", "valido"),
        ])
        .await;
        let (nodes_deleted, _, _) = db.purge_legacy_nested_graphrag_summaries().await.unwrap();
        assert_eq!(nodes_deleted, 1);
        assert_synced(&db, "purge_legacy_nested_graphrag_summaries").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fsrs_decay_pruning_leaves_zero_desync() {
        let db = db_with(&[("viejo", "memoria vieja"), ("nuevo", "memoria nueva")]).await;
        {
            let conn = db.conn.lock().await;
            // `apply_decay` recalcula el peso desde FSRS antes de podar; con
            // `fsrs_last_review = 0` la retrievabilidad es 1.0 y el peso vuelve
            // a 1.0, así que la reseña tiene que ser antigua para que el nodo
            // siga por debajo del umbral.
            conn.execute(
                "UPDATE nodes SET weight = 0.05, fsrs_stability = 1.0,
                    fsrs_last_review = CAST(strftime('%s','now') AS INTEGER) - 7776000,
                    updated_at = datetime('now', '-90 days') WHERE id = 'viejo'",
                [],
            )
            .unwrap();
        }
        db.apply_decay().await.unwrap();
        let left: i64 = {
            let conn = db.conn.lock().await;
            conn.query_row("SELECT COUNT(*) FROM nodes WHERE id = 'viejo'", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(left, 0, "apply_decay debía podar el nodo viejo");
        assert_synced(&db, "apply_decay").await;
    }

    /// Guarda anti-deriva: si alguien añade un `DELETE FROM nodes` nuevo, este
    /// test falla y le obliga a pasar por `delete_nodes_where`/`fts_clear_all`.
    #[test]
    fn raw_node_deletes_are_only_where_fts_is_cleaned_inline() {
        let decay = include_str!("decay.rs");
        let nodes = include_str!("nodes.rs");
        let maint = include_str!("maintenance.rs");
        let admin = include_str!("../../transport/http/api_v1/api_admin.rs");

        assert_eq!(
            maint.matches("DELETE FROM nodes").count(),
            0,
            "maintenance.rs debe borrar solo vía fts::delete_nodes_where"
        );

        assert_eq!(decay.matches("DELETE FROM nodes").count(), 2);
        assert_eq!(
            decay.matches("fts_delete_rowids").count(),
            2,
            "los dos borrados por id de decay.rs deben limpiar el índice"
        );

        assert_eq!(nodes.matches("DELETE FROM nodes").count(), 3);
        assert_eq!(
            nodes.matches("fts_delete_rowids").count(),
            3,
            "delete_node, merge_node_into y el purge GraphRAG deben limpiar el índice"
        );

        assert!(
            admin.contains("fts_clear_all"),
            "el purge masivo de api_admin debe vaciar el índice FTS"
        );
    }
}
