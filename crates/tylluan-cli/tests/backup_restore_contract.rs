//! T-DSH-156 (H-60) — contrato de `tylluan-cli backup` / `restore`.
//!
//! Antes del arreglo: el backup cubria 5 de las 12 bases del DATA_DIR
//! (agent_profiles, audit, contracts, curriculum, jobs, journal, mailbox,
//! peers, pending_dispatches, silva, task_context, tylluan) y el restore solo
//! volvia "in situ" (se negaba si habia OTRO kernel vivo, aunque el destino
//! fuera otro directorio).
//!
//! Estos tests ejecutan el binario real (`CARGO_BIN_EXE_tylluan-cli`) sobre un
//! DATA_DIR temporal; nunca tocan `E:\tylluan`, ni `:47004`, ni datos reales.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DATA_DBS: &[&str] = &[
    "agent_profiles.db",
    "audit.db",
    "contracts.db",
    "curriculum.db",
    "episodes.db",
    "guild_audit.db",
    "jobs.db",
    "journal.db",
    "mailbox.db",
    "peers.db",
    "pending_dispatches.db",
    "silva.db",
    "task_context.db",
    "tylluan.db",
];

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tylluan-cli")
}

fn make_db(path: &Path, marker: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("CREATE TABLE IF NOT EXISTS t (v TEXT); DELETE FROM t;").unwrap();
    conn.execute("INSERT INTO t (v) VALUES (?1)", [marker]).unwrap();
}

fn read_marker(path: &Path) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap()
}

/// DATA_DIR temporal con las 12 bases + tylluan.toml apuntando a el.
struct Setup {
    root: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        for (i, name) in DATA_DBS.iter().enumerate() {
            make_db(&data.join(name), &format!("marker-{i}"));
        }
        let silva = data.join("silva.db").display().to_string().replace('\\', "/");
        let memory = data.join("tylluan.db").display().to_string().replace('\\', "/");
        std::fs::write(
            root.path().join("tylluan.toml"),
            format!("[silva]\ndb_path = \"{silva}\"\n[memory]\ndb_path = \"{memory}\"\n"),
        )
        .unwrap();
        Self { root }
    }

    fn data(&self) -> PathBuf {
        self.root.path().join("data")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(bin())
            .args(args)
            .current_dir(self.root.path())
            .output()
            .expect("ejecutar tylluan-cli")
    }
}

fn manifest_of(backup: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(backup.join("backup-manifest.json"))
        .expect("el backup debe escribir backup-manifest.json");
    serde_json::from_str(&raw).expect("manifest JSON valido")
}

fn names_of(manifest: &serde_json::Value) -> Vec<String> {
    manifest["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|e| e["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// T-DSH-156/T-DSH-165: el backup por defecto debe cubrir TODAS las bases del DATA_DIR (incluida episodes.db).
/// Antes del arreglo faltaban 7: agent_profiles, contracts, curriculum, jobs,
/// journal, pending_dispatches y task_context.
#[test]
fn backup_covers_all_kernel_databases() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "backup fallo: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = manifest_of(&backup);
    let names = names_of(&manifest);
    let missing: Vec<&str> =
        DATA_DBS.iter().copied().filter(|db| !names.iter().any(|n| n == db)).collect();
    assert!(
        missing.is_empty(),
        "bases del DATA_DIR que faltan en el backup ({}/12 copiadas): {missing:?}",
        DATA_DBS.len() - missing.len()
    );
    assert!(
        manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| !e["secret"].as_bool().unwrap_or(false)),
        "sin --with-secrets el manifiesto no debe marcar secretos"
    );
}

/// T-DSH-156: identity.key y oauth_jwt_secret solo con --with-secrets.
#[test]
fn with_secrets_includes_identity_and_oauth_secret() {
    let setup = Setup::new();
    let data = setup.data();
    std::fs::write(data.join("identity.key"), b"identity-bytes").unwrap();
    std::fs::write(data.join("oauth_jwt_secret"), b"ab".repeat(32)).unwrap();

    // Sin flag: no aparecen.
    let plain = setup.root.path().join("backup-plain");
    let out = setup.run(&["backup", plain.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));
    let names = names_of(&manifest_of(&plain));
    assert!(
        !names.iter().any(|n| n == "identity.key" || n == "oauth_jwt_secret"),
        "sin --with-secrets no deben copiarse ficheros sensibles: {names:?}"
    );

    // Con flag: ambos, marcados como secret y copiados verbatim.
    let with = setup.root.path().join("backup-secrets");
    let out = setup.run(&["backup", with.to_str().unwrap(), "--with-secrets"]);
    assert!(
        out.status.success(),
        "backup --with-secrets fallo: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let manifest = manifest_of(&with);
    let names = names_of(&manifest);
    for secret in ["identity.key", "oauth_jwt_secret"] {
        assert!(names.iter().any(|n| n == secret), "falta {secret}: {names:?}");
        let entry = manifest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"].as_str() == Some(secret))
            .unwrap();
        assert_eq!(entry["secret"], serde_json::Value::Bool(true));
    }
    assert_eq!(std::fs::read(with.join("identity.key")).unwrap(), b"identity-bytes");
}

/// T-DSH-156: restore con --target-dir y --port propios restaura a otro
/// directorio sin negarse por un kernel de otro DATA_DIR (en un equipo de
/// desarrollo con el kernel de produccion vivo, antes del arreglo esto fallaba
/// con "refusing to restore: ... kernel process ... is running").
#[test]
fn restore_target_dir_does_not_depend_on_other_kernel() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));

    // Puerto libre para el kernel "propio" (no hay nada escuchando).
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let target = setup.root.path().join("restored");
    let out = setup.run(&[
        "restore",
        backup.to_str().unwrap(),
        "--target-dir",
        target.to_str().unwrap(),
        "--port",
        &port.to_string(),
    ]);
    assert!(
        out.status.success(),
        "restore --target-dir fallo (no debe depender de otro kernel): stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    for (i, name) in DATA_DBS.iter().enumerate() {
        let restored = target.join(name);
        assert!(restored.exists(), "falta {name} en el destino del restore");
        assert_eq!(
            read_marker(&restored),
            format!("marker-{i}"),
            "contenido restaurado incorrecto en {name}"
        );
    }
}

/// T-DSH-156: la guarda de seguridad se mantiene: si el puerto indicado tiene
/// un /health respondiendo, el restore se niega (kernel del destino vivo).
#[test]
fn restore_refuses_when_target_port_has_health() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            use std::io::{Read, Write};
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            );
        }
    });

    let target = setup.root.path().join("restored-refused");
    let out = setup.run(&[
        "restore",
        backup.to_str().unwrap(),
        "--target-dir",
        target.to_str().unwrap(),
        "--port",
        &port.to_string(),
    ]);
    assert!(!out.status.success(), "el restore debio negarse con /health respondiendo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rechazando restore in situ"),
        "mensaje esperado de rechazo, got: {stderr}"
    );
    assert!(
        stderr.contains(&port.to_string()),
        "el mensaje debe incluir el puerto comprobado, got: {stderr}"
    );
    assert!(!target.exists() || std::fs::read_dir(&target).unwrap().next().is_none());
}

/// T-DSH-158: el kernel resuelve TODAS sus bases (incluida `journal.db`) desde
/// `TYLLUAN_DATA_DIR`; el backup/restore deben hacer lo mismo aunque el cwd sea
/// distinto. Antes del arreglo no habia tylluan.toml en el cwd y el backup
/// resolvia `./data/*` relativo al cwd (vacio) -> fallaba sin copiar nada.
#[test]
fn backup_and_restore_resolve_journal_from_data_dir_env() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("custom-data");
    std::fs::create_dir_all(&data).unwrap();
    for (i, name) in DATA_DBS.iter().enumerate() {
        make_db(&data.join(name), &format!("env-marker-{i}"));
    }
    // A proposito NO hay tylluan.toml: solo el env define el DATA_DIR real.

    let backup = root.path().join("backup-env");
    let out = Command::new(bin())
        .args(["backup", backup.to_str().unwrap()])
        .current_dir(root.path())
        .env("TYLLUAN_DATA_DIR", &data)
        .output()
        .expect("ejecutar tylluan-cli backup");
    assert!(
        out.status.success(),
        "backup con TYLLUAN_DATA_DIR fallo: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let names = names_of(&manifest_of(&backup));
    assert!(
        names.iter().any(|n| n == "journal.db"),
        "journal.db debe resolverse desde TYLLUAN_DATA_DIR (no desde el cwd): {names:?}"
    );
    for db in DATA_DBS {
        assert!(names.iter().any(|n| n == db), "falta {db} en el backup: {names:?}");
    }

    // Restore a otro directorio con su propio puerto: journal.db tambien viaja.
    let target = root.path().join("restored-env");
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let out = Command::new(bin())
        .args([
            "restore",
            backup.to_str().unwrap(),
            "--target-dir",
            target.to_str().unwrap(),
            "--port",
            &port.to_string(),
        ])
        .current_dir(root.path())
        .env("TYLLUAN_DATA_DIR", &data)
        .output()
        .expect("ejecutar tylluan-cli restore");
    assert!(
        out.status.success(),
        "restore con TYLLUAN_DATA_DIR fallo: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let journal = target.join("journal.db");
    assert!(journal.exists(), "journal.db debe restaurarse en el target-dir");
    let expected = format!("env-marker-{}", DATA_DBS.iter().position(|d| *d == "journal.db").unwrap());
    assert_eq!(read_marker(&journal), expected, "contenido de journal.db restaurado");
}

/// T-DSH-159: restore IN SITU se niega si `/health` responde, con mensaje en
/// espanol que incluye el puerto comprobado y el PID (aqui no hay fichero PID:
/// "PID no identificado").
#[test]
fn in_place_restore_refuses_when_health_responds() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup-inplace-health");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            use std::io::{Read, Write};
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            );
        }
    });

    let out = setup.run(&[
        "restore",
        backup.to_str().unwrap(),
        "--port",
        &port.to_string(),
    ]);
    assert!(!out.status.success(), "el restore in situ debio negarse con /health respondiendo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rechazando restore in situ"),
        "mensaje esperado de rechazo, got: {stderr}"
    );
    assert!(stderr.contains(&port.to_string()), "el mensaje debe incluir el puerto: {stderr}");
    assert!(stderr.contains("PID"), "el mensaje debe incluir el PID (o 'no identificado'): {stderr}");
}

/// T-DSH-159: restore IN SITU se niega si el fichero PID del DATA_DIR apunta a
/// un proceso vivo, y el mensaje incluye ese PID y el puerto.
#[test]
fn in_place_restore_refuses_when_pid_file_is_alive() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup-inplace-pid");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));

    let pid = std::process::id();
    std::fs::write(setup.data().join("tylluan-nexus.pid"), pid.to_string()).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let out = setup.run(&[
        "restore",
        backup.to_str().unwrap(),
        "--port",
        &port.to_string(),
    ]);
    assert!(!out.status.success(), "el restore in situ debio negarse con PID vivo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rechazando restore in situ"),
        "mensaje esperado de rechazo, got: {stderr}"
    );
    assert!(stderr.contains(&format!("PID {pid}")), "el mensaje debe incluir PID {pid}: {stderr}");
    assert!(stderr.contains(&port.to_string()), "el mensaje debe incluir el puerto: {stderr}");
}

/// T-DSH-159: si nada responde en el puerto y no hay kernel de este DATA_DIR,
/// el restore IN SITU funciona (aunque haya un kernel de OTRO DATA_DIR vivo).
#[test]
fn in_place_restore_works_when_nothing_responds() {
    let setup = Setup::new();
    let backup = setup.root.path().join("backup-inplace-ok");
    let out = setup.run(&["backup", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup fallo: {}", String::from_utf8_lossy(&out.stderr));

    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let out = setup.run(&[
        "restore",
        backup.to_str().unwrap(),
        "--port",
        &port.to_string(),
    ]);
    assert!(
        out.status.success(),
        "el restore in situ debio funcionar sin kernel de este DATA_DIR: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let journal = setup.data().join("journal.db");
    assert!(journal.exists(), "journal.db debe seguir en el DATA_DIR tras el restore in situ");
}

/// T-DSH-168: el inventario del backup debe cubrir TODAS las bases del
/// DATA_DIR. Si alguien anade una base al kernel y olvida registrarla aqui
/// (o borra `episodes.db`), este test falla y el simulacro de recuperacion
/// no puede darse por bueno.
#[test]
fn data_dbs_inventory_is_complete_or_the_drill_fails() {
    const REQUIRED: &[&str] = &[
        "agent_profiles.db",
        "audit.db",
        "contracts.db",
        "curriculum.db",
        "episodes.db",
        "guild_audit.db",
        "jobs.db",
        "journal.db",
        "mailbox.db",
        "peers.db",
        "pending_dispatches.db",
        "silva.db",
        "task_context.db",
        "tylluan.db",
    ];
    for name in REQUIRED {
        assert!(
            DATA_DBS.contains(name),
            "falta la base '{name}' en tylluan-cli::backup::DATA_DBS: el backup/restore no la cubriria"
        );
    }
    assert_eq!(
        DATA_DBS.len(),
        REQUIRED.len(),
        "DATA_DBS tiene {} bases y el inventario esperado es {}: revisa la lista",
        DATA_DBS.len(),
        REQUIRED.len()
    );
}
