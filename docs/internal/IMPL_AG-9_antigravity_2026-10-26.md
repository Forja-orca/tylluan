# Informe de Entrega: AG-9b — Causa Raíz de NightConsolidation Encadenada sin Reposo (Corregido)

**Fecha:** 2026-10-26  
**Agente:** @antigravity  
**Worktree:** `E:\tylluan-wt-antigravity`  
**Base commit:** `cd4a74e` (`origin/main`, push167)  
**Estado:** Diff sin commitear listo para revisión del Tech Lead (@claude-code)

---

## 1. Causa Raíz y Alcance de la Solución (Contexto T-DSH-101)

### Alcance y Naturaleza del Cambio:
A raíz de la medición en vivo de T-DSH-101 (donde se comprobó que el nuevo kernel PID 24436 se encuentra en reposo a ~0 núcleos una vez superado el arranque transitorio), **se hace constar explícitamente que AG-9 es un cambio de endurecimiento estructural contra ráfagas de CPU**, y **NO** la causa raíz del consumo anómalo acumulado durante horas del kernel anterior.

### Problema Específico Resuelto:
En `crates/tylluan-kernel/src/main.rs`:
```rust
let mut interval = tokio::time::interval(Duration::from_secs(night_config.interval_secs.max(1)));
loop {
    interval.tick().await;
    orchestrator.run_all(&ctx).await;
}
```
1. `tokio::time::interval` calcula los vencimientos desde el inicio de cada ciclo y aplica `MissedTickBehavior::Burst` por defecto.
2. Si un ciclo de consolidación tarda más que `interval_secs` (o si `interval_secs` se configura bajo, o durante ráfagas de indexación intensiva), los ticks acumulados vencen durante la ejecución de las fases y `tick().await` retorna inmediatamente sin conceder reposo a SQLite ni a los hilos de Tokio.
3. El intervalo fija la cadencia inicio-inicio. Si las 12 fases tardan 1600s de un intervalo de 1800s, el reposo efectivo es de apenas 200s; y si tardan 1800s o más, el reposo efectivo es de **0 segundos**.

---

## 2. Modificaciones Implementadas (Corrección AG-9b)

### A. `crates/tylluan-kernel/src/memory/night/mod.rs`
1. Se define la constante `MIN_NIGHT_CONSOLIDATION_REPOSE_SECS = 60` (suelo mínimo de seguridad de 60 segundos de reposo entre ciclos en entornos de producción).
2. Se introduce la función canónica:
   ```rust
   pub async fn run_night_consolidation_loop_with_repose(
       orchestrator: PhaseOrchestrator,
       ctx: PhaseContext,
       repose: std::time::Duration,
   ) {
       loop {
           orchestrator.run_all(&ctx).await;
           tokio::time::sleep(repose).await;
       }
   }
   ```
3. **Test unitario directo (sin tautología):**
   `test_night_consolidation_loop_waits_after_cycle` invoca directamente a `run_night_consolidation_loop_with_repose` pasándole `repose = Duration::from_millis(80)`:
   - Comprueba que durante la ejecución de la fase (~40ms) el contador es 1.
   - Comprueba que en la ventana de reposo (a t=70ms total, 30ms en reposo) el contador **sigue siendo 1** (no hay disparo inmediato).
   - Comprueba que únicamente tras transcurrir los 80ms de reposo (t > 120ms), arranca el segundo ciclo (contador pasa a 2).

### B. `crates/tylluan-kernel/src/main.rs`
En el punto de llamada de `main.rs`, el llamador computa la duración del reposo aplicando el suelo de seguridad:
```rust
let repose = Duration::from_secs(
    night_config.interval_secs.max(tylluan_kernel::memory::night::MIN_NIGHT_CONSOLIDATION_REPOSE_SECS)
);
tylluan_kernel::memory::night::run_night_consolidation_loop_with_repose(
    orchestrator,
    ctx,
    repose,
).await;
```

---

## 3. Verificaciones Técnicas Ejecutadas

Ejecutadas en `E:\tylluan-wt-antigravity`:

1. **Test unitario directo:**
   `cargo test --manifest-path crates/tylluan-kernel/Cargo.toml --lib test_night_consolidation_loop_waits_after_cycle`
   -> `test memory::night::tests::test_night_consolidation_loop_waits_after_cycle ... ok` (0.35s).

2. **Clippy estricto (-D warnings):**
   `cargo clippy --manifest-path crates/tylluan-kernel/Cargo.toml --all-targets -- -D warnings`
   -> Exit code 0, 0 advertencias, 0 errores.

3. **Test vigilante anti-mojibake:**
   `cargo test --manifest-path crates/tylluan-kernel/Cargo.toml --test mojibake_tripwire_test`
   -> `test test_no_mojibake_in_kernel_src ... ok` (1 passed, 0 failed).

4. **Suite completa kernel lib:**
   `cargo test --manifest-path crates/tylluan-kernel/Cargo.toml --lib`
   -> **`ok. 1297 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out`** (67.87s).

---

## 4. Archivos Modificados

```
 M crates/tylluan-kernel/src/main.rs
 M crates/tylluan-kernel/src/memory/night/mod.rs
```

---

## 5. ## Impact

- **Estabilidad / Endurecimiento:** Previene de forma determinista la concatenación sin descanso de ciclos de `NightConsolidation` si una corrida excede el intervalo configurado o si el reloj acumula ticks atrasados. Garantiza que siempre mediará una ventana de reposo (mínimo 60s en producción) medida **desde el fin** del ciclo previo, liberando SQLite para la actividad interactiva.
- **Invariantes:** Cero cambios en timeouts de guilds o de herramientas MCP. Cero impacto en la lógica de las fases.

---

## 6. Verificación de Entrega

No verificado en producción ni integrado en main hasta que el Tech Lead (@claude-code) revise e integre este lote.
