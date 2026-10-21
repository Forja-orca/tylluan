//! T-DSH-24 — auditoría del consentimiento OAuth tras D-25 (solo lectura + tests).
//!
//! Cuatro preguntas del encargo, medidas contra el código real de
//! `transport/http/oauth_consent.rs` (no contra la doc):
//!   1. V7c sigue probando que 20 fallos desde 20 IPs distintas NO bloquean a
//!      una IP nueva ni al operador legítimo (el test de router vive en
//!      `oauth_f2_consent_adversarial.rs`, aquí se mide además la unidad).
//!   2. Con `127.0.0.1` como único bucket: cuántos fallos de terceros locales
//!      bloquean al operador y por cuánto (escalado 30/60/120/300).
//!   3. `MAX_PENDING=128`: un atacante local que abre 128 pendientes, ¿echa
//!      fuera el pendiente legítimo? (las dos órdenes).
//!   4. `consents.json` inválido ⇒ fail-closed en memoria: caso de ida y vuelta
//!      (corrompo, arreglo, ¿vuelven los consentimientos?).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test oauth_consent_audit_t24 -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tylluan_kernel::transport::http::oauth_consent::{
    lockout_secs, ConsentStore, PendingConsent, CONSENTS_FILE, GLOBAL_MAX_FAILURES, MAX_PENDING,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "tylluan_t24_{tag}_{}_{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn pending(client_id: &str) -> PendingConsent {
    PendingConsent {
        consent_id: String::new(),
        csrf: String::new(),
        client_id: client_id.to_string(),
        client_name: client_id.to_string(),
        redirect_uri: "http://127.0.0.1:8001/cb".to_string(),
        redirect_host: "http://127.0.0.1:8001".to_string(),
        code_challenge: "challenge".to_string(),
        state: None,
        resource: None,
        scopes: vec!["memory:read".to_string()],
        created_at: 0,
        expires_at: 0,
    }
}

fn write_raw(dir: &Path, body: &str) {
    std::fs::write(dir.join(CONSENTS_FILE), body).expect("write consents.json");
}

#[test]
fn t24_c1_global_ceiling_never_blocks_a_fresh_ip_nor_the_operator() {
    let store = ConsentStore::in_memory();
    // 20 fallos desde 20 IPs distintas (un tercero): techo global saturado.
    for i in 0..GLOBAL_MAX_FAILURES {
        store.record_failure(&format!("203.0.113.{i}"));
    }
    // El operador (bucket propio, sin fallos propios) NO queda bloqueado…
    assert!(
        !store.is_locked("127.0.0.1"),
        "el techo global no puede bloquear a una IP sin fallos propios (V7c)"
    );
    assert_eq!(store.retry_after_secs("127.0.0.1"), 0);
    // …ni una IP nueva cualquiera.
    assert!(!store.is_locked("198.51.100.7"));
    assert_eq!(store.retry_after_secs("198.51.100.7"), 0);
    // Y una IP que sí falló sigue su propio escalado (aquí 1 fallo: sin bloqueo).
    assert!(!store.is_locked("203.0.113.0"));
    println!(
        "| 1 | 20 fallos/20 IPs -> IP nueva y operador | no bloqueados | is_locked(operador)={} fresh={} |",
        store.is_locked("127.0.0.1"),
        store.is_locked("198.51.100.7")
    );
}

#[test]
fn t24_c2_loopback_only_bucket_blocks_the_operator_with_escalation() {
    let store = ConsentStore::in_memory();
    let mut real: Vec<(usize, i64, i64)> = Vec::new();
    for n in 1..=12usize {
        store.record_failure("127.0.0.1");
        let expected = lockout_secs(n);
        let real_retry = store.retry_after_secs("127.0.0.1");
        // Tolerancia de 1 s: el reloj avanza entre record y consulta.
        let ok = if expected == 0 {
            real_retry == 0
        } else {
            real_retry >= expected - 1 && real_retry <= expected + 1
        };
        assert!(
            ok,
            "con {n} fallos locales el operador debe quedar bloqueado ~{expected}s, real {real_retry}s"
        );
        real.push((n, expected, real_retry));
    }
    // A partir de MAX_FAILURES el bucket loopback (único en un despliegue
    // local) es el MISMO que usa el operador: 5 fallos de terceros locales
    // bastan para dejarlo fuera 30 s, y cada escalón lo alarga.
    assert!(store.is_locked("127.0.0.1"));
    assert!(store.retry_after_secs("127.0.0.1") >= 299, "12 fallos -> 300 s");

    // El extra global (+2 s) solo se suma a quien ya tiene fallos propios.
    for i in 0..(GLOBAL_MAX_FAILURES - 12) {
        store.record_failure(&format!("10.9.9.{i}"));
    }
    let with_global = store.retry_after_secs("127.0.0.1");
    assert!(
        (301..=302).contains(&with_global),
        "techo global saturado: 300 + 2 s de extra, real {with_global}"
    );
    assert!(!store.is_locked("10.9.9.99"), "IP nueva sigue sin bloqueo");

    println!("| 2 | fallos locales en el bucket 127.0.0.1 | escalado 30/60/120/300 s |");
    for (n, expected, got) in &real {
        println!("| 2 | {n} fallos locales | lockout {expected} s | real {got} s |");
    }
    println!("| 2 | +techo global saturado (>= {GLOBAL_MAX_FAILURES}) | +2 s solo a quien falló | real {with_global} s |");
}

#[test]
fn t24_c3_pending_cap_evicts_the_legitimate_pending() {
    // (a) legítimo primero, flood después: el legítimo es la víctima (FIFO).
    let store = ConsentStore::in_memory();
    let legit = store.create_pending(pending("legitimo"));
    for i in 0..MAX_PENDING {
        store.create_pending(pending(&format!("atacante-{i}")));
    }
    assert_eq!(store.pending_count(), MAX_PENDING, "el tope acota la memoria");
    let legit_alive = store.peek_pending(&legit.consent_id).is_some();
    assert!(
        !legit_alive,
        "con MAX_PENDING={MAX_PENDING} el pendiente legítimo (el más antiguo) se desaloja"
    );

    // (b) flood primero, legítimo después: ahora sobrevive el legítimo y cae el
    // más antiguo del atacante (la víctima depende del orden).
    let store2 = ConsentStore::in_memory();
    let mut attacker_ids = Vec::new();
    for i in 0..MAX_PENDING {
        attacker_ids.push(store2.create_pending(pending(&format!("atacante-{i}"))).consent_id);
    }
    let legit2 = store2.create_pending(pending("legitimo"));
    assert_eq!(store2.pending_count(), MAX_PENDING);
    assert!(store2.peek_pending(&legit2.consent_id).is_some());
    assert!(
        store2.peek_pending(&attacker_ids[0]).is_none(),
        "el desalojo es del más antiguo, sea de quien sea"
    );
    println!(
        "| 3 | legítimo y luego {MAX_PENDING} flood | legítimo desalojado (FIFO) | alive={legit_alive} count={} |",
        store.pending_count()
    );
    println!(
        "| 3 | {MAX_PENDING} flood y luego legítimo | legítimo sobrevive, cae el 1er atacante | alive={} primer_atacante={} |",
        store2.peek_pending(&legit2.consent_id).is_some(),
        store2.peek_pending(&attacker_ids[0]).is_some()
    );
}

#[test]
fn t24_c4_corrupt_file_revokes_and_returns_when_fixed() {
    let dir = fresh_dir("c4");
    let store = ConsentStore::with_file(&dir);
    let rec = store.record_consent(
        "cliente-t24",
        "Cliente T24",
        "http://127.0.0.1:8001/cb",
        &["memory:read".to_string()],
    );
    assert!(store.valid_consent("cliente-t24", "http://127.0.0.1:8001/cb").is_some());
    let good = std::fs::read_to_string(dir.join(CONSENTS_FILE)).expect("fichero válido");

    // 1) corrompo el fichero -> fail-closed en memoria.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write_raw(&dir, "{ esto no es json valido");
    let revoked = store.valid_consent("cliente-t24", "http://127.0.0.1:8001/cb");
    assert!(revoked.is_none(), "fichero inválido = consentimiento revocado");
    // La consulta fallida NO reescribe el fichero (el operador puede arreglarlo).
    let after_probe = std::fs::read_to_string(dir.join(CONSENTS_FILE)).unwrap_or_default();
    assert_eq!(after_probe, "{ esto no es json valido", "no se pisa el fichero roto");

    // 2) arreglo el fichero con el contenido válido original -> ¿vuelven?
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write_raw(&dir, &good);
    let returned = store.valid_consent("cliente-t24", "http://127.0.0.1:8001/cb");
    assert!(
        returned.is_some(),
        "tras arreglar consents.json los consentimientos válidos vuelven (reload por mtime)"
    );
    let _ = rec;

    // 3) matiz medido: si tras corromper se APRUEBA otro cliente, el persist
    // reescribe el fichero desde el estado en memoria (vacío) y el
    // consentimiento anterior se pierde del disco, no solo se revoca.
    let dir2 = fresh_dir("c4b");
    let store2 = ConsentStore::with_file(&dir2);
    store2.record_consent("viejo", "Viejo", "http://127.0.0.1:8001/cb", &["memory:read".to_string()]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write_raw(&dir2, "roto");
    assert!(store2.valid_consent("viejo", "http://127.0.0.1:8001/cb").is_none());
    store2.record_consent("nuevo", "Nuevo", "http://127.0.0.1:8001/cb", &["memory:read".to_string()]);
    let after_approve = std::fs::read_to_string(dir2.join(CONSENTS_FILE)).unwrap_or_default();
    let old_survived = after_approve.contains("\"viejo\"");
    assert!(
        !old_survived,
        "observación: el persist posterior al fail-closed pierde el consentimiento anterior"
    );
    println!(
        "| 4 | corrompo -> arreglo | revocado y luego recuperado | revoked={} returned={} |",
        revoked.is_none(),
        returned.is_some()
    );
    println!(
        "| 4b | corrompo y luego apruebo otro cliente | el viejo no vuelve del disco (persist del estado vacío) | viejo_en_disco={old_survived} |"
    );
}
