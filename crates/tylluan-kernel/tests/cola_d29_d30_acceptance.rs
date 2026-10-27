//! T-DSH-39 â€” aceptaciÃ³n ejecutable de D-29 y D-30 (cola de Deep).
//!
//! Dos defectos medidos en la cola `docs/internal/COLA_deep_2026-10-22.md`:
//!
//! - **D-29**: un `consents.json` ilegible se trata como revocado en memoria
//!   (fail-closed correcto), pero el siguiente `persist` lo **sobrescribe** con
//!   el estado vacÃ­o y el contenido anterior desaparece del disco. Lo deseado:
//!   antes de escribir, mover el ilegible a `consents.json.corrupt-<ts>` y
//!   registrar `warn!` (no escribir si el movimiento falla).
//! - **D-30**: los planes pendientes de aprobaciÃ³n viven en `PLAN_STORE`, un
//!   `OnceLock<RwLock<HashMap>>` en memoria (`security/grants.rs`); un reinicio
//!   los pierde. Lo deseado: `init_plan_store` carga los pendientes no caducados
//!   al arrancar desde la base de datos del kernel.
//!
//! ESTADO verificado 2026-10-27 (M-BUFFY-13): los dos defectos estan
//! CERRADOS en `396af2b` y este fichero es su **lista de aceptacion**.
//! Los 4 tests corren en la suite normal (0 ignorados) y afirman el
//! comportamiento DESEADO (el ilegible se conserva como
//! `consents.json.corrupt-<ts>`; el plan pendiente sobrevive al reinicio)
//! mas una guarda por defecto de cada uno (un fichero valido no se
//! cuarentena; un plan retirado no resucita). Antes del arreglo habia 2
//! tests `#[ignore]`; hoy no queda ninguno.
//!
//! El Â«reinicioÂ» de D-30 se simula con un **proceso hijo** del propio binario de
//! test (un `OnceLock` no se puede reiniciar in-process): el padre crea el plan
//! con `DATA_DIR` apuntando a un temporal, y el hijo arranca de cero, llama a
//! `init_plan_store()` y busca el plan.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d29_d30_acceptance
//! (sin tests ignorados: `-- --ignored` no ejecuta nada y no se documenta aqui)
//!
//! Solo directorios temporales; sin `cfg(unix)`/`cfg(windows)` en ninguna
//! funciÃ³n; `src/` intacto.

use std::path::{Path, PathBuf};
use std::process::Command;

use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::oauth_consent::{CONSENTS_FILE, ConsentStore};

fn fresh_dir(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "tylluan_d29d30_{tag}_{}_{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("data dir temporal");
    dir
}

/// Copias `consents.json.corrupt-*` que deje el arreglo de D-29.
fn corrupt_backups(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("consents.json.corrupt-"))
                .unwrap_or(false)
        })
        .collect()
}

/// Escenario comÃºn de D-29: fichero corrupto + aprobaciÃ³n de un cliente nuevo.
fn corrupt_then_approve(dir: &Path) -> (Vec<u8>, serde_json::Value) {
    let corrupt = b"{ esto no es JSON valido ///".to_vec();
    std::fs::write(dir.join(CONSENTS_FILE), &corrupt).expect("escribir consents corrupto");
    // with_file relee: falla, deja el estado vacÃ­o (fail-closed) y marca error.
    let store = ConsentStore::with_file(dir);
    let _ = store.record_consent(
        "cliente-nuevo-d29",
        "Cliente D29",
        "http://127.0.0.1:47004/callback",
        &["read".to_string()],
    );
    let raw = std::fs::read_to_string(dir.join(CONSENTS_FILE)).expect("consents.json nuevo");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("consents.json debe ser JSON");
    (corrupt, parsed)
}

fn consent_client_ids(parsed: &serde_json::Value) -> Vec<String> {
    parsed
        .get("consents")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("client_id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// D-29
// ---------------------------------------------------------------------------

/// **Comportamiento DESEADO (D-29).** El fichero corrupto se conserva como
/// `consents.json.corrupt-*` y el nuevo `consents.json` solo tiene el cliente
/// Cumplido desde `396af2b` (el test pasa en la suite normal).
#[test]
fn d29_corrupt_consents_must_be_preserved() {
    let dir = fresh_dir("d29_desired");
    let (corrupt, parsed) = corrupt_then_approve(&dir);

    let backups = corrupt_backups(&dir);
    assert!(
        !backups.is_empty(),
        "D-29 pendiente: no se conservÃ³ copia del consents.json ilegible \
         (consents.json.corrupt-<ts>); el contenido original se perdiÃ³"
    );
    let backup = std::fs::read(&backups[0]).expect("leer copia");
    assert_eq!(
        backup, corrupt,
        "D-29: la copia .corrupt-* debe tener los bytes originales del fichero ilegible"
    );
    assert_eq!(
        consent_client_ids(&parsed),
        vec!["cliente-nuevo-d29".to_string()],
        "D-29: el consents.json nuevo solo debe tener el cliente nuevo"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Guarda de la otra cara de D-29.** Un fichero **vÃ¡lido** no se cuarentena:
/// el arreglo no debe empezar a llenar el directorio de copias ni a perder
/// consentimientos que estaban bien.
#[test]
fn d29_valid_consents_file_is_not_quarantined() {
    let dir = fresh_dir("d29_valid");
    let valid = r#"{
  "version": 1,
  "consents": [
    {
      "client_id": "cliente-viejo-d29",
      "client_name": "Cliente viejo",
      "redirect_uri": "http://127.0.0.1:47004/callback",
      "scopes": ["read"],
      "granted_at": 1,
      "last_used_at": 1,
      "expires_at": 9999999999
    }
  ]
}"#;
    std::fs::write(dir.join(CONSENTS_FILE), valid).expect("escribir consents vÃ¡lido");
    let store = ConsentStore::with_file(&dir);
    let _ = store.record_consent(
        "cliente-nuevo-d29",
        "Cliente D29",
        "http://127.0.0.1:47004/callback",
        &["read".to_string()],
    );
    assert!(
        corrupt_backups(&dir).is_empty(),
        "D-29: un consents.json vÃ¡lido no debe moverse a .corrupt-*"
    );
    let raw = std::fs::read_to_string(dir.join(CONSENTS_FILE)).expect("consents.json");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    let mut ids = consent_client_ids(&parsed);
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "cliente-nuevo-d29".to_string(),
            "cliente-viejo-d29".to_string()
        ],
        "D-29: el consentimiento que ya estaba vÃ¡lido debe sobrevivir a la escritura"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// D-30 â€” reinicio simulado con un proceso hijo del propio binario de test
// ---------------------------------------------------------------------------

/// Si esta variable estÃ¡ en el entorno, el test actÃºa como PROCESO HIJO
/// (Â«reinicioÂ»): arranca el plan store desde cero y busca el plan. CÃ³digos de
/// salida: 0 = encontrado, 2 = no encontrado, otros = error.
const CHILD_ENV: &str = "TYLLUAN_D30_CHILD";
const PLAN_ID_ENV: &str = "TYLLUAN_D30_PLAN_ID";

/// Los dos tests de D-30 comparten `DATA_DIR` (variable de **proceso**) y el
/// `PLAN_STORE` (`OnceLock`): en paralelo se pisan entre sÃ­ (uno escribe el
/// snapshot del otro en su directorio). Se serializan dentro del binario.
static D30_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Crea el plan en ESTE proceso (con `DATA_DIR` fijado al temporal) y devuelve
/// el id. Desde `396af2b` `store_plan` persiste el plan en la base del
/// kernel y `init_plan_store` recarga los pendientes no caducados al arrancar.
async fn create_plan(dir: &Path) -> String {
    // El kernel real resuelve su directorio de datos por entorno; el test lo
    // fija antes de inicializar el store.
    // SAFETY: variable de proceso del binario de test.
    unsafe {
        std::env::set_var("DATA_DIR", dir);
    }
    grants::init_plan_store();
    let id = format!("d30-plan-{}", std::process::id());
    grants::store_plan(
        &id,
        "bash",
        "bash_execute",
        &serde_json::json!({"command": "cargo test"}),
        "agent-d30",
        "run tests",
    )
    .await;
    id
}

/// Arranca un proceso hijo del mismo binario para simular el reinicio y
/// devuelve su cÃ³digo de salida.
fn restart_and_lookup(dir: &Path, plan_id: &str, test_name: &str, test_is_ignored: bool) -> Option<i32> {
    let exe = std::env::current_exe().expect("current_exe");
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test_name, "--nocapture"]);
    if test_is_ignored {
        cmd.arg("--ignored");
    }
    let out = cmd
        .env("DATA_DIR", dir)
        .env(CHILD_ENV, "1")
        .env(PLAN_ID_ENV, plan_id)
        .output()
        .expect("lanzar proceso hijo de reinicio");
    // El marcador demuestra que el hijo ejecutÃ³ el helper (si el filtro de
    // libtest no casa, el hijo saldrÃ­a con 0 sin haber comprobado nada).
    assert!(
        dir.join("child_ran.marker").exists(),
        "el proceso hijo no ejecutÃ³ el helper (Â¿filtro de test incorrecto?): {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out.status.code()
}

/// Rama del PROCESO HIJO: store nuevo, `init_plan_store()` (que D-30 debe hacer
/// cargar desde disco) y bÃºsqueda del plan.
async fn child_lookup() -> ! {
    let id = std::env::var(PLAN_ID_ENV).expect("plan id en el hijo");
    let dir = std::env::var("DATA_DIR").expect("DATA_DIR en el hijo");
    let _ = std::fs::write(std::path::Path::new(&dir).join("child_ran.marker"), b"ok");
    grants::init_plan_store();
    let found = grants::get_plan(&id).await.is_some();
    std::process::exit(if found { 0 } else { 2 });
}

/// **Comportamiento DESEADO (D-30).** El plan sobrevive a un reinicio: un
/// proceso nuevo con el mismo `DATA_DIR` lo encuentra tras `init_plan_store`.
#[tokio::test(flavor = "multi_thread")]
async fn d30_pending_plan_must_survive_restart() {
    if std::env::var(CHILD_ENV).is_ok() {
        child_lookup().await;
    }
    let _guard = D30_GUARD.lock().await;
    let dir = fresh_dir("d30_desired");
    let id = create_plan(&dir).await;
    let code = restart_and_lookup(&dir, &id, "d30_pending_plan_must_survive_restart", false);
    assert_eq!(
        code,
        Some(0),
        "D-30 pendiente: el plan '{id}' no sobrevive al reinicio (el hijo saliÃ³ con {code:?}; \
         2 = no encontrado). PLAN_STORE sigue siendo solo memoria de proceso"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Guarda de la otra cara de D-30.** Persistir no debe resucitar lo aprobado:
/// un plan que se quita del store desaparece tambiÃ©n del disco, asÃ­ que un
/// reinicio posterior no lo encuentra (si lo encontrara, la aprobaciÃ³n se
/// ejecutarÃ­a dos veces).
#[tokio::test(flavor = "multi_thread")]
async fn d30_removed_plan_does_not_survive_restart() {
    if std::env::var(CHILD_ENV).is_ok() {
        child_lookup().await;
    }
    let _guard = D30_GUARD.lock().await;
    let dir = fresh_dir("d30_removed");
    create_plan(&dir).await;
    let id = format!("d30-plan-{}", std::process::id());
    assert!(
        grants::remove_plan(&id).await,
        "D-30: el plan reciÃ©n creado debe existir para poder quitarlo"
    );
    let code = restart_and_lookup(&dir, &id, "d30_removed_plan_does_not_survive_restart", false);
    assert_eq!(
        code,
        Some(2),
        "D-30: un plan ya quitado no debe aparecer tras el reinicio (hijo saliÃ³ con {code:?}; \
         0 = resucitÃ³)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
