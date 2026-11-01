//! T-DSH-166: acceso generico al buzon clave/valor `silva_kv`.
//!
//! El flywheel de Coloquio necesita una marca de agua PERSISTENTE por canal
//! (no en memoria): se guarda aqui, en la misma SilvaDB, sin tabla nueva.
use anyhow::Result;

impl super::SilvaDB {
    /// Lee un valor de `silva_kv`; `None` si la clave no existe.
    pub async fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn_lock();
        let conn = conn.lock().await;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM silva_kv WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .ok();
        Ok(value)
    }

    /// Escribe (o reemplaza) un valor en `silva_kv`.
    pub async fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn_lock();
        let conn = conn.lock().await;
        conn.execute(
            "INSERT INTO silva_kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
