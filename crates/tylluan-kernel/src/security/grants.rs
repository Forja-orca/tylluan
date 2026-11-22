use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::{RwLock, oneshot};


static GRANT_REGISTRY: OnceLock<RwLock<GrantStore>> = OnceLock::new();
static GRANT_NOTIFIER: OnceLock<tokio::sync::broadcast::Sender<serde_json::Value>> = OnceLock::new();

pub type GrantId = String;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GrantLevel {
    #[serde(rename = "this_time")]
    ThisTime,
    #[serde(rename = "this_session")]
    ThisSession,
    #[serde(rename = "always_for_guild")]
    AlwaysForGuild,
}

impl GrantLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            GrantLevel::ThisTime => "this_time",
            GrantLevel::ThisSession => "this_session",
            GrantLevel::AlwaysForGuild => "always_for_guild",
        }
    }
}

pub struct GrantRequest {
    pub guild: String,
    pub tool_name: String,
    pub agent_id: String,
    pub reason: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub tx: oneshot::Sender<GrantLevel>,
    pub expires_at: tokio::time::Instant,
}

struct GrantStore {
    pending: HashMap<GrantId, GrantRequest>,
}

// M31-P2: Plan mode — stores resolved guild+tool+args for approval before execution
static PLAN_STORE: OnceLock<RwLock<HashMap<String, PendingPlan>>> = OnceLock::new();

/// D-30: los planes pendientes de aprobación sobreviven a un reinicio.
///
/// Antes vivían solo en memoria: un plan aprobado por el operador podía ejecutar
/// una herramienta que ya no existía, y uno **pendiente** desaparecía sin dejar
/// rastro (el operador no podía aprobarlo nunca y el agente no sabía por qué).
/// Ahora se persisten en `<data>/pending_plans.json` con escritura atómica
/// (tmp + rename, `0600` en Unix) y `init_plan_store` los recarga al arrancar.
///
/// Un plan caduca a las [`PLAN_TTL_SECS`]: una aprobación vieja no debe ejecutar
/// nada al día siguiente por un reinicio.
pub const PLANS_FILE: &str = "pending_plans.json";
pub const PLAN_TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPlan {
    pub guild: String,
    pub tool: String,
    pub args: serde_json::Map<String, serde_json::Value>,
    pub agent_id: String,
    pub intent: String,
    /// Segundos Unix de creación; `0` (planes antiguos sin el campo) se trata
    /// como recién creado al cargar, no se descarta.
    #[serde(default)]
    pub created_at_unix: i64,
    /// Jose 2026-11-15: un plan puede quedar **en duda** (`hold`) sin aprobarse
    /// ni rechazarse. No consume el plan: sigue pendiente y escala al humano con
    /// lo que falta. `None` = nadie ha dudado de él.
    #[serde(default)]
    pub held: Option<HeldNote>,
}

/// Marca de duda sobre un plan pendiente.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HeldNote {
    /// Qué falta para poder juzgarlo (texto del agente o del revisor).
    pub reason: String,
    /// Segundos Unix en que se puso en duda.
    pub at_unix: i64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PendingPlansFile {
    version: u32,
    plans: HashMap<String, PendingPlan>,
}

/// Serializa las escrituras del fichero de planes (dos aprobaciones a la vez
/// compartirían el `.tmp` y una podría perder la otra).
static PLAN_PERSIST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Jose 2026-11-15: contadores de **criterio**, no de aprobación.
///
/// Si el único número que existe es «aprobaciones», el sistema empuja a aprobar.
/// Estos dos cuentan lo contrario: planes que alguien puso en duda y planes que
/// su propio autor retiró antes de que nadie los aprobara (eso es acierto, no
/// trabajo perdido).
static PLANS_HELD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PLANS_WITHDRAWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Directorio de datos para los planes.
///
/// Orden: `TYLLUAN_DATA_DIR`, `DATA_DIR` (tests y lanzadores) y, si no hay
/// variable, el `data/` del directorio de trabajo **solo si ya existe** — el
/// kernel lo crea con sus bases de datos, así que en un binario de test sin
/// entorno no se inventa un `data/` en la raíz del crate. Sin directorio, los
/// planes son solo de proceso (comportamiento anterior).
fn plans_dir() -> Option<PathBuf> {
    for key in ["TYLLUAN_DATA_DIR", "DATA_DIR"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    let fallback = PathBuf::from("data");
    fallback.is_dir().then_some(fallback)
}

fn plans_path() -> Option<PathBuf> {
    plans_dir().map(|dir| dir.join(PLANS_FILE))
}

#[cfg(unix)]
fn write_private(path: &Path, body: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(body.as_bytes())?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, body: &str) -> std::io::Result<()> {
    std::fs::write(path, body)
}

fn load_plans() -> HashMap<String, PendingPlan> {
    let Some(path) = plans_path() else {
        return HashMap::new();
    };
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            tracing::warn!(
                "plan store: no se pudo leer {} ({e}); planes pendientes solo en memoria",
                path.display()
            );
            return HashMap::new();
        }
    };
    let parsed: PendingPlansFile = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(e) => {
            // No se borra ni se pisa: se ignora hasta que alguien lo mire.
            tracing::warn!(
                "plan store: {} ilegible ({e}); planes pendientes solo en memoria",
                path.display()
            );
            return HashMap::new();
        }
    };
    let now = now_unix();
    let before = parsed.plans.len();
    let plans: HashMap<String, PendingPlan> = parsed
        .plans
        .into_iter()
        .filter(|(_, plan)| plan.created_at_unix == 0 || now - plan.created_at_unix <= PLAN_TTL_SECS)
        .collect();
    let expired = before - plans.len();
    if expired > 0 {
        tracing::info!("plan store: {expired} plan(es) caducado(s) al arrancar, descartados");
    }
    plans
}

fn persist_plans(plans: &HashMap<String, PendingPlan>) {
    let Some(path) = plans_path() else {
        return;
    };
    let _serialized = PLAN_PERSIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!("plan store: no se pudo crear {} ({e})", dir.display());
        return;
    }
    let body = match serde_json::to_string_pretty(&PendingPlansFile {
        version: 1,
        plans: plans.clone(),
    }) {
        Ok(body) => body,
        Err(e) => {
            tracing::warn!("plan store: no se pudo serializar ({e})");
            return;
        }
    };
    let tmp = path.with_extension("json.tmp");
    match write_private(&tmp, &body).and_then(|_| std::fs::rename(&tmp, &path)) {
        Ok(()) => {}
        Err(e) => tracing::warn!(
            "plan store: no se pudo escribir {} ({e}); el plan vale solo en este proceso",
            path.display()
        ),
    }
}

/// Arranca el store de planes cargando los pendientes no caducados del disco.
pub fn init_plan_store() {
    PLAN_STORE.set(RwLock::new(load_plans())).ok();
}

pub async fn store_plan(id: &str, guild: &str, tool: &str, args: &serde_json::Value, agent_id: &str, intent: &str) {
    let store = PLAN_STORE.get_or_init(|| RwLock::new(HashMap::new()));
    let snapshot = {
        let mut locked = store.write().await;
        locked.insert(id.to_string(), PendingPlan {
            guild: guild.to_string(),
            tool: tool.to_string(),
            args: args.as_object().cloned().unwrap_or_default(),
            agent_id: agent_id.to_string(),
            intent: intent.to_string(),
            created_at_unix: now_unix(),
            held: None,
        });
        locked.clone()
    };
    persist_plans(&snapshot);
}

pub async fn get_plan(id: &str) -> Option<PendingPlan> {
    let store = PLAN_STORE.get()?;
    let locked = store.read().await;
    locked.get(id).cloned()
}

/// Lista los planes pendientes **sin valores de args**.
///
/// Jose 2026-11-15 (contrato v2): el operador tiene que poder ver lo que espera
/// un criterio, pero un plan puede llevar una ruta o un secreto. Se publica el
/// número de claves y un hash corto de los NOMBRES de args, nunca los valores.
pub async fn list_plans() -> Vec<serde_json::Value> {
    let Some(store) = PLAN_STORE.get() else { return Vec::new() };
    let locked = store.read().await;
    let mut out: Vec<serde_json::Value> = locked
        .iter()
        .map(|(id, plan)| {
            let arrow = crate::security::grants::args_names_summary(&plan.args);
            serde_json::json!({
                "id": id,
                "kind": "plan",
                "state": if plan.held.is_some() { "held" } else { "pending" },
                "guild": plan.guild,
                "tool_name": plan.tool,
                "agent_id": plan.agent_id,
                "intent": plan.intent,
                "summary": arrow,
                "held": plan.held,
                "created_at_unix": plan.created_at_unix,
            })
        })
        .collect();
    out.sort_by_key(|v| v.get("created_at_unix").and_then(|c| c.as_i64()).unwrap_or(0));
    out
}

/// Nombres de los args + hash corto. **Nunca** los valores.
pub fn args_names_summary(args: &serde_json::Map<String, serde_json::Value>) -> String {
    use sha2::{Digest, Sha256};
    let mut nombres: Vec<&str> = args.keys().map(|k| k.as_str()).collect();
    nombres.sort_unstable();
    let mut hasher = Sha256::new();
    for n in &nombres {
        hasher.update(n.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("{} arg(s), hash {}", nombres.len(), hash)
}

/// Pone un plan **en duda** sin consumirlo (Jose 2026-11-15).
///
/// Aprobar y rechazar dejan de ser las únicas salidas: un agente o un revisor
/// que detecta que falta contexto marca `hold` y el plan sigue pendiente, con el
/// motivo a la vista. Devuelve `false` si el plan no existe (o ya se resolvió).
pub async fn hold_plan(id: &str, reason: &str) -> bool {
    let Some(store) = PLAN_STORE.get() else { return false };
    let (held, snapshot) = {
        let mut locked = store.write().await;
        match locked.get_mut(id) {
            Some(plan) => {
                plan.held = Some(HeldNote {
                    reason: reason.trim().to_string(),
                    at_unix: now_unix(),
                });
                (true, locked.clone())
            }
            None => (false, HashMap::new()),
        }
    };
    if held {
        PLANS_HELD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        persist_plans(&snapshot);
        tracing::info!("plan store: plan '{id}' en duda — {}", reason.trim());
    }
    held
}

/// Retira un plan **por su propio autor** antes de que nadie lo apruebe.
///
/// Jose 2026-11-15: darse cuenta de que el plan está mal y retirarlo es un
/// acierto del sistema, así que se cuenta aparte de aprobaciones y rechazos.
pub async fn withdraw_plan(id: &str) -> bool {
    if remove_plan(id).await {
        PLANS_WITHDRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("plan store: plan '{id}' retirado por su autor");
        true
    } else {
        false
    }
}

/// Contadores de criterio: `held` y `withdrawn` (nunca «aprobaciones/hora»).
pub fn judgement_counters() -> serde_json::Value {
    serde_json::json!({
        "held": PLANS_HELD.load(std::sync::atomic::Ordering::Relaxed),
        "withdrawn": PLANS_WITHDRAWN.load(std::sync::atomic::Ordering::Relaxed),
    })
}

pub async fn remove_plan(id: &str) -> bool {
    let Some(store) = PLAN_STORE.get() else { return false };
    let (removed, snapshot) = {
        let mut locked = store.write().await;
        let removed = locked.remove(id).is_some();
        (removed, locked.clone())
    };
    if removed {
        // Un plan aprobado (o cancelado) no debe resucitar si el kernel
        // reinicia antes del siguiente `store_plan`.
        persist_plans(&snapshot);
    }
    removed
}

pub fn init() {
    GRANT_REGISTRY.set(RwLock::new(GrantStore {
        pending: HashMap::new(),
    })).ok();
}

pub fn set_notifier(tx: tokio::sync::broadcast::Sender<serde_json::Value>) {
    GRANT_NOTIFIER.set(tx).ok();
}

pub async fn register(request: GrantRequest) -> GrantId {
    let guild = request.guild.clone();
    let tool_name = request.tool_name.clone();
    let agent_id = request.agent_id.clone();
    let reason = request.reason.clone();

    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized — call grants::init() during startup")
        .write()
        .await;
    let id = format!("{:08x}", rand::random::<u32>());
    locked.pending.insert(id.clone(), request);
    drop(locked);

    if let Some(tx) = GRANT_NOTIFIER.get() {
        let _ = tx.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "grant_required",
            "params": {
                "id": id,
                "guild": guild,
                "tool_name": tool_name,
                "agent_id": agent_id,
                "reason": reason,
            }
        }));
    }

    id
}

pub async fn resolve(id: &str, level: GrantLevel) -> bool {
    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .write()
        .await;
    if let Some(req) = locked.pending.remove(id) {
        req.tx.send(level).is_ok()
    } else {
        false
    }
}

/// Remove a grant without resolving it (receiver gets Canceled).
/// Used when the approver rejects the request.
pub async fn remove(id: &str) -> bool {
    let mut locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .write()
        .await;
    locked.pending.remove(id).is_some()
}

/// Resultado de un juicio (rechazo) con compuerta de autoría.
///
/// H-20261116 APPROVE-ACTION-REJECT-SIN-COMPUERTA (P1): la vía de rechazo llamaba a
/// `remove` / `remove_plan` sin comparar al que decide con el autor, así que
/// **cualquier autenticado cancelaba el grant o el plan de otro** y no quedaba fila de
/// auditoría de lo ocurrido. `resolve` (aprobar) ya tenía su compuerta de
/// autoaprobación (D-13); rechazar no tenía ninguna, y rechazar *sí* destruye trabajo
/// ajeno aunque no ejecute nada.
#[derive(Debug, PartialEq, Eq)]
pub enum JudgementOutcome {
    /// Se quitó: estos datos son los de la fila de auditoría.
    Removed {
        guild: String,
        tool: String,
        owner: String,
    },
    NotFound,
    /// Existe, pero no es suyo: no se toca nada.
    NotOwner {
        owner: String,
    },
}

/// Descarta un plan o un grant **solo si quien lo pide es su autor o el operador**.
///
/// Precedencia idéntica a la histórica (plan primero, luego grant) para no cambiar qué
/// se cancela; lo único nuevo es la compuerta. Un id vacío o `anonymous` solo puede
/// decidir sobre lo que también nació anónimo, que es el caso normal con `dev_mode`.
pub async fn reject_checked(id: &str, requester: &str, is_operator: bool) -> JudgementOutcome {
    let anon = |s: &str| s.is_empty() || s == "anonymous";
    let is_mine = |owner: &str| owner == requester || (anon(owner) && anon(requester));

    if let Some(plan) = get_plan(id).await {
        if !(is_operator || is_mine(&plan.agent_id)) {
            return JudgementOutcome::NotOwner { owner: plan.agent_id };
        }
        return if remove_plan(id).await {
            JudgementOutcome::Removed {
                guild: plan.guild,
                tool: plan.tool,
                owner: plan.agent_id,
            }
        } else {
            JudgementOutcome::NotFound
        };
    }

    let pendiente = {
        let locked = GRANT_REGISTRY
            .get()
            .expect("GrantRegistry not initialized")
            .read()
            .await;
        locked
            .pending
            .get(id)
            .map(|r| (r.agent_id.clone(), r.guild.clone(), r.tool_name.clone()))
    };
    let Some((owner, guild, tool)) = pendiente else {
        return JudgementOutcome::NotFound;
    };
    if !(is_operator || is_mine(&owner)) {
        return JudgementOutcome::NotOwner { owner };
    }
    if remove(id).await {
        JudgementOutcome::Removed { guild, tool, owner }
    } else {
        JudgementOutcome::NotFound
    }
}

pub async fn list_pending() -> Vec<serde_json::Value> {
    let locked = GRANT_REGISTRY
        .get()
        .expect("GrantRegistry not initialized")
        .read()
        .await;
    locked
        .pending
        .iter()
        .map(|(id, req)| {
            serde_json::json!({
                "id": id,
                "guild": req.guild,
                "tool_name": req.tool_name,
                "agent_id": req.agent_id,
                "reason": req.reason,
            })
        })
        .collect()
}

pub fn spawn_reaper() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let Some(store) = GRANT_REGISTRY.get() else { continue };
            let mut locked = store.write().await;
            let before = locked.pending.len();
            let now = tokio::time::Instant::now();
            locked.pending.retain(|_, req| req.expires_at > now);
            let reaped = before - locked.pending.len();
            if reaped > 0 {
                tracing::debug!("🗑️ GrantRegistry: reaped {} expired grants", reaped);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_init() {
        if GRANT_REGISTRY.get().is_none() {
            init();
        }
    }

    #[tokio::test]
    async fn test_register_and_resolve_this_time() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "test-agent".into(),
            reason: "blocked by policy".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;
        assert!(!id.is_empty());

        let resolved = resolve(&id, GrantLevel::ThisTime).await;
        assert!(resolved);

        let decision = rx.await.unwrap();
        assert_eq!(decision, GrantLevel::ThisTime);
    }

    #[tokio::test]
    async fn test_register_and_resolve_this_session() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "code".into(),
            tool_name: "code_analysis".into(),
            agent_id: "agent-x".into(),
            reason: "filesystem scope exceeded".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::ThisSession).await);
        assert_eq!(rx.await.unwrap(), GrantLevel::ThisSession);
    }

    #[tokio::test]
    async fn test_register_and_resolve_always_for_guild() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "docker".into(),
            tool_name: "docker_exec".into(),
            agent_id: "devops".into(),
            reason: "process execution blocked".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::AlwaysForGuild).await);
        assert_eq!(rx.await.unwrap(), GrantLevel::AlwaysForGuild);
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_returns_false() {
        ensure_init();
        assert!(!resolve("nonexistent", GrantLevel::ThisTime).await);
    }

    #[tokio::test]
    async fn test_resolve_twice_returns_false_second_time() {
        ensure_init();
        let (tx, _rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "tester".into(),
            reason: "test".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        assert!(resolve(&id, GrantLevel::ThisTime).await);
        assert!(!resolve(&id, GrantLevel::ThisSession).await);
    }

    #[tokio::test]
    async fn test_list_pending_after_register() {
        ensure_init();
        let (tx, _rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "websearch".into(),
            tool_name: "web_search".into(),
            agent_id: "agent-1".into(),
            reason: "network blocked".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        let pending = list_pending().await;
        let found = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id)
                && v.get("guild").and_then(|g| g.as_str()) == Some("websearch")
        });
        assert!(found, "registered grant must appear in list_pending");

        resolve(&id, GrantLevel::ThisTime).await;
        let pending_after = list_pending().await;
        let still_found = pending_after.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id)
        });
        assert!(!still_found, "resolved grant must not appear in list_pending");
    }

    #[tokio::test]
    async fn test_cancelled_sender_returns_false() {
        ensure_init();
        let (tx, rx) = oneshot::channel();
        let id = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "anon".into(),
            reason: "test".into(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(60),
        }).await;

        // Drop the receiver to simulate cancellation
        drop(rx);

        // resolve should return false because the send failed (receiver dropped)
        assert!(!resolve(&id, GrantLevel::ThisTime).await);
    }

    #[tokio::test]
    async fn test_grant_level_as_str() {
        assert_eq!(GrantLevel::ThisTime.as_str(), "this_time");
        assert_eq!(GrantLevel::ThisSession.as_str(), "this_session");
        assert_eq!(GrantLevel::AlwaysForGuild.as_str(), "always_for_guild");
    }

    #[tokio::test]
    async fn test_reaper_removes_expired_grants() {
        ensure_init();
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        // checked_sub instead of `-`: `Instant::now() - 1s` panics if the
        // machine has been up for less than 1s (CI runners / cold boots;
        // incident 2026-10-12: same pattern killed a kernel maintenance task
        // in production). If it saturates, fall back to `now` — an expires_at
        // equal to (or before) the reaper's own `now` is still reaped by the
        // `expires_at > now` retain, so the test's invariant is preserved.
        let now_test = tokio::time::Instant::now();
        let expired = now_test
            .checked_sub(tokio::time::Duration::from_secs(1))
            .unwrap_or(now_test);
        let future = now_test + tokio::time::Duration::from_secs(3600);

        let id1 = register(GrantRequest {
            guild: "bash".into(),
            tool_name: "bash_execute".into(),
            agent_id: "a".into(),
            reason: "expired".into(),
            arguments: serde_json::Map::new(),
            tx: tx1,
            expires_at: expired,
        }).await;

        let id2 = register(GrantRequest {
            guild: "code".into(),
            tool_name: "code_analysis".into(),
            agent_id: "b".into(),
            reason: "future".into(),
            arguments: serde_json::Map::new(),
            tx: tx2,
            expires_at: future,
        }).await;

        // Manually run the reaper logic
        {
            let store = GRANT_REGISTRY.get().unwrap();
            let mut locked = store.write().await;
            let now = tokio::time::Instant::now();
            locked.pending.retain(|_, req| req.expires_at > now);
        }

        let pending = list_pending().await;
        let expired_still_present = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id1)
        });
        assert!(!expired_still_present, "expired grant must be reaped");

        let future_still_present = pending.iter().any(|v| {
            v.get("id").and_then(|i| i.as_str()) == Some(&id2)
        });
        assert!(future_still_present, "future grant must survive reaping");
    }

    /// Jose 2026-11-15: `hold` deja el plan pendiente (no lo consume) y cuenta
    /// como criterio, no como aprobación.
    #[tokio::test]
    async fn hold_marca_en_duda_sin_consumir_y_no_cuenta_como_aprobacion() {
        init_plan_store();
        let id = format!("hold-test-{}", uuid::Uuid::new_v4().simple());
        store_plan(&id, "bash", "run", &serde_json::json!({"cmd": "ls"}), "deep", "listar").await;

        let antes = judgement_counters();
        assert!(hold_plan(&id, "falta saber si el directorio es el correcto").await);

        let plan = get_plan(&id).await.expect("el plan debe seguir pendiente tras hold");
        let nota = plan.held.expect("hold debe dejar nota");
        assert_eq!(nota.reason, "falta saber si el directorio es el correcto");

        let despues = judgement_counters();
        assert_eq!(
            despues["held"].as_u64().unwrap(),
            antes["held"].as_u64().unwrap() + 1,
            "hold cuenta como criterio"
        );
        // El contrato no premia aprobar: no existe contador de aprobaciones que
        // este cambio infle, y el test falla si alguien mete «hold» como aprobado.
        assert!(despues.get("approved").is_none(), "hold no es aprobación");

        remove_plan(&id).await;
    }

    /// Retirar el propio plan lo borra y se cuenta aparte.
    #[tokio::test]
    async fn withdraw_borra_el_plan_y_se_cuenta_aparte() {
        init_plan_store();
        let id = format!("withdraw-test-{}", uuid::Uuid::new_v4().simple());
        store_plan(&id, "bash", "run", &serde_json::json!({}), "deep", "algo").await;

        let antes = judgement_counters();
        assert!(withdraw_plan(&id).await);
        assert!(get_plan(&id).await.is_none(), "retirado = ya no está");
        assert_eq!(
            judgement_counters()["withdrawn"].as_u64().unwrap(),
            antes["withdrawn"].as_u64().unwrap() + 1
        );
        // Retirar dos veces no inventa cuenta.
        assert!(!withdraw_plan(&id).await);
    }

    /// `hold` sobre un id inexistente no miente.
    #[tokio::test]
    async fn hold_de_id_inexistente_devuelve_false() {
        init_plan_store();
        assert!(!hold_plan("no-existe-este-plan", "x").await);
    }

}

/// H-20261116 APPROVE-ACTION-REJECT-SIN-COMPUERTA (P1): el rechazo ya no puede
/// tocar lo ajeno. Estos tests son el contrato.
#[cfg(test)]
mod reject_checked_tests {
    use super::*;
    use tokio::sync::oneshot;

    fn grant(id_agent: &str) -> GrantRequest {
        let (tx, rx) = oneshot::channel();
        // el receptor no se usa: solo se comprueba si el grant sigue en la lista
        std::mem::forget(rx);
        GrantRequest {
            guild: "bash".to_string(),
            tool_name: "bash".to_string(),
            agent_id: id_agent.to_string(),
            reason: "test".to_string(),
            arguments: serde_json::Map::new(),
            tx,
            expires_at: tokio::time::Instant::now() + tokio::time::Duration::from_secs(300),
        }
    }

    async fn alta(agent: &str) -> GrantId {
        init();
        init_plan_store();
        register(grant(agent)).await
    }

    async fn sigue_pendiente(id: &str) -> bool {
        list_pending()
            .await
            .iter()
            .any(|v| v.get("id").and_then(|x| x.as_str()) == Some(id))
    }

    /// MUTACION: quitar la compuerta de autoría en `reject_checked` hace que este
    /// test reciba `Removed` y que el grant del otro desaparezca.
    #[tokio::test]
    async fn otro_agente_no_puede_rechazar_un_grant_ajeno() {
        let id = alta("autor-rechazo-1").await;
        let outcome = reject_checked(&id, "intruso", false).await;
        assert_eq!(
            outcome,
            JudgementOutcome::NotOwner { owner: "autor-rechazo-1".to_string() },
            "un tercero no puede descartar el grant de otro"
        );
        assert!(sigue_pendiente(&id).await, "el grant tiene que seguir pendiente");
    }

    #[tokio::test]
    async fn el_autor_si_puede_y_devuelve_los_datos_de_auditoria() {
        let id = alta("autor-rechazo-2").await;
        let outcome = reject_checked(&id, "autor-rechazo-2", false).await;
        match outcome {
            JudgementOutcome::Removed { guild, tool, owner } => {
                assert_eq!(guild, "bash");
                assert_eq!(tool, "bash");
                assert_eq!(owner, "autor-rechazo-2");
            }
            otro => panic!("esperaba Removed, salió {otro:?}"),
        }
        assert!(!sigue_pendiente(&id).await, "ya no está pendiente");
    }

    #[tokio::test]
    async fn el_operador_si_puede_rechazar_lo_ajeno() {
        let id = alta("autor-rechazo-3").await;
        let outcome = reject_checked(&id, "operador-humano", true).await;
        assert!(matches!(outcome, JudgementOutcome::Removed { .. }), "el operador puede: {outcome:?}");
        assert!(!sigue_pendiente(&id).await);
    }

    #[tokio::test]
    async fn un_id_que_no_existe_es_not_found_y_no_toca_nada() {
        init();
        init_plan_store();
        assert_eq!(
            reject_checked("no-existe-rechazo", "quien-sea", false).await,
            JudgementOutcome::NotFound
        );
    }

    /// Los planes tienen la misma compuerta que los grants (antes `remove_plan`
    /// se llamaba a secas desde el handler).
    #[tokio::test]
    async fn un_plan_ajeno_tampoco_se_rechaza_y_sigue_pendiente() {
        init_plan_store();
        store_plan(
            "plan-rechazo-ajeno",
            "bash",
            "bash",
            &serde_json::json!({"command": "echo hola"}),
            "autor-plan-1",
            "probar",
        )
        .await;
        let outcome = reject_checked("plan-rechazo-ajeno", "intruso", false).await;
        assert_eq!(
            outcome,
            JudgementOutcome::NotOwner { owner: "autor-plan-1".to_string() }
        );
        assert!(get_plan("plan-rechazo-ajeno").await.is_some(), "el plan sigue ahí");

        let outcome = reject_checked("plan-rechazo-ajeno", "autor-plan-1", false).await;
        assert!(matches!(outcome, JudgementOutcome::Removed { .. }), "{outcome:?}");
        assert!(get_plan("plan-rechazo-ajeno").await.is_none());
    }
}
