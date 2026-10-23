//! T-DSH-38 — auditoría del entorno que recibe una guild al arrancar (prepara D-34).
//!
//! `GuildProcess::spawn` construye el `Command` de la guild y, tras el `match`,
//! inyecta `TYLLUAN_TOKEN` (guild_process.rs ~478-491). No llama a `env_clear()`:
//! el hijo **hereda todo el entorno del kernel**. Este test arranca una guild
//! MCP mínima (servidor stdio de una línea, en un directorio temporal) que
//! vuelca su `os.environ` a un JSON, y fija el comportamiento ACTUAL:
//!
//! (a) qué variables `TYLLUAN_*` recibe;
//! (b) si recibe el token maestro (`TYLLUAN_TOKEN` con el valor resuelto por el kernel);
//! (c) si recibe secretos del proceso padre (variables con TOKEN/KEY/SECRET/PASSWORD).
//!
//! Both tests are LIVE since D-34: the guild gets no master token, no parent
//! secrets, and one ephemeral `TYLLUAN_GUILD_TOKEN` credential.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test guild_env_audit -- --nocapture
//!
//! No arranca el kernel ni toca puertos; sin `cfg(unix)`/`cfg(windows)` en
//! ninguna función; `src/` intacto.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::registry::guild_process::{GuildLauncher, GuildProcess};

/// Servidor MCP stdio mínimo: vuelca el entorno y responde initialize/tools/list.
const PROBE_SERVER: &str = r#"
import json, os, sys
out = sys.argv[1]
with open(out, "w", encoding="utf-8") as f:
    json.dump(dict(os.environ), f)

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except Exception:
        continue
    if "id" not in msg:
        continue
    method = msg.get("method")
    params = msg.get("params") or {}
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {
            "protocolVersion": params.get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "d34-env-probe", "version": "0.0.1"}}})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"tools": []}})
    else:
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
"#;

/// Valor del token maestro que el test pone en el entorno del kernel (padre).
const SENTINEL_TOKEN: &str = "d34-master-token-sentinel";

/// Secretos de padre con los nombres que pide la tarea (TOKEN/KEY/SECRET/PASSWORD).
const PARENT_SECRETS: &[(&str, &str)] = &[
    ("D34_PARENT_TOKEN_KEY", "parent-secret-token"),
    ("D34_PARENT_API_KEY", "parent-secret-key"),
    ("D34_PARENT_PASSWORD", "parent-secret-password"),
    ("D34_PARENT_CREDENTIAL_SECRET", "parent-secret-value"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Arranca una guild MCP mínima por el camino REAL (`GuildProcess::spawn`,
/// launcher External) y devuelve su entorno tal y como lo vio el hijo.
async fn spawn_probe_guild(tag: &str) -> (HashMap<String, String>, GuildProcess) {
    let dir = std::env::temp_dir().join(format!("d34-env-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("crear temp dir");
    let script = dir.join("probe_mcp_server.py");
    let dump = dir.join("child_env.json");
    std::fs::write(&script, PROBE_SERVER).expect("escribir probe server");

    let guild_name = format!("d34-env-probe-{tag}");
    let mut process = GuildProcess::new(
        &guild_name,
        GuildLauncher::External {
            command: "python".to_string(),
            args: vec![
                script.to_string_lossy().to_string(),
                dump.to_string_lossy().to_string(),
            ],
            cwd: Some(dir.clone()),
            env: None,
        },
        false,
        Some(20_000),
        1,
    );
    let guilds_dir = repo_root();
    process
        .spawn(&guilds_dir, &TimeoutsConfig::default())
        .await
        .unwrap_or_else(|e| panic!("spawn de la guild mínima falló: {e}"));

    // El volcado ocurre antes del handshake; espera breve por si el FS va lento.
    let mut text = String::new();
    for _ in 0..50 {
        match std::fs::read_to_string(&dump) {
            Ok(t) if !t.trim().is_empty() => {
                text = t;
                break;
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    assert!(!text.is_empty(), "la guild no volcó su entorno en {}", dump.display());
    let env: HashMap<String, String> =
        serde_json::from_str(&text).expect("el volcado de entorno no es JSON de strings");
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_dir(&dir);
    (env, process)
}

fn is_secret_name(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    k.contains("TOKEN") || k.contains("KEY") || k.contains("SECRET") || k.contains("PASSWORD")
}

fn mask(value: &str) -> String {
    if value.is_empty() {
        "(vacío)".to_string()
    } else if value.len() <= 4 {
        "****".to_string()
    } else {
        format!("{}… ({} bytes)", &value[..4], value.len())
    }
}

/// ¿El nombre de la variable y el valor vienen del token maestro inyectado?
fn is_master_token(env: &HashMap<String, String>, parent_token: &str) -> bool {
    env.get("TYLLUAN_TOKEN").map(|v| v == parent_token).unwrap_or(false)
}

/// **D-34 (live):** the guild receives its own ephemeral credential and
/// inherits neither the master token nor the parent's secrets.
#[tokio::test(flavor = "multi_thread")]
async fn guild_env_is_cleared_and_carries_only_its_own_credential() {
    // Entorno del "kernel" (este proceso) con token maestro y secretos conocidos.
    // SAFETY: variables de proceso; el binario de test solo corre estos dos tests
    // y los ignorados no se ejecutan en la suite normal.
    unsafe {
        std::env::set_var("TYLLUAN_TOKEN", SENTINEL_TOKEN);
        for (k, v) in PARENT_SECRETS {
            std::env::set_var(k, v);
        }
    }

    let (env, mut process) = spawn_probe_guild("current").await;

    println!(
        "{:<38} {:<10} {:<26} motivo",
        "variable", "recibida", "valor"
    );
    let mut tylluan_keys: Vec<&String> = env.keys().filter(|k| k.starts_with("TYLLUAN")).collect();
    tylluan_keys.sort();
    for k in &tylluan_keys {
        println!(
            "{:<38} {:<10} {:<26} motivo",
            k,
            "SI",
            mask(env.get(*k).map(String::as_str).unwrap_or(""))
        );
    }
    println!(
        "{:<38} {:<10} {:<26} harness del test",
        "TYLLUAN_TOKEN (valor)", if env.contains_key("TYLLUAN_TOKEN") { "SI" } else { "no" }, mask(SENTINEL_TOKEN)
    );

    // TYLLUAN_TOKEN ya se muestra en la tabla de TYLLUAN_*; no lo dupliques aquí.
    let mut secret_keys: Vec<&String> = env
        .keys()
        .filter(|k| is_secret_name(k) && !k.starts_with("TYLLUAN"))
        .collect();
    secret_keys.sort();
    for k in &secret_keys {
        println!(
            "{:<38} {:<10} {:<26} {}",
            k,
            "SI",
            mask(env.get(*k).map(String::as_str).unwrap_or("")),
            if k.starts_with("D34_") {
                "LEAK: inherited from the kernel environment"
            } else {
                "LEAK: inherited from the parent process"
            }
        );
    }

    // (b) D-34: no master token, under any spelling.
    assert!(
        !is_master_token(&env, SENTINEL_TOKEN),
        "D-34: the guild must not receive the master token"
    );
    assert!(
        !env.contains_key("TYLLUAN_TOKEN"),
        "D-34: TYLLUAN_TOKEN must never be injected into a guild"
    );

    // (c) D-34: parent secrets do not cross the boundary (env_clear + whitelist).
    for (k, _) in PARENT_SECRETS {
        assert!(
            !env.contains_key(*k),
            "D-34: the guild inherited parent secret '{k}'"
        );
    }

    // (a) D-34: it DOES get one ephemeral credential of its own class.
    let guild_token = env
        .get("TYLLUAN_GUILD_TOKEN")
        .expect("D-34: the guild must receive TYLLUAN_GUILD_TOKEN");
    assert!(
        guild_token.starts_with("tyg_"),
        "D-34: the guild credential must carry the tyg_ prefix"
    );
    assert_ne!(guild_token, SENTINEL_TOKEN);

    let _ = process.kill().await;
}

/// **Comportamiento DESEADO para D-34 (ignorado):** la guild recibe una
/// credencial de clase agente, no el token maestro, y no hereda secretos.
#[tokio::test(flavor = "multi_thread")]
async fn desired_guild_env_does_not_receive_master_token_or_parent_secrets() {
    unsafe {
        std::env::set_var("TYLLUAN_TOKEN", SENTINEL_TOKEN);
        for (k, v) in PARENT_SECRETS {
            std::env::set_var(k, v);
        }
    }

    let (env, mut process) = spawn_probe_guild("desired").await;

    assert!(
        !env.contains_key("TYLLUAN_TOKEN") || !is_master_token(&env, SENTINEL_TOKEN),
        "D-34 pendiente: la guild sigue recibiendo el token maestro como TYLLUAN_TOKEN"
    );
    for (k, _) in PARENT_SECRETS {
        assert!(
            !env.contains_key(*k),
            "D-34 pendiente: la guild hereda el secreto del padre '{k}' (Command sin env_clear)"
        );
    }

    let _ = process.kill().await;
}
