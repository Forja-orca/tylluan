# T-DSH-32 — F0 del ADR-022: qué se rompe si se enciende `capabilities_enforce`

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`d121c8a`); limpio antes de empezar.
- **Encargo (turno 1547):** sin tocar `src/`, crear `tests/capabilities_enforce_matrix.rs` que (1) obtenga las guilds con `CAPABILITIES` del catálogo, (2) construya llamadas representativas por herramienta con casos límite, (3) llame a `enforce_capabilities` y tabule herramienta × llamada × BLOQUEADA/PERMITIDA × motivo, (4) marque falsos bloqueos (legítimas bloqueadas) y maliciosas que pasan. Test debe pasar HOY; el informe concluye si encender el flag por defecto rompería algún uso legítimo. Sin `cfg(unix)/cfg(windows)`, clippy 0, fichero nuevo, `## Impact` y no verificado, diff sin commitear.

## Matriz medida (21 llamadas × 3 guilds, CAPABILITIES reales del `.py`)

```text
guild      herramienta            clase      resultado  falso?         nota
bash       state_checkpoint       legitima   PERMITIDA                 checkpoint de estado
bash       state_restore          legitima   PERMITIDA                 restaurar estado
bash       bash_execute           legitima   PERMITIDA                 comando simple
bash       bash_execute           legitima   PERMITIDA                 cargo test en el workspace
bash       bash_execute           maliciosa  PERMITIDA                 comillas + destructivo (bool true no filtra)
bash       bash_execute           maliciosa  PERMITIDA                 ruta absoluta + cwd externo
vision     vision_device_status   legitima   PERMITIDA                 estado del device
vision     vision_analyze         legitima   PERMITIDA                 ruta absoluta POSIX
vision     vision_analyze         legitima   PERMITIDA                 ruta absoluta Windows
vision     vision_ocr             legitima   PERMITIDA                 OCR legítimo
vision     vision_extract         maliciosa  PERMITIDA                 lee fichero de host (scope "/" es comodín)
vision     vision_analyze         maliciosa  PERMITIDA                 traversal relativo (".." no se canonicaliza)
vision     execute_python         maliciosa  BLOQUEADA                 nombre exec-like
vision     vision_analyze         maliciosa  BLOQUEADA                 argumento "command" inyectado
websearch  web_search             legitima   PERMITIDA                 búsqueda normal
websearch  web_search             legitima   PERMITIDA                 búsqueda por intent
websearch  web_search             legitima   PERMITIDA                 query vacía
websearch  web_search             legitima   BLOQUEADA  FALSO BLOQUEO  "command" es fallback de query en el .py
websearch  run_shell              maliciosa  BLOQUEADA                 nombre exec-like
websearch  web_search             maliciosa  PERMITIDA                 dominio ajeno (network_hosts no se aplica)
websearch  web_search             maliciosa  PERMITIDA                 ruta FS (filesystem_scope vacío no aplica)

resumen: 21 llamadas · 1 legítima bloqueada (falso bloqueo) · 6 maliciosas que pasan
```

## Hallazgo F0 (el que decide): hoy el flag NO se aplica a casi nada

`extract_capabilities` (`catalog.rs`, línea 374-420) solo cierra correctamente los dicts **de una línea**. En una declaración multilínea, la línea final `}` decrementa la profundidad a 0 y el parser **hace `break` antes de añadirla** al buffer → JSON truncado (`{ ..., `) → `serde_json` falla → **`None`**. Evidencia:

- El catálogo real solo expone capabilities de **`bash`** (dict de una línea: `{"process_execution": True}`). `vision` y `websearch` las tienen declaradas en su `.py` pero el catálogo las pierde (`null`). Verificado con `builtin_catalog()` en el test (`catalog: guilds ... PARSEADAS = ["bash"]`) y replicando el parser fuera del repo.
- `check_capabilities` trata "sin CAPABILITIES" como **nunca enforzar** (`guild_process.rs`, línea 555-560). Consecuencia: aunque se encienda `capabilities_enforce=true`, **`vision` y `websearch` no se ven afectadas en absoluto**; y `bash` sí llega pero declara `process_execution: true` (bool true = sin allowlist) → deja pasar todo.

**Respuesta a la pregunta de F0: encender `capabilities_enforce` por defecto HOY no rompe ningún uso legítimo, pero tampoco aporta seguridad real** — el único efecto es sobre `bash`, y su declaración es un "sí a todo". El techo de seguridad no es el flag, es (a) el parser y (b) las declaraciones.

## ¿Y si se arregla el parser? (lo que hay que saber antes de encenderlo de verdad)

Con las declaraciones reales en la entrada de `enforce_capabilities` (lo que hace la matriz), el resultado es:

- **1 falso bloqueo**: `websearch.web_search(command="rust async")`. En `websearch.py` (línea 44), `command` es el **tercer fallback legítimo de la query** (`q = query or intent or command`), no un comando de shell. Al encender el flag, ese uso legítimo se bloquea con `CAPABILITY_BLOCKED ... process_execution=false`.
- **Ningún falso bloqueo en `vision`**: sus herramientas legítimas (`vision_analyze`, `vision_ocr`, `vision_extract`, `vision_device_status`) no llevan argumento `command` ni nombre exec-like; las rutas absolutas legítimas pasan por el comodín `"/"`.
- **6 maliciosas que pasan** (el flag no las cubre): (1-2) `bash` con `process_execution: true` permite comandos destructivos con comillas y con `cwd` externo; (3) `vision` con `filesystem_scope: ["/"]` es comodín y no canonicaliza → lee `/etc/shadow`; (4) mismo motivo → `../../../../etc/passwd`; (5) `network_hosts` **no se comprueba en ningún sitio** (solo aparece en un comentario de `catalog.rs`) → dominio ajeno permitido; (6) `filesystem_scope: []` no se aplica (el chequeo exige lista no vacía) → cualquier `path` pasa.

## Qué haría falta antes de encender el flag por defecto (para D-32/Deep)

1. **Arreglar `extract_capabilities`** para cerrar dicts multilínea (y test de regresión; hoy solo `bash` llega al catálogo). Es un bug de una línea y desbloquea todo lo demás.
2. **`websearch`**: renombrar/quitar el fallback `command` de `web_search` o excluirlo de la heurística de `command` (el argumento no ejecuta nada).
3. **`bash`**: `process_execution: true` no es una capacidad, es una rendición; declarar allowlist real si se quiere que el flag aporte algo.
4. **`vision`**: `filesystem_scope: ["/"]` = sin restricción; acotar y canonicalizar (`..`, symlinks) antes de confiar en el chequeo de prefijo.
5. **`network_hosts`**: implementar la comprobación (hoy es decorativo) o retirarlo del manifiesto para no vender una defensa que no existe.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test capabilities_enforce_matrix -- --nocapture` | **2 passed / 0 failed** (0,06 s); matriz y avisos de catálogo impresos |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** `?? crates/tylluan-kernel/tests/capabilities_enforce_matrix.rs` (fichero nuevo; `src/` intacto) |
| `check_test_count.sh` | exit 0 (1264 sin cambios: test de integración en `tests/`) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

El test **fija** el comportamiento actual: si alguien arregla el parser, `catalog_capability_parser_gap` falla a propósito pidiendo actualizar la matriz; si cambia un veredicto, la matriz falla con el caso exacto. No hay `cfg(unix)/cfg(windows)`.

## Impact

- **Solo test, fichero nuevo**: cero cambios en `src/`, guilds, configuración o corpus; no se arrancó kernel ni guilds (la función medida es pura). No se tocó `E:\tylluan`.
- **Para F0/ADR-022**: la conclusión es que **encender el flag hoy no rompe usos legítimos pero es casi inerte** (parser solo expone `bash`, que declara `true`), y que **arreglar el parser sin tocar `websearch` bloquearía exactamente un uso legítimo** (fallback `command`). Da la lista concreta de 6 huecos que el flag no cubre, para no vender el flag como frontera.
- **Coste**: 0,06 s, sin red/BD/embeddings; test de integración normal de CI.

## No verificado

- **Runtime con `check_capabilities`**: no ejecuté el kernel ni el despacho real; la matriz llama directamente a `enforce_capabilities` (pura), y el efecto de `check_capabilities`/perfil (`Balanced`/`Strict`/`Permissive`) se deduce del código, no se ejecutó.
- **`extract_capabilities` arreglado**: no implementé el fix (fuera de alcance: sin tocar `src`); la matriz con parser correcto es una proyección de lo que pasaría, no una ejecución del catálogo arreglado.
- **Otras guilds**: solo las 3 con `CAPABILITIES`; no se auditaron las demás (sin declaración → nunca se enforzan, por diseño actual).
- **Declaraciones reales de `.py`**: el test las parsea con un parser propio (con coma final tolerada, cosa que Python permite y JSON no); si el formato de declaración cambia, el test fallará al parsear.
- **Casos límite no cubiertos**: no probé symlinks en disco (el chequeo es de strings, así que sería el mismo resultado que `..`, pero no se ejecutó), ni argumentos anidados en JSON, ni listas de comandos con `sh -c`.
