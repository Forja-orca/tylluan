//! T-DSH-171 — contrato de `tylluan-cli repair-fts`.
//!
//! Ejecuta el binario real sobre bases SQLite sinteticas con el mismo esquema
//! FTS5 de contenido externo que SilvaDB. Nunca toca produccion.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::Connection;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tylluan-cli")
}

/// Crea una "silva.db" sintetica con `nodes` + `nodes_fts` y un desajuste:
/// un huerfano (nodo borrado a mano) y un nodo sin indexar.
fn make_silva_db(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE nodes (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL DEFAULT '',
            metadata TEXT NOT NULL DEFAULT '{}'
        );
        CREATE VIRTUAL TABLE nodes_fts USING fts5(
            id UNINDEXED,
            content,
            metadata,
            content=nodes,
            content_rowid=rowid
        );",
    )
    .unwrap();
    for id in ["a", "b", "c"] {
        conn.execute(
            "INSERT INTO nodes (id, content, metadata) VALUES (?1, ?2, '{}')",
            rusqlite::params![id, format!("contenido de {id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO nodes_fts(rowid, id, content, metadata)
         SELECT rowid, id, content, metadata FROM nodes",
        [],
    )
    .unwrap();
    // Huerfano: borrar el nodo 'b' sin desindexar (la ruta infractora real).
    conn.execute("DELETE FROM nodes WHERE id = 'b'", []).unwrap();
    // Sin indexar: insertar 'd' sin tocar el FTS.
    conn.execute("INSERT INTO nodes (id, content, metadata) VALUES ('d', 'contenido de d', '{}')", [])
        .unwrap();
}

#[derive(Debug, PartialEq, Eq)]
struct Counts {
    nodes: i64,
    orphan: i64,
    missing: i64,
}

fn counts(path: &Path) -> Counts {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let nodes = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0)).unwrap();
    let orphan = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes_fts_docsize d \
             WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.rowid = d.id)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let missing = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes n \
             WHERE NOT EXISTS (SELECT 1 FROM nodes_fts_docsize d WHERE d.id = n.rowid)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    Counts { nodes, orphan, missing }
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("ejecutar tylluan-cli")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn dry_run_reports_the_desync_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("silva.db");
    make_silva_db(&db);
    let backup = tmp.path().join("backup");
    let out = run(&[
        "repair-fts",
        "--db",
        db.to_str().unwrap(),
        "--backup-dir",
        backup.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "dry-run debe salir 0: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("DRY-RUN"), "debe anunciar dry-run: {text}");
    assert!(text.contains("huerfanos=1"), "debe mostrar el desajuste: {text}");
    assert_eq!(
        counts(&db),
        Counts { nodes: 3, orphan: 1, missing: 1 },
        "el dry-run no puede tocar la base"
    );
    assert!(!backup.exists(), "el dry-run no puede crear backup");
}

#[test]
fn repair_backs_up_rebuilds_and_verifies_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("silva.db");
    make_silva_db(&db);
    let backup = tmp.path().join("backup_previo");
    let out = run(&[
        "repair-fts",
        "--db",
        db.to_str().unwrap(),
        "--backup-dir",
        backup.to_str().unwrap(),
        "--yes",
    ]);
    assert!(out.status.success(), "repair debe salir 0: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("OK: nodes_fts reconstruido"), "salida: {text}");
    assert_eq!(
        counts(&db),
        Counts { nodes: 3, orphan: 0, missing: 0 },
        "el rebuild debe dejar huerfanos=0 y sin_indexar=0"
    );
    let manifest = backup.join("backup-manifest.json");
    assert!(manifest.exists(), "debe existir backup-manifest.json");
    let raw = std::fs::read_to_string(&manifest).unwrap();
    assert!(raw.contains("silva.db"), "el manifiesto debe listar silva.db: {raw}");
}

#[test]
fn aborts_without_touching_the_db_when_the_backup_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("silva.db");
    make_silva_db(&db);
    // Un FICHERO en la ruta del backup hace fallar la copia previa.
    let backup = tmp.path().join("backup_roto");
    std::fs::write(&backup, b"no soy un directorio").unwrap();

    let out = run(&[
        "repair-fts",
        "--db",
        db.to_str().unwrap(),
        "--backup-dir",
        backup.to_str().unwrap(),
        "--yes",
    ]);
    assert!(!out.status.success(), "debe abortar si el backup falla");
    assert_eq!(
        counts(&db),
        Counts { nodes: 3, orphan: 1, missing: 1 },
        "si el backup falla, la base NO se toca"
    );
}

#[test]
fn refuses_a_file_that_is_not_a_silva_db() {
    let tmp = tempfile::tempdir().unwrap();
    let other = tmp.path().join("otra.db");
    let conn = Connection::open(&other).unwrap();
    conn.execute_batch("CREATE TABLE foo (x TEXT);").unwrap();
    drop(conn);

    let out = run(&["repair-fts", "--db", other.to_str().unwrap(), "--yes"]);
    assert!(!out.status.success(), "una base que no es silva.db debe rechazarse");
    let text = stdout(&out);
    assert!(
        text.contains("no parece una silva.db") || String::from_utf8_lossy(&out.stderr).contains("no parece una silva.db"),
        "debe explicar por que la rechaza: {text} / {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn default_backup_dir_is_next_to_the_database() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("silva.db");
    make_silva_db(&db);
    let out = run(&["repair-fts", "--db", db.to_str().unwrap(), "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let backups: Vec<PathBuf> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("fts_rebuild_backup_"))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(backups.len(), 1, "debe crear un backup por defecto junto a la base");
    assert_eq!(counts(&db), Counts { nodes: 3, orphan: 0, missing: 0 });
}
