//! T-DSH-25 — Test de Continuidad (Q1): sondas propias sin LLM, contra el router real.
//!
//! Reinicio SIMULADO: se construye el estado con `DATA_DIR` en una carpeta
//! temporal, se sueltan router y estado, y se reconstruye desde el mismo
//! directorio. No se arranca ningún proceso (regla de flota).
//!
//! Sondas: (a) identidad tras reinicio, (b) tarea abierta (mensaje de buzón sin
//! leer; el plan pendiente queda documentado como no persistente), (c) memoria
//! (normal y marcada `untrusted`), (d) fecha-hora real y viva.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test continuity_probes -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tokio::sync::RwLock;
use tower::ServiceExt;

use tylluan_kernel::config::{TimeoutsConfig, TylluanConfig};
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::memory::coloquio::ColoquioDb;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::actor::RegistryActor;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::server::TylluanServer;

const MASTER: &str = "master-t25-token";

fn fresh_dir(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("tylluan_t25_{tag}_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("data dir");
    dir
}

fn p(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().to_string()
}

/// `HttpState` completo con almacenes EN FICHERO dentro de `dir` (silva,
/// buzón, coloquio, journal, peers, contratos, jobs). Es el equivalente
/// persistente del harness `support::test_state_custom`, que es en memoria.
async fn try_build_state(dir: &Path, tag: &str) -> anyhow::Result<Arc<HttpState>> {
    let reg_root = dir.join(format!("registry_{tag}"));
    std::fs::create_dir_all(&reg_root)?;
    let registry_raw = GuildRegistry::new(reg_root, 5, TimeoutsConfig::default(), 5);
    let registry_arc = Arc::new(RwLock::new(registry_raw));
    let (registry_actor, registry_handle) = RegistryActor::new(registry_arc.clone());
    tokio::spawn(async move {
        registry_actor.run().await;
    });
    {
        let mut reg = registry_arc.write().await;
        for g in tylluan_kernel::router::catalog::builtin_catalog() {
            reg.register(&g.name, &g.module_path, false, None);
        }
    }

    let silva = Arc::new(SilvaDB::open(&p(dir, "silva.db"))?);
    silva.init().await?;
    let mailbox = Arc::new(Mailbox::open(&p(dir, "mailbox.db"))?);
    mailbox.init().await?;
    let coloquio = Arc::new(ColoquioDb::new(&p(dir, "coloquio.db"))?);
    let curriculum = Arc::new(std::sync::Mutex::new(
        tylluan_kernel::curriculum::CurriculumLearner::new_in_memory(1)?,
    ));
    let doctor = Arc::new(Doctor::new(registry_arc.clone(), silva.clone(), curriculum));
    let matcher = Arc::new(GuildMatcher::new(tylluan_kernel::router::catalog::builtin_catalog()));
    let node_router =
        tylluan_kernel::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(1).0);
    let server = TylluanServer::new(
        registry_arc,
        matcher.clone(),
        silva.clone(),
        mailbox.clone(),
        doctor.clone(),
        node_router.clone(),
    );
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(10);
    let (download_tx, _) = tokio::sync::broadcast::channel(10);
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_map = tylluan_kernel::repo_map::RepoMapHandle::ready(tylluan_kernel::repo_map::RepoMap::build(&cwd));

    Ok(Arc::new(HttpState {
        task_context: Arc::new(tylluan_kernel::memory::task_context::TaskContextStore::in_memory()?),
        version: "test-t25".to_string(),
        auth_token: Some(MASTER.to_string()),
        dev_mode: Some(false),
        start_time: Instant::now(),
        server: Some(Arc::new(RwLock::new(server))),
        registry: registry_handle,
        doctor,
        silva: silva.clone(),
        mailbox,
        coloquio,
        broadcast_tx,
        download_progress_tx: download_tx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
        agent_rate_limiter: Arc::new(dashmap::DashMap::new()),
        ip_rate_limiter: Arc::new(tylluan_kernel::security::rate_limiter::RateLimiter::new(Some(300))),
        config: TylluanConfig::load_cached()
            .unwrap_or_else(|_| Arc::new(RwLock::new(TylluanConfig::default()))),
        matcher,
        tunnel_wsl_url: None,
        oauth: Arc::new(
            tylluan_kernel::transport::http::oauth::OAuthState::new("http://localhost:47004".to_string())
                .with_dev_mode(false)
                .with_operator_credential(Some(MASTER.to_string())),
        ),
        metrics_ring: Arc::new(RwLock::new(tylluan_kernel::metrics_ring::MetricsRingBuffer::new())),
        jobs: Arc::new(tylluan_kernel::memory::jobs::JobQueue::open(Path::new(&p(dir, "jobs.db")))?),
        cancel_token: tokio_util::sync::CancellationToken::new(),
        node_router,
        health_ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        journal: Arc::new(
            tylluan_kernel::transport::http::api_v1::api_journal::JournalDb::open(&p(dir, "journal.db"))?,
        ),
        agent_registry: tylluan_kernel::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
        contract_registry: tylluan_kernel::transport::http::api_v1::api_contracts::ContractRegistry::new(),
        contract_db: Arc::new(
            tylluan_kernel::transport::http::api_v1::api_contracts::ContractDb::open(&p(dir, "contracts.db"))?,
        ),
        peer_db: Arc::new(tylluan_kernel::federation::PeerDb::open(&p(dir, "peers.db"))?),
        node_identity: Arc::new(tylluan_link::identity::NodeIdentity::load_or_create(
            &dir.join("identity.key"),
        )?),
        nat_cache: Arc::new(RwLock::new(None)),
        dht_routing_table: Arc::new(RwLock::new(tylluan_link::dht::RoutingTable::new(
            "t25-node".to_string(),
        ))),
        p2p_pool: Arc::new(tokio::sync::Mutex::new(tylluan_link::p2p::P2pSessionPool::new(16, 300))),
        gossip_engine: Arc::new(RwLock::new(tylluan_link::gossip::GossipEngine::new(
            "t25-node".to_string(),
            tylluan_link::gossip::GossipConfig::default(),
        ))),
        capability_registry: Arc::new(std::sync::Mutex::new(
            tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)),
        )),
        dispatch_router: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchRouter::new(
            Arc::new(std::sync::Mutex::new(
                tylluan_link::capability::CapabilityRegistry::new(std::time::Duration::from_secs(300)),
            )),
            std::time::Duration::from_secs(60),
        ))),
        dispatch_queue: Arc::new(std::sync::Mutex::new(tylluan_link::dispatch::DispatchQueue::new(1000))),
        repo_map,
        a2a_task_manager: Arc::new(tylluan_kernel::transport::http::a2a::A2aTaskManager::new(silva.clone())),
        a2a_agents: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aAgentStore::new(silva.clone())),
        a2a_client: Arc::new(tylluan_kernel::transport::http::a2a_client::A2aClient::new()?),
        agents_contract: Arc::new(tylluan_kernel::security::agents_contract::AgentsContract::empty()),
    }))
}

/// Reconstruye desde el mismo directorio, con reintentos: en Windows el fichero
/// SQLite puede tardar un instante en soltarse tras el drop del estado anterior.
async fn build_state(dir: &Path, tag: &str) -> Arc<HttpState> {
    let mut last = String::new();
    for _ in 0..20 {
        match try_build_state(dir, tag).await {
            Ok(s) => return s,
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
    panic!("no se pudo reconstruir el estado desde {}: {last}", dir.display());
}

fn once_init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        tylluan_kernel::security::grants::init();
        tylluan_kernel::security::grants::init_plan_store();
    });
}

async fn mcp_call(app: &axum::Router, tool: &str, arguments: serde_json::Value) -> (StatusCode, String) {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn post_json(app: &axum::Router, uri: &str, body: serde_json::Value) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20).await.unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// Texto del primer `content` de una respuesta MCP `tools/call`.
fn text_of(mcp_body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(mcp_body)
        .ok()
        .and_then(|v| v["result"]["content"][0]["text"].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn json_of(mcp_body: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(&text_of(mcp_body)).unwrap_or(serde_json::Value::Null)
}

async fn whoami(app: &axum::Router, agent: &str) -> serde_json::Value {
    let (_st, body) = mcp_call(
        app,
        "tylluan_do",
        serde_json::json!({ "intent": "whoami", "agent_id": agent }),
    )
    .await;
    json_of(&body)
}

// ───────────────────────── sonda (a): identidad ─────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn t25_a_identity_survives_restart() {
    once_init();
    let dir = fresh_dir("a");
    let state = build_state(&dir, "a").await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    // Secuencia realista: primer contacto (auto-bootstrap de identidad),
    // después el registro con biografía real. El auto-bootstrap se lanza con
    // `tokio::spawn` (handlers.rs línea 51-66) y solo escribe si NO hay
    // identidad: si se registra en el PRIMER contacto, la carrera puede dejar
    // el placeholder encima (ver O1 del informe).
    let (_s0, _w0) = mcp_call(
        &app,
        "tylluan_do",
        serde_json::json!({ "intent": "whoami", "agent_id": "probe-t25" }),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    let (_s, reg) = mcp_call(
        &app,
        "tylluan_do",
        serde_json::json!({
            "intent": "register_identity",
            "agent_id": "probe-t25",
            "human_name": "Dora T25",
            "role": "Probe Continuity",
            "purpose": "medir continuidad tras reinicio"
        }),
    )
    .await;
    assert!(text_of(&reg).contains("Identity registered"), "registro falló: {reg}");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let before = whoami(&app, "probe-t25").await;

    drop(app);
    drop(state);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let state2 = build_state(&dir, "a").await;
    let app2 = tylluan_kernel::transport::http::build_router(state2.clone());
    let after = whoami(&app2, "probe-t25").await;

    assert_eq!(before["agent_id"], after["agent_id"], "agent_id distinto tras reinicio");
    let bio_before = before["biography"].as_str().unwrap_or("");
    let bio_after = after["biography"].as_str().unwrap_or("");
    assert!(!bio_before.is_empty(), "sin biografía antes: {before}");
    assert_eq!(bio_before, bio_after, "rol/fecha de creación distintos tras reinicio");
    assert!(bio_after.contains("Probe Continuity"), "rol perdido: {bio_after}");
    let node = state2.silva.get_node("agent:probe-t25").await.ok().flatten();
    assert!(node.is_some(), "nodo de identidad ausente tras reinicio");

    println!("| a | identidad tras reinicio | mismo agent_id, rol y fecha | igual: {bio_after} |");
}

// ───────────────────── sonda (b): tarea abierta (buzón) ─────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn t25_b_mailbox_message_survives_restart() {
    once_init();
    let dir = fresh_dir("b");
    let state = build_state(&dir, "b").await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    // Remitente local (registrado) para que el mensaje no nazca marcado.
    let (_s, _reg) = mcp_call(
        &app,
        "tylluan_do",
        serde_json::json!({
            "intent": "register_identity",
            "agent_id": "probe-sender-t25",
            "human_name": "Remitente T25",
            "role": "Probe Sender",
            "purpose": "enviar un mensaje que debe sobrevivir"
        }),
    )
    .await;
    let marker = "T25-MAILBOX-sobrevive-al-reinicio";
    let (st, sent) = post_json(
        &app,
        "/api/v1/mailbox/send",
        serde_json::json!({
            "from": "probe-sender-t25",
            "to": "probe-t25",
            "subject": "T25 continuidad",
            "body": marker,
            "ttlSecs": 86400
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "envío al buzón falló: {sent}");

    drop(app);
    drop(state);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let state2 = build_state(&dir, "b").await;
    let app2 = tylluan_kernel::transport::http::build_router(state2.clone());
    let (_s2, inbox) = mcp_call(
        &app2,
        "tylluan_recall",
        serde_json::json!({ "query": "@inbox", "agent_id": "probe-t25" }),
    )
    .await;
    let inbox_text = text_of(&inbox);
    assert!(
        inbox_text.contains(marker),
        "el mensaje del buzón no sobrevivió: {inbox_text}"
    );
    assert!(
        !inbox_text.contains("[CONTENIDO NO CONFIABLE"),
        "remitente local registrado no debe llevar banner: {inbox_text}"
    );

    println!("| b | mensaje de buzón sin leer tras reinicio | legible | ok |");
}

/// HALLAZGO (documentado, no ejecutable in-process): el store de planes
/// pendientes es `static PLAN_STORE: OnceLock<RwLock<HashMap<..>>>` en
/// `security/grants.rs` (línea 46-58): vive en el PROCESO, no en disco. Un
/// reinicio real (proceso nuevo) pierde los planes pendientes; un "reinicio"
/// in-process no los pierde porque el estático sigue vivo, así que este assert
/// (la expectativa honesta de reinicio real) no puede pasar aquí.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "PLAN_STORE es un OnceLock en proceso (security/grants.rs línea 46-58): los planes pendientes NO persisten entre procesos; in-process no es medible"]
async fn t25_b2_pending_plan_does_not_survive_a_real_restart() {
    once_init();
    let dir = fresh_dir("b2");
    let state = build_state(&dir, "b2").await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    let (_s, planned) = mcp_call(
        &app,
        "tylluan_do",
        serde_json::json!({ "intent": "check system CPU usage", "plan": true }),
    )
    .await;
    let _ = text_of(&planned);
    // El plan se crea por la API real del store (la ruta MCP con `plan: true`
    // necesita arrancar el guild, que en tests no levanta handshake MCP).
    tylluan_kernel::security::grants::store_plan(
        "t25-plan",
        "system_metrics",
        "check",
        &serde_json::json!({}),
        "probe-t25",
        "check system CPU usage",
    )
    .await;
    assert!(
        tylluan_kernel::security::grants::get_plan("t25-plan").await.is_some(),
        "el plan debería existir antes del reinicio"
    );

    drop(app);
    drop(state);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let state2 = build_state(&dir, "b2").await;
    let app2 = tylluan_kernel::transport::http::build_router(state2.clone());
    let (_s2, listed) = mcp_call(
        &app2,
        "tylluan_do",
        serde_json::json!({ "intent": "list_pending_actions" }),
    )
    .await;
    let listed_text = text_of(&listed);
    let plan_gone = tylluan_kernel::security::grants::get_plan("t25-plan").await.is_none();
    assert!(
        plan_gone && !listed_text.contains("t25-plan"),
        "reinicio real: el plan pendiente no debería seguir ahí; in-process sigue por el estático: {listed_text}"
    );
}

// ───────────────────────── sonda (c): memoria ───────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn t25_c_memory_and_untrusted_mark_survive_restart() {
    once_init();
    let dir = fresh_dir("c");
    let state = build_state(&dir, "c").await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    let normal = "T25-MEM-NORMAL-recuerdo-que-sobrevive";
    let untrusted = "T25-MEM-UNTRUSTED-contenido-externo";
    // Sin `agent_id`: el camino con agent_id guarda un nodo `agent_memory` y
    // descarta el metadata del llamante (O2 del informe); sin él, el recuerdo
    // es un nodo `memory` de Silva con el metadata tal cual (incluida la marca).
    let (_s1, r1) = mcp_call(
        &app,
        "tylluan_remember",
        serde_json::json!({ "content": normal }),
    )
    .await;
    assert!(!text_of(&r1).is_empty(), "remember normal vacío: {r1}");
    let (_s2, r2) = mcp_call(
        &app,
        "tylluan_remember",
        serde_json::json!({
            "content": untrusted,
            "metadata": { "untrusted_content": true, "untrusted_reason": "probe-t25 externo" }
        }),
    )
    .await;
    assert!(!text_of(&r2).is_empty(), "remember untrusted vacío: {r2}");

    drop(app);
    drop(state);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let state2 = build_state(&dir, "c").await;
    let app2 = tylluan_kernel::transport::http::build_router(state2.clone());

    // Evidencia directa: el nodo marcado conserva su metadata tras el reinicio.
    let mut marked_meta: Option<serde_json::Value> = None;
    if let Ok(hits) = state2
        .silva
        .search_hybrid(untrusted, None, 10, Some("memory"), true)
        .await
    {
        for (n, _score) in hits {
            if n.content.contains(untrusted) {
                marked_meta = serde_json::from_str(&n.metadata).ok();
            }
        }
    }
    let marked_flag = marked_meta
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    assert!(marked_flag, "metadata untrusted ausente tras reinicio: {marked_meta:?}");

    let (_s3, rec_normal) = mcp_call(
        &app2,
        "tylluan_recall",
        serde_json::json!({ "query": normal }),
    )
    .await;
    let normal_text = text_of(&rec_normal);
    assert!(normal_text.contains(normal), "el recuerdo normal no sobrevivió: {normal_text}");
    assert!(
        !normal_text.contains("[CONTENIDO NO CONFIABLE"),
        "un recuerdo local no debe llevar banner: {normal_text}"
    );

    let (_s4, rec_untrusted) = mcp_call(
        &app2,
        "tylluan_recall",
        serde_json::json!({ "query": untrusted }),
    )
    .await;
    let untrusted_text = text_of(&rec_untrusted);
    assert!(
        untrusted_text.contains(untrusted),
        "el recuerdo marcado no sobrevivió: {untrusted_text}"
    );
    // La marca persiste en metadata (arriba) pero `tylluan_recall` NO la
    // surfacea como banner: en handler_recall el banner solo cubre el resumen
    // de sesión (línea 195) y los mensajes externos del buzón (línea 260), no
    // los resultados de búsqueda normales. Se mide y se documenta como O2.
    let recall_banner = untrusted_text.contains("[CONTENIDO NO CONFIABLE");

    // `tylluan_think` sí aplica mark_text a los nodos marcados (handler_think.rs
    // línea 388): comprobamos que la marca sigue siendo visible tras el reinicio.
    let (_s5, think) = mcp_call(&app2, "tylluan_think", serde_json::json!({ "query": untrusted })).await;
    let think_text = text_of(&think);
    let think_banner = think_text.contains("[CONTENIDO NO CONFIABLE");
    assert!(
        think_banner,
        "la marca untrusted no es visible por ninguna superficie (think): {think_text}"
    );

    println!(
        "| c | memoria normal + marcada untrusted | sobreviven; marca en metadata y visible por think | recall_banner={recall_banner} think_banner={think_banner} |"
    );
}

// ───────────────────── sonda (d): fecha-hora real y viva ────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn t25_d_whoami_reports_live_time() {
    once_init();
    let dir = fresh_dir("d");
    let state = build_state(&dir, "d").await;
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    let before = whoami(&app, "probe-t25").await;
    let utc_before = before["now"]["utc"].as_str().unwrap_or("").to_string();
    let t_before = chrono::DateTime::parse_from_rfc3339(&utc_before).expect("utc rfc3339");
    let drift_before = (chrono::Utc::now() - t_before.with_timezone(&chrono::Utc)).num_seconds().abs();
    assert!(drift_before <= 5, "whoami no da la hora real (drift {drift_before}s): {utc_before}");

    drop(app);
    drop(state);
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let state2 = build_state(&dir, "d").await;
    let app2 = tylluan_kernel::transport::http::build_router(state2.clone());
    let after = whoami(&app2, "probe-t25").await;
    let utc_after = after["now"]["utc"].as_str().unwrap_or("").to_string();
    let t_after = chrono::DateTime::parse_from_rfc3339(&utc_after).expect("utc rfc3339");
    let drift_after = (chrono::Utc::now() - t_after.with_timezone(&chrono::Utc)).num_seconds().abs();
    assert!(drift_after <= 5, "whoami tras reinicio no da la hora real (drift {drift_after}s)");
    assert!(
        t_after > t_before,
        "la hora tras el reinicio no avanzó: {utc_before} -> {utc_after}"
    );

    println!("| d | fecha-hora de whoami | hora real (±5 s) y avanza tras reinicio | {utc_before} -> {utc_after} |");
}
