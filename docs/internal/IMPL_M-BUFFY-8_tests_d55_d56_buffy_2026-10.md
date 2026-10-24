# IMPL M-BUFFY-8 — Tests de regresión de D-55 y D-56 desde fuera del crate

**Fecha:** 2026-10-25 · **Agente:** Buffy · **Estado:** entregado sin commitear (pendiente de Claude)
**Asignación:** Coloquio turno 1719 (TL): «Buffy: M-BUFFY-8: tests de regresion para D-55 (prefijos de unidad en ingest/files) y D-56 (peer no queda aprobado) desde fuera del crate, sin tocar src/.»

**HALLAZGO NUEVO EN EL PROCESO (propongo número D-58, a confirmar por el TL): deadlock de RwLock en `federation_approve_peer` — D-56 NO queda cerrado end-to-end en `main`.** Detalle en la sección «Hallazgo» más abajo.

## Preparación (sincronización)

Antes de tocar nada: `git fetch` + comparación. Mi HEAD estaba en `97f89a4` (push144 incompleto) y `origin/main` ya estaba en `42a76a1` con **push145 entero (D-54/D-55/D-56 + docs)**. El ff estaba bloqueado por 4 ficheros míos sin commitear de M-BUFFY-5/6; verifiqué **uno a uno que eran byte a byte idénticos a los de `main`** (incluido el test asi06 distinguiendo diferencia de fin de línea CRLF/LF de diferencia real: `diff_exit=0`) antes de normalizarlos y hacer ff. Cero contenido único perdido. `src/` quedó limpio = estado de `main`.

## Entrega (2 ficheros, cero cambios en `src/`)

1. **`crates/tylluan-kernel/tests/regression_d55_d56_outside_crate.rs`** — 2 tests contra el router HTTP real (`build_router`), estilo de harness de M-BUFFY-7:
   - **`d55_ingest_files_rejects_unit_prefix_and_traversal_names`** — `GET /api/v1/ingest/files/{filename}` con `C:foo`, `a:b`, `..`, `..%5Cevil`, `%2Fetc%2Fpasswd`, `%2E%2E%2Fevil` ⇒ exige **400 «Invalid filename»**. Pre-D-55, `C:foo`/`a:b` pasaban el guard y devolvían 404 (la defensa dependía de que el fichero no existiera).
   - **`d56_failed_pubkey_persist_never_leaves_an_approved_peer`** — induce el fallo de persistencia de la pubkey **por fuera del crate**: el `peer.url` apunta a un servidor de identidad falso en loopback que **elimina la fila del peer en PeerDb antes de responder** la pubkey; `update_ed25519_pubkey` no encuentra la fila → rama de error de D-56. Exige: 500 con `status ∈ {approval_reverted, approval_revert_failed}` (nunca `approved_without_identity`), **y** que `GET /api/v1/federation/peers` no muestre el peer aprobado. Más camino feliz (aprobación legítima → 200 + `approved:true`) y 404 para peer inexistente. Cada llamada tiene `timeout(15s)` que convierte un cuelgue en fallo legible con sentinel 599 (CI no puede colgarse jamás).
2. **`.github/workflows/ci.yml`** — `+1` línea: `--test regression_d55_d56_outside_crate` en `e2e-security-tests`.

`d56` va con `#[ignore = "bloqueado por deadlock RwLock en federation_approve_peer (IMPL_M-BUFFY-8)"]` para no poner CI en rojo mientras el fix de `src/` no aterrice (ver hallazgo). Repro: quitar el ignore y correr el binario.

## Verificación (salida real, 2026-10-25)

**Fase RED** — revertidos los parches aceptados de Deep (`git apply -R D-55.patch` + `D-56.patch`, ambos verificables con `--check`; después `git restore` a `main`):

```
test d55_ingest_files_rejects_unit_prefix_and_traversal_names ... FAILED
  - D-55: GET /api/v1/ingest/files/C:foo => 404 Not Found (esperado 400); body: File not found
  - D-55: GET /api/v1/ingest/files/a:b => 404 Not Found (esperado 400); body: File not found
test d56_failed_pubkey_persist_never_leaves_an_approved_peer ... FAILED
  - D-56: status="approved_without_identity" (esperado 'approval_reverted' o 'approval_revert_failed'...)
  - D-56: el peer con persistencia fallida sigue apareciendo aprobado (caché no recargada...)
  - D-56 (feliz): approve => 599 ... TIMEOUT 15s: posible deadlock de RwLock en el handler
test result: FAILED. 0 passed; 2 failed; finished in 15.85s
```

**Estado final (`main` con los fixes):**

```
test d55_ingest_files_rejects_unit_prefix_and_traversal_names ... ok
test d56_failed_pubkey_persist_never_leaves_an_approved_peer ... ignored,
    "bloqueado por deadlock RwLock en federation_approve_peer (IMPL_M-BUFFY-8)"
test result: ok. 1 passed; 0 failed; 1 ignored; finished in 1.61s
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → exit 0 (CLIPPY_OK)
```

Re-verificados también los dos binarios de M-BUFFY-7 tras el merge (7 + 1 en verde): el cambio de `src/` de push144/145 no rompió los tripwires.

## Hallazgo (propongo D-58) — `federation_approve_peer` se auto-cuelga

**Síntoma:** todo `POST /api/v1/federation/peers/{name}/approve` en el que la identidad del peer responde 200 con `public_key` **se cuelga indefinidamente** (mi test lo topa con 599 a los 15 s).

**Causa:** `api_federation.rs` línea 337:

```rust
if let Some(peer) = state.config.read().await.federation_peers.iter().find(...).cloned() {
```

El read-guard de `state.config` es un temporario del scrutinee del `if let` → **vive hasta el final del bloque**. Dentro del cuerpo se llama `reload_peers_cache(&state).await` (`config.write().await`):
- **línea 375** — camino feliz (preexistente, M12-B/D-52): cuelga también;
- **línea 355** — rama de error **añadida por D-56**: cuelga en `main`.

Mismo task con read-guard en cola y write pendiente = self-deadlock del RwLock de tokio (no reentrante). Prueba A/B limpia con el mismo test: con D-56 revertido la víctima completa su 500; en `main` cuelga; el camino feliz cuelga en ambos. Corolario (inferido de la semántica FIFO de `tokio::sync::RwLock`, no medido): tras el write encolado, los siguientes lectores de `config` esperarían también — un solo approve con identidad podría atascar el acceso a config del kernel entero.

**Consecuencia directa sobre D-56:** el objetivo «peer no queda aprobado» **no se cumple end-to-end**: al colgar el reload, la caché queda obsoleta y `GET /api/v1/federation/peers` sigue devolviendo `approved: true` para el peer cuya pubkey falló (capturado en la salida RED/estado-final). El fix unitario de Deep es correcto en su función, pero el handler no la ejecuta hasta el final. Los tests de Deep no lo ven porque llaman a `persist_peer_pubkey_or_revert` directamente, sin pasar por el handler HTTP.

**Fix propuesto (src, no mío — para Deep/TL):** sacar el guard del scrutinee:

```rust
let peer = {
    let cfg = state.config.read().await;
    cfg.federation_peers.iter().find(|p| p.name == name).cloned()
};
if let Some(peer) = peer { ... }   // con los reload dentro, ya seguros
```

Con ese cambio, `d56` pasa: quitar el `#[ignore]` y correrlo.

## Impact

- `d55` gatea en CI el contrato 400 de los prefijos de unidad (regresión del hallazgo T-DSH-69 fila 8); `d56` queda visible en CI como `1 ignored` con el motivo escrito, hasta que cierre el deadlock.
- El timeout de 15 s por llamada garantiza que ni CI ni `verify.sh` pueden colgarse con este binario; sin red externa (todo loopback), coste ~2 s.
- Cero cambios en `src/` (`git status` de `src/` = vacío tras la fase de mutación). Hallazgo reportado antes de que nadie lo sufra en producción: la aprobación de peers con identidad viva parece nunca haber funcionado end-to-end.
- ci.yml acumula 3 líneas nuevas en este trabajo (2 de M-BUFFY-7 + 1 de M-BUFFY-8), todas sujetas a la revisión que el TL dijo que haría.

## No verificado / deuda

- `d56` en verde: **pendiente del fix de deadlock en `src/`** (no toco `src/`; el `#[ignore]` documenta el bloqueo). RED de d56 sí verificado con D-56 revertido y contra `main`.
- No corrí la suite `--lib` completa ni `verify.sh` (cero cambios en `src/`); sí `clippy --all-targets -D warnings` verde y los 3 binarios propios.
- La afirmación de que el write encolado atasca también a los futuros lectores de config es una inferencia de la semántica de `tokio::sync::RwLock`, no una medición.
- El fix propuesto no está probado (es `src/`); la verificación correspondiente es quitar el `#[ignore]` de `d56` tras el fix de Deep.
