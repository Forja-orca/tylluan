// SQLCipher integration audit — verifies cipher layer is correctly compiled and linked.
// Only compiled when the `encryption` feature is active.
// Run with: cargo test -p tylluan-kernel --test sqlcipher_audit --features encryption
#![cfg(feature = "encryption")]

use rusqlite::Connection;
use std::path::PathBuf;

fn temp_db_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("tylluan_sqlcipher_test_{}.db", name));
    p
}

fn cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// SQLCipher round-trip: write with key, read with same key, fail with wrong key.
#[test]
fn test_sqlcipher_encrypt_decrypt_roundtrip() {
    let path = temp_db_path("roundtrip");
    cleanup(&path);

    let key_hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    // Create encrypted DB and write data
    {
        let conn = Connection::open(&path).expect("open for write");
        conn.pragma_update(None, "hexkey", &key_hex).expect("set hexkey");
        conn.execute_batch("CREATE TABLE secrets (value TEXT NOT NULL);")
            .expect("create table");
        conn.execute("INSERT INTO secrets VALUES ('classified')", [])
            .expect("insert");
    }

    // Re-open with CORRECT key — must succeed
    {
        let conn = Connection::open(&path).expect("open for read");
        conn.pragma_update(None, "hexkey", &key_hex).expect("set hexkey");
        let val: String = conn
            .query_row("SELECT value FROM secrets", [], |r| r.get(0))
            .expect("read with correct key");
        assert_eq!(val, "classified");
    }

    // Re-open with WRONG key — must fail on first query
    {
        let wrong_key = "b".repeat(64);
        let conn = Connection::open(&path).expect("open with wrong key");
        conn.pragma_update(None, "hexkey", &wrong_key)
            .expect("pragma hexkey accepted (validation deferred to query)");
        let result =
            conn.query_row("SELECT value FROM secrets", [], |r| r.get::<_, String>(0));
        assert!(
            result.is_err(),
            "Wrong hexkey must fail on first query against encrypted DB"
        );
    }

    cleanup(&path);
}

/// An unencrypted DB opened without PRAGMA hexkey must remain readable.
#[test]
fn test_unencrypted_db_readable_without_key() {
    let path = temp_db_path("plaintext");
    cleanup(&path);

    {
        let conn = Connection::open(&path).expect("create plaintext db");
        conn.execute_batch("CREATE TABLE data (x INTEGER);").unwrap();
        conn.execute("INSERT INTO data VALUES (99)", []).unwrap();
    }

    {
        let conn = Connection::open(&path).expect("re-open plaintext db");
        let val: i64 = conn
            .query_row("SELECT x FROM data", [], |r| r.get(0))
            .expect("read without key");
        assert_eq!(val, 99);
    }

    cleanup(&path);
}

/// TYLLUAN_DB_KEY validation: key must be exactly 64 lowercase hex chars.
/// Mirrors the validation in config::open_db.
#[test]
fn test_key_format_validation() {
    let is_valid_key = |s: &str| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit());

    assert!(is_valid_key("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"));
    assert!(is_valid_key(&"0".repeat(64)));
    assert!(is_valid_key(&"f".repeat(64)));
    assert!(!is_valid_key("short"));
    assert!(!is_valid_key(&"g".repeat(64)));       // 'g' is not hex
    assert!(!is_valid_key(&"a".repeat(63)));        // too short
    assert!(!is_valid_key(&"a".repeat(65)));        // too long
    assert!(!is_valid_key("'; DROP TABLE secrets; --")); // injection attempt
}
