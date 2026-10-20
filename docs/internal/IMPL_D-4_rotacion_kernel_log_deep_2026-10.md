# D-4 — Rotación de `kernel.log` por tamaño (Deep, 2026-10-19)

**Estado: no verificado** por terceros. Diff **sin commitear** en el worktree
`E:\tylluan-wt-deep`, base `17465b9`.
Parche: `E:\tylluan-patches\deep\D-4.patch` (11 archivos; para las líneas de
recuento es acumulado con D-2/D-3 — ver nota al final).

## Problema

`init_logging` abría `./logs/kernel.log` con `OpenOptions::append` y un `File`
pelado (`main.rs:41-51`). Sin límite: 1,3 GB observados. No había configuración,
ni rotación, ni forma de acotarlo salvo borrar el fichero a mano con el kernel
parado.

## Qué se entregó

### 1. `RotatingFileWriter` (`crates/tylluan-kernel/src/logging.rs`, nuevo)

`impl Write`, pensado para `tracing_subscriber::fmt::layer().with_writer(...)`.

- Rota cuando `written + buf.len() > max_bytes`, comprobando **antes** de cada
  escritura: el tamaño máximo real es `max_bytes` más, como mucho, el tamaño de
  un registro.
- **Nunca parte una línea**: un registro mayor que el límite se escribe entero y
  vive solo en su fichero.
- Nomenclatura: `kernel.log` (vivo), `.1` (anterior), `.2`, … hasta
  `.{max_files-1}`. Con los defaults (50 MiB, 5) el techo es ~250 MiB.
- **Rotación al arrancar** si el fichero vivo ya nace por encima del límite: el
  caso real de 1,3 GB se resuelve en el primer arranque, sin intervención.
- **Un fallo de rotación no pierde líneas**: se sigue escribiendo en el fichero
  actual y se cuenta el fallo (`rotation_failures()`), en vez de propagar el
  error y tirar la escritura.
- `max_file_size_mb = 0` desactiva la rotación y recupera el comportamiento
  histórico (un solo fichero sin límite).
- `max_files = 1` no conserva historial: se trunca el vivo al rotar.

Detalles de plataforma resueltos (Windows es el runtime real de este proyecto):

- `fs::rename` no sobrescribe en Windows → se borra el destino antes de mover,
  y se desplaza de mayor a menor para no pisar ficheros pendientes.
- Truncar un fichero sobre un handle abierto en modo append no es fiable →
  para `max_files = 1` se recrea el fichero (`File::create`) y se reabre en
  append, en vez de `set_len(0)`.

### 2. Configuración (`[logging]`)

En `config.rs`, con `serde(default)` para que **la ausencia de la sección ya
rote** (el fichero de 1,3 GB no puede depender de que alguien edite el TOML):

```toml
[logging]
max_file_size_mb = 50   # 0 desactiva la rotación
max_files = 5           # total, contando el vivo
```

`LoggingConfig::max_file_size_bytes()` hace la conversión en un solo sitio.

El `tylluan.toml` del worktree (no versionado) ya lleva la sección con los
valores por defecto explícitos. **No va en el parche** porque el fichero no está
trackeado; el TL puede añadirla o dejarla fuera (los defaults son los mismos).

### 3. Cableado (`main.rs`)

```rust
let logging = TylluanConfig::load().map(|c| c.logging).unwrap_or_default();
let log_writer = RotatingFileWriter::open("./logs", "kernel.log", …)
    .or_else(|e| { /* fallback ./kernel_<pid>.log, también con rotación */ })
    .unwrap_or_else(|e| { eprintln!("FATAL: …"); exit(1) });
```

- La configuración se lee best-effort en `init_logging` (corre antes de la carga
  principal, pero **después** del `--config` override), así que respeta el
  fichero que el operador haya indicado.
- Se conserva el fallback histórico (`./kernel_<pid>.log`), ahora también con
  rotación, y el `FATAL + exit(1)` solo si no se puede abrir ningún fichero.

## Verificación (ejecutada por mí, salidas reales)

```
cargo test -p tylluan-kernel --lib logging::                        → 5 passed
cargo test -p tylluan-kernel --lib config::tests::logging           → 1 passed
cargo test -p tylluan-kernel --lib                                  → 1088 passed; 0 failed; 4 ignored
cargo clippy -p tylluan-kernel --all-targets -- -D warnings         → EXIT 0
scripts/check_test_count.sh                                         → EXIT 0 (1175)
scripts/check_docs_reality.sh                                       → sólo el ❌ preexistente de ROADMAP_O3.md:42
```

Tests nuevos:

| Test | Qué fija |
|------|----------|
| `rotates_at_size_and_preserves_every_line` | rota; ningún fichero supera el límite; **las 100 líneas sobreviven intactas y en orden** |
| `keeps_at_most_max_files` | nunca más de `max_files` ficheros; la retención borra lo más viejo |
| `oversized_record_is_never_split` | un registro de 1 KB con límite 256 B sobrevive entero en un solo fichero |
| `existing_oversized_file_is_rotated_on_open` | el caso real: un `kernel.log` de 2 KB con límite 512 B se rota al abrir y el vivo arranca vacío |
| `single_file_mode_truncates_instead_of_keeping_history` | `max_files=1` trunca y no crea `.1`, sin pasarse del límite |
| `logging_defaults_are_safe_and_configurable` (config) | 50/5 por defecto; `0` desactiva; el TOML explícito manda |

Nota de honestidad sobre la suite completa: la primera ejecución de `cargo test
--lib` se lanzó **en paralelo** con `cargo clippy --all-targets` y dio
`1087 passed; 1 failed`. No capturé el nombre del test (la salida se truncó) y
no se reprodujo: las dos ejecuciones siguientes, en serie, dieron **1088 passed;
0 failed** ambas. Lo más probable es contención de CPU/recursos al compilar y
testear a la vez; lo dejo escrito para que no se lea como "todo verde a la
primera".

## Efectos que el operador debe conocer

1. **Cambio de comportamiento por defecto (declarado, no silencioso)**: a partir
   de este cambio los logs rotan a 50 MiB conservando 5 ficheros. Para volver al
   comportamiento anterior: `[logging] max_file_size_mb = 0`.
2. El `kernel.log` actual de 1,3 GB **no se borra**: en el primer arranque pasa a
   `kernel.log.1`. A partir de ahí la retención lo irá desplazando y, cuando
   toque, eliminando. Si se quiere recuperar espacio ya, borrarlo a mano con el
   kernel parado.
3. `/api/v1/admin/logs` (`api_admin.rs:754`) y el lector de `api_ops.rs:167`
   siguen leyendo **solo** `./logs/kernel.log`, es decir, las líneas recientes.
   Las antiguas están en `.1`…`.4`. No cambié su semántica: hacer que "tail"
   recorra el historial es una decisión de producto (¿cuántas líneas? ¿qué
   ficheros?) que no me corresponde tomar aquí. Queda anotado como follow-up.

## Impact

Toca `main.rs` (arranque), `config.rs` (config nueva con defaults) y añade un
módulo. Responde a ADR-013:

- **¿Depreda a otro agente?** No. Ficheros propios de mi cola; el único cruce
  son las líneas de recuento de tests, documentadas.
- **¿Depreda a un usuario?** No destructivo: no se pierde ninguna línea viva, no
  se borra el log histórico sin aviso (se renombra), y el fallback de arranque
  se conserva. El único cambio es que el log deja de crecer sin límite.
- **¿Rompe despliegues?** Un proceso externo que estuviera siguiendo
  `kernel.log` con `tail -f` dejaría de ver el fichero al rotar (pasa a `.1`).
  Es inherente a la rotación por rename y está documentado aquí.
- **¿Se puede verificar sin mí?** Sí: los cinco comandos de arriba.

## Nota sobre los parches de la cola

`D-4.patch` incluye el valor **final** de las líneas de recuento (1175), porque
cada parche se genera contra `17465b9`. `D-3.patch` trae 1169 y `D-2.patch` 1165.
Aplicar los tres en orden exige `--3way` en las líneas de recuento, o aplicar
solo el último (todos son acumulados respecto de la base). El código de cada
parche sí es exclusivo de su ítem: D-2/D-3 tocan `transport/http/*`, D-4 toca
`logging.rs`/`config.rs`/`lib.rs`/`main.rs`.
