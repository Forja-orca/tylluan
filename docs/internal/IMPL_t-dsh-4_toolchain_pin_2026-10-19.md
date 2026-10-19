# T-DSH-4 — Datos para la decisión de pin de toolchain (2026-10-19, dsh)

Asignación: TL T1328 («propón por escrito, informe, sin tocar nada»). Naturaleza: **solo informe** — ningún fichero del repo modificado; las corridas de verificación escribieron únicamente en `CARGO_TARGET_DIR` externos y logs sin versionar. Base del worktree al correr las mediciones: `80d7d65` (al cerrar, `14d82b8`; ver nota de equivalencia).

## Qué versión usa HOY el CI

Evidencia (en orden de solidez):

1. **El workflow no fija versión**: los 10 pasos `setup-rust-toolchain` piden `toolchain: stable` (8 en `ci.yml`, 1 en `portability-roundtrip.yml`, 1 en `release.yml`), y el action está fijo por SHA, pero el input `stable` se resuelve en runtime: rustup instala el stable vigente en cada corrida.
2. **Run CI de referencia**: `api.github.com/repos/Forja-orca/tylluan/actions/runs` (sin autenticación, datos de repo público) muestra el run de CI 1076 sobre `80d7d65`, completado con éxito el 2026-10-19 a las 07:39 UTC.
3. **Canal estable de Rust** (`api.github.com/repos/rust-lang/rust/releases`): 1.99.0 publicado 2026-10-01, **1.99.1 publicado 2026-10-16** (parche: incluye fix de malcompilación FFI bool en x86_64, entre otros). Ninguna versión posterior al cierre de esta hoja.
4. Por (1)+(2)+(3): el CI ejecutó **rustc/clippy 1.99.1** en la corrida verde más reciente. Limitación declarada: el endpoint de logs (`/actions/runs/1076…/logs`) responde 403 sin autenticación, así que la versión se infiere del canal, no se leyó del log. Si alguien con token abre el paso «Install Rust toolchain», la línea `rustc 1.x` lo confirmará en 1 minuto.

## Estado local (esta máquina de flota, Windows)

| Elemento | Valor |
|----------|-------|
| `rust-toolchain.toml` (mandato del repo) | channel **1.88** |
| Estable por defecto de la máquina | **1.97.1** — NI el del fichero NI el del CI: tres versiones distintas conviviendo |
| Instaladas además | 1.85, 1.88, 1.91, 1.99.1 (ésta instalada para esta tarea vía `rustup toolchain install 1.99.1 --component clippy`) |

Esto materializa exactamente el aviso de CLAUDE.md («el toolchain local por defecto puede ir por delante o detrás del que usa CI — falsos positivos reales más de una vez»): un agente local con la herramienta del fichero valida con 1.88 (11 versiones de retraso), con `rustup run stable` de `verify.sh` valida con 1.97.1 (2 detrás), y CI corrige con 1.99.1.

## Diferencia real de clippy 1.88 vs 1.99.1 sobre este código

Comando exacto (equivalente al paso de CI, sin `-D` para ver el listado completo):

```
$env:RUSTUP_TOOLCHAIN='1.88.0'   # y después '1.99.1'
$env:CARGO_TARGET_DIR='E:\tylluan-targets\dsh-t4-188'   # y dsh-t4-1991
cargo clippy -p tylluan-kernel --all-targets -j 3 --quiet
```

Resultado (`t-dsh-4_clippy_188.log`, `t-dsh-4_clippy_1991.log`; compilación real verificada: 4205 y 4210 artefactos, 455 `rmeta`):

| Toolchain | Avisos | Detalle |
|-----------|--------|---------|
| 1.88.0 | **1** | `clippy::unnecessary_cast` en `crates/tylluan-kernel/src/memory/silva/nodes.rs` línea 1365: `Ok(deleted as usize)` — cast `usize`→`usize`, con `#[warn]` por defecto |
| 1.99.1 | **0** | verde limpio |

Lectura de la evidencia:

- La divergencia entre once versiones mayores es, hoy, **un solo lint** — el código está razonablemente alineado con el clippy actual.
- El sentido del choque es el **menos esperado**: no es «el stable nuevo rompe CI» (el riesgo teórico que motiva el pin) sino «el toolchain viejo del repo rompe con el código actual». Con `-D warnings`, hoy: CI (1.99.1) verde; local con el mandato de `rust-toolchain.toml` (1.88) **rojo por ese cast**.
- El lint es un caso conocido de endurecimiento entre clippy de 1.88 y 1.99; la línea es trivialmente arreglable (`deleted` a secas) — **no la toqué** porque la orden era no tocar nada; con ese arreglo de una línea, el código quedaría limpio en ambas versiones y la diferencia medida sería cero.
- Cobertura y límites de esta medición (sin-verificar): `tylluan-kernel` con `--all-targets` (lib + 51 binarios de test); **no** medí `tylluan-link`, `tylluan-cli` ni `--features` opcionales — la decisión de pin debería medirlos también si se elige fijar. Lints que solo disparan con flags extra (`-W clippy::pedantic`) no entran. Y 1.99.1 es el estable de HOY: cualquier decisión «flotante» queda sujeta al próximo estable (≈cada 6 semanas; 1.100 ya en hornos).

## Las dos opciones, sin decidir (la decisión es del TL)

**(a) Fijar versión** — `rust-toolchain.toml` a la versión de referencia y los 10 `toolchain: stable` a la misma cadena. Efectos con datos: colapsa los tres mundos actuales (1.88 fichero / 1.97.1 máquina / 1.99.1 CI) a uno; el coste de mantenimiento es un bump coordinado (11 sitios + `verify.sh` dice `rustup run stable` explícito, que también habría que alinear) por cada… decisión de subir; el día del bump se necesita una corrida completa de gates (clippy doble incluido, ~15 min de compilación con -j3 en esta máquina). Si se fija, el candidato con evidencia es **1.99.1** — es la que CI ya ejecuta y está verde — con el cast de la línea 1365 arreglado antes para que 1.88-to-1.99.1 nadie pise el rojo.

**(b) Mantener flotante con aviso** — dejar `stable`, y hacer que el repo deje de mentir: el canal de `rust-toolchain.toml` debería ser `stable` (no 1.88, que nadie ejecuta ya salvo quien respete el fichero, que es justo lo que da falsos rojos como el del cast) y una nota en AGENTS.md/WORK_PROTOCOL de que el CI flota y las correcciones de lint pueden llegar con stable nuevo. Coste: cero coordinación; riesgo: la próxima ventana de 6 semanas puede traer un lint que sí afecte a código real (hoy la exposición medida es 1 línea).

## Impact

- **Producción: cero** — ningún `.rs`, workflow ni config tocado; esta hoja es el único artefacto nuevo.
- Decisión de política reservada al TL (orden textual: «con datos, sin decidir por mí»); el párrafo de recomendación se limita a dejar señalado qué opción resuelve qué evidencia.
- Sin cambio en recuento de tests ni cifras vigiladas.
