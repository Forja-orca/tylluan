//! T-DSH-109 — centinela opt-in de write_gate L2 (juez local Qwen3Guard).
//!
//! Apagado por defecto: si `[security.sentinel] enabled = false` (default) este
//! módulo no arranca ningún proceso y `judge()` devuelve `Unavailable`, de modo
//! que write_gate conserva exactamente su comportamiento actual (juez por
//! gramática vía `llama_backend`).
//!
//! Cuando se activa, arranca **bajo demanda** un `llama-server` propio con la
//! ruta de modelo GGUF **explícita** de config (nunca descarga nada), lo usa en
//! cola serie (un slot, una petición a la vez) y lo descarga tras inactividad.
//! Prioridad BELOW_NORMAL best-effort en Windows; `--threads` acotado a 4; sin
//! `--mlock`. Puerto propio (efímero o el de config), nunca 47004.
//!
//! Política (la aplica `write_gate`): `Unsafe` → cuarentena; `Controversial` →
//! marca para revisión (no cuarentena); `Safe` → pasa. Fallo/timeout del
//! centinela → `Unavailable` → **no bloquea**: write_gate cae a las reglas
//! L1/L2 existentes (fail-open).

use crate::config::SentinelConfig;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Veredicto del centinela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentinelVerdict {
    Safe,
    Controversial,
    Unsafe,
    /// No se pudo dictaminar (apagado, sin modelo, timeout, error). Nunca bloquea.
    Unavailable,
}

/// Mapea la salida cruda del modelo (p. ej. `Safety: Unsafe\nCategories: ...`) a
/// veredicto. `Controversial` se comprueba antes que `Safe` porque la palabra
/// "unsafe" contiene "safe".
pub fn classify_verdict(raw: &str) -> SentinelVerdict {
    let low = raw.to_lowercase();
    if low.contains("unsafe") {
        SentinelVerdict::Unsafe
    } else if low.contains("controversial") {
        SentinelVerdict::Controversial
    } else if low.contains("safe") {
        SentinelVerdict::Safe
    } else {
        SentinelVerdict::Unavailable
    }
}

/// Acción de write_gate L2 para un veredicto del centinela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentinelAction {
    Quarantine,
    MarkReview,
    Pass,
    /// Fallo/timeout: no bloquear; seguir con las reglas existentes.
    FallbackToExistingRules,
}

pub fn action_for(verdict: SentinelVerdict) -> SentinelAction {
    match verdict {
        SentinelVerdict::Unsafe => SentinelAction::Quarantine,
        SentinelVerdict::Controversial => SentinelAction::MarkReview,
        SentinelVerdict::Safe => SentinelAction::Pass,
        SentinelVerdict::Unavailable => SentinelAction::FallbackToExistingRules,
    }
}

/// T-DSH-113: modo del centinela.
///
/// - `Off`: apagado (default). No arranca nada; write_gate usa las reglas L1/L2
///   de siempre.
/// - `Observe`: puntúa y **registra** (tracing + contadores) pero NO pone
///   cuarentena ni marca de revisión; fail-open igual que en `Enforce`.
/// - `Enforce`: aplica la política (Unsafe → cuarentena, Controversial → marca).
///
/// Compatibilidad: `enabled = true` sin `mode` equivale a `Enforce`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentinelMode {
    Off,
    Observe,
    Enforce,
}

impl SentinelMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SentinelMode::Off => "off",
            SentinelMode::Observe => "observe",
            SentinelMode::Enforce => "enforce",
        }
    }

    /// `true` si el centinela debe puntuar (observe/enforce).
    pub fn judges(&self) -> bool {
        !matches!(self, SentinelMode::Off)
    }
}

/// Resuelve el modo a partir de `[security.sentinel]`. `mode` explícito manda;
/// si falta, `enabled = true` → Enforce y `enabled = false` → Off (compatibilidad).
pub fn resolve_mode(enabled: bool, mode: Option<&str>) -> SentinelMode {
    match mode.map(|m| m.trim().to_ascii_lowercase()) {
        Some(m) if m == "off" => SentinelMode::Off,
        Some(m) if m == "observe" => SentinelMode::Observe,
        Some(m) if m == "enforce" => SentinelMode::Enforce,
        Some(other) => {
            tracing::warn!(
                "[T-DSH-113] sentinel.mode = '{other}' no reconocido; se usa off (valores: off|observe|enforce)"
            );
            SentinelMode::Off
        }
        None => {
            if enabled {
                SentinelMode::Enforce
            } else {
                SentinelMode::Off
            }
        }
    }
}

/// Contadores del centinela para `/health` (sin tokens ni rutas).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SentinelStats {
    pub safe: u64,
    pub controversial: u64,
    pub unsafe_verdicts: u64,
    pub unavailable: u64,
    pub fallbacks: u64,
    pub cpu_skips: u64,
    pub in_flight: usize,
}

/// `true` solo si el modo aplica acciones (cuarentena/marca). Observe puntúa
/// pero no actúa; Off ni siquiera puntúa.
pub fn mode_applies_action(mode: SentinelMode) -> bool {
    matches!(mode, SentinelMode::Enforce)
}

/// T-DSH-114: ¿hay que saltarse el centinela por carga de CPU? Devuelve true
/// si la CPU global supera `max_cpu_pct` (fail-open, sin lanzar llama-server).
pub fn should_skip_for_cpu(cpu_pct: Option<f32>, max_cpu_pct: u8) -> bool {
    cpu_pct.map(|p| p > f32::from(max_cpu_pct)).unwrap_or(false)
}

/// CPU global del sistema, cacheada 2 s (sysinfo es caro para llamarla por nota).
fn system_cpu_pct_cached() -> Option<f32> {
    use std::sync::Mutex;
    use std::time::Instant;
    static CACHE: Mutex<Option<(Instant, f32)>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((ts, pct)) = *guard
        && ts.elapsed() < Duration::from_secs(2)
    {
        return Some(pct);
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_usage();
    let pct = sys.global_cpu_usage();
    *guard = Some((Instant::now(), pct));
    Some(pct)
}

/// ¿Toca descargar el servidor? Falso si hay peticiones en vuelo (T-DSH-113:
/// el reaper de T-DSH-110 podía descargar el servidor en mitad de una cola).
pub fn should_unload_idle(alive: bool, in_flight: usize, idle_secs: u64, idle_elapsed_secs: u64) -> bool {
    alive && in_flight == 0 && idle_elapsed_secs > idle_secs
}

/// Guard de petición en vuelo: incrementa al entrar y decrementa al salir
/// (incluso si el future se cancela).
struct InFlightGuard<'a>(&'a std::sync::atomic::AtomicUsize);
impl<'a> InFlightGuard<'a> {
    fn new(counter: &'a std::sync::atomic::AtomicUsize) -> Self {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self(counter)
    }
}
impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn mode_to_u8(mode: SentinelMode) -> u8 {
    match mode {
        SentinelMode::Off => 0,
        SentinelMode::Observe => 1,
        SentinelMode::Enforce => 2,
    }
}

fn mode_from_u8(v: u8) -> SentinelMode {
    match v {
        1 => SentinelMode::Observe,
        2 => SentinelMode::Enforce,
        _ => SentinelMode::Off,
    }
}

struct ServerState {
    child: Option<Child>,
    port: u16,
    last_used: Instant,
    /// Job Object de Windows con KILL_ON_JOB_CLOSE: si el kernel muere (incluso
    /// con kill), el handle se cierra y el hijo muere con él (sin huérfanos).
    job: Option<isize>,
}

impl ServerState {
    fn alive_mut(&mut self) -> bool {
        self.child
            .as_mut()
            .map(|c| c.try_wait().ok().flatten().is_none())
            .unwrap_or(false)
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        close_job(self.job.take());
    }
}

/// Juez centinela con servidor propio bajo demanda y cola serie.
pub struct SentinelJudge {
    cfg: SentinelConfig,
    mode: std::sync::atomic::AtomicU8,
    reaper_started: std::sync::atomic::AtomicBool,
    state: Mutex<ServerState>,
    http: reqwest::Client,
    /// Solo tests: base URL de un servidor falso (no se arranca llama-server).
    test_base: Option<String>,
    in_flight: std::sync::atomic::AtomicUsize,
    verdicts: [std::sync::atomic::AtomicU64; 4],
    fallbacks: std::sync::atomic::AtomicU64,
    cpu_skips: std::sync::atomic::AtomicU64,
}

impl SentinelJudge {
    pub fn new(cfg: SentinelConfig) -> Self {
        let mode = resolve_mode(cfg.enabled, cfg.mode.as_deref());
        Self {
            cfg,
            mode: std::sync::atomic::AtomicU8::new(mode_to_u8(mode)),
            reaper_started: std::sync::atomic::AtomicBool::new(false),
            state: Mutex::new(ServerState {
                child: None,
                port: 0,
                last_used: Instant::now(),
                job: None,
            }),
            http: reqwest::Client::new(),
            test_base: None,
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            verdicts: std::array::from_fn(|_| std::sync::atomic::AtomicU64::new(0)),
            fallbacks: std::sync::atomic::AtomicU64::new(0),
            cpu_skips: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn mode(&self) -> SentinelMode {
        mode_from_u8(self.mode.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// T-DSH-114: cambia el modo en caliente (sin reiniciar el kernel).
    pub fn set_mode(&self, mode: SentinelMode) -> SentinelMode {
        let prev = self.mode();
        self.mode
            .store(mode_to_u8(mode), std::sync::atomic::Ordering::SeqCst);
        prev
    }

    /// `true` si el servidor está cargado (para `/health`).
    pub fn loaded(&self) -> bool {
        self.state.try_lock().map(|mut st| st.alive_mut()).unwrap_or(false)
    }

    pub fn stats(&self) -> SentinelStats {
        use std::sync::atomic::Ordering::Relaxed;
        SentinelStats {
            safe: self.verdicts[0].load(Relaxed),
            controversial: self.verdicts[1].load(Relaxed),
            unsafe_verdicts: self.verdicts[2].load(Relaxed),
            unavailable: self.verdicts[3].load(Relaxed),
            fallbacks: self.fallbacks.load(Relaxed),
            cpu_skips: self.cpu_skips.load(Relaxed),
            in_flight: self.in_flight.load(Relaxed),
        }
    }

    fn note_verdict(&self, verdict: SentinelVerdict) {
        use std::sync::atomic::Ordering::Relaxed;
        let idx = match verdict {
            SentinelVerdict::Safe => 0,
            SentinelVerdict::Controversial => 1,
            SentinelVerdict::Unsafe => 2,
            SentinelVerdict::Unavailable => 3,
        };
        self.verdicts[idx].fetch_add(1, Relaxed);
    }

    /// Cuenta un fail-open (el llamante cae a las reglas L1/L2 existentes).
    pub fn note_fallback(&self) {
        self.fallbacks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Constructor de tests: apunta a un servidor falso ya levantado por el test.
    #[cfg(test)]
    fn for_test(base: &str, timeout_ms: u64) -> Self {
        let mut judge = Self::new(SentinelConfig {
            enabled: true,
            timeout_ms,
            // En tests no queremos que el gate de CPU (maquina cargada) tumbe
            // los fake servers: umbral 100 = nunca salta.
            max_cpu_pct: 100,
            ..SentinelConfig::default()
        });
        judge.test_base = Some(base.trim_end_matches('/').to_string());
        judge
    }

    fn base_url(&self, port: u16) -> String {
        self.test_base
            .clone()
            .unwrap_or_else(|| format!("http://127.0.0.1:{port}"))
    }

    /// Arranca (si hace falta) el `llama-server` propio. Debe llamarse con el
    /// mutex tomado (cola serie).
    async fn ensure_server(&self, st: &mut ServerState) -> Result<u16, String> {
        if st.alive_mut() {
            return Ok(st.port);
        }
        st.kill();
        let model = self
            .cfg
            .model_path
            .as_ref()
            .ok_or_else(|| "sentinel: model_path no configurado".to_string())?;
        if !model.is_file() {
            return Err(format!(
                "sentinel: model_path no existe: {}",
                model.display()
            ));
        }
        let bin = resolve_llama_server(self.cfg.llama_server_path.as_deref())
            .ok_or_else(|| "sentinel: llama-server no encontrado (config o TYLLUAN_LLAMA_SERVER)".to_string())?;
        let port = if self.cfg.port > 0 {
            self.cfg.port
        } else {
            free_port()?
        };
        let threads = self.cfg.threads.clamp(1, 4);
        let mut cmd = Command::new(&bin);
        cmd.arg("-m")
            .arg(model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-t")
            .arg(threads.to_string())
            .arg("--parallel")
            .arg("1")
            .arg("-c")
            .arg(self.cfg.ctx_size.max(512).to_string())
            .arg("--no-webui")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = cmd
            .spawn()
            .map_err(|e| format!("sentinel: no pude arrancar {}: {e}", bin.display()))?;
        set_below_normal(child.id());
        st.job = assign_kill_on_close(child.id());
        st.child = Some(child);
        st.port = port;
        st.last_used = Instant::now();

        // Espera real a /health (el puerto abierto no implica modelo cargado).
        let base = self.base_url(port);
        let deadline = Instant::now() + Duration::from_millis(self.cfg.timeout_ms.max(1000));
        while Instant::now() < deadline {
            if !st.alive_mut() {
                return Err("sentinel: llama-server murió durante el arranque".to_string());
            }
            let healthy = self
                .http
                .get(format!("{base}/health"))
                .timeout(Duration::from_millis(500))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if healthy {
                return Ok(port);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        st.kill();
        Err("sentinel: llama-server no llegó a /health a tiempo".to_string())
    }

    /// Dictamina un texto. Cola serie: un veredicto a la vez; descarga el
    /// servidor si ha estado inactivo más de `idle_unload_secs`.
    pub async fn judge(&self, text: &str) -> SentinelVerdict {
        if !self.mode().judges() {
            self.note_verdict(SentinelVerdict::Unavailable);
            return SentinelVerdict::Unavailable;
        }
        // T-DSH-114: salto por carga de CPU (fail-open, sin lanzar llama-server).
        if should_skip_for_cpu(system_cpu_pct_cached(), self.cfg.max_cpu_pct) {
            self.cpu_skips
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.note_verdict(SentinelVerdict::Unavailable);
            tracing::debug!(
                "[T-DSH-114] centinela: CPU > {}% — se salta el juez (fail-open)",
                self.cfg.max_cpu_pct
            );
            return SentinelVerdict::Unavailable;
        }
        let _in_flight = InFlightGuard::new(&self.in_flight);
        let mut st = self.state.lock().await;
        // T-DSH-113: last_used al EMPEZAR la petición (además de al terminar),
        // para que el reaper no cuente como inactivo el tiempo de una cola larga.
        st.last_used = Instant::now();
        if should_unload_idle(
            st.alive_mut(),
            self.in_flight.load(std::sync::atomic::Ordering::Relaxed),
            self.cfg.idle_unload_secs,
            st.last_used.elapsed().as_secs(),
        ) {
            tracing::info!(
                "[T-DSH-109] centinela: descargo llama-server tras {}s inactivo",
                st.last_used.elapsed().as_secs()
            );
            st.kill();
        }
        // En tests apuntamos a un servidor falso ya levantado: no se arranca nada.
        let port = if self.test_base.is_some() {
            0
        } else {
            match self.ensure_server(&mut st).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("[T-DSH-109] centinela no disponible: {e} (fail-open)");
                    self.note_verdict(SentinelVerdict::Unavailable);
                    return SentinelVerdict::Unavailable;
                }
            }
        };
        let base = self.base_url(port);
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": text.chars().take(2000).collect::<String>()}],
            "temperature": 0.0,
            "max_tokens": 8,
            "stream": false,
        });
        let fut = self
            .http
            .post(format!("{base}/v1/chat/completions"))
            .json(&body)
            .send();
        let resp = match tokio::time::timeout(Duration::from_millis(self.cfg.timeout_ms), fut).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::warn!("[T-DSH-109] centinela: error HTTP: {e} (fail-open)");
                self.note_verdict(SentinelVerdict::Unavailable);
                return SentinelVerdict::Unavailable;
            }
            Err(_) => {
                tracing::warn!("[T-DSH-109] centinela: timeout {}ms (fail-open)", self.cfg.timeout_ms);
                self.note_verdict(SentinelVerdict::Unavailable);
                return SentinelVerdict::Unavailable;
            }
        };
        let raw = match resp.json::<serde_json::Value>().await {
            Ok(v) => v
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or_default()
                .to_string(),
            Err(e) => {
                tracing::warn!("[T-DSH-109] centinela: respuesta no parseable: {e} (fail-open)");
                self.note_verdict(SentinelVerdict::Unavailable);
                return SentinelVerdict::Unavailable;
            }
        };
        st.last_used = Instant::now();
        let verdict = classify_verdict(&raw);
        self.note_verdict(verdict);
        verdict
    }

    /// T-DSH-115: completion CRUDA (prompt + gramatica GBNF opcional) para el
    /// juez de gramatica de write_gate. Mismo servidor efimero aislado y misma
    /// cola serie que el centinela; NO usa el llama-server del usuario.
    pub async fn raw_completion(
        &self,
        prompt: &str,
        grammar: Option<&str>,
    ) -> Result<String, String> {
        if !self.mode().judges() {
            return Err("write_gate_judge: modo off".to_string());
        }
        if should_skip_for_cpu(system_cpu_pct_cached(), self.cfg.max_cpu_pct) {
            self.cpu_skips
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(format!(
                "write_gate_judge: CPU > {}% (fail-open)",
                self.cfg.max_cpu_pct
            ));
        }
        let _in_flight = InFlightGuard::new(&self.in_flight);
        let mut st = self.state.lock().await;
        // Descarga perezosa por inactividad (patron T-DSH-113): si el servidor
        // lleva mas de idle_unload_secs sin usarse, se apaga antes de reusarlo.
        if st.alive_mut()
            && self.cfg.idle_unload_secs > 0
            && st.last_used.elapsed().as_secs() > self.cfg.idle_unload_secs
            && self.in_flight.load(std::sync::atomic::Ordering::Relaxed) <= 1
        {
            st.kill();
        }
        st.last_used = Instant::now();
        let port = if self.test_base.is_some() {
            0
        } else {
            self.ensure_server(&mut st).await?
        };
        let base = self.base_url(port);
        let mut body = serde_json::json!({
            "messages": [{"role": "user", "content": prompt.chars().take(4000).collect::<String>()}],
            "temperature": 0.0,
            "max_tokens": 16,
            "stream": false,
        });
        if let Some(g) = grammar {
            body["grammar"] = serde_json::Value::String(g.to_string());
        }
        let fut = self
            .http
            .post(format!("{base}/v1/chat/completions"))
            .json(&body)
            .send();
        let resp = tokio::time::timeout(Duration::from_millis(self.cfg.timeout_ms), fut)
            .await
            .map_err(|_| format!("write_gate_judge: timeout {}ms", self.cfg.timeout_ms))?
            .map_err(|e| format!("write_gate_judge: error HTTP: {e}"))?;
        let json = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("write_gate_judge: respuesta no parseable: {e}"))?;
        let text = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        st.last_used = Instant::now();
        Ok(text)
    }
}

impl Drop for SentinelJudge {
    fn drop(&mut self) {
        if let Ok(mut st) = self.state.try_lock() {
            st.kill();
        }
    }
}

/// `llama-server` explícito de config, o `TYLLUAN_LLAMA_SERVER`, o el del PATH.
fn resolve_llama_server(configured: Option<&std::path::Path>) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| p.is_file()) {
        return Some(p.to_path_buf());
    }
    if let Ok(env_path) = std::env::var("TYLLUAN_LLAMA_SERVER") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    let candidates = ["llama-server", "llama-server.exe"];
    for cand in candidates {
        if let Ok(path) = which(cand) {
            return Some(path);
        }
    }
    None
}

fn which(name: &str) -> Result<PathBuf, ()> {
    let path_var = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(())
}

fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("sentinel: no pude reservar puerto efímero: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("sentinel: local_addr: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Reaper de inactividad: cada `idle_unload_secs/2` mira si el servidor lleva
/// más de `idle_unload_secs` sin uso y lo descarga (T-DSH-110: sin esto, el
/// servidor solo se descargaba al entrar la siguiente llamada).
fn spawn_idle_reaper(judge: &Arc<SentinelJudge>) {
    let weak = Arc::downgrade(judge);
    let idle = judge.cfg.idle_unload_secs.max(1);
    let every = Duration::from_secs((idle / 2).max(1));
    let handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => return,
    };
    handle.spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            let Some(j) = weak.upgrade() else { break };
            if let Ok(mut st) = j.state.try_lock()
                && should_unload_idle(
                    st.alive_mut(),
                    j.in_flight.load(std::sync::atomic::Ordering::Relaxed),
                    j.cfg.idle_unload_secs,
                    st.last_used.elapsed().as_secs(),
                )
            {
                tracing::info!(
                    "[T-DSH-109] centinela: descargo llama-server tras {}s inactivo (reaper)",
                    st.last_used.elapsed().as_secs()
                );
                st.kill();
            }
        }
    });
}

/// Job Object de Windows con KILL_ON_JOB_CLOSE. Devuelve el handle (se cierra
/// en `kill`/`Drop`); si el kernel muere de golpe, Windows cierra el handle y
/// mata el hijo (no quedan huérfanos).
#[cfg(windows)]
fn assign_kill_on_close(pid: u32) -> Option<isize> {
    use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() {
            return None;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut winapi::ctypes::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            winapi::um::handleapi::CloseHandle(job);
            return None;
        }
        let hp = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if hp.is_null() {
            winapi::um::handleapi::CloseHandle(job);
            return None;
        }
        let assigned = AssignProcessToJobObject(job, hp);
        winapi::um::handleapi::CloseHandle(hp);
        if assigned == 0 {
            winapi::um::handleapi::CloseHandle(job);
            return None;
        }
        Some(job as isize)
    }
}

#[cfg(not(windows))]
fn assign_kill_on_close(_pid: u32) -> Option<isize> {
    None
}

fn close_job(job: Option<isize>) {
    #[cfg(windows)]
    unsafe {
        if let Some(handle) = job {
            winapi::um::handleapi::CloseHandle(handle as *mut winapi::ctypes::c_void);
        }
    }
    #[cfg(not(windows))]
    let _ = job;
}

#[cfg(windows)]
fn set_below_normal(pid: u32) {
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::processthreadsapi::SetPriorityClass;
    use winapi::um::winbase::BELOW_NORMAL_PRIORITY_CLASS;
    use winapi::um::winnt::PROCESS_SET_INFORMATION;
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if !handle.is_null() {
            SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS);
            winapi::um::handleapi::CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn set_below_normal(_pid: u32) {}

/// T-DSH-114: cambia el modo del centinela global en caliente. Arranca el
/// reaper si se pasa a observe/enforce (idempotente). Devuelve el modo previo.
pub fn set_mode(mode: SentinelMode) -> SentinelMode {
    let judge = global();
    let prev = judge.set_mode(mode);
    if mode.judges() && !judge.reaper_started.swap(true, std::sync::atomic::Ordering::SeqCst) {
        spawn_idle_reaper(&judge);
    }
    prev
}

/// T-DSH-113: bloque `sentinel` de `/health` (sin tokens ni rutas).
pub fn health_json() -> serde_json::Value {
    let judge = global();
    let stats = judge.stats();
    serde_json::json!({
        "mode": judge.mode().as_str(),
        "loaded": judge.loaded(),
        "verdicts": {
            "safe": stats.safe,
            "controversial": stats.controversial,
            "unsafe": stats.unsafe_verdicts,
            "unavailable": stats.unavailable,
        },
        "fallbacks": stats.fallbacks,
        "cpu_skips": stats.cpu_skips,
        "max_cpu_pct": judge.cfg.max_cpu_pct,
        "in_flight": stats.in_flight,
    })
}

/// Juez global perezoso (config cargada una vez). `Unavailable` si está apagado.
pub fn global() -> Arc<SentinelJudge> {
    static GLOBAL: std::sync::OnceLock<Arc<SentinelJudge>> = std::sync::OnceLock::new();
    GLOBAL
        .get_or_init(|| {
            let cfg = crate::config::TylluanConfig::load_cached()
                .ok()
                .and_then(|c| c.try_read().ok().map(|g| g.security.sentinel.clone()))
                .unwrap_or_default();
            let judge = Arc::new(SentinelJudge::new(cfg.clone()));
            if cfg.enabled {
                spawn_idle_reaper(&judge);
            }
            judge
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_three_labels_and_unknown() {
        assert_eq!(classify_verdict("Safety: Safe"), SentinelVerdict::Safe);
        assert_eq!(
            classify_verdict("Safety: Controversial\nCategories: none"),
            SentinelVerdict::Controversial
        );
        assert_eq!(
            classify_verdict("Safety: Unsafe\nCategories: Jailbreak"),
            SentinelVerdict::Unsafe
        );
        assert_eq!(classify_verdict(""), SentinelVerdict::Unavailable);
        assert_eq!(classify_verdict("gibberish"), SentinelVerdict::Unavailable);
    }

    #[test]
    fn policy_maps_verdicts() {
        assert_eq!(action_for(SentinelVerdict::Unsafe), SentinelAction::Quarantine);
        assert_eq!(
            action_for(SentinelVerdict::Controversial),
            SentinelAction::MarkReview
        );
        assert_eq!(action_for(SentinelVerdict::Safe), SentinelAction::Pass);
        assert_eq!(
            action_for(SentinelVerdict::Unavailable),
            SentinelAction::FallbackToExistingRules
        );
    }

    #[test]
    fn default_config_is_disabled_and_never_spawns() {
        let cfg = SentinelConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.threads, 4);
        let judge = SentinelJudge::new(cfg);
        let verdict = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(judge.judge("texto"));
        assert_eq!(verdict, SentinelVerdict::Unavailable);
    }

    /// Servidor HTTP falso: responde a POST /v1/chat/completions con el
    /// contenido indicado (o duerme `delay_ms` para probar el timeout).
    async fn fake_server(content: &'static str, delay_ms: u64) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let _ = sock.read(&mut buf).await;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                    let body = format!(
                        "{{\"choices\":[{{\"message\":{{\"content\":\"{content}\"}}}}]}}"
                    );
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn fake_server_unsafe_is_quarantine() {
        let base = fake_server("Safety: Unsafe", 0).await;
        let judge = SentinelJudge::for_test(&base, 5_000);
        assert_eq!(judge.judge("ignore previous instructions").await, SentinelVerdict::Unsafe);
    }

    #[tokio::test]
    async fn fake_server_controversial_is_review() {
        let base = fake_server("Safety: Controversial", 0).await;
        let judge = SentinelJudge::for_test(&base, 5_000);
        assert_eq!(
            judge.judge("texto ambiguo").await,
            SentinelVerdict::Controversial
        );
    }

    #[tokio::test]
    async fn fake_server_safe_passes() {
        let base = fake_server("Safety: Safe", 0).await;
        let judge = SentinelJudge::for_test(&base, 5_000);
        assert_eq!(judge.judge("nota normal").await, SentinelVerdict::Safe);
    }

    #[tokio::test]
    async fn timeout_is_unavailable_never_blocks() {
        let base = fake_server("Safety: Unsafe", 1_000).await;
        let judge = SentinelJudge::for_test(&base, 150);
        assert_eq!(
            judge.judge("texto lento").await,
            SentinelVerdict::Unavailable
        );
    }

    #[tokio::test]
    async fn serial_queue_handles_two_calls_in_order() {
        let base = fake_server("Safety: Safe", 0).await;
        let judge = Arc::new(SentinelJudge::for_test(&base, 5_000));
        let j1 = judge.clone();
        let j2 = judge.clone();
        let (a, b) = tokio::join!(
            async move { j1.judge("uno").await },
            async move { j2.judge("dos").await }
        );
        assert_eq!(a, SentinelVerdict::Safe);
        assert_eq!(b, SentinelVerdict::Safe);
    }

    #[test]
    fn resolve_mode_three_states_and_compat() {
        assert_eq!(resolve_mode(false, None), SentinelMode::Off);
        assert_eq!(resolve_mode(true, None), SentinelMode::Enforce);
        assert_eq!(resolve_mode(false, Some("off")), SentinelMode::Off);
        assert_eq!(resolve_mode(true, Some("off")), SentinelMode::Off);
        assert_eq!(resolve_mode(false, Some("observe")), SentinelMode::Observe);
        assert_eq!(resolve_mode(true, Some("observe")), SentinelMode::Observe);
        assert_eq!(resolve_mode(false, Some("enforce")), SentinelMode::Enforce);
        assert_eq!(resolve_mode(true, Some("ENFORCE")), SentinelMode::Enforce);
        assert_eq!(resolve_mode(true, Some("basura")), SentinelMode::Off);
    }

    #[test]
    fn should_unload_idle_respects_in_flight_requests() {
        // Reproduccion T-DSH-113: con una peticion en vuelo el reaper NO debe
        // descargar aunque el reloj de inactividad haya vencido (antes fallaba).
        assert!(!should_unload_idle(true, 1, 10, 999));
        assert!(!should_unload_idle(true, 5, 10, 999));
        // Sin peticiones en vuelo y con inactividad vencida, si descarga.
        assert!(should_unload_idle(true, 0, 10, 11));
        // Servidor apagado o inactividad no vencida: no toca.
        assert!(!should_unload_idle(false, 0, 10, 999));
        assert!(!should_unload_idle(true, 0, 10, 5));
    }

    #[test]
    fn observe_scores_but_does_not_enforce() {
        assert!(!mode_applies_action(SentinelMode::Off));
        assert!(!mode_applies_action(SentinelMode::Observe));
        assert!(mode_applies_action(SentinelMode::Enforce));
        assert!(SentinelMode::Observe.judges());
        assert!(!SentinelMode::Off.judges());
    }

    #[tokio::test]
    async fn observe_mode_counts_verdicts_but_does_not_apply_actions() {
        let base = fake_server("Safety: Unsafe", 0).await;
        let judge = SentinelJudge::for_test(&base, 5_000);
        judge.set_mode(SentinelMode::Observe);
        assert_eq!(judge.judge("ataque").await, SentinelVerdict::Unsafe);
        let stats = judge.stats();
        assert_eq!(stats.unsafe_verdicts, 1);
        assert!(!mode_applies_action(judge.mode()));
        assert_eq!(stats.in_flight, 0);
    }

    #[test]
    fn health_json_has_required_keys_and_no_paths_or_tokens() {
        let v = health_json();
        assert!(v.get("mode").is_some());
        assert!(v.get("loaded").is_some());
        let verdicts = v.get("verdicts").expect("verdicts");
        for k in ["safe", "controversial", "unsafe", "unavailable"] {
            assert!(verdicts.get(k).is_some(), "falta {k}");
        }
        assert!(v.get("fallbacks").is_some());
        let text = v.to_string();
        assert!(!text.contains("Qwen3Guard"), "no debe exponer el modelo: {text}");
        assert!(!text.contains("\\\\"), "no debe exponer rutas: {text}");
        assert!(!text.to_lowercase().contains("token"), "no debe exponer tokens: {text}");
    }

    #[test]
    fn cpu_gate_skips_above_threshold_only() {
        assert!(!should_skip_for_cpu(None, 85));
        assert!(!should_skip_for_cpu(Some(84.9), 85));
        assert!(!should_skip_for_cpu(Some(85.0), 85));
        assert!(should_skip_for_cpu(Some(85.1), 85));
        assert!(should_skip_for_cpu(Some(99.0), 85));
        assert!(!should_skip_for_cpu(Some(100.0), 100));
    }

    #[test]
    fn set_mode_changes_mode_in_hot() {
        let judge = SentinelJudge::new(SentinelConfig::default());
        assert_eq!(judge.mode(), SentinelMode::Off);
        judge.set_mode(SentinelMode::Observe);
        assert_eq!(judge.mode(), SentinelMode::Observe);
        judge.set_mode(SentinelMode::Enforce);
        assert_eq!(judge.mode(), SentinelMode::Enforce);
        judge.set_mode(SentinelMode::Off);
        assert_eq!(judge.mode(), SentinelMode::Off);
    }

    #[tokio::test]
    async fn cpu_gate_returns_unavailable_without_calling_server() {
        // max_cpu_pct = 0 => cualquier CPU > 0 hace saltar el juez; con el
        // servidor falso respondiendo Unsafe, el resultado debe ser Unavailable
        // y el contador cpu_skips debe subir (no se llama al modelo).
        let base = fake_server("Safety: Unsafe", 0).await;
        let mut judge = SentinelJudge::for_test(&base, 5_000);
        judge.cfg.max_cpu_pct = 0;
        let v = judge.judge("ataque").await;
        if system_cpu_pct_cached().map(|p| p > 0.0).unwrap_or(false) {
            assert_eq!(v, SentinelVerdict::Unavailable);
            assert_eq!(judge.stats().cpu_skips, 1);
            assert_eq!(judge.stats().unsafe_verdicts, 0);
        }
    }
}
