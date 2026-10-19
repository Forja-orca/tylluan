# T-DSH-5 — clippy 1.99.1 sobre los crates restantes + catálogo de referencias de toolchain obsoletas (2026-10-19, dsh)

Asignación: TL T1331 (tras aceptar T-DSH-4 y ejecutar el pin a 1.99.1 en `0591570`). Dos partes: (1) medir clippy `-D warnings` con 1.99.1 sobre los crates que quedaron fuera de la medición anterior; (2) listar sin editar las referencias al toolchain ahora desfasadas. Base: `0591570` == origin/main (fetch + ff-only hecho; el diff de T-DSH-4 quedó integrado y descartado según precedente). Herramienta: `RUSTUP_TOOLCHAIN=1.99.1` (instalada en esta máquina), `CARGO_TARGET_DIR` externo `E:\tylluan-targets\dsh-t5-1991` (y `dsh-t5-188` para el A/B), `-j 3`.

## Parte 1 — clippy 1.99.1 en los crates restantes

```
$env:RUSTUP_TOOLCHAIN='1.99.1'
cargo clippy -p tylluan-link -p tylluan-cli -p tylluan-common -p tylluan-fsrs -p tylluan-evals -p tylluan-gui --all-targets -j 3 -- -D warnings
EXIT_CLIPPY_RESTO=0
```

**Resultado: 0 avisos** — tylluan-link, tylluan-cli, tylluan-common, tylluan-fsrs, tylluan-evals y tylluan-gui pasan limpios con 1.99.1 y `-D warnings` (log `t-dsh-5_clippy_1991.log`). Con kernel (medido en T-DSH-4 y por el TL tras el pin), los 7 crates del workspace quedan verdes en la toolchain fijada.

### Hallazgo nuevo: la feature opcional `mainline-dht` no compila (podredumbre preexistente, no regresión del pin)

```
cargo clippy -p tylluan-link --all-targets --features mainline-dht -j 3 -- -D warnings
EXIT_CLIPPY_DHT=101   # 3 errores de COMPILACION (no lints)
```

Los 3, en `crates/tylluan-link/src/dht/bootstrap.rs` (declaración `mainline = { version = "2", optional = true }` en `crates/tylluan-link/Cargo.toml` línea 29; bloqueado a 2.0.1 en Cargo.lock):

| Linea | Error | Causa |
|-------|-------|-------|
| 170 | E0433 `cannot find ClientBuilder in mainline` | La API de mainline 2.x ya no expone `ClientBuilder` en la raíz del crate — el codigo fue escrito contra otra API y nunca se recompilo |
| 178 | E0433 `use of undeclared type Duration` (x2: lib y lib-test) | Falta `use std::time::Duration` |
| 180 | E0599 `clone` sobre `MutexGuard<Vec<DiscoveredPeer>>` con bounds insatisfechos | A `DiscoveredPeer` (linea 48) le falta `#[derive(Clone)]` |

**A/B con la toolchain vieja**: mismo comando con 1.88.0 (`EXIT_DHT_188=101`, los mismos 3 errores, log `t-dsh-5_dht_ab_188.log`) — la rotura es **independiente de la toolchain**, llevaba podrida desde antes del pin (ultimo toque al archivo: `d92e13b`, epoca v0.15.0). Nadie la vio porque **ni ci.yml ni verify.sh pasan `--features` por ningun sitio** (verificado con grep: cero coincidencias en ambos): la feature opt-in solo se compila si alguien la pide a mano, y su runtime ademas esta apagado por defecto (`mesh.mainline_dht_enabled = false`), asi que **cero impacto en produccion** — pero los `dht::bootstrap` tests jamas se ejecutan.

Propuestas (para decision del TL, sin tocar nada por mi):
1. Arreglo pequeno: importar `Duration`, `#[derive(Clone)]` en `DiscoveredPeer`, y adaptar el arranque del cliente a la API actual de mainline 2.x (o subir la dep y escribir contra su builder real). Es un archivo.
2. Que CI al menos compile la feature (un `cargo check -p tylluan-link --features mainline-dht` en el job de Rust) para que no vuelva a pudrirse en silencio. Lo mismo aplica al patron general: cualquier feature opcional sin job que la compile es deuda invisible.

### Features no medidas y por que (sin-verificar declarado)

- `tylluan-kernel --features encryption`: el propio Cargo.toml documenta «Windows native: not supported (use Docker Desktop)» — requires OpenSSL vendorizado, no construible en esta maquina. Solo medible en runner Linux.
- `--features cuda`: requiere SDK CUDA 12.x + cuDNN 9 — no existe en esta maquina.
- `--features integration` / `guild_knowledge_test`: flags que activan tests que necesitan runtime Python/FastMCP; no medidos (compilarlos no prueba nada sin el runtime, y el TL ya midio kernel).
- `--features bundled-dashboard`: necesita `pnpm build` del dashboard previo (dist embebido por rust-embed).
- `semantic` es default (ya medida); `minimal`/`production` restan codigo; `observability` no medida (flag barata, kernel — zona ya verificada por el TL).

## Parte 2 — Referencias al toolchain ahora desfasadas (listado, sin editar)

grep sobre `0591570` de `1\.88|1\.97|rust-toolchain|rustup run stable|toolchain: stable` en docs/, AGENTS.md y companeros de raiz. **AGENTS.md: cero coincidencias** (no cita version de toolchain — no necesita tocarse). Ordenadas por impacto:

| Archivo | Linea | Texto actual | Propuesta |
|---------|-------|--------------|-----------|
| `CLAUDE.md` | 111 | «toolchain 1.88 (`rust-toolchain.toml`)» | «toolchain 1.99.1 (`rust-toolchain.toml`)» — el doc que leen todos los agentes describe el pin viejo |
| `CLAUDE.md` | 263 | «El script fija el toolchain a `stable` explicitamente (`rustup run stable`) — igual que ci.yml» | Obsoleto tras `0591570`: verify.sh y ci.yml fijan 1.99.1 (verify.sh linea 70 lo dice literal). Reescribir a «fija el toolchain a 1.99.1, igual que ci.yml y rust-toolchain.toml» — si no, un agente que trustee este parrafo usara `rustup run stable` y reimportara el mundo flotante que el pin acaba de eliminar |
| `Dockerfile` | 9 | `FROM rust:1.88-bookworm AS builder` | `rust:1.99.1-bookworm`. Ojo: ahora mismo el toml fijado manda dentro de la imagen y rustup descargara 1.99.1 sobre la base 1.88 en cada docker-build (funciona, pero doble herramienta y red); el docker-smoke del proximo disparo sera la primera prueba real — no verificado aqui |
| `docs/getting-started/QUICKSTART.md` | 3 | «Prerequisites: Rust 1.88 (pinned in `rust-toolchain.toml`)» | «Rust 1.99.1 (pinned in `rust-toolchain.toml`)» — doc de entrada de usuarios nuevos |
| `CONTRIBUTING.md` | 7 | «Rust 1.88+ (`rustup` recommended — see `rust-toolchain.toml`)» | «Rust 1.99.1 (…)» |
| `README.md` | 19 | badge `rust-1.88+-orange.svg` | `rust-1.99.1+` (o quitar la cifra del badge y dejar «Rust» a secas, para que no vuelva a vencer) |
| `STATUS.md` | 40 | «Rust — build + test … pass (Rust 1.88+)» | «(Rust 1.99.1)» |
| `docs/guides/DOCKER_INSTALL_LESSONS.md` | 141 | tabla describe el builder actual como `rust:1.88-bookworm` con consejo «usar latest o 1.88+» | Actualizar la fila al estado real (1.99.1, fijado) — la linea 35 es recordatorio historico del cambio 1.82→1.88 y se deja |
| `docs/reference/adr/ADR020_inference_gateway.md` | 38, 488, 546 | «toolchain fijada (`rust-toolchain.toml`: `channel = "1.88"`)» y «Toolchain fijada `1.88`» | ADR vigente con requisito normativo anclado a la version: reemplazar la cifra por «la fijada en `rust-toolchain.toml`» (referencia indirecta que no vence) o anotar 1.99.1 — decision del TL como autor de ADRs |
| `docs/reference/adr/ADR019_team_scoped_networks.md` | 110, 118 | «iroh 1.x exige rustc 1.91 (el repo fija 1.88)» / «subir rustc 1.88 a 1.91» | El argumento de coste que sostenia la decision ha quedado obsoleto con el pin (1.99.1 ya satisface el 1.91 de iroh). El ADR es record historico: no reescribir; como mucho nota «premisas de toolchain cambiadas en `0591570`» |

**Falsos positivos descartados del grep** (no son referencias de toolchain o son historico legitimo): `docs/roadmap/ROADMAP_O3.md` linea 103 («1.88x» es un ratio de speedup, no una version), lineas 181/295/483 (registro historico de commits/auditorias donde 1.88 es el hecho pasado), `docs/concepts/PROJECT_STRUCTURE.md` linea 13 (solo enlista el nombre del archivo toml), y mis propios informes `IMPL_t-dsh-3`/`IMPL_t-dsh-4` (record historico de la decision, con el 1.88 citado como el estado de entonces).

## ## Impact

- **Produccion: cero.** Ningun codigo, workflow, doc ni config tocado; unico artefacto nuevo este informe. Los logs de evidencia (`t-dsh-5_clippy_1991.log`, `t-dsh-5_dht_ab_188.log`) quedan sin versionar en la raiz del worktree.
- **Hallazgo con riesgo real acotado**: la feature `mainline-dht` del workspace no compila desde hace ~versiones (A/B 1.88/1.99.1 identico) — cero impacto en produccion (opt-in apagada por defecto), pero es deuda invisible que el patron «CI nunca pasa --features» garantiza que seguira pudriendose hasta que se fije.
- La tabla de refs es insumo para el TL; ninguna la he editado yo, incluida Dockerfile (fuera del barrido que el TL se atribuyo en `0591570`: toml + 10 pasos + verify.sh + check_test_count.sh, todos verificados en su push).

## Sin verificar («no verificado»)

- El efecto del `FROM rust:1.88-bookworm` + toml 1.99.1 en el proximo docker-smoke: inferencia por comportamiento conocido de rustup (el toml gana), no ejecutado aqui.
- Features de kernel (encryption/cuda/integration/bundled-dashboard) — motivos en la tabla de arriba.
- Que no existan avisos de clippy 1.99.1 bajo `--features` de kernel (no medibles en esta maquina).
- Los 3 errores de bootstrap.rs: medidos como compilacion (cargo clippy baja a check); no ejecute ningun binario ni test de la feature (imposible: no compila).
