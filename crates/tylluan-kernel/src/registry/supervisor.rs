//! # Sovereign Supervisor
//! 
//! Periodically monitors "Always-On" guilds and automatically re-spawns
//! them if they crash or stop unexpectedly. Mimics systemd behavior.

use super::guild_process::GuildRegistry;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// D-115: número de intentos seguidos antes de dar el guild por degradado.
const SUPERVISOR_MAX_ATTEMPTS: u32 = 3;

/// D-115: techo del backoff del supervisor.
const SUPERVISOR_MAX_BACKOFF_SECS: u64 = 600;

/// D-140 (decisión de José, 06-nov-2026): tras `Degraded`, un reintento lento en
/// vez de quedarse mudo para siempre. Un guild que falla tres restauraciones
/// seguidas puede haberse caído por algo transitorio (presión de memoria); con
/// esto se recupera solo en una hora.
///
/// Precisión sobre el coste, para no venderlo mejor de lo que es: no es "un
/// intento por hora" sino **un ciclo de reintentos por hora** — al rearmarse el
/// contador, el backoff normal vuelve a permitir hasta `SUPERVISOR_MAX_ATTEMPTS`
/// respawns (2 s, 4 s) antes de volver a `Degraded`. Lo que queda acotado es el
/// goteo permanente, no el número absoluto de intentos.
const SUPERVISOR_SLOW_RETRY_SECS: u64 = 3_600;

/// D-115: ventana mínima de estabilidad. Si un guild restaurado sigue en pie
/// pasado este tiempo, se le perdona el historial de fallos.
const SUPERVISOR_MIN_STABILITY_SECS: u64 = 5;

/// D-115: estado del backoff de un guild. Era una tupla `(u32, Instant)` y por eso
/// no se podía probar: la decisión vivía enterrada en un bucle infinito.
#[derive(Debug)]
struct BackoffState {
    /// Fallos seguidos (no "respawns": fallos que NO sobrevivieron).
    attempts: u32,
    /// Cuándo se intentó el último respawn, para medir el backoff.
    last_attempt: Instant,
    /// Cuándo se dio el guild por restaurado y aún no ha demostrado que aguanta.
    /// `None` si no hay ninguna restauración pendiente de confirmar.
    restored_at: Option<Instant>,
    /// D-138 (nit del TL en T2925): `true` si ya se ha avisado del estado
    /// `Degraded`. Sin esto, el `error!` salía en **cada tick** (2 líneas/min para
    /// siempre) mientras el guild siguiera caído.
    degraded_logged: bool,
    /// Si **esta** llamada a `decide()` cruza a `Degraded`. El bucle loguea solo
    /// cuando esto es `true`.
    degraded_just_now: bool,
    /// D-140: cuándo se cruzó a `Degraded`. Marca el reloj del reintento lento.
    degraded_since: Option<Instant>,
}

impl BackoffState {
    fn new(now: Instant) -> Self {
        Self {
            attempts: 0,
            // Un guild recién visto no espera backoff: el primer respawn es ya.
            last_attempt: now
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or(now),
            restored_at: None,
            degraded_logged: false,
            degraded_just_now: false,
            degraded_since: None,
        }
    }
}

/// D-115: la decisión del supervisor, separada del bucle para poder probarla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SupervisorDecision {
    /// Vivo (y, si venía de una restauración, ya la ha confirmado).
    Healthy,
    /// Muerto y con el backoff cumplido: toca respawnear.
    Respawn,
    /// Muerto, pero dentro del backoff: se espera.
    InBackoff,
    /// Demasiados fallos seguidos: no se reintenta más.
    Degraded,
}

/// D-115: decide qué hacer con un guild, sin efectos sobre el registro.
///
/// El defecto que corrige: `ensure_guild_running` devuelve `Ok` cuando el guild
/// **conecta**, y el supervisor lo tomaba como éxito y ponía el contador a 0. Pero
/// conectar no es servir: en la tormenta medida, el supervisor logueaba
/// "Successfully restored" y **60 microsegundos después** el registro mataba el
/// guild. Resultado: contador 0 → restaurar → 0 → restaurar, sin escalar nunca, y
/// el tope de 3 intentos era inalcanzable.
///
/// Aquí una restauración solo se perdona si el guild **sigue vivo** pasado un
/// chequeo completo (`interval`, con un mínimo de 5 s). Si se muere antes, cuenta
/// como fallo.
fn decide(
    state: &mut BackoffState,
    running: bool,
    now: Instant,
    interval: Duration,
) -> SupervisorDecision {
    let window = interval.max(Duration::from_secs(SUPERVISOR_MIN_STABILITY_SECS));

    if running {
        // Sigue vivo: si venía de una restauración y ya pasó la ventana, se le
        // perdona el historial.
        if let Some(restored_at) = state.restored_at
            && now.duration_since(restored_at) >= window
        {
            state.restored_at = None;
            state.attempts = 0;
            state.degraded_logged = false;
            state.degraded_just_now = false;
            state.degraded_since = None;
        }
        return SupervisorDecision::Healthy;
    }

    // Muerto. Si había una restauración pendiente de confirmar, murió dentro de
    // la ventana: la restauración NO fue un éxito y el fallo cuenta.
    if state.restored_at.take().is_some() {
        state.attempts = state.attempts.saturating_add(1);
        state.last_attempt = now;
    }

    if state.attempts >= SUPERVISOR_MAX_ATTEMPTS {
        // D-140: en vez de quedarse en Degraded para siempre, un reintento por
        // hora. El aviso sigue saliendo UNA sola vez (D-138).
        match state.degraded_since {
            None => {
                state.degraded_since = Some(now);
                state.degraded_just_now = !state.degraded_logged;
                state.degraded_logged = true;
                return SupervisorDecision::Degraded;
            }
            Some(desde) => {
                if now.duration_since(desde) >= Duration::from_secs(SUPERVISOR_SLOW_RETRY_SECS) {
                    // Toca el intento lento: se rearma el contador y se reintenta una vez.
                    state.attempts = 0;
                    state.degraded_since = None;
                    state.degraded_logged = false;
                    state.degraded_just_now = false;
                    state.last_attempt = now;
                    return SupervisorDecision::Respawn;
                }
                state.degraded_just_now = false;
                return SupervisorDecision::Degraded;
            }
        }
    }

    let backoff_secs = 2u64
        .saturating_pow(state.attempts)
        .min(SUPERVISOR_MAX_BACKOFF_SECS);
    if now.duration_since(state.last_attempt) < Duration::from_secs(backoff_secs) {
        return SupervisorDecision::InBackoff;
    }

    SupervisorDecision::Respawn
}

/// Start the sovereign supervisor as a background task.
/// T13: Enhanced with exponential backoff to prevent CPU thrashing.
pub fn start_supervisor(
    registry: Arc<RwLock<GuildRegistry>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(check_interval_secs);
        let mut backoffs: HashMap<String, BackoffState> = HashMap::new();
        
        loop {
            tokio::time::sleep(interval).await;
            
            let mut reg = registry.write().await;
            
            // D-115: se miran TODOS los always-on, no solo los caídos: el contador
            // solo se puede perdonar si vemos vivo al que acabamos de restaurar.
            let guilds: Vec<(String, bool)> = reg.guilds
                .values()
                .filter(|g| g.always_on)
                .map(|g| (g.name.clone(), g.is_running()))
                .collect();
            
            for (name, running) in guilds {
                let entry = backoffs
                    .entry(name.clone())
                    .or_insert_with(|| BackoffState::new(Instant::now()));

                match decide(entry, running, Instant::now(), interval) {
                    SupervisorDecision::Healthy | SupervisorDecision::InBackoff => {}
                    SupervisorDecision::Degraded => {
                        // D-138: solo en la transición (antes salía cada tick).
                        if entry.degraded_just_now {
                            error!("💀 [Supervisor] Guild '{}' failed {} times in a row — marking as degraded, no further restarts", name, entry.attempts);
                        }
                    }
                    SupervisorDecision::Respawn => {
                        warn!("🔄 [Supervisor] Detected crash/stop in '{}' (Retry #{}).", name, entry.attempts);
                        entry.last_attempt = Instant::now();
                        // D-138: si vuelve a degradarse tras recuperarse, el aviso sale otra vez.
                        entry.degraded_logged = false;
                        entry.degraded_just_now = false;
                        match reg.ensure_guild_running(&name).await {
                            Ok(_) => {
                                info!("✅ [Supervisor] Successfully restored guild '{}'", name);
                                // D-115: conectar no es servir. NO se resetea aquí:
                                // se marca la restauración como pendiente de confirmar
                                // en el siguiente chequeo.
                                entry.restored_at = Some(Instant::now());
                            }
                            Err(e) => {
                                error!("❌ [Supervisor] Failure restoring '{}': {}", name, e);
                                entry.attempts = entry.attempts.saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn past(secs: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(secs))
            .expect("instante en el pasado")
    }

    /// La ventana de estabilidad que se usa en los tests.
    const WINDOW: Duration = Duration::from_secs(30);

    /// D-115: restaurar y morirse **cuenta como fallo**. Antes del arreglo el
    /// contador volvía a 0 con cada `Ok` de `ensure_guild_running`, así que este
    /// caso lo dejaba en 0 para siempre.
    #[test]
    fn d115_restaurar_y_morir_cuenta_como_fallo() {
        let now = Instant::now();
        let mut state = BackoffState::new(past(3600));
        state.restored_at = Some(now);

        let decision = decide(&mut state, false, now, WINDOW);

        assert_eq!(
            state.attempts, 1,
            "una restauración que no sobrevive debe contar como fallo"
        );
        assert_eq!(
            decision,
            SupervisorDecision::InBackoff,
            "y el reintento debe esperar su backoff, no ser inmediato"
        );
    }

    /// D-115: si sobrevive la ventana, sí se le perdona el historial.
    #[test]
    fn d115_sobrevivir_la_ventana_resetea() {
        let now = Instant::now();
        let mut state = BackoffState::new(past(3600));
        state.attempts = 2;
        state.restored_at = Some(past(40)); // 40 s > ventana de 30 s

        let decision = decide(&mut state, true, now, WINDOW);

        assert_eq!(decision, SupervisorDecision::Healthy);
        assert_eq!(state.attempts, 0, "ha aguantado: se le perdona");
        assert!(state.restored_at.is_none());
    }

    /// D-115: la ventana aún no se ha cumplido. El guild sigue vivo, pero el
    /// historial NO se borra todavía: no se perdona antes de tiempo.
    #[test]
    fn d115_vivo_pero_sin_cumplir_la_ventana_no_resetea() {
        let now = Instant::now();
        let mut state = BackoffState::new(past(3600));
        state.attempts = 2;
        state.restored_at = Some(past(3)); // 3 s < ventana de 30 s

        let decision = decide(&mut state, true, now, WINDOW);

        assert_eq!(decision, SupervisorDecision::Healthy);
        assert_eq!(state.attempts, 2, "todavía no ha demostrado que aguanta");
        assert!(state.restored_at.is_some());
    }

    /// D-115: el bucle de la tormenta **termina**.
    ///
    /// Simula el supervisor tick a tick: el registro restaura el guild y lo mata
    /// dentro de la ventana. Antes del arreglo el contador se quedaba en 0 y los
    /// respawns no paraban nunca; ahora escala y acaba en `Degraded`.
    #[test]
    fn d115_el_bucle_de_la_tormenta_termina_en_degraded() {
        let mut state = BackoffState::new(past(3600));
        let mut now = Instant::now();
        let mut decisiones = Vec::new();
        let mut attempts_al_respawn = Vec::new();

        for _ in 0..8 {
            now = now
                .checked_add(WINDOW)
                .expect("instante futuro"); // un tick del supervisor
            let decision = decide(&mut state, false, now, WINDOW);
            if decision == SupervisorDecision::Respawn {
                attempts_al_respawn.push(state.attempts);
                // El registro lo "restaura"… y lo mata antes del siguiente tick.
                state.restored_at = Some(now);
            }
            decisiones.push(decision);
        }

        assert_eq!(
            attempts_al_respawn,
            vec![0, 1, 2],
            "solo se reintenta 3 veces, y el contador va escalando: {decisiones:?}"
        );
        assert_eq!(
            decisiones.iter().filter(|d| **d == SupervisorDecision::Respawn).count(),
            3,
            "la tormenta debe acotarse a 3 respawns: {decisiones:?}"
        );
        assert_eq!(
            decisiones.last(),
            Some(&SupervisorDecision::Degraded),
            "al final debe quedar degradado y dejar de reintentar: {decisiones:?}"
        );
    }

    /// D-115: el backoff sigue creciendo 1 s, 2 s, 4 s con los intentos (no se ha
    /// roto T13 al cambiar el reset).
    #[test]
    fn d115_el_backoff_crece_con_los_intentos() {
        let now = Instant::now();
        // Márgenes de ±1 s a propósito: comparar justo en el umbral hace que el
        // test dependa de los microsegundos entre dos `Instant::now()`.
        for (attempts, elapsed, esperado) in [
            (0u32, 2u64, SupervisorDecision::Respawn),   // 2^0 = 1 s
            (1, 1, SupervisorDecision::InBackoff),       // necesita 2 s
            (1, 3, SupervisorDecision::Respawn),
            (2, 3, SupervisorDecision::InBackoff),       // necesita 4 s
            (2, 5, SupervisorDecision::Respawn),
        ] {
            let mut state = BackoffState::new(now);
            state.attempts = attempts;
            state.last_attempt = past(elapsed);
            assert_eq!(
                decide(&mut state, false, now, WINDOW),
                esperado,
                "attempts={attempts} elapsed={elapsed}s"
            );
        }
    }
}

#[cfg(test)]
mod tests_d138 {
    use super::*;

    fn pasado(segundos: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(segundos))
            .expect("instante pasado")
    }

    /// D-138 (nit del TL en T2925): el aviso de `Degraded` salía en **cada tick**.
    /// Ahora `decide()` marca `degraded_just_now` **solo la primera vez**.
    #[test]
    fn d138_el_aviso_de_degraded_sale_una_sola_vez() {
        let now = Instant::now();
        let mut state = BackoffState::new(pasado(3600));
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;

        // Primer tick en Degraded: toca avisar.
        assert_eq!(
            decide(&mut state, false, now, Duration::from_secs(30)),
            SupervisorDecision::Degraded
        );
        assert!(state.degraded_just_now, "la transición tiene que avisar");

        // Segundo y tercer tick: sigue Degraded, pero YA NO avisa.
        for _ in 0..2 {
            assert_eq!(
                decide(&mut state, false, now, Duration::from_secs(30)),
                SupervisorDecision::Degraded
            );
            assert!(!state.degraded_just_now, "no puede repetirse el aviso");
        }
    }

    /// Y si el guild se recupera y vuelve a degradarse, el aviso vuelve a salir.
    #[test]
    fn d138_tras_recuperarse_el_aviso_vuelve() {
        let now = Instant::now();
        let mut state = BackoffState::new(pasado(3600));
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;
        let _ = decide(&mut state, false, now, Duration::from_secs(30));
        assert!(state.degraded_just_now);

        // Vivo y con la ventana cumplida: se perdona y se rearma.
        state.restored_at = Some(pasado(60));
        assert_eq!(
            decide(&mut state, true, now, Duration::from_secs(30)),
            SupervisorDecision::Healthy
        );
        assert!(!state.degraded_logged, "al recuperarse se rearma el aviso");

        // Cae otra vez hasta el tope: debe volver a avisar.
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;
        let _ = decide(&mut state, false, now, Duration::from_secs(30));
        assert!(state.degraded_just_now, "tras recuperarse, el aviso vuelve");
    }
}

#[cfg(test)]
mod tests_d140 {
    use super::*;

    fn pasado(segundos: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(segundos))
            .expect("instante pasado")
    }

    /// D-140: dentro de la hora sigue en `Degraded` y **no** repite el aviso.
    #[test]
    fn d140_degraded_espera_la_hora() {
        let now = Instant::now();
        let mut state = BackoffState::new(pasado(3600));
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;

        // Primer tick: cruza a Degraded y avisa.
        assert_eq!(
            decide(&mut state, false, now, Duration::from_secs(30)),
            SupervisorDecision::Degraded
        );
        assert!(state.degraded_just_now);
        assert!(state.degraded_since.is_some());

        // A los 59 minutos: sigue Degraded y ya no avisa.
        let casi = now + Duration::from_secs(SUPERVISOR_SLOW_RETRY_SECS - 60);
        assert_eq!(
            decide(&mut state, false, casi, Duration::from_secs(30)),
            SupervisorDecision::Degraded
        );
        assert!(!state.degraded_just_now);
    }

    /// D-140: cumplida la hora, reintenta y rearma el contador.
    #[test]
    fn d140_pasada_la_hora_reintenta_una_vez() {
        let now = Instant::now();
        let mut state = BackoffState::new(pasado(3600));
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;
        let _ = decide(&mut state, false, now, Duration::from_secs(30));

        let despues = now + Duration::from_secs(SUPERVISOR_SLOW_RETRY_SECS);
        assert_eq!(
            decide(&mut state, false, despues, Duration::from_secs(30)),
            SupervisorDecision::Respawn,
            "pasada la hora tiene que reintentar, no quedarse mudo"
        );
        assert_eq!(state.attempts, 0, "el contador se rearma para el ciclo lento");
        assert!(state.degraded_since.is_none());
        assert!(!state.degraded_logged, "si vuelve a caer, avisará otra vez");
    }

    /// D-140: recuperarse rearma **también** el reloj del reintento lento. Sin
    /// esto, un guild que se degradó hace más de una hora volvería a reintentar
    /// al instante en su siguiente caída, saltándose la espera.
    #[test]
    fn d140_la_recuperacion_rearma_el_reloj() {
        let now = Instant::now();
        let mut state = BackoffState::new(pasado(3600));
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;
        let _ = decide(&mut state, false, now, Duration::from_secs(30));
        assert!(state.degraded_since.is_some());

        // Vivo y con la ventana de estabilidad cumplida: se perdona el historial.
        state.restored_at = Some(pasado(60));
        assert_eq!(
            decide(&mut state, true, now, Duration::from_secs(30)),
            SupervisorDecision::Healthy
        );
        assert!(
            state.degraded_since.is_none(),
            "al recuperarse, el reloj del reintento lento tiene que rearmarse"
        );

        // Y si vuelve a caer hasta el tope, la hora se cuenta desde AHORA: a los
        // 59 minutos sigue esperando, no reintenta por el reloj viejo.
        state.attempts = SUPERVISOR_MAX_ATTEMPTS;
        let ahora2 = now + Duration::from_secs(1);
        assert_eq!(
            decide(&mut state, false, ahora2, Duration::from_secs(30)),
            SupervisorDecision::Degraded
        );
        let casi = ahora2 + Duration::from_secs(SUPERVISOR_SLOW_RETRY_SECS - 60);
        assert_eq!(
            decide(&mut state, false, casi, Duration::from_secs(30)),
            SupervisorDecision::Degraded,
            "el reloj viejo no puede servir para reintentar ya"
        );
    }

    /// Y el comportamiento normal no cambia: con intentos por debajo del tope, backoff.
    #[test]
    fn d140_el_backoff_normal_sigue_igual() {
        let now = Instant::now();
        let mut state = BackoffState::new(now);
        state.attempts = 1;
        state.last_attempt = pasado(1);
        assert_eq!(
            decide(&mut state, false, now, Duration::from_secs(30)),
            SupervisorDecision::InBackoff
        );
    }
}
