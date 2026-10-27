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

struct ServerState {
    child: Option<Child>,
    port: u16,
    last_used: Instant,
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
    }
}

/// Juez centinela con servidor propio bajo demanda y cola serie.
pub struct SentinelJudge {
    cfg: SentinelConfig,
    state: Mutex<ServerState>,
    http: reqwest::Client,
    /// Solo tests: base URL de un servidor falso (no se arranca llama-server).
    test_base: Option<String>,
}

impl SentinelJudge {
    pub fn new(cfg: SentinelConfig) -> Self {
        Self {
            cfg,
            state: Mutex::new(ServerState {
                child: None,
                port: 0,
                last_used: Instant::now(),
            }),
            http: reqwest::Client::new(),
            test_base: None,
        }
    }

    /// Constructor de tests: apunta a un servidor falso ya levantado por el test.
    #[cfg(test)]
    fn for_test(base: &str, timeout_ms: u64) -> Self {
        let mut judge = Self::new(SentinelConfig {
            enabled: true,
            timeout_ms,
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
        if !self.cfg.enabled {
            return SentinelVerdict::Unavailable;
        }
        let mut st = self.state.lock().await;
        if st.alive_mut() && st.last_used.elapsed() > Duration::from_secs(self.cfg.idle_unload_secs) {
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
                return SentinelVerdict::Unavailable;
            }
            Err(_) => {
                tracing::warn!("[T-DSH-109] centinela: timeout {}ms (fail-open)", self.cfg.timeout_ms);
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
                return SentinelVerdict::Unavailable;
            }
        };
        st.last_used = Instant::now();
        classify_verdict(&raw)
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

/// Juez global perezoso (config cargada una vez). `Unavailable` si está apagado.
pub fn global() -> Arc<SentinelJudge> {
    static GLOBAL: std::sync::OnceLock<Arc<SentinelJudge>> = std::sync::OnceLock::new();
    GLOBAL
        .get_or_init(|| {
            let cfg = crate::config::TylluanConfig::load_cached()
                .ok()
                .and_then(|c| c.try_read().ok().map(|g| g.security.sentinel.clone()))
                .unwrap_or_default();
            Arc::new(SentinelJudge::new(cfg))
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
}
