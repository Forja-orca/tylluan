// Fase 2 (D-15) — consentimiento de operador para OAuth.
//
// Spec: `docs/internal/PLAN_oauth_f2_consentimiento_deep_2026-10.md` (contrato
// congelado por José, 2026-10-19). Este módulo contiene SOLO el estado y el
// HTML de la página P1; los handlers viven en `oauth.rs`.
//
// Decisiones aprobadas que este módulo materializa:
//   1. Una sola pantalla (P1) para el primer consentimiento; sin asistentes.
//   2. Consentimiento recordado por cliente+redirect_uri, 30 días DESLIZANTES.
//   3. `scope=operator` NUNCA se concede (lo rechaza `oauth.rs` antes de aquí).
//   4. Revocación = editar `consents.json` (no hay endpoint). Por eso el
//      fichero se relee cuando cambia: revocar surte efecto sin reiniciar.
//   5. No se guarda ninguna credencial. `consents.json` solo tiene metadatos.
//
// El HTML es autocontenido (sin JS, sin build del dashboard): el kernel es
// headless-first y la página tiene que funcionar servida por `axum` tal cual.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Consentimiento recordado: 30 días deslizantes (decisión 2).
pub const CONSENT_TTL_DAYS: i64 = 30;
/// Vida de la petición de consentimiento en pantalla (P1). Si el humano no
/// decide en 5 minutos, la página ya no sirve: se vuelve a pedir.
pub const PENDING_TTL_SECS: i64 = 300;
/// Intentos de credencial por IP antes de bloquear (P6).
pub const MAX_FAILURES: usize = 5;
/// Ventana del bloqueo por IP, en segundos.
pub const FAILURE_WINDOW_SECS: i64 = 300;
/// Tope global de fallos en la ventana: una sola IP no es el único vector
/// (decenas de loopbacks/proxies locales), así que además del cupo por IP hay
/// un techo agregado.
pub const GLOBAL_MAX_FAILURES: usize = 20;
/// Fichero de consentimientos (mismo directorio que `oauth_jwt_secret`).
pub const CONSENTS_FILE: &str = "consents.json";

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

/// Registro persistido. NUNCA contiene la credencial del operador ni el code:
/// solo quién pidió qué y hasta cuándo vale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsentRecord {
    pub client_id: String,
    #[serde(default)]
    pub client_name: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub granted_at: i64,
    pub last_used_at: i64,
    pub expires_at: i64,
    /// Revocación manual: cualquier valor no nulo lo invalida. El operador
    /// añade `"revoked_at": 1` (o borra la entrada) y el siguiente
    /// `/oauth/authorize` vuelve a pedir consentimiento.
    #[serde(default)]
    pub revoked_at: Option<i64>,
}

/// Petición de consentimiento viva (en memoria, 5 minutos). No se persiste:
/// si el kernel reinicia a mitad de una aprobación, el cliente reintenta.
#[derive(Debug, Clone)]
pub struct PendingConsent {
    pub consent_id: String,
    pub csrf: String,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub redirect_host: String,
    pub code_challenge: String,
    pub state: Option<String>,
    pub resource: Option<String>,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsentFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    consents: Vec<ConsentRecord>,
}

fn default_version() -> u32 {
    1
}

#[derive(Default)]
struct StoreState {
    consents: HashMap<String, ConsentRecord>,
    pending: HashMap<String, PendingConsent>,
    failures: HashMap<String, Vec<i64>>,
    global_failures: Vec<i64>,
}

pub struct ConsentStore {
    /// `None` = solo memoria (tests, o kernel sin directorio de datos).
    path: Option<PathBuf>,
    state: Mutex<StoreState>,
    loaded_mtime: Mutex<Option<SystemTime>>,
}

impl std::fmt::Debug for ConsentStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsentStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ConsentStore {
    /// Store efímero: los consentimientos no sobreviven al proceso.
    pub fn in_memory() -> Self {
        Self {
            path: None,
            state: Mutex::new(StoreState::default()),
            loaded_mtime: Mutex::new(None),
        }
    }

    /// Store persistido en `dir/consents.json`. Nunca hace fallar el arranque:
    /// si el fichero no se puede leer se empieza vacío (el peor caso es volver
    /// a pedir consentimiento, nunca dejar a alguien fuera).
    pub fn with_file(dir: &Path) -> Self {
        let store = Self {
            path: Some(dir.join(CONSENTS_FILE)),
            state: Mutex::new(StoreState::default()),
            loaded_mtime: Mutex::new(None),
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("oauth consent: no se pudo crear {} ({e})", dir.display());
        }
        store.reload_if_changed();
        store
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn key(client_id: &str, redirect_uri: &str) -> String {
        format!("{client_id}|{redirect_uri}")
    }

    /// Relee el fichero si cambió en disco (revocación editando el JSON).
    fn reload_if_changed(&self) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let mtime = meta.modified().ok();
        {
            let mut loaded = self.loaded_mtime.lock().unwrap_or_else(|e| e.into_inner());
            if *loaded == mtime {
                return;
            }
            *loaded = mtime;
        }
        let parsed = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<ConsentFile>(&raw).ok());
        match parsed {
            Some(file) => {
                let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                st.consents = file
                    .consents
                    .into_iter()
                    .map(|c| (Self::key(&c.client_id, &c.redirect_uri), c))
                    .collect();
            }
            None => tracing::warn!(
                "oauth consent: {} no se pudo parsear; se mantiene el estado en memoria",
                path.display()
            ),
        }
    }

    fn persist(&self) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let file = {
            let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            ConsentFile {
                version: 1,
                consents: st.consents.values().cloned().collect(),
            }
        };
        let body = match serde_json::to_string_pretty(&file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("oauth consent: no se pudo serializar ({e})");
                return;
            }
        };
        let tmp = path.with_extension("json.tmp");
        let write = std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, path));
        match write {
            Ok(()) => {
                if let Ok(meta) = std::fs::metadata(path)
                    && let Ok(mtime) = meta.modified()
                {
                    *self.loaded_mtime.lock().unwrap_or_else(|e| e.into_inner()) = Some(mtime);
                }
            }
            Err(e) => tracing::warn!(
                "oauth consent: no se pudo escribir {} ({e}); el consentimiento vale solo en este proceso",
                path.display()
            ),
        }
    }

    /// Nombre a mostrar del cliente: el registrado en su consentimiento si
    /// existe, o el `client_id` si es el primer contacto (el plan prohíbe
    /// inventarse un nombre).
    pub fn display_name(&self, client_id: &str, redirect_uri: &str) -> Option<String> {
        self.reload_if_changed();
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.consents
            .get(&Self::key(client_id, redirect_uri))
            .map(|c| c.client_name.clone())
            .filter(|n| !n.is_empty())
    }

    /// ¿Hay consentimiento vigente para cliente+redirect? Si lo hay, desliza
    /// la caducidad (30 días desde AHORA) y lo persiste.
    pub fn valid_consent(&self, client_id: &str, redirect_uri: &str) -> Option<ConsentRecord> {
        self.reload_if_changed();
        let now = now_unix();
        let key = Self::key(client_id, redirect_uri);
        let existing = {
            let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.consents.get(&key).cloned()
        }?;
        if existing.revoked_at.is_some() || existing.expires_at <= now {
            return None;
        }
        let slid = ConsentRecord {
            last_used_at: now,
            expires_at: now + CONSENT_TTL_DAYS * 86_400,
            ..existing
        };
        {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.consents.insert(key, slid.clone());
        }
        self.persist();
        tracing::info!(
            target: "oauth_consent",
            "decision=granted_cached client_id={} redirect={} scopes={} expires_at={}",
            slid.client_id,
            slid.redirect_uri,
            slid.scopes.join(","),
            slid.expires_at
        );
        Some(slid)
    }

    /// Guarda la aprobación del operador (30 días desde ahora).
    pub fn record_consent(
        &self,
        client_id: &str,
        client_name: &str,
        redirect_uri: &str,
        scopes: &[String],
    ) -> ConsentRecord {
        let now = now_unix();
        // Un nombre que el operador haya escrito a mano en `consents.json`
        // sobrevive a una re-aprobacion: el cliente no puede sobrescribirlo
        // (el `client_id` lo controla el cliente, el nombre no).
        let preserved_name = {
            let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.consents
                .get(&Self::key(client_id, redirect_uri))
                .map(|c| c.client_name.clone())
                .filter(|n| !n.is_empty() && n != client_id)
        };
        let record = ConsentRecord {
            client_id: client_id.to_string(),
            client_name: preserved_name.unwrap_or_else(|| client_name.to_string()),
            redirect_uri: redirect_uri.to_string(),
            scopes: scopes.to_vec(),
            granted_at: now,
            last_used_at: now,
            expires_at: now + CONSENT_TTL_DAYS * 86_400,
            revoked_at: None,
        };
        {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.consents
                .insert(Self::key(client_id, redirect_uri), record.clone());
        }
        self.persist();
        tracing::info!(
            target: "oauth_consent",
            "decision=approved client_id={} redirect={} scopes={} expires_at={}",
            record.client_id,
            record.redirect_uri,
            record.scopes.join(","),
            record.expires_at
        );
        record
    }

    pub fn create_pending(&self, mut p: PendingConsent) -> PendingConsent {
        let now = now_unix();
        p.consent_id = random_token();
        p.csrf = random_token();
        p.created_at = now;
        p.expires_at = now + PENDING_TTL_SECS;
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Barrido perezoso: sin temporizadores propios (headless-first).
        st.pending.retain(|_, v| v.expires_at > now);
        st.pending.insert(p.consent_id.clone(), p.clone());
        p
    }

    pub fn peek_pending(&self, consent_id: &str) -> Option<PendingConsent> {
        let now = now_unix();
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.pending
            .get(consent_id)
            .filter(|p| p.expires_at > now)
            .cloned()
    }

    pub fn take_pending(&self, consent_id: &str) -> Option<PendingConsent> {
        let now = now_unix();
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match st.pending.remove(consent_id) {
            Some(p) if p.expires_at > now => Some(p),
            _ => None,
        }
    }

    /// Rota el CSRF conservando la petición: un intento fallido de credencial
    /// re-renderiza P1 sin obligar al cliente a repetir `/oauth/authorize`.
    pub fn refresh_csrf(&self, consent_id: &str) -> Option<String> {
        let csrf = random_token();
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.pending.get_mut(consent_id).map(|p| {
            p.csrf = csrf.clone();
            csrf
        })
    }

    pub fn drop_pending(&self, consent_id: &str) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.pending.remove(consent_id);
    }

    fn prune_failures(&self, ip: &str, now: i64) -> bool {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.global_failures
            .retain(|t| now - *t < FAILURE_WINDOW_SECS);
        let entry = st.failures.entry(ip.to_string()).or_default();
        entry.retain(|t| now - *t < FAILURE_WINDOW_SECS);
        entry.len() >= MAX_FAILURES || st.global_failures.len() >= GLOBAL_MAX_FAILURES
    }

    pub fn is_locked(&self, ip: &str) -> bool {
        self.prune_failures(ip, now_unix())
    }

    pub fn retry_after_secs(&self, ip: &str) -> i64 {
        let now = now_unix();
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let oldest = st
            .failures
            .get(ip)
            .and_then(|v| v.first().copied())
            .or_else(|| st.global_failures.first().copied());
        oldest.map(|t| (t + FAILURE_WINDOW_SECS - now).max(0)).unwrap_or(0)
    }

    pub fn record_failure(&self, ip: &str) {
        let now = now_unix();
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.failures.entry(ip.to_string()).or_default().push(now);
        st.global_failures.push(now);
        tracing::warn!(
            target: "oauth_consent",
            "decision=credential_rejected ip={ip} failures={}",
            st.failures.get(ip).map(|v| v.len()).unwrap_or(0)
        );
    }

    pub fn clear_failures(&self, ip: &str) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.failures.remove(ip);
    }
}

/// `scheme://host[:port]` del redirect — lo ÚNICO que se muestra en P1
/// (decisión de José: el hostname del redirect, no la URI con su path).
pub fn redirect_host(uri: &str) -> String {
    let Some((scheme, rest)) = uri.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{authority}")
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Vista de P1.
pub struct ConsentPage<'a> {
    pub client_id: &'a str,
    pub client_name: &'a str,
    pub redirect_host: &'a str,
    pub kernel_origin: &'a str,
    pub consent_id: &'a str,
    pub csrf: &'a str,
    pub nonce: &'a str,
    pub error: Option<&'a str>,
}

/// HTML de P1. Sin JS (CSP `default-src 'none'`), sin recursos externos, con
/// estilo inline bajo nonce. Todos los valores del cliente van escapados.
pub fn render_consent_page(v: &ConsentPage<'_>) -> String {
    let client_id = html_escape(v.client_id);
    let client_name = html_escape(v.client_name);
    let client_block = if client_name == client_id {
        format!(r#"<strong>{client_id}</strong> <span class="hint">(sin nombre registrado)</span>"#)
    } else {
        format!(r#"<strong>{client_name}</strong> <code>{client_id}</code>"#)
    };
    let redirect_host = html_escape(v.redirect_host);
    let kernel_origin = html_escape(v.kernel_origin);
    let consent_id = html_escape(v.consent_id);
    let csrf = html_escape(v.csrf);
    let error = v.error.map(html_escape).unwrap_or_default();
    let error_block = if error.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="err" role="alert">{error}</p>"#
        )
    };
    let nonce = html_escape(v.nonce);
    format!(
        r#"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<title>Autorizar acceso a Tylluan</title>
<style nonce="{nonce}">
:root{{color-scheme:light dark}}
*{{box-sizing:border-box}}
body{{margin:0;padding:32px 24px;font:16px/1.5 system-ui,sans-serif;background:#f6f5f2;color:#1c1b18}}
main{{max-width:620px;margin:0 auto;background:#ffffff;border:1px solid #d8d4cc;border-radius:12px;padding:24px 28px}}
h1{{font-size:22px;margin:0 0 4px}}
p.sub{{margin:0 0 20px;color:#4a463f}}
dl{{display:grid;grid-template-columns:max-content 1fr;gap:8px 16px;margin:0 0 20px}}
dt{{color:#4a463f}}
dd{{margin:0}}
code{{font:14px/1.4 ui-monospace,monospace;background:#f0eee9;border:1px solid #d8d4cc;border-radius:4px;padding:1px 6px;word-break:break-all}}
.host{{font-size:18px;font-weight:600}}
ul.tools{{margin:8px 0 20px;padding-left:20px;columns:2;color:#4a463f;font-size:14px}}
form{{border-top:1px solid #e4e1da;padding-top:20px}}
label{{display:block;font-weight:600;margin-bottom:6px}}
input[type=password]{{width:100%;padding:10px 12px;font:16px system-ui,sans-serif;border:1px solid #8c877d;border-radius:8px;background:#fff;color:#1c1b18}}
.actions{{display:flex;gap:12px;margin-top:16px;flex-wrap:wrap}}
button{{font:16px system-ui,sans-serif;padding:10px 18px;border-radius:8px;border:1px solid #1c1b18;cursor:pointer}}
.primary{{background:#1c1b18;color:#fff}}
.secondary{{background:#fff;color:#1c1b18}}
.err{{background:#fbe9e7;border:1px solid #b3261e;color:#8c1d18;border-radius:8px;padding:10px 12px;margin:0 0 16px}}
footer{{max-width:620px;margin:16px auto 0;color:#4a463f;font-size:13px}}
</style>
</head>
<body>
<main>
<h1>Autorizar acceso a Tylluan</h1>
<p class="sub">Un cliente quiere conectarse a este kernel. Solo tú puedes aprobarlo.</p>
{error_block}
<dl>
<dt>Cliente</dt><dd>{client_block}</dd>
<dt>Destino del código</dt><dd><span class="host">{redirect_host}</span></dd>
<dt>Kernel</dt><dd><code>{kernel_origin}</code></dd>
<dt>Permiso</dt><dd>Herramientas soberanas (<code>mcp</code>): 6 tools. No incluye operación.</dd>
</dl>
<ul class="tools">
<li>tylluan_do</li>
<li>tylluan_remember</li>
<li>tylluan_recall</li>
<li>tylluan_think</li>
<li>tylluan_graph</li>
<li>tylluan_decide</li>
</ul>
<form method="post" action="/oauth/authorize">
<input type="hidden" name="consent_id" value="{consent_id}">
<input type="hidden" name="csrf" value="{csrf}">
<label for="credential">Credencial de operador</label>
<input id="credential" name="credential" type="password" autocomplete="off" autofocus required>
<div class="actions">
<button class="primary" type="submit" name="decision" value="approve">Aprobar</button>
<button class="secondary" type="submit" name="decision" value="deny">Denegar</button>
</div>
</form>
</main>
<footer>Si apruebas, este cliente se recuerda 30 días. Para revocar, edita <code>consents.json</code> en el directorio de datos y añade <code>"revoked_at": 1</code>.</footer>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(store: &ConsentStore, client: &str) -> PendingConsent {
        store.create_pending(PendingConsent {
            consent_id: String::new(),
            csrf: String::new(),
            client_id: client.to_string(),
            client_name: client.to_string(),
            redirect_uri: "http://127.0.0.1:9999/cb".to_string(),
            redirect_host: "http://127.0.0.1:9999".to_string(),
            code_challenge: "challenge".to_string(),
            state: Some("st".to_string()),
            resource: None,
            scopes: vec!["mcp".to_string()],
            created_at: 0,
            expires_at: 0,
        })
    }

    #[test]
    fn consent_is_valid_for_thirty_days_and_slides_on_use() {
        let store = ConsentStore::in_memory();
        let rec = store.record_consent("c1", "Cliente", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        assert_eq!(rec.expires_at, rec.granted_at + 30 * 86_400);
        let first = store
            .valid_consent("c1", "http://127.0.0.1:9999/cb")
            .expect("vigente");
        assert!(first.last_used_at >= rec.granted_at);
        // Otro redirect del mismo cliente no hereda el consentimiento.
        assert!(store.valid_consent("c1", "http://127.0.0.1:5000/cb").is_none());
    }

    #[test]
    fn operator_written_client_name_survives_reapproval() {
        let store = ConsentStore::in_memory();
        store.record_consent("c1", "c1", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        {
            let mut st = store.state.lock().unwrap();
            st.consents
                .get_mut(&ConsentStore::key("c1", "http://127.0.0.1:9999/cb"))
                .unwrap()
                .client_name = "Claude Desktop".to_string();
        }
        let rec = store.record_consent("c1", "c1", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        assert_eq!(rec.client_name, "Claude Desktop");
        assert_eq!(
            store.display_name("c1", "http://127.0.0.1:9999/cb").unwrap(),
            "Claude Desktop"
        );
    }

    #[test]
    fn revoked_or_expired_consent_is_not_valid() {
        let store = ConsentStore::in_memory();
        store.record_consent("c1", "Cliente", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        {
            let mut st = store.state.lock().unwrap();
            let rec = st
                .consents
                .get_mut(&ConsentStore::key("c1", "http://127.0.0.1:9999/cb"))
                .unwrap();
            rec.revoked_at = Some(1);
        }
        assert!(store.valid_consent("c1", "http://127.0.0.1:9999/cb").is_none());

        store.record_consent("c2", "Cliente", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        {
            let mut st = store.state.lock().unwrap();
            let rec = st
                .consents
                .get_mut(&ConsentStore::key("c2", "http://127.0.0.1:9999/cb"))
                .unwrap();
            rec.expires_at = now_unix() - 1;
        }
        assert!(store.valid_consent("c2", "http://127.0.0.1:9999/cb").is_none());
    }

    #[test]
    fn revoking_by_editing_the_file_takes_effect_without_restart() {
        let dir = std::env::temp_dir().join(format!("tylluan_consent_{}", random_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConsentStore::with_file(&dir);
        store.record_consent("c1", "Cliente", "http://127.0.0.1:9999/cb", &["mcp".into()]);
        assert!(store.valid_consent("c1", "http://127.0.0.1:9999/cb").is_some());

        let raw = std::fs::read_to_string(dir.join(CONSENTS_FILE)).unwrap();
        // No hay credencial en el fichero (decisión 5).
        assert!(!raw.contains("credential"));
        let tampered = raw.replace("\"revoked_at\": null", "\"revoked_at\": 1");
        assert_ne!(raw, tampered, "el fichero debe traer revoked_at");
        // El mtime tiene resolución de 1s en algunos FS: forzamos cambio visible.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join(CONSENTS_FILE), tampered).unwrap();
        assert!(store.valid_consent("c1", "http://127.0.0.1:9999/cb").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_consent_expires_and_csrf_rotates() {
        let store = ConsentStore::in_memory();
        let p = pending(&store, "c1");
        assert!(store.peek_pending(&p.consent_id).is_some());
        assert_eq!(store.peek_pending(&p.consent_id).unwrap().csrf, p.csrf);
        let rotated = store.refresh_csrf(&p.consent_id).unwrap();
        assert_ne!(rotated, p.csrf);
        {
            let mut st = store.state.lock().unwrap();
            st.pending.get_mut(&p.consent_id).unwrap().expires_at = now_unix() - 1;
        }
        assert!(store.peek_pending(&p.consent_id).is_none());
        assert!(store.take_pending(&p.consent_id).is_none());
    }

    #[test]
    fn rate_limit_locks_after_five_failures() {
        let store = ConsentStore::in_memory();
        for _ in 0..MAX_FAILURES - 1 {
            store.record_failure("127.0.0.1");
        }
        assert!(!store.is_locked("127.0.0.1"));
        store.record_failure("127.0.0.1");
        assert!(store.is_locked("127.0.0.1"));
        assert!(store.retry_after_secs("127.0.0.1") > 0);
        store.clear_failures("127.0.0.1");
        assert!(!store.is_locked("127.0.0.1"));
    }

    #[test]
    fn html_escapes_client_controlled_values() {
        let page = render_consent_page(&ConsentPage {
            client_id: "<script>alert(1)</script>",
            client_name: "\"><img src=x>",
            redirect_host: "http://127.0.0.1:9999",
            kernel_origin: "http://127.0.0.1:47004",
            consent_id: "id",
            csrf: "csrf",
            nonce: "n",
            error: Some("<b>mal</b>"),
        });
        assert!(!page.contains("<script>alert(1)</script>"));
        assert!(page.contains("&lt;script&gt;"));
        assert!(page.contains("&quot;&gt;&lt;img"));
        assert!(!page.contains("<b>mal</b>"));
    }

    #[test]
    fn redirect_host_keeps_only_scheme_and_authority() {
        assert_eq!(redirect_host("http://127.0.0.1:9999/cb?x=1"), "http://127.0.0.1:9999");
        assert_eq!(redirect_host("http://localhost/cb"), "http://localhost");
        assert_eq!(redirect_host("nope"), "");
    }
}
