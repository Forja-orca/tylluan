# T-DSH-6 — Arregla la feature opcional mainline-dht de tylluan-link + paso anti-pudrición en CI

- **Agente:** dsh
- **Fecha:** 2026-10-19
- **Base:** `d79fd16` == `origin/main` al cerrar este diff (segundo rebase del día: origin avanzó a `514c7a2` primero y a `d79fd16` después; ninguno de los commits entrantes toca los dos ficheros modificados — rebase limpio ambas veces, verificado con `git fetch` + `git diff --name-only` antes de cada `stash pop`)
- **Estado:** diff sin commitear (2 ficheros: `crates/tylluan-link/src/dht/bootstrap.rs` +92/−46 y `.github/workflows/ci.yml` +5 líneas de comentario+paso)

## Tarea (briefing del TL, turno 1335)

Arreglar la feature opcional `mainline-dht` de tylluan-link (3 errores de compilación en
`dht/bootstrap.rs`), comprobando **primero** la API real de mainline 2.0.1 en la doc o el
código fuente del registro local — sin adivinar. Dejar `cargo check --features mainline-dht`
y `clippy -D warnings` con la feature verdes en 1.99.1, y añadir **un** paso a `ci.yml`
para que no vuelva a pudrirse, manteniendo ese cambio aparte y explicado. Exclusiones:
no cambiar defaults de features, no tocar kernel, no tocar claims.

## Hallazgo previo: la feature nunca compiló

No es una regresión por bump de dependencia. En `02ed993` (M14-A, 2026-06-29) la dependencia
ya nació declarada `mainline = { version = "2", optional = true }` y `bootstrap.rs` ya nació
usando `mainline::ClientBuilder::new().on_announce(..).on_query(..)` — API del mainline 0.x
que en 2.x **no existe**. Con `--features` ausente de todos los workflows y de `verify.sh`
(auditoría T-DSH-5: 0 coincidencias), ninguna compilación del repo tocó nunca ese código.
El bloque `bootstrap_mainline` era código muerto que ni siquiera llegaba a compilador.

## API real de mainline 2.0.1 (leída en fuente local, no adivinada)

Fuentes del registro de cargo (`~/.cargo/registry/src/…/mainline-2.0.1`) contra `Cargo.lock`
(lockea 2.0.1):

- El crate exporta `Dht`, `Id`, `Testnet`, `Error`. **No existe `ClientBuilder`** (eso era
  del crate `mainline` 0.x).
- Construcción: `Dht::client() -> Result<Self>` (cliente ya auto-bootstrapped con
  `rpc::DEFAULT_BOOTSTRAP_NODES` por defecto del crate), `Dht::server()`, o
  `Dht::builder().bootstrap(&[..]).port(n).build()`.
- Operaciones síncronas: `get_peers(info_hash) -> Result<flume::IntoIter<Vec<SocketAddr>>>`
  (iterador bloqueante por lotes), `announce_peer(info_hash, Option<port>) -> Result<Id>`,
  `shutdown(&mut self)`.
- Existe `async_dht` (`as_async()`), pero detrás del feature **no default** `async`, y su
  stream exige `StreamExt` — ni `futures` ni `tokio-stream` son dependencia de link.
  Decartado: la vía síncrona + `spawn_blocking` arregla los 3 errores sin tocar ninguna
  dependencia. `Cargo.toml` y `Cargo.lock` quedan **intactos**.
- `Id::from_bytes(AsRef<[u8]>) -> Result<Id>`: se le pasa el prefijo de 20 bytes del
  sha256 del node id (misma semilla de info-hash que usaba el código viejo).

## Cambios

### 1. `crates/tylluan-link/src/dht/bootstrap.rs`

| Punto | Qué |
|-------|-----|
| línea 47 | `DiscoveredPeer` ahora deriva `Clone` (tercer error original: `clone()` sobre el `MutexGuard` del código viejo; como efecto colateral es una mejora aditiva de API pública) |
| líneas 143-234 | `bootstrap_mainline` reescrito a la API 2.x real: cliente síncrono dentro de `spawn_blocking`; announce fire-and-forget (warn si falla); `get_peers` drenado en hilo dedicado vía `std::sync::mpsc` con ventana de 5 s (equivalente al `sleep` de 5 s del código viejo); tope duro de 10 s con `tokio::time::timeout` — si se excede devuelve `Err`, el llamador lo loguea como warn y degrada a la capa de seeds manuales: **bootstrap ya no puede colgarse**; `shutdown()` fuerza la desconexión del canal de query para que el hilo de drenaje termine siempre (sin fugas de hilos ni ventana de 5 s cumplida a costa de un hilo zombi) |
| línea 268 | `Sha256_of` renombrado a `sha256_of`: con la feature activada, `clippy -D warnings` la rechaza (`non_snake_case`) — con feature off la función ni se compilaba, así que nadie la referenciaba fuera de este fichero |

`bootstrap()` (el llamador) queda intacto: firma y capas 1-3 (persisted → mainline → seeds
manuales) sin cambios de comportamiento para quien no activa la feature.

### 2. `.github/workflows/ci.yml` — el paso, aparte y explicado

En el job `Rust — build + test`, tras `cargo test -p tylluan-cli` (líneas 31-35):

```yaml
      # Optional features get no --features anywhere else in CI, so tylluan-link's
      # mainline-dht code never compiled: it was written against the mainline 0.x
      # client API while the dependency has been "version = 2" since M14-A birth
      # (found 2026-10-19). This step keeps it compiling from now on.
      - run: cargo check -p tylluan-link --features mainline-dht
```

Por qué ahí: es el job que ya porta clippy del kernel con toolchain 1.99.1 fijado; un paso
más con el mismo toolchain evita abrir un job nuevo por 10 segundos de compilación
(mainline+flume ya están en el lock, cargo-deny los ya audita). Compila, no ejecuta — no
abre flaky de red en CI. El comentario dice la causa raíz para que nadie lo borre "por
limpieza".

### 3. Por qué no añadí un test

El briefing lo permitía ("un test **o al menos** cargo check verde"). Tests reales de
`bootstrap_mainline` exigirían red contra el DHT de BitTorrent mainline (no deterministas)
o mockear UDP (proporción de esfuerzo desmedida para la feature apagada). Un test unitario
del sha256/info-hash quedaría tras `#[cfg(feature = "mainline-dht")]`: los jobs de tests de
CI compilan con features default y **nunca lo ejecutarían**, y añadir cualquier `#[test]`
a link altera la cifra que vigila `scripts/check_test_count.sh` (link = 75 → 76), cuyos
números viven en README/STATUS — flujo del TL. Decisión: no tocar contadores; si quieres el
test, son dos líneas y un `--fix` de la puerta.

## Verificación (toolchain 1.99.1, misma rama que CI: `rustc +1.99.1` = 1.99.1, Windows x64, `-j 3`, CARGO_TARGET_DIR propio de dsh)

Antes (evidencia T-DSH-5, misma base de código): `EXIT_CLIPPY_DHT=101` con exactamente los
3 errores del briefing (t-dsh-5_clippy_1991.log) y A/B con 1.88 idéntico
(t-dsh-5_dht_ab_188.log → deriva de API, independiente del toolchain).

Después (t-dsh-6_verify3.log):

| Comando | Salida |
|---------|--------|
| `cargo check -p tylluan-link --features mainline-dht` | `EXIT_CHECK_DHT=0` |
| `cargo clippy -p tylluan-link --all-targets --features mainline-dht -- -D warnings` | `EXIT_CLIPPY_DHT=0` |
| `cargo clippy -p tylluan-link --all-targets -- -D warnings` (default) | `EXIT_CLIPPY_DEFAULT=0` (sin regresión) |
| `cargo test -p tylluan-link --all-targets` (default) | `EXIT_TEST_DEFAULT=0` — 75 passed |

## ## Impact

- **Runtime por defecto: cero cambio.** `mainline-dht` sigue sin ser default (exclusión
  respetada: `mainline-dht = ["mainline"]` intacto, sin nuevos defaults), ningún build
  binario la activa, y `mesh.mainline_dht_enabled` sigue apagado. Clippy default y los 75
  tests link verdes lo respaldan.
- **Con la feature activada:** bootstrap ya no puede colgarse (tope duro 10 s y degradación
  a seeds manuales); announce+lookup reales vía API 2.x sustituyen a los callbacks 0.x que
  nunca existieron en la dependencia lockeada.
- **CI:** +1 paso solo-compila en el job principal (ubuntu, mismo toolchain fijado); coste
  ~10-20 s una vez cacheados deps; el fallo que provocaría es exactamente el bug que
  cerramos. Ningún otro job tocado.
- **API pública:** `DiscoveredPeer` gana `Clone` (aditivo, no rompe). `Sha256_of` era
  privado del fichero: renombrar no expone nada.
- **No tocado:** `Cargo.toml`, `Cargo.lock`, kernel, claims, features defaults, timeouts de
  guilds. Ningún número de tests cambia.
- **Riesgo residual bajo:** el paso nuevo es check-only y la feature sigue sin builds que la
  activen; el riesgo de regresión del reescribe queda acotado a la feature opt-in.

## No verificado

- **Comportamiento en vivo contra el DHT mainline real**: requeriría un binario con la
  feature activada y red; el kernel de producción va sin ella. Anunciar/lookup reales sin
  prueba end-to-end.
- **El paso de CI sobre ubuntu**: la comprobación local es Windows; el paso de CI corre en
  ubuntu-latest (mismo crate puro-Rust, riesgo bajo, no ejecutado aquí).
- **Cross-compile aarch64 con la feature**: el job ARM de CI no pasa `--features`, queda
  fuera de la protección de este paso.
- Que 20.1 sea la última 2.x disponible (no consulté crates.io en vivo; `version = "2"` ya
  resolvía a 2.0.1 en el lock actual).
