# IMPL D-75 — Modelo de amenazas de persistir transcritos de agentes

**Agente:** Deep
**Base:** `origin/main` = `44c68fc` (push204; `git rev-parse --short HEAD` confirmado).
**Tipo:** análisis de solo lectura. Sin cambios de código.

## Qué se persiste hoy

| Activo | Dónde | Contenido | Protección actual |
|---|---|---|---|
| Mensajes de Coloquio | `data/mailbox.db`, tabla `coloquio_messages` (`memory/coloquio.rs`, línea 322) | `author_id`, `role`, `content` completo, `metadata` JSON | `.gitignore` ignora `data/` y `*.db` (líneas 12-15); no hay scrubbing al escribir |
| Copia para memoria | `data/silva.db`, nodos `coloquio_memory` (`main.rs`, líneas 1903-1918) | Copia el contenido del mensaje + metadata de origen | Depende de `[security] encrypt_at_rest`; sin retención específica |
| Auditoría de herramientas | `data/audit.db` (`handler_do/audit.rs`) | `intent`, guild, tool, agente, estado y `result_preview` de hasta 200 caracteres (`handler_do/outcome.rs`, línea 186) | Cadena de hashes; no hay scrubbing del preview |
| Logs de producción | `logs/kernel.log` (`logging.rs`) | Por defecto `log_content=false`: canal, longitud y sha; con opt-in, texto pasado por `scrub` y recortado | Redacción `sk-`, Bearer, `token=`, JWT y base64/hex largos; rotación 50 MiB × 5 |
| Transcripciones en el repo | `benchmarks/decision_fabric_dataset.jsonl` (3,6 MB, 4.827 líneas) | Miles de `intent` reales, incluidos posts de Coloquio con contenido operativo y nombres de agentes | **Trackeado en Git**; no está cubierto por `.gitignore` |
| Estado de sesión/agente | `data/` y `.tylluan/agents.toml` | Identidades, roles, wake, historial de sesión | `data/` ignorado; `.tylluan/agents.toml` es tracked por diseño |

## Modelo de amenazas

| Amenaza | Vector | Impacto | Mitigación propuesta |
|---|---|---|---|
| **T1. Secreto en el texto** | Un agente pega un token, una clave API o una URL con credenciales en Coloquio, `tylluan_remember` o un intent | Secreto en `mailbox.db`, copia en `silva.db` y, si `log_content=true`, en logs | DLP/scrubbing en la escritura de Coloquio y en `result_preview`; no depender solo de la redacción de logs; rotación y cuarentena |
| **T2. Lectura cruzada entre agentes** | Cualquier cliente autenticado llama a `/api/v1/coloquio/channels/...`; las rutas son agent-class y no consultan `agent_permissions` | Un agente lee conversaciones de otro | Autorización por agente/canal (rol y `agent_permissions`), no solo bearer global; decidir si el histórico es compartido por diseño |
| **T3. Suplantación de autor** | `author_id` viene del payload; solo `jose`, `admin` y `system` están protegidos (`memory/coloquio.rs`, línea 510) | Atribuir un mensaje a otro agente no protegido | Binding token→agent_id también en Coloquio (mismo patrón que D-69), o identificar el autor desde el contexto de auth |
| **T4. Borrado incompleto** | `delete_channel` borra `coloquio_messages` (línea 492), pero la copia `coloquio_memory` en Silva y la auditoría no se borran | El usuario cree que borró el historial y sigue en otra base | Borrado en cascada o tombstone con IDs de origen; documentar que el borrado es multi-almacén |
| **T5. Retención inexistente** | `mailbox.db` no tiene retención por turnos; H-51 propone `silva.coloquio_retention_turns`, pero no existe en este árbol | Crecimiento indefinido y exposición prolongada | Retención por base y por agente; exportación/archivo antes de borrar; política explícita de José |
| **T6. Robo del fichero en reposo** | Copia de `data/` o de un backup sin cifrar; `encrypt_at_rest` solo actúa si la build tiene la feature `encryption` | Lectura de todos los transcritos | Compilar releases con `--features encryption`, activar `encrypt_at_rest`, usar `TYLLUAN_DB_KEY` o keychain; permisos de directorio 0700/0600 |
| **T7. Fuga por el repo público** | `benchmarks/decision_fabric_dataset.jsonl` está trackeado y contiene intents reales, no solo etiquetas sintéticas | Exposición de conversaciones, nombres de agentes y detalles operativos en GitHub | Sacar el dataset del repo público, sustituirlo por uno sintético/anonimizado o publicar solo hashes/etiquetas; decisión de José |
| **T8. Fuga por backup/CI** | Copias de `data/` en artefactos, logs de CI o backups fuera del equipo | El cifrado del original no protege la copia | Cifrar backups, excluir `data/` de artefactos, revisar `.gitignore` y permisos de los directorios de exportación |
| **T9. Log de auditoría con contenido** | `result_preview` de 200 caracteres se escribe sin `scrub`; no es la capa de producción redactada | Secreto o dato personal en `audit.db` | Aplicar `logging::scrub` al preview o guardar solo longitud/hash; definir acceso operador y retención |
| **T10. Rotación con históricos** | `max_files=5` conserva hasta cinco ficheros; `log_content=true` amplía la ventana de exposición | Secreto en logs rotados antiguos | `log_content=false` por defecto, retención corta, borrado seguro y permisos restringidos |

## Corrección de premisa: `encrypt_at_rest`

La tarea decía que `encrypt_at_rest` está declarado pero “no se lee en ningún sitio”. **No es exacto en `44c68fc`**: `config::open_db` sí lo lee (`config.rs`, línea 1856) y aplica `PRAGMA hexkey` cuando la build tiene la feature `encryption` (líneas 1860-1869). El problema real es otro:

- `default_encrypt_at_rest()` es `cfg!(feature = "encryption")` (`config.rs`, línea 1806); la feature **no está en el default build** y en Windows nativo no se soporta.
- Si un operador pone `encrypt_at_rest = true` en una build sin la feature, el código registra un `error!` y **continúa abriendo la base en claro** (líneas 1871-1877).
- Si no hay keychain, la clave cae a `.tylluan-db-key` junto a la base; eso no protege frente a acceso al sistema de archivos (comentario en `config.rs`, líneas 1967-1975).

Por tanto, “cifrado en reposo” no es un invariante de la instalación: es una propiedad condicionada a la feature, al flag, a la clave y al perfil. Debe declararse así en la política de transcritos.

## Relación con “memoria fuera del GitHub público”

`data/`, `*.db` y `.tylluan-db-key` están ignorados (`.gitignore`, líneas 12-19), pero eso **no cubre**:

- `benchmarks/decision_fabric_dataset.jsonl`, que sí está trackeado y contiene intents reales (por ejemplo, la línea 83 incluye un post de Coloquio completo y nombres de agentes);
- logs o exports copiados manualmente fuera de `data/`;
- informes internos con fragmentos de conversaciones;
- backups de `data/` que se suban como artefacto.

La regla correcta no es “todo lo que está en `data/` es privado”, sino “todo transcrito es privado salvo que haya sido anonimizado y revisado antes de entrar en el repo”.

## Recomendación priorizada

1. **P0 — repo público:** sacar `benchmarks/decision_fabric_dataset.jsonl` del árbol público o anonimizarlo; el contenido actual no es solo etiquetas. Decisión de José/TL, no automática.
2. **P1 — escritura:** scrubbing/DLP de secretos al guardar Coloquio y al construir `result_preview`; cuarentena si el detector marca.
3. **P1 — acceso:** autorización por agente/canal en las rutas de Coloquio; binding de `author_id` al bearer, no solo a los tres nombres protegidos.
4. **P1 — borrado/retención:** cascada `mailbox.db` → `silva.db` → auditoría, con retención por base y exportación explícita.
5. **P1 — reposo:** release con `--features encryption` o declarar claramente que el cifrado no está activo; fallar cerrado si `encrypt_at_rest=true` sin soporte.
6. **P2 — permisos:** directorios de datos 0700 y ficheros 0600; backups cifrados y fuera del repo.
7. **P2 — logs:** mantener `log_content=false`; si se activa, retención más corta y revisión del `audit.db`.

## Impact

- **Privacidad:** identifica que el mayor riesgo actual no es un log, sino el dataset de benchmark trackeado y la copia de Coloquio en dos bases.
- **Seguridad:** corrige la premisa sobre `encrypt_at_rest`; el cifrado es condicional y puede fallar abierto.
- **Operación:** da criterios para retención, borrado y acceso antes de persistir transcripciones.
- **No depredación:** solo lectura; no se tocó código, configuración ni datos.

## No verificado

- No abrí las bases de producción ni medí tamaños reales de `mailbox.db`/`silva.db`; el análisis es de esquema y código.
- No verifiqué si el dataset de benchmark contiene secretos reales; un grep de patrones habituales no encontró tokens, pero sí contiene conversaciones y nombres, y su origen/anonimización no está demostrado.
- No probé SQLCipher en esta máquina Windows; la feature se documenta como no soportada en Windows nativo.
- No revisé H-54: no existe aún en `44c68fc`; por eso D-74 sigue bloqueada y no se evalúa la capa de auditoría nueva.
