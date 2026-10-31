//! T-DSH-18 — auditoría adversarial de `consents.json` (D-15, `oauth_consent.rs`).
//!
//! Solo tests, contra la API pública del store real (`ConsentStore`). Vectores:
//!   1. fichero corrupto/truncado/JSON inválido al arrancar y en recarga por mtime
//!   2. `expires_at` en el futuro lejano / negativo / desbordado, `revoked_at`
//!      raro, `version` desconocida
//!   3. `client_id` con el separador `|` colisionando claves internas
//!   4. escrituras concurrentes (aprobaciones a la vez) sin perder ni duplicar
//!   5. fichero sustituido por symlink / permisos legibles por otros (nota Windows)
//!   6. ni credencial, ni csrf, ni consent_id en el fichero
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_file_adversarial
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_file_adversarial -- --ignored

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tylluan_kernel::transport::http::oauth_consent::{CONSENTS_FILE, ConsentStore};

const R1: &str = "http://127.0.0.1:9999/cb";
const R2: &str = "http://127.0.0.1:8888/cb";

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tylluan_t18_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join(CONSENTS_FILE)
}

fn write_raw(dir: &Path, body: &str) {
    std::fs::write(file_in(dir), body).unwrap();
}

fn record(client: &str, redirect: &str) -> Value {
    json!({
        "client_id": client,
        "client_name": client,
        "redirect_uri": redirect,
        "scopes": ["mcp"],
        "granted_at": 1_700_000_000,
        "last_used_at": 1_700_000_000,
        "expires_at": 4_700_000_000_i64,
        "revoked_at": null,
    })
}

fn file_doc(records: Vec<Value>) -> String {
    json!({ "version": 1, "consents": records }).to_string()
}

fn file_consents(dir: &Path) -> Vec<Value> {
    let raw = std::fs::read_to_string(file_in(dir)).unwrap_or_default();
    let doc: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("JSON inválido: {e}: {raw}"));
    doc["consents"].as_array().cloned().unwrap_or_default()
}

// ── V1: fichero corrupto/truncado/inválido (arranque) ───────────────────────

#[test]
fn v1_corrupt_file_at_startup_never_crashes_nor_grants_consent() {
    for (tag, body) in [
        ("garbage", "esto no es json {{{"),
        ("truncated", "{\"version\":1,\"consents\":[{\"client_id\":\"c1\""),
        ("empty", ""),
        ("null", "null"),
        ("wrong_types", "{\"version\":\"uno\",\"consents\":\"nada\"}"),
    ] {
        let dir = fresh_dir(tag);
        write_raw(&dir, body);
        let store = ConsentStore::with_file(&dir); // no debe paniquear
        assert!(
            store.valid_consent("c1", R1).is_none(),
            "{tag}: un fichero corrupto NO puede conceder consentimiento"
        );
        // El store sigue siendo usable y reescribe un fichero válido.
        store.record_consent("c1", "c1", R1, &["mcp".to_string()]);
        assert!(store.valid_consent("c1", R1).is_some(), "{tag}: tras aprobar");
        assert!(!file_consents(&dir).is_empty(), "{tag}: debe persistir");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn v1_corrupt_file_on_mtime_reload_revokes_previous_state() {
    // D-25 / F3 (T-DSH-18): fail-closed. Antes, un JSON inválido en la recarga
    // mantenía el estado en memoria: revocar editando el fichero fallaba
    // ABIERTO si el edit dejaba el JSON roto. Ahora la corrupción revoca todo
    // hasta que el fichero vuelva a ser válido (el peor caso es volver a pedir
    // consentimiento, nunca dejar fuera a quien aprobó).
    let dir = fresh_dir("reload_corrupt");
    let store = ConsentStore::with_file(&dir);
    store.record_consent("c1", "c1", R1, &["mcp".to_string()]);
    assert!(store.valid_consent("c1", R1).is_some());

    std::thread::sleep(std::time::Duration::from_millis(20));
    write_raw(&dir, "{\"version\":1,\"consents\":[ B R O K E N");
    // La recarga (mtime distinto) parsea mal: todo queda revocado.
    assert!(
        store.valid_consent("c1", R1).is_none(),
        "un fichero inválido en recarga debe tratar los consentimientos como revocados"
    );
    // Y una aprobación nueva repara el fichero sin resucitar lo anterior.
    store.record_consent("c2", "c2", R2, &["mcp".to_string()]);
    assert!(store.valid_consent("c2", R2).is_some());
    assert!(store.valid_consent("c1", R1).is_none(), "c1 sigue revocado");
    assert!(!file_consents(&dir).is_empty(), "el fichero debe quedar válido");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── V2: valores de caducidad/revocación/versión hostiles ────────────────────

#[test]
fn v2_hostile_expiry_and_revocation_values() {
    let dir = fresh_dir("hostile_values");
    let far = 9_000_000_000_i64; // muy por encima de 2038
    let mut r_future = record("lejano", R1);
    r_future["expires_at"] = json!(far);
    let mut r_negative = record("negativo", R1);
    r_negative["expires_at"] = json!(-1);
    let mut r_i64max = record("maximo", R1);
    r_i64max["expires_at"] = json!(i64::MAX);
    let mut r_revoked0 = record("revocado0", R1);
    r_revoked0["revoked_at"] = json!(0);
    let mut r_revokedneg = record("revocadoneg", R1);
    r_revokedneg["revoked_at"] = json!(-1);
    let mut r_clean = record("limpio", R1);
    r_clean["revoked_at"] = json!(null);

    let mut doc = json!({ "version": 99, "consents": [
        r_future, r_negative, r_i64max, r_revoked0, r_revokedneg, r_clean
    ]});
    write_raw(&dir, &doc.to_string());
    let store = ConsentStore::with_file(&dir);

    // Futuro lejano y i64::MAX: vigentes (no hay tope superior; es decisión del
    // operador que edita el fichero). No debe desbordar ni paniquear.
    assert!(store.valid_consent("lejano", R1).is_some(), "futuro lejano -> vigente");
    assert!(store.valid_consent("maximo", R1).is_some(), "i64::MAX -> vigente, sin overflow");
    // Negativo y revocados: no vigentes.
    assert!(store.valid_consent("negativo", R1).is_none(), "negativo -> caducado");
    assert!(store.valid_consent("revocado0", R1).is_none(), "revoked_at=0 es Some -> revocado");
    assert!(store.valid_consent("revocadoneg", R1).is_none(), "revoked_at=-1 -> revocado");
    assert!(store.valid_consent("limpio", R1).is_some(), "revoked_at=null -> vigente");

    // `version` desconocida: se acepta tal cual (no hay validación de versión).
    doc["version"] = json!(12345);
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_raw(&dir, &doc.to_string());
    assert!(
        store.valid_consent("lejano", R1).is_some(),
        "documentado: version desconocida se acepta (no se valida el campo version)"
    );
    // `version` con tipo inválido → parse error → fail-closed (D-25/F3): el
    // fichero inválido revoca todo hasta que vuelva a ser válido.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_raw(&dir, "{\"version\":-1,\"consents\":[]}");
    assert!(
        store.valid_consent("lejano", R1).is_none(),
        "version inválida: fichero inválido → consentimientos revocados"
    );
    // Un fichero válido posterior vuelve a habilitar lo que contiene.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_raw(
        &dir,
        &json!({ "version": 1, "consents": [record("de-nuevo", R1)] }).to_string(),
    );
    assert!(
        store.valid_consent("de-nuevo", R1).is_some(),
        "un fichero válido vuelve a habilitar sus entradas"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── V3: separador `|` en client_id → colisión de claves ─────────────────────

#[test]
fn v3_pipe_in_client_id_collides_internal_keys() {
    // (a) Vía API pública en memoria: el par (a|b, c) y el par (a, b|c)
    // producen la MISMA clave "a|b|c".
    let store = tylluan_kernel::transport::http::oauth_consent::ConsentStore::in_memory();
    store.record_consent("a|b", "a|b", "c", &["mcp".to_string()]);
    let confused = store.valid_consent("a", "b|c");
    assert!(
        confused.is_none(),
        "esperado: el par (a, b|c) no hereda el consentimiento de (a|b, c); \
         real: Some({:?})",
        confused.map(|c| (c.client_id, c.redirect_uri))
    );

    // (a) Vía API pública en memoria: el par (a|b, c) y el par (a, b|c) son
    // entradas distintas y no se heredan la una a la otra (F1, cerrado en D-25:
    // la clave interna pasó a `len(client_id):client_idredirect_uri`).
    let store = tylluan_kernel::transport::http::oauth_consent::ConsentStore::in_memory();
    store.record_consent("a|b", "a|b", "c", &["mcp".to_string()]);
    let confused = store.valid_consent("a", "b|c");
    assert!(
        confused.is_none(),
        "esperado: el par (a, b|c) no hereda el consentimiento de (a|b, c); \
         real: Some({:?})",
        confused.map(|c| (c.client_id, c.redirect_uri))
    );
    assert!(store.valid_consent("a|b", "c").is_some());

    // (b) Vía fichero: las dos entradas coexisten como pares distintos.
    let dir = fresh_dir("pipe_collision");
    let two = file_doc(vec![
        record("a|b", "c"),
        record("a", "b|c"),
    ]);
    write_raw(&dir, &two);
    let file_store = ConsentStore::with_file(&dir);
    let left = file_store
        .valid_consent("a|b", "c")
        .expect("(a|b, c) debe seguir vigente");
    let right = file_store
        .valid_consent("a", "b|c")
        .expect("(a, b|c) debe seguir vigente");
    assert_eq!(
        (left.client_id.as_str(), left.redirect_uri.as_str()),
        ("a|b", "c")
    );
    assert_eq!(
        (right.client_id.as_str(), right.redirect_uri.as_str()),
        ("a", "b|c")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── V4: escrituras concurrentes (dos aprobaciones a la vez) ─────────────────

#[test]
fn v4_concurrent_approvals_do_not_lose_or_duplicate_records() {
    let dir = fresh_dir("concurrent");
    let store = std::sync::Arc::new(ConsentStore::with_file(&dir));

    // Rondas de aprobaciones simultáneas con claves distintas: ninguna puede
    // perderse ni duplicarse en el fichero (tmp + rename bajo mutex).
    // (Se probó también con 200 rondas × 4 hilos = 800 aprobaciones: sin
    // pérdida. 10 rondas es lo que corre en CI.)
    const ROUNDS: usize = 10;
    for round in 0..ROUNDS {
        let mut handles = Vec::new();
        for i in 0..4 {
            let s = store.clone();
            handles.push(std::thread::spawn(move || {
                let client = format!("conc-{round}-{i}");
                s.record_consent(&client, &client, R1, &["mcp".to_string()]);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }
    let entries = file_consents(&dir);
    let mut ids: Vec<String> = entries
        .iter()
        .filter_map(|c| c["client_id"].as_str().map(str::to_string))
        .collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "hay registros duplicados en el fichero");
    assert_eq!(
        total,
        ROUNDS * 4,
        "se perdieron aprobaciones concurrentes: {total}/{} en disco",
        ROUNDS * 4
    );
    for round in 0..ROUNDS {
        for i in 0..4 {
            let client = format!("conc-{round}-{i}");
            assert!(
                store.valid_consent(&client, R1).is_some(),
                "la aprobación concurrente '{client}' no está vigente tras las carreras"
            );
        }
    }

    // Misma clave aprobada dos veces a la vez: una sola entrada (HashMap).
    let mut handles = Vec::new();
    for _ in 0..2 {
        let s = store.clone();
        handles.push(std::thread::spawn(move || {
            s.record_consent("duplicada", "duplicada", R2, &["mcp".to_string()]);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let dup = file_consents(&dir)
        .iter()
        .filter(|c| c["client_id"] == "duplicada")
        .count();
    assert_eq!(dup, 1, "la misma clave no debe duplicarse en el fichero");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── V5: symlink / permisos (nota Windows) ──────────────────────────────────

#[test]
fn v5_file_replacement_and_permission_notes() {
    let dir = fresh_dir("perms");

    // Sustitución del fichero por otro fichero con contenido válido: se relee
    // (es la vía documentada de revocación). No debe caer.
    let store = ConsentStore::with_file(&dir);
    store.record_consent("c1", "c1", R1, &["mcp".to_string()]);
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_raw(&dir, &file_doc(vec![record("c2", R1)]));
    assert!(store.valid_consent("c1", R1).is_none(), "la sustitución se relee");
    assert!(store.valid_consent("c2", R1).is_some());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(file_in(&dir))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode & 0o077,
            0,
            "hallazgo: consents.json es legible por grupo/otros (modo {mode:o}); \
             std::fs::write usa el umask y no se endurecen permisos"
        );
        // Symlink: el fichero puede sustituirse por un enlace a otro fichero
        // (el store lo sigue al leer; persist reemplaza el enlace, no el target).
        let other = dir.join("otro.json");
        std::fs::write(&other, file_doc(vec![record("c3", R1)])).unwrap();
        let link = file_in(&dir);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&other, &link).unwrap();
        // T-DSH-155: en Linux el mtime puede tener granularidad de tick: la
        // escritura anterior (persist) y `otro.json` pueden caer en el MISMO
        // tick y el store (que compara mtime) no detecta el cambio -> falso
        // negativo solo en Linux. Fijamos un mtime posterior explícito para
        // medir el seguimiento del symlink, no la resolución del reloj del fs.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&other)
            .unwrap()
            .set_modified(later)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(store.valid_consent("c3", R1).is_some(), "sigue el symlink al leer");
    }

    #[cfg(windows)]
    {
        // En Windows no hay bits POSIX: el fichero hereda la ACL del directorio
        // de datos (perfil del usuario). No se puede "chmod 0600" portable.
        // Symlink: crear uno requiere SeCreateSymbolicLinkPrivilege (o modo
        // desarrollador); sin ese privilegio no es un vector reproducible aquí.
        eprintln!(
            "Windows: consents.json hereda ACL del directorio de datos; no hay bits \
             POSIX. Symlink requiere privilegio SeCreateSymbolicLinkPrivilege: \
             NO VERIFICADO en esta máquina (no se intenta crear)."
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ── V6: nada de credencial / csrf / consent_id en el fichero ────────────────

#[test]
fn v6_file_never_contains_credential_csrf_or_consent_id() {
    let dir = fresh_dir("secrets");
    let store = ConsentStore::with_file(&dir);
    let credential = "MASTER-CREDENCIAL-SECRETA-D18";
    let pending = store.create_pending(
        tylluan_kernel::transport::http::oauth_consent::PendingConsent {
            consent_id: String::new(),
            csrf: String::new(),
            client_id: "c1".to_string(),
            client_name: "c1".to_string(),
            redirect_uri: R1.to_string(),
            redirect_host: "http://127.0.0.1:9999".to_string(),
            code_challenge: "challenge".to_string(),
            state: Some("st".to_string()),
            resource: None,
            scopes: vec!["mcp".to_string()],
            created_at: 0,
            expires_at: 0,
        },
    );
    store.record_consent("c1", "c1", R1, &["mcp".to_string()]);
    // La credencial nunca entra al store; el pending no se persiste.
    let raw = std::fs::read_to_string(file_in(&dir)).unwrap();
    for (what, needle) in [
        ("credencial", credential),
        ("csrf", pending.csrf.as_str()),
        ("consent_id", pending.consent_id.as_str()),
    ] {
        assert!(
            !raw.contains(needle),
            "el fichero contiene {what} ({needle:?}): {raw}"
        );
    }
    // Y las claves del JSON son solo metadatos del registro.
    let doc: Value = serde_json::from_str(&raw).unwrap();
    let allowed = [
        "client_id",
        "client_name",
        "redirect_uri",
        "scopes",
        "granted_at",
        "last_used_at",
        "expires_at",
        "revoked_at",
    ];
    for c in doc["consents"].as_array().unwrap() {
        for key in c.as_object().unwrap().keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "clave inesperada en el fichero: {key}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
